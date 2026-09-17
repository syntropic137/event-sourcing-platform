"""A projection rebuild must not starve the projections already at head (#1318).

Measured in production: two projections bumped their schema version, so the
coordinator deleted their checkpoints and replayed them from 0. The other 23
then stopped processing new events for the whole ~40 minute replay - all of
them frozen at one identical position while the head of the stream kept
moving. The read model went blind: ``GET /api/v1/executions`` reported no
running executions while four workspace containers were running.

The cause is a single shared subscription. One ``subscribe()`` generator feeds
every projection, so it cannot fetch event N+1 - live or historical - until
event N has been handed to every projection, the slowly rebuilding ones
included.

These tests pin the fix: the rebuild replays on its own track, and the
projections at head keep consuming live events while it runs.
"""

from __future__ import annotations

import asyncio
from datetime import UTC, datetime
from typing import TYPE_CHECKING

import pytest

from event_sourcing.core.checkpoint import (
    DispatchContext,
    ProjectionCheckpoint,
    ProjectionCheckpointStore,
    ProjectionResult,
)
from event_sourcing.core.event import DomainEvent, EventEnvelope, EventMetadata
from event_sourcing.stores.memory_checkpoint import MemoryCheckpointStore
from event_sourcing.subscriptions.coordinator import SubscriptionCoordinator

if TYPE_CHECKING:
    from collections.abc import AsyncIterator

pytestmark = pytest.mark.unit

# A live event has to reach the at-head projection inside this budget for it to
# count as "still consuming". Generous: the fixed path needs one event loop
# turn, the starved path never gets there at all.
LIVE_DELIVERY_TIMEOUT_S = 2.0

HISTORY_SIZE = 100


class SampleEvent(DomainEvent):
    """Event carried by the fake store."""

    event_type = "SampleEvent"


def _envelope(global_nonce: int) -> EventEnvelope[DomainEvent]:
    return EventEnvelope(
        event=SampleEvent(),
        metadata=EventMetadata(
            aggregate_nonce=1,
            aggregate_id="agg-1",
            aggregate_type="SampleAggregate",
            event_type="SampleEvent",
            global_nonce=global_nonce,
        ),
    )


class BroadcastEventStore:
    """Fake event store whose ``subscribe()`` calls are independent.

    Each call gets its own cursor over history and its own live queue. That is
    the whole point: it lets a test tell apart "the coordinator opened one
    stream for everybody" from "the coordinator opened a stream per track".
    """

    def __init__(self, history_size: int) -> None:
        self._events: list[EventEnvelope[DomainEvent]] = [
            _envelope(nonce) for nonce in range(1, history_size + 1)
        ]
        self._listeners: list[asyncio.Queue[EventEnvelope[DomainEvent]]] = []
        self._subscription_opened = asyncio.Event()
        self.subscribed_from: list[int] = []

    @property
    def head_nonce(self) -> int:
        return self._events[-1].metadata.global_nonce or 0

    async def wait_until_subscribed(self) -> None:
        """Block until the coordinator has opened its first subscription.

        Publishing before then would move the head the coordinator snapshots,
        which silently changes which track each projection lands on.
        """
        while not self.subscribed_from:
            try:
                await asyncio.wait_for(
                    self._subscription_opened.wait(), timeout=LIVE_DELIVERY_TIMEOUT_S
                )
            except TimeoutError:
                raise AssertionError("coordinator never subscribed") from None

    def publish(self, envelope: EventEnvelope[DomainEvent]) -> None:
        """Append a live event and fan it out to every open subscription.

        It is appended to history too, so a subscription opened after this
        call finds it there. Delivery therefore does not depend on how many
        subscriptions happened to be open, which is what the tests vary.
        """
        self._events.append(envelope)
        for queue in self._listeners:
            queue.put_nowait(envelope)

    async def read_all(
        self,
        from_global_nonce: int = 0,
        max_count: int = 100,
        forward: bool = True,
    ) -> tuple[list[EventEnvelope[DomainEvent]], bool, int]:
        if not forward:
            # Backwards read: highest nonces first, which is how the
            # coordinator snapshots the head of the stream.
            tail = [e for e in self._events if (e.metadata.global_nonce or 0) <= from_global_nonce]
            return list(reversed(tail))[:max_count], True, 0
        page = [e for e in self._events if (e.metadata.global_nonce or 0) >= from_global_nonce]
        return page[:max_count], True, from_global_nonce + max_count

    async def subscribe(self, from_global_nonce: int) -> AsyncIterator[EventEnvelope[DomainEvent]]:
        self.subscribed_from.append(from_global_nonce)
        self._subscription_opened.set()
        queue: asyncio.Queue[EventEnvelope[DomainEvent]] = asyncio.Queue()
        # Register before snapshotting history so nothing published
        # concurrently is lost; `highest` then drops the duplicate.
        self._listeners.append(queue)
        history = list(self._events)
        highest = 0
        for envelope in history:
            nonce = envelope.metadata.global_nonce or 0
            if nonce >= from_global_nonce:
                highest = max(highest, nonce)
                yield envelope
        while True:
            envelope = await queue.get()
            nonce = envelope.metadata.global_nonce or 0
            if nonce <= highest:
                continue
            highest = nonce
            yield envelope


class RecordingProjection:
    """Projection that records what it saw, with what context, and checkpoints."""

    SIDE_EFFECTS_ALLOWED = False

    def __init__(self, name: str, version: int = 1) -> None:
        self._name = name
        self._version = version
        self.handled_nonces: list[int] = []
        self.contexts: dict[int, DispatchContext | None] = {}
        self.cleared = False

    def get_name(self) -> str:
        return self._name

    def get_version(self) -> int:
        return self._version

    def get_subscribed_event_types(self) -> set[str] | None:
        return None

    async def clear_all_data(self) -> None:
        self.cleared = True
        self.handled_nonces.clear()

    async def handle_event(
        self,
        envelope: EventEnvelope[DomainEvent],
        checkpoint_store: ProjectionCheckpointStore,
        context: DispatchContext | None = None,
    ) -> ProjectionResult:
        nonce = envelope.metadata.global_nonce or 0
        # Recorded before `_before_handle` so a projection parked mid-event
        # still reports the context it was dispatched with.
        self.contexts[nonce] = context
        await self._before_handle()
        self.handled_nonces.append(nonce)
        await checkpoint_store.save_checkpoint(
            ProjectionCheckpoint(
                projection_name=self._name,
                global_position=nonce,
                updated_at=datetime.now(UTC),
                version=self._version,
            )
        )
        return ProjectionResult.SUCCESS

    async def _before_handle(self) -> None:
        """Hook for subclasses. This projection handles events immediately."""
        # Yield control, so a coordinator that shares one stream cannot look
        # concurrent purely because no handler ever suspends.
        await asyncio.sleep(0)


class BlockingProjection(RecordingProjection):
    """A rebuild that makes no progress until the test releases it.

    Stands in for the real thing: a replay doing per-event database writes
    across ~13k events. Blocking has the same shape as slow, and it takes the
    timing guesswork out of the test.
    """

    def __init__(self, name: str, version: int = 1) -> None:
        super().__init__(name, version)
        self.released = asyncio.Event()
        self.reached_first_event = asyncio.Event()

    async def _before_handle(self) -> None:
        self.reached_first_event.set()
        await self.released.wait()


async def _checkpoint_at(
    store: MemoryCheckpointStore, name: str, position: int, version: int
) -> None:
    await store.save_checkpoint(
        ProjectionCheckpoint(
            projection_name=name,
            global_position=position,
            updated_at=datetime.now(UTC),
            version=version,
        )
    )


async def _await_nonce(projection: RecordingProjection, nonce: int) -> bool:
    """True if `projection` handles `nonce` within the delivery budget."""

    async def poll() -> None:
        while nonce not in projection.handled_nonces:
            await asyncio.sleep(0.01)

    try:
        await asyncio.wait_for(poll(), timeout=LIVE_DELIVERY_TIMEOUT_S)
    except TimeoutError:
        return False
    return True


class _Fixture:
    """One projection that must replay, one at head, and a live coordinator.

    `stored_version` is how the replay is provoked. Left at 1 it matches the
    projection's declared version, so no rebuild is triggered and the
    projection is simply behind - an interrupted replay, or one wedged by
    failures. Set to 0 it is a version bump, which the coordinator clears and
    replays from 0.
    """

    def __init__(self, stored_position: int = 0, stored_version: int = 0) -> None:
        self.store = BroadcastEventStore(HISTORY_SIZE)
        self.checkpoints = MemoryCheckpointStore()
        self.replaying = BlockingProjection("replaying", version=1)
        self.at_head = RecordingProjection("at_head", version=1)
        self._stored_position = stored_position
        self._stored_version = stored_version
        self.coordinator = SubscriptionCoordinator(
            event_store=self.store,
            checkpoint_store=self.checkpoints,
            projections=[self.replaying, self.at_head],
        )
        self._runner: asyncio.Task[None] | None = None

    async def start(self) -> None:
        await _checkpoint_at(
            self.checkpoints,
            "replaying",
            position=self._stored_position,
            version=self._stored_version,
        )
        # `at_head` is fully caught up and its version is unchanged.
        await _checkpoint_at(self.checkpoints, "at_head", position=self.store.head_nonce, version=1)
        self._runner = asyncio.create_task(self.coordinator.start())
        await self.store.wait_until_subscribed()

    async def stop(self) -> None:
        self.replaying.released.set()
        await self.coordinator.stop()
        if self._runner is not None:
            self._runner.cancel()
            await asyncio.gather(self._runner, return_exceptions=True)


class TestRebuildDoesNotStarveProjectionsAtHead:
    """The starvation from #1318, reproduced and pinned."""

    async def test_at_head_projection_receives_live_event_during_rebuild(self) -> None:
        """A projection at head must keep consuming while another rebuilds.

        Without a separate track this times out: the shared subscription is
        parked inside the rebuilding projection's first historical event, so
        the live event never reaches the projection that is already at head.
        """
        f = _Fixture(stored_position=HISTORY_SIZE, stored_version=0)
        await f.start()
        try:
            # Let the rebuild actually begin, so we measure starvation by a
            # replay in flight rather than a race with startup.
            assert await _await_started(f.replaying), "the rebuild never started"

            live_nonce = f.store.head_nonce + 1
            f.store.publish(_envelope(live_nonce))

            assert await _await_nonce(f.at_head, live_nonce), (
                f"projection at head never received live event {live_nonce} while "
                f"'rebuilding' replayed from 0 - it is starved behind the rebuild. "
                f"at_head handled={f.at_head.handled_nonces}, "
                f"rebuilding handled={f.replaying.handled_nonces}, "
                f"subscriptions opened from={f.store.subscribed_from}"
            )

            # The rebuild must still be mid-replay, which is what makes the
            # assertion above about concurrency and not about a rebuild that
            # happened to be quick.
            assert f.replaying.handled_nonces == [], (
                "the rebuild completed; the test no longer proves concurrency"
            )
            assert f.replaying.cleared is True, "a version bump must clear the rebuilt projection"
        finally:
            await f.stop()

    async def test_rebuild_and_live_tail_get_their_own_subscriptions(self) -> None:
        """Two tracks, two subscriptions, each from its own position.

        This is the mechanism behind the test above. A replay from 0 and a
        tail from head cannot share one cursor, so asserting the opened
        positions catches a "fix" that merely reorders dispatch.
        """
        f = _Fixture(stored_position=HISTORY_SIZE, stored_version=0)
        await f.start()
        try:
            assert await _await_started(f.replaying), "the rebuild never started"

            assert sorted(f.store.subscribed_from) == [0, HISTORY_SIZE + 1], (
                f"expected one subscription replaying from 0 and one tailing from "
                f"{HISTORY_SIZE + 1}, got {f.store.subscribed_from}"
            )
        finally:
            await f.stop()

    async def test_rebuild_stays_in_catch_up_while_the_at_head_track_goes_live(self) -> None:
        """Catch-up state must be per track, or replay fires side effects.

        The at-head track goes live immediately. If that flipped one shared
        flag, a rebuilding ``ProcessManager`` would see ``is_catching_up ==
        False`` for its historical events and ``process_pending()`` would fire
        during replay - the duplicate-side-effect bug ADR-025 exists to
        prevent. So the replay track's context must stay in catch-up while the
        at-head track's context reports live.
        """
        f = _Fixture(stored_position=HISTORY_SIZE, stored_version=0)
        await f.start()
        try:
            assert await _await_started(f.replaying), "the rebuild never started"

            live_nonce = f.store.head_nonce + 1
            f.store.publish(_envelope(live_nonce))
            assert await _await_nonce(f.at_head, live_nonce), (
                f"at-head projection never saw live event {live_nonce}"
            )

            live_context = f.at_head.contexts[live_nonce]
            assert live_context is not None
            assert live_context.is_catching_up is False, (
                "an event above the boundary must reach the at-head track as live"
            )

            replay_contexts = {
                nonce: ctx
                for nonce, ctx in f.replaying.contexts.items()
                if nonce <= f.store.head_nonce
            }
            assert replay_contexts, "the rebuild replayed nothing"
            assert all(
                ctx is not None and ctx.is_catching_up for ctx in replay_contexts.values()
            ), (
                f"the rebuild replayed historical events with is_catching_up=False, so "
                f"ProcessManager side effects would fire during replay (ADR-025): "
                f"{replay_contexts}"
            )
        finally:
            await f.stop()

    async def test_a_projection_left_behind_replays_separately_too(self) -> None:
        """Behind is behind, whether or not a rebuild was just triggered.

        This is what keeps the fix alive across a reconnect. A replay that is
        interrupted partway has a perfectly valid checkpoint at a low
        position, so on the next attempt nothing is "rebuilding" any more -
        and a coordinator that grouped on "did we just clear this one" would
        put it straight back on the shared subscription and starve everyone
        again. The backoff reconnect makes that likely during a long replay,
        so grouping is on position instead.
        """
        resumed_at = 10
        f = _Fixture(stored_position=resumed_at, stored_version=1)
        await f.start()
        try:
            assert await _await_started(f.replaying), "the interrupted replay never resumed"

            live_nonce = f.store.head_nonce + 1
            f.store.publish(_envelope(live_nonce))

            assert await _await_nonce(f.at_head, live_nonce), (
                f"projection at head never received live event {live_nonce} while a "
                f"projection behind at {resumed_at} caught up - it is starved behind "
                f"it. subscriptions opened from={f.store.subscribed_from}"
            )
            assert f.replaying.cleared is False, (
                "no version bump, so nothing should have been cleared"
            )
            assert sorted(f.store.subscribed_from) == [resumed_at + 1, HISTORY_SIZE + 1], (
                f"expected a replay resuming at {resumed_at + 1} and a tail from "
                f"{HISTORY_SIZE + 1}, got {f.store.subscribed_from}"
            )
        finally:
            await f.stop()


async def _await_started(projection: BlockingProjection) -> bool:
    try:
        await asyncio.wait_for(
            projection.reached_first_event.wait(), timeout=LIVE_DELIVERY_TIMEOUT_S
        )
    except TimeoutError:
        return False
    return True
