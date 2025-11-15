"""Event handler decorators."""

from collections.abc import Callable
from typing import Any, TypeVar

F = TypeVar("F", bound=Callable[..., Any])


def event_sourcing_handler(event_type: str) -> Callable[[F], F]:
    """
    Decorator for event handler methods in aggregates.

    Marks a method as an event handler that should be invoked when
    an event of the specified type is applied to the aggregate.

    Example:
        class OrderAggregate(AggregateRoot):
            @event_sourcing_handler("OrderPlaced")
            def on_order_placed(self, event: OrderPlaced) -> None:
                self.status = "PLACED"

    Args:
        event_type: The type of event this handler processes

    Returns:
        Decorated method with event_type metadata attached
    """

    def decorator(func: F) -> F:
        # Attach metadata to the function
        func._event_type = event_type  # type: ignore[attr-defined]
        return func

    return decorator
