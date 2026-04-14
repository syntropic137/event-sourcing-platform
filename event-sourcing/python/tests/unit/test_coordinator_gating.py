"""Tests for coordinator catch-up tracking and ProcessManager gating.

Verifies that the SubscriptionCoordinator:
1. Tracks catch-up/live transition using global_nonce boundary
2. Passes DispatchContext to handle_event()
3. Gates process_pending() - never called during catch-up
4. Calls process_pending() only for live events on ProcessManager instances
"""

from __future__ import annotations

from unittest.mock import AsyncMock

import pytest

from event_sourcing.core.checkpoint import (
    DispatchContext,
    ProjectionCheckpointStore,
    ProjectionResult,
)
from event_sourcing.core.event import DomainEvent, EventEnvelope, EventMetadata
from event_sourcing.core.process_manager import ProcessManager
from event_sourcing.stores.memory_checkpoint import MemoryCheckpointStore
from event_sourcing.subscriptions.coordinator import SubscriptionCoordinator

# ============================================================================
# Test Fixtures
# ============================================================================


class TestEvent(DomainEvent):
    """Simple test event."""

    event_type = "TestEvent"
    value: str


class StubProjection:
    """Minimal stub that acts like CheckpointedProjection without subclassing."""

    SIDE_EFFECTS_ALLOWED = False

    def __init__(self, name: str = "stub") -> None:
        self._name = name
        self.received_contexts: list[DispatchContext | None] = []

    def get_name(self) -> str:
        return self._name

    def get_version(self) -> int:
        return 1

    def get_subscribed_event_types(self) -> set[str] | None:
        return None  # Subscribe to all

    async def handle_event(
        self,
        envelope: EventEnvelope[DomainEvent],
        checkpoint_store: ProjectionCheckpointStore,
        context: DispatchContext | None = None,
    ) -> ProjectionResult:
        self.received_contexts.append(context)
        # Save checkpoint to prevent "already processed" skips
        from datetime import UTC, datetime

        from event_sourcing.core.checkpoint import ProjectionCheckpoint

        await checkpoint_store.save_checkpoint(
            ProjectionCheckpoint(
                projection_name=self._name,
                global_position=envelope.metadata.global_nonce or 0,
                updated_at=datetime.now(UTC),
                version=self.get_version(),
            )
        )
        return ProjectionResult.SUCCESS

    async def clear_all_data(self) -> None:
        pass


class StubProcessManager(ProcessManager):
    """Concrete ProcessManager for coordinator gating tests."""

    def __init__(self, name: str = "stub_pm") -> None:
        self._name = name
        self.process_pending_calls: int = 0
        self.handle_event_calls: int = 0

    def get_name(self) -> str:
        return self._name

    def get_version(self) -> int:
        return 1

    def get_subscribed_event_types(self) -> set[str] | None:
        return None

    async def handle_event(
        self,
        envelope: EventEnvelope[DomainEvent],
        checkpoint_store: ProjectionCheckpointStore,
        context: DispatchContext | None = None,
    ) -> ProjectionResult:
        self.handle_event_calls += 1
        from datetime import UTC, datetime

        from event_sourcing.core.checkpoint import ProjectionCheckpoint

        await checkpoint_store.save_checkpoint(
            ProjectionCheckpoint(
                projection_name=self._name,
                global_position=envelope.metadata.global_nonce or 0,
                updated_at=datetime.now(UTC),
                version=self.get_version(),
            )
        )
        return ProjectionResult.SUCCESS

    async def process_pending(self) -> int:
        self.process_pending_calls += 1
        return 1

    def get_idempotency_key(self, todo_item: dict[str, object]) -> str:
        return str(todo_item.get("id", ""))

    async def clear_all_data(self) -> None:
        pass


def _make_envelope(global_nonce: int) -> EventEnvelope[DomainEvent]:
    """Create a test EventEnvelope."""
    return EventEnvelope(
        event=TestEvent(value="test"),
        metadata=EventMetadata(
            aggregate_nonce=1,
            aggregate_id="agg-1",
            aggregate_type="TestAggregate",
            global_nonce=global_nonce,
            event_type="TestEvent",
        ),
    )


def _make_coordinator(
    projections: list[object],
    live_boundary_nonce: int = 100,
    is_catching_up: bool = True,
) -> SubscriptionCoordinator:
    """Create a coordinator with pre-set catch-up state."""
    mock_store = AsyncMock()
    mock_checkpoint_store = MemoryCheckpointStore()

    coordinator = SubscriptionCoordinator(
        event_store=mock_store,
        checkpoint_store=mock_checkpoint_store,
        projections=projections,  # type: ignore[arg-type]
    )
    coordinator.live_boundary_nonce = live_boundary_nonce
    coordinator.is_catching_up = is_catching_up
    return coordinator


# ============================================================================
# Catch-Up Transition Tests
# ============================================================================


class TestCatchUpTransition:
    """Tests for catch-up to live transition logic."""

    @pytest.mark.asyncio
    async def test_stays_catching_up_below_boundary(self) -> None:
        """Should remain in catch-up mode for events at or below boundary."""
        proj = StubProjection("p1")
        coordinator = _make_coordinator([proj], live_boundary_nonce=100, is_catching_up=True)

        await coordinator.dispatch_event(_make_envelope(global_nonce=50))
        assert coordinator.is_catching_up is True

        await coordinator.dispatch_event(_make_envelope(global_nonce=100))
        assert coordinator.is_catching_up is True  # AT boundary, still catching up

    @pytest.mark.asyncio
    async def test_transitions_to_live_past_boundary(self) -> None:
        """Should transition to live when global_nonce > live_boundary_nonce."""
        proj = StubProjection("p1")
        coordinator = _make_coordinator([proj], live_boundary_nonce=100, is_catching_up=True)

        await coordinator.dispatch_event(_make_envelope(global_nonce=101))
        assert coordinator.is_catching_up is False

    @pytest.mark.asyncio
    async def test_transition_is_one_way(self) -> None:
        """Once live, should stay live even if global_nonce appears lower."""
        proj = StubProjection("p1")
        coordinator = _make_coordinator([proj], live_boundary_nonce=100, is_catching_up=False)

        # Already live, dispatch event with lower nonce - stays live
        await coordinator.dispatch_event(_make_envelope(global_nonce=50))
        assert coordinator.is_catching_up is False


# ============================================================================
# DispatchContext Passing Tests
# ============================================================================


class TestDispatchContextPassing:
    """Tests that coordinator passes DispatchContext to projections."""

    @pytest.mark.asyncio
    async def test_context_passed_during_catchup(self) -> None:
        """Should pass DispatchContext with is_catching_up=True during catch-up."""
        proj = StubProjection("p1")
        coordinator = _make_coordinator([proj], live_boundary_nonce=100, is_catching_up=True)

        await coordinator.dispatch_event(_make_envelope(global_nonce=50))

        assert len(proj.received_contexts) == 1
        ctx = proj.received_contexts[0]
        assert ctx is not None
        assert ctx.is_catching_up is True
        assert ctx.global_nonce == 50
        assert ctx.live_boundary_nonce == 100

    @pytest.mark.asyncio
    async def test_context_passed_during_live(self) -> None:
        """Should pass DispatchContext with is_catching_up=False for live events."""
        proj = StubProjection("p1")
        coordinator = _make_coordinator([proj], live_boundary_nonce=100, is_catching_up=False)

        await coordinator.dispatch_event(_make_envelope(global_nonce=101))

        assert len(proj.received_contexts) == 1
        ctx = proj.received_contexts[0]
        assert ctx is not None
        assert ctx.is_catching_up is False
        assert ctx.global_nonce == 101


# ============================================================================
# ProcessManager Gating Tests
# ============================================================================


class TestProcessManagerGating:
    """Tests that coordinator gates process_pending() on live mode."""

    @pytest.mark.asyncio
    async def test_no_process_pending_during_catchup(self) -> None:
        """process_pending() must NOT be called during catch-up replay."""
        pm = StubProcessManager("pm1")
        coordinator = _make_coordinator([pm], live_boundary_nonce=100, is_catching_up=True)

        await coordinator.dispatch_event(_make_envelope(global_nonce=50))

        assert pm.handle_event_calls == 1
        assert pm.process_pending_calls == 0  # Key invariant

    @pytest.mark.asyncio
    async def test_process_pending_called_for_live_events(self) -> None:
        """process_pending() should be called for live events on ProcessManager."""
        pm = StubProcessManager("pm1")
        coordinator = _make_coordinator([pm], live_boundary_nonce=100, is_catching_up=False)

        await coordinator.dispatch_event(_make_envelope(global_nonce=101))

        assert pm.handle_event_calls == 1
        assert pm.process_pending_calls == 1  # Should be called

    @pytest.mark.asyncio
    async def test_no_process_pending_on_regular_projection(self) -> None:
        """Regular projections should not have process_pending() called."""
        proj = StubProjection("p1")
        coordinator = _make_coordinator([proj], live_boundary_nonce=100, is_catching_up=False)

        await coordinator.dispatch_event(_make_envelope(global_nonce=101))

        # StubProjection doesn't have process_pending_calls attribute
        assert not hasattr(proj, "process_pending_calls")

    @pytest.mark.asyncio
    async def test_replay_then_live_sequence(self) -> None:
        """Full sequence: catch-up events then live. process_pending only on live."""
        pm = StubProcessManager("pm1")
        coordinator = _make_coordinator([pm], live_boundary_nonce=5, is_catching_up=True)

        # Catch-up events (1-5): no process_pending
        for nonce in range(1, 6):
            await coordinator.dispatch_event(_make_envelope(global_nonce=nonce))
        assert pm.handle_event_calls == 5
        assert pm.process_pending_calls == 0

        # Live events (6-8): process_pending called each time
        for nonce in range(6, 9):
            await coordinator.dispatch_event(_make_envelope(global_nonce=nonce))
        assert pm.handle_event_calls == 8
        assert pm.process_pending_calls == 3

    @pytest.mark.asyncio
    async def test_process_pending_exception_does_not_crash_coordinator(self) -> None:
        """If process_pending() raises, coordinator should log and continue."""
        pm = StubProcessManager("pm1")

        async def _exploding_process_pending() -> int:
            raise RuntimeError("Kaboom!")

        pm.process_pending = _exploding_process_pending  # type: ignore[method-assign]  # noqa: E501

        coordinator = _make_coordinator([pm], live_boundary_nonce=0, is_catching_up=False)

        # Should not raise - coordinator catches and logs the exception
        await coordinator.dispatch_event(_make_envelope(global_nonce=1))

        # handle_event was still called
        assert pm.handle_event_calls == 1
