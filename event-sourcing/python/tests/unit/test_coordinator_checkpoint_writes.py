"""How many checkpoint reads and commits the coordinator costs per live event.

Measured on Syntropic137 production (2026-10-07, 30 projections): checkpoint
upserts and reads were the largest steady Postgres load. On a live event every
projection that skipped it - most of them, since a projection subscribes to a
handful of types - cost one read and one commit, and every projection that
handled it one more read.

These tests pin the cost: projections that skip an event share one batched
write per event, and the coordinator reads no checkpoint per event. They also
pin what a crash between the two writes of one event leaves behind.
"""

from __future__ import annotations

from contextlib import asynccontextmanager
from datetime import UTC, datetime
from typing import TYPE_CHECKING

import pytest

from event_sourcing.core.checkpoint import ProjectionCheckpoint, ProjectionResult
from event_sourcing.core.event import EventEnvelope, EventMetadata
from event_sourcing.stores.memory_checkpoint import MemoryCheckpointStore
from event_sourcing.stores.postgres_checkpoint import PostgresCheckpointStore
from event_sourcing.subscriptions.coordinator import SubscriptionCoordinator
from tests.unit.test_coordinator_rebuild_isolation import (
    BroadcastEventStore,
    RecordingProjection,
    SampleEvent,
    _checkpoint_at,
)

if TYPE_CHECKING:
    from collections.abc import AsyncIterator, Sequence

    from event_sourcing.core.checkpoint import DispatchContext, ProjectionCheckpointStore
    from event_sourcing.core.event import DomainEvent

pytestmark = pytest.mark.unit


def _event(global_nonce: int, event_type: str) -> EventEnvelope[DomainEvent]:
    return EventEnvelope(
        event=SampleEvent(),
        metadata=EventMetadata(
            aggregate_nonce=1,
            aggregate_id="agg-1",
            aggregate_type="SampleAggregate",
            event_type=event_type,
            global_nonce=global_nonce,
        ),
    )


class _Subscribes(RecordingProjection):
    def __init__(self, name: str, types: set[str]) -> None:
        super().__init__(name)
        self._types = types

    def get_subscribed_event_types(self) -> set[str] | None:
        return self._types


class _CountingStore(MemoryCheckpointStore):
    """Counts store round trips: reads, commits, and rows written."""

    def __init__(self) -> None:
        super().__init__()
        self.reads = 0
        self.commits = 0
        self.rows = 0
        self._in_batch = False

    async def get_checkpoint(self, projection_name: str) -> ProjectionCheckpoint | None:
        self.reads += 1
        return await super().get_checkpoint(projection_name)

    async def save_checkpoint(self, checkpoint: ProjectionCheckpoint) -> None:
        if not self._in_batch:
            self.commits += 1
        self.rows += 1
        await super().save_checkpoint(checkpoint)

    async def advance_checkpoints(self, checkpoints: Sequence[ProjectionCheckpoint]) -> None:
        self.commits += 1
        self._in_batch = True
        try:
            await super().advance_checkpoints(checkpoints)
        finally:
            self._in_batch = False

    def reset_counts(self) -> None:
        self.reads = self.commits = self.rows = 0


class _UnbatchedStore:
    """A third-party store that predates BatchCheckpointStore."""

    def __init__(self) -> None:
        self.inner = MemoryCheckpointStore()
        self.saves = 0

    async def get_checkpoint(self, projection_name: str) -> ProjectionCheckpoint | None:
        return await self.inner.get_checkpoint(projection_name)

    async def save_checkpoint(self, checkpoint: ProjectionCheckpoint) -> None:
        self.saves += 1
        await self.inner.save_checkpoint(checkpoint)

    async def delete_checkpoint(self, projection_name: str) -> None:
        await self.inner.delete_checkpoint(projection_name)

    async def get_all_checkpoints(self) -> list[ProjectionCheckpoint]:
        return await self.inner.get_all_checkpoints()


class _CrashesOnBatch(MemoryCheckpointStore):
    """The process dies while saving the skips of one event."""

    def __init__(self, crash_at: int) -> None:
        super().__init__()
        self.crash_at = crash_at

    async def advance_checkpoints(self, checkpoints: Sequence[ProjectionCheckpoint]) -> None:
        if any(checkpoint.global_position == self.crash_at for checkpoint in checkpoints):
            raise ConnectionError("killed mid-batch")
        await super().advance_checkpoints(checkpoints)


async def _live_coordinator(
    store: object, projections: list[RecordingProjection], at: int = 0
) -> SubscriptionCoordinator:
    """A coordinator with every projection checkpointed at ``at`` and on the live track."""
    for projection in projections:
        if isinstance(store, MemoryCheckpointStore):
            await _checkpoint_at(store, projection.get_name(), position=at, version=1)
        else:
            assert isinstance(store, _UnbatchedStore)
            await _checkpoint_at(store.inner, projection.get_name(), position=at, version=1)
    coordinator = SubscriptionCoordinator(
        event_store=BroadcastEventStore(1),
        checkpoint_store=store,  # pyright: ignore[reportArgumentType]
        projections=list(projections),
    )
    coordinator.live_boundary_nonce = at
    coordinator._tracks = await coordinator._plan_tracks(at)
    return coordinator


async def _position(store: MemoryCheckpointStore, name: str) -> int | None:
    checkpoint = await store.get_checkpoint(name)
    return checkpoint.global_position if checkpoint else None


class TestLiveWriteCount:
    async def test_n_events_by_m_projections(self) -> None:
        """1 projection handles each event, 4 skip it: 2 commits and 0 reads per event.

        Before: 1 + 4 commits and 1 + 4 reads per event.
        """
        events, skippers = 10, 4
        store = _CountingStore()
        handler = _Subscribes("handler", {"Handled"})
        skipping = [_Subscribes(f"skipper-{index}", {"Never"}) for index in range(skippers)]
        coordinator = await _live_coordinator(store, [handler, *skipping])
        store.reset_counts()

        for nonce in range(1, events + 1):
            await coordinator.dispatch_event(_event(nonce, "Handled"))
            # Saved with the event, not held back: a live position is fresh.
            for projection in skipping:
                assert await _position(store, projection.get_name()) == nonce
            store.reads -= skippers  # the assertions' own reads

        assert handler.handled_nonces == list(range(1, events + 1))
        assert store.reads == 0
        # The handler's own save, plus one batch for every skipper.
        assert store.commits == events * 2
        assert store.rows == events * (1 + skippers)

    async def test_an_event_nobody_handles_is_one_commit(self) -> None:
        store = _CountingStore()
        projections = [_Subscribes(f"p-{index}", {"Never"}) for index in range(30)]
        coordinator = await _live_coordinator(store, list(projections))
        store.reset_counts()

        await coordinator.dispatch_event(_event(1, "Unrelated"))

        assert (store.reads, store.commits, store.rows) == (0, 1, 30)

    async def test_a_store_without_batches_still_gets_every_skip(self) -> None:
        store = _UnbatchedStore()
        projections = [_Subscribes(f"p-{index}", {"Never"}) for index in range(3)]
        coordinator = await _live_coordinator(store, list(projections))
        store.saves = 0

        await coordinator.dispatch_event(_event(1, "Unrelated"))

        assert store.saves == 3
        for projection in projections:
            assert await _position(store.inner, projection.get_name()) == 1


class TestCrashMidBatch:
    async def test_positions_resume_where_each_projection_was_saved(self) -> None:
        """Killed after the handler's own save of event 3, before the skips' batch.

        The handler is at 3 and must not handle 3 again; the skippers are at
        2 and re-skip 3. Nothing runs twice that wrote anything.
        """
        store = _CrashesOnBatch(crash_at=3)
        handler = _Subscribes("handler", {"Handled"})
        skipper = _Subscribes("skipper", {"Never"})
        coordinator = await _live_coordinator(store, [handler, skipper])

        for nonce in (1, 2):
            await coordinator.dispatch_event(_event(nonce, "Handled"))
        with pytest.raises(ConnectionError):
            await coordinator.dispatch_event(_event(3, "Handled"))

        assert await _position(store, "handler") == 3
        assert await _position(store, "skipper") == 2

        # Restart: a fresh process plans from what reached the store.
        store.crash_at = -1
        handler_again = _Subscribes("handler", {"Handled"})
        skipper_again = _Subscribes("skipper", {"Never"})
        restarted = SubscriptionCoordinator(
            event_store=BroadcastEventStore(1),
            checkpoint_store=store,
            projections=[handler_again, skipper_again],
        )
        restarted.live_boundary_nonce = 3
        restarted._tracks = await restarted._plan_tracks(3)
        resume = {
            name: track.from_position
            for track in restarted._tracks
            for name in track.projections
        }
        assert resume == {"handler": 4, "skipper": 3}

        # The subscription redelivers from the lowest track position.
        for nonce in (3, 4, 5):
            await restarted.dispatch_event(_event(nonce, "Handled"))

        assert handler_again.handled_nonces == [4, 5], "event 3 handled twice"
        assert await _position(store, "handler") == 5
        assert await _position(store, "skipper") == 5


class _ReturnsSkip(RecordingProjection):
    """Subscribes to everything, then declines every event."""

    async def handle_event(
        self,
        envelope: EventEnvelope[DomainEvent],
        checkpoint_store: ProjectionCheckpointStore,
        context: DispatchContext | None = None,
    ) -> ProjectionResult:
        return ProjectionResult.SKIP


class TestSkipResult:
    async def test_a_skip_result_joins_the_events_batch(self) -> None:
        store = _CountingStore()
        declines = _ReturnsSkip("declines")
        unsubscribed = _Subscribes("unsubscribed", {"Never"})
        coordinator = await _live_coordinator(store, [declines, unsubscribed])
        store.reset_counts()

        await coordinator.dispatch_event(_event(1, "Anything"))

        assert (store.reads, store.commits, store.rows) == (0, 1, 2)
        assert await _position(store, "declines") == 1


class TestAdvanceNeverMovesBackwards:
    async def test_memory_store(self) -> None:
        store = MemoryCheckpointStore()
        await _checkpoint_at(store, "ahead", position=9, version=1)
        await _checkpoint_at(store, "behind", position=2, version=1)

        await store.advance_checkpoints(
            [
                ProjectionCheckpoint(
                    projection_name=name,
                    global_position=5,
                    updated_at=datetime.now(UTC),
                    version=1,
                )
                for name in ("ahead", "behind", "new")
            ]
        )

        assert await _position(store, "ahead") == 9
        assert await _position(store, "behind") == 5
        assert await _position(store, "new") == 5


class _RecordingConnection:
    def __init__(self) -> None:
        self.executed: list[tuple[str, tuple[object, ...]]] = []

    async def execute(self, query: str, *args: object) -> str:
        self.executed.append((query, args))
        return "INSERT 0 2"

    async def fetchrow(self, query: str, *args: object) -> None:
        return None

    async def fetch(self, query: str, *args: object) -> list[object]:
        return []


class _RecordingPool:
    def __init__(self) -> None:
        self.connection = _RecordingConnection()
        self.acquired = 0

    @asynccontextmanager
    async def acquire(self) -> AsyncIterator[_RecordingConnection]:
        self.acquired += 1
        yield self.connection


class TestPostgresAdvanceCheckpoints:
    async def test_one_statement_for_the_whole_batch(self) -> None:
        pool = _RecordingPool()
        store = PostgresCheckpointStore(pool)  # pyright: ignore[reportArgumentType]
        when = datetime(2026, 10, 7, tzinfo=UTC)

        await store.advance_checkpoints(
            [
                ProjectionCheckpoint(projection_name="a", global_position=7, updated_at=when, version=1),
                ProjectionCheckpoint(projection_name="b", global_position=7, updated_at=when, version=3),
            ]
        )

        upserts = [
            (query, args)
            for query, args in pool.connection.executed
            if query is PostgresCheckpointStore.ADVANCE_CHECKPOINTS_SQL
        ]
        assert upserts == [
            (
                PostgresCheckpointStore.ADVANCE_CHECKPOINTS_SQL,
                (["a", "b"], [7, 7], [when, when], [1, 3]),
            )
        ]

    async def test_an_empty_batch_touches_nothing(self) -> None:
        pool = _RecordingPool()
        store = PostgresCheckpointStore(pool)  # pyright: ignore[reportArgumentType]

        await store.advance_checkpoints([])

        assert pool.acquired == 0
