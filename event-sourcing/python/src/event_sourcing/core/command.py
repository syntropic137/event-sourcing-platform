"""Command handling patterns and abstractions."""

from abc import ABC, abstractmethod
from typing import Protocol


class Command(Protocol):
    """
    Base protocol for commands.

    Commands represent intentions to change state and typically
    target a specific aggregate.
    """

    @property
    def aggregate_id(self) -> str:
        """Get the ID of the aggregate this command targets."""
        ...


class CommandHandler(Protocol):
    """Protocol for command handlers."""

    async def handle(self, command: Command) -> None:
        """
        Handle a command.

        Args:
            command: The command to handle
        """
        ...


class CommandBus(ABC):
    """Abstract base class for command buses."""

    @abstractmethod
    async def send(self, command: Command) -> None:
        """
        Send a command to its handler.

        Args:
            command: The command to send
        """
        ...

    @abstractmethod
    def register_handler(self, command_type: type[Command], handler: CommandHandler) -> None:
        """
        Register a command handler.

        Args:
            command_type: The command type
            handler: The handler for this command type
        """
        ...


class InMemoryCommandBus(CommandBus):
    """Simple in-memory command bus implementation."""

    def __init__(self) -> None:
        self._handlers: dict[str, CommandHandler] = {}

    async def send(self, command: Command) -> None:
        """Send a command to its registered handler."""
        command_type = type(command).__name__

        handler = self._handlers.get(command_type)
        if handler is None:
            raise ValueError(f"No handler registered for command type: {command_type}")

        await handler.handle(command)

    def register_handler(self, command_type: type[Command], handler: CommandHandler) -> None:
        """Register a command handler."""
        type_name = command_type.__name__
        self._handlers[type_name] = handler
