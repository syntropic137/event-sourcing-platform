"""Tests for HistoricalPoller base class and cold-start fence."""

from __future__ import annotations

from datetime import UTC, datetime, timedelta

import pytest

from event_sourcing.core.historical_poller import (
    CursorData,
    CursorStore,
    HistoricalPoller,
    PollEvent,
    PollResult,
)

# ============================================================================
# In-Memory CursorStore for Testing
# ============================================================================


class MemoryCursorStore:
    """In-memory cursor store for testing."""

    def __init__(self, initial: dict[str, CursorData] | None = None) -> None:
        self._cursors: dict[str, CursorData] = dict(initial) if initial else {}

    async def save(self, source_key: str, cursor: CursorData) -> None:
        self._cursors[source_key] = cursor

    async def load(self, source_key: str) -> CursorData | None:
        return self._cursors.get(source_key)

    async def load_all(self) -> dict[str, CursorData]:
        return dict(self._cursors)


# ============================================================================
# Concrete HistoricalPoller for Testing
# ============================================================================


class StubPoller(HistoricalPoller):
    """Concrete poller for testing. Returns pre-configured results."""

    def __init__(self, cursor_store: CursorStore) -> None:
        super().__init__(cursor_store)
        self.fetch_results: dict[str, PollResult] = {}
        self.processed_events: list[tuple[str, list[PollEvent]]] = []
        self.fetch_calls: list[str] = []

    async def fetch(self, source_key: str) -> PollResult:
        self.fetch_calls.append(source_key)
        return self.fetch_results[source_key]

    async def process(self, source_key: str, events: list[PollEvent]) -> None:
        self.processed_events.append((source_key, list(events)))


def _event(minutes_ago: float, data: dict[str, str] | None = None) -> PollEvent:
    """Create a PollEvent with created_at relative to now."""
    return PollEvent(
        created_at=datetime.now(UTC) - timedelta(minutes=minutes_ago),
        data=data or {},
    )


def _event_at(timestamp: datetime, data: dict[str, str] | None = None) -> PollEvent:
    """Create a PollEvent with an exact timestamp."""
    return PollEvent(created_at=timestamp, data=data or {})


# ============================================================================
# CursorStore Protocol Tests
# ============================================================================


class TestCursorStoreProtocol:
    """Verify MemoryCursorStore satisfies the CursorStore protocol."""

    def test_implements_protocol(self) -> None:
        store = MemoryCursorStore()
        assert isinstance(store, CursorStore)


# ============================================================================
# HistoricalPoller Interface Tests
# ============================================================================


class TestHistoricalPollerInterface:
    """Tests for HistoricalPoller base class behavior."""

    def test_cannot_instantiate_without_abstract_methods(self) -> None:
        """Cannot instantiate HistoricalPoller without implementing fetch/process."""

        class IncompletePoller(HistoricalPoller):
            pass

        with pytest.raises(TypeError, match="abstract"):
            IncompletePoller(MemoryCursorStore())  # type: ignore[abstract]

    def test_started_at_is_set(self) -> None:
        """Poller records its start time on construction."""
        before = datetime.now(UTC)
        poller = StubPoller(MemoryCursorStore())
        after = datetime.now(UTC)
        assert before <= poller.started_at <= after

    def test_primed_sources_initially_empty(self) -> None:
        """No sources are primed before initialize()."""
        poller = StubPoller(MemoryCursorStore())
        assert poller.primed_sources == frozenset()

    @pytest.mark.asyncio
    async def test_initialize_loads_cursors(self) -> None:
        """initialize() loads persisted cursors and marks sources as primed."""
        store = MemoryCursorStore({"repo-a": CursorData(value="etag-1")})
        poller = StubPoller(store)
        await poller.initialize()
        assert poller.primed_sources == frozenset({"repo-a"})

    @pytest.mark.asyncio
    async def test_initialize_empty_store(self) -> None:
        """initialize() with empty store leaves no sources primed."""
        poller = StubPoller(MemoryCursorStore())
        await poller.initialize()
        assert poller.primed_sources == frozenset()


# ============================================================================
# Warm Start Tests (cursor exists)
# ============================================================================


class TestWarmStart:
    """When a cursor exists, all events are processed."""

    @pytest.mark.asyncio
    async def test_all_events_processed(self) -> None:
        """Warm start: all events are passed to process(), including old ones."""
        store = MemoryCursorStore({"repo-a": CursorData(value="etag-old")})
        poller = StubPoller(store)
        await poller.initialize()

        old_event = _event(minutes_ago=60)
        new_event = _event(minutes_ago=0)
        poller.fetch_results["repo-a"] = PollResult(
            events=[old_event, new_event],
            cursor=CursorData(value="etag-new"),
            has_new=True,
        )

        await poller.poll("repo-a")

        assert len(poller.processed_events) == 1
        source, events = poller.processed_events[0]
        assert source == "repo-a"
        assert len(events) == 2
        assert events[0] is old_event
        assert events[1] is new_event

    @pytest.mark.asyncio
    async def test_cursor_updated_on_warm_start(self) -> None:
        """Warm start: cursor is updated after processing."""
        store = MemoryCursorStore({"repo-a": CursorData(value="etag-old")})
        poller = StubPoller(store)
        await poller.initialize()

        poller.fetch_results["repo-a"] = PollResult(
            events=[_event(minutes_ago=0)],
            cursor=CursorData(value="etag-new"),
            has_new=True,
        )

        await poller.poll("repo-a")

        saved = await store.load("repo-a")
        assert saved is not None
        assert saved.value == "etag-new"

    @pytest.mark.asyncio
    async def test_no_new_events_skips_processing(self) -> None:
        """Warm start: has_new=False means no processing."""
        store = MemoryCursorStore({"repo-a": CursorData(value="etag-1")})
        poller = StubPoller(store)
        await poller.initialize()

        poller.fetch_results["repo-a"] = PollResult(
            events=[],
            cursor=CursorData(value="etag-1"),
            has_new=False,
        )

        await poller.poll("repo-a")

        assert len(poller.processed_events) == 0


# ============================================================================
# Cold Start Tests (no cursor)
# ============================================================================


class TestColdStart:
    """When no cursor exists, historical events are filtered by startup time."""

    @pytest.mark.asyncio
    async def test_historical_events_skipped(self) -> None:
        """Cold start: events before startup are not processed."""
        poller = StubPoller(MemoryCursorStore())
        await poller.initialize()

        # All events are from before the poller started
        old_events = [_event(minutes_ago=60), _event(minutes_ago=30)]
        poller.fetch_results["repo-a"] = PollResult(
            events=old_events,
            cursor=CursorData(value="etag-1"),
            has_new=True,
        )

        await poller.poll("repo-a")

        # No events should be processed
        assert len(poller.processed_events) == 0

    @pytest.mark.asyncio
    async def test_post_startup_events_processed(self) -> None:
        """Cold start: events after startup ARE processed."""
        poller = StubPoller(MemoryCursorStore())
        await poller.initialize()

        old_event = _event(minutes_ago=60)
        # Event created "in the future" relative to poller start (simulates
        # an event that arrived between startup and first poll)
        new_event = _event(minutes_ago=-1)  # 1 minute in the future

        poller.fetch_results["repo-a"] = PollResult(
            events=[old_event, new_event],
            cursor=CursorData(value="etag-1"),
            has_new=True,
        )

        await poller.poll("repo-a")

        assert len(poller.processed_events) == 1
        _, events = poller.processed_events[0]
        assert len(events) == 1
        assert events[0] is new_event

    @pytest.mark.asyncio
    async def test_cursor_persisted_on_cold_start(self) -> None:
        """Cold start: cursor is persisted even when all events are skipped."""
        store = MemoryCursorStore()
        poller = StubPoller(store)
        await poller.initialize()

        poller.fetch_results["repo-a"] = PollResult(
            events=[_event(minutes_ago=60)],
            cursor=CursorData(value="etag-primed"),
            has_new=True,
        )

        await poller.poll("repo-a")

        saved = await store.load("repo-a")
        assert saved is not None
        assert saved.value == "etag-primed"

    @pytest.mark.asyncio
    async def test_source_primed_after_cold_start(self) -> None:
        """Cold start: source is marked as primed after first poll."""
        poller = StubPoller(MemoryCursorStore())
        await poller.initialize()

        poller.fetch_results["repo-a"] = PollResult(
            events=[_event(minutes_ago=60)],
            cursor=CursorData(value="etag-1"),
            has_new=True,
        )

        assert "repo-a" not in poller.primed_sources
        await poller.poll("repo-a")
        assert "repo-a" in poller.primed_sources

    @pytest.mark.asyncio
    async def test_second_poll_is_warm_start(self) -> None:
        """After cold-start priming, second poll processes all events."""
        store = MemoryCursorStore()
        poller = StubPoller(store)
        await poller.initialize()

        # First poll: cold start, prime cursor
        poller.fetch_results["repo-a"] = PollResult(
            events=[_event(minutes_ago=60)],
            cursor=CursorData(value="etag-1"),
            has_new=True,
        )
        await poller.poll("repo-a")
        assert len(poller.processed_events) == 0  # historical, skipped

        # Second poll: warm start, process everything
        old_event = _event(minutes_ago=30)
        poller.fetch_results["repo-a"] = PollResult(
            events=[old_event],
            cursor=CursorData(value="etag-2"),
            has_new=True,
        )
        await poller.poll("repo-a")

        assert len(poller.processed_events) == 1
        _, events = poller.processed_events[0]
        assert len(events) == 1
        assert events[0] is old_event

    @pytest.mark.asyncio
    async def test_cold_start_with_no_events(self) -> None:
        """Cold start with has_new=False: nothing happens, source not primed."""
        poller = StubPoller(MemoryCursorStore())
        await poller.initialize()

        poller.fetch_results["repo-a"] = PollResult(
            events=[],
            cursor=CursorData(value="etag-1"),
            has_new=False,
        )

        await poller.poll("repo-a")

        assert len(poller.processed_events) == 0
        assert "repo-a" not in poller.primed_sources


# ============================================================================
# Multiple Sources Tests
# ============================================================================


class TestMultipleSources:
    """Each source has independent cold-start state."""

    @pytest.mark.asyncio
    async def test_independent_cold_start_per_source(self) -> None:
        """Cold start is tracked per source, not globally."""
        store = MemoryCursorStore({"repo-a": CursorData(value="etag-a")})
        poller = StubPoller(store)
        await poller.initialize()

        # repo-a: warm start (has cursor)
        old_event_a = _event(minutes_ago=60)
        poller.fetch_results["repo-a"] = PollResult(
            events=[old_event_a],
            cursor=CursorData(value="etag-a2"),
            has_new=True,
        )

        # repo-b: cold start (no cursor)
        old_event_b = _event(minutes_ago=60)
        poller.fetch_results["repo-b"] = PollResult(
            events=[old_event_b],
            cursor=CursorData(value="etag-b1"),
            has_new=True,
        )

        await poller.poll("repo-a")
        await poller.poll("repo-b")

        # repo-a processed (warm), repo-b skipped (cold, historical)
        assert len(poller.processed_events) == 1
        source, events = poller.processed_events[0]
        assert source == "repo-a"
        assert events[0] is old_event_a


# ============================================================================
# Edge Cases
# ============================================================================


class TestEdgeCases:
    """Edge cases for the cold-start fence."""

    @pytest.mark.asyncio
    async def test_event_exactly_at_startup_time_is_processed(self) -> None:
        """Events with created_at == started_at are processed (>= comparison)."""
        poller = StubPoller(MemoryCursorStore())
        await poller.initialize()

        exact_event = _event_at(poller.started_at)
        poller.fetch_results["repo-a"] = PollResult(
            events=[exact_event],
            cursor=CursorData(value="etag-1"),
            has_new=True,
        )

        await poller.poll("repo-a")

        assert len(poller.processed_events) == 1
        _, events = poller.processed_events[0]
        assert events[0] is exact_event

    @pytest.mark.asyncio
    async def test_cursor_metadata_preserved(self) -> None:
        """CursorData metadata is preserved through save/load cycle."""
        store = MemoryCursorStore()
        poller = StubPoller(store)
        await poller.initialize()

        cursor = CursorData(value="etag-1", metadata={"last_event_id": "12345"})
        poller.fetch_results["repo-a"] = PollResult(
            events=[_event(minutes_ago=60)],
            cursor=cursor,
            has_new=True,
        )

        await poller.poll("repo-a")

        saved = await store.load("repo-a")
        assert saved is not None
        assert saved.metadata == {"last_event_id": "12345"}

    @pytest.mark.asyncio
    async def test_warm_start_empty_events_list(self) -> None:
        """Warm start with has_new=True but empty events list: no crash."""
        store = MemoryCursorStore({"repo-a": CursorData(value="etag-1")})
        poller = StubPoller(store)
        await poller.initialize()

        poller.fetch_results["repo-a"] = PollResult(
            events=[],
            cursor=CursorData(value="etag-2"),
            has_new=True,
        )

        await poller.poll("repo-a")

        # No events to process, but cursor should be updated
        assert len(poller.processed_events) == 0
        saved = await store.load("repo-a")
        assert saved is not None
        assert saved.value == "etag-2"
