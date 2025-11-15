"""Decorators for event sourcing patterns."""

from event_sourcing.decorators.commands import command_handler
from event_sourcing.decorators.events import event_sourcing_handler

__all__ = ["event_sourcing_handler", "command_handler"]
