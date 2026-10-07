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
import logging
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
    CheckpointedProjection,
    DispatchContext,
    ProjectionCheckpoint,
    ProjectionCheckpointStore,
    ProjectionResult,
)
from event_sourcing.core.errors import (
    EventStoreError,
    SubscriptionHaltedError,
    UndecodableEventError,
)
from event_sourcing.core.event import DomainEvent, EventEnvelope, EventMetadata
from event_sourcing.core.process_manager import ProcessManager
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


    async def test_undecodable_is_raised_without_client_error_logs(
        self, caplog: pytest.LogCaptureFixture
    ) -> None:
        # The typed error is the signal; the coordinator logs one ERROR per
        # position. The client must not add an ERROR per attempt (#360).
        async def stream() -> AsyncIterator[object]:
            raise _data_loss(7)
            yield  # pragma: no cover

        client = GrpcEventStoreClient(address="localhost:50051")
        client._stub = MagicMock()
        client._stub.Subscribe = MagicMock(return_value=stream())
        client._stub.ReadAll = MagicMock(side_effect=_data_loss(7))
        client._stub.ReadStream = MagicMock(side_effect=_data_loss(7))
        with caplog.at_level(logging.DEBUG, logger="event_sourcing.client.grpc_client"):
            with pytest.raises(UndecodableEventError):
                async for _ in client.subscribe(from_global_nonce=0):
                    pass
            with pytest.raises(UndecodableEventError):
                await client.read_all(from_global_nonce=0)
        assert [r for r in caplog.records if r.levelno >= logging.ERROR] == []


class CorruptHeadStore:
    """History 1..CORRUPT_HEAD where the head event cannot be decoded."""

    def __init__(self) -> None:
        self.subscribed_from: list[int] = []
        self.subscribed_at: list[float] = []
        self.subscribed = asyncio.Event()
        # Operator fixed the row (or deployed a store that decodes it).
        self.repaired = False
        # Fail the next N subscribe attempts with a transient (UNAVAILABLE) error.
        self.unavailable_failures = 0
        self._live: asyncio.Queue[EventEnvelope[DomainEvent]] = asyncio.Queue()

    @property
    def blocked_attempts(self) -> list[float]:
        """When each subscription that must cross the corrupt event was opened."""
        return [
            at
            for start, at in zip(self.subscribed_from, self.subscribed_at, strict=True)
            if start <= CORRUPT_HEAD
        ]

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
        self.subscribed_at.append(asyncio.get_running_loop().time())
        self.subscribed.set()
        if self.unavailable_failures:
            self.unavailable_failures -= 1
            raise EventStoreError("Subscription failed: UNAVAILABLE")
        for nonce in range(from_global_nonce, CORRUPT_HEAD):
            yield _envelope(nonce)
        if from_global_nonce <= CORRUPT_HEAD:
            if not self.repaired:
                raise UndecodableEventError(CORRUPT_HEAD, "corrupt")
            yield _envelope(CORRUPT_HEAD)
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

    def __init__(
        self,
        positions: dict[str, int],
        recheck: float | None = None,
        extra: list[CheckpointedProjection] | None = None,
    ) -> None:
        self.store = CorruptHeadStore()
        self.checkpoints = MemoryCheckpointStore()
        self.positions = positions
        self.projections = {name: RecordingProjection(name) for name in positions}
        self.coordinator = SubscriptionCoordinator(
            event_store=self.store,  # type: ignore[arg-type]
            checkpoint_store=self.checkpoints,
            projections=[*self.projections.values(), *(extra or [])],  # type: ignore[list-item]
            undecodable_recheck_interval=recheck,
        )
        self._runner: asyncio.Task[None] | None = None

    @property
    def runner(self) -> asyncio.Task[None]:
        assert self._runner is not None
        return self._runner

    async def move_checkpoint(self, name: str, position: int) -> None:
        """The ADR-026 operator step: set a consumer's checkpoint explicitly."""
        await self.checkpoints.save_checkpoint(
            ProjectionCheckpoint(
                projection_name=name,
                global_position=position,
                updated_at=datetime.now(UTC),
                version=1,
            )
        )

    def restart(self) -> None:
        """Start the coordinator again, as an operator would after recovery."""
        self._runner = asyncio.create_task(self.coordinator.start())

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
        # No operator action: delivered up to N - 1, then halted at N (#360).
        async with _Running({"behind": CORRUPT_HEAD - 2}) as run:
            behind = run.projections["behind"]
            with pytest.raises(SubscriptionHaltedError):
                await asyncio.wait_for(run.runner, timeout=TIMEOUT_S)
            assert behind.handled == [CORRUPT_HEAD - 1]
            assert await run.position("behind") == CORRUPT_HEAD - 1

    async def test_skipping_only_some_projections_keeps_the_rest_stopped(self) -> None:
        # ADR-026: every projection that has not passed N must be moved; one
        # failing track restarts the whole plan, so nobody passes N on their own.
        async with _Running({"skipped": CORRUPT_HEAD, "behind": CORRUPT_HEAD - 2}) as run:
            with pytest.raises(SubscriptionHaltedError):
                await asyncio.wait_for(run.runner, timeout=TIMEOUT_S)
            run.restart()
            with pytest.raises(SubscriptionHaltedError):
                await asyncio.wait_for(run.runner, timeout=TIMEOUT_S)
            assert CORRUPT_HEAD not in run.projections["behind"].handled
            assert await run.position("behind") == CORRUPT_HEAD - 1
            assert await run.position("skipped") == CORRUPT_HEAD

    async def test_skipping_every_projection_resumes_all(self) -> None:
        async with _Running({"a": CORRUPT_HEAD, "b": CORRUPT_HEAD}) as run:
            run.store.publish(CORRUPT_HEAD + 1)
            await run.until(
                lambda: all(CORRUPT_HEAD + 1 in p.handled for p in run.projections.values())
            )


class TestDataLossHaltsInsteadOfRetrying:
    """#360: DATA_LOSS is not fixed by retrying, so the coordinator must not spin."""

    async def test_default_halts_with_typed_error_without_retrying(self) -> None:
        async with _Running({"behind": CORRUPT_HEAD - 2}) as run:
            with pytest.raises(SubscriptionHaltedError) as info:
                await asyncio.wait_for(run.runner, timeout=TIMEOUT_S)
            # One attempt (behind track + live track), no backoff loop.
            assert sorted(run.store.subscribed_from) == [CORRUPT_HEAD - 1, CORRUPT_HEAD + 1]
            halted = info.value
            assert halted.global_nonce == CORRUPT_HEAD
            assert isinstance(halted.__cause__, UndecodableEventError)
            assert "ADR-026" in str(halted)
            assert run.coordinator.halted is halted
            assert not run.coordinator.is_healthy
            # Checkpoint never moves past the failing position.
            assert await run.position("behind") == CORRUPT_HEAD - 1
            await asyncio.sleep(0.05)
            assert len(run.store.blocked_attempts) == 1

    async def test_restart_after_checkpoint_move_resumes_and_clears_halt(self) -> None:
        async with _Running({"behind": CORRUPT_HEAD - 2}) as run:
            with pytest.raises(SubscriptionHaltedError):
                await asyncio.wait_for(run.runner, timeout=TIMEOUT_S)
            await run.move_checkpoint("behind", CORRUPT_HEAD)
            run.restart()
            await run.until(lambda: run.store.subscribed_from[-1] == CORRUPT_HEAD + 1)
            run.store.publish(CORRUPT_HEAD + 1)
            behind = run.projections["behind"]
            await run.until(lambda: CORRUPT_HEAD + 1 in behind.handled)
            assert CORRUPT_HEAD not in behind.handled
            assert run.coordinator.halted is None
            assert run.coordinator.is_healthy

    async def test_restart_without_operator_action_halts_again(self) -> None:
        async with _Running({"behind": CORRUPT_HEAD - 2}) as run:
            with pytest.raises(SubscriptionHaltedError):
                await asyncio.wait_for(run.runner, timeout=TIMEOUT_S)
            run.restart()
            with pytest.raises(SubscriptionHaltedError):
                await asyncio.wait_for(run.runner, timeout=TIMEOUT_S)
            assert await run.position("behind") == CORRUPT_HEAD - 1
            assert run.coordinator.halted is not None

    async def test_recheck_mode_waits_halted_then_resumes_after_checkpoint_move(self) -> None:
        async with _Running({"behind": CORRUPT_HEAD - 2}, recheck=0.05) as run:
            await run.until(lambda: run.coordinator.halted is not None)
            assert not run.coordinator.is_healthy
            await asyncio.sleep(0.3)
            assert not run.runner.done()
            # Paced by the recheck interval, not spinning.
            attempts = run.store.blocked_attempts
            assert 2 <= len(attempts) <= 10
            assert min(b - a for a, b in zip(attempts, attempts[1:], strict=False)) >= 0.04
            assert await run.position("behind") == CORRUPT_HEAD - 1

            await run.move_checkpoint("behind", CORRUPT_HEAD)
            await run.until(lambda: run.coordinator.halted is None)
            run.store.publish(CORRUPT_HEAD + 1)
            await run.until(lambda: CORRUPT_HEAD + 1 in run.projections["behind"].handled)
            assert run.coordinator.is_healthy

    async def test_recheck_mode_logs_one_error_per_position(
        self, caplog: pytest.LogCaptureFixture
    ) -> None:
        with caplog.at_level(logging.DEBUG, logger="event_sourcing"):
            async with _Running({"behind": CORRUPT_HEAD - 2}, recheck=0.02) as run:
                await run.until(lambda: len(run.store.blocked_attempts) >= 5)
        loud = [r for r in caplog.records if r.levelno >= logging.WARNING]
        assert [r.levelno for r in loud] == [logging.WARNING, logging.ERROR], [
            r.getMessage() for r in loud
        ]

    async def test_recheck_mode_resumes_after_row_repair(self) -> None:
        async with _Running({"behind": CORRUPT_HEAD - 2}, recheck=0.05) as run:
            await run.until(lambda: run.coordinator.halted is not None)
            run.store.repaired = True
            behind = run.projections["behind"]
            await run.until(lambda: CORRUPT_HEAD in behind.handled)
            await run.until(lambda: run.coordinator.halted is None)
            assert await run.position("behind") == CORRUPT_HEAD

    async def test_recheck_mode_stays_halted_while_any_projection_is_before_n(self) -> None:
        async with _Running(
            {"skipped": CORRUPT_HEAD, "behind": CORRUPT_HEAD - 2}, recheck=0.05
        ) as run:
            await run.until(lambda: len(run.store.blocked_attempts) >= 3)
            assert run.coordinator.halted is not None
            assert await run.position("behind") == CORRUPT_HEAD - 1

    async def test_invalid_recheck_interval_rejected(self) -> None:
        with pytest.raises(ValueError, match="undecodable_recheck_interval"):
            SubscriptionCoordinator(
                event_store=CorruptHeadStore(),  # type: ignore[arg-type]
                checkpoint_store=MemoryCheckpointStore(),
                projections=[],
                undecodable_recheck_interval=0,
            )

    async def test_unavailable_still_retries_with_backoff(self) -> None:
        # A transient error is retried (backoff starts at 1s); it never halts.
        run = _Running({"skipped": CORRUPT_HEAD})
        run.store.unavailable_failures = 1
        async with run:
            await run.until(lambda: len(run.store.subscribed_from) >= 2, timeout=5.0)
            assert run.store.subscribed_at[1] - run.store.subscribed_at[0] >= 0.9
            assert run.coordinator.halted is None
            run.store.publish(CORRUPT_HEAD + 1)
            await run.until(lambda: CORRUPT_HEAD + 1 in run.projections["skipped"].handled)
            assert not run.runner.done()

    async def test_halt_stops_process_manager_drains(self) -> None:
        manager = CountingProcessManager()
        recheck = 0.3
        run = _Running({"behind": CORRUPT_HEAD - 2}, recheck=recheck, extra=[manager])
        # Past the corrupt head, so the manager is live and its drain is woken
        # at plan time, before the behind track hits DATA_LOSS.
        await run.move_checkpoint("manager", CORRUPT_HEAD + 10)
        loop = asyncio.get_running_loop()

        def in_recheck_sleep() -> bool:
            attempts = run.store.blocked_attempts
            if not attempts or run.coordinator.halted is None:
                return False
            # An attempt fails within milliseconds; the next starts after `recheck`.
            return 0.05 <= loop.time() - attempts[-1] <= recheck - 0.1

        async with run:
            # The drain really ran: process_pending() blocks until cancelled.
            await run.until(lambda: manager.processed >= 1, timeout=5.0)
            await run.until(in_recheck_sleep, timeout=5.0)
            drains = run.coordinator._drains  # pyright: ignore[reportPrivateUsage]
            # No side effects run while halted: the drain was closed.
            assert not drains["manager"].is_running

    async def test_no_side_effects_across_recheck_attempts_until_halt_clears(self) -> None:
        # Each re-check re-plans, and planning wakes live drains. While halted
        # that must not run process_pending() between failures.
        manager = CountingProcessManager(block=False)
        run = _Running({"behind": CORRUPT_HEAD - 2}, recheck=0.05, extra=[manager])
        await run.move_checkpoint("manager", CORRUPT_HEAD + 10)
        async with run:
            await run.until(
                lambda: run.coordinator.halted is not None and len(run.store.blocked_attempts) >= 1
            )
            baseline_calls = manager.processed
            baseline_attempts = len(run.store.blocked_attempts)
            await run.until(lambda: len(run.store.blocked_attempts) >= baseline_attempts + 4)
            assert run.coordinator.halted is not None
            assert manager.processed == baseline_calls

            # Recovery: the halt clears and the held drain is woken again.
            await run.move_checkpoint("behind", CORRUPT_HEAD)
            await run.until(lambda: run.coordinator.halted is None)
            await run.until(lambda: manager.processed > baseline_calls)


class CountingProcessManager(ProcessManager):
    """Counts process_pending() calls; holds no data."""

    def __init__(self, block: bool = True) -> None:
        self.processed = 0
        self._block = block

    def get_name(self) -> str:
        return "manager"

    def get_version(self) -> int:
        return 1

    def get_subscribed_event_types(self) -> set[str] | None:
        return None

    async def clear_all_data(self) -> None:
        return None

    async def handle_event(
        self,
        envelope: EventEnvelope[DomainEvent],
        checkpoint_store: ProjectionCheckpointStore,
        context: DispatchContext | None = None,
    ) -> ProjectionResult:
        return ProjectionResult.SUCCESS

    async def process_pending(self) -> int:
        self.processed += 1
        if self._block:
            await asyncio.Event().wait()  # a slow side effect: runs until cancelled
        return 0

    def get_idempotency_key(self, todo_item: dict[str, str | int | float | bool | None]) -> str:
        return "key"
