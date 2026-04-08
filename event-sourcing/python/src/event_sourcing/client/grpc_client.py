# pyright: basic
"""gRPC event store client implementation.

The gRPC proto stubs (eventstore_pb2, eventstore_pb2_grpc) are auto-generated
and lack type information. Pyright strict mode flags every access as "unknown".
Rather than sprinkling hundreds of casts, we relax pyright to "basic" for this
adapter file. The public API (return types) is still fully typed — the "unknown"
types are confined to internal proto interactions.
"""

import json
import logging
from collections.abc import AsyncIterator

import grpc

from event_sourcing.core.errors import (
    ConcurrencyConflictError,
    EventStoreError,
    StreamAlreadyExistsError,
)
from event_sourcing.core.event import (
    DomainEvent,
    EventEnvelope,
    EventMetadata,
    GenericDomainEvent,
)
from event_sourcing.decorators.events import resolve_event_type
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
        """Connect to the event store.

        Idempotent: if a channel is already open, this is a no-op.
        Calling connect() repeatedly must NOT replace the channel because
        an active Subscribe stream would be silently killed.
        """
        if self._channel is not None and self._stub is not None:
            return

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
        self,
        stream_name: str,
        events: list[EventEnvelope[DomainEvent]],
        expected_version: int | None = None,
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
            if e.code() == grpc.StatusCode.ABORTED:
                actual_version = self._extract_actual_version(e)
                logger.warning(
                    f"Concurrency conflict on stream '{stream_name}' "
                    f"(expected={expected_ver}, actual={actual_version})"
                )
                if expected_ver == 0:
                    raise StreamAlreadyExistsError(stream_name, actual_version) from e
                raise ConcurrencyConflictError(expected_ver, actual_version) from e

            logger.error(f"gRPC error appending events: {e}")
            raise EventStoreError(f"Failed to append events: {e}") from e

    async def stream_exists(self, stream_name: str) -> bool:
        """Check if a stream exists."""
        try:
            events = await self.read_events(stream_name, from_version=0)
            return len(events) > 0
        except Exception:
            return False

    @staticmethod
    def _extract_actual_version(rpc_error: grpc.RpcError) -> int:
        """Extract actual_last_aggregate_nonce from gRPC ConcurrencyErrorDetail.

        The Rust event store encodes the detail as a ``google.protobuf.Any``
        wrapper inside the gRPC status trailing metadata.  Falls back to ``-1``
        if the detail cannot be decoded.
        """
        try:

            # grpc-python exposes trailing_metadata on RpcError
            status = rpc_error  # type: ignore[union-attr]
            raw_details = status.trailing_metadata()
            for key, value in raw_details:
                if key == "grpc-status-details-bin":
                    # The value is a serialized google.rpc.Status
                    from google.rpc import status_pb2  # type: ignore[import-untyped]

                    rpc_status = status_pb2.Status()
                    rpc_status.ParseFromString(value)
                    for detail in rpc_status.details:
                        if detail.Is(eventstore_pb2.ConcurrencyErrorDetail.DESCRIPTOR):
                            concurrency_detail = eventstore_pb2.ConcurrencyErrorDetail()
                            detail.Unpack(concurrency_detail)
                            return int(concurrency_detail.actual_last_aggregate_nonce)
        except Exception:
            logger.debug("Could not decode ConcurrencyErrorDetail from gRPC status", exc_info=True)
        return -1

    def _envelope_to_proto(
        self, envelope: EventEnvelope[DomainEvent], aggregate_id: str, aggregate_type: str
    ) -> eventstore_pb2.EventData:
        """Convert an EventEnvelope to protobuf EventData."""
        # Serialize event payload to JSON
        payload_dict = envelope.event.model_dump(mode="json")
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

    def _proto_to_envelope(
        self, event_data: eventstore_pb2.EventData
    ) -> EventEnvelope[DomainEvent]:
        """Convert protobuf EventData to an EventEnvelope."""
        meta = event_data.meta

        # Deserialize payload from JSON
        payload_dict = json.loads(event_data.payload.decode("utf-8"))

        # Create metadata — event_type lives here, not in the payload.
        # Injecting it into the payload caused ValidationError when downstream
        # code called model_validate() on concrete DomainEvent subclasses
        # (which use extra="forbid").
        metadata = EventMetadata(
            event_id=meta.event_id,
            aggregate_id=meta.aggregate_id,
            aggregate_type=meta.aggregate_type,
            aggregate_nonce=meta.aggregate_nonce,
            correlation_id=meta.correlation_id if meta.correlation_id else None,
            causation_id=meta.causation_id if meta.causation_id else None,
            actor_id=meta.actor_id if meta.actor_id else None,
            global_nonce=meta.global_nonce if meta.global_nonce > 0 else None,
            event_type=meta.event_type if meta.event_type else None,
        )

        # ADR-023: Consult the event type registry to resolve concrete types.
        # If the event type was registered via @event decorator, deserialize
        # into the concrete class. Otherwise fall back to GenericDomainEvent
        # with event_type preserved as an instance attribute so aggregate
        # rehydration can still route to the correct handler.
        event_type_str = meta.event_type if meta.event_type else ""
        concrete_cls = resolve_event_type(event_type_str) if event_type_str else None

        if concrete_cls is not None:
            try:
                event: DomainEvent = concrete_cls.model_validate(payload_dict)
            except Exception:
                logger.debug(
                    "Failed to deserialize as %s, falling back to GenericDomainEvent",
                    concrete_cls.__name__,
                    exc_info=True,
                )
                event = self._make_generic_event(payload_dict, event_type_str)
        else:
            event = self._make_generic_event(payload_dict, event_type_str)

        return EventEnvelope(event=event, metadata=metadata)

    @staticmethod
    def _make_generic_event(
        payload_dict: dict[str, object], event_type_str: str
    ) -> GenericDomainEvent:
        """Build a GenericDomainEvent, handling payload that may already contain event_type."""
        # Remove event_type from payload to avoid TypeError on duplicate kwarg
        cleaned = {k: v for k, v in payload_dict.items() if k != "event_type"}
        if event_type_str:
            return GenericDomainEvent(**cleaned, event_type=event_type_str)
        return GenericDomainEvent(**cleaned)

    async def read_all(
        self,
        from_global_nonce: int = 0,
        max_count: int = 100,
        forward: bool = True,
    ) -> tuple[list[EventEnvelope[DomainEvent]], bool, int]:
        """
        Read all events from a global position (for projections/catch-up).

        This method uses the dedicated ReadAll RPC which provides explicit
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
        """
        if not self._stub:
            raise EventStoreError("Client is not connected")

        request = eventstore_pb2.ReadAllRequest(
            tenant_id=self.tenant_id,
            from_global_nonce=from_global_nonce,
            max_count=max_count,
            forward=forward,
        )

        try:
            response = await self._stub.ReadAll(request)
            envelopes = []

            for event_data in response.events:
                envelope = self._proto_to_envelope(event_data)
                envelopes.append(envelope)

            logger.debug(
                f"ReadAll returned {len(envelopes)} events, "
                f"is_end={response.is_end}, next={response.next_from_global_nonce}"
            )
            return envelopes, response.is_end, response.next_from_global_nonce

        except grpc.RpcError as e:
            logger.error(f"gRPC error in ReadAll: {e}")
            raise EventStoreError(f"Failed to read all events: {e}") from e

    async def read_all_events_from(
        self,
        after_global_nonce: int = 0,
        limit: int = 100,
    ) -> list[EventEnvelope[DomainEvent]]:
        """
        Read all events from a global nonce (for projections/catch-up).

        .. deprecated::
            Use :meth:`read_all` instead for explicit pagination and end-of-batch signaling.
            This method is kept for backward compatibility.

        Args:
            after_global_nonce: global nonce to read from (exclusive)
            limit: Maximum number of events to return

        Returns:
            List of event envelopes in global order
        """
        # Use the new ReadAll RPC with from_global_nonce = after_global_nonce + 1
        # because read_all_events_from is exclusive, but ReadAll is inclusive
        events, _is_end, _next_pos = await self.read_all(
            from_global_nonce=after_global_nonce + 1,
            max_count=limit,
            forward=True,
        )
        return events

    async def subscribe(
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
        if not self._stub:
            raise EventStoreError("Client is not connected")

        request = eventstore_pb2.SubscribeRequest(
            tenant_id=self.tenant_id,
            aggregate_id_prefix="",  # Subscribe to all aggregates
            from_global_nonce=from_global_nonce,
        )

        try:
            logger.info(f"Starting subscription from global nonce {from_global_nonce}")
            async for response in self._stub.Subscribe(request):
                # Skip keepalive messages (event: None)
                if not response.HasField("event"):
                    logger.debug("Received keepalive from Subscribe stream")
                    continue

                envelope = self._proto_to_envelope(response.event)
                yield envelope

        except grpc.RpcError as e:
            if e.code() == grpc.StatusCode.CANCELLED:
                logger.info("Subscription cancelled")
                return
            logger.error(f"gRPC error in subscription: {e}")
            raise EventStoreError(f"Subscription failed: {e}") from e
