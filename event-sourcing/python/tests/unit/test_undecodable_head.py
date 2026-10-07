"""Operator recovery past an undecodable head event (#351, ADR-026).

The event store refuses to decode a corrupt stored event and reports gRPC
DATA_LOSS with the event's position. The documented recovery for an
unrecoverable row is to move consumer checkpoints to its position N, so they
resume at N + 1. That must work even when N is the tenant's latest event:
the coordinator's head probe reads exactly that event, and used to fail
with the same error before any checkpoint was consulted, so the coordinator
retried forever.
"""

from __future__ import annotations

import asyncio
from datetime import UTC, datetime
from typing import TYPE_CHECKING
from unittest.mock import MagicMock

import grpc
import pytest

from event_sourcing.client.grpc_client import (
    UNDECODABLE_GLOBAL_NONCE_KEY,
    GrpcEventStoreClient,
)
from event_sourcing.core.checkpoint import (
    DispatchContext,
    ProjectionCheckpoint,
    ProjectionCheckpointStore,
    ProjectionResult,
)
from event_sourcing.core.errors import EventStoreError, UndecodableEventError
from event_sourcing.core.event import DomainEvent, EventEnvelope, EventMetadata
from event_sourcing.stores.memory_checkpoint import MemoryCheckpointStore
from event_sourcing.subscriptions.coordinator import SubscriptionCoordinator

if TYPE_CHECKING:
    from collections.abc import AsyncIterator, Callable

pytestmark = pytest.mark.unit

TIMEOUT_S = 2.0
CORRUPT_HEAD = 5


class SampleEvent(DomainEvent):
    event_type = "SampleEvent"


def _envelope(global_nonce: int) -> EventEnvelope[DomainEvent]:
    return EventEnvelope(
        event=SampleEvent(),
        metadata=EventMetadata(
            aggregate_nonce=global_nonce,
            aggregate_id="agg-1",
            aggregate_type="SampleAggregate",
            event_type="SampleEvent",
            global_nonce=global_nonce,
        ),
    )


def _data_loss(nonce: int | None) -> grpc.aio.AioRpcError:
    trailing = (
        grpc.aio.Metadata((UNDECODABLE_GLOBAL_NONCE_KEY, str(nonce)))
        if nonce is not None
        else grpc.aio.Metadata()
    )
    return grpc.aio.AioRpcError(
        code=grpc.StatusCode.DATA_LOSS,
        initial_metadata=grpc.aio.Metadata(),
        trailing_metadata=trailing,
        details=f"data integrity: stored event at global_nonce {nonce} cannot be decoded",
    )


class TestGrpcClientMapsDataLoss:
    async def test_read_all_raises_typed_error_with_position(self) -> None:
        client = GrpcEventStoreClient(address="localhost:50051")
        client._stub = MagicMock()
        client._stub.ReadAll = MagicMock(side_effect=_data_loss(42))
        with pytest.raises(UndecodableEventError) as info:
            await client.read_all(from_global_nonce=0)
        assert info.value.global_nonce == 42

    async def test_data_loss_without_position_stays_generic(self) -> None:
        client = GrpcEventStoreClient(address="localhost:50051")
        client._stub = MagicMock()
        client._stub.ReadAll = MagicMock(side_effect=_data_loss(None))
        with pytest.raises(EventStoreError) as info:
            await client.read_all(from_global_nonce=0)
        assert not isinstance(info.value, UndecodableEventError)

    async def test_subscribe_raises_typed_error_with_position(self) -> None:
        async def stream() -> AsyncIterator[object]:
            raise _data_loss(7)
            yield  # pragma: no cover

        client = GrpcEventStoreClient(address="localhost:50051")
        client._stub = MagicMock()
        client._stub.Subscribe = MagicMock(return_value=stream())
        with pytest.raises(UndecodableEventError) as info:
            async for _ in client.subscribe(from_global_nonce=0):
                pass
        assert info.value.global_nonce == 7


class CorruptHeadStore:
    """History 1..CORRUPT_HEAD where the head event cannot be decoded."""

    def __init__(self) -> None:
        self.subscribed_from: list[int] = []
        self.subscribed = asyncio.Event()
        self._live: asyncio.Queue[EventEnvelope[DomainEvent]] = asyncio.Queue()

    async def read_all(
        self,
        from_global_nonce: int = 0,
        max_count: int = 100,
        forward: bool = True,
    ) -> tuple[list[EventEnvelope[DomainEvent]], bool, int]:
        if not forward and from_global_nonce >= CORRUPT_HEAD:
            raise UndecodableEventError(CORRUPT_HEAD, "head is corrupt")
        raise AssertionError("unexpected read_all")

    async def subscribe(self, from_global_nonce: int) -> AsyncIterator[EventEnvelope[DomainEvent]]:
        self.subscribed_from.append(from_global_nonce)
        self.subscribed.set()
        for nonce in range(from_global_nonce, CORRUPT_HEAD):
            yield _envelope(nonce)
        if from_global_nonce <= CORRUPT_HEAD:
            raise UndecodableEventError(CORRUPT_HEAD, "corrupt")
        while True:
            yield await self._live.get()

    def publish(self, nonce: int) -> None:
        self._live.put_nowait(_envelope(nonce))


class RecordingProjection:
    SIDE_EFFECTS_ALLOWED = False

    def __init__(self, name: str) -> None:
        self._name = name
        self.handled: list[int] = []

    def get_name(self) -> str:
        return self._name

    def get_version(self) -> int:
        return 1

    def get_subscribed_event_types(self) -> set[str] | None:
        return None

    async def clear_all_data(self) -> None:
        self.handled.clear()

    async def handle_event(
        self,
        envelope: EventEnvelope[DomainEvent],
        checkpoint_store: ProjectionCheckpointStore,
        context: DispatchContext | None = None,
    ) -> ProjectionResult:
        nonce = envelope.metadata.global_nonce or 0
        self.handled.append(nonce)
        await checkpoint_store.save_checkpoint(
            ProjectionCheckpoint(
                projection_name=self.get_name(),
                global_position=nonce,
                updated_at=datetime.now(UTC),
                version=1,
            )
        )
        return ProjectionResult.SUCCESS


class _Running:
    """A coordinator over CorruptHeadStore with projections at given checkpoints."""

    def __init__(self, positions: dict[str, int]) -> None:
        self.store = CorruptHeadStore()
        self.checkpoints = MemoryCheckpointStore()
        self.positions = positions
        self.projections = {name: RecordingProjection(name) for name in positions}
        self.coordinator = SubscriptionCoordinator(
            event_store=self.store,  # type: ignore[arg-type]
            checkpoint_store=self.checkpoints,
            projections=list(self.projections.values()),  # type: ignore[arg-type]
        )
        self._runner: asyncio.Task[None] | None = None

    async def __aenter__(self) -> _Running:
        for name, position in self.positions.items():
            await self.checkpoints.save_checkpoint(
                ProjectionCheckpoint(
                    projection_name=name,
                    global_position=position,
                    updated_at=datetime.now(UTC),
                    version=1,
                )
            )
        self._runner = asyncio.create_task(self.coordinator.start())
        await asyncio.wait_for(self.store.subscribed.wait(), timeout=TIMEOUT_S)
        return self

    async def __aexit__(self, *_: object) -> None:  # OBJRATCHET: exception triple
        await self.coordinator.stop()
        if self._runner is not None:
            self._runner.cancel()
            await asyncio.gather(self._runner, return_exceptions=True)

    async def position(self, name: str) -> int:
        checkpoint = await self.checkpoints.get_checkpoint(name)
        assert checkpoint is not None
        return checkpoint.global_position

    async def until(self, predicate: Callable[[], bool], timeout: float = TIMEOUT_S) -> None:
        async def poll() -> None:
            while not predicate():
                await asyncio.sleep(0.01)

        await asyncio.wait_for(poll(), timeout=timeout)


class TestCheckpointSkipPastCorruptHead:
    async def test_projection_moved_past_corrupt_head_resumes_live(self) -> None:
        # Operator recovery: this consumer explicitly skips the corrupt event.
        async with _Running({"skipped": CORRUPT_HEAD}) as run:
            assert run.store.subscribed_from == [CORRUPT_HEAD + 1]
            run.store.publish(CORRUPT_HEAD + 1)
            await run.until(lambda: CORRUPT_HEAD + 1 in run.projections["skipped"].handled)
            assert CORRUPT_HEAD not in run.projections["skipped"].handled

    async def test_projection_not_moved_past_stops_before_corrupt_event(self) -> None:
        # No operator action: delivered up to N - 1, then stopped at N, retrying.
        async with _Running({"behind": CORRUPT_HEAD - 2}) as run:
            behind = run.projections["behind"]
            await run.until(lambda: CORRUPT_HEAD - 1 in behind.handled)
            # The coordinator retries (backoff starts at 1s) from the checkpoint,
            # which never moves past the corrupt event.
            await run.until(lambda: len(run.store.subscribed_from) >= 2, timeout=5.0)
            assert run.store.subscribed_from[1] <= CORRUPT_HEAD
            assert CORRUPT_HEAD not in behind.handled
            assert await run.position("behind") == CORRUPT_HEAD - 1

    async def test_skipping_only_some_projections_keeps_the_rest_stopped(self) -> None:
        # ADR-026: every projection that has not passed N must be moved; one
        # failing track restarts the whole plan, so nobody passes N on their own.
        async with _Running({"skipped": CORRUPT_HEAD, "behind": CORRUPT_HEAD - 2}) as run:
            await run.until(lambda: len(run.store.subscribed_from) >= 3, timeout=5.0)
            assert CORRUPT_HEAD not in run.projections["behind"].handled
            assert await run.position("behind") == CORRUPT_HEAD - 1
            assert await run.position("skipped") == CORRUPT_HEAD

    async def test_skipping_every_projection_resumes_all(self) -> None:
        async with _Running({"a": CORRUPT_HEAD, "b": CORRUPT_HEAD}) as run:
            run.store.publish(CORRUPT_HEAD + 1)
            await run.until(
                lambda: all(CORRUPT_HEAD + 1 in p.handled for p in run.projections.values())
            )
