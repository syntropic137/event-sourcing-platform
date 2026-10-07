"""
Event Sourcing Python SDK

This SDK provides high-level abstractions for implementing event sourcing patterns
in Python applications. It builds on top of event stores to provide developer-friendly
APIs for aggregates, commands, events, and repositories.
"""

from event_sourcing.client import (
    BasicAuth,
    BearerToken,
    EventStoreClient,
    EventStoreClientFactory,
    GrpcEventStoreClient,
    MemoryEventStoreClient,
    SharedToken,
    TlsConfig,
    TokenProviderAuth,
)
from event_sourcing.core.aggregate import AggregateRoot, BaseAggregate
from event_sourcing.core.checkpoint import DispatchContext, ProjectionReadStore, ProjectionStore
from event_sourcing.core.command import Command, CommandBus, CommandHandler, InMemoryCommandBus
from event_sourcing.core.envelope import ENVELOPE_ECHO_KEYS, DecodedEvent, decode_event
from event_sourcing.core.errors import (
    AggregateNotFoundError,
    ClientConfigError,
    ConcurrencyConflictError,
    EventDecodeError,
    EventPayloadError,
    EventSourcingError,
    EventStoreAuthenticationError,
    EventStoreError,
    InvalidAggregateStateError,
    StreamAlreadyExistsError,
    SubscriptionHaltedError,
    UndecodableEventError,
    UnknownEventTypeError,
    UnknownEventVersionError,
    UnsupportedContentTypeError,
    UpcastError,
)
from event_sourcing.core.event import (
    BaseDomainEvent,
    DomainEvent,
    EventEnvelope,
    EventMetadata,
    GenericDomainEvent,
)
from event_sourcing.core.expected_version import ExpectedVersion
from event_sourcing.core.historical_poller import (
    CursorData,
    CursorStore,
    HistoricalPoller,
    PollEvent,
    PollResult,
)
from event_sourcing.core.process_manager import ProcessManager
from event_sourcing.core.projection import (
    AutoDispatchProjection,
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
from event_sourcing.core.upcast import Upcasters
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
    get_event_type_registry,
    registered_event_versions,
    resolve_event_class,
    resolve_event_type,
)
from event_sourcing.stores import (
    MemoryCheckpointStore,
    MemoryProjectionStore,
    PostgresCheckpointStore,
)
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
    "AutoDispatchProjection",
    "CheckpointedProjection",
    "DispatchContext",
    "ProjectionCheckpoint",
    "ProjectionCheckpointStore",
    "ProjectionReadStore",
    "ProjectionResult",
    "ProjectionStore",
    # Process Manager (To-Do List pattern)
    "ProcessManager",
    # Historical Poller (Cold-Start-Safe External Ingestion)
    "HistoricalPoller",
    "CursorStore",
    "CursorData",
    "PollEvent",
    "PollResult",
    # Checkpoint Stores
    "PostgresCheckpointStore",
    "MemoryCheckpointStore",
    # Projection Stores
    "MemoryProjectionStore",
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
    # Connection credentials and TLS (ADR-024 gateway)
    "BasicAuth",
    "BearerToken",
    "TokenProviderAuth",
    "SharedToken",
    "TlsConfig",
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
    "get_event_type_registry",
    "registered_event_versions",
    "resolve_event_class",
    "resolve_event_type",
    # Cross-language envelope and upcasting (ADR-027)
    "DecodedEvent",
    "ENVELOPE_ECHO_KEYS",
    "Upcasters",
    "decode_event",
    # Concurrency Control
    "ExpectedVersion",
    # Errors
    "EventSourcingError",
    "AggregateNotFoundError",
    "ConcurrencyConflictError",
    "StreamAlreadyExistsError",
    "InvalidAggregateStateError",
    "EventStoreError",
    "EventStoreAuthenticationError",
    "ClientConfigError",
    "UndecodableEventError",
    "SubscriptionHaltedError",
    "EventDecodeError",
    "EventPayloadError",
    "UnknownEventTypeError",
    "UnknownEventVersionError",
    "UnsupportedContentTypeError",
    "UpcastError",
]
