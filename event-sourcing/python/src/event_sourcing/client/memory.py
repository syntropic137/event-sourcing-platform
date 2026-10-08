"""In-memory event store client for testing."""

from __future__ import annotations

import asyncio
import logging
import os
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from collections.abc import AsyncIterator

from event_sourcing.core.errors import (
    ConcurrencyConflictError,
    EventStoreError,
    StreamAlreadyExistsError,
)
from event_sourcing.core.event import DomainEvent, EventEnvelope

logger = logging.getLogger(__name__)


class MockTestEnvironmentError(RuntimeError):
    """Raised when mock objects are used outside of test environment."""

    pass


def _assert_test_environment() -> None:
    """Assert test environment - REQUIRED for all mocks.

    This prevents mock objects from being used in development, staging,
    or production environments where real implementations should be used.

    Raises:
        MockTestEnvironmentError: If APP_ENVIRONMENT is not 'test'
    """
    app_env = os.getenv("APP_ENVIRONMENT", "").lower()
    if app_env != "test":
        raise MockTestEnvironmentError(
            f"MemoryEventStoreClient can only be used in test environment. "
            f"Current APP_ENVIRONMENT: '{app_env}'. "
            f"Set APP_ENVIRONMENT=test for unit tests, or use GrpcEventStoreClient "
            f"for development/production."
        )


def _stream_key(stream_name: str) -> str:
    """The key the ESP server stores ``stream_name`` under: the aggregate id alone.

    The gRPC client splits ``"Type-id"`` and the server keys a stream by
    ``(tenant, aggregate_id)``; the type is stored but is not part of the key.
    So ``Order-1`` and ``Invoice-1`` are ONE stream on the server, and must be
    one stream here too, or a collision production hits passes every unit test
    (event-sourcing-platform#344).
    """
    parts = stream_name.split("-", 1)
    if len(parts) != 2:
        raise EventStoreError(f"Invalid stream name format: {stream_name}")
    return parts[1]


class MemoryEventStoreClient:
    """
    In-memory implementation of event store client.

    This implementation stores events in memory and provides the same
    interface as the gRPC client, making it ideal for testing.

    WARNING: This client can ONLY be used in test environments.
    It will raise MockTestEnvironmentError if APP_ENVIRONMENT is not 'test'.
    For development and production, use GrpcEventStoreClient.
    """

    def __init__(self) -> None:
        # CRITICAL: Validate test environment before allowing usage
        _assert_test_environment()

        # Keyed by aggregate id alone, as the server keys them: see _stream_key.
        self._streams: dict[str, list[EventEnvelope[DomainEvent]]] = {}
        self._connected = False
        # Next global nonce to assign. The store numbers events from 1, so a
        # backward read's terminal cursor 0 names no event (#405).
        self._global_nonce_counter = 1

    async def connect(self) -> None:
        """Connect (no-op for memory client)."""
        self._connected = True
        logger.debug("Memory event store client connected")

    async def disconnect(self) -> None:
        """Disconnect (no-op for memory client)."""
        self._connected = False
        logger.debug("Memory event store client disconnected")

    async def read_events(
        self,
        stream_name: str,
        from_version: int | None = None,
    ) -> list[EventEnvelope[DomainEvent]]:
        """
        Read events from a stream.

        Args:
            stream_name: The stream identifier
            from_version: Aggregate nonce to read from, inclusive; 0, 1 or
                None read from the first event (like the store)

        Returns:
            List of event envelopes; empty for an unknown stream
        """
        events = self._streams.get(_stream_key(stream_name), [])
        # Version N is the event at index N - 1.
        return events[max((from_version or 0) - 1, 0) :]  # a copy

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
            expected_version: Expected current version; 0 or None means the
                stream must be new (the gRPC client sends None as 0)

        Raises:
            ConcurrencyConflictError: If version mismatch detected
            EventStoreError: If aggregate nonces do not continue the stream
                one by one (the store refuses them)
        """
        if not events:
            return

        key = _stream_key(stream_name)
        # Get current version (number of events in stream)
        current_version = len(self._streams.get(key, []))
        expected_version = expected_version or 0

        if current_version != expected_version:
            if expected_version == 0:
                raise StreamAlreadyExistsError(
                    stream_name=stream_name,
                    actual_version=current_version,
                )
            raise ConcurrencyConflictError(
                expected_version=expected_version,
                actual_version=current_version,
            )

        # Version N is the event at index N - 1 (read_events relies on it).
        for offset, event in enumerate(events, start=1):
            if event.metadata.aggregate_nonce != current_version + offset:
                raise EventStoreError(
                    f"event {offset - 1} aggregate_nonce {event.metadata.aggregate_nonce} "
                    f"must equal expected {current_version + offset}"
                )

        # Create stream if it doesn't exist
        if key not in self._streams:
            self._streams[key] = []

        # The store numbers every event, ignoring any global nonce the caller
        # set: keeping it could duplicate one and break paging (#405).
        # EventEnvelope is frozen, so each gets new metadata.
        updated_events: list[EventEnvelope[DomainEvent]] = []
        for event in events:
            new_metadata = event.metadata.model_copy(
                update={"global_nonce": self._global_nonce_counter}
            )
            updated_events.append(EventEnvelope(event=event.event, metadata=new_metadata))
            self._global_nonce_counter += 1

        # Append events
        self._streams[key].extend(updated_events)

        logger.debug(
            f"Appended {len(events)} event(s) to stream '{stream_name}' "
            f"(new version: {len(self._streams[key])})"
        )

    async def stream_exists(self, stream_name: str) -> bool:
        """Check if a stream exists."""
        return len(self._streams.get(_stream_key(stream_name), [])) > 0

    def clear(self) -> None:
        """Clear all streams (useful for tests)."""
        self._streams.clear()

    def get_stream_version(self, stream_name: str) -> int:
        """Get the current version of a stream (for testing)."""
        return len(self._streams.get(_stream_key(stream_name), []))

    def _filter_and_sort_events(
        self,
        from_global_nonce: int,
        forward: bool,
    ) -> list[EventEnvelope[DomainEvent]]:
        """Collect all events, filter by nonce/direction, and sort."""
        all_events: list[EventEnvelope[DomainEvent]] = []
        for stream_events in self._streams.values():
            all_events.extend(stream_events)

        if forward:
            filtered = [
                e for e in all_events
                if e.metadata.global_nonce is not None
                and e.metadata.global_nonce >= from_global_nonce
            ]
            return sorted(filtered, key=lambda e: e.metadata.global_nonce or 0)

        filtered = [
            e for e in all_events
            if e.metadata.global_nonce is not None
            and e.metadata.global_nonce <= from_global_nonce
        ]
        return sorted(filtered, key=lambda e: e.metadata.global_nonce or 0, reverse=True)

    @staticmethod
    def _calculate_next_position(
        page: list[EventEnvelope[DomainEvent]],
        from_global_nonce: int,
        forward: bool,
    ) -> int:
        """Next global nonce: one past the page's last event, either direction (#403)."""
        if forward:
            if page and page[-1].metadata.global_nonce is not None:
                return page[-1].metadata.global_nonce + 1
            return from_global_nonce

        if page and page[-1].metadata.global_nonce is not None:
            return max(0, page[-1].metadata.global_nonce - 1)
        return 0

    async def read_all(
        self,
        from_global_nonce: int = 0,
        max_count: int = 100,
        forward: bool = True,
    ) -> tuple[list[EventEnvelope[DomainEvent]], bool, int]:
        """
        Read all events from a global position (for projections/catch-up).

        Args:
            from_global_nonce: Global nonce to read from (inclusive)
            max_count: Maximum number of events to return per page
            forward: Direction (True = ascending order)

        Returns:
            Tuple of (events, is_end, next_from_global_nonce)
        """
        sorted_events = self._filter_and_sort_events(from_global_nonce, forward)
        limit = min(max_count, 1000) if max_count > 0 else 100  # like the store
        page = sorted_events[:limit]
        is_end = len(sorted_events) <= limit  # nothing remains after this page
        next_from = self._calculate_next_position(page, from_global_nonce, forward)
        return page, is_end, next_from

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
            limit: Maximum number of events to return

        Returns:
            List of event envelopes in global order
        """
        # Use the new read_all method with from_global_nonce = after_global_nonce + 1
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

        For the memory client, this polls for new events every 100ms.
        This is suitable for testing but not for production.

        Args:
            from_global_nonce: global nonce to start from (inclusive)

        Yields:
            EventEnvelope objects as they arrive
        """
        current_nonce = from_global_nonce
        logger.debug(f"Memory subscription starting from global nonce {from_global_nonce}")

        while True:
            # Read events from current position using read_all directly
            events, _is_end, _next_pos = await self.read_all(
                from_global_nonce=current_nonce,
                max_count=100,
                forward=True,
            )

            for event in events:
                if event.metadata.global_nonce is not None:
                    if event.metadata.global_nonce >= current_nonce:
                        yield event
                        current_nonce = event.metadata.global_nonce + 1

            # Poll interval
            await asyncio.sleep(0.1)
