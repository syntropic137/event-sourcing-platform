"""Event store client interface."""

from collections.abc import AsyncIterator
from typing import TYPE_CHECKING, Protocol

from event_sourcing.core.envelope import InvalidPayloadPolicy
from event_sourcing.core.event import DomainEvent, EventEnvelope
from event_sourcing.core.upcast import Upcasters

if TYPE_CHECKING:
    from event_sourcing.client.auth import Credentials, TlsConfig


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
            True if the stream exists, False if it has no events

        Raises:
            EventStoreError: If the store cannot be reached or refuses the
                read. Never reported as False: a caller must be able to tell
                "no such stream" from "could not look".
        """
        ...

    async def connect(self) -> None:
        """Connect to the event store."""
        ...

    async def disconnect(self) -> None:
        """Disconnect from the event store."""
        ...

    async def read_all(
        self,
        from_global_nonce: int = 0,
        max_count: int = 100,
        forward: bool = True,
    ) -> tuple[list[EventEnvelope[DomainEvent]], bool, int]:
        """
        Read all events from a global position (for projections/catch-up).

        This is the preferred method for catch-up reads as it provides explicit
        pagination and end-of-batch signaling.

        Args:
            from_global_nonce: Global nonce to read from (inclusive)
            max_count: Maximum number of events to return per page
            forward: Direction (True = ascending order)

        Returns:
            Tuple of (events, is_end, next_from_global_nonce)
            - events: List of event envelopes in requested order
            - is_end: True if no more events after this batch
            - next_from_global_nonce: Position for next page (if not is_end)

        Raises:
            EventStoreError: If reading fails
        """
        ...

    async def read_all_events_from(
        self,
        after_global_nonce: int = 0,
        limit: int = 100,
    ) -> list[EventEnvelope[DomainEvent]]:
        """
        Read all events from a global nonce (for projections/catch-up).

        .. deprecated::
            Use :meth:`read_all` instead for explicit pagination and end-of-batch signaling.

        Args:
            after_global_nonce: global nonce to read from (exclusive)
            limit: Maximum number of events to return (for batching)

        Returns:
            List of event envelopes in global order

        Raises:
            EventStoreError: If reading fails
        """
        ...

    def subscribe(
        self,
        from_global_nonce: int = 0,
    ) -> AsyncIterator[EventEnvelope[DomainEvent]]:
        """
        Subscribe to events from a global nonce (live streaming).

        This method returns an async iterator that yields events as they arrive.
        It's designed for live subscriptions that run indefinitely until cancelled.

        Args:
            from_global_nonce: global nonce to start from (inclusive)

        Yields:
            EventEnvelope objects as they arrive

        Raises:
            EventStoreError: If subscription fails
        """
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
        *,
        auth: "Credentials | None" = None,
        tls: "TlsConfig | bool | None" = None,
        allow_insecure_credentials: bool = False,
        upcasters: Upcasters | None = None,
        on_invalid_payload: InvalidPayloadPolicy = "raise",
    ) -> EventStoreClient:
        """
        Create a gRPC event store client for production.

        Args:
            host: Event store server host
            port: Event store server port
            tenant_id: Tenant identifier for multi-tenancy
            auth: Credentials sent on every call (e.g. ``BasicAuth`` for the
                ADR-024 gateway)
            tls: ``True`` or a ``TlsConfig`` to connect over TLS
            allow_insecure_credentials: allow ``auth`` over plaintext to a
                non-loopback host
            upcasters: Steps that migrate stored events before decoding (ADR-027)
            on_invalid_payload: See ``GrpcEventStoreClient``

        Returns:
            GrpcEventStoreClient instance
        """
        from event_sourcing.client.grpc_client import GrpcEventStoreClient

        address = f"{host}:{port}"
        return GrpcEventStoreClient(
            address=address,
            tenant_id=tenant_id,
            auth=auth,
            tls=tls,
            allow_insecure_credentials=allow_insecure_credentials,
            upcasters=upcasters,
            on_invalid_payload=on_invalid_payload,
        )
