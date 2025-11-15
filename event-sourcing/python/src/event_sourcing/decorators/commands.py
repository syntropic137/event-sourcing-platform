"""Command handler decorators."""

from collections.abc import Callable
from typing import Any, TypeVar

F = TypeVar("F", bound=Callable[..., Any])


def command_handler(command_type: str) -> Callable[[F], F]:
    """
    Decorator for command handler methods in aggregates.

    Marks a method as a command handler that should be invoked when
    a command of the specified type is dispatched to the aggregate.

    Example:
        class OrderAggregate(AggregateRoot):
            @command_handler("PlaceOrder")
            async def place_order(self, cmd: PlaceOrderCommand) -> None:
                self._raise_event(OrderPlaced(...))

    Args:
        command_type: The type of command this handler processes

    Returns:
        Decorated method with command_type metadata attached
    """

    def decorator(func: F) -> F:
        # Attach metadata to the function
        func._command_type = command_type  # type: ignore[attr-defined]
        return func

    return decorator


def aggregate(aggregate_type: str | None = None) -> Callable[[type[Any]], type[Any]]:
    """
    Decorator for aggregate classes.

    Marks a class as an aggregate and optionally sets its type name.

    Example:
        @aggregate("Order")
        class OrderAggregate(AggregateRoot):
            ...

    Args:
        aggregate_type: Optional aggregate type name (defaults to class name)

    Returns:
        Decorated class with aggregate_type metadata attached
    """

    def decorator(cls: type[Any]) -> type[Any]:
        # Attach metadata to the class
        cls._aggregate_type = aggregate_type or cls.__name__
        return cls

    return decorator
