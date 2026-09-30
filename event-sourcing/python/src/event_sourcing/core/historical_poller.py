"""
HistoricalPoller base class for cold-start-safe external event ingestion.

This module provides ``HistoricalPoller``, a base class for background
pollers that fetch events from external sources (APIs, message queues,
RSS feeds, etc.) and feed them into a processing pipeline.

The core problem: external event APIs (e.g. GitHub Events API) return
**historical** events alongside new ones. On a fresh start with no
persisted cursor, the poller would re-process the entire event history,
potentially triggering duplicate side effects (workflow executions,
notifications, etc.) and causing billing events.

The ``HistoricalPoller`` makes cold-start safety the default through a
template method pattern:

1. **Warm start** (cursor exists): all events are processed normally.
2. **Cold start** (no cursor): events are split by ``created_at``
   timestamp relative to the poller's start time. Historical events
   (before startup) are skipped; post-startup events are processed.
   The cursor is persisted so subsequent polls resume correctly.

The ``poll()`` method is concrete and non-overridable -- subclasses
implement ``fetch()`` and ``process()`` but cannot bypass the fence.
This is a poka-yoke: the safe behavior is the default, and processing
historical events requires an explicit cursor (opt-in, not opt-out).

**Relationship to other ESP patterns:**

- ``CheckpointedProjection`` -- consumes internal event store streams
  (replay-safe read models)
- ``ProcessManager`` -- consumes internal events with side effects
  (to-do list pattern)
- ``HistoricalPoller`` -- ingests external event sources with
  cold-start safety (cursor + timestamp fence)

See Also:
    - ADR-060: Restart-safe trigger deduplication (Syntropic137)
"""

from __future__ import annotations

import logging
from abc import ABC, abstractmethod
from dataclasses import dataclass
from datetime import UTC, datetime
from typing import Any, Protocol, final, runtime_checkable

logger = logging.getLogger(__name__)


# ---------------------------------------------------------------------------
# Supporting types
# ---------------------------------------------------------------------------


@dataclass(frozen=True, slots=True)
class PollEvent:
    """A single event from an external source.

    The ``created_at`` timestamp is required for cold-start filtering.
    Sources that do not provide reliable timestamps are not safe for
    historical polling and should not use this base class.

    Attributes:
        created_at: When the event was created at the source (UTC).
        data: Raw event payload. Structure is source-specific.
    """

    created_at: datetime
    data: dict[str, Any]

    def __post_init__(self) -> None:
        if self.created_at.tzinfo is None:
            msg = (
                "PollEvent.created_at must be timezone-aware (UTC). "
                "Got naive datetime. Use datetime.now(UTC) or similar."
            )
            raise ValueError(msg)


@dataclass(frozen=True, slots=True)
class CursorData:
    """Opaque cursor state for resuming from a known position.

    The exact semantics are source-specific (e.g. ETag for HTTP APIs,
    offset for Kafka, sequence number for SQS).

    Attributes:
        value: Primary cursor value (e.g. ETag string, offset number).
        metadata: Additional cursor state (e.g. last event ID).
    """

    value: str
    metadata: dict[str, str] | None = None


@dataclass(frozen=True, slots=True)
class PollResult:
    """Result of a single poll cycle from an external source.

    Attributes:
        events: Events returned by the source, ordered oldest-first.
        cursor: Cursor state to persist for resuming later.
        has_new: False when the source returned "no changes"
            (e.g. HTTP 304 Not Modified). When False, ``events``
            is empty and ``cursor`` may be unchanged.
    """

    events: list[PollEvent]
    cursor: CursorData
    has_new: bool


@runtime_checkable
class CursorStore(Protocol):
    """Protocol for persisting poller cursor state across restarts.

    Implementations must be durable (e.g. Postgres, file system).
    In-memory implementations are only appropriate for tests.
    """

    async def save(self, source_key: str, cursor: CursorData) -> None:
        """Persist cursor state for a source."""
        ...

    async def load(self, source_key: str) -> CursorData | None:
        """Load cursor state for a source. Returns None if not found."""
        ...

    async def load_all(self) -> dict[str, CursorData]:
        """Load all persisted cursors. Used during initialization."""
        ...


# ---------------------------------------------------------------------------
# Base class
# ---------------------------------------------------------------------------


class HistoricalPoller(ABC):
    """Base class for cold-start-safe external event polling.

    Subclasses implement ``fetch()`` to retrieve events from the external
    source and ``process()`` to feed them into the application pipeline.
    The ``poll()`` template method handles cold-start detection, timestamp
    filtering, and cursor persistence automatically.

    **Cold-start behavior:** On the first poll for a source with no
    persisted cursor, only events created after the poller's start time
    are processed. Historical events are skipped but the cursor is still
    persisted, so subsequent polls resume from the correct position.

    **Warm-start behavior:** When a cursor exists (from a previous run),
    all returned events are processed. The external source is expected to
    return only new events when a cursor is provided (e.g. via ETag,
    offset, or similar mechanism).

    Usage::

        class GitHubEventPoller(HistoricalPoller):
            async def fetch(self, source_key: str) -> PollResult:
                # Call GitHub Events API with ETag from cursor
                ...

            async def process(self, source_key: str, events: list[PollEvent]) -> None:
                # Feed events through the EventPipeline
                for event in events:
                    await self._pipeline.ingest(normalize(event))

        poller = GitHubEventPoller(cursor_store=postgres_cursor_store)
        await poller.initialize()
        # In poll loop:
        await poller.poll("owner/repo")
    """

    def __init__(self, cursor_store: CursorStore) -> None:
        self._cursor_store = cursor_store
        self._primed_sources: set[str] = set()
        self._started_at: datetime = datetime.now(UTC)
        self._initialized: bool = False

    async def initialize(self) -> None:
        """Load persisted cursors from the store.

        Sources with existing cursors are marked as primed (warm start).
        Must be called before the first ``poll()`` call.
        """
        cursors = await self._cursor_store.load_all()
        self._primed_sources = set(cursors.keys())
        self._initialized = True
        if self._primed_sources:
            logger.info(
                "HistoricalPoller initialized with %d primed source(s): %s",
                len(self._primed_sources),
                ", ".join(sorted(self._primed_sources)),
            )

    @abstractmethod
    async def fetch(self, source_key: str) -> PollResult:
        """Fetch events from the external source.

        Implementations should use the persisted cursor (if available)
        to request only new events from the source. For example, an
        HTTP-based poller would send an ``If-None-Match`` header with
        the stored ETag.

        Args:
            source_key: Identifies the source (e.g. "owner/repo").

        Returns:
            PollResult with events and updated cursor state.
        """
        ...

    @abstractmethod
    async def process(
        self,
        source_key: str,
        events: list[PollEvent],
        is_replay: bool = False,
    ) -> None:
        """Process fetched events through the application pipeline.

        Called only for events that pass the cold-start fence. On warm
        start, all events are passed. On cold start, only events created
        after the poller's start time are passed.

        Args:
            source_key: Identifies the source (e.g. "owner/repo").
            events: Events to process, ordered oldest-first.
            is_replay: True when this batch is being replayed during the
                cold-start path (events that survived the timestamp fence
                on the very first poll for this source). Subclasses may
                use this signal to mark events as un-primed so downstream
                consumers skip side-effecting work like trigger evaluation.
                False on the warm-start (steady-state) path.

                Defaults to False so existing subclasses remain Liskov-safe
                without modification.
        """
        ...

    @final
    async def poll(self, source_key: str) -> None:
        """Poll a source for new events with cold-start safety.

        This is a template method -- **not meant to be overridden**.
        Subclasses implement ``fetch()`` and ``process()`` instead.
        Enforced at runtime via ``@final`` and statically by the
        ``check_historical_poller_structure()`` fitness function.

        The cold-start fence works as follows:

        1. If the source has a persisted cursor (warm start), all
           returned events are processed.
        2. If the source has no cursor (cold start), events are split
           by the poller's start time:
           - Events with ``created_at < started_at`` are skipped
             (historical, would cause a flood).
           - Events with ``created_at >= started_at`` are processed
             (genuinely new, happened after the system came online).
        3. The cursor is always persisted, so subsequent polls resume
           from the correct position regardless of cold/warm start.

        Raises:
            RuntimeError: If ``initialize()`` has not been called.
        """
        if not self._initialized:
            msg = (
                f"{type(self).__name__}.poll() called before initialize(). "
                f"Call initialize() first to load persisted cursors."
            )
            raise RuntimeError(msg)

        result = await self.fetch(source_key)

        if not result.has_new:
            return

        if source_key in self._primed_sources:
            # Warm start: process events, then update cursor.
            # Ordering matters: if process() crashes, the old cursor
            # remains and events will be re-fetched on next poll (safe).
            if result.events:
                await self.process(source_key, result.events)
            await self._persist_cursor(source_key, result.cursor)
            return

        # Cold start: filter historical events, keep post-startup
        new_events = [e for e in result.events if e.created_at >= self._started_at]
        skipped = len(result.events) - len(new_events)

        # Prime cursor BEFORE process(). Trade-off: if process() crashes,
        # these events are lost (cursor advanced past them). The alternative
        # (prime after process) risks the restart storm if prime fails --
        # events would be re-fetched and re-processed on every restart.
        # Losing a few events on crash is cheaper than a billing flood.
        await self._prime(source_key, result.cursor)

        if new_events:
            logger.info(
                "Cold start for %s: processing %d new event(s) as replay, skipping %d historical",
                source_key,
                len(new_events),
                skipped,
            )
            await self.process(source_key, new_events, is_replay=True)
        elif result.events:
            logger.info(
                "Cold start for %s: skipped %d historical event(s), cursor primed",
                source_key,
                skipped,
            )

    @final
    async def _prime(self, source_key: str, cursor: CursorData) -> None:
        """Persist cursor and mark source as primed."""
        await self._cursor_store.save(source_key, cursor)
        self._primed_sources.add(source_key)

    @final
    async def _persist_cursor(self, source_key: str, cursor: CursorData) -> None:
        """Persist cursor for a primed source."""
        await self._cursor_store.save(source_key, cursor)

    @property
    def started_at(self) -> datetime:
        """When this poller instance was created (UTC)."""
        return self._started_at

    @property
    def primed_sources(self) -> frozenset[str]:
        """Sources that have been primed (have a cursor)."""
        return frozenset(self._primed_sources)
