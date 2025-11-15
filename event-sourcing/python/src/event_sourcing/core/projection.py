"""
Projection support for building read models from event streams.

Projections transform event streams into queryable read models optimized for specific use cases.
"""

from abc import ABC, abstractmethod
from typing import Any, Generic, TypeVar

from event_sourcing.core.event import DomainEvent, EventEnvelope

E = TypeVar("E", bound=DomainEvent)


class Projection(ABC, Generic[E]):
    """
    Base class for event projections that build read models.

    Projections are the "read side" of CQRS - they consume events and build
    denormalized views optimized for queries.

    Example:
        class OrderSummaryProjection(Projection):
            def __init__(self):
                self.orders = {}

            def get_name(self) -> str:
                return "OrderSummary"

            async def handle_event(self, envelope: EventEnvelope) -> None:
                if envelope.event.event_type == "OrderPlaced":
                    self.orders[envelope.event.order_id] = {
                        "id": envelope.event.order_id,
                        "status": "placed",
                        "total": envelope.event.total_amount
                    }
    """

    @abstractmethod
    async def handle_event(self, envelope: EventEnvelope[E]) -> None:
        """
        Handle an event and update the projection's read model.

        Args:
            envelope: Event envelope containing the event and metadata
        """
        pass

    @abstractmethod
    def get_name(self) -> str:
        """
        Get the projection name (used for identification and tracking).

        Returns:
            Unique name for this projection
        """
        pass

    def get_version(self) -> int:
        """
        Get the projection version for schema tracking.

        Increment this when the projection schema changes to trigger rebuilds.

        Returns:
            Projection version number (default: 1)
        """
        return 1


class ProjectionManager:
    """
    Manages multiple projections and coordinates event dispatch.

    The ProjectionManager maintains a registry of projections and dispatches
    events to all registered projections.

    Example:
        manager = ProjectionManager()
        manager.register(OrderSummaryProjection())
        manager.register(SalesAnalyticsProjection())

        # Dispatch event to all projections
        await manager.dispatch(event_envelope)
    """

    def __init__(self) -> None:
        """Initialize the projection manager."""
        self.projections: dict[str, Projection[Any]] = {}

    def register(self, projection: Projection[Any]) -> None:
        """
        Register a projection with the manager.

        Args:
            projection: Projection instance to register

        Raises:
            ValueError: If a projection with the same name is already registered
        """
        name = projection.get_name()
        if name in self.projections:
            raise ValueError(f"Projection '{name}' is already registered")

        self.projections[name] = projection

    def unregister(self, name: str) -> None:
        """
        Unregister a projection by name.

        Args:
            name: Name of the projection to unregister
        """
        self.projections.pop(name, None)

    async def dispatch(self, envelope: EventEnvelope[Any]) -> None:
        """
        Dispatch an event to all registered projections.

        Args:
            envelope: Event envelope to dispatch
        """
        for projection in self.projections.values():
            try:
                await projection.handle_event(envelope)
            except Exception as e:
                # Log error but don't stop other projections
                # In production, you'd want proper error handling/logging
                print(f"Error in projection {projection.get_name()}: {e}")

    def get_projection(self, name: str) -> Projection[Any] | None:
        """
        Get a registered projection by name.

        Args:
            name: Name of the projection

        Returns:
            Projection instance or None if not found
        """
        return self.projections.get(name)

    def get_all_projections(self) -> dict[str, Projection[Any]]:
        """
        Get all registered projections.

        Returns:
            Dictionary mapping projection names to instances
        """
        return self.projections.copy()


class AutoDispatchProjection(Projection[E]):
    """
    Base class for projections with automatic event routing based on event type.

    Subclasses can define handler methods named `on_{event_type}` and they will
    be automatically called when matching events are received.

    Example:
        class OrderProjection(AutoDispatchProjection):
            def __init__(self):
                super().__init__()
                self.orders = {}

            def get_name(self) -> str:
                return "Orders"

            async def on_OrderPlaced(self, envelope: EventEnvelope) -> None:
                # Automatically called for "OrderPlaced" events
                event = envelope.event
                self.orders[event.order_id] = {...}

            async def on_OrderShipped(self, envelope: EventEnvelope) -> None:
                # Automatically called for "OrderShipped" events
                ...
    """

    async def handle_event(self, envelope: EventEnvelope[E]) -> None:
        """
        Automatically dispatch event to handler method based on event type.

        Looks for a method named `on_{event_type}` and calls it if found.

        Args:
            envelope: Event envelope containing the event
        """
        event_type = envelope.event.event_type
        handler_name = f"on_{event_type}"

        handler = getattr(self, handler_name, None)
        if handler and callable(handler):
            await handler(envelope)
