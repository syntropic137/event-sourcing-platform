"""Event store client interface."""

from typing import Protocol

from event_sourcing.core.event import DomainEvent, EventEnvelope


class EventStoreClient(Protocol):
    """
    Protocol for event store clients.

    Defines the interface that all event store implementations must follow,
    whether in-memory for testing or gRPC for production.
    """

    async def read_events(
        self,
        stream_name: str,
        from_version: int | None = None,
    ) -> list[EventEnvelope[DomainEvent]]:
        """
        Read events from a stream.

        Args:
            stream_name: The stream identifier (typically aggregateType-aggregateId)
            from_version: Optional version to read from (defaults to start)

        Returns:
            List of event envelopes in order

        Raises:
            EventStoreError: If reading fails
        """
        ...

    async def append_events(
        self,
        stream_name: str,
        events: list[EventEnvelope[DomainEvent]],
        expected_version: int | None = None,
    ) -> None:
        """
        Append events to a stream with optimistic concurrency control.

        Args:
            stream_name: The stream identifier
            events: Events to append
            expected_version: Expected current version for concurrency control

        Raises:
            ConcurrencyConflictError: If version mismatch detected
            EventStoreError: If append fails
        """
        ...

    async def stream_exists(self, stream_name: str) -> bool:
        """
        Check if a stream exists.

        Args:
            stream_name: The stream identifier

        Returns:
            True if the stream exists
        """
        ...

    async def connect(self) -> None:
        """Connect to the event store."""
        ...

    async def disconnect(self) -> None:
        """Disconnect from the event store."""
        ...


class EventStoreClientFactory:
    """Factory for creating event store clients."""

    @staticmethod
    def create_memory_client() -> EventStoreClient:
        """
        Create an in-memory event store client for testing.

        Returns:
            MemoryEventStoreClient instance
        """
        from event_sourcing.client.memory import MemoryEventStoreClient

        return MemoryEventStoreClient()

    @staticmethod
    def create_grpc_client(
        host: str = "localhost",
        port: int = 50051,
        tenant_id: str = "default",
    ) -> EventStoreClient:
        """
        Create a gRPC event store client for production.

        Args:
            host: Event store server host
            port: Event store server port
            tenant_id: Tenant identifier for multi-tenancy

        Returns:
            GrpcEventStoreClient instance
        """
        from event_sourcing.client.grpc_client import GrpcEventStoreClient

        address = f"{host}:{port}"
        return GrpcEventStoreClient(address=address, tenant_id=tenant_id)

