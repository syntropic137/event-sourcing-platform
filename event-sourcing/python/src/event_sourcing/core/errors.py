"""Error types for the event sourcing SDK."""

from typing import Any


class EventSourcingError(Exception):
    """Base class for all event sourcing errors."""

    def __init__(self, message: str, details: dict[str, Any] | None = None) -> None:
        super().__init__(message)
        self.message = message
        self.details = details or {}
        self.code = self.__class__.__name__


class AggregateNotFoundError(EventSourcingError):
    """Raised when an aggregate is not found."""

    def __init__(self, aggregate_type: str, aggregate_id: str) -> None:
        super().__init__(
            f"Aggregate not found: {aggregate_type}:{aggregate_id}",
            {"aggregate_type": aggregate_type, "aggregate_id": aggregate_id},
        )
        self.aggregate_type = aggregate_type
        self.aggregate_id = aggregate_id


class ConcurrencyConflictError(EventSourcingError):
    """Raised when a concurrency conflict is detected."""

    def __init__(self, expected_version: int, actual_version: int) -> None:
        super().__init__(
            f"Concurrency conflict: expected version {expected_version}, got {actual_version}",
            {"expected_version": expected_version, "actual_version": actual_version},
        )
        self.expected_version = expected_version
        self.actual_version = actual_version


class InvalidAggregateStateError(EventSourcingError):
    """Raised when an aggregate is in an invalid state."""

    def __init__(self, aggregate_type: str, reason: str) -> None:
        super().__init__(
            f"Invalid aggregate state for {aggregate_type}: {reason}",
            {"aggregate_type": aggregate_type, "reason": reason},
        )
        self.aggregate_type = aggregate_type
        self.reason = reason


class CommandValidationError(EventSourcingError):
    """Raised when a command fails validation."""

    def __init__(self, command_type: str, validation_errors: list[str]) -> None:
        super().__init__(
            f"Command validation failed for {command_type}: {', '.join(validation_errors)}",
            {"command_type": command_type, "validation_errors": validation_errors},
        )
        self.command_type = command_type
        self.validation_errors = validation_errors


class EventStoreError(EventSourcingError):
    """Raised when an event store operation fails."""

    def __init__(self, message: str, original_error: Exception | None = None) -> None:
        details = {}
        if original_error:
            details["original_error"] = str(original_error)
            details["original_type"] = type(original_error).__name__
        super().__init__(f"Event store error: {message}", details)
        self.original_error = original_error


class SerializationError(EventSourcingError):
    """Raised when serialization/deserialization fails."""

    def __init__(
        self, operation: str, data_type: str, original_error: Exception | None = None
    ) -> None:
        details = {"operation": operation, "data_type": data_type}
        if original_error:
            details["original_error"] = str(original_error)
        super().__init__(f"Failed to {operation} {data_type}", details)
        self.operation = operation
        self.data_type = data_type
        self.original_error = original_error

