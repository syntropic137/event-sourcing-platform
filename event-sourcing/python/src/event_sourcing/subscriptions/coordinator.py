"""
Subscription Coordinator for managing event delivery to projections.

This module provides the SubscriptionCoordinator which:
1. Groups projections into subscription tracks by position: at head, near
   head, or replaying
2. Routes events to relevant projections based on type filtering
3. Handles per-projection checkpointing
4. Provides proper error handling (no silent failures)
5. Drains each ProcessManager's to-do list on its own task, off the cursor

See ADR-014 for architectural rationale.
"""

from __future__ import annotations

import asyncio
import inspect
import logging
import math
import sys
from dataclasses import dataclass, field
from datetime import UTC, datetime
from typing import TYPE_CHECKING, NamedTuple, Protocol, TypedDict, cast

from event_sourcing.core.checkpoint import (
    CheckpointedProjection,
    DispatchContext,
    ProjectionCheckpoint,
    ProjectionCheckpointStore,
    ProjectionResult,
)
from event_sourcing.core.errors import (
    ProjectionHandlerFailedError,
    SubscriptionHaltedError,
    UndecodableEventError,
)
from event_sourcing.core.process_manager import ProcessManager
from event_sourcing.subscriptions.drain import ProcessManagerDrain

if TYPE_CHECKING:
    from collections.abc import AsyncIterator, Callable

    from event_sourcing.core.envelope import EventTypeFilter
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

# How many replay tracks may be open at once, on top of the always-present
# live track. Each track is one event-store subscription and so one
# connection, which is why isolating rebuilds has to be bounded rather than
# one-per-projection: a version bump applied across a whole read model would
# otherwise open a stream per rebuilding projection and exhaust the pool.
# Four covers the realistic case - #1318 was two simultaneous version bumps -
# while leaving a 25-projection deployment well inside a default pool.
DEFAULT_REPLAY_CONCURRENCY = 4

# How far behind the live boundary, in events, a projection may be and still
# catch up on the short near-head track instead of competing for a replay
# track. A replay track starts at its furthest-behind member, so a projection
# 9 events behind dealt onto the same track as a rebuild from 0 waits for the
# whole rebuild (syntropic137#1554: ~46k events, about an hour). Inside the
# window the wait is bounded by the window itself. 1000 events is seconds of
# catch-up even at the slowest replay rate observed (~12 events/s is ~80s),
# and far below any real rebuild.
DEFAULT_NEAR_HEAD_WINDOW = 1000

# While a track is catching up, the checkpoint a projection earns by
# *skipping* an event it does not subscribe to is held in memory and saved at
# most once per this many events, instead of a read plus a write per event.
# Skips are the bulk of a replay - most projections subscribe to a handful of
# types - and each checkpoint call is a database round trip in production.
# Losing an unsaved skip on a crash is safe: the events are re-skipped on the
# next replay. Checkpoints a projection saves itself in handle_event() are
# untouched, so its data and position stay atomic.
CATCH_UP_SKIP_CHECKPOINT_INTERVAL = 500

# A projection that fails an event is held below it and fed it again on a
# track of its own after this delay, doubling per consecutive failure of the
# same event up to the cap (the same pace start() retries a failed attempt).
# Only the failing projection waits: the others keep consuming (#1696).
HELD_RETRY_INITIAL_DELAY = 1.0
HELD_RETRY_MAX_DELAY = 30.0
# The doubling stops at the cap. The exponent is bounded before it is used:
# 2.0 ** 1024 overflows a float, which a poison event reaches after ~8.5h.
_HELD_RETRY_MAX_EXPONENT = math.ceil(math.log2(HELD_RETRY_MAX_DELAY / HELD_RETRY_INITIAL_DELAY))

# The always-present track at head; see ``_plan_tracks``.
LIVE_TRACK = "live"


def _find_undecodable(error: BaseException) -> UndecodableEventError | None:
    """The undecodable-event error in ``error``, unwrapping TaskGroup groups.

    One failing track fails the whole TaskGroup, so the store's DATA_LOSS
    arrives inside an ExceptionGroup, possibly beside other tracks' errors.
    If several tracks hit undecodable rows, the lowest position is reported:
    it is the one every projection before it must get past first.
    """
    if isinstance(error, UndecodableEventError):
        return error
    if isinstance(error, BaseExceptionGroup):
        group = cast("BaseExceptionGroup[BaseException]", error)
        found = [
            nested
            for nested in (_find_undecodable(inner) for inner in group.exceptions)
            if nested is not None
        ]
        if found:
            return min(found, key=lambda nested: nested.global_nonce)
    return None


@dataclass
class _SubscriptionTrack:
    """One event-store subscription and the projections fed from it.

    Tracks exist because projections at different positions cannot share a
    cursor. A subscription cannot hand out event N+1 until every projection
    on it has taken event N, so the furthest-behind member sets the pace for
    all of them: a projection replaying 13k events from 0 stops the ones
    already at head from seeing anything new for the whole replay (#1318).
    Splitting projections across tracks by position is what removes that.

    The same pacing applies between two rebuilds, so a rebuild gets a track
    to itself rather than a shared "replay" track - #1318 was two version
    bumps at once. Past ``replay_concurrency`` rebuilds they share again,
    because a track costs a connection; see ``_plan_tracks``.

    ``is_catching_up`` belongs to the track for the same reason. The at-head
    track is live from its first event while a replay track is still in
    history, and ProcessManager side effects are gated on this flag
    (ADR-025) - one shared flag would fire them during a replay.

    Attributes:
        name: Identifier used in logs ("live", "near-head", "replay-0", ...).
        from_position: Inclusive global_nonce the subscription starts at.
        projections: The projections this track feeds, by name.
        is_catching_up: True while this track is replaying historical events.
        unsaved_skips: Catch-up only. Highest skipped global_nonce per
            projection whose checkpoint has not been saved yet; see
            ``CATCH_UP_SKIP_CHECKPOINT_INTERVAL``.
        skips_saved_at: global_nonce at which ``unsaved_skips`` was last saved.
        generations: Each projection's rebuild generation when this track was
            planned. A track whose generation for a projection is stale was
            planned before a ``rebuild_projection`` of it, and no longer
            touches it: no dispatch, no checkpoint, no drain unlock, no
            decoding for it (see ``_is_member``).
    """

    name: str
    from_position: int
    projections: dict[str, CheckpointedProjection]
    is_catching_up: bool
    unsaved_skips: dict[str, int] = field(default_factory=dict[str, int])
    skips_saved_at: int = 0
    generations: dict[str, int] = field(default_factory=dict[str, int])


@dataclass
class _HeldProjection:
    """A projection held below an event it failed to apply (syntropic137#1696).

    Attributes:
        failure: The latest failure, naming the event it is held below.
        attempts: Consecutive failures of that same event; sets the backoff.
    """

    failure: ProjectionHandlerFailedError
    attempts: int


def _held_retry_delay(attempts: int) -> float:
    """Seconds before retrying a projection held for ``attempts`` consecutive failures."""
    exponent = min(max(attempts - 1, 0), _HELD_RETRY_MAX_EXPONENT)
    return min(HELD_RETRY_INITIAL_DELAY * 2**exponent, HELD_RETRY_MAX_DELAY)


class _TrackEventTypes:
    """The types the projections on a track handle, read live.

    Live, not a snapshot: a projection held off the track (``_hold``) or
    rebuilt since it was planned must stop widening its filter, or its types
    would still be decoded, and could halt, for the members left on it.
    """

    def __init__(
        self,
        track: _SubscriptionTrack,
        members: Callable[[_SubscriptionTrack], list[CheckpointedProjection]],
    ) -> None:
        self._track = track
        self._members = members

    def __contains__(self, event_type: str, /) -> bool:
        for projection in self._members(self._track):
            subscribed = projection.get_subscribed_event_types()
            if subscribed is None or event_type in subscribed:
                return True
        return False


class _BehindProjection(NamedTuple):
    """A projection that still needs history, and where it needs it from."""

    name: str
    projection: CheckpointedProjection
    resume_from: int
    generation: int


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


class TypeFilteringSubscriber(Protocol):
    """A subscriber that can leave events of unwanted types undecoded.

    ``GrpcEventStoreClient`` is one. Events whose type (after upcasting) is
    not in ``event_types`` are yielded without decoding, so one the track's
    projections would skip cannot fail decoding and halt them (ADR-027).
    The coordinator passes ``event_types`` only to a ``subscribe`` that
    declares it.
    """

    def subscribe(
        self, from_global_nonce: int, event_types: EventTypeFilter | None = None
    ) -> AsyncIterator[EventEnvelope[DomainEvent]]: ...


def _accepts_event_types(subscriber: EventStoreSubscriber) -> bool:
    subscribe = getattr(subscriber, "subscribe", None)
    if subscribe is None:
        return False
    try:
        return "event_types" in inspect.signature(subscribe).parameters
    except (TypeError, ValueError):
        return False


class SubscriptionCoordinator:
    """
    Coordinates event subscription across multiple projections.

    The coordinator groups projections into subscription tracks by how far
    behind the head of the stream each one is, opens one subscription per
    track, and routes events to the projections on that track based on their
    subscribed types. In steady state every projection is at head and there
    is exactly one track. Projections within ``near_head_window`` events of
    head share one short near-head track, so their latency is bounded by
    their own distance from head (syntropic137#1554). A projection that has
    to replay further than that gets a replay track to itself, so it can hold
    up neither the projections at or near head nor another rebuild (#1318).
    At most ``replay_concurrency`` of those exist at once, so the number of
    connections stays bounded however many projections are rebuilding.

    Key features:
    1. One subscription per track (one in steady state, at most
       2 + replay_concurrency: live, near-head, replays)
    2. Per-projection checkpointing (independent progress)
    3. A replay never starves projections already at head, or another replay
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
        replay_concurrency: int = DEFAULT_REPLAY_CONCURRENCY,
        near_head_window: int = DEFAULT_NEAR_HEAD_WINDOW,
        undecodable_recheck_interval: float | None = None,
    ) -> None:
        """
        Initialize the subscription coordinator.

        Args:
            event_store: Event store client with subscribe() method
            checkpoint_store: Store for checkpoint persistence
            projections: List of projections to manage
            replay_concurrency: How many rebuilds may replay independently.
                One event-store connection each, so size it against the pool.
            near_head_window: A projection needing at most this many events
                at or below the live boundary catches up on the near-head
                track (one more connection, only while such projections
                exist) rather than a replay track. 0 disables it.
            undecodable_recheck_interval: What to do when the store reports
                an undecodable stored event (gRPC DATA_LOSS, ADR-026). None
                (default): halt, and start() raises SubscriptionHaltedError.
                A number of seconds: stay halted inside start() (``halted``
                set, ``is_healthy`` False) and re-check at that fixed pace,
                resuming on its own once an operator has repaired the row or
                moved the checkpoints. Either way it is never retried with
                the transient-error backoff.

        Raises:
            ValueError: If replay_concurrency is below 1, which would leave a
                behind projection with no track to replay on, if
                near_head_window is negative, or if
                undecodable_recheck_interval is not positive.
        """
        if replay_concurrency < 1:
            raise ValueError(f"replay_concurrency must be at least 1, got {replay_concurrency}")
        if near_head_window < 0:
            raise ValueError(f"near_head_window must not be negative, got {near_head_window}")
        if undecodable_recheck_interval is not None and not undecodable_recheck_interval > 0:
            raise ValueError(
                "undecodable_recheck_interval must be positive or None, "
                f"got {undecodable_recheck_interval}"
            )

        self._event_store = event_store
        self._filters_types = _accepts_event_types(event_store)
        self._checkpoint_store = checkpoint_store
        self._replay_concurrency = replay_concurrency
        self._near_head_window = near_head_window
        self._undecodable_recheck_interval = undecodable_recheck_interval
        # Set while stopped at an undecodable event (#360). Cleared once a
        # plan needs nothing at or below its position, or every track that
        # did has delivered an event at or past it (the row was repaired).
        self._halt: SubscriptionHaltedError | None = None
        self._halt_pending_tracks: set[str] = set()
        # A track planned before rebuild_projection() is still running after
        # it. Nothing it does to that projection may land after the rebuild
        # deletes the checkpoint, or the rebuild resumes past history. So every
        # touch of a projection from a track (dispatch, skip save) and the
        # rebuild's delete+clear hold that projection's lock, and the rebuild
        # bumps its generation under the lock: a track still holding the old
        # generation sees it is stale and leaves the projection alone. The
        # projection is fed again from the next plan (restart), as before.
        self._checkpoint_locks: dict[str, asyncio.Lock] = {}
        self._generations: dict[str, int] = {}
        self._running = False
        # Closed by stop(): a handler still suspended when the drains were
        # closed must not wake a fresh one afterwards. Open by default, since
        # dispatch_event() is used without start().
        self._wakes_open = True
        self._last_error: Exception | None = None
        self._live_boundary_nonce: int = 0
        # Projections held below an event they failed to apply, by name. Set
        # on the failure, cleared once the projection has taken that event.
        self._held: dict[str, _HeldProjection] = {}
        # Projections that failed an event live and have not been live since.
        # A re-plan (reconnect, retry) puts them back on a catching-up track,
        # which goes live once it delivers the head rather than past it: the
        # event past it may never come, and their drains would stay asleep.
        # Kept across re-plans and later failures until they are live again.
        self._resume_live: set[str] = set()
        # The running plan's task group, so a held projection's retry track
        # runs in it and is cancelled with the plan. None between plans.
        self._task_group: asyncio.TaskGroup | None = None

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

        # One drain per ProcessManager, so a slow process_pending() holds up
        # neither the track it is on nor another ProcessManager (#1528).
        self._drains: dict[str, ProcessManagerDrain] = {
            name: self._drain_for(name, projection)
            for name, projection in self._projections.items()
            if isinstance(projection, ProcessManager)
        }

        # Until start() plans them against real checkpoints, everything shares
        # one catching-up track. Each subscription attempt replaces this.
        self._tracks: list[_SubscriptionTrack] = [
            _SubscriptionTrack(
                name="all",
                from_position=0,
                # A copy: tracks own their membership.
                projections=dict(self._projections),
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
        """True if running, with no active error, not halted, and no projection held."""
        return self._running and self._last_error is None and self._halt is None and not self._held

    @property
    def held_projections(self) -> dict[str, ProjectionHandlerFailedError]:
        """Projections held below an event they failed to apply (#1696), by name.

        Each value names the event. The projection's checkpoint is below it
        and it is being fed it again with backoff; it leaves this view once
        it has applied it.
        """
        return {name: held.failure for name, held in self._held.items()}

    @property
    def halted(self) -> SubscriptionHaltedError | None:
        """The halt at an undecodable stored event (ADR-026), or None.

        Stays set across a restart until the coordinator has got past the
        position, so a health check sees the stop until it is really fixed.
        """
        return self._halt

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

        Every projection is offered the event, even after one of them failed
        it. A failed one is not checkpointed past it, so dispatching the same
        envelope again redelivers it to that projection only.

        Raises:
            ProjectionHandlerFailedError: A projection failed the event. With
                several, an ExceptionGroup of them.
        """
        failures: list[ProjectionHandlerFailedError] = []
        for track in self._tracks:
            failures.extend(await self._dispatch_to_track(track, envelope))
        if len(failures) == 1:
            raise failures[0]
        if failures:
            raise ExceptionGroup("Projections failed to apply an event", failures)

    async def start(self) -> None:
        """
        Start the subscription coordinator with exponential-backoff retry.

        Retries on any transient error (e.g. UNAVAILABLE, RST_STREAM,
        connection reset) with exponential backoff. An undecodable stored
        event (gRPC DATA_LOSS) is not transient: retrying fails at the same
        position, so the coordinator halts there instead (#360, ADR-026). See
        ``undecodable_recheck_interval`` for what halting means.

        Raises:
            SubscriptionHaltedError: On an undecodable stored event, when
                ``undecodable_recheck_interval`` is None. ``halted`` holds the
                same error. Fix it as its message says, then call start()
                again.
        """
        if self._running:
            logger.warning("Subscription coordinator already running")
            return

        self._running = True
        self._wakes_open = True
        backoff = 1.0

        try:
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
                    undecodable = _find_undecodable(e)
                    if undecodable is not None:
                        halt = await self._halt_at(undecodable)
                        if self._undecodable_recheck_interval is None:
                            self._running = False
                            raise halt from undecodable
                        await asyncio.sleep(self._undecodable_recheck_interval)
                        continue
                    self._last_error = e
                    logger.warning(
                        "Subscription error — retrying in %.1fs",
                        backoff,
                        extra={"error": str(e)},
                        exc_info=True,
                    )
                    await asyncio.sleep(backoff)
                    backoff = min(backoff * 2, 30.0)
        finally:
            # However start() ends - stopped, cancelled, or failed - no drain
            # task may outlive it.
            await self._close_drains(self._drains)

        logger.info("Subscription coordinator stopped")

    async def _halt_at(self, undecodable: UndecodableEventError) -> SubscriptionHaltedError:
        """Enter the halted state at ``undecodable``'s position.

        Every track is already cancelled (one failing track fails the plan)
        and nothing saves a checkpoint past the position: no projection was
        handed the event. ProcessManager drains are stopped too, so no side
        effect runs while the read model is stuck. A live one is woken again
        by the next plan.
        """
        await self._close_drains(self._drains)
        previous = self._halt
        halt = SubscriptionHaltedError(undecodable.global_nonce, self._undecodable_recheck_interval)
        self._halt = halt
        # Nothing can clear it until a new plan says what still needs N.
        self._halt_pending_tracks = set()
        extra: dict[str, str | int | float | None] = {
            "global_nonce": undecodable.global_nonce,
            "recheck_interval": self._undecodable_recheck_interval,
        }
        if previous is None or previous.global_nonce != halt.global_nonce:
            logger.error("%s", halt.message, extra=extra)
        else:
            # Same stop, re-checked: one ERROR per position, not per attempt.
            logger.debug("Still halted at undecodable stored event", extra=extra)
        return halt

    def _plan_halt_clearance(self, tracks: list[_SubscriptionTrack]) -> None:
        """Clear the halt, or note which tracks must pass its position first.

        A track starting above the position cannot meet it again. One at or
        below it has a projection that has not passed it: the halt holds
        until that track delivers an event at or past the position, which
        happens only once the row decodes again.
        """
        if self._halt is None:
            return
        position = self._halt.global_nonce
        self._halt_pending_tracks = {
            track.name for track in tracks if track.from_position <= position
        }
        if not self._halt_pending_tracks:
            self._clear_halt()

    def _clear_halt(self) -> None:
        if self._halt is None:
            return
        logger.info(
            "Resumed past undecodable stored event",
            extra={"global_nonce": self._halt.global_nonce},
        )
        self._halt = None
        self._halt_pending_tracks = set()
        # Drains were held while halted; whatever is pending is actionable now.
        self._wake_live_drains(self._tracks)

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
        # Every drain is stopped BEFORE planning: planning may clear a
        # ProcessManager's data on a version change, and a drain still running
        # from the previous subscription must not process underneath that, nor
        # run into a replay of the same ProcessManager. The live ones are woken
        # again below, so nothing pending is lost.
        await self._close_drains(self._drains)

        self._live_boundary_nonce = await self._read_head_nonce()
        self._tracks = await self._plan_tracks(self._live_boundary_nonce)
        self._plan_halt_clearance(self._tracks)

        # Items left pending before a restart must not wait for the next live
        # event to be noticed: wake every ProcessManager that is live already.
        self._wake_live_drains(self._tracks)

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
        # A projection failing an event is not a track failure: it is taken
        # off its track and retried on its own (``_hold``), in this group.
        try:
            async with asyncio.TaskGroup() as group:
                self._task_group = group
                for track in self._tracks:
                    group.create_task(self._run_track(track))
        finally:
            self._task_group = None

    async def _run_track(self, track: _SubscriptionTrack) -> None:
        """Feed one track from its own subscription until stopped."""
        async for envelope in self._subscribe(track):
            if not self._running:
                break
            self._last_error = None
            failures = await self._dispatch_to_track(track, envelope)
            for failure in failures:
                self._hold(track, failure)
            if (
                self._halt is not None
                and track.name in self._halt_pending_tracks
                and (envelope.metadata.global_nonce or 0) >= self._halt.global_nonce
            ):
                self._halt_pending_tracks.discard(track.name)
                if not self._halt_pending_tracks:
                    self._clear_halt()
            if failures and not track.projections and track.name != LIVE_TRACK:
                # Every member is held and retried on its own track; reading
                # on would feed nobody. The live track stays, so the
                # coordinator always holds a subscription on the live tail.
                return

    def _subscribe(self, track: _SubscriptionTrack) -> AsyncIterator[EventEnvelope[DomainEvent]]:
        """Subscribe for ``track``, decoding only the types its projections handle.

        Filter before decode (ADR-027): an event no projection on the track
        handles is skipped, so it must not be decoded either. An evolved type
        with no upcaster would otherwise raise an ``UndecodableEventError``
        and halt every track for projections that never asked for it.
        """
        if not self._filters_types:
            return self._event_store.subscribe(from_global_nonce=track.from_position)
        store = cast("TypeFilteringSubscriber", self._event_store)
        return store.subscribe(
            from_global_nonce=track.from_position, event_types=_TrackEventTypes(track, self._current_members)
        )

    def _hold(self, track: _SubscriptionTrack, failure: ProjectionHandlerFailedError) -> None:
        """Take the projection that failed off ``track`` and retry it alone.

        The track's cursor moves on and the projection's checkpoint must not,
        so the projection cannot stay on the track: its next successful event
        would checkpoint past the failed one (#1696). Off the track it is fed
        nothing, so its checkpoint stays below the failure, and every other
        projection keeps consuming. After a backoff a track of its own
        resumes it from that checkpoint, which delivers the event again.
        """
        name = failure.projection_name
        projection = track.projections.pop(name, None)
        track.unsaved_skips.pop(name, None)
        # The generation it was held under. A rebuild after this point makes
        # the retry stale (see _retry_held).
        generation = track.generations.pop(name, 0)
        if projection is None:
            return
        attempts = self._held[name].attempts if name in self._held else 1
        delay = _held_retry_delay(attempts)
        logger.warning(
            "Projection held below an event it failed to apply; retrying it alone",
            extra={
                "projection_name": name,
                "event_type": failure.event_type,
                "global_nonce": failure.global_nonce,
                "track": track.name,
                "attempts": attempts,
                "retry_in_seconds": delay,
            },
        )
        group = self._task_group
        if group is None:
            return  # no plan running; the next plan resumes it from its checkpoint
        try:
            group.create_task(self._retry_held(name, projection, generation, delay))
        except RuntimeError:
            # The plan is shutting down; the next one resumes it from its
            # checkpoint, which is still below the failure.
            return

    async def _retry_held(
        self, name: str, projection: CheckpointedProjection, generation: int, delay: float
    ) -> None:
        """After ``delay``, feed a held projection from its checkpoint on its own track.

        ``generation`` is the one it was held under, never the current one. A
        rebuild_projection() in between bumps the generation first and deletes
        the checkpoint after, so a retry reading the generation now could pair
        the new generation with the old checkpoint and, once the rebuild has
        cleared the data, resume above history it no longer has. Read under
        the checkpoint lock, and given up if the generation moved: a rebuilt
        projection is fed by the next plan, from 0, as rebuild_projection says.
        """
        await asyncio.sleep(delay)
        if not self._running:
            return
        async with self._checkpoint_lock(name):
            if self._generations.get(name, 0) != generation:
                logger.info(
                    "Held projection was rebuilt; dropping its retry",
                    extra={"projection_name": name},
                )
                return
            resume_from = await self._resume_position(name, projection)
        track = self._catch_up_track(
            f"retry-{name}",
            [_BehindProjection(name, projection, resume_from, generation)],
            self._live_boundary_nonce,
        )
        self._tracks.append(track)
        logger.info(
            "Starting subscription track",
            extra={
                "track": track.name,
                "from_position": track.from_position,
                "is_catching_up": track.is_catching_up,
                "live_boundary_nonce": self._live_boundary_nonce,
                "projection_count": 1,
                "projection_names": [name],
            },
        )
        # Already live: whatever it left pending is actionable again.
        self._wake_live_drains([track])
        try:
            await self._run_track(track)
        finally:
            if track in self._tracks:
                self._tracks.remove(track)

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
        try:
            head_events, _is_end, _next = await self._event_store.read_all(
                from_global_nonce=sys.maxsize, max_count=1, forward=False,
            )
        except UndecodableEventError as e:
            # The probe reads only the highest row, so the undecodable event
            # IS the head. Its position is enough for the boundary, and lets
            # projections whose checkpoint an operator moved past it resume
            # (ADR-026). Anyone still before it hits the error again on
            # subscribe, which is the intended stop.
            # Once per position: while halted there, re-checks repeat it.
            already_halted_here = self._halt is not None and self._halt.global_nonce == e.global_nonce
            logger.log(
                logging.DEBUG if already_halted_here else logging.WARNING,
                "Head event is undecodable; using its position as the live boundary",
                extra={"global_nonce": e.global_nonce},
            )
            return e.global_nonce
        if head_events and head_events[0].metadata.global_nonce is not None:
            return head_events[0].metadata.global_nonce
        return 0

    async def stop(self) -> None:
        """Stop the subscription coordinator gracefully.

        Sets `_running` to False which causes the subscription loop in `start()`
        to exit on the next iteration, and cancels and awaits every
        ProcessManager drain. The drains are closed even if start() was never
        called, since dispatch_event() can start them too.
        """
        # Stop admitting first, so no wake can spawn a drain after it is closed.
        was_running, self._running = self._running, False
        self._wakes_open = False
        await self._close_drains(self._drains)

        if not was_running:
            return

        logger.info("Stopping subscription coordinator")

    async def wait_for_process_managers(self) -> None:
        """Wait until every ProcessManager drain is idle with no wake pending.

        Drains run off the dispatch path, so returning from dispatch_event()
        does not mean ``process_pending()`` has run. Test and fitness tooling
        that needs to observe its effects waits here first.
        """
        await asyncio.gather(*(drain.settled() for drain in self._drains.values()))

    def _wake_live_drains(self, tracks: list[_SubscriptionTrack]) -> None:
        for track in tracks:
            if track.is_catching_up:
                continue
            for name in list(track.projections):
                if self._is_member(track, name):
                    self._wake(name)

    def _wake(self, name: str) -> None:
        """Wake ``name``'s drain, if it is a ProcessManager and wakes are open.

        Never while halted at an undecodable event: re-check attempts re-plan
        and would otherwise run side effects between failures (#360).
        ``_clear_halt`` wakes the live drains once the halt is over. Nor while
        ``name`` is held below an event it failed (#1696): its to-do list is
        missing that event. Recovery wakes it.
        """
        drain = self._drains.get(name)
        if drain is not None and self._wakes_open and self._halt is None and name not in self._held:
            drain.wake()

    def _drain_for(self, name: str, process_manager: ProcessManager) -> ProcessManagerDrain:
        return ProcessManagerDrain(
            process_manager,
            may_run=lambda: self._halt is None and self._is_live(name),
        )

    def _is_live(self, name: str) -> bool:
        """True when the track currently feeding ``name`` is past its catch-up.

        Read off the track, not the coordinator: a sibling track reaching live
        must not unlock side effects for a replay still in history. A
        projection on no track is not being fed, so it is not live either,
        and nor is one whose only track was planned before it was rebuilt.
        Nor is one held below an event it failed (#1696): a retry track can
        start live, but its to-do list is missing that event until it applies
        it.
        """
        if name in self._held:
            return False
        return any(
            self._is_member(track, name) and not track.is_catching_up for track in self._tracks
        )

    def _is_member(self, track: _SubscriptionTrack, name: str) -> bool:
        """True while ``track`` feeds ``name``: on it, and not rebuilt since it was planned.

        The one test for dispatch, type filtering and drain eligibility.
        """
        return name in track.projections and (
            track.generations.get(name, 0) == self._generations.get(name, 0)
        )

    def _current_members(self, track: _SubscriptionTrack) -> list[CheckpointedProjection]:
        return [p for name, p in track.projections.items() if self._is_member(track, name)]

    @staticmethod
    async def _close_drains(drains: dict[str, ProcessManagerDrain]) -> None:
        await asyncio.gather(*(drain.close() for drain in drains.values()))

    async def _plan_tracks(self, live_boundary_nonce: int) -> list[_SubscriptionTrack]:
        """
        Split the projections into subscription tracks by position.

        A projection that needs nothing at or below ``live_boundary_nonce`` is
        at head and can take live events straight away. One that still needs
        history - never run, version bumped and cleared, or left behind by a
        failure or a crash - has to catch up first, and how far it has to go
        decides where:

        - Resuming from a checkpoint within ``near_head_window`` events of
          the boundary: the near-head track. It starts at the lowest of these
          positions, so every member waits at most the window, then runs live. Its latency depends on
          its own distance from head, never on a rebuild's (syntropic137#1554:
          23 projections 9 events behind sat on rebuild tracks from 0 for an
          hour). Not merged into the live track, because that would put the
          projections already at head back into catch-up, and gate their
          ProcessManagers, for the length of the window.
        - Rebuilding from 0, or further behind: a replay track, so the
          projections at or near head keep consuming while it replays (#1318).

        Grouping on position rather than on "was a rebuild triggered" is what
        makes this hold across a reconnect: a rebuild interrupted halfway has
        a valid checkpoint at a low position, and it must stay on a replay
        track rather than drag the whole plan back to where it got to.

        A track per rebuild, not one track for "the rebuilds": two rebuilds
        sharing a cursor pace each other exactly as a rebuild and a live
        projection do, and #1318 was two version bumps in the same deploy.
        The count is capped at ``replay_concurrency`` because each track is a
        connection held for the length of a replay, and a version bump rolled
        out across a whole read model would otherwise open one per projection.
        Past the cap the rebuilds share, grouped by position: they are sorted
        and split into contiguous runs, so the ones sharing a cursor are the
        ones whose positions are closest and the least is replayed twice.

        Args:
            live_boundary_nonce: Head snapshot separating history from live

        Returns:
            The tracks to subscribe, at-head track first. It is always
            present, even when empty, so the coordinator always holds one
            subscription on the live tail. A near-head track follows when any
            projection is near head, then at most ``replay_concurrency``
            replay tracks.
        """
        at_head: dict[str, CheckpointedProjection] = {}
        at_head_generations: dict[str, int] = {}
        near_head: list[_BehindProjection] = []
        far_behind: list[_BehindProjection] = []

        for name, projection in self._projections.items():
            # Captured BEFORE the checkpoint is read: a rebuild landing after
            # the read makes this plan stale for the projection, not wrong.
            generation = self._generations.get(name, 0)
            resume_from = await self._resume_position(name, projection)
            if resume_from > live_boundary_nonce:
                at_head[name] = projection
                at_head_generations[name] = generation
                continue
            entry = _BehindProjection(name, projection, resume_from, generation)
            # 0 is a rebuild (no checkpoint, or a version bump): a replay,
            # however short the stream is today. Otherwise, count the events
            # this projection still needs at or below the boundary.
            needs = live_boundary_nonce - resume_from + 1
            if resume_from > 0 and needs <= self._near_head_window:
                near_head.append(entry)
            else:
                far_behind.append(entry)

        tracks = [
            _SubscriptionTrack(
                name=LIVE_TRACK,
                from_position=live_boundary_nonce + 1,
                projections=at_head,
                # Starts above the boundary by construction, so every event it
                # ever sees is live.
                is_catching_up=False,
                generations=at_head_generations,
            )
        ]

        if near_head:
            tracks.append(self._catch_up_track("near-head", near_head, live_boundary_nonce))

        far_behind.sort(key=lambda entry: entry.resume_from)
        track_count = min(len(far_behind), self._replay_concurrency)
        if track_count:
            size, larger = divmod(len(far_behind), track_count)
            start = 0
            for index in range(track_count):
                end = start + size + (1 if index < larger else 0)
                tracks.append(
                    self._catch_up_track(
                        f"replay-{index}", far_behind[start:end], live_boundary_nonce
                    )
                )
                start = end

        return tracks

    @staticmethod
    def _catch_up_track(
        name: str,
        group: list[_BehindProjection],
        live_boundary_nonce: int,
    ) -> _SubscriptionTrack:
        # A track can only start where its furthest-behind member needs it
        # to; members already past that skip the redelivered events on their
        # checkpoint.
        from_position = min(entry.resume_from for entry in group)
        return _SubscriptionTrack(
            name=name,
            from_position=from_position,
            projections={entry.name: entry.projection for entry in group},
            is_catching_up=from_position <= live_boundary_nonce,
            skips_saved_at=from_position,
            generations={entry.name: entry.generation for entry in group},
        )

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
    ) -> list[ProjectionHandlerFailedError]:
        """
        Dispatch an event to the projections on one track.

        Tracks that track's own catch-up/live transition based on global_nonce.

        Delivery to the track's projections is sequential, and stays that way:
        they share one cursor, so it cannot advance until the slowest of them
        has taken this event whatever order they are handed it in. Isolation
        comes from which projections share a track, decided in
        ``_plan_tracks``, and not from this loop.

        A projection that fails the event does not stop the others taking
        it. Its checkpoint stays below the event, and the failure is returned
        for the caller to act on: ``_run_track`` takes it off the track.

        Args:
            track: The track the event arrived on
            envelope: Event envelope to dispatch

        Returns:
            One failure per projection that did not apply the event.
        """
        global_nonce = envelope.metadata.global_nonce or 0
        failures: list[ProjectionHandlerFailedError] = []

        # Transition: catch-up -> live when this track passes the boundary
        # nonce. Uses > (strictly greater): events at the boundary were
        # already in the store when we subscribed, so they are historical.
        if track.is_catching_up and global_nonce > self._live_boundary_nonce:
            self._go_live(track, global_nonce)

        for name, projection in list(track.projections.items()):
            held = self._held.get(name)
            if held is not None and global_nonce > held.failure.global_nonce:
                # It must take the event it is held below first. Handing it
                # a later one, even one it skips, would checkpoint past it.
                continue
            # Held across the handler, so rebuild_projection() cannot delete
            # the checkpoint underneath it and the handler's own save cannot
            # land after the delete.
            async with self._checkpoint_lock(name):
                if not self._is_member(track, name):
                    continue  # rebuilt since this track was planned
                try:
                    await self._dispatch_under_lock(track, name, projection, envelope)
                except ProjectionHandlerFailedError as failure:
                    failures.append(failure)
                    self._record_failure(failure)
                    if not track.is_catching_up:
                        self._resume_live.add(name)
                    failed = True
                else:
                    failed = False
            if failed:
                # Wakes and may_run only gate a drain that has not started. One
                # already inside process_pending() would go on running side
                # effects while held, so it is cancelled. Safe: the contract
                # makes process_pending() idempotent, and recovery wakes it.
                drain = self._drains.get(name)
                if drain is not None:
                    await drain.close()
                continue
            held = self._held.get(name)
            if held is not None and global_nonce >= held.failure.global_nonce:
                # Delivered in order from below the failure without failing:
                # the event it was held below has been applied.
                del self._held[name]
                logger.info(
                    "Held projection applied the event it was held below",
                    extra={"projection_name": name, "global_nonce": held.failure.global_nonce},
                )
                # Recovered: a live ProcessManager's to-do list is complete again.
                if not track.is_catching_up:
                    self._resume_live.discard(name)
                    self._wake(name)

        # Save the held-back skips periodically, and all of them once the
        # track has delivered the last historical event, so a projection that
        # reaches head by skipping is checkpointed at head before it goes live.
        if track.unsaved_skips and (
            global_nonce >= self._live_boundary_nonce
            or global_nonce - track.skips_saved_at >= CATCH_UP_SKIP_CHECKPOINT_INTERVAL
        ):
            await self._save_skips(track, global_nonce)

        # A projection that failed live event N and was re-planned with a head
        # H >= N (a reconnect) catches up on a track that would only go live
        # past H, at an event that may never come: its drain would never run
        # again. Once the track has delivered H with it on board and not held,
        # the track is at head, so it goes live then (see ``_resume_live``).
        # Every member has taken H; nothing is replayed.
        if (
            track.is_catching_up
            and global_nonce >= self._live_boundary_nonce
            and any(
                name in self._resume_live and name not in self._held
                for name in track.projections
                if self._is_member(track, name)
            )
        ):
            self._go_live(track, global_nonce)

        return failures

    def _go_live(self, track: _SubscriptionTrack, global_nonce: int) -> None:
        track.is_catching_up = False
        self._resume_live.difference_update(
            name for name in track.projections if name not in self._held
        )
        logger.info(
            "Subscription track transitioned to live mode",
            extra={
                "track": track.name,
                "global_nonce": global_nonce,
                "live_boundary_nonce": self._live_boundary_nonce,
            },
        )
        # Whatever the replay left on these to-do lists is now actionable.
        self._wake_live_drains([track])

    def _record_failure(self, failure: ProjectionHandlerFailedError) -> None:
        """Note ``failure`` against its projection, counting repeats of one event."""
        name = failure.projection_name
        previous = self._held.get(name)
        same_event = previous is not None and previous.failure.global_nonce == failure.global_nonce
        attempts = previous.attempts + 1 if previous is not None and same_event else 1
        self._held[name] = _HeldProjection(failure=failure, attempts=attempts)

    async def _dispatch_under_lock(
        self,
        track: _SubscriptionTrack,
        name: str,
        projection: CheckpointedProjection,
        envelope: EventEnvelope[DomainEvent],
    ) -> None:
        """Deliver one event to one projection; caller holds its checkpoint lock."""
        event_type = envelope.metadata.event_type or "Unknown"
        global_nonce = envelope.metadata.global_nonce or 0

        # Check if projection subscribes to this event type
        subscribed = projection.get_subscribed_event_types()
        if subscribed is not None and event_type not in subscribed:
            # Skip but advance checkpoint. During catch-up the save is
            # deferred and batched (CATCH_UP_SKIP_CHECKPOINT_INTERVAL);
            # live, it is saved at once as before.
            if track.is_catching_up:
                track.unsaved_skips[name] = global_nonce
            else:
                await self._advance_checkpoint_if_behind(name, global_nonce)
            return

        # A skip held back for this projection is superseded by the event
        # it is about to handle: handle_event() checkpoints past it. If
        # handling fails, the skipped events are simply skipped again on
        # the next replay.
        track.unsaved_skips.pop(name, None)

        # Check if projection is already past this position
        checkpoint = await self._checkpoint_store.get_checkpoint(name)
        if checkpoint and checkpoint.global_position >= global_nonce:
            return  # Already processed

        # Dispatch to projection
        await self._dispatch_to_projection(track, projection, envelope)

    async def _save_skip(self, track: _SubscriptionTrack, name: str) -> None:
        async with self._checkpoint_lock(name):
            position = track.unsaved_skips.pop(name, None)
            if position is not None and self._is_member(track, name):
                await self._advance_checkpoint_if_behind(name, position)

    async def _save_skips(self, track: _SubscriptionTrack, global_nonce: int) -> None:
        for name in list(track.unsaved_skips):
            await self._save_skip(track, name)
        track.skips_saved_at = global_nonce

    async def _dispatch_to_projection(
        self,
        track: _SubscriptionTrack,
        projection: CheckpointedProjection,
        envelope: EventEnvelope[DomainEvent],
    ) -> None:
        """
        Dispatch an event to a single projection with error handling.

        A handler that fails, by returning FAILURE or by raising, raises
        ProjectionHandlerFailedError here. Logging and moving on is not an
        option: the track's cursor moves on, and the projection's next
        successful event would save a checkpoint above the one that failed.
        Nothing re-reads below a checkpoint, so the event would be lost until
        a rebuild, with the read model reporting itself current
        (syntropic137#1696). The caller holds the projection below the event
        instead (``_hold``) and feeds it again after a backoff; a handler that
        keeps failing holds its read model at that event, visibly
        (``held_projections``, ``is_healthy``), rather than skipping it.

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
                raise ProjectionHandlerFailedError(name, event_type, global_nonce)
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

                # ProcessManager: the to-do item is written and checkpointed,
                # so ask its drain to run. Never awaited here - the track's
                # cursor is shared, and waiting on side effects would hold
                # every projection on it to the drain's pace (#1528). Live
                # events only; the drain re-checks the track before it calls
                # process_pending(), which is never called during catch-up.
                if not track.is_catching_up:
                    self._wake(name)
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

        except ProjectionHandlerFailedError:
            raise
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
            raise ProjectionHandlerFailedError(name, event_type, global_nonce) from e

    async def _advance_checkpoint_if_behind(
        self,
        projection_name: str,
        position: int,
    ) -> None:
        """
        Advance checkpoint for skipped events (event type not subscribed).

        The caller holds ``projection_name``'s checkpoint lock and has
        checked its track is current, which is what keeps this save from
        landing after a ``rebuild_projection`` delete.

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

    def _checkpoint_lock(self, projection_name: str) -> asyncio.Lock:
        lock = self._checkpoint_locks.get(projection_name)
        if lock is None:
            lock = self._checkpoint_locks[projection_name] = asyncio.Lock()
        return lock

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

        async with self._checkpoint_lock(projection_name):
            # From here every running track is stale for this projection: it
            # dispatches nothing to it and saves no checkpoint for it, now or
            # after the delete below. Taking the lock first lets a dispatch or
            # skip save already in flight finish before the delete.
            self._generations[projection_name] = self._generations.get(projection_name, 0) + 1
            self._resume_live.discard(projection_name)  # replays from 0
            for track in self._tracks:
                track.unsaved_skips.pop(projection_name, None)

        # A ProcessManager must not be processing while its to-do list is
        # cleared and replayed underneath it. Its tracks are stale now, so
        # _is_live() keeps the drain from restarting it.
        drain = self._drains.get(projection_name)
        if drain is not None:
            await drain.close()

        async with self._checkpoint_lock(projection_name):
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
