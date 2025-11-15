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
    AutoDispatchProjection,
    Projection,
    ProjectionManager,
)
from event_sourcing.core.query import Query, QueryBus, QueryHandler
from event_sourcing.core.repository import (
    EventStoreRepository,
    Repository,
    RepositoryFactory,
)
from event_sourcing.decorators.commands import command_handler
from event_sourcing.decorators.events import event_sourcing_handler

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
    # Projections
    "Projection",
    "ProjectionManager",
    "AutoDispatchProjection",
    # Repository
    "Repository",
    "EventStoreRepository",
    "RepositoryFactory",
    # Event Store Clients
    "EventStoreClient",
    "EventStoreClientFactory",
    "GrpcEventStoreClient",
    "MemoryEventStoreClient",
    # Decorators
    "event_sourcing_handler",
    "command_handler",
    # Errors
    "EventSourcingError",
    "AggregateNotFoundError",
    "ConcurrencyConflictError",
    "InvalidAggregateStateError",
    "EventStoreError",
]
