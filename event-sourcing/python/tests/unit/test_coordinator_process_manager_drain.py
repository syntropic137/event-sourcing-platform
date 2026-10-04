"""A slow ProcessManager must not hold up the projections it shares a track with (#1528).

Measured in production: one ProcessManager's ``process_pending()`` took 6-25s,
and the coordinator awaited it inline while dispatching. Every projection at
head shares the single live track and so one cursor, so all 28 read models
advanced about one event per 10s and fell further behind the store.

These tests pin the fix: each ProcessManager drains on its own task, woken
after a live event is delivered. Wakes coalesce, drains are single-flight,
``process_pending()`` is still never called while the ProcessManager's track
is catching up, and shutdown leaves no drain task behind.

Every wait that could hang on the old behaviour is bounded by
``asyncio.wait_for`` so it fails rather than hangs. The bound only turns a hang
into a failure; no assertion depends on how long anything took, except the
throughput floor, which is an explicit and deliberately loose budget.
"""

from __future__ import annotations

import asyncio
import logging
import time
from datetime import UTC, datetime
from typing import TYPE_CHECKING, TypeVar

import pytest

from event_sourcing.core.checkpoint import (
    DispatchContext,
    ProjectionCheckpoint,
    ProjectionCheckpointStore,
    ProjectionResult,
)
from event_sourcing.core.event import DomainEvent, EventEnvelope, EventMetadata
from event_sourcing.core.process_manager import ProcessManager
from event_sourcing.stores.memory_checkpoint import MemoryCheckpointStore
from event_sourcing.subscriptions.coordinator import SubscriptionCoordinator

if TYPE_CHECKING:
    from collections.abc import AsyncIterator, Awaitable

pytestmark = pytest.mark.unit

T = TypeVar("T")

# Upper bound on any single wait. The fixed path needs a few loop turns; the
# broken path never gets there, so this only decides how fast a failure shows.
HANG_TIMEOUT_S = 5.0

# Measured on the run that introduced this test (#1528), 10,000 events through
# 28 projections plus one blocked ProcessManager, in-memory checkpoint store:
# 1,426-1,497 events/s under the suite's default coverage tracing (pytest
# addopts), 3,929-4,852 events/s without it. The floor is about half the
# slowest covered run, so a loaded CI box does not flake. Awaiting the drain on
# the cursor again would not slow this down - it would stop it - which the
# starvation test pins.
THROUGHPUT_FLOOR_EVENTS_PER_S = 700
THROUGHPUT_HANG_TIMEOUT_S = 60.0


async def _within(awaitable: Awaitable[T], what: str) -> T:
    try:
        return await asyncio.wait_for(awaitable, timeout=HANG_TIMEOUT_S)
    except TimeoutError:
        raise AssertionError(f"timed out waiting for {what}") from None


class SampleEvent(DomainEvent):
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


async def _save(store: ProjectionCheckpointStore, name: str, position: int) -> None:
    await store.save_checkpoint(
        ProjectionCheckpoint(
            projection_name=name,
            global_position=position,
            updated_at=datetime.now(UTC),
            version=1,
        )
    )


class InMemoryEventStore:
    """Event store fake: one history, and a live feed per subscription."""

    def __init__(self) -> None:
        self._events: list[EventEnvelope[DomainEvent]] = []
        self._listeners: list[asyncio.Queue[EventEnvelope[DomainEvent]]] = []
        self.subscriptions = 0
        self._subscribed = asyncio.Event()

    @property
    def head_nonce(self) -> int:
        return len(self._events)

    def publish(self, count: int = 1) -> int:
        """Append `count` events, fan them out, and return the last nonce."""
        for _ in range(count):
            envelope = _envelope(self.head_nonce + 1)
            self._events.append(envelope)
            for queue in self._listeners:
                queue.put_nowait(envelope)
        return self.head_nonce

    async def wait_until_subscribed(self, count: int = 1) -> None:
        while self.subscriptions < count:
            self._subscribed.clear()
            await _within(self._subscribed.wait(), "the coordinator to subscribe")

    async def read_all(
        self,
        from_global_nonce: int = 0,
        max_count: int = 100,
        forward: bool = True,
    ) -> tuple[list[EventEnvelope[DomainEvent]], bool, int]:
        if not forward:
            return list(reversed(self._events))[:max_count], True, 0
        return self._events[from_global_nonce - 1 :][:max_count], True, 0

    async def subscribe(self, from_global_nonce: int) -> AsyncIterator[EventEnvelope[DomainEvent]]:
        queue: asyncio.Queue[EventEnvelope[DomainEvent]] = asyncio.Queue()
        self._listeners.append(queue)
        self.subscriptions += 1
        self._subscribed.set()
        try:
            highest = 0
            for envelope in list(self._events):
                nonce = envelope.metadata.global_nonce or 0
                if nonce >= from_global_nonce:
                    highest = nonce
                    yield envelope
            while True:
                envelope = await queue.get()
                nonce = envelope.metadata.global_nonce or 0
                if nonce > highest and nonce >= from_global_nonce:
                    highest = nonce
                    yield envelope
        finally:
            self._listeners.remove(queue)


class Projection:
    """Trivial read model: checkpoints every event and can be awaited on a nonce."""

    SIDE_EFFECTS_ALLOWED = False

    def __init__(self, name: str) -> None:
        self._name = name
        self.position = 0
        self._reached: dict[int, asyncio.Event] = {}

    def get_name(self) -> str:
        return self._name

    def get_version(self) -> int:
        return 1

    def get_subscribed_event_types(self) -> set[str] | None:
        return None

    async def clear_all_data(self) -> None:
        self.position = 0

    async def handle_event(
        self,
        envelope: EventEnvelope[DomainEvent],
        checkpoint_store: ProjectionCheckpointStore,
        context: DispatchContext | None = None,
    ) -> ProjectionResult:
        self.position = envelope.metadata.global_nonce or 0
        await _save(checkpoint_store, self._name, self.position)
        reached = self._reached.pop(self.position, None)
        if reached is not None:
            reached.set()
        return ProjectionResult.SUCCESS

    async def reach(self, nonce: int) -> None:
        if self.position < nonce:
            await self._reached.setdefault(nonce, asyncio.Event()).wait()


class SlowProcessManager(ProcessManager):
    """A ProcessManager whose drain blocks until the test releases it.

    Stands in for the 6-25s drain from #1528. Blocking has the shape of slow
    without a wall-clock sleep: the test decides when the drain finishes.
    """

    def __init__(self, name: str = "slow_pm", *, released: bool = False) -> None:
        self._name = name
        self.release = asyncio.Event()
        if released:
            self.release.set()
        self.entered = asyncio.Event()
        self.calls = 0
        self.cancelled = 0
        self.in_flight = 0
        self.max_in_flight = 0
        self.handled: list[int] = []
        # Catch-up state of the last event delivered before each call.
        self.called_after_catch_up_event: list[bool] = []
        self._last_context: DispatchContext | None = None

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
        await _save(checkpoint_store, self._name, nonce)
        return ProjectionResult.SUCCESS

    async def process_pending(self) -> int:
        self.calls += 1
        self.called_after_catch_up_event.append(
            self._last_context is not None and self._last_context.is_catching_up
        )
        self.in_flight += 1
        self.max_in_flight = max(self.max_in_flight, self.in_flight)
        self.entered.set()
        try:
            await self.release.wait()
        except asyncio.CancelledError:
            self.cancelled += 1
            raise
        finally:
            self.in_flight -= 1
        return 1

    def get_idempotency_key(self, todo_item: dict[str, str | int | float | bool | None]) -> str:
        return str(todo_item.get("id", ""))


class Running:
    """A coordinator running against the in-memory store, at head from nonce 0."""

    def __init__(self, projections: list[Projection | SlowProcessManager]) -> None:
        self.store = InMemoryEventStore()
        self.checkpoints = MemoryCheckpointStore()
        self.projections = projections
        self.coordinator = SubscriptionCoordinator(
            event_store=self.store,
            checkpoint_store=self.checkpoints,
            projections=projections,  # type: ignore[arg-type]
        )
        self._runner: asyncio.Task[None] | None = None

    async def start(self, *, at_head: bool = True) -> None:
        if at_head:
            # A checkpoint at the (empty) head puts everyone on the live track.
            for projection in self.projections:
                await _save(self.checkpoints, projection.get_name(), self.store.head_nonce)
        opened = self.store.subscriptions
        self._runner = asyncio.create_task(self.coordinator.start())
        await self.store.wait_until_subscribed(opened + 1)

    async def checkpoints_reach(self, name: str, nonce: int) -> None:
        while True:
            checkpoint = await self.checkpoints.get_checkpoint(name)
            if checkpoint is not None and checkpoint.global_position >= nonce:
                return
            await asyncio.sleep(0)

    async def stop(self) -> None:
        await self.coordinator.stop()
        await self.cancel()

    async def cancel(self) -> None:
        """Cancel start() without stop(), as a process shutdown would."""
        if self._runner is not None:
            self._runner.cancel()
            await asyncio.gather(self._runner, return_exceptions=True)
            self._runner = None


def _drain_tasks() -> list[asyncio.Task[object]]:
    return [
        task
        for task in asyncio.all_tasks()
        if task.get_name().startswith("process-manager-drain:") and not task.done()
    ]


class TestSlowProcessManagerDoesNotStarveSiblings:
    async def test_sibling_checkpoints_100_events_while_the_drain_is_blocked(self) -> None:
        """The #1528 starvation, reproduced through start() and pinned.

        The ProcessManager is first on the track, so on the old code the
        cursor parks inside its process_pending() on event 1 and the sibling
        never sees anything.
        """
        pm = SlowProcessManager()
        sibling = Projection("sibling")
        run = Running([pm, sibling])
        await run.start()
        try:
            first = run.store.publish()
            await _within(pm.entered.wait(), "the ProcessManager's drain to start")

            last = run.store.publish(100)
            await _within(
                sibling.reach(last),
                f"sibling to reach {last} while the ProcessManager drain is blocked "
                f"(sibling at {sibling.position}) - it is starved behind process_pending()",
            )

            checkpoint = await run.checkpoints.get_checkpoint("sibling")
            assert checkpoint is not None
            assert checkpoint.global_position == first + 100
            # Still the same, still-blocked drain: this is concurrency, not a
            # drain that happened to finish quickly.
            assert not pm.release.is_set()
            assert pm.in_flight == 1
            assert pm.calls == 1
            # The ProcessManager's projection side kept pace too.
            assert pm.handled == list(range(first, last + 1))
        finally:
            await run.stop()


class TestDrainCoalescingAndSingleFlight:
    async def test_fifty_wakes_during_a_blocked_drain_cause_exactly_one_more(self) -> None:
        pm = SlowProcessManager()
        run = Running([pm])
        await run.start()
        try:
            run.store.publish()
            await _within(pm.entered.wait(), "the first drain to start")

            last = run.store.publish(50)
            await _within(_handled(pm, last), "the ProcessManager to take all 50 events")
            assert pm.calls == 1, "a wake started a second drain while one was running"

            pm.release.set()
            await _within(run.coordinator.wait_for_process_managers(), "the drains to settle")

            assert pm.calls == 2
            assert pm.max_in_flight == 1
        finally:
            await run.stop()

    async def test_a_failing_drain_is_logged_and_the_next_wake_still_drains(
        self, caplog: pytest.LogCaptureFixture
    ) -> None:
        pm = SlowProcessManager(released=True)
        failures = 0
        original = pm.process_pending

        async def fail_once() -> int:
            nonlocal failures
            if failures == 0:
                failures += 1
                raise RuntimeError("upstream unavailable")
            return await original()

        pm.process_pending = fail_once  # type: ignore[method-assign]
        run = Running([pm])
        await run.start()
        try:
            with caplog.at_level(logging.ERROR):
                run.store.publish()
                await _within(_handled(pm, 1), "the first event")
                await _within(run.coordinator.wait_for_process_managers(), "the failed drain")
            assert "ProcessManager.process_pending() failed" in caplog.text

            # The start-up wake (items pending before a restart) may already
            # have drained, so count from here rather than from zero.
            before = pm.calls
            run.store.publish()
            await _within(_handled(pm, 2), "the second event")
            await _within(run.coordinator.wait_for_process_managers(), "the second drain")
            assert pm.calls == before + 1, "the drain loop did not survive the exception"
        finally:
            await run.stop()


class TestCatchUpInvariant:
    async def test_no_drain_during_replay_then_drains_once_live(self) -> None:
        pm = SlowProcessManager(released=True)
        run = Running([pm])
        run.store.publish(20)
        # No checkpoint: the ProcessManager replays 1..20 on a catching-up track.
        await run.start(at_head=False)
        try:
            await _within(_handled(pm, 20), "the replay")
            await _within(run.coordinator.wait_for_process_managers(), "the drains to settle")
            assert pm.calls == 0

            run.store.publish()
            await _within(_handled(pm, 21), "the live event")
            await _within(run.coordinator.wait_for_process_managers(), "the live drain")
            assert pm.calls == 1
            assert pm.called_after_catch_up_event == [False]
        finally:
            await run.stop()

    async def test_wake_raised_live_and_consumed_after_re_entering_catch_up_is_dropped(
        self,
    ) -> None:
        """The signal outlives the live state it was raised in, so it is re-checked."""
        pm = SlowProcessManager()
        run = Running([pm])
        await run.start()
        try:
            run.store.publish()
            await _within(pm.entered.wait(), "the first drain to start")
            # A wake is now pending behind the running drain...
            run.store.publish()
            await _within(_handled(pm, 2), "the second event")

            # ...and the track goes back into catch-up before it is consumed.
            run.coordinator.is_catching_up = True
            pm.release.set()
            await _within(run.coordinator.wait_for_process_managers(), "the drains to settle")

            assert pm.calls == 1, "process_pending() ran while the track was catching up"
        finally:
            await run.stop()

    async def test_rebuild_stops_the_drain_and_replay_does_not_restart_it(self) -> None:
        """A rebuild mid-drain: the drain is cancelled, the replay never drains."""
        pm = SlowProcessManager()
        run = Running([pm])
        await run.start()
        try:
            run.store.publish(5)
            await _within(pm.entered.wait(), "the live drain to start")
            await _within(_handled(pm, 5), "the live events")

            await run.coordinator.rebuild_projection(pm.get_name())
            assert pm.cancelled == 1, "the in-flight drain survived the rebuild"
            assert _drain_tasks() == []

            # Restart, as rebuild_projection() requires: the ProcessManager
            # now has no checkpoint and replays 1..5 on a catching-up track.
            await run.stop()
            pm.release.set()
            pm.handled.clear()
            calls_before_replay = pm.calls
            await run.start(at_head=False)

            await _within(_handled(pm, 5), "the replay")
            await _within(run.coordinator.wait_for_process_managers(), "the drains to settle")
            assert pm.calls == calls_before_replay, "process_pending() ran during the replay"

            run.store.publish()
            await _within(_handled(pm, 6), "the live event after the replay")
            await _within(run.coordinator.wait_for_process_managers(), "the live drain")
            assert pm.calls == calls_before_replay + 1
            assert not any(pm.called_after_catch_up_event)
        finally:
            await run.stop()


class TestPendingWorkIsNoticedWithoutANewEvent:
    async def test_a_live_process_manager_drains_at_start_with_no_event(self) -> None:
        # Items left pending before a restart: nothing new arrives, but the
        # to-do list is not empty, so the drain must run anyway.
        pm = SlowProcessManager(released=True)
        run = Running([pm])
        await run.start()
        try:
            await _within(run.coordinator.wait_for_process_managers(), "the start-up drain")
            assert pm.calls == 1
            assert pm.handled == []
        finally:
            await run.stop()

    async def test_a_catching_up_process_manager_does_not_drain_at_start(self) -> None:
        pm = SlowProcessManager(released=True)
        run = Running([pm])
        run.store.publish(5)
        await run.start(at_head=False)
        try:
            await _within(_handled(pm, 5), "the replay")
            await _within(run.coordinator.wait_for_process_managers(), "the drains to settle")
            assert pm.calls == 0
        finally:
            await run.stop()


class OtherEventsProcessManager(SlowProcessManager):
    """Subscribes to nothing the test store publishes, so it only ever skips."""

    def get_subscribed_event_types(self) -> set[str] | None:
        return {"SomethingElse"}


class TestGoingLiveWakesTheDrain:
    async def test_an_unhandled_event_crossing_the_boundary_still_drains(self) -> None:
        # Pending work, then a catch-up, then a live event this ProcessManager
        # does not subscribe to. No delivery wakes it, so only the catch-up ->
        # live transition can notice the to-do list.
        pm = OtherEventsProcessManager(released=True)
        run = Running([pm])
        run.store.publish(5)
        await run.start(at_head=False)
        try:
            await _within(run.coordinator.wait_for_process_managers(), "the replay to settle")
            assert pm.calls == 0

            run.store.publish()
            await _within(run.checkpoints_reach(pm.get_name(), 6), "the skipped live event")
            await _within(run.coordinator.wait_for_process_managers(), "the live drain")
            assert pm.calls == 1
            assert pm.handled == []
        finally:
            await run.stop()


class TestShutdown:
    async def test_stop_cancels_and_awaits_a_blocked_drain(self) -> None:
        pm = SlowProcessManager()
        run = Running([pm])
        await run.start()
        run.store.publish()
        await _within(pm.entered.wait(), "the drain to start")
        assert len(_drain_tasks()) == 1

        await run.stop()

        assert pm.cancelled == 1
        assert pm.in_flight == 0
        assert _drain_tasks() == []

    async def test_a_wake_after_stop_starts_no_drain(self) -> None:
        # A live handler still suspended when stop() closed the drains resumes
        # and dispatches: it must not leave a fresh drain task behind.
        pm = SlowProcessManager(released=True)
        run = Running([pm])
        await run.start()
        await _within(run.coordinator.wait_for_process_managers(), "the start-up drain")
        calls = pm.calls

        await run.stop()
        await run.coordinator.dispatch_event(_envelope(run.store.publish()))

        assert _drain_tasks() == []
        assert pm.calls == calls

    async def test_cancelling_start_closes_drains_without_stop(self) -> None:
        pm = SlowProcessManager()
        run = Running([pm])
        await run.start()
        run.store.publish()
        await _within(pm.entered.wait(), "the drain to start")

        await run.cancel()

        assert pm.cancelled == 1
        assert _drain_tasks() == []


class TestThroughputFloor:
    async def test_10k_events_through_28_projections_and_a_slow_process_manager(self) -> None:
        events = 10_000
        pm = SlowProcessManager()  # never released: one drain blocked throughout
        projections = [Projection(f"read_model_{index}") for index in range(28)]
        run = Running([pm, *projections])
        await run.start()
        try:
            started = time.perf_counter()
            last = run.store.publish(events)
            # Bounded only so a stalled cursor fails instead of hanging; the
            # floor is asserted on the measured rate below.
            await asyncio.wait_for(
                asyncio.gather(*(projection.reach(last) for projection in projections)),
                timeout=THROUGHPUT_HANG_TIMEOUT_S,
            )
            elapsed = time.perf_counter() - started
            rate = events / elapsed
            print(f"\nthroughput: {events} events in {elapsed:.2f}s = {rate:,.0f} events/s")

            assert rate >= THROUGHPUT_FLOOR_EVENTS_PER_S, (
                f"{rate:,.0f} events/s is below the {THROUGHPUT_FLOOR_EVENTS_PER_S:,} floor"
            )
            assert pm.handled[-1] == last
            assert pm.calls == 1
        finally:
            await run.stop()


async def _handled(pm: SlowProcessManager, nonce: int) -> None:
    while nonce not in pm.handled:
        await asyncio.sleep(0)


class FilteredProjection(Projection):
    """A read model that subscribes to a type this history never carries.

    Most production projections skip most events, so on a replay the
    coordinator's skip path, not ``handle_event``, is what every one of them
    pays per event (syntropic137#1554).
    """

    def get_subscribed_event_types(self) -> set[str] | None:
        return {"SomeOtherEvent"}


class CountingCheckpointStore(MemoryCheckpointStore):
    """Counts checkpoint round trips and signals when every name reaches a target.

    In memory each call is free, so the count is the number that matters: in
    production every call is a Postgres round trip.
    """

    def __init__(self, names: list[str], target: int) -> None:
        super().__init__()
        self.gets = 0
        self.saves = 0
        self._target = target
        self._pending = set(names)
        self.all_reached = asyncio.Event()

    async def get_checkpoint(self, projection_name: str) -> ProjectionCheckpoint | None:
        self.gets += 1
        return await super().get_checkpoint(projection_name)

    async def save_checkpoint(self, checkpoint: ProjectionCheckpoint) -> None:
        self.saves += 1
        await super().save_checkpoint(checkpoint)
        if checkpoint.global_position >= self._target:
            self._pending.discard(checkpoint.projection_name)
            if not self._pending:
                self.all_reached.set()


class TestReplayThroughput:
    """Replay from 0, measured: rate, and checkpoint round trips per event.

    Not a floor on the round-trip count - the print is the measurement the
    PR reports - but the rate is held to the same loose floor as the live
    benchmark so a pathological regression still fails.
    """

    async def test_10k_event_replay_of_28_projections_from_zero(self) -> None:
        events = 10_000
        handling = [Projection(f"handles_{index}") for index in range(14)]
        skipping = [FilteredProjection(f"skips_{index}") for index in range(14)]
        projections: list[Projection] = [*handling, *skipping]
        store = InMemoryEventStore()
        store.publish(events)
        checkpoints = CountingCheckpointStore([p.get_name() for p in projections], events)
        coordinator = SubscriptionCoordinator(
            event_store=store,
            checkpoint_store=checkpoints,
            projections=projections,  # type: ignore[arg-type]
        )
        started = time.perf_counter()
        runner = asyncio.create_task(coordinator.start())
        try:
            await asyncio.wait_for(checkpoints.all_reached.wait(), THROUGHPUT_HANG_TIMEOUT_S)
            elapsed = time.perf_counter() - started
            rate = events / elapsed
            round_trips = checkpoints.gets + checkpoints.saves
            print(
                f"\nreplay: {events} events x {len(projections)} projections in "
                f"{elapsed:.2f}s = {rate:,.0f} events/s; checkpoint round trips "
                f"{round_trips:,} ({checkpoints.gets:,} get + {checkpoints.saves:,} save) "
                f"= {round_trips / events:.1f}/event"
            )
            assert rate >= THROUGHPUT_FLOOR_EVENTS_PER_S
        finally:
            await coordinator.stop()
            runner.cancel()
            await asyncio.gather(runner, return_exceptions=True)
