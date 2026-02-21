"""
Projection checkpoint architecture for reliable event processing.

This module provides the core abstractions for checkpointed projections:
- ProjectionCheckpoint: Immutable checkpoint tracking per-projection position
- ProjectionResult: Explicit result type for event handlers
- ProjectionCheckpointStore: Protocol for checkpoint persistence
- CheckpointedProjection: Abstract base class with mandatory checkpoint tracking

See ADR-014 for architectural decision rationale.
"""

import re
from abc import ABC, abstractmethod
from dataclasses import dataclass
from datetime import UTC, datetime
from enum import Enum
from typing import TYPE_CHECKING, Any, Protocol, runtime_checkable

if TYPE_CHECKING:
    from event_sourcing.core.event import EventEnvelope


class ProjectionResult(Enum):
    """
    Result of handling an event in a projection.

    Every event handler MUST return an explicit result to indicate
    whether the checkpoint should advance.

    Values:
        SUCCESS: Event processed successfully, advance checkpoint
        SKIP: Event not relevant to this projection, advance checkpoint
        FAILURE: Error occurred, DO NOT advance checkpoint

    Example:
        async def handle_event(self, envelope, checkpoint_store) -> ProjectionResult:
            if envelope.event.event_type not in self._handlers:
                return ProjectionResult.SKIP

            try:
                await self._process(envelope)
                await checkpoint_store.save_checkpoint(...)
                return ProjectionResult.SUCCESS
            except Exception:
                return ProjectionResult.FAILURE
    """

    SUCCESS = "success"
    SKIP = "skip"
    FAILURE = "failure"


@dataclass(frozen=True)
class ProjectionCheckpoint:
    """
    Immutable checkpoint for tracking a projection's position in the event stream.

    Checkpoints are stored separately from projection data to enable:
    - Independent position tracking per projection
    - Atomic updates with projection data (same transaction)
    - Easy rebuild by deleting checkpoint and data

    Attributes:
        projection_name: Unique identifier for the projection
        global_position: Last successfully processed global nonce
        updated_at: Timestamp of last checkpoint update
        version: Projection schema version (increment to trigger rebuild)

    Example:
        checkpoint = ProjectionCheckpoint(
            projection_name="order_summary",
            global_position=42,
            updated_at=datetime.now(UTC),
            version=1,
        )
    """

    projection_name: str
    global_position: int
    updated_at: datetime
    version: int = 1

    def __post_init__(self) -> None:
        """Validate checkpoint values."""
        if not self.projection_name:
            raise ValueError("projection_name cannot be empty")
        if self.global_position < 0:
            raise ValueError("global_position cannot be negative")

    def advance_to(self, new_position: int) -> "ProjectionCheckpoint":
        """
        Create a new checkpoint advanced to a new position.

        Args:
            new_position: The new global position

        Returns:
            New checkpoint instance with updated position and timestamp

        Raises:
            ValueError: If new_position is less than current position
        """
        if new_position < self.global_position:
            raise ValueError(
                f"Cannot advance checkpoint backwards: {self.global_position} -> {new_position}"
            )
        return ProjectionCheckpoint(
            projection_name=self.projection_name,
            global_position=new_position,
            updated_at=datetime.now(UTC),
            version=self.version,
        )

    @classmethod
    def initial(cls, projection_name: str, version: int = 1) -> "ProjectionCheckpoint":
        """
        Create an initial checkpoint at position 0.

        Args:
            projection_name: Unique identifier for the projection
            version: Projection schema version

        Returns:
            New checkpoint at position 0
        """
        return cls(
            projection_name=projection_name,
            global_position=0,
            updated_at=datetime.now(UTC),
            version=version,
        )


@runtime_checkable
class ProjectionCheckpointStore(Protocol):
    """
    Protocol for checkpoint persistence.

    Implementations can use different storage backends:
    - PostgreSQL (recommended for production)
    - Memory (test environments only)
    - Redis (for distributed deployments)

    IMPORTANT: Checkpoints should be stored in a reliable, ACID-compliant
    store to ensure exactly-once processing semantics.

    Example PostgreSQL implementation:
        class PostgresCheckpointStore:
            async def save_checkpoint(self, checkpoint: ProjectionCheckpoint) -> None:
                await self.execute(
                    "INSERT INTO projection_checkpoints (name, position, updated_at, version) "
                    "VALUES ($1, $2, $3, $4) "
                    "ON CONFLICT (name) DO UPDATE SET "
                    "position = $2, updated_at = $3, version = $4",
                    checkpoint.projection_name,
                    checkpoint.global_position,
                    checkpoint.updated_at,
                    checkpoint.version,
                )
    """

    async def get_checkpoint(self, projection_name: str) -> ProjectionCheckpoint | None:
        """
        Get the current checkpoint for a projection.

        Args:
            projection_name: Unique identifier for the projection

        Returns:
            Current checkpoint or None if no checkpoint exists
        """
        ...

    async def save_checkpoint(self, checkpoint: ProjectionCheckpoint) -> None:
        """
        Save a checkpoint atomically.

        This should use upsert semantics (INSERT ... ON CONFLICT UPDATE).

        Args:
            checkpoint: Checkpoint to save
        """
        ...

    async def delete_checkpoint(self, projection_name: str) -> None:
        """
        Delete a checkpoint (used for projection rebuilds).

        Args:
            projection_name: Unique identifier for the projection
        """
        ...

    async def get_all_checkpoints(self) -> list[ProjectionCheckpoint]:
        """
        Get all checkpoints (used by SubscriptionCoordinator).

        Returns:
            List of all stored checkpoints
        """
        ...


class CheckpointedProjection(ABC):
    """
    Abstract base class for projections with mandatory checkpoint tracking.

    This is the RECOMMENDED base class for all projections. It enforces:
    1. Unique projection name for checkpoint tracking
    2. Version number for schema evolution and rebuild detection
    3. Explicit event type filtering for performance
    4. Explicit result types (no silent failures)

    Subclasses MUST implement:
    - get_name(): Unique projection identifier
    - get_version(): Schema version (increment to trigger rebuild)
    - get_subscribed_event_types(): Filter optimization (or None for all events)
    - handle_event(): Event processing with explicit result

    The SubscriptionCoordinator manages checkpoint updates based on the
    returned ProjectionResult.

    Example:
        class OrderSummaryProjection(CheckpointedProjection):
            def get_name(self) -> str:
                return "order_summary"

            def get_version(self) -> int:
                return 1

            def get_subscribed_event_types(self) -> set[str]:
                return {"OrderPlaced", "OrderShipped", "OrderCancelled"}

            async def handle_event(
                self,
                envelope: EventEnvelope,
                checkpoint_store: ProjectionCheckpointStore,
            ) -> ProjectionResult:
                try:
                    async with self.db.transaction():
                        # Update projection data
                        await self._update_order_summary(envelope)

                        # Update checkpoint in same transaction
                        await checkpoint_store.save_checkpoint(
                            ProjectionCheckpoint(
                                projection_name=self.get_name(),
                                global_position=envelope.metadata.global_nonce,
                                updated_at=datetime.now(UTC),
                                version=self.get_version(),
                            )
                        )
                    return ProjectionResult.SUCCESS
                except Exception as e:
                    logger.error("Failed to process event", exc_info=True)
                    return ProjectionResult.FAILURE
    """

    @abstractmethod
    def get_name(self) -> str:
        """
        Get the unique projection name for checkpoint tracking.

        This name is used as the key in the checkpoint store and must be
        unique across all projections in the system.

        Returns:
            Unique projection name (e.g., "order_summary", "user_analytics")
        """
        ...

    @abstractmethod
    def get_version(self) -> int:
        """
        Get the projection schema version.

        Increment this when the projection schema changes to trigger
        a rebuild. The SubscriptionCoordinator will detect version
        mismatches and initiate rebuilds.

        Returns:
            Projection version number (start at 1)
        """
        ...

    @abstractmethod
    def get_subscribed_event_types(self) -> set[str] | None:
        """
        Get the event types this projection handles.

        This is used for filtering optimization. If you return a set,
        the coordinator will only dispatch matching events. If you
        return None, all events will be dispatched.

        Returns:
            Set of event type names to receive, or None for all events
        """
        ...

    @abstractmethod
    async def handle_event(
        self,
        envelope: "EventEnvelope[Any]",
        checkpoint_store: ProjectionCheckpointStore,
    ) -> ProjectionResult:
        """
        Handle an event and update the checkpoint atomically.

        IMPORTANT: For projections with SQL-based storage, the projection
        data and checkpoint should be updated in the same transaction to
        ensure consistency.

        For projections with external storage (e.g., vector DB, Redis),
        update the external store first, then save the checkpoint. If
        the checkpoint save fails, the event will be reprocessed on
        restart (at-least-once delivery).

        Args:
            envelope: Event envelope containing event and metadata
            checkpoint_store: Store for persisting checkpoints

        Returns:
            ProjectionResult.SUCCESS: Event processed, advance checkpoint
            ProjectionResult.SKIP: Event not relevant, advance checkpoint
            ProjectionResult.FAILURE: Error occurred, DO NOT advance checkpoint
        """
        ...

    async def clear_all_data(self) -> None:  # noqa: B027
        """
        Clear all projection data for rebuild.

        Override this method to delete all read model data when a
        projection needs to be rebuilt from scratch.

        The default implementation does nothing - override if your
        projection has persistent storage.
        """

    def should_handle_event_type(self, event_type: str) -> bool:
        """
        Check if this projection should handle a given event type.

        Args:
            event_type: The event type to check

        Returns:
            True if this projection should receive the event
        """
        subscribed = self.get_subscribed_event_types()
        return subscribed is None or event_type in subscribed


def _camel_to_snake(name: str) -> str:
    """Convert CamelCase event type name to snake_case handler name.

    Examples:
        WorkflowExecutionStarted -> workflow_execution_started
        ExecutionCancelled -> execution_cancelled
        PhaseStarted -> phase_started
    """
    return re.sub(r"(?<!^)(?=[A-Z])", "_", name).lower()


def _snake_to_camel(name: str) -> str:
    """Convert snake_case handler suffix to CamelCase event type name.

    Examples:
        workflow_execution_started -> WorkflowExecutionStarted
        execution_cancelled -> ExecutionCancelled
        phase_started -> PhaseStarted
    """
    return "".join(part.capitalize() for part in name.split("_"))


class AutoDispatchProjection(CheckpointedProjection, ABC):
    """Projection base class with automatic event dispatch.

    Eliminates the two-places-to-update problem in projections:

    OLD pattern (fragile):
        _SUBSCRIBED_EVENTS = {"EventA", "EventB"}  # 1. Add here
        async def handle_event(...):
            if event_type == "EventA": ...       # 2. And here — easy to miss
            elif event_type == "EventB": ...

    NEW pattern (safe):
        async def on_event_a(self, data): ...    # Only place to add
        async def on_event_b(self, data): ...

    Convention: define ``async def on_<snake_case_event_type>(self, data: dict)``
    methods. The base class:
    - Automatically derives ``get_subscribed_event_types()`` from those methods
    - Automatically dispatches each event to the right handler
    - Saves the checkpoint after every successful dispatch

    Subclasses must still implement:
    - ``get_name() -> str``
    - ``get_version() -> int``
    - ``clear_all_data() -> None``
    """

    @classmethod
    def _discover_handlers(cls) -> dict[str, str]:
        """Map event type -> handler method name by scanning on_* methods.

        Scans the MRO so inherited handlers are included. Ignores non-async
        methods and private helpers (double-underscore names).

        Returns:
            Dict of {event_type: method_name}, e.g.
            {"WorkflowExecutionStarted": "on_workflow_execution_started"}
        """
        handlers: dict[str, str] = {}
        for klass in reversed(cls.__mro__):
            for attr_name, attr_value in vars(klass).items():
                if not attr_name.startswith("on_") or not callable(attr_value):
                    continue
                suffix = attr_name[3:]  # strip "on_"
                event_type = _snake_to_camel(suffix)
                handlers[event_type] = attr_name
        return handlers

    def get_subscribed_event_types(self) -> set[str] | None:
        """Derived automatically from on_* method names — do not override."""
        return set(self._discover_handlers().keys())

    async def handle_event(
        self,
        envelope: "EventEnvelope[Any]",
        checkpoint_store: "ProjectionCheckpointStore",
    ) -> "ProjectionResult":
        """Auto-dispatch to the matching on_* handler and save checkpoint."""
        event_type = envelope.event.event_type
        event_data = envelope.event.model_dump()
        global_nonce = envelope.metadata.global_nonce or 0

        try:
            handler_name = self._discover_handlers().get(event_type)
            if handler_name:
                handler = getattr(self, handler_name)
                await handler(event_data)

            await checkpoint_store.save_checkpoint(
                ProjectionCheckpoint(
                    projection_name=self.get_name(),
                    global_position=global_nonce,
                    updated_at=datetime.now(UTC),
                    version=self.get_version(),
                )
            )
            return ProjectionResult.SUCCESS

        except Exception:
            return ProjectionResult.FAILURE
