"""
Subscription Coordinator for managing event delivery to projections.

This module provides the SubscriptionCoordinator which:
1. Maintains a single connection to the event store
2. Routes events to relevant projections based on type filtering
3. Handles per-projection checkpointing
4. Provides proper error handling (no silent failures)

See ADR-014 for architectural rationale.
"""

import asyncio
import logging
from collections.abc import AsyncIterator
from datetime import UTC, datetime
from typing import Any, Protocol

from event_sourcing.core.checkpoint import (
    CheckpointedProjection,
    ProjectionCheckpoint,
    ProjectionCheckpointStore,
    ProjectionResult,
)
from event_sourcing.core.event import EventEnvelope

logger = logging.getLogger(__name__)


class EventStoreSubscriber(Protocol):
    """Protocol for event store subscription interface."""

    async def subscribe(self, from_global_nonce: int) -> AsyncIterator[EventEnvelope[Any]]:
        """
        Subscribe to events starting from a given global nonce.

        Args:
            from_global_nonce: Starting position (inclusive)

        Yields:
            Event envelopes in order
        """
        ...


class SubscriptionCoordinator:
    """
    Coordinates event subscription across multiple projections.

    The coordinator maintains a single subscription to the event store and
    routes events to relevant projections based on their subscribed types.

    Key features:
    1. Single connection to event store (efficient)
    2. Per-projection checkpointing (independent progress)
    3. Event type filtering (performance)
    4. Proper error handling (no silent failures)
    5. Structured logging (observability)

    Usage:
        # Create coordinator
        coordinator = SubscriptionCoordinator(
            event_store=event_store,
            checkpoint_store=checkpoint_store,
            projections=[
                OrderSummaryProjection(),
                UserAnalyticsProjection(),
            ],
        )

        # Start processing (runs until stopped)
        await coordinator.start()

        # Stop gracefully
        await coordinator.stop()

        # Rebuild a single projection
        await coordinator.rebuild_projection("order_summary")
    """

    def __init__(
        self,
        event_store: Any,  # EventStoreClient or compatible
        checkpoint_store: ProjectionCheckpointStore,
        projections: list[CheckpointedProjection],
    ) -> None:
        """
        Initialize the subscription coordinator.

        Args:
            event_store: Event store client with subscribe() method
            checkpoint_store: Store for checkpoint persistence
            projections: List of projections to manage
        """
        self._event_store = event_store
        self._checkpoint_store = checkpoint_store
        self._running = False
        self._last_error: Exception | None = None

        # Validate for duplicate projection names
        self._projections: dict[str, CheckpointedProjection] = {}
        for projection in projections:
            name = projection.get_name()
            if name in self._projections:
                raise ValueError(
                    f"Duplicate projection name: '{name}'. "
                    "Each projection must have a unique name."
                )
            self._projections[name] = projection

        logger.info(
            "Initialized subscription coordinator",
            extra={
                "projection_count": len(self._projections),
                "projection_names": list(self._projections.keys()),
            },
        )

    @property
    def is_healthy(self) -> bool:
        """True if the coordinator is running and has no active error."""
        return self._running and self._last_error is None

    async def start(self) -> None:
        """
        Start the subscription coordinator with exponential-backoff retry.

        Retries on any transient error (e.g. RST_STREAM, connection reset).
        Stops only on explicit stop() or CancelledError.
        """
        if self._running:
            logger.warning("Subscription coordinator already running")
            return

        self._running = True
        backoff = 1.0

        while self._running:
            try:
                await self._subscribe_loop()
                backoff = 1.0  # clean exit — reset backoff
            except asyncio.CancelledError:
                logger.info("Subscription cancelled")
                raise
            except Exception as e:
                if not self._running:
                    break
                self._last_error = e
                logger.warning(
                    "Subscription error — retrying in %.1fs",
                    backoff,
                    extra={"error": str(e)},
                    exc_info=True,
                )
                await asyncio.sleep(backoff)
                backoff = min(backoff * 2, 30.0)

        logger.info("Subscription coordinator stopped")

    async def _subscribe_loop(self) -> None:
        """
        Run a single subscription attempt.

        Loads checkpoints, subscribes from the minimum position, and
        dispatches events until the stream ends or self._running is False.
        Clears _last_error on the first successful event dispatch.
        """
        min_position = await self._get_minimum_position()

        logger.info(
            "Starting subscription",
            extra={
                "from_position": min_position,
                "projection_count": len(self._projections),
            },
        )

        async for envelope in self._event_store.subscribe(from_global_nonce=min_position):
            if not self._running:
                break
            self._last_error = None
            await self._dispatch_event(envelope)

    async def stop(self) -> None:
        """Stop the subscription coordinator gracefully.

        Sets `_running` to False which causes the subscription loop in `start()`
        to exit on the next iteration.
        """
        if not self._running:
            return

        logger.info("Stopping subscription coordinator")
        self._running = False

    async def _get_minimum_position(self) -> int:
        """
        Get the minimum checkpoint position across all projections.

        Checks ALL projections for version mismatches before returning,
        so that multiple projections bumped simultaneously are all rebuilt.

        Returns:
            Position to start subscription from
        """
        min_pos: int | None = None
        needs_full_replay = False

        for name, projection in self._projections.items():
            checkpoint = await self._checkpoint_store.get_checkpoint(name)

            if checkpoint is None:
                # Projection needs full replay
                logger.info(
                    "Projection has no checkpoint, starting from 0",
                    extra={"projection_name": name},
                )
                needs_full_replay = True
                continue

            # Check version mismatch (needs rebuild)
            if checkpoint.version != projection.get_version():
                logger.warning(
                    "Projection version mismatch, clearing data and checkpoint for rebuild",
                    extra={
                        "projection_name": name,
                        "stored_version": checkpoint.version,
                        "current_version": projection.get_version(),
                    },
                )
                # Clear projection data before replay to avoid data corruption
                await projection.clear_all_data()
                await self._checkpoint_store.delete_checkpoint(name)
                needs_full_replay = True
                continue

            if min_pos is None or checkpoint.global_position < min_pos:
                min_pos = checkpoint.global_position

        if needs_full_replay:
            return 0

        # Start from next position after minimum
        return (min_pos or 0) + 1

    async def _dispatch_event(self, envelope: EventEnvelope[Any]) -> None:
        """
        Dispatch an event to all relevant projections.

        Args:
            envelope: Event envelope to dispatch
        """
        event_type = envelope.event.event_type
        global_nonce = envelope.metadata.global_nonce or 0

        for name, projection in self._projections.items():
            # Check if projection subscribes to this event type
            subscribed = projection.get_subscribed_event_types()
            if subscribed is not None and event_type not in subscribed:
                # Skip but advance checkpoint
                await self._advance_checkpoint_if_behind(name, global_nonce)
                continue

            # Check if projection is already past this position
            checkpoint = await self._checkpoint_store.get_checkpoint(name)
            if checkpoint and checkpoint.global_position >= global_nonce:
                continue  # Already processed

            # Dispatch to projection
            await self._dispatch_to_projection(projection, envelope)

    async def _dispatch_to_projection(
        self,
        projection: CheckpointedProjection,
        envelope: EventEnvelope[Any],
    ) -> None:
        """
        Dispatch an event to a single projection with error handling.

        Args:
            projection: Target projection
            envelope: Event envelope to dispatch
        """
        name = projection.get_name()
        event_type = envelope.event.event_type
        global_nonce = envelope.metadata.global_nonce or 0

        try:
            result = await projection.handle_event(envelope, self._checkpoint_store)

            if result == ProjectionResult.FAILURE:
                logger.error(
                    "Projection returned FAILURE",
                    extra={
                        "projection_name": name,
                        "event_type": event_type,
                        "global_nonce": global_nonce,
                    },
                )
                # DO NOT advance checkpoint - event will be retried
            elif result == ProjectionResult.SUCCESS:
                logger.debug(
                    "Projection processed event",
                    extra={
                        "projection_name": name,
                        "event_type": event_type,
                        "global_nonce": global_nonce,
                        "result": result.value,
                    },
                )
                # Checkpoint should be saved by the projection itself
                # for atomicity with data updates
            elif result == ProjectionResult.SKIP:
                # SKIP means the projection doesn't care about this event.
                # We must still advance the checkpoint so it's not retried.
                logger.debug(
                    "Projection skipped event, advancing checkpoint",
                    extra={
                        "projection_name": name,
                        "event_type": event_type,
                        "global_nonce": global_nonce,
                    },
                )
                await self._advance_checkpoint_if_behind(name, global_nonce)

        except Exception as e:
            logger.error(
                "Projection raised exception",
                extra={
                    "projection_name": name,
                    "event_type": event_type,
                    "global_nonce": global_nonce,
                    "error": str(e),
                },
                exc_info=True,
            )
            # DO NOT advance checkpoint - event will be retried

    async def _advance_checkpoint_if_behind(
        self,
        projection_name: str,
        position: int,
    ) -> None:
        """
        Advance checkpoint for skipped events (event type not subscribed).

        Args:
            projection_name: Name of the projection
            position: Current event position
        """
        projection = self._projections.get(projection_name)
        if not projection:
            return

        checkpoint = await self._checkpoint_store.get_checkpoint(projection_name)
        if checkpoint and checkpoint.global_position >= position:
            return  # Already past this position

        # Advance checkpoint without processing
        new_checkpoint = ProjectionCheckpoint(
            projection_name=projection_name,
            global_position=position,
            updated_at=datetime.now(UTC),
            version=projection.get_version(),
        )
        await self._checkpoint_store.save_checkpoint(new_checkpoint)

    async def rebuild_projection(self, projection_name: str) -> None:
        """
        Rebuild a single projection from scratch.

        This method:
        1. Deletes the projection checkpoint
        2. Clears projection data
        3. The next start() will re-process from position 0

        Args:
            projection_name: Name of the projection to rebuild

        Raises:
            KeyError: If projection not found
        """
        if projection_name not in self._projections:
            raise KeyError(f"Projection '{projection_name}' not found")

        projection = self._projections[projection_name]

        logger.warning(
            "Rebuilding projection",
            extra={"projection_name": projection_name},
        )

        # Delete checkpoint
        await self._checkpoint_store.delete_checkpoint(projection_name)

        # Clear projection data
        await projection.clear_all_data()

        logger.info(
            "Projection rebuild prepared - restart coordinator to re-process",
            extra={"projection_name": projection_name},
        )

    def get_projection(self, name: str) -> CheckpointedProjection | None:
        """
        Get a registered projection by name.

        Args:
            name: Projection name

        Returns:
            Projection instance or None if not found
        """
        return self._projections.get(name)

    def get_all_projections(self) -> dict[str, CheckpointedProjection]:
        """
        Get all registered projections.

        Returns:
            Dictionary mapping names to projections
        """
        return self._projections.copy()

    async def get_projection_status(
        self,
        name: str,
    ) -> dict[str, Any] | None:
        """
        Get status information for a projection.

        Args:
            name: Projection name

        Returns:
            Status dictionary or None if not found
        """
        projection = self._projections.get(name)
        if not projection:
            return None

        checkpoint = await self._checkpoint_store.get_checkpoint(name)

        return {
            "name": name,
            "version": projection.get_version(),
            "subscribed_types": projection.get_subscribed_event_types(),
            "checkpoint": {
                "position": checkpoint.global_position if checkpoint else None,
                "updated_at": checkpoint.updated_at.isoformat() if checkpoint else None,
                "version": checkpoint.version if checkpoint else None,
            },
        }
