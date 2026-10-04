"""A projection near head must never share a replay track with a from-zero rebuild (syntropic137#1554).

Measured in production on 2026-10-04: after an event-store crash loop, 23
projections were 9 events behind the live boundary and 4 were rebuilding from
0 after version bumps. ``_plan_tracks`` classed all 27 as "behind" and dealt
them round-robin onto ``replay_concurrency=4`` replay tracks. A track starts
at its furthest-behind member, so every one of the 23 shared a cursor with a
from-zero rebuild and waited for ~46k events of replay (about an hour at the
observed ~12 events/s) to receive events 9 positions away.

These tests pin the fix: tracks are planned by position. A projection within
``near_head_window`` events of the boundary catches up on a short track of its
own and then runs live, so its latency depends on its own distance from head
and never on a sibling's.
"""

from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING

import pytest

from event_sourcing.core.checkpoint import (
    DispatchContext,
    ProjectionCheckpoint,
    ProjectionCheckpointStore,
    ProjectionResult,
)
from event_sourcing.core.process_manager import ProcessManager
from event_sourcing.stores.memory_checkpoint import MemoryCheckpointStore
from event_sourcing.subscriptions.coordinator import (
    CATCH_UP_SKIP_CHECKPOINT_INTERVAL,
    DEFAULT_NEAR_HEAD_WINDOW,
    SubscriptionCoordinator,
)
from tests.unit.test_coordinator_rebuild_isolation import (
    HISTORY_SIZE,
    LIVE_DELIVERY_TIMEOUT_S,
    BlockingProjection,
    BroadcastEventStore,
    RecordingProjection,
    _await_nonce,
    _await_started,
    _checkpoint_at,
    _envelope,
)

if TYPE_CHECKING:
    from event_sourcing.core.event import DomainEvent, EventEnvelope

pytestmark = pytest.mark.unit

# The production shape: checkpoint 46350 against boundary 46359.
NEAR_DISTANCE = 9


class _Plan:
    """Rebuilds parked at 0, near-head projections, and a running coordinator.

    Every rebuild is a ``BlockingProjection`` that never gets released before
    the assertions, so any projection sharing its cursor cannot move. That is
    the hostage situation from the incident with the timing taken out.
    """

    def __init__(
        self,
        *,
        rebuilds: int,
        near: dict[str, int],
        replay_concurrency: int,
        near_head_window: int | None = None,
    ) -> None:
        self.store = BroadcastEventStore(HISTORY_SIZE)
        self.checkpoints = MemoryCheckpointStore()
        self.rebuilding = [BlockingProjection(f"rebuild-{index}") for index in range(rebuilds)]
        # name -> checkpointed position
        self.near_positions = near
        self.near = {name: RecordingProjection(name) for name in near}
        kwargs: dict[str, int] = {"replay_concurrency": replay_concurrency}
        if near_head_window is not None:
            kwargs["near_head_window"] = near_head_window
        self.coordinator = SubscriptionCoordinator(
            event_store=self.store,
            checkpoint_store=self.checkpoints,
            projections=[*self.rebuilding, *self.near.values()],
            **kwargs,
        )
        self._runner: asyncio.Task[None] | None = None

    async def start(self) -> None:
        # Rebuilds have no checkpoint at all: they replay from 0.
        for name, position in self.near_positions.items():
            await _checkpoint_at(self.checkpoints, name, position=position, version=1)
        self._runner = asyncio.create_task(self.coordinator.start())
        await self.store.wait_until_subscribed()

    async def stop(self) -> None:
        for projection in self.rebuilding:
            projection.released.set()
        await self.coordinator.stop()
        if self._runner is not None:
            self._runner.cancel()
            await asyncio.gather(self._runner, return_exceptions=True)


class TestNearHeadIsNeverHeldByARebuild:
    async def test_projection_9_behind_reaches_head_while_a_rebuild_from_0_replays(self) -> None:
        """The incident in miniature: one projection at 0, one at boundary-9.

        ``replay_concurrency=1`` is the smallest config where both used to
        land on the same replay track; the production deploy got there with
        27 projections and 4 tracks. Before the fix this times out: the shared
        cursor starts at 0 and is parked inside the rebuild's first event.
        """
        near_at = HISTORY_SIZE - NEAR_DISTANCE
        plan = _Plan(rebuilds=1, near={"near": near_at}, replay_concurrency=1)
        await plan.start()
        try:
            rebuild = plan.rebuilding[0]
            near = plan.near["near"]
            assert await _await_started(rebuild), "the rebuild never started"

            assert await _await_nonce(near, HISTORY_SIZE), (
                f"projection {NEAR_DISTANCE} events behind never reached head "
                f"({HISTORY_SIZE}) while a rebuild replayed from 0 - it shares the "
                f"rebuild's cursor. near handled={near.handled_nonces}, "
                f"subscriptions opened from={plan.store.subscribed_from}"
            )
            assert near.handled_nonces == list(range(near_at + 1, HISTORY_SIZE + 1)), (
                f"near-head projection should replay exactly its own "
                f"{NEAR_DISTANCE} events, got {near.handled_nonces}"
            )

            live_nonce = plan.store.head_nonce + 1
            plan.store.publish(_envelope(live_nonce))
            assert await _await_nonce(near, live_nonce), (
                "near-head projection caught up but never received the next live event"
            )

            # The rebuild must still be parked on its first event: the claim is
            # about concurrency, not about a rebuild that happened to be quick.
            assert rebuild.handled_nonces == [], "the rebuild progressed; test proves nothing"
        finally:
            await plan.stop()

    async def test_the_production_shape_4_rebuilds_and_many_near_head(self) -> None:
        """27 projections in production; 4 rebuilds and 8 near-head here.

        Round-robin over 4 replay tracks put two near-head projections on each
        rebuild's track. Every near-head projection must reach head while all
        four rebuilds are still parked.
        """
        near = {f"near-{index}": HISTORY_SIZE - NEAR_DISTANCE for index in range(8)}
        plan = _Plan(rebuilds=4, near=near, replay_concurrency=4)
        await plan.start()
        try:
            for rebuild in plan.rebuilding:
                assert await _await_started(rebuild), f"{rebuild.get_name()} never started"

            for name, projection in plan.near.items():
                assert await _await_nonce(projection, HISTORY_SIZE), (
                    f"{name} never reached head while the rebuilds replayed; "
                    f"subscriptions opened from={plan.store.subscribed_from}"
                )

            assert all(rebuild.handled_nonces == [] for rebuild in plan.rebuilding)
            # One near-head track, never one per projection: connections stay
            # bounded at live + near + replay_concurrency.
            assert plan.store.max_open_subscriptions <= 2 + 4, plan.store.subscribed_from
        finally:
            await plan.stop()


class TestWindowBoundary:
    WINDOW = 20

    async def test_exactly_window_events_behind_is_near_head(self) -> None:
        at_window = HISTORY_SIZE - self.WINDOW  # needs WINDOW events: at_window+1..HISTORY
        plan = _Plan(
            rebuilds=1,
            near={"edge": at_window},
            replay_concurrency=1,
            near_head_window=self.WINDOW,
        )
        await plan.start()
        try:
            assert await _await_started(plan.rebuilding[0]), "the rebuild never started"
            assert await _await_nonce(plan.near["edge"], HISTORY_SIZE), (
                f"a projection exactly {self.WINDOW} events behind is inside the window "
                f"and must not wait on the rebuild; opened from={plan.store.subscribed_from}"
            )
            assert sorted(plan.store.subscribed_from) == [0, at_window + 1, HISTORY_SIZE + 1]
        finally:
            await plan.stop()

    async def test_one_past_the_window_replays_with_the_rebuilds(self) -> None:
        """One event further and it is a replay, so it competes for replay tracks.

        With one replay track that means sharing the rebuild's cursor: the
        window is a hard edge, which is what makes it configurable rather than
        a guess.
        """
        past_window = HISTORY_SIZE - self.WINDOW - 1
        plan = _Plan(
            rebuilds=1,
            near={"far": past_window},
            replay_concurrency=1,
            near_head_window=self.WINDOW,
        )
        await plan.start()
        try:
            assert await _await_started(plan.rebuilding[0]), "the rebuild never started"
            assert sorted(plan.store.subscribed_from) == [0, HISTORY_SIZE + 1], (
                f"a projection {self.WINDOW + 1} events behind is outside the window; "
                f"expected only the replay and live tracks, got {plan.store.subscribed_from}"
            )
            await asyncio.sleep(0.05)
            assert plan.near["far"].handled_nonces == []
        finally:
            await plan.stop()

    async def test_window_zero_disables_the_near_head_track(self) -> None:
        plan = _Plan(
            rebuilds=1,
            near={"near": HISTORY_SIZE - 1},
            replay_concurrency=1,
            near_head_window=0,
        )
        await plan.start()
        try:
            assert await _await_started(plan.rebuilding[0]), "the rebuild never started"
            assert sorted(plan.store.subscribed_from) == [0, HISTORY_SIZE + 1]
        finally:
            await plan.stop()

    def test_default_window_covers_the_incident(self) -> None:
        assert DEFAULT_NEAR_HEAD_WINDOW >= NEAR_DISTANCE

    def test_negative_window_is_rejected(self) -> None:
        with pytest.raises(ValueError, match="near_head_window"):
            SubscriptionCoordinator(
                event_store=BroadcastEventStore(1),
                checkpoint_store=MemoryCheckpointStore(),
                projections=[],
                near_head_window=-1,
            )


class _RecordingProcessManager(ProcessManager):
    """Near-head ProcessManager that records the catch-up state behind each drain."""

    def __init__(self, name: str) -> None:
        self._name = name
        self.handled: list[int] = []
        self.calls = 0
        self.catching_up_at_call: list[bool] = []
        self._last_context: DispatchContext | None = None
        self.called = asyncio.Event()

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
        self._last_context = context
        await _checkpoint_at(
            checkpoint_store,  # type: ignore[arg-type]
            self._name,
            position=nonce,
            version=1,
        )
        return ProjectionResult.SUCCESS

    async def process_pending(self) -> int:
        self.calls += 1
        self.catching_up_at_call.append(
            self._last_context is not None and self._last_context.is_catching_up
        )
        self.called.set()
        return 0

    def get_idempotency_key(self, todo_item: dict[str, str | int | float | bool | None]) -> str:
        return str(todo_item.get("id", ""))


class TestDrainGateOnTheNearHeadTrack:
    async def test_process_pending_waits_for_the_near_head_track_to_go_live(self) -> None:
        """The near-head track replays history, so it is a catch-up track.

        ADR-025 / #334: process_pending() is never called while the
        ProcessManager's own track is catching up. Joining a short track must
        not shortcut that.
        """
        store = BroadcastEventStore(HISTORY_SIZE)
        checkpoints = MemoryCheckpointStore()
        rebuild = BlockingProjection("rebuild")
        pm = _RecordingProcessManager("near_pm")
        near_at = HISTORY_SIZE - NEAR_DISTANCE
        await _checkpoint_at(checkpoints, "near_pm", position=near_at, version=1)
        coordinator = SubscriptionCoordinator(
            event_store=store,
            checkpoint_store=checkpoints,
            projections=[rebuild, pm],
            replay_concurrency=1,
        )
        runner = asyncio.create_task(coordinator.start())
        try:
            await store.wait_until_subscribed()
            assert await _await_started(rebuild)

            async def caught_up() -> None:
                while HISTORY_SIZE not in pm.handled:
                    await asyncio.sleep(0.01)

            await asyncio.wait_for(caught_up(), timeout=LIVE_DELIVERY_TIMEOUT_S)
            await coordinator.wait_for_process_managers()
            assert pm.calls == 0, "process_pending ran while the near-head track was replaying"

            store.publish(_envelope(HISTORY_SIZE + 1))
            await asyncio.wait_for(pm.called.wait(), timeout=LIVE_DELIVERY_TIMEOUT_S)
            await coordinator.wait_for_process_managers()
            assert pm.calls >= 1
            assert pm.catching_up_at_call == [False] * pm.calls
        finally:
            rebuild.released.set()
            await coordinator.stop()
            runner.cancel()
            await asyncio.gather(runner, return_exceptions=True)


def _typed(global_nonce: int, event_type: str) -> EventEnvelope[DomainEvent]:
    envelope = _envelope(global_nonce)
    metadata = envelope.metadata.model_copy(update={"event_type": event_type})
    return envelope.model_copy(update={"metadata": metadata})


class _SavesRecorder(MemoryCheckpointStore):
    def __init__(self) -> None:
        super().__init__()
        self.saved: dict[str, list[int]] = {}

    async def save_checkpoint(self, checkpoint: ProjectionCheckpoint) -> None:
        self.saved.setdefault(checkpoint.projection_name, []).append(checkpoint.global_position)
        await super().save_checkpoint(checkpoint)


class _Subscribes(RecordingProjection):
    def __init__(self, name: str, types: set[str]) -> None:
        super().__init__(name)
        self._types = types

    def get_subscribed_event_types(self) -> set[str] | None:
        return self._types


class TestCatchUpSkipCheckpoints:
    """Skips are checkpointed in batches during catch-up, never out of order."""

    async def _replay(
        self, projections: list[RecordingProjection], types: list[str]
    ) -> tuple[SubscriptionCoordinator, _SavesRecorder]:
        checkpoints = _SavesRecorder()
        coordinator = SubscriptionCoordinator(
            event_store=BroadcastEventStore(1),
            checkpoint_store=checkpoints,
            projections=list(projections),
        )
        boundary = len(types)
        coordinator.live_boundary_nonce = boundary
        coordinator._tracks = await coordinator._plan_tracks(boundary)
        (track,) = [t for t in coordinator._tracks if t.projections]
        assert track.is_catching_up
        for nonce, event_type in enumerate(types, start=1):
            await coordinator._dispatch_to_track(track, _typed(nonce, event_type))
        return coordinator, checkpoints

    async def test_a_skip_only_replay_saves_per_interval_and_at_the_boundary(self) -> None:
        events = CATCH_UP_SKIP_CHECKPOINT_INTERVAL * 2 + 37
        skipper = _Subscribes("skipper", {"Never"})
        _, checkpoints = await self._replay([skipper], ["Other"] * events)

        assert checkpoints.saved["skipper"] == [
            CATCH_UP_SKIP_CHECKPOINT_INTERVAL,
            CATCH_UP_SKIP_CHECKPOINT_INTERVAL * 2,
            events,
        ], "skips must be saved once per interval and once at the boundary"

    async def test_mixed_handle_and_skip_never_moves_a_checkpoint_backwards(self) -> None:
        types = ["Wanted" if nonce % 7 == 0 else "Other" for nonce in range(1, 1201)]
        mixed = _Subscribes("mixed", {"Wanted"})
        _, checkpoints = await self._replay([mixed], types)

        saved = checkpoints.saved["mixed"]
        assert saved == sorted(saved), f"checkpoint moved backwards: {saved}"
        assert saved[-1] == len(types)
        assert mixed.handled_nonces == [n for n in range(1, 1201) if n % 7 == 0]
        # One save per handled event (the projection's own) plus the skip
        # saves at 500, 1000 and the boundary: a handled event supersedes a
        # held-back skip rather than costing a save of its own.
        assert len(saved) == len(mixed.handled_nonces) + 3, saved

    async def test_rebuild_drops_a_held_back_skip(self) -> None:
        skipper = _Subscribes("skipper", {"Never"})
        checkpoints = _SavesRecorder()
        coordinator = SubscriptionCoordinator(
            event_store=BroadcastEventStore(1),
            checkpoint_store=checkpoints,
            projections=[skipper],
        )
        coordinator.live_boundary_nonce = 100
        coordinator._tracks = await coordinator._plan_tracks(100)
        (track,) = [t for t in coordinator._tracks if t.projections]
        for nonce in range(1, 11):
            await coordinator._dispatch_to_track(track, _typed(nonce, "Other"))
        assert track.unsaved_skips == {"skipper": 10}

        await coordinator.rebuild_projection("skipper")
        await coordinator._save_skips(track, 10)

        assert await checkpoints.get_checkpoint("skipper") is None, (
            "a skip held back before the rebuild resurrected the deleted checkpoint"
        )


class _SlowSaves(_SavesRecorder):
    """A store whose next save parks mid-flight until released."""

    def __init__(self) -> None:
        super().__init__()
        self.park_next_save = False
        self.parked = asyncio.Event()
        self.release = asyncio.Event()

    async def save_checkpoint(self, checkpoint: ProjectionCheckpoint) -> None:
        if self.park_next_save:
            self.park_next_save = False
            self.parked.set()
            await self.release.wait()
        await super().save_checkpoint(checkpoint)


class TestSkipSaveRacesRebuild:
    async def test_a_skip_save_in_flight_never_lands_after_the_rebuild_delete(self) -> None:
        """Codex review: a skip save awaiting the store while rebuild_projection runs.

        The rebuild must wait for it and delete afterwards, so the rebuilt
        projection has no checkpoint.
        """
        skipper = _Subscribes("skipper", {"Never"})
        checkpoints = _SlowSaves()
        coordinator = SubscriptionCoordinator(
            event_store=BroadcastEventStore(1),
            checkpoint_store=checkpoints,
            projections=[skipper],
        )
        checkpoints.park_next_save = True
        in_flight = asyncio.create_task(coordinator._advance_checkpoint_if_behind("skipper", 50))
        await asyncio.wait_for(checkpoints.parked.wait(), LIVE_DELIVERY_TIMEOUT_S)

        rebuild = asyncio.create_task(coordinator.rebuild_projection("skipper"))
        await asyncio.sleep(0)
        assert not rebuild.done(), "rebuild deleted while a skip save was still in flight"
        checkpoints.release.set()
        await asyncio.wait_for(asyncio.gather(in_flight, rebuild), LIVE_DELIVERY_TIMEOUT_S)

        assert await checkpoints.get_checkpoint("skipper") is None
