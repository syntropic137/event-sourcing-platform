"""gRPC event store client implementation."""

import json
import logging

import grpc

from event_sourcing.core.errors import ConcurrencyConflictError, EventStoreError
from event_sourcing.core.event import BaseDomainEvent, DomainEvent, EventEnvelope, EventMetadata
from event_sourcing.proto.eventstore.v1 import eventstore_pb2, eventstore_pb2_grpc

logger = logging.getLogger(__name__)


class GrpcEventStoreClient:
    """
    gRPC implementation of the EventStoreClient.

    Connects to the Rust event store via gRPC and provides high-level
    event sourcing operations.
    """

    def __init__(
        self,
        address: str = "localhost:50051",
        tenant_id: str = "default",
        credentials: grpc.ChannelCredentials | None = None,
    ) -> None:
        """
        Initialize the gRPC client.

        Args:
            address: The gRPC server address (host:port)
            tenant_id: The tenant ID for multi-tenancy support
            credentials: Optional gRPC credentials for TLS/auth
        """
        self.address = address
        self.tenant_id = tenant_id
        self._channel: grpc.Channel | None = None
        self._stub: eventstore_pb2_grpc.EventStoreStub | None = None
        self._credentials = credentials

    async def connect(self) -> None:
        """Connect to the event store."""
        logger.info(f"Connecting to event store at {self.address}")

        if self._credentials:
            self._channel = grpc.aio.secure_channel(self.address, self._credentials)
        else:
            self._channel = grpc.aio.insecure_channel(self.address)

        self._stub = eventstore_pb2_grpc.EventStoreStub(self._channel)  # type: ignore[no-untyped-call]
        logger.info("Connected to event store")

    async def disconnect(self) -> None:
        """Disconnect from the event store."""
        if self._channel:
            await self._channel.close()
            self._channel = None
            self._stub = None
            logger.info("Disconnected from event store")

    async def read_events(
        self, stream_name: str, from_version: int | None = None
    ) -> list[EventEnvelope[DomainEvent]]:
        """
        Read events from a specific stream.

        Args:
            stream_name: The name of the event stream.
            from_version: The starting version (inclusive) to read from.

        Returns:
            A list of EventEnvelope objects.
        """
        if not self._stub:
            raise EventStoreError("Client is not connected")

        # Parse stream name (format: "AggregateType-AggregateId")
        parts = stream_name.split("-", 1)
        if len(parts) != 2:
            raise EventStoreError(f"Invalid stream name format: {stream_name}")

        aggregate_type, aggregate_id = parts

        # Default to 0, then convert to 1-based indexing for protobuf
        start_version = from_version if from_version is not None else 0

        request = eventstore_pb2.ReadStreamRequest(
            tenant_id=self.tenant_id,
            aggregate_id=aggregate_id,
            from_aggregate_nonce=max(start_version, 1),  # Protobuf uses 1-based indexing
            max_count=1000,
            forward=True,
        )

        try:
            response = await self._stub.ReadStream(request)
            envelopes = []

            for event_data in response.events:
                envelope = self._proto_to_envelope(event_data)
                envelopes.append(envelope)

            logger.debug(f"Read {len(envelopes)} events from stream '{stream_name}'")
            return envelopes

        except grpc.RpcError as e:
            logger.error(f"gRPC error reading stream: {e}")
            raise EventStoreError(f"Failed to read stream: {e}") from e

    async def append_events(
        self, stream_name: str, events: list[EventEnvelope[DomainEvent]], expected_version: int | None = None
    ) -> None:
        """
        Append new events to a stream.

        Args:
            stream_name: The name of the event stream.
            events: A list of EventEnvelope objects to append.
            expected_version: The expected current version of the stream.

        Raises:
            ConcurrencyConflictError: If the expected version does not match.
        """
        if not self._stub:
            raise EventStoreError("Client is not connected")

        if not events:
            return

        # Parse stream name
        parts = stream_name.split("-", 1)
        if len(parts) != 2:
            raise EventStoreError(f"Invalid stream name format: {stream_name}")

        aggregate_type, aggregate_id = parts

        # Convert events to proto format
        proto_events = []
        for envelope in events:
            proto_event = self._envelope_to_proto(envelope, aggregate_id, aggregate_type)
            proto_events.append(proto_event)

        # Default to 0 if not provided
        expected_ver = expected_version if expected_version is not None else 0

        request = eventstore_pb2.AppendRequest(
            tenant_id=self.tenant_id,
            aggregate_id=aggregate_id,
            aggregate_type=aggregate_type,
            expected_aggregate_nonce=expected_ver,
            events=proto_events,
        )

        try:
            response = await self._stub.Append(request)
            logger.debug(
                f"Appended {len(events)} events to stream '{stream_name}'. "
                f"New global nonce: {response.last_global_nonce}"
            )

        except grpc.RpcError as e:
            # Check if it's a concurrency conflict
            if e.code() == grpc.StatusCode.FAILED_PRECONDITION:
                logger.warning(f"Concurrency conflict on stream '{stream_name}'")
                # Extract actual version from error details if possible, otherwise use -1
                raise ConcurrencyConflictError(expected_ver, -1) from e

            logger.error(f"gRPC error appending events: {e}")
            raise EventStoreError(f"Failed to append events: {e}") from e

    async def stream_exists(self, stream_name: str) -> bool:
        """Check if a stream exists."""
        try:
            events = await self.read_events(stream_name, from_version=0)
            return len(events) > 0
        except Exception:
            return False

    def _envelope_to_proto(
        self, envelope: EventEnvelope[DomainEvent], aggregate_id: str, aggregate_type: str
    ) -> eventstore_pb2.EventData:
        """Convert an EventEnvelope to protobuf EventData."""
        # Serialize event payload to JSON
        payload_dict = envelope.event.model_dump()
        payload_bytes = json.dumps(payload_dict).encode("utf-8")

        # Get event type from the event
        event_type = (
            envelope.event.event_type
            if hasattr(envelope.event, "event_type")
            else type(envelope.event).__name__
        )

        # Build metadata
        meta = eventstore_pb2.EventMetadata(
            event_id=envelope.metadata.event_id,
            aggregate_id=aggregate_id,
            aggregate_type=aggregate_type,
            aggregate_nonce=envelope.metadata.aggregate_nonce,
            event_type=event_type,
            event_version=1,  # Default schema version
            content_type="application/json",
            tenant_id=self.tenant_id,
            correlation_id=envelope.metadata.correlation_id or "",
            causation_id=envelope.metadata.causation_id or "",
            actor_id=envelope.metadata.actor_id or "",
            timestamp_unix_ms=int(envelope.metadata.timestamp.timestamp() * 1000),
        )

        return eventstore_pb2.EventData(meta=meta, payload=payload_bytes)

    def _proto_to_envelope(self, event_data: eventstore_pb2.EventData) -> EventEnvelope[DomainEvent]:
        """Convert protobuf EventData to an EventEnvelope."""
        meta = event_data.meta

        # Deserialize payload from JSON
        payload_dict = json.loads(event_data.payload.decode("utf-8"))

        # Create metadata
        metadata = EventMetadata(
            event_id=meta.event_id,
            aggregate_id=meta.aggregate_id,
            aggregate_type=meta.aggregate_type,
            aggregate_nonce=meta.aggregate_nonce,
            correlation_id=meta.correlation_id if meta.correlation_id else None,
            causation_id=meta.causation_id if meta.causation_id else None,
            actor_id=meta.actor_id if meta.actor_id else None,
        )

        # Create a generic event dict (will be deserialized by the repository)
        # For now, we store the raw payload
        event = BaseDomainEvent(**payload_dict)

        return EventEnvelope(event=event, metadata=metadata)

