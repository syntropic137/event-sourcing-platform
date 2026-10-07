"""A projection that fails an event must not checkpoint past it (syntropic137#1696).

Measured in production: two ``WorkflowExecutionStarted`` events were passed by
both execution read models without being applied. The rows never existed, the
checkpoints were above them, and nothing reported a problem until a separate
detector compared the two.

The mechanism is the coordinator. A handler that failed was logged and the
track moved on ("event will be retried" - nothing retried it). The
projection's next successful event saved its own, higher, checkpoint, and an
event below a checkpoint is never read again. One transient store error was
enough to lose an event for good.

These tests pin the fix: the projection that failed is held below the event,
its checkpoint stays there, it is fed the event again until it applies it, and
every other projection keeps consuming meanwhile (one poison event must not
stall the whole read side).
"""

from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING

import pytest

from event_sourcing.core.checkpoint import AutoDispatchProjection
from event_sourcing.core.errors import ProjectionHandlerFailedError
from event_sourcing.core.event import DomainEvent, EventEnvelope, EventMetadata
from event_sourcing.stores.memory_checkpoint import MemoryCheckpointStore
from event_sourcing.subscriptions.coordinator import SubscriptionCoordinator

if TYPE_CHECKING:
    from collections.abc import AsyncIterator

pytestmark = pytest.mark.unit

NAME = "flaky"
FAILS_AT = 2
HISTORY = (1, 2, 3)
#: start() backs off 1s after a failed attempt; this leaves room for one retry.
SETTLE_TIMEOUT_S = 5.0


class SampleEvent(DomainEvent):
    event_type = "SampleEvent"
    n: int


def _envelope(nonce: int) -> EventEnvelope[DomainEvent]:
    return EventEnvelope(
        event=SampleEvent(n=nonce),
        metadata=EventMetadata(
            aggregate_nonce=nonce,
            aggregate_id="agg-1",
            aggregate_type="SampleAggregate",
            event_type="SampleEvent",
            global_nonce=nonce,
        ),
    )


class FiniteEventStore:
    """History only: each subscription yields what is stored from its position on."""

    def __init__(self) -> None:
        self._events = [_envelope(n) for n in HISTORY]
        self.subscribed_from: list[int] = []

    async def read_all(
        self, from_global_nonce: int = 0, max_count: int = 100, forward: bool = True
    ) -> tuple[list[EventEnvelope[DomainEvent]], bool, int]:
        if not forward:
            tail = [e for e in self._events if (e.metadata.global_nonce or 0) <= from_global_nonce]
            return list(reversed(tail))[:max_count], True, 0
        page = [e for e in self._events if (e.metadata.global_nonce or 0) >= from_global_nonce]
        return page[:max_count], True, from_global_nonce + max_count

    async def subscribe(self, from_global_nonce: int) -> AsyncIterator[EventEnvelope[DomainEvent]]:
        self.subscribed_from.append(from_global_nonce)
        for envelope in self._events:
            if (envelope.metadata.global_nonce or 0) >= from_global_nonce:
                yield envelope
        # A real subscription stays open; idling keeps start() from re-planning
        # in a tight loop once everything is applied.
        await asyncio.Event().wait()


class OtherEvent(DomainEvent):
    event_type = "OtherEvent"
    n: int


def _other(nonce: int) -> EventEnvelope[DomainEvent]:
    """An event of a type FlakyProjection does not subscribe to (it skips it)."""
    return EventEnvelope(
        event=OtherEvent(n=nonce),
        metadata=EventMetadata(
            aggregate_nonce=nonce,
            aggregate_id="agg-2",
            aggregate_type="SampleAggregate",
            event_type="OtherEvent",
            global_nonce=nonce,
        ),
    )


class FlakyProjection(AutoDispatchProjection):
    """Raises the first ``fail_times`` times it sees FAILS_AT, the way a store blip would."""

    def __init__(self, name: str = NAME, fail_times: int = 1) -> None:
        self.applied: list[int] = []
        self.failures = 0
        self._name = name
        self._fail_times = fail_times

    def get_name(self) -> str:
        return self._name

    def get_version(self) -> int:
        return 1

    async def clear_all_data(self) -> None:
        self.applied.clear()

    async def on_sample_event(self, data: dict[str, int]) -> None:
        if data["n"] == FAILS_AT and self.failures < self._fail_times:
            self.failures += 1
            raise ConnectionError("projection store unavailable")
        self.applied.append(data["n"])


async def test_a_failed_event_holds_the_checkpoint_below_it() -> None:
    """The defect itself: before the fix this applied [1, 3] and checkpointed at 3."""
    projection = FlakyProjection()
    checkpoints = MemoryCheckpointStore()
    coordinator = SubscriptionCoordinator(
        event_store=FiniteEventStore(), checkpoint_store=checkpoints, projections=[projection]
    )

    await coordinator.dispatch_event(_envelope(1))
    with pytest.raises(ProjectionHandlerFailedError) as raised:
        await coordinator.dispatch_event(_envelope(FAILS_AT))

    assert raised.value.global_nonce == FAILS_AT
    checkpoint = await checkpoints.get_checkpoint(NAME)
    assert checkpoint is not None
    assert checkpoint.global_position == 1


async def test_a_later_event_does_not_step_over_the_failed_one() -> None:
    """Not even one it skips: a skip saves a checkpoint too."""
    projection = FlakyProjection()
    checkpoints = MemoryCheckpointStore()
    coordinator = SubscriptionCoordinator(
        event_store=FiniteEventStore(), checkpoint_store=checkpoints, projections=[projection]
    )
    coordinator.is_catching_up = False  # skips are saved at once while live

    await coordinator.dispatch_event(_envelope(1))
    with pytest.raises(ProjectionHandlerFailedError):
        await coordinator.dispatch_event(_envelope(FAILS_AT))
    await coordinator.dispatch_event(_other(3))
    await coordinator.dispatch_event(_envelope(4))

    checkpoint = await checkpoints.get_checkpoint(NAME)
    assert checkpoint is not None
    assert checkpoint.global_position == 1
    assert projection.applied == [1]
    assert set(coordinator.held_projections) == {NAME}

    # Redelivered, it is applied and the projection is released.
    await coordinator.dispatch_event(_envelope(FAILS_AT))
    await coordinator.dispatch_event(_envelope(4))
    assert projection.applied == [1, FAILS_AT, 4]
    assert coordinator.held_projections == {}


async def test_the_others_still_get_the_event_one_projection_failed() -> None:
    failing = FlakyProjection("failing")
    healthy = FlakyProjection("healthy", fail_times=0)
    checkpoints = MemoryCheckpointStore()
    coordinator = SubscriptionCoordinator(
        event_store=FiniteEventStore(),
        checkpoint_store=checkpoints,
        projections=[failing, healthy],  # failing is offered the event first
    )

    await coordinator.dispatch_event(_envelope(1))
    with pytest.raises(ProjectionHandlerFailedError) as raised:
        await coordinator.dispatch_event(_envelope(FAILS_AT))

    assert raised.value.projection_name == "failing"
    assert healthy.applied == [1, FAILS_AT]
    healthy_checkpoint = await checkpoints.get_checkpoint("healthy")
    assert healthy_checkpoint is not None
    assert healthy_checkpoint.global_position == FAILS_AT


async def test_a_poison_event_holds_only_its_own_projection() -> None:
    """Under start(): the sibling reaches head while the failing one is held and retried."""
    poisoned = FlakyProjection("poisoned", fail_times=1_000)
    healthy = FlakyProjection("healthy", fail_times=0)
    checkpoints = MemoryCheckpointStore()
    store = FiniteEventStore()
    coordinator = SubscriptionCoordinator(
        event_store=store, checkpoint_store=checkpoints, projections=[poisoned, healthy]
    )

    running = asyncio.create_task(coordinator.start())
    try:
        async with asyncio.timeout(SETTLE_TIMEOUT_S):
            # Two failures: the first delivery and one retry on its own track.
            while healthy.applied != list(HISTORY) or poisoned.failures < 2:
                await asyncio.sleep(0.01)
        assert not coordinator.is_healthy
        held = coordinator.held_projections
    finally:
        await coordinator.stop()
        running.cancel()
        await asyncio.gather(running, return_exceptions=True)

    assert set(held) == {"poisoned"}
    assert held["poisoned"].global_nonce == FAILS_AT
    assert poisoned.applied == [1]
    poisoned_checkpoint = await checkpoints.get_checkpoint("poisoned")
    assert poisoned_checkpoint is not None
    assert poisoned_checkpoint.global_position == 1
    healthy_checkpoint = await checkpoints.get_checkpoint("healthy")
    assert healthy_checkpoint is not None
    assert healthy_checkpoint.global_position == HISTORY[-1]
    # The plan never restarted: the live tail was subscribed once, and every
    # later subscription is the poisoned projection's own retry from its
    # held checkpoint.
    live_tail = HISTORY[-1] + 1
    assert store.subscribed_from.count(live_tail) == 1
    retries = store.subscribed_from[store.subscribed_from.index(FAILS_AT) :]
    assert retries and set(retries) == {FAILS_AT}


async def test_the_failed_event_is_delivered_again_and_applied_once() -> None:
    projection = FlakyProjection()
    checkpoints = MemoryCheckpointStore()
    store = FiniteEventStore()
    coordinator = SubscriptionCoordinator(
        event_store=store, checkpoint_store=checkpoints, projections=[projection]
    )

    running = asyncio.create_task(coordinator.start())
    try:
        async with asyncio.timeout(SETTLE_TIMEOUT_S):
            while projection.applied != list(HISTORY):
                await asyncio.sleep(0.01)
        assert coordinator.held_projections == {}
        assert coordinator.is_healthy
    finally:
        await coordinator.stop()
        running.cancel()
        await asyncio.gather(running, return_exceptions=True)

    assert projection.failures == 1
    assert projection.applied == list(HISTORY)
    # The retry resumed from the held checkpoint, not from the start.
    assert store.subscribed_from[-1] == FAILS_AT
    checkpoint = await checkpoints.get_checkpoint(NAME)
    assert checkpoint is not None
    assert checkpoint.global_position == HISTORY[-1]
