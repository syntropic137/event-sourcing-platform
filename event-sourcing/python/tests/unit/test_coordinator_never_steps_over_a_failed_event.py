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

These tests pin the fix: a failed event stops the track, the checkpoint stays
below it, and the next subscription attempt delivers it again.
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


class FlakyProjection(AutoDispatchProjection):
    """Raises the first time it sees FAILS_AT, the way a store blip would."""

    def __init__(self) -> None:
        self.applied: list[int] = []
        self.failures = 0

    def get_name(self) -> str:
        return NAME

    def get_version(self) -> int:
        return 1

    async def clear_all_data(self) -> None:
        self.applied.clear()

    async def on_sample_event(self, data: dict[str, int]) -> None:
        if data["n"] == FAILS_AT and self.failures == 0:
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
