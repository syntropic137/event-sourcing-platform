"""Tests for the projection checkpoint module (ADR-014)."""

from datetime import UTC, datetime
from typing import Any

import pytest

from event_sourcing.core.checkpoint import (
    AutoDispatchProjection,
    CheckpointedProjection,
    ProjectionCheckpoint,
    ProjectionCheckpointStore,
    ProjectionResult,
    _snake_to_camel,
)
from event_sourcing.core.event import DomainEvent, EventEnvelope, EventMetadata

# ============================================================================
# Test Events
# ============================================================================


class TestEvent(DomainEvent):
    """Test event for unit tests."""

    event_type = "TestEvent"
    value: str


class AnotherEvent(DomainEvent):
    """Another test event."""

    event_type = "AnotherEvent"
    count: int


# ============================================================================
# ProjectionResult Tests
# ============================================================================


class TestProjectionResult:
    """Tests for ProjectionResult enum."""

    def test_result_values(self) -> None:
        """Should have correct string values."""
        assert ProjectionResult.SUCCESS.value == "success"
        assert ProjectionResult.SKIP.value == "skip"
        assert ProjectionResult.FAILURE.value == "failure"

    def test_result_members(self) -> None:
        """Should have exactly 3 members."""
        assert len(ProjectionResult) == 3
        assert ProjectionResult.SUCCESS in ProjectionResult
        assert ProjectionResult.SKIP in ProjectionResult
        assert ProjectionResult.FAILURE in ProjectionResult


# ============================================================================
# ProjectionCheckpoint Tests
# ============================================================================


class TestProjectionCheckpoint:
    """Tests for ProjectionCheckpoint dataclass."""

    def test_create_checkpoint(self) -> None:
        """Should create checkpoint with all fields."""
        now = datetime.now(UTC)
        checkpoint = ProjectionCheckpoint(
            projection_name="test_projection",
            global_position=42,
            updated_at=now,
            version=1,
        )

        assert checkpoint.projection_name == "test_projection"
        assert checkpoint.global_position == 42
        assert checkpoint.updated_at == now
        assert checkpoint.version == 1

    def test_checkpoint_immutable(self) -> None:
        """Checkpoint should be frozen (immutable)."""
        checkpoint = ProjectionCheckpoint(
            projection_name="test",
            global_position=0,
            updated_at=datetime.now(UTC),
        )

        with pytest.raises(AttributeError):
            checkpoint.global_position = 100  # type: ignore

    def test_checkpoint_default_version(self) -> None:
        """Version should default to 1."""
        checkpoint = ProjectionCheckpoint(
            projection_name="test",
            global_position=0,
            updated_at=datetime.now(UTC),
        )

        assert checkpoint.version == 1

    def test_checkpoint_empty_name_fails(self) -> None:
        """Empty projection name should fail validation."""
        with pytest.raises(ValueError) as exc:
            ProjectionCheckpoint(
                projection_name="",
                global_position=0,
                updated_at=datetime.now(UTC),
            )

        assert "projection_name cannot be empty" in str(exc.value)

    def test_checkpoint_negative_position_fails(self) -> None:
        """Negative position should fail validation."""
        with pytest.raises(ValueError) as exc:
            ProjectionCheckpoint(
                projection_name="test",
                global_position=-1,
                updated_at=datetime.now(UTC),
            )

        assert "global_position cannot be negative" in str(exc.value)

    def test_advance_to(self) -> None:
        """Should create new checkpoint with advanced position."""
        original = ProjectionCheckpoint(
            projection_name="test",
            global_position=10,
            updated_at=datetime.now(UTC),
            version=2,
        )

        advanced = original.advance_to(20)

        assert advanced.projection_name == "test"
        assert advanced.global_position == 20
        assert advanced.version == 2
        assert advanced.updated_at > original.updated_at

    def test_advance_to_same_position(self) -> None:
        """Advancing to same position should work."""
        original = ProjectionCheckpoint(
            projection_name="test",
            global_position=10,
            updated_at=datetime.now(UTC),
        )

        advanced = original.advance_to(10)

        assert advanced.global_position == 10

    def test_advance_backwards_fails(self) -> None:
        """Advancing backwards should fail."""
        checkpoint = ProjectionCheckpoint(
            projection_name="test",
            global_position=50,
            updated_at=datetime.now(UTC),
        )

        with pytest.raises(ValueError) as exc:
            checkpoint.advance_to(40)

        assert "Cannot advance checkpoint backwards" in str(exc.value)

    def test_initial_checkpoint(self) -> None:
        """Should create initial checkpoint at position 0."""
        checkpoint = ProjectionCheckpoint.initial("my_projection", version=3)

        assert checkpoint.projection_name == "my_projection"
        assert checkpoint.global_position == 0
        assert checkpoint.version == 3

    def test_initial_checkpoint_default_version(self) -> None:
        """Initial checkpoint should default to version 1."""
        checkpoint = ProjectionCheckpoint.initial("test")

        assert checkpoint.version == 1


# ============================================================================
# ProjectionCheckpointStore Protocol Tests
# ============================================================================


class InMemoryCheckpointStore:
    """In-memory implementation for testing (test-only)."""

    def __init__(self) -> None:
        self._checkpoints: dict[str, ProjectionCheckpoint] = {}

    async def get_checkpoint(self, projection_name: str) -> ProjectionCheckpoint | None:
        return self._checkpoints.get(projection_name)

    async def save_checkpoint(self, checkpoint: ProjectionCheckpoint) -> None:
        self._checkpoints[checkpoint.projection_name] = checkpoint

    async def delete_checkpoint(self, projection_name: str) -> None:
        self._checkpoints.pop(projection_name, None)

    async def get_all_checkpoints(self) -> list[ProjectionCheckpoint]:
        return list(self._checkpoints.values())


class TestProjectionCheckpointStore:
    """Tests for ProjectionCheckpointStore protocol."""

    def test_protocol_runtime_checkable(self) -> None:
        """Store should be runtime checkable."""
        store = InMemoryCheckpointStore()

        assert isinstance(store, ProjectionCheckpointStore)

    @pytest.mark.asyncio
    async def test_save_and_get_checkpoint(self) -> None:
        """Should save and retrieve checkpoint."""
        store = InMemoryCheckpointStore()
        checkpoint = ProjectionCheckpoint(
            projection_name="test",
            global_position=100,
            updated_at=datetime.now(UTC),
        )

        await store.save_checkpoint(checkpoint)
        retrieved = await store.get_checkpoint("test")

        assert retrieved == checkpoint

    @pytest.mark.asyncio
    async def test_get_nonexistent_checkpoint(self) -> None:
        """Should return None for nonexistent checkpoint."""
        store = InMemoryCheckpointStore()

        result = await store.get_checkpoint("nonexistent")

        assert result is None

    @pytest.mark.asyncio
    async def test_delete_checkpoint(self) -> None:
        """Should delete checkpoint."""
        store = InMemoryCheckpointStore()
        checkpoint = ProjectionCheckpoint(
            projection_name="test",
            global_position=50,
            updated_at=datetime.now(UTC),
        )

        await store.save_checkpoint(checkpoint)
        await store.delete_checkpoint("test")
        result = await store.get_checkpoint("test")

        assert result is None

    @pytest.mark.asyncio
    async def test_get_all_checkpoints(self) -> None:
        """Should return all checkpoints."""
        store = InMemoryCheckpointStore()
        c1 = ProjectionCheckpoint(
            projection_name="proj1",
            global_position=10,
            updated_at=datetime.now(UTC),
        )
        c2 = ProjectionCheckpoint(
            projection_name="proj2",
            global_position=20,
            updated_at=datetime.now(UTC),
        )

        await store.save_checkpoint(c1)
        await store.save_checkpoint(c2)
        all_checkpoints = await store.get_all_checkpoints()

        assert len(all_checkpoints) == 2
        assert c1 in all_checkpoints
        assert c2 in all_checkpoints


# ============================================================================
# CheckpointedProjection Tests
# ============================================================================


class TestProjection(CheckpointedProjection):
    """Concrete test projection implementation."""

    def __init__(self, name: str = "test_projection") -> None:
        self._name = name
        self.processed_events: list[EventEnvelope[Any]] = []

    def get_name(self) -> str:
        return self._name

    def get_version(self) -> int:
        return 1

    def get_subscribed_event_types(self) -> set[str]:
        return {"TestEvent", "AnotherEvent"}

    async def handle_event(
        self,
        envelope: EventEnvelope[Any],
        checkpoint_store: ProjectionCheckpointStore,
    ) -> ProjectionResult:
        self.processed_events.append(envelope)
        await checkpoint_store.save_checkpoint(
            ProjectionCheckpoint(
                projection_name=self.get_name(),
                global_position=envelope.metadata.global_nonce or 0,
                updated_at=datetime.now(UTC),
                version=self.get_version(),
            )
        )
        return ProjectionResult.SUCCESS


class AllEventsProjection(CheckpointedProjection):
    """Projection that subscribes to all events."""

    def get_name(self) -> str:
        return "all_events"

    def get_version(self) -> int:
        return 1

    def get_subscribed_event_types(self) -> set[str] | None:
        return None  # Subscribe to all

    async def handle_event(
        self,
        envelope: EventEnvelope[Any],
        checkpoint_store: ProjectionCheckpointStore,
    ) -> ProjectionResult:
        return ProjectionResult.SUCCESS


def _make_envelope(
    event: DomainEvent,
    global_nonce: int = 0,
) -> EventEnvelope[Any]:
    """Helper to create test event envelopes."""
    return EventEnvelope(
        event=event,
        metadata=EventMetadata(
            aggregate_id="agg-123",
            aggregate_type="TestAggregate",
            aggregate_nonce=1,
            global_nonce=global_nonce,
            event_type=event.event_type,
        ),
    )


class TestCheckpointedProjection:
    """Tests for CheckpointedProjection base class."""

    def test_get_name(self) -> None:
        """Should return projection name."""
        proj = TestProjection("my_projection")

        assert proj.get_name() == "my_projection"

    def test_get_version(self) -> None:
        """Should return version."""
        proj = TestProjection()

        assert proj.get_version() == 1

    def test_get_subscribed_event_types(self) -> None:
        """Should return subscribed event types."""
        proj = TestProjection()

        assert proj.get_subscribed_event_types() == {"TestEvent", "AnotherEvent"}

    def test_get_subscribed_event_types_none(self) -> None:
        """None means subscribe to all events."""
        proj = AllEventsProjection()

        assert proj.get_subscribed_event_types() is None

    @pytest.mark.asyncio
    async def test_handle_event(self) -> None:
        """Should process event and save checkpoint."""
        proj = TestProjection()
        store = InMemoryCheckpointStore()
        envelope = _make_envelope(TestEvent(value="test"), global_nonce=42)

        result = await proj.handle_event(envelope, store)

        assert result == ProjectionResult.SUCCESS
        assert len(proj.processed_events) == 1
        assert proj.processed_events[0] == envelope

        checkpoint = await store.get_checkpoint("test_projection")
        assert checkpoint is not None
        assert checkpoint.global_position == 42

    def test_should_handle_event_type(self) -> None:
        """Should check if event type is subscribed."""
        proj = TestProjection()

        assert proj.should_handle_event_type("TestEvent") is True
        assert proj.should_handle_event_type("AnotherEvent") is True
        assert proj.should_handle_event_type("UnknownEvent") is False

    def test_should_handle_event_type_all_events(self) -> None:
        """None subscription means handle all events."""
        proj = AllEventsProjection()

        assert proj.should_handle_event_type("AnyEvent") is True
        assert proj.should_handle_event_type("Whatever") is True

    @pytest.mark.asyncio
    async def test_clear_all_data_default(self) -> None:
        """Default clear_all_data should be a no-op."""
        proj = TestProjection()

        # Should not raise
        await proj.clear_all_data()


# ============================================================================
# Integration-style Tests
# ============================================================================


class TestProjectionWorkflow:
    """Tests for complete projection workflows."""

    @pytest.mark.asyncio
    async def test_process_multiple_events(self) -> None:
        """Should process multiple events in sequence."""
        proj = TestProjection()
        store = InMemoryCheckpointStore()

        events = [
            _make_envelope(TestEvent(value="first"), global_nonce=1),
            _make_envelope(AnotherEvent(count=5), global_nonce=2),
            _make_envelope(TestEvent(value="third"), global_nonce=3),
        ]

        for envelope in events:
            result = await proj.handle_event(envelope, store)
            assert result == ProjectionResult.SUCCESS

        assert len(proj.processed_events) == 3

        checkpoint = await store.get_checkpoint("test_projection")
        assert checkpoint is not None
        assert checkpoint.global_position == 3

    @pytest.mark.asyncio
    async def test_rebuild_workflow(self) -> None:
        """Should support rebuild by deleting checkpoint."""
        proj = TestProjection()
        store = InMemoryCheckpointStore()

        # Process some events
        envelope = _make_envelope(TestEvent(value="test"), global_nonce=100)
        await proj.handle_event(envelope, store)

        # Verify checkpoint exists
        checkpoint = await store.get_checkpoint("test_projection")
        assert checkpoint is not None
        assert checkpoint.global_position == 100

        # Simulate rebuild: delete checkpoint and data
        await store.delete_checkpoint("test_projection")
        await proj.clear_all_data()
        proj.processed_events.clear()

        # Verify checkpoint is gone
        checkpoint = await store.get_checkpoint("test_projection")
        assert checkpoint is None

        # Re-process from start
        envelope = _make_envelope(TestEvent(value="replayed"), global_nonce=1)
        await proj.handle_event(envelope, store)

        checkpoint = await store.get_checkpoint("test_projection")
        assert checkpoint is not None
        assert checkpoint.global_position == 1


# ============================================================================
# _snake_to_camel Tests
# ============================================================================


class TestSnakeToCamel:
    """Tests for the _snake_to_camel helper."""

    def test_single_word(self) -> None:
        assert _snake_to_camel("started") == "Started"

    def test_multi_word(self) -> None:
        assert _snake_to_camel("workflow_execution_started") == "WorkflowExecutionStarted"

    def test_two_words(self) -> None:
        assert _snake_to_camel("phase_started") == "PhaseStarted"


# ============================================================================
# AutoDispatchProjection Tests
# ============================================================================


class SampleAutoProjection(AutoDispatchProjection):
    """Concrete AutoDispatchProjection for testing."""

    def __init__(self) -> None:
        self.handled_events: list[dict[str, Any]] = []
        self.cleared = False

    def get_name(self) -> str:
        return "sample_auto"

    def get_version(self) -> int:
        return 1

    async def on_test_event(self, data: dict[str, Any]) -> None:
        self.handled_events.append(data)

    async def on_another_event(self, data: dict[str, Any]) -> None:
        self.handled_events.append(data)

    async def clear_all_data(self) -> None:
        self.cleared = True
        self.handled_events.clear()


class FailingAutoProjection(AutoDispatchProjection):
    """AutoDispatchProjection that raises on every event."""

    def get_name(self) -> str:
        return "failing_auto"

    def get_version(self) -> int:
        return 1

    async def on_test_event(self, data: dict[str, Any]) -> None:
        raise RuntimeError("handler exploded")


class TestAutoDispatchProjection:
    """Tests for AutoDispatchProjection base class."""

    def test_discover_handlers(self) -> None:
        """Should discover on_* methods and map to CamelCase event types."""
        handlers = SampleAutoProjection._discover_handlers()

        assert "TestEvent" in handlers
        assert handlers["TestEvent"] == "on_test_event"
        assert "AnotherEvent" in handlers
        assert handlers["AnotherEvent"] == "on_another_event"

    def test_get_subscribed_event_types(self) -> None:
        """Should derive subscribed types from on_* methods."""
        proj = SampleAutoProjection()
        types = proj.get_subscribed_event_types()

        assert types == {"TestEvent", "AnotherEvent"}

    @pytest.mark.asyncio
    async def test_handle_event_dispatches(self) -> None:
        """Should dispatch to the correct on_* handler."""
        proj = SampleAutoProjection()
        store = InMemoryCheckpointStore()
        envelope = _make_envelope(TestEvent(value="hello"), global_nonce=10)

        result = await proj.handle_event(envelope, store)

        assert result == ProjectionResult.SUCCESS
        assert len(proj.handled_events) == 1
        assert proj.handled_events[0]["value"] == "hello"

    @pytest.mark.asyncio
    async def test_handle_event_saves_checkpoint(self) -> None:
        """Should save checkpoint after successful dispatch."""
        proj = SampleAutoProjection()
        store = InMemoryCheckpointStore()
        envelope = _make_envelope(TestEvent(value="test"), global_nonce=42)

        await proj.handle_event(envelope, store)

        checkpoint = await store.get_checkpoint("sample_auto")
        assert checkpoint is not None
        assert checkpoint.global_position == 42
        assert checkpoint.version == 1

    @pytest.mark.asyncio
    async def test_handle_event_returns_failure_on_error(self) -> None:
        """Should return FAILURE and log when handler raises."""
        proj = FailingAutoProjection()
        store = InMemoryCheckpointStore()
        envelope = _make_envelope(TestEvent(value="boom"), global_nonce=5)

        result = await proj.handle_event(envelope, store)

        assert result == ProjectionResult.FAILURE
        # Checkpoint should NOT be saved on failure
        checkpoint = await store.get_checkpoint("failing_auto")
        assert checkpoint is None

    def test_discover_handlers_cached(self) -> None:
        """Handler discovery should be cached (same dict object on repeated calls)."""
        h1 = SampleAutoProjection._discover_handlers()
        h2 = SampleAutoProjection._discover_handlers()

        assert h1 is h2


# ============================================================================
# SubscriptionCoordinator _get_minimum_position Tests
# ============================================================================


class TestCoordinatorGetMinimumPosition:
    """Tests for SubscriptionCoordinator._get_minimum_position edge cases."""

    @pytest.mark.asyncio
    async def test_all_version_mismatched_projections_get_cleared(self) -> None:
        """All version-mismatched projections should get clear_all_data() called."""
        from event_sourcing.subscriptions.coordinator import SubscriptionCoordinator

        class VersionedProjection(TestProjection):
            def __init__(self, name: str, version: int) -> None:
                super().__init__(name)
                self._version = version
                self.cleared = False

            def get_version(self) -> int:
                return self._version

            async def clear_all_data(self) -> None:
                self.cleared = True

        proj_a = VersionedProjection("proj_a", version=2)
        proj_b = VersionedProjection("proj_b", version=3)

        store = InMemoryCheckpointStore()
        # Stored checkpoints have old versions (1)
        await store.save_checkpoint(
            ProjectionCheckpoint(
                projection_name="proj_a",
                global_position=50,
                updated_at=datetime.now(UTC),
                version=1,
            )
        )
        await store.save_checkpoint(
            ProjectionCheckpoint(
                projection_name="proj_b",
                global_position=100,
                updated_at=datetime.now(UTC),
                version=1,
            )
        )

        # Use a dummy event store — we only test _get_minimum_position
        coordinator = SubscriptionCoordinator(
            event_store=None,  # type: ignore[arg-type]
            checkpoint_store=store,
            projections=[proj_a, proj_b],
        )

        min_pos = await coordinator._get_minimum_position()

        assert min_pos == 0
        assert proj_a.cleared is True
        assert proj_b.cleared is True

        # Checkpoints should be deleted
        assert await store.get_checkpoint("proj_a") is None
        assert await store.get_checkpoint("proj_b") is None
