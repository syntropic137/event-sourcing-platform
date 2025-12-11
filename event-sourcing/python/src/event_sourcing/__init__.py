"""
Event Sourcing Python SDK

This SDK provides high-level abstractions for implementing event sourcing patterns
in Python applications. It builds on top of event stores to provide developer-friendly
APIs for aggregates, commands, events, and repositories.
"""

from event_sourcing.client import (
    EventStoreClient,
    EventStoreClientFactory,
    GrpcEventStoreClient,
    MemoryEventStoreClient,
)
from event_sourcing.core.aggregate import AggregateRoot, BaseAggregate
from event_sourcing.core.command import Command, CommandBus, CommandHandler, InMemoryCommandBus
from event_sourcing.core.errors import (
    AggregateNotFoundError,
    ConcurrencyConflictError,
    EventSourcingError,
    EventStoreError,
    InvalidAggregateStateError,
)
from event_sourcing.core.event import (
    BaseDomainEvent,
    DomainEvent,
    EventEnvelope,
    EventMetadata,
    GenericDomainEvent,
)
from event_sourcing.core.projection import (
    CheckpointedProjection,
    ProjectionCheckpoint,
    ProjectionCheckpointStore,
    ProjectionResult,
)
from event_sourcing.core.query import Query, QueryBus, QueryHandler
from event_sourcing.core.repository import (
    EventStoreRepository,
    Repository,
    RepositoryFactory,
)
from event_sourcing.decorators.commands import (
    aggregate,
    command,
    command_handler,
    get_command_metadata,
)
from event_sourcing.decorators.events import (
    event,
    event_sourcing_handler,
    get_event_metadata,
)
from event_sourcing.stores import MemoryCheckpointStore, PostgresCheckpointStore
from event_sourcing.subscriptions import SubscriptionCoordinator

__version__ = "0.1.0"

__all__ = [
    # Core abstractions
    "AggregateRoot",
    "BaseAggregate",
    "BaseDomainEvent",
    "DomainEvent",
    "EventEnvelope",
    "EventMetadata",
    "GenericDomainEvent",
    # Commands
    "Command",
    "CommandHandler",
    "CommandBus",
    "InMemoryCommandBus",
    # Queries
    "Query",
    "QueryHandler",
    "QueryBus",
    # Projections (ADR-014 Checkpoint Architecture)
    "CheckpointedProjection",
    "ProjectionCheckpoint",
    "ProjectionCheckpointStore",
    "ProjectionResult",
    # Checkpoint Stores
    "PostgresCheckpointStore",
    "MemoryCheckpointStore",
    # Subscription Coordinator
    "SubscriptionCoordinator",
    # Repository
    "Repository",
    "EventStoreRepository",
    "RepositoryFactory",
    # Event Store Clients
    "EventStoreClient",
    "EventStoreClientFactory",
    "GrpcEventStoreClient",
    "MemoryEventStoreClient",
    # Class Decorators (for aggregate, command, event classes)
    "aggregate",
    "command",
    "event",
    # Method Decorators (for aggregate methods)
    "event_sourcing_handler",
    "command_handler",
    # Metadata helpers
    "get_command_metadata",
    "get_event_metadata",
    # Errors
    "EventSourcingError",
    "AggregateNotFoundError",
    "ConcurrencyConflictError",
    "InvalidAggregateStateError",
    "EventStoreError",
]
