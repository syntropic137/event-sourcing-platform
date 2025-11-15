"""Repository pattern for loading and saving aggregates."""

import logging
from collections.abc import Callable
from typing import Any, Generic, Protocol, TypeVar

from event_sourcing.core.aggregate import BaseAggregate

logger = logging.getLogger(__name__)

TAggregate = TypeVar("TAggregate", bound=BaseAggregate[Any])


class Repository(Protocol, Generic[TAggregate]):
    """Repository interface for aggregate persistence."""

    async def load(self, aggregate_id: str) -> TAggregate | None:
        """
        Load an aggregate by ID.

        Args:
            aggregate_id: The aggregate ID

        Returns:
            The loaded aggregate or None if not found
        """
        ...

    async def save(self, aggregate: TAggregate) -> None:
        """
        Save an aggregate with optimistic concurrency control.

        Args:
            aggregate: The aggregate to save

        Raises:
            ConcurrencyConflictError: If version conflict detected
        """
        ...

    async def exists(self, aggregate_id: str) -> bool:
        """
        Check if an aggregate exists.

        Args:
            aggregate_id: The aggregate ID

        Returns:
            True if the aggregate exists
        """
        ...


class EventStoreRepository(Generic[TAggregate]):
    """
    Event store implementation of the repository pattern.

    Loads aggregates by replaying their event history and saves them
    by appending new events to the stream.
    """

    def __init__(
        self,
        event_store_client: Any,  # EventStoreClient
        aggregate_factory: Callable[[], TAggregate],
        aggregate_type: str,
    ) -> None:
        """
        Initialize the repository.

        Args:
            event_store_client: Event store client for persistence
            aggregate_factory: Factory function to create new aggregate instances
            aggregate_type: The aggregate type name (used in stream names)
        """
        self._client = event_store_client
        self._aggregate_factory = aggregate_factory
        self._aggregate_type = aggregate_type

    def _get_stream_name(self, aggregate_id: str) -> str:
        """Get stream name for an aggregate."""
        return f"{self._aggregate_type}-{aggregate_id}"

    async def load(self, aggregate_id: str) -> TAggregate | None:
        """
        Load an aggregate by replaying its event history.

        Args:
            aggregate_id: The aggregate ID

        Returns:
            The loaded aggregate or None if not found
        """
        stream_name = self._get_stream_name(aggregate_id)

        # Read events from the stream
        events = await self._client.read_events(stream_name)

        if not events:
            logger.debug(f"Aggregate not found: {self._aggregate_type}:{aggregate_id}")
            return None

        # Create new aggregate instance
        aggregate = self._aggregate_factory()

        # Rehydrate from events
        aggregate.rehydrate(events)

        logger.debug(
            f"Loaded aggregate {self._aggregate_type}:{aggregate_id} at version {aggregate.version}"
        )

        return aggregate

    async def save(self, aggregate: TAggregate) -> None:
        """
        Save an aggregate by appending uncommitted events.

        Args:
            aggregate: The aggregate to save

        Raises:
            ConcurrencyConflictError: If version conflict detected
            InvalidAggregateStateError: If aggregate has no ID
        """
        if aggregate.id is None:
            from event_sourcing.core.errors import InvalidAggregateStateError

            raise InvalidAggregateStateError(
                self._aggregate_type, "Cannot save aggregate without ID"
            )

        # Get uncommitted events
        uncommitted_events = aggregate.get_uncommitted_events()

        if not uncommitted_events:
            logger.debug(f"No uncommitted events for {self._aggregate_type}:{aggregate.id}")
            return

        stream_name = self._get_stream_name(aggregate.id)

        # Calculate expected version (current version - uncommitted events)
        expected_version = aggregate.version - len(uncommitted_events)

        # Append events with optimistic concurrency
        await self._client.append_events(
            stream_name=stream_name,
            events=uncommitted_events,
            expected_version=expected_version,
        )

        # Mark events as committed
        aggregate.mark_events_as_committed()

        logger.debug(
            f"Saved aggregate {self._aggregate_type}:{aggregate.id} "
            f"with {len(uncommitted_events)} event(s), new version: {aggregate.version}"
        )

    async def exists(self, aggregate_id: str) -> bool:
        """Check if an aggregate exists."""
        stream_name = self._get_stream_name(aggregate_id)
        result: bool = await self._client.stream_exists(stream_name)
        return result


class RepositoryFactory:
    """Factory for creating event store repositories."""

    def __init__(self, event_store_client: Any) -> None:  # EventStoreClient
        """
        Initialize the factory.

        Args:
            event_store_client: Event store client for all repositories
        """
        self._client = event_store_client

    def create_repository(
        self,
        aggregate_factory: type[TAggregate],
        aggregate_type: str | None = None,
    ) -> EventStoreRepository[TAggregate]:
        """
        Create a repository for a specific aggregate type.

        Args:
            aggregate_factory: Aggregate class (used as factory)
            aggregate_type: Optional aggregate type name (defaults to class name)

        Returns:
            EventStoreRepository for the aggregate type
        """
        # Use class name if aggregate_type not provided
        if aggregate_type is None:
            aggregate_type = aggregate_factory.__name__.replace("Aggregate", "")

        # Create factory function
        def factory() -> TAggregate:
            return aggregate_factory()

        return EventStoreRepository(
            event_store_client=self._client,
            aggregate_factory=factory,
            aggregate_type=aggregate_type,
        )
