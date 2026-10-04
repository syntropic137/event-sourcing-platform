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

import pytest

from event_sourcing.stores.memory_checkpoint import MemoryCheckpointStore
from event_sourcing.subscriptions.coordinator import SubscriptionCoordinator
from tests.unit.test_coordinator_rebuild_isolation import (
    HISTORY_SIZE,
    BlockingProjection,
    BroadcastEventStore,
    RecordingProjection,
    _await_nonce,
    _await_started,
    _checkpoint_at,
    _envelope,
)

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
