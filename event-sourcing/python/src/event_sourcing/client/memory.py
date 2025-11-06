"""In-memory event store client for testing."""

import logging

from event_sourcing.core.errors import ConcurrencyConflictError, EventStoreError
from event_sourcing.core.event import DomainEvent, EventEnvelope

logger = logging.getLogger(__name__)


class MemoryEventStoreClient:
    """
    In-memory implementation of event store client.

    This implementation stores events in memory and provides the same
    interface as the gRPC client, making it ideal for testing.
    """

    def __init__(self) -> None:
        self._streams: dict[str, list[EventEnvelope[DomainEvent]]] = {}
        self._connected = False

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
            from_version: Optional version to read from (1-based)

        Returns:
            List of event envelopes

        Raises:
            EventStoreError: If stream doesn't exist and from_version is specified
        """
        if stream_name not in self._streams:
            if from_version is not None:
                raise EventStoreError(f"Stream not found: {stream_name}")
            return []

        events = self._streams[stream_name]

        if from_version is not None:
            # from_version is 1-based, so we need to convert to 0-based index
            start_index = from_version
            events = events[start_index:]

        return list(events)  # Return copy to prevent external modification

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
            expected_version: Expected current version (0 means stream must be new)

        Raises:
            ConcurrencyConflictError: If version mismatch detected
        """
        if not events:
            return

        # Get current version (number of events in stream)
        current_version = len(self._streams.get(stream_name, []))

        # Check expected version if provided
        if expected_version is not None:
            if current_version != expected_version:
                raise ConcurrencyConflictError(
                    expected_version=expected_version,
                    actual_version=current_version,
                )

        # Create stream if it doesn't exist
        if stream_name not in self._streams:
            self._streams[stream_name] = []

        # Append events
        self._streams[stream_name].extend(events)

        logger.debug(
            f"Appended {len(events)} event(s) to stream '{stream_name}' "
            f"(new version: {len(self._streams[stream_name])})"
        )

    async def stream_exists(self, stream_name: str) -> bool:
        """Check if a stream exists."""
        return stream_name in self._streams and len(self._streams[stream_name]) > 0

    def clear(self) -> None:
        """Clear all streams (useful for tests)."""
        self._streams.clear()

    def get_stream_version(self, stream_name: str) -> int:
        """Get the current version of a stream (for testing)."""
        return len(self._streams.get(stream_name, []))

