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
        self.process_replay_flags: list[bool] = []
        self.fetch_calls: list[str] = []
        self.fetch_error: Exception | None = None
        self.process_error: Exception | None = None

    async def fetch(self, source_key: str) -> PollResult:
        self.fetch_calls.append(source_key)
        if self.fetch_error is not None:
            raise self.fetch_error
        return self.fetch_results[source_key]

    async def process(
        self,
        source_key: str,
        events: list[PollEvent],
        is_replay: bool = False,
    ) -> None:
        self.processed_events.append((source_key, list(events)))
        self.process_replay_flags.append(is_replay)
        if self.process_error is not None:
            raise self.process_error


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
# is_replay Propagation Tests (ADR-060 Layer 4)
# ============================================================================


class TestIsReplayPropagation:
    """Verify ESP signals cold-start replay vs warm-start to subclasses.

    See ADR-060 Section 9 Layer 4: the original ``source_primed=False``
    safety net was dead code because ``_prime()`` runs before ``process()``.
    The fix passes ``is_replay`` directly from ``poll()`` to ``process()``.
    """

    @pytest.mark.asyncio
    async def test_cold_start_passes_is_replay_true(self) -> None:
        """Cold-start path must call process(..., is_replay=True)."""
        poller = StubPoller(MemoryCursorStore())
        await poller.initialize()

        # Event after startup so it survives the timestamp fence
        new_event = _event(minutes_ago=-1)
        poller.fetch_results["repo-a"] = PollResult(
            events=[new_event],
            cursor=CursorData(value="etag-1"),
            has_new=True,
        )

        await poller.poll("repo-a")

        assert poller.process_replay_flags == [True]

    @pytest.mark.asyncio
    async def test_warm_start_passes_is_replay_false(self) -> None:
        """Warm-start path must call process(..., is_replay=False)."""
        store = MemoryCursorStore({"repo-a": CursorData(value="etag-old")})
        poller = StubPoller(store)
        await poller.initialize()

        poller.fetch_results["repo-a"] = PollResult(
            events=[_event(minutes_ago=0)],
            cursor=CursorData(value="etag-new"),
            has_new=True,
        )

        await poller.poll("repo-a")

        assert poller.process_replay_flags == [False]

    @pytest.mark.asyncio
    async def test_subclass_without_is_replay_kwarg_still_works(self) -> None:
        """Liskov: existing subclasses with old (no-kwarg) signature still work.

        The default value ``is_replay=False`` means subclasses that don't
        accept the kwarg won't break -- ESP only passes ``is_replay=True``
        as a kwarg, never positionally.
        """

        class LegacyPoller(HistoricalPoller):
            def __init__(self, cursor_store: CursorStore) -> None:
                super().__init__(cursor_store)
                self.processed: list[list[PollEvent]] = []

            async def fetch(self, source_key: str) -> PollResult:
                return PollResult(
                    events=[_event(minutes_ago=0)],
                    cursor=CursorData(value="etag-1"),
                    has_new=True,
                )

            async def process(  # type: ignore[override]  # legacy signature without is_replay
                self,
                source_key: str,
                events: list[PollEvent],
            ) -> None:
                self.processed.append(list(events))

        store = MemoryCursorStore({"repo-a": CursorData(value="etag-old")})
        poller = LegacyPoller(store)
        await poller.initialize()
        await poller.poll("repo-a")

        # Warm path passes no kwarg, so legacy subclasses keep working.
        assert len(poller.processed) == 1


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

    def test_naive_datetime_rejected(self) -> None:
        """PollEvent rejects naive (timezone-unaware) datetimes."""
        from datetime import datetime as dt

        with pytest.raises(ValueError, match="timezone-aware"):
            PollEvent(created_at=dt(2026, 1, 1, 12, 0, 0), data={})  # noqa: DTZ001

    def test_aware_datetime_accepted(self) -> None:
        """PollEvent accepts timezone-aware datetimes."""
        event = PollEvent(created_at=datetime.now(UTC), data={})
        assert event.created_at.tzinfo is not None


# ============================================================================
# Exception Safety Tests
# ============================================================================


class TestExceptionSafety:
    """Verify poller state remains consistent when fetch/process raise."""

    @pytest.mark.asyncio
    async def test_fetch_error_does_not_corrupt_state(self) -> None:
        """If fetch() raises, poller state is unchanged."""
        poller = StubPoller(MemoryCursorStore())
        await poller.initialize()

        poller.fetch_error = RuntimeError("network timeout")

        with pytest.raises(RuntimeError, match="network timeout"):
            await poller.poll("repo-a")

        # Source should NOT be primed after a failed fetch
        assert "repo-a" not in poller.primed_sources
        assert len(poller.processed_events) == 0

    @pytest.mark.asyncio
    async def test_process_error_on_cold_start_still_primes(self) -> None:
        """If process() raises on cold start, cursor was already persisted."""
        store = MemoryCursorStore()
        poller = StubPoller(store)
        await poller.initialize()

        new_event = _event(minutes_ago=-1)
        poller.fetch_results["repo-a"] = PollResult(
            events=[new_event],
            cursor=CursorData(value="etag-1"),
            has_new=True,
        )
        poller.process_error = RuntimeError("pipeline down")

        with pytest.raises(RuntimeError, match="pipeline down"):
            await poller.poll("repo-a")

        # Cursor should still be primed (prime happens before process)
        assert "repo-a" in poller.primed_sources
        saved = await store.load("repo-a")
        assert saved is not None
        assert saved.value == "etag-1"

    @pytest.mark.asyncio
    async def test_process_error_on_warm_start_cursor_not_updated(self) -> None:
        """If process() raises on warm start, cursor was NOT yet updated."""
        store = MemoryCursorStore({"repo-a": CursorData(value="etag-old")})
        poller = StubPoller(store)
        await poller.initialize()

        poller.fetch_results["repo-a"] = PollResult(
            events=[_event(minutes_ago=0)],
            cursor=CursorData(value="etag-new"),
            has_new=True,
        )
        poller.process_error = RuntimeError("pipeline down")

        with pytest.raises(RuntimeError, match="pipeline down"):
            await poller.poll("repo-a")

        # Cursor should NOT have been updated (process failed before persist)
        saved = await store.load("repo-a")
        assert saved is not None
        assert saved.value == "etag-old"

    @pytest.mark.asyncio
    async def test_poll_without_initialize_raises(self) -> None:
        """Calling poll() before initialize() raises RuntimeError."""
        store = MemoryCursorStore({"repo-a": CursorData(value="etag-1")})
        poller = StubPoller(store)
        # Deliberately skip initialize()

        poller.fetch_results["repo-a"] = PollResult(
            events=[_event(minutes_ago=60)],
            cursor=CursorData(value="etag-2"),
            has_new=True,
        )

        with pytest.raises(RuntimeError, match="before initialize"):
            await poller.poll("repo-a")


# ============================================================================
# Fitness Function Tests
# ============================================================================


class TestHistoricalPollerFitness:
    """Tests for the HistoricalPoller architectural fitness check."""

    def test_valid_subclass_passes(self) -> None:
        """A correctly implemented subclass should have no violations."""
        from event_sourcing.fitness.historical_poller_check import (
            check_historical_poller_structure,
        )

        violations = check_historical_poller_structure(StubPoller)
        assert violations == []

    def test_overriding_poll_is_violation(self) -> None:
        """Overriding poll() defeats the cold-start fence - must be flagged."""
        from event_sourcing.fitness.historical_poller_check import (
            check_historical_poller_structure,
        )

        class BadPoller(HistoricalPoller):
            async def fetch(self, source_key: str) -> PollResult:
                return PollResult(events=[], cursor=CursorData(value=""), has_new=False)

            async def process(
                self,
                source_key: str,
                events: list[PollEvent],
                is_replay: bool = False,
            ) -> None:
                pass

            async def poll(self, source_key: str) -> None:  # type: ignore[override]  # intentional for test
                await super().poll(source_key)

        violations = check_historical_poller_structure(BadPoller)
        assert len(violations) == 1
        assert "poll" in violations[0].message
        assert violations[0].rule == "historical-poller-fence"

    def test_overriding_prime_is_violation(self) -> None:
        """Overriding _prime() bypasses cursor management - must be flagged."""
        from event_sourcing.fitness.historical_poller_check import (
            check_historical_poller_structure,
        )

        class SneakyPoller(HistoricalPoller):
            async def fetch(self, source_key: str) -> PollResult:
                return PollResult(events=[], cursor=CursorData(value=""), has_new=False)

            async def process(
                self,
                source_key: str,
                events: list[PollEvent],
                is_replay: bool = False,
            ) -> None:
                pass

            async def _prime(self, source_key: str, cursor: CursorData) -> None:  # type: ignore[override]  # intentional for test
                pass  # Silently swallows the prime

        violations = check_historical_poller_structure(SneakyPoller)
        assert len(violations) == 1
        assert "_prime" in violations[0].message

    def test_multiple_violations_reported(self) -> None:
        """Multiple violations should all be reported."""
        from event_sourcing.fitness.historical_poller_check import (
            check_historical_poller_structure,
        )

        class TerriblePoller(HistoricalPoller):
            async def fetch(self, source_key: str) -> PollResult:
                return PollResult(events=[], cursor=CursorData(value=""), has_new=False)

            async def process(
                self,
                source_key: str,
                events: list[PollEvent],
                is_replay: bool = False,
            ) -> None:
                pass

            async def poll(self, source_key: str) -> None:  # type: ignore[override]  # intentional for test
                pass

            async def _prime(self, source_key: str, cursor: CursorData) -> None:  # type: ignore[override]  # intentional for test
                pass

            async def _persist_cursor(self, source_key: str, cursor: CursorData) -> None:  # type: ignore[override]  # intentional for test
                pass

        violations = check_historical_poller_structure(TerriblePoller)
        assert len(violations) == 3
        violation_methods = {
            v.message.split("overrides ")[1].split(".")[0] if "overrides" in v.message else ""
            for v in violations
        }
        assert "poll()" in violation_methods
