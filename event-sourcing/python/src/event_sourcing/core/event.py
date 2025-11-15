"""Event definitions and metadata handling for the event sourcing SDK."""

from datetime import UTC, datetime
from typing import Any, ClassVar, Generic, TypeVar
from uuid import uuid4

from pydantic import BaseModel, Field


class DomainEvent(BaseModel):
    """
    Base class for all domain events.

    Domain events represent facts that have occurred in the domain.
    They are immutable and should contain all information needed to
    understand what happened.

    Example:
        class OrderPlaced(DomainEvent):
            event_type: ClassVar[str] = "OrderPlaced"
            order_id: str
            customer_id: str
            items: list[str]
    """

    event_type: ClassVar[str]
    schema_version: ClassVar[int] = 1

    model_config = {"frozen": True, "extra": "forbid"}

    def model_dump_json(self, **kwargs: Any) -> str:
        """Serialize event to JSON."""
        return super().model_dump_json(exclude_none=True, **kwargs)


class BaseDomainEvent(DomainEvent):
    """
    Convenience base class for domain events with common functionality.

    This class provides default implementations for event_type based on
    the class name, making it easier to create simple events.
    """

    @classmethod
    def get_event_type(cls) -> str:
        """Get the event type (defaults to class name)."""
        if hasattr(cls, "event_type") and isinstance(cls.event_type, str):
            return cls.event_type
        return cls.__name__


class GenericDomainEvent(DomainEvent):
    """
    Generic domain event that allows arbitrary fields.

    Used for deserializing events from the event store when the concrete
    event type is not known at deserialization time.
    """

    model_config = {"frozen": True, "extra": "allow"}


class EventMetadata(BaseModel):
    """
    Event metadata that accompanies every event.

    Contains information about when, where, and by whom the event was created.
    """

    event_id: str = Field(default_factory=lambda: str(uuid4()))
    timestamp: datetime = Field(default_factory=lambda: datetime.now(UTC))
    recorded_timestamp: datetime = Field(default_factory=lambda: datetime.now(UTC))
    aggregate_nonce: int  # Version/sequence number when event was created
    aggregate_id: str
    aggregate_type: str
    tenant_id: str | None = None
    global_nonce: int | None = None  # Matches proto: uint64 global_nonce
    content_type: str = "application/json"
    correlation_id: str | None = None
    causation_id: str | None = None
    actor_id: str | None = None
    headers: dict[str, str] = Field(default_factory=dict)
    payload_hash: str | None = None
    custom_metadata: dict[str, Any] = Field(default_factory=dict)

    model_config = {"frozen": True}


TEvent = TypeVar("TEvent", bound=DomainEvent)


class EventEnvelope(BaseModel, Generic[TEvent]):
    """
    Event envelope that wraps a domain event with metadata.

    The envelope pattern separates the domain event (what happened)
    from the metadata (when, where, by whom it happened).
    """

    event: TEvent
    metadata: EventMetadata

    model_config = {"frozen": True}


class EventFactory:
    """Factory for creating event envelopes with generated metadata."""

    @staticmethod
    def create(
        event: TEvent,
        aggregate_id: str,
        aggregate_type: str,
        aggregate_nonce: int,
        tenant_id: str | None = None,
        correlation_id: str | None = None,
        causation_id: str | None = None,
        actor_id: str | None = None,
        headers: dict[str, str] | None = None,
        custom_metadata: dict[str, Any] | None = None,
    ) -> EventEnvelope[TEvent]:
        """
        Create an event envelope with generated metadata.

        Args:
            event: The domain event
            aggregate_id: ID of the aggregate that produced the event
            aggregate_type: Type of the aggregate
            aggregate_nonce: Version when event was created
            tenant_id: Optional tenant identifier
            correlation_id: Optional correlation identifier
            causation_id: Optional causation identifier
            actor_id: Optional actor identifier
            headers: Optional custom headers
            custom_metadata: Optional custom metadata

        Returns:
            EventEnvelope wrapping the event with metadata
        """
        metadata = EventMetadata(
            aggregate_id=aggregate_id,
            aggregate_type=aggregate_type,
            aggregate_nonce=aggregate_nonce,
            tenant_id=tenant_id,
            correlation_id=correlation_id,
            causation_id=causation_id,
            actor_id=actor_id,
            headers=headers or {},
            custom_metadata=custom_metadata or {},
        )

        return EventEnvelope(event=event, metadata=metadata)
