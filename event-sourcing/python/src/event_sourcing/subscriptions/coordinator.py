"""
Subscription Coordinator for managing event delivery to projections.

This module provides the SubscriptionCoordinator which:
1. Groups projections into subscription tracks by how far behind they are
2. Routes events to relevant projections based on type filtering
3. Handles per-projection checkpointing
4. Provides proper error handling (no silent failures)

See ADR-014 for architectural rationale.
"""

from __future__ import annotations

import asyncio
import logging
import sys
from dataclasses import dataclass
from datetime import UTC, datetime
from typing import TYPE_CHECKING, Protocol, TypedDict

from event_sourcing.core.checkpoint import (
    CheckpointedProjection,
    DispatchContext,
    ProjectionCheckpoint,
    ProjectionCheckpointStore,
    ProjectionResult,
)
from event_sourcing.core.process_manager import ProcessManager

if TYPE_CHECKING:
    from collections.abc import AsyncIterator

    from event_sourcing.core.event import DomainEvent, EventEnvelope


class CheckpointStatus(TypedDict):
    """Typed checkpoint status for projection health reporting."""

    position: int | None
    updated_at: str | None
    version: int | None


class ProjectionStatus(TypedDict):
    """Typed projection status returned by SubscriptionCoordinator."""

    name: str
    version: int
    subscribed_types: list[str] | None
    checkpoint: CheckpointStatus

logger = logging.getLogger(__name__)


@dataclass
class _SubscriptionTrack:
    """One event-store subscription and the projections fed from it.

    Tracks exist because projections at different positions cannot share a
    cursor. A subscription cannot hand out event N+1 until every projection
    on it has taken event N, so the furthest-behind member sets the pace for
    all of them: a projection replaying 13k events from 0 stops the ones
    already at head from seeing anything new for the whole replay (#1318).
    Splitting projections across tracks by position is what removes that.

    ``is_catching_up`` belongs to the track for the same reason. The at-head
    track is live from its first event while a replay track is still in
    history, and ProcessManager side effects are gated on this flag
    (ADR-025) - one shared flag would fire them during a replay.

    Attributes:
        name: Identifier used in logs ("live", "replay").
        from_position: Inclusive global_nonce the subscription starts at.
        projections: The projections this track feeds, by name.
        is_catching_up: True while this track is replaying historical events.
    """

    name: str
    from_position: int
    projections: dict[str, CheckpointedProjection]
    is_catching_up: bool


class EventStoreSubscriber(Protocol):
    """Protocol for event store subscription interface."""

    def subscribe(self, from_global_nonce: int) -> AsyncIterator[EventEnvelope[DomainEvent]]:
        """
        Subscribe to events starting from a given global nonce.

        Args:
            from_global_nonce: Starting position (inclusive)

        Yields:
            Event envelopes in order
        """
        ...

    async def read_all(
        self,
        from_global_nonce: int = 0,
        max_count: int = 100,
        forward: bool = True,
    ) -> tuple[list[EventEnvelope[DomainEvent]], bool, int]:
        """
        Read events from a global position (for catch-up and head detection).

        Args:
            from_global_nonce: Inclusive start position (0 = beginning)
            max_count: Maximum events to return per page
            forward: Direction (True = ascending, False = descending)

        Returns:
            Tuple of (events, is_end, next_from_global_nonce)
        """
        ...


class SubscriptionCoordinator:
    """
    Coordinates event subscription across multiple projections.

    The coordinator groups projections into subscription tracks by how far
    behind the head of the stream each one is, opens one subscription per
    track, and routes events to the projections on that track based on their
    subscribed types. In steady state every projection is at head and there
    is exactly one track; a projection that has to replay history gets its
    own, so it cannot hold up the rest (#1318).

    Key features:
    1. One subscription per track (one in steady state)
    2. Per-projection checkpointing (independent progress)
    3. A replay never starves projections already at head
    4. Event type filtering (performance)
    5. Proper error handling (no silent failures)
    6. Structured logging (observability)

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
        event_store: EventStoreSubscriber,
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
        self._live_boundary_nonce: int = 0

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

        # Until start() plans them against real checkpoints, everything shares
        # one catching-up track. Each subscription attempt replaces this.
        self._tracks: list[_SubscriptionTrack] = [
            _SubscriptionTrack(
                name="all",
                from_position=0,
                projections=self._projections,
                is_catching_up=True,
            )
        ]

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

    @property
    def projections(self) -> dict[str, CheckpointedProjection]:
        """Registered projections (name -> instance). Read-only view."""
        return self._projections

    @property
    def is_catching_up(self) -> bool:
        """True while any track is replaying historical events.

        A rebuild running alongside projections at head reads as True: some
        of the read model is still behind, which is what a caller asking this
        wants to know.
        """
        return any(track.is_catching_up for track in self._tracks)

    @is_catching_up.setter
    def is_catching_up(self, value: bool) -> None:
        """Force every track's catch-up state (fitness harnesses, tests)."""
        for track in self._tracks:
            track.is_catching_up = value

    @property
    def live_boundary_nonce(self) -> int:
        """The head global_nonce snapshot taken before subscribing."""
        return self._live_boundary_nonce

    @live_boundary_nonce.setter
    def live_boundary_nonce(self, value: int) -> None:
        self._live_boundary_nonce = value

    async def dispatch_event(
        self,
        envelope: EventEnvelope[DomainEvent],
    ) -> None:
        """Dispatch a single event to all subscribed projections.

        Public interface for testing and fitness tooling. Production code
        uses start(), which feeds each track from its own subscription.
        """
        for track in self._tracks:
            await self._dispatch_to_track(track, envelope)

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

        Snapshots the head of the stream, plans the tracks, and runs one
        subscription per track concurrently until the streams end or
        self._running is False. Returns when every track has finished, so
        start() re-plans on the next attempt; a reconnect mid-replay is
        therefore re-grouped against the positions reached so far.

        The head is snapshotted BEFORE the checkpoints are read, so events
        arriving while we plan cannot make a projection that is genuinely at
        head look behind and strand it on a replay track.
        """
        self._live_boundary_nonce = await self._read_head_nonce()
        self._tracks = await self._plan_tracks(self._live_boundary_nonce)

        for track in self._tracks:
            logger.info(
                "Starting subscription track",
                extra={
                    "track": track.name,
                    "from_position": track.from_position,
                    "is_catching_up": track.is_catching_up,
                    "live_boundary_nonce": self._live_boundary_nonce,
                    "projection_count": len(track.projections),
                    "projection_names": sorted(track.projections),
                },
            )

        # A failure on any track cancels the others and propagates, so start()
        # retries the whole plan with backoff rather than leaving the read
        # model half-fed by a track nobody is watching.
        async with asyncio.TaskGroup() as group:
            for track in self._tracks:
                group.create_task(self._run_track(track))

    async def _run_track(self, track: _SubscriptionTrack) -> None:
        """Feed one track from its own subscription until stopped."""
        async for envelope in self._event_store.subscribe(from_global_nonce=track.from_position):
            if not self._running:
                break
            self._last_error = None
            await self._dispatch_to_track(track, envelope)

    async def _read_head_nonce(self) -> int:
        """
        Snapshot the highest global_nonce currently in the event store.

        This is the durable catch-up/live boundary: events at or below it were
        already stored when we subscribed and are historical; events above it
        are live.

        Read backwards from the highest possible nonce to get the head event.
        from_global_nonce is inclusive, so a backwards read returns events
        with global_nonce <= from_global_nonce; 0 would return nothing useful.
        """
        head_events, _is_end, _next = await self._event_store.read_all(
            from_global_nonce=sys.maxsize, max_count=1, forward=False,
        )
        if head_events and head_events[0].metadata.global_nonce is not None:
            return head_events[0].metadata.global_nonce
        return 0

    async def stop(self) -> None:
        """Stop the subscription coordinator gracefully.

        Sets `_running` to False which causes the subscription loop in `start()`
        to exit on the next iteration.
        """
        if not self._running:
            return

        logger.info("Stopping subscription coordinator")
        self._running = False

    async def _plan_tracks(self, live_boundary_nonce: int) -> list[_SubscriptionTrack]:
        """
        Split the projections into subscription tracks by position.

        A projection that needs nothing at or below ``live_boundary_nonce`` is
        at head and can take live events straight away. One that still needs
        history - never run, version bumped and cleared, or left behind by a
        failure - has to replay first, and it replays on its own track so the
        at-head projections keep consuming while it does (#1318).

        Grouping on position rather than on "was a rebuild triggered" is what
        makes this hold across a reconnect: a rebuild interrupted halfway has
        a valid checkpoint at a low position, and it must stay on the replay
        track rather than drag the whole plan back to where it got to.

        Args:
            live_boundary_nonce: Head snapshot separating history from live

        Returns:
            The tracks to subscribe, at-head track first. It is always
            present, even when empty, so the coordinator always holds one
            subscription on the live tail.
        """
        at_head: dict[str, CheckpointedProjection] = {}
        behind: dict[str, CheckpointedProjection] = {}
        behind_positions: list[int] = []

        for name, projection in self._projections.items():
            resume_from = await self._resume_position(name, projection)
            if resume_from > live_boundary_nonce:
                at_head[name] = projection
            else:
                behind[name] = projection
                behind_positions.append(resume_from)

        tracks = [
            _SubscriptionTrack(
                name="live",
                from_position=live_boundary_nonce + 1,
                projections=at_head,
                # Starts above the boundary by construction, so every event it
                # ever sees is live.
                is_catching_up=False,
            )
        ]

        if behind:
            from_position = min(behind_positions)
            tracks.append(
                _SubscriptionTrack(
                    name="replay",
                    from_position=from_position,
                    projections=behind,
                    is_catching_up=from_position <= live_boundary_nonce,
                )
            )

        return tracks

    async def _resume_position(
        self,
        name: str,
        projection: CheckpointedProjection,
    ) -> int:
        """
        The first global_nonce this projection still needs.

        Returns 0 - the whole stream - when the projection has no checkpoint,
        or when its declared version has moved past the stored one. In the
        version case the stored data and checkpoint are dropped first, so the
        replay rebuilds from an empty read model.

        Callers get a position and nothing else; whether a rebuild was
        triggered, and what clearing it cost, stays in here.

        Args:
            name: Projection name
            projection: The projection to locate in the stream

        Returns:
            Inclusive global_nonce to resume this projection from
        """
        checkpoint = await self._checkpoint_store.get_checkpoint(name)

        if checkpoint is None:
            logger.info(
                "Projection has no checkpoint, replaying from 0",
                extra={"projection_name": name},
            )
            return 0

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
            return 0

        return checkpoint.global_position + 1

    async def _dispatch_to_track(
        self,
        track: _SubscriptionTrack,
        envelope: EventEnvelope[DomainEvent],
    ) -> None:
        """
        Dispatch an event to the projections on one track.

        Tracks that track's own catch-up/live transition based on global_nonce.

        Args:
            track: The track the event arrived on
            envelope: Event envelope to dispatch
        """
        event_type = envelope.metadata.event_type or "Unknown"
        global_nonce = envelope.metadata.global_nonce or 0

        # Transition: catch-up -> live when this track passes the boundary
        # nonce. Uses > (strictly greater): events at the boundary were
        # already in the store when we subscribed, so they are historical.
        if track.is_catching_up and global_nonce > self._live_boundary_nonce:
            track.is_catching_up = False
            logger.info(
                "Subscription track transitioned to live mode",
                extra={
                    "track": track.name,
                    "global_nonce": global_nonce,
                    "live_boundary_nonce": self._live_boundary_nonce,
                },
            )

        for name, projection in track.projections.items():
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
            await self._dispatch_to_projection(track, projection, envelope)

    async def _dispatch_to_projection(
        self,
        track: _SubscriptionTrack,
        projection: CheckpointedProjection,
        envelope: EventEnvelope[DomainEvent],
    ) -> None:
        """
        Dispatch an event to a single projection with error handling.

        Args:
            track: The track the event arrived on, source of catch-up state
            projection: Target projection
            envelope: Event envelope to dispatch
        """
        name = projection.get_name()
        event_type = envelope.metadata.event_type or "Unknown"
        global_nonce = envelope.metadata.global_nonce or 0

        context = DispatchContext(
            is_catching_up=track.is_catching_up,
            global_nonce=global_nonce,
            live_boundary_nonce=self._live_boundary_nonce,
        )

        try:
            result = await projection.handle_event(
                envelope, self._checkpoint_store, context,
            )

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

                # ProcessManager: run the processor side for live events only.
                # The key invariant: process_pending() is NEVER called while
                # this track's is_catching_up is True. Read off the track, not
                # the coordinator: a sibling track reaching live must not
                # unlock side effects for a replay still in history.
                if not track.is_catching_up and isinstance(projection, ProcessManager):
                    try:
                        processed = await projection.process_pending()
                        if processed > 0:
                            logger.info(
                                "ProcessManager processed pending items",
                                extra={
                                    "projection_name": name,
                                    "items_processed": processed,
                                },
                            )
                    except Exception:
                        logger.exception(
                            "ProcessManager.process_pending() failed",
                            extra={"projection_name": name},
                        )
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
    ) -> ProjectionStatus | None:
        """
        Get status information for a projection.

        Args:
            name: Projection name

        Returns:
            Typed projection status or None if not found
        """
        projection = self._projections.get(name)
        if not projection:
            return None

        checkpoint = await self._checkpoint_store.get_checkpoint(name)

        subscribed = projection.get_subscribed_event_types()
        return ProjectionStatus(
            name=name,
            version=projection.get_version(),
            subscribed_types=sorted(subscribed) if subscribed is not None else None,
            checkpoint=CheckpointStatus(
                position=checkpoint.global_position if checkpoint else None,
                updated_at=checkpoint.updated_at.isoformat() if checkpoint else None,
                version=checkpoint.version if checkpoint else None,
            ),
        )
