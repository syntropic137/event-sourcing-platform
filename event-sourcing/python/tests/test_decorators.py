"""Tests for event sourcing decorators."""

import pytest

from event_sourcing import (
    DomainEvent,
    aggregate,
    command,
    event,
    get_command_metadata,
    get_event_metadata,
)
from event_sourcing.decorators.commands import CommandDecoratorMetadata
from event_sourcing.decorators.events import EventDecoratorMetadata

# ============================================================================
# @event decorator tests
# ============================================================================


class TestEventDecorator:
    """Tests for the @event class decorator."""

    def test_event_decorator_simple_version(self) -> None:
        """@event should accept simple version format (v1, v2, etc.)."""

        @event("TaskCreated", "v1")
        class TaskCreatedEvent(DomainEvent):
            event_type = "TaskCreated"
            task_id: str

        metadata = get_event_metadata(TaskCreatedEvent)
        assert metadata is not None
        assert metadata.event_type == "TaskCreated"
        assert metadata.version == "v1"

    def test_event_decorator_semver_version(self) -> None:
        """@event should accept semantic version format (1.0.0, etc.)."""

        @event("TaskUpdated", "2.1.0")
        class TaskUpdatedEvent(DomainEvent):
            event_type = "TaskUpdated"
            task_id: str

        metadata = get_event_metadata(TaskUpdatedEvent)
        assert metadata is not None
        assert metadata.event_type == "TaskUpdated"
        assert metadata.version == "2.1.0"

    def test_event_decorator_invalid_version_raises(self) -> None:
        """@event should raise ValueError for invalid version format."""
        with pytest.raises(ValueError, match="Invalid event version format"):

            @event("TaskCreated", "invalid")
            class BadEvent(DomainEvent):
                event_type = "TaskCreated"

    def test_event_decorator_invalid_version_formats(self) -> None:
        """@event should reject various invalid version formats."""
        invalid_versions = [
            "1",  # Just number
            "v",  # Just v
            "1.0",  # Incomplete semver
            "v1.0",  # Mixed format
            "version1",  # Word format
            "",  # Empty
        ]

        for version in invalid_versions:
            with pytest.raises(ValueError, match="Invalid event version format"):

                @event("TestEvent", version)
                class TestEvent(DomainEvent):
                    event_type = "TestEvent"

    def test_event_decorator_sets_event_type_if_missing(self) -> None:
        """@event should set event_type class attribute if not present."""

        @event("AutoTypedEvent", "v1")
        class AutoTypedEvent(DomainEvent):
            pass

        # The decorator should have set event_type
        assert hasattr(AutoTypedEvent, "event_type")
        assert AutoTypedEvent.event_type == "AutoTypedEvent"

    def test_get_event_metadata_returns_none_for_undecorated(self) -> None:
        """get_event_metadata should return None for undecorated classes."""

        class PlainEvent(DomainEvent):
            event_type = "PlainEvent"

        assert get_event_metadata(PlainEvent) is None

    def test_event_metadata_type(self) -> None:
        """Metadata should be EventDecoratorMetadata instance."""

        @event("TypedEvent", "v1")
        class TypedEvent(DomainEvent):
            event_type = "TypedEvent"

        metadata = get_event_metadata(TypedEvent)
        assert isinstance(metadata, EventDecoratorMetadata)


# ============================================================================
# @command decorator tests
# ============================================================================


class TestCommandDecorator:
    """Tests for the @command class decorator."""

    def test_command_decorator_basic(self) -> None:
        """@command should store command type metadata."""

        @command("CreateTask")
        class CreateTaskCommand:
            aggregate_id: str
            title: str

        metadata = get_command_metadata(CreateTaskCommand)
        assert metadata is not None
        assert metadata.command_type == "CreateTask"
        assert metadata.description is None

    def test_command_decorator_with_description(self) -> None:
        """@command should store description when provided."""

        @command("UpdateTask", "Updates an existing task")
        class UpdateTaskCommand:
            aggregate_id: str
            title: str

        metadata = get_command_metadata(UpdateTaskCommand)
        assert metadata is not None
        assert metadata.command_type == "UpdateTask"
        assert metadata.description == "Updates an existing task"

    def test_get_command_metadata_returns_none_for_undecorated(self) -> None:
        """get_command_metadata should return None for undecorated classes."""

        class PlainCommand:
            aggregate_id: str

        assert get_command_metadata(PlainCommand) is None

    def test_command_metadata_type(self) -> None:
        """Metadata should be CommandDecoratorMetadata instance."""

        @command("TypedCommand")
        class TypedCommand:
            aggregate_id: str

        metadata = get_command_metadata(TypedCommand)
        assert isinstance(metadata, CommandDecoratorMetadata)


# ============================================================================
# @aggregate decorator tests
# ============================================================================


class TestAggregateDecorator:
    """Tests for the @aggregate class decorator."""

    def test_aggregate_decorator_with_type(self) -> None:
        """@aggregate should set aggregate type."""

        @aggregate("Order")
        class OrderAggregate:
            pass

        assert OrderAggregate._aggregate_type == "Order"

    def test_aggregate_decorator_defaults_to_class_name(self) -> None:
        """@aggregate should default to class name if type not provided."""

        @aggregate()
        class MyAggregate:
            pass

        assert MyAggregate._aggregate_type == "MyAggregate"
