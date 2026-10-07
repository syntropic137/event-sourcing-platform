"""Tests for GrpcEventStoreClient event type registry resolution (ADR-023).

Verifies that _proto_to_envelope() resolves concrete event types from the
registry when available, and falls back to GenericDomainEvent with event_type
preserved when the type is unknown.
"""

import json

import pytest

from event_sourcing import DomainEvent, event
from event_sourcing.client.grpc_client import GrpcEventStoreClient
from event_sourcing.core.errors import EventPayloadError
from event_sourcing.core.event import GenericDomainEvent
from event_sourcing.proto.eventstore.v1 import eventstore_pb2

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
    event_version: int = 1,
) -> eventstore_pb2.EventData:
    """A real protobuf EventData with the given event type and payload."""
    meta = eventstore_pb2.EventMetadata(
        event_id="evt-001",
        aggregate_id=aggregate_id,
        aggregate_type=aggregate_type,
        aggregate_nonce=aggregate_nonce,
        global_nonce=1,
        event_type=event_type,
        event_version=event_version,
        content_type="application/json",
    )
    return eventstore_pb2.EventData(meta=meta, payload=json.dumps(payload).encode("utf-8"))


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

    def test_malformed_payload_is_a_typed_error(self) -> None:
        """A payload the registered class rejects is EventPayloadError (ADR-027)."""
        proto_event = _make_proto_event_data(
            event_type="TestOrderPlaced",
            payload={"wrong_field": "value"},  # Missing required fields
        )

        with pytest.raises(EventPayloadError) as exc:
            self.client._proto_to_envelope(proto_event)
        assert exc.value.event_type == "TestOrderPlaced"
        assert exc.value.global_nonce == 1

    def test_malformed_payload_falls_back_to_generic_when_opted_in(self) -> None:
        """on_invalid_payload="generic" keeps the pre-ADR-027 fallback."""
        client = GrpcEventStoreClient(address="localhost:50051", on_invalid_payload="generic")
        proto_event = _make_proto_event_data(
            event_type="TestOrderPlaced",
            payload={"wrong_field": "value"},
        )

        envelope = client._proto_to_envelope(proto_event)

        assert isinstance(envelope.event, GenericDomainEvent)
        assert envelope.event.event_type == "TestOrderPlaced"
        assert envelope.event.model_dump()["wrong_field"] == "value"
