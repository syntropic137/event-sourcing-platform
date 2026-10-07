"""Event store client interfaces and implementations."""

from event_sourcing.client.auth import (
    BasicAuth,
    BearerToken,
    Credentials,
    SharedToken,
    TlsConfig,
    TokenProvider,
    TokenProviderAuth,
)
from event_sourcing.client.event_store import EventStoreClient, EventStoreClientFactory
from event_sourcing.client.grpc_client import GrpcEventStoreClient
from event_sourcing.client.memory import MemoryEventStoreClient
from event_sourcing.client.server_info import (
    LEGACY_SERVER_INFO,
    SERVER_INFO_MIN_VERSION,
    Capabilities,
    CompatibilityError,
    ServerInfo,
)

__all__ = [
    "EventStoreClient",
    "EventStoreClientFactory",
    "MemoryEventStoreClient",
    "GrpcEventStoreClient",
    "BasicAuth",
    "BearerToken",
    "Credentials",
    "SharedToken",
    "TlsConfig",
    "TokenProvider",
    "TokenProviderAuth",
    "Capabilities",
    "CompatibilityError",
    "LEGACY_SERVER_INFO",
    "SERVER_INFO_MIN_VERSION",
    "ServerInfo",
]
