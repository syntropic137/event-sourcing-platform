"""Core event sourcing abstractions."""

from event_sourcing.core.aggregate import AggregateRoot, BaseAggregate
from event_sourcing.core.checkpoint import (
    CheckpointedProjection,
    DispatchContext,
    ProjectionCheckpoint,
    ProjectionCheckpointStore,
    ProjectionResult,
)
from event_sourcing.core.command import Command, CommandBus, CommandHandler
from event_sourcing.core.errors import EventSourcingError
from event_sourcing.core.event import DomainEvent, EventEnvelope, EventMetadata
from event_sourcing.core.process_manager import ProcessManager
from event_sourcing.core.query import Query, QueryHandler
from event_sourcing.core.repository import Repository

__all__ = [
    # Aggregates
    "AggregateRoot",
    "BaseAggregate",
    # Checkpoints (ADR-014)
    "CheckpointedProjection",
    "DispatchContext",
    "ProjectionCheckpoint",
    "ProjectionCheckpointStore",
    "ProjectionResult",
    # Process Manager (To-Do List pattern)
    "ProcessManager",
    # Commands
    "Command",
    "CommandHandler",
    "CommandBus",
    # Events
    "DomainEvent",
    "EventEnvelope",
    "EventMetadata",
    # Errors
    "EventSourcingError",
    # Queries
    "Query",
    "QueryHandler",
    # Repository
    "Repository",
]
