"""Core event sourcing abstractions."""

from event_sourcing.core.aggregate import AggregateRoot, BaseAggregate
from event_sourcing.core.command import Command, CommandBus, CommandHandler
from event_sourcing.core.errors import EventSourcingError
from event_sourcing.core.event import DomainEvent, EventEnvelope, EventMetadata
from event_sourcing.core.query import Query, QueryHandler
from event_sourcing.core.repository import Repository

__all__ = [
    "AggregateRoot",
    "BaseAggregate",
    "Command",
    "CommandHandler",
    "CommandBus",
    "DomainEvent",
    "EventEnvelope",
    "EventMetadata",
    "EventSourcingError",
    "Query",
    "QueryHandler",
    "Repository",
]

