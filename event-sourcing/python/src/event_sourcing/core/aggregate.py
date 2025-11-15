"""Aggregate abstractions and base implementations for event sourcing."""

import logging
from abc import ABC, abstractmethod
from collections.abc import Callable
from typing import Any, Generic, TypeVar

from event_sourcing.core.errors import InvalidAggregateStateError
from event_sourcing.core.event import DomainEvent, EventEnvelope, EventFactory

logger = logging.getLogger(__name__)

TEvent = TypeVar("TEvent", bound=DomainEvent)


class BaseAggregate(ABC, Generic[TEvent]):
    """
    Base class for event-sourced aggregates.

    Provides the foundational lifecycle methods for aggregates:
    - Managing aggregate ID and version
    - Tracking uncommitted events
    - Rehydrating from event history

    Subclasses must implement:
    - get_aggregate_type(): Return the aggregate type name
    - apply_event(): Apply an event to update state
    """

    def __init__(self) -> None:
        self._id: str | None = None
        self._version: int = 0
        self._uncommitted_events: list[EventEnvelope[TEvent]] = []

    @property
    def id(self) -> str | None:
        """Get the aggregate ID."""
        return self._id

    @property
    def version(self) -> int:
        """Get the current version (event count)."""
        return self._version

    @abstractmethod
    def get_aggregate_type(self) -> str:
        """
        Get the aggregate type name.

        Returns:
            The type name for this aggregate (e.g., "Order", "Account")
        """
        ...

    @abstractmethod
    def apply_event(self, event: TEvent) -> None:
        """
        Apply an event to update the aggregate's state.

        This method should be pure and deterministic - given the same
        sequence of events, it should always produce the same state.

        Args:
            event: The event to apply
        """
        ...

    def get_uncommitted_events(self) -> list[EventEnvelope[TEvent]]:
        """Get all uncommitted events."""
        return self._uncommitted_events.copy()

    def mark_events_as_committed(self) -> None:
        """Clear uncommitted events after they've been persisted."""
        self._uncommitted_events.clear()

    def has_uncommitted_events(self) -> bool:
        """Check if there are any uncommitted events."""
        return len(self._uncommitted_events) > 0

    def _initialize(self, aggregate_id: str) -> None:
        """
        Initialize the aggregate with an ID.

        Args:
            aggregate_id: The unique identifier for this aggregate

        Raises:
            InvalidAggregateStateError: If trying to change existing ID
        """
        if self._id is not None and self._id != aggregate_id:
            raise InvalidAggregateStateError(
                self.get_aggregate_type(),
                f"Cannot change aggregate ID from {self._id} to {aggregate_id}",
            )
        self._id = aggregate_id
        self._version = 0

    def _raise_event(self, event: TEvent) -> None:
        """
        Raise and apply a new event.

        Creates an envelope, applies the event to state, increments version,
        and adds to uncommitted events.

        Args:
            event: The event to raise

        Raises:
            InvalidAggregateStateError: If aggregate has no ID
        """
        if self._id is None:
            raise InvalidAggregateStateError(
                self.get_aggregate_type(), "Cannot raise events on an aggregate without an ID"
            )

        # Create envelope with metadata
        envelope = EventFactory.create(
            event=event,
            aggregate_id=self._id,
            aggregate_type=self.get_aggregate_type(),
            aggregate_nonce=self._version + 1,
        )

        # Apply event to update state
        self.apply_event(event)

        # Increment version
        self._version += 1

        # Track uncommitted
        self._uncommitted_events.append(envelope)

    def rehydrate(self, events: list[EventEnvelope[TEvent]]) -> None:
        """
        Load the aggregate from an event history.

        Resets the aggregate state and replays all events in order.

        Args:
            events: The event history to replay
        """
        self._uncommitted_events.clear()
        self._id = None
        self._version = 0

        for envelope in events:
            if self._id is None:
                self._id = envelope.metadata.aggregate_id

            self.apply_event(envelope.event)
            self._version = envelope.metadata.aggregate_nonce


class AggregateRoot(BaseAggregate[TEvent]):
    """
    Production-ready aggregate base class with automatic event dispatching.

    This is the main class that aggregates should extend in production.

    Features:
    - Automatic event dispatching via @event_sourcing_handler decorators
    - Command handling via @command_handler decorators
    - Full event sourcing lifecycle support

    Example:
        class OrderAggregate(AggregateRoot[OrderEvent]):
            def __init__(self) -> None:
                super().__init__()
                self.status = "NEW"

            def get_aggregate_type(self) -> str:
                return "Order"

            @command_handler("PlaceOrder")
            def place_order(self, cmd: PlaceOrderCommand) -> None:
                self._raise_event(OrderPlaced(...))

            @event_sourcing_handler("OrderPlaced")
            def on_order_placed(self, event: OrderPlaced) -> None:
                self.status = "PLACED"
    """

    @property
    def aggregate_id(self) -> str | None:
        """Get the aggregate identifier (alias for id)."""
        return self.id

    def apply_event(self, event: TEvent) -> None:
        """
        Apply event using automatic method dispatch.

        Automatically routes events to methods decorated with @event_sourcing_handler
        based on the event type.

        Args:
            event: The event to apply
        """
        handler_map = self._get_event_handlers()

        # Get event type from the event
        event_type = self._get_event_type(event)

        if event_type in handler_map:
            handler = handler_map[event_type]
            handler(self, event)
        else:
            self._handle_unknown_event(event)

    def _handle_unknown_event(self, event: TEvent) -> None:
        """
        Handle unknown events.

        Default behavior is to log a warning. Can be overridden by subclasses
        to implement custom handling.

        Args:
            event: The unknown event
        """
        event_type = self._get_event_type(event)
        logger.warning(f"No handler found for event type: {event_type}")

    def _apply(self, event: TEvent) -> None:
        """
        Apply an event (convenience method for use in command handlers).

        This is an alias for _raise_event() to match TypeScript SDK naming.

        Args:
            event: The event to apply
        """
        self._raise_event(event)

    @classmethod
    def _get_event_handlers(cls) -> dict[str, Callable[[Any, TEvent], None]]:
        """
        Get event handler map from decorated methods.

        Scans the class for methods decorated with @event_sourcing_handler
        and builds a mapping of event types to handler methods.

        Returns:
            Dictionary mapping event types to handler methods
        """
        handlers: dict[str, Callable[[Any, TEvent], None]] = {}

        # Scan all attributes of the class
        for name in dir(cls):
            try:
                attr = getattr(cls, name)
                if callable(attr) and hasattr(attr, "_event_type"):
                    event_type = attr._event_type
                    handlers[event_type] = attr
            except AttributeError:
                # Skip attributes that can't be accessed
                continue

        return handlers

    @classmethod
    def _get_command_handlers(cls) -> dict[str, str]:
        """
        Get command handler map from decorated methods.

        Scans the class for methods decorated with @command_handler
        and builds a mapping of command types to method names.

        Returns:
            Dictionary mapping command types to method names
        """
        handlers: dict[str, str] = {}

        for name in dir(cls):
            try:
                attr = getattr(cls, name)
                if callable(attr) and hasattr(attr, "_command_type"):
                    command_type = attr._command_type
                    handlers[command_type] = name
            except AttributeError:
                continue

        return handlers

    def _handle_command(self, command: Any) -> None:
        """
        Handle a command by dispatching to the appropriate @command_handler method.

        Args:
            command: The command to handle

        Raises:
            ValueError: If no handler found for command type
        """
        command_type = type(command).__name__
        handlers = self._get_command_handlers()

        if command_type not in handlers:
            raise ValueError(
                f"No @command_handler found for command type: {command_type} "
                f"on aggregate {self.get_aggregate_type()}"
            )

        method_name = handlers[command_type]
        handler = getattr(self, method_name)

        if not callable(handler):
            raise ValueError(
                f"Command handler method '{method_name}' is not callable "
                f"on aggregate {self.get_aggregate_type()}"
            )

        # Invoke the command handler
        handler(command)

    @staticmethod
    def _get_event_type(event: TEvent) -> str:
        """
        Get the event type from an event.

        Args:
            event: The event

        Returns:
            The event type string
        """
        # Try to get event_type from the event object
        if hasattr(event, "event_type"):
            event_type_attr = event.event_type
            if isinstance(event_type_attr, str):
                return event_type_attr

        # Fallback to class name
        return type(event).__name__
