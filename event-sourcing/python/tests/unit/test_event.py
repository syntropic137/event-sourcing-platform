"""Tests for event module."""


import pytest
from pydantic import ValidationError

from event_sourcing.core.event import (
    BaseDomainEvent,
    DomainEvent,
    EventEnvelope,
    EventFactory,
    EventMetadata,
)


class TestEvent(DomainEvent):
    """Test event for unit tests."""

    event_type = "TestEvent"
    value: str


class TestDomainEvent:
    """Tests for DomainEvent base class."""

    def test_event_is_immutable(self):
        """Events should be immutable."""
        event = TestEvent(value="test")

        with pytest.raises(ValidationError):
            event.value = "changed"  # type: ignore

    def test_event_serialization(self):
        """Events should serialize to JSON."""
        event = TestEvent(value="test")
        json_str = event.model_dump_json()

        assert "test" in json_str
        assert '"value"' in json_str

    def test_event_type_is_accessible(self):
        """Event type should be accessible as class variable."""
        event = TestEvent(value="test")
        assert event.event_type == "TestEvent"


class TestBaseDomainEvent:
    """Tests for BaseDomainEvent convenience class."""

    def test_get_event_type_from_class_var(self):
        """Should get event type from class variable."""

        class MyEvent(BaseDomainEvent):
            event_type = "MyCustomEvent"
            data: str

        assert MyEvent.get_event_type() == "MyCustomEvent"

    def test_get_event_type_defaults_to_class_name(self):
        """Should default to class name if no event_type set."""

        class AnotherEvent(BaseDomainEvent):
            data: str

        # This will still return "AnotherEvent" from parent class
        assert AnotherEvent.get_event_type() == "AnotherEvent"


class TestEventMetadata:
    """Tests for EventMetadata."""

    def test_metadata_creation_with_defaults(self):
        """Should create metadata with auto-generated fields."""
        metadata = EventMetadata(
            aggregate_id="test-123",
            aggregate_type="Test",
            aggregate_nonce=1,
        )

        assert metadata.aggregate_id == "test-123"
        assert metadata.aggregate_type == "Test"
        assert metadata.aggregate_nonce == 1
        assert metadata.event_id is not None
        assert metadata.timestamp is not None
        assert metadata.content_type == "application/json"

    def test_metadata_is_immutable(self):
        """Metadata should be immutable."""
        metadata = EventMetadata(
            aggregate_id="test-123",
            aggregate_type="Test",
            aggregate_nonce=1,
        )

        with pytest.raises(ValidationError):
            metadata.aggregate_id = "changed"  # type: ignore

    def test_metadata_with_custom_fields(self):
        """Should support custom metadata fields."""
        metadata = EventMetadata(
            aggregate_id="test-123",
            aggregate_type="Test",
            aggregate_nonce=1,
            tenant_id="tenant-1",
            correlation_id="corr-123",
            causation_id="cause-456",
            actor_id="user-789",
            headers={"trace-id": "trace-123"},
            custom_metadata={"key": "value"},
        )

        assert metadata.tenant_id == "tenant-1"
        assert metadata.correlation_id == "corr-123"
        assert metadata.causation_id == "cause-456"
        assert metadata.actor_id == "user-789"
        assert metadata.headers["trace-id"] == "trace-123"
        assert metadata.custom_metadata["key"] == "value"


class TestEventEnvelope:
    """Tests for EventEnvelope."""

    def test_envelope_creation(self):
        """Should create envelope with event and metadata."""
        event = TestEvent(value="test")
        metadata = EventMetadata(
            aggregate_id="test-123",
            aggregate_type="Test",
            aggregate_nonce=1,
        )

        envelope = EventEnvelope(event=event, metadata=metadata)

        assert envelope.event == event
        assert envelope.metadata == metadata

    def test_envelope_is_immutable(self):
        """Envelope should be immutable."""
        event = TestEvent(value="test")
        metadata = EventMetadata(
            aggregate_id="test-123",
            aggregate_type="Test",
            aggregate_nonce=1,
        )
        envelope = EventEnvelope(event=event, metadata=metadata)

        with pytest.raises(ValidationError):
            envelope.event = TestEvent(value="changed")  # type: ignore


class TestEventFactory:
    """Tests for EventFactory."""

    def test_create_envelope(self):
        """Should create complete envelope with generated metadata."""
        event = TestEvent(value="test")

        envelope = EventFactory.create(
            event=event,
            aggregate_id="test-123",
            aggregate_type="Test",
            aggregate_nonce=1,
        )

        assert envelope.event == event
        assert envelope.metadata.aggregate_id == "test-123"
        assert envelope.metadata.aggregate_type == "Test"
        assert envelope.metadata.aggregate_nonce == 1
        assert envelope.metadata.event_id is not None

    def test_create_with_optional_metadata(self):
        """Should create envelope with optional metadata fields."""
        event = TestEvent(value="test")

        envelope = EventFactory.create(
            event=event,
            aggregate_id="test-123",
            aggregate_type="Test",
            aggregate_nonce=1,
            tenant_id="tenant-1",
            correlation_id="corr-123",
            actor_id="user-789",
        )

        assert envelope.metadata.tenant_id == "tenant-1"
        assert envelope.metadata.correlation_id == "corr-123"
        assert envelope.metadata.actor_id == "user-789"

