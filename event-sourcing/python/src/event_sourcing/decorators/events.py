"""Event decorators for event sourcing patterns."""

from __future__ import annotations

import re
from collections.abc import Callable
from typing import Any, TypeVar

from event_sourcing.core.event import DomainEvent

F = TypeVar("F", bound=Callable[..., object])  # OBJRATCHET: decorator preserves any callable signature
T = TypeVar("T", bound=type)

# ============================================================================
# EVENT TYPE REGISTRY (ADR-023)
# ============================================================================

# Global registry mapping event_type strings to concrete DomainEvent subclasses.
# Populated automatically by the @event decorator at import time.
# Consulted by GrpcEventStoreClient._proto_to_envelope() to resolve concrete
# types from the wire format instead of always falling back to GenericDomainEvent.
_EVENT_TYPE_REGISTRY: dict[str, type[DomainEvent]] = {}

# event_type -> schema_version -> class (ADR-027). Readers decode by
# (event_type, event_version), so every registered version is kept.
_EVENT_VERSION_REGISTRY: dict[str, dict[int, type[DomainEvent]]] = {}


def get_event_type_registry() -> dict[str, type[DomainEvent]]:
    """Return the global event type registry (read-only snapshot).

    The registry maps event type strings (e.g. ``"WorkflowCreated"``) to their
    concrete ``DomainEvent`` subclass at the highest registered
    ``schema_version``.  Populated automatically by ``@event``.

    Returns:
        A copy of the registry dict.
    """
    return dict(_EVENT_TYPE_REGISTRY)


def resolve_event_type(event_type: str) -> type[DomainEvent] | None:
    """Look up the class registered for ``event_type`` at its highest version.

    Readers decode by ``(event_type, event_version)``: use
    :func:`resolve_event_class` for that.

    Returns:
        The registered class, or ``None`` if not found.
    """
    return _EVENT_TYPE_REGISTRY.get(event_type)


def resolve_event_class(event_type: str, event_version: int) -> type[DomainEvent] | None:
    """Look up the class registered for ``(event_type, event_version)``.

    ``event_version`` is the class's ``schema_version`` (the value written to
    the store's ``event_version``), not the ``@event`` version string.
    """
    return _EVENT_VERSION_REGISTRY.get(event_type, {}).get(event_version)


def registered_event_versions(event_type: str) -> tuple[int, ...]:
    """The ``schema_version`` values registered for ``event_type``, ascending."""
    return tuple(sorted(_EVENT_VERSION_REGISTRY.get(event_type, {})))


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


def _try_register_event_type(event_type: str, cls: type) -> None:
    """Register cls in the event type registry if it's a DomainEvent subclass.

    Keyed by ``(event_type, cls.schema_version)``. A later registration of the
    same pair overwrites the earlier one (ADR-023).
    """
    try:
        if not issubclass(cls, DomainEvent):
            return
    except TypeError:
        return
    version: Any = cls.schema_version  # runtime-checked: subclasses may assign anything
    if isinstance(version, bool) or not isinstance(version, int) or version < 1:
        msg = f"{cls.__name__}.schema_version must be an int >= 1 (ADR-027), got {version!r}"
        raise ValueError(msg)
    by_version = _EVENT_VERSION_REGISTRY.setdefault(event_type, {})
    by_version[version] = cls
    if version == max(by_version):
        _EVENT_TYPE_REGISTRY[event_type] = cls


def event(event_type: str, version: str) -> Callable[[T], T]:
    """
    Decorator for event classes to store metadata about event type and version.

    This enables the VSA CLI to discover and validate events automatically.

    Args:
        event_type: The event type identifier (e.g., "TaskCreated")
        version: The event version. Must be either:
                 - Simple format: "v1", "v2", "v3", etc. (recommended)
                 - Semantic format: "1.0.0", "2.1.3", etc. (advanced)
                 This string is descriptive metadata. The version written to
                 the store (``event_version``) and used to decode is the
                 class's ``schema_version`` ClassVar (default 1), ADR-027.

    Raises:
        ValueError: If version format is invalid, or ``schema_version`` is not
            an int >= 1

    Example (simple versioning - recommended):
        @event("TaskCreated", "v1")
        class TaskCreatedEvent(DomainEvent):
            task_id: str
            title: str

    Example (semantic versioning - advanced):
        @event("TaskCreated", "2.0.0")
        class TaskCreatedEventV2(DomainEvent):
            schema_version: ClassVar[int] = 2  # what is written and decoded
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

        # Auto-register in the global event type registry (ADR-023).
        # Only register DomainEvent subclasses — non-domain classes
        # decorated with @event (e.g. for VSA metadata only) are skipped.
        _try_register_event_type(event_type, cls)

        return cls

    return decorator


def get_event_metadata(event_class: type[object]) -> EventDecoratorMetadata | None:  # OBJRATCHET: accepts any class for metadata inspection
    """
    Get event metadata from an event class.

    Args:
        event_class: The event class to get metadata from

    Returns:
        EventDecoratorMetadata if decorated with @event, None otherwise
    """
    return getattr(event_class, EVENT_METADATA_KEY, None)
