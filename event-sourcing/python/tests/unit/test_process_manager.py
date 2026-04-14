"""Tests for ProcessManager base class and coordinator gating."""

from __future__ import annotations

import pytest

from event_sourcing.core.checkpoint import (
    CheckpointedProjection,
    DispatchContext,
    ProjectionCheckpointStore,
    ProjectionResult,
)
from event_sourcing.core.event import DomainEvent, EventEnvelope, EventMetadata
from event_sourcing.core.process_manager import ProcessManager


# ============================================================================
# Test Events
# ============================================================================


class TaskCreatedEvent(DomainEvent):
    """Test event."""

    event_type = "TaskCreatedEvent"
    task_id: str


# ============================================================================
# Test ProcessManager Implementation
# ============================================================================


class TodoProcessManager(ProcessManager):
    """Concrete ProcessManager for testing.

    Projection side: writes todo items to an in-memory list.
    Processor side: marks items as done and tracks call count.
    """

    def __init__(self) -> None:
        self.pending_items: list[dict[str, object]] = []
        self.done_items: list[dict[str, object]] = []
        self.process_pending_calls: int = 0
        self.handle_event_calls: int = 0

    def get_name(self) -> str:
        return "todo_pm"

    def get_version(self) -> int:
        return 1

    def get_subscribed_event_types(self) -> set[str]:
        return {"TaskCreatedEvent"}

    async def handle_event(
        self,
        envelope: EventEnvelope[DomainEvent],
        checkpoint_store: ProjectionCheckpointStore,
        context: DispatchContext | None = None,
    ) -> ProjectionResult:
        self.handle_event_calls += 1
        event = envelope.event
        if isinstance(event, TaskCreatedEvent):
            self.pending_items.append({"task_id": event.task_id, "status": "pending"})
        return ProjectionResult.SUCCESS

    async def process_pending(self) -> int:
        self.process_pending_calls += 1
        processed = 0
        for item in list(self.pending_items):
            if item["status"] == "pending":
                item["status"] = "done"
                self.done_items.append(item)
                processed += 1
        self.pending_items = [i for i in self.pending_items if i["status"] == "pending"]
        return processed

    def get_idempotency_key(self, todo_item: dict[str, object]) -> str:
        return str(todo_item["task_id"])


def _make_envelope(
    event: DomainEvent,
    global_nonce: int,
    event_type: str = "TaskCreatedEvent",
) -> EventEnvelope[DomainEvent]:
    """Create a test EventEnvelope."""
    return EventEnvelope(
        event=event,
        metadata=EventMetadata(
            aggregate_nonce=1,
            aggregate_id="test-agg-1",
            aggregate_type="TestAggregate",
            global_nonce=global_nonce,
            event_type=event_type,
        ),
    )


# ============================================================================
# ProcessManager Interface Tests
# ============================================================================


class TestProcessManagerInterface:
    """Tests for ProcessManager base class behavior."""

    def test_is_subclass_of_checkpointed_projection(self) -> None:
        """ProcessManager must extend CheckpointedProjection."""
        assert issubclass(ProcessManager, CheckpointedProjection)

    def test_side_effects_allowed_is_true(self) -> None:
        """ProcessManager.SIDE_EFFECTS_ALLOWED must be True."""
        assert ProcessManager.SIDE_EFFECTS_ALLOWED is True

    def test_concrete_subclass_instantiation(self) -> None:
        """Should instantiate a concrete ProcessManager subclass."""
        pm = TodoProcessManager()
        assert pm.get_name() == "todo_pm"
        assert pm.SIDE_EFFECTS_ALLOWED is True

    def test_abstract_methods_enforced(self) -> None:
        """Cannot instantiate ProcessManager without implementing all methods."""

        class IncompleteManager(ProcessManager):
            def get_name(self) -> str:
                return "incomplete"

            def get_version(self) -> int:
                return 1

            def get_subscribed_event_types(self) -> set[str]:
                return set()

            # Missing: handle_event, process_pending, get_idempotency_key

        with pytest.raises(TypeError, match="abstract"):
            IncompleteManager()  # type: ignore[abstract]


# ============================================================================
# ProcessManager Projection Side Tests
# ============================================================================


class TestProcessManagerProjectionSide:
    """Tests for the projection side of ProcessManager (handle_event)."""

    @pytest.mark.asyncio
    async def test_handle_event_writes_todo(self) -> None:
        """handle_event() should write to-do records."""
        pm = TodoProcessManager()
        event = TaskCreatedEvent(task_id="t1")
        envelope = _make_envelope(event, global_nonce=1)

        from event_sourcing.stores.memory_checkpoint import MemoryCheckpointStore

        store = MemoryCheckpointStore()
        result = await pm.handle_event(envelope, store)

        assert result == ProjectionResult.SUCCESS
        assert len(pm.pending_items) == 1
        assert pm.pending_items[0]["task_id"] == "t1"
        assert pm.pending_items[0]["status"] == "pending"

    @pytest.mark.asyncio
    async def test_handle_event_with_context(self) -> None:
        """handle_event() should accept optional DispatchContext."""
        pm = TodoProcessManager()
        event = TaskCreatedEvent(task_id="t2")
        envelope = _make_envelope(event, global_nonce=5)

        from event_sourcing.stores.memory_checkpoint import MemoryCheckpointStore

        store = MemoryCheckpointStore()
        ctx = DispatchContext(is_catching_up=True, global_nonce=5, live_boundary_nonce=100)
        result = await pm.handle_event(envelope, store, ctx)

        assert result == ProjectionResult.SUCCESS
        assert pm.handle_event_calls == 1

    @pytest.mark.asyncio
    async def test_handle_event_without_context(self) -> None:
        """handle_event() should work without context (backwards-compatible)."""
        pm = TodoProcessManager()
        event = TaskCreatedEvent(task_id="t3")
        envelope = _make_envelope(event, global_nonce=1)

        from event_sourcing.stores.memory_checkpoint import MemoryCheckpointStore

        store = MemoryCheckpointStore()
        result = await pm.handle_event(envelope, store)

        assert result == ProjectionResult.SUCCESS


# ============================================================================
# ProcessManager Processor Side Tests
# ============================================================================


class TestProcessManagerProcessorSide:
    """Tests for the processor side of ProcessManager (process_pending)."""

    @pytest.mark.asyncio
    async def test_process_pending_executes_items(self) -> None:
        """process_pending() should execute pending items."""
        pm = TodoProcessManager()
        pm.pending_items = [
            {"task_id": "t1", "status": "pending"},
            {"task_id": "t2", "status": "pending"},
        ]

        processed = await pm.process_pending()

        assert processed == 2
        assert len(pm.pending_items) == 0
        assert len(pm.done_items) == 2

    @pytest.mark.asyncio
    async def test_process_pending_is_idempotent(self) -> None:
        """process_pending() called twice should process items only once."""
        pm = TodoProcessManager()
        pm.pending_items = [{"task_id": "t1", "status": "pending"}]

        first = await pm.process_pending()
        second = await pm.process_pending()

        assert first == 1
        assert second == 0  # Nothing left to process

    @pytest.mark.asyncio
    async def test_process_pending_empty(self) -> None:
        """process_pending() with no pending items returns 0."""
        pm = TodoProcessManager()
        assert await pm.process_pending() == 0

    def test_idempotency_key(self) -> None:
        """get_idempotency_key() should return stable dedup key."""
        pm = TodoProcessManager()
        key = pm.get_idempotency_key({"task_id": "t1"})
        assert key == "t1"
        # Same input, same key
        assert pm.get_idempotency_key({"task_id": "t1"}) == key
