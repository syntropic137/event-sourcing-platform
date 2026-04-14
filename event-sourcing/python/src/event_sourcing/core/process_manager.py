"""
ProcessManager base class implementing the Processor To-Do List pattern.

This module provides ``ProcessManager``, a base class for event consumers
that need to produce side effects (dispatch workflows, call external APIs,
send notifications, etc.) in response to domain events.

The To-Do List pattern (Martin Dilger, *Understanding Event Sourcing*,
Ch. 37) splits event-driven side effects into two parts with a hard
boundary between them:

1. **Projection side** (``handle_event``) -- writes to-do records to a
   projection store. Called during both replay and live processing.
   Must be pure: no side effects, no external calls.

2. **Processor side** (``process_pending``) -- reads pending to-do
   records and executes them. Called by the coordinator ONLY for live
   events, never during catch-up replay. Must be idempotent.

The ``SubscriptionCoordinator`` enforces the boundary: it never calls
``process_pending()`` while ``is_catching_up`` is True.

See Also:
    - ADR-025 for the architectural decision rationale
    - ``CheckpointedProjection`` for pure read-model projections
    - ``DispatchContext`` for replay awareness
"""

from __future__ import annotations

from abc import abstractmethod
from typing import TYPE_CHECKING, ClassVar

from event_sourcing.core.checkpoint import (
    CheckpointedProjection,
    ProjectionResult,
)

if TYPE_CHECKING:
    from event_sourcing.core.checkpoint import (
        DispatchContext,
        ProjectionCheckpointStore,
    )
    from event_sourcing.core.event import DomainEvent, EventEnvelope


class ProcessManager(CheckpointedProjection):
    """Processor To-Do List pattern (Dilger, Ch. 37).

    Two parts with a hard boundary between them:

    PROJECTION SIDE (read-only, replay-safe):
        ``handle_event()`` writes to-do records to the projection store.
        Called during both catch-up replay and live processing.
        MUST NOT call external services, create tasks, or dispatch work.

    PROCESSOR SIDE (live-only, idempotent):
        ``process_pending()`` reads pending records and executes them.
        Called by the coordinator ONLY for live events, never during
        catch-up replay. Implementations MUST be idempotent -- the same
        item processed twice must produce the same result.

    Subclasses MUST implement:
        - ``get_name()`` -- unique projection identifier
        - ``get_version()`` -- schema version (increment triggers rebuild)
        - ``get_subscribed_event_types()`` -- event type filter
        - ``handle_event()`` -- write to-do records (no side effects)
        - ``process_pending()`` -- execute pending items (idempotent)
        - ``get_idempotency_key()`` -- stable dedup key per to-do item

    Example::

        class WorkflowDispatchManager(ProcessManager):
            def get_name(self) -> str:
                return "workflow_dispatch"

            def get_version(self) -> int:
                return 1

            def get_subscribed_event_types(self) -> set[str]:
                return {"TriggerFiredEvent"}

            async def handle_event(self, envelope, checkpoint_store, context=None):
                # PROJECTION SIDE: write a dispatch record (to-do item)
                await self._store.upsert(dispatch_record)
                await checkpoint_store.save_checkpoint(...)
                return ProjectionResult.SUCCESS

            async def process_pending(self) -> int:
                # PROCESSOR SIDE: dispatch pending workflows
                pending = await self._store.query(status="pending")
                for item in pending:
                    execution_id = self.get_idempotency_key(item)
                    if await self._execution_exists(execution_id):
                        await self._store.mark_done(item)
                        continue
                    await self._dispatch_workflow(item)
                    await self._store.mark_done(item)
                return len(pending)

            def get_idempotency_key(self, todo_item: dict[str, object]) -> str:
                return str(todo_item["execution_id"])
    """

    SIDE_EFFECTS_ALLOWED: ClassVar[bool] = True
    """ProcessManagers are allowed to produce side effects via process_pending()."""

    @abstractmethod
    async def handle_event(
        self,
        envelope: EventEnvelope[DomainEvent],
        checkpoint_store: ProjectionCheckpointStore,
        context: DispatchContext | None = None,
    ) -> ProjectionResult:
        """PROJECTION SIDE: Write to-do records. No side effects.

        Called during both catch-up replay and live processing. Only
        update the to-do list (projection store). Do not call external
        services, create infrastructure, or dispatch work.

        Args:
            envelope: Event envelope containing the domain event.
            checkpoint_store: Store for persisting the projection checkpoint.
            context: Dispatch context with replay awareness.

        Returns:
            ProjectionResult indicating success, skip, or failure.
        """
        ...

    @abstractmethod
    async def process_pending(self) -> int:
        """PROCESSOR SIDE: Execute pending to-do items.

        Called by the coordinator ONLY for live events. Never called
        during catch-up replay. The coordinator enforces this invariant.

        Implementations MUST be idempotent: processing the same item
        twice must produce the same result (e.g., check if the workflow
        execution already exists before dispatching).

        Returns:
            The number of items processed.
        """
        ...

    @abstractmethod
    def get_idempotency_key(self, todo_item: dict[str, object]) -> str:
        """Return a stable dedup key for a to-do item.

        The key must be deterministic for the same logical work item.
        Used by the framework and by ``process_pending()`` implementations
        to prevent duplicate processing.

        Args:
            todo_item: The to-do record to generate a key for.

        Returns:
            A stable, content-based dedup key string.
        """
        ...
