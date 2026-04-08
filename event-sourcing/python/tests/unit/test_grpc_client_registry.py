"""Tests for GrpcEventStoreClient event type registry resolution (ADR-023).

Verifies that _proto_to_envelope() resolves concrete event types from the
registry when available, and falls back to GenericDomainEvent with event_type
preserved when the type is unknown.
"""

import json
from unittest.mock import MagicMock

import pytest

from event_sourcing import DomainEvent, event
from event_sourcing.client.grpc_client import GrpcEventStoreClient
from event_sourcing.core.event import GenericDomainEvent

# --- Test event classes (auto-registered via @event) ---


@event("TestOrderPlaced", "v1")
class TestOrderPlacedEvent(DomainEvent):
    """Concrete event for testing registry resolution."""

    event_type = "TestOrderPlaced"
    order_id: str
    amount: float


@event("TestItemAdded", "v1")
class TestItemAddedEvent(DomainEvent):
    """Another concrete event for testing."""

    event_type = "TestItemAdded"
    item_id: str
    quantity: int


# --- Helpers ---


def _make_proto_event_data(
    event_type: str,
    payload: dict,
    aggregate_id: str = "test-123",
    aggregate_type: str = "TestAggregate",
    aggregate_nonce: int = 1,
) -> MagicMock:
    """Create a mock protobuf EventData with the given event type and payload."""
    meta = MagicMock()
    meta.event_id = "evt-001"
    meta.aggregate_id = aggregate_id
    meta.aggregate_type = aggregate_type
    meta.aggregate_nonce = aggregate_nonce
    meta.correlation_id = ""
    meta.causation_id = ""
    meta.actor_id = ""
    meta.global_nonce = 1
    meta.event_type = event_type

    event_data = MagicMock()
    event_data.meta = meta
    event_data.payload = json.dumps(payload).encode("utf-8")

    return event_data


# --- Tests ---


class TestProtoToEnvelopeRegistryResolution:
    """Tests for _proto_to_envelope() concrete type resolution (ADR-023)."""

    def setup_method(self) -> None:
        self.client = GrpcEventStoreClient(address="localhost:50051")

    def test_resolves_registered_event_to_concrete_type(self) -> None:
        """Known event types should be deserialized as their concrete class."""
        proto_event = _make_proto_event_data(
            event_type="TestOrderPlaced",
            payload={"order_id": "ord-001", "amount": 99.99},
        )

        envelope = self.client._proto_to_envelope(proto_event)

        assert isinstance(envelope.event, TestOrderPlacedEvent)
        assert envelope.event.order_id == "ord-001"
        assert envelope.event.amount == 99.99
        assert envelope.event.event_type == "TestOrderPlaced"

    def test_resolves_another_registered_event(self) -> None:
        """Verify a second registered event type also resolves correctly."""
        proto_event = _make_proto_event_data(
            event_type="TestItemAdded",
            payload={"item_id": "item-42", "quantity": 3},
        )

        envelope = self.client._proto_to_envelope(proto_event)

        assert isinstance(envelope.event, TestItemAddedEvent)
        assert envelope.event.item_id == "item-42"
        assert envelope.event.quantity == 3

    def test_unknown_event_type_falls_back_to_generic(self) -> None:
        """Unknown event types should become GenericDomainEvent."""
        proto_event = _make_proto_event_data(
            event_type="SomeUnknownEvent",
            payload={"foo": "bar", "baz": 42},
        )

        envelope = self.client._proto_to_envelope(proto_event)

        assert isinstance(envelope.event, GenericDomainEvent)

    def test_unknown_event_preserves_event_type_as_attribute(self) -> None:
        """GenericDomainEvent fallback should have event_type as an instance attribute."""
        proto_event = _make_proto_event_data(
            event_type="SomeUnknownEvent",
            payload={"foo": "bar"},
        )

        envelope = self.client._proto_to_envelope(proto_event)

        # event_type should be accessible via hasattr (critical for aggregate dispatch)
        assert hasattr(envelope.event, "event_type")
        assert envelope.event.event_type == "SomeUnknownEvent"

    def test_unknown_event_preserves_payload_fields(self) -> None:
        """GenericDomainEvent should preserve all payload fields."""
        proto_event = _make_proto_event_data(
            event_type="SomeUnknownEvent",
            payload={"custom_field": "value", "nested": {"key": "val"}},
        )

        envelope = self.client._proto_to_envelope(proto_event)

        event = envelope.event
        assert isinstance(event, GenericDomainEvent)
        data = event.model_dump()
        assert data["custom_field"] == "value"
        assert data["nested"] == {"key": "val"}

    def test_metadata_has_event_type_regardless_of_resolution(self) -> None:
        """EventMetadata.event_type should always be populated from proto meta."""
        for event_type in ["TestOrderPlaced", "UnknownType"]:
            proto_event = _make_proto_event_data(
                event_type=event_type,
                payload={"order_id": "x", "amount": 1.0}
                if event_type == "TestOrderPlaced"
                else {"data": "test"},
            )

            envelope = self.client._proto_to_envelope(proto_event)
            assert envelope.metadata.event_type == event_type

    def test_empty_event_type_falls_back_to_generic(self) -> None:
        """Empty event_type string should fall back to GenericDomainEvent."""
        proto_event = _make_proto_event_data(
            event_type="",
            payload={"some": "data"},
        )

        envelope = self.client._proto_to_envelope(proto_event)

        assert isinstance(envelope.event, GenericDomainEvent)

    def test_concrete_event_is_immutable(self) -> None:
        """Resolved concrete events should be frozen (immutable)."""
        proto_event = _make_proto_event_data(
            event_type="TestOrderPlaced",
            payload={"order_id": "ord-001", "amount": 99.99},
        )

        envelope = self.client._proto_to_envelope(proto_event)

        with pytest.raises((TypeError, AttributeError, ValueError)):
            envelope.event.order_id = "changed"  # type: ignore[attr-defined]

    def test_malformed_payload_falls_back_to_generic(self) -> None:
        """If concrete class rejects the payload, fall back to GenericDomainEvent."""
        # TestOrderPlaced requires order_id (str) and amount (float)
        # Sending wrong fields should trigger fallback
        proto_event = _make_proto_event_data(
            event_type="TestOrderPlaced",
            payload={"wrong_field": "value"},  # Missing required fields
        )

        envelope = self.client._proto_to_envelope(proto_event)

        # Should fall back to GenericDomainEvent since model_validate fails
        assert isinstance(envelope.event, GenericDomainEvent)
        assert envelope.event.event_type == "TestOrderPlaced"
