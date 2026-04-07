"""Command decorators for event sourcing patterns."""

from __future__ import annotations

from collections.abc import Callable
from typing import TypeVar

F = TypeVar("F", bound=Callable[..., object])  # OBJRATCHET: decorator preserves any callable signature
T = TypeVar("T", bound=type)

# ============================================================================
# COMMAND HANDLER DECORATOR (for aggregate methods)
# ============================================================================


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


# ============================================================================
# AGGREGATE DECORATOR
# ============================================================================


def aggregate(aggregate_type: str | None = None) -> Callable[[T], T]:
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

    def decorator(cls: T) -> T:
        # Attach metadata to the class
        cls._aggregate_type = aggregate_type or cls.__name__
        return cls

    return decorator


# ============================================================================
# COMMAND CLASS DECORATOR (ADR-010)
# ============================================================================

# Metadata key for command decorator
COMMAND_METADATA_KEY = "_command_metadata"


class CommandDecoratorMetadata:
    """Metadata stored by @command decorator."""

    __slots__ = ("command_type", "description")

    def __init__(self, command_type: str, description: str | None = None) -> None:
        self.command_type = command_type
        self.description = description


def command(command_type: str, description: str | None = None) -> Callable[[T], T]:
    """
    Decorator for command classes to store metadata about command type.

    This enables the VSA CLI to discover and validate commands automatically.

    Args:
        command_type: The command type identifier (e.g., "CreateTask")
        description: Optional description of what the command does

    Example:
        @command("CreateTask", "Creates a new task in the system")
        class CreateTaskCommand:
            aggregate_id: str
            title: str

    Example (without description):
        @command("CreateTask")
        class CreateTaskCommand:
            aggregate_id: str
            title: str

    See Also:
        - ADR-006: Domain Organization Pattern
        - ADR-010: Decorator Patterns for Framework Integration
    """

    def decorator(cls: T) -> T:
        # Store metadata on the class
        metadata = CommandDecoratorMetadata(
            command_type=command_type,
            description=description,
        )
        setattr(cls, COMMAND_METADATA_KEY, metadata)

        return cls

    return decorator


def get_command_metadata(command_class: type[object]) -> CommandDecoratorMetadata | None:  # OBJRATCHET: accepts any class for metadata inspection
    """
    Get command metadata from a command class.

    Args:
        command_class: The command class to get metadata from

    Returns:
        CommandDecoratorMetadata if decorated with @command, None otherwise
    """
    return getattr(command_class, COMMAND_METADATA_KEY, None)
