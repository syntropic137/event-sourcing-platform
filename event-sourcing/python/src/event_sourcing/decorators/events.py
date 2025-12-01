"""Event decorators for event sourcing patterns."""

import re
from collections.abc import Callable
from typing import Any, TypeVar

F = TypeVar("F", bound=Callable[..., Any])
T = TypeVar("T", bound=type[Any])

# ============================================================================
# EVENT HANDLER DECORATOR (for aggregate methods)
# ============================================================================


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


# ============================================================================
# EVENT CLASS DECORATOR (ADR-010)
# ============================================================================

# Metadata key for event decorator
EVENT_METADATA_KEY = "_event_metadata"

# Pre-compiled regex patterns for version validation (compiled once at module load)
_SIMPLE_VERSION_PATTERN = re.compile(r"^v\d+$")
_SEMVER_PATTERN = re.compile(r"^\d+\.\d+\.\d+$")


class EventDecoratorMetadata:
    """Metadata stored by @event decorator."""

    __slots__ = ("event_type", "version")

    def __init__(self, event_type: str, version: str) -> None:
        self.event_type = event_type
        self.version = version


def _is_valid_event_version(version: str) -> bool:
    """
    Validate event version format.

    Supports two formats:
    - Simple: "v1", "v2", "v3", etc. (v followed by integer)
    - Semantic: "1.0.0", "2.1.3", etc. (major.minor.patch)

    Args:
        version: The version string to validate

    Returns:
        True if valid, False otherwise
    """
    if _SIMPLE_VERSION_PATTERN.match(version):
        return True

    if _SEMVER_PATTERN.match(version):
        return True

    return False


def event(event_type: str, version: str) -> Callable[[T], T]:
    """
    Decorator for event classes to store metadata about event type and version.

    This enables the VSA CLI to discover and validate events automatically.

    Args:
        event_type: The event type identifier (e.g., "TaskCreated")
        version: The event version. Must be either:
                 - Simple format: "v1", "v2", "v3", etc. (recommended)
                 - Semantic format: "1.0.0", "2.1.3", etc. (advanced)

    Raises:
        ValueError: If version format is invalid

    Example (simple versioning - recommended):
        @event("TaskCreated", "v1")
        class TaskCreatedEvent(DomainEvent):
            task_id: str
            title: str

    Example (semantic versioning - advanced):
        @event("TaskCreated", "2.0.0")
        class TaskCreatedEventV2(DomainEvent):
            task_id: str
            title: str
            description: str  # New field in v2

    See Also:
        - ADR-007: Event Versioning and Upcasters
        - ADR-010: Decorator Patterns for Framework Integration
    """

    def decorator(cls: T) -> T:
        # Validate version format
        if not _is_valid_event_version(version):
            msg = (
                f'Invalid event version format: "{version}" for event "{event_type}". '
                f'Version must be either simple format (e.g., "v1", "v2") or '
                f'semantic format (e.g., "1.0.0", "2.1.3"). '
                f"See ADR-007 for event versioning guidelines."
            )
            raise ValueError(msg)

        # Store metadata on the class
        metadata = EventDecoratorMetadata(event_type=event_type, version=version)
        setattr(cls, EVENT_METADATA_KEY, metadata)

        # Validate and set event_type class attribute for DomainEvent compatibility
        existing_event_type = getattr(cls, "event_type", None)
        if existing_event_type is not None and existing_event_type != event_type:
            msg = (
                f"event_type mismatch in {cls.__name__}: "
                f'decorator parameter "{event_type}" does not match '
                f'class attribute "{existing_event_type}"'
            )
            raise ValueError(msg)
        cls.event_type = event_type

        return cls

    return decorator


def get_event_metadata(event_class: type[Any]) -> EventDecoratorMetadata | None:
    """
    Get event metadata from an event class.

    Args:
        event_class: The event class to get metadata from

    Returns:
        EventDecoratorMetadata if decorated with @event, None otherwise
    """
    return getattr(event_class, EVENT_METADATA_KEY, None)
