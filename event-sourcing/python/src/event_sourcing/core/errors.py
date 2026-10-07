"""Error types for the event sourcing SDK."""

from __future__ import annotations

# Covers all actual error detail values in the codebase.
# Each error subclass stores str, int, or list[str] — never arbitrary objects.
ErrorDetails = dict[str, str | int | list[str]]


class EventSourcingError(Exception):
    """Base class for all event sourcing errors."""

    def __init__(self, message: str, details: ErrorDetails | None = None) -> None:
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


class StreamAlreadyExistsError(ConcurrencyConflictError):
    """Raised when appending with ExpectedVersion.NO_STREAM to an existing stream.

    This is the specific error for set-based validation via the
    stream-per-unique-value pattern. Catching this error (rather than
    the broader ``ConcurrencyConflictError``) lets callers distinguish
    "duplicate creation" from "stale read" conflicts.
    """

    def __init__(self, stream_name: str, actual_version: int) -> None:
        super().__init__(expected_version=0, actual_version=actual_version)
        self.stream_name = stream_name
        self.message = f"Stream '{stream_name}' already exists (version: {actual_version})"


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
        details: ErrorDetails = {"command_type": command_type, "validation_errors": validation_errors}
        super().__init__(
            f"Command validation failed for {command_type}: {', '.join(validation_errors)}",
            details,
        )
        self.command_type = command_type
        self.validation_errors = validation_errors


class EventStoreError(EventSourcingError):
    """Raised when an event store operation fails."""

    def __init__(self, message: str, original_error: Exception | None = None) -> None:
        details: ErrorDetails = {}
        if original_error:
            details["original_error"] = str(original_error)
            details["original_type"] = type(original_error).__name__
        super().__init__(f"Event store error: {message}", details)
        self.original_error = original_error


class UndecodableEventError(EventStoreError):
    """A stored event cannot be decoded by the event store (gRPC DATA_LOSS).

    Retrying does not help: an operator must repair the row or explicitly move
    consumer checkpoints past ``global_nonce`` (see ADR-026).
    """

    def __init__(
        self, global_nonce: int, message: str, original_error: Exception | None = None
    ) -> None:
        super().__init__(message, original_error)
        self.global_nonce = global_nonce
        self.details["global_nonce"] = global_nonce


ADR_026_PATH = "docs/adrs/ADR-026-subscription-failure-semantics.md"


class SubscriptionHaltedError(EventSourcingError):
    """A subscription stopped at an undecodable stored event and will not retry.

    Raised by ``SubscriptionCoordinator.start()`` (and exposed as its
    ``halted`` health state) when the event store reports gRPC DATA_LOSS for
    the event at ``global_nonce``. Retrying reconnects from the same
    checkpoint and fails at the same position, so the coordinator stops
    instead. No checkpoint is moved past ``global_nonce``.

    Operator recovery (ADR-026): repair the row, deploy an event store that
    decodes it, or set the checkpoint of every projection that has not passed
    ``global_nonce`` to ``global_nonce``. Then start the coordinator again.
    The original ``UndecodableEventError`` is the ``__cause__``.
    """

    def __init__(self, global_nonce: int, recheck_interval: float | None = None) -> None:
        resume = (
            f"the coordinator re-checks every {recheck_interval:g}s"
            if recheck_interval is not None
            else "then start the coordinator again"
        )
        super().__init__(
            f"Subscription halted at undecodable stored event global_nonce={global_nonce} "
            "(gRPC DATA_LOSS); retrying cannot fix this, so it will not retry. "
            "Operator recovery (ADR-026, "
            f"{ADR_026_PATH}): repair the row or deploy an event store that decodes it, "
            "or, if unrecoverable, set the checkpoint of every projection that has not "
            f"passed {global_nonce} to {global_nonce} (it resumes at {global_nonce + 1}); "
            f"{resume}.",
            {"global_nonce": global_nonce},
        )
        self.global_nonce = global_nonce


class SerializationError(EventSourcingError):
    """Raised when serialization/deserialization fails."""

    def __init__(
        self, operation: str, data_type: str, original_error: Exception | None = None
    ) -> None:
        details: ErrorDetails = {"operation": operation, "data_type": data_type}
        if original_error:
            details["original_error"] = str(original_error)
        super().__init__(f"Failed to {operation} {data_type}", details)
        self.operation = operation
        self.data_type = data_type
        self.original_error = original_error


class EventDecodeError(UndecodableEventError):
    """A stored event cannot be decoded by this reader (ADR-027).

    Raised on read (``read_events``, ``read_all``, ``subscribe``) instead of
    handing the event to code written for another schema. It is an
    ``UndecodableEventError``: retrying does not help, so a
    ``SubscriptionCoordinator`` halts at ``global_nonce`` rather than retrying
    or moving a checkpoint past the event. Fix it in code (register the event
    class or an upcaster), then restart.

    ``event_type`` and ``event_version`` are as stored (version 0 read as 1),
    or as produced by the upcaster chain when a later stage failed.
    """

    def __init__(
        self,
        event_type: str,
        event_version: int,
        reason: str,
        global_nonce: int = 0,
        original_error: Exception | None = None,
    ) -> None:
        super().__init__(
            global_nonce,
            f"Cannot decode event '{event_type}' v{event_version}: {reason}",
            original_error,
        )
        self.event_type = event_type
        self.event_version = event_version
        self.reason = reason
        self.details["event_type"] = event_type
        self.details["event_version"] = event_version


class UnknownEventTypeError(EventDecodeError):
    """No event class is registered for ``event_type`` (strict decode only).

    The gRPC client does not raise this: it returns an unregistered type as a
    ``GenericDomainEvent`` (ADR-023) so projections can filter by type.
    """


class UnknownEventVersionError(EventDecodeError):
    """``event_type`` is registered, but not at this version, and no upcaster
    maps the stored version to a registered one."""


class EventPayloadError(EventDecodeError):
    """The payload is not a JSON object (or not JSON), or the class registered
    for its ``(event_type, event_version)`` rejects it."""


class UnsupportedContentTypeError(EventDecodeError):
    """The stored ``content_type`` is neither empty nor ``application/json``."""


class UpcastError(EventDecodeError):
    """An upcaster step raised or did not return a dict, or the chain cycled."""
