"""Decorators for event sourcing patterns."""

from event_sourcing.decorators.commands import (
    CommandDecoratorMetadata,
    aggregate,
    command,
    command_handler,
    get_command_metadata,
)
from event_sourcing.decorators.events import (
    EventDecoratorMetadata,
    event,
    event_sourcing_handler,
    get_event_metadata,
)

__all__ = [
    # Method decorators (for aggregate methods)
    "command_handler",
    "event_sourcing_handler",
    # Class decorators (for aggregate, command, event classes)
    "aggregate",
    "command",
    "event",
    # Metadata types and helpers
    "CommandDecoratorMetadata",
    "EventDecoratorMetadata",
    "get_command_metadata",
    "get_event_metadata",
]
