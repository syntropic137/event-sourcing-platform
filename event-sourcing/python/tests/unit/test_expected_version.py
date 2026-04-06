"""Tests for ExpectedVersion semantics."""

import pytest

from event_sourcing.core.event import DomainEvent, EventEnvelope, EventMetadata
from event_sourcing.core.expected_version import ExpectedVersion


class _TestEvent(DomainEvent):
    """Simple event for memory client tests."""

    event_type = "TestEvent"
    value: str = "test"


def _make_envelope(
    aggregate_id: str = "abc",
    aggregate_type: str = "Test",
    nonce: int = 1,
) -> EventEnvelope[_TestEvent]:
    return EventEnvelope(
        event=_TestEvent(),
        metadata=EventMetadata(
            event_id=f"evt-{nonce}",
            aggregate_id=aggregate_id,
            aggregate_type=aggregate_type,
            aggregate_nonce=nonce,
            event_type="TestEvent",
        ),
    )


class TestExpectedVersion:
    """ExpectedVersion constants and factory."""

    def test_no_stream_is_zero(self):
        assert ExpectedVersion.NO_STREAM == 0

    def test_any_is_none(self):
        assert ExpectedVersion.ANY is None

    def test_exact_returns_version(self):
        assert ExpectedVersion.exact(1) == 1
        assert ExpectedVersion.exact(5) == 5
        assert ExpectedVersion.exact(100) == 100

    def test_exact_rejects_zero(self):
        with pytest.raises(ValueError, match="must be >= 1"):
            ExpectedVersion.exact(0)

    def test_exact_rejects_negative(self):
        with pytest.raises(ValueError, match="must be >= 1"):
            ExpectedVersion.exact(-1)


class TestStreamAlreadyExistsError:
    """StreamAlreadyExistsError is a ConcurrencyConflictError subclass."""

    def test_inherits_from_concurrency_conflict(self):
        from event_sourcing.core.errors import (
            ConcurrencyConflictError,
            StreamAlreadyExistsError,
        )

        err = StreamAlreadyExistsError("Test-abc", actual_version=3)
        assert isinstance(err, ConcurrencyConflictError)
        assert err.expected_version == 0
        assert err.actual_version == 3
        assert err.stream_name == "Test-abc"
        assert "already exists" in err.message

    def test_catchable_as_concurrency_conflict(self):
        from event_sourcing.core.errors import (
            ConcurrencyConflictError,
            StreamAlreadyExistsError,
        )

        with pytest.raises(ConcurrencyConflictError):
            raise StreamAlreadyExistsError("Test-abc", actual_version=1)


class TestMemoryClientNoStream:
    """MemoryEventStoreClient raises StreamAlreadyExistsError for NoStream violations."""

    @pytest.fixture(autouse=True)
    def _set_test_env(self, monkeypatch: pytest.MonkeyPatch):
        monkeypatch.setenv("APP_ENVIRONMENT", "test")

    @pytest.fixture
    def client(self):
        from event_sourcing.client.memory import MemoryEventStoreClient

        return MemoryEventStoreClient()

    @pytest.mark.asyncio
    async def test_no_stream_succeeds_on_new_stream(self, client):
        """expected_version=0 should succeed when the stream doesn't exist."""
        envelope = _make_envelope()
        await client.append_events(
            "Test-abc", [envelope], expected_version=ExpectedVersion.NO_STREAM
        )
        assert client.get_stream_version("Test-abc") == 1

    @pytest.mark.asyncio
    async def test_no_stream_raises_stream_already_exists(self, client):
        """expected_version=0 should raise StreamAlreadyExistsError on existing stream."""
        from event_sourcing.core.errors import StreamAlreadyExistsError

        envelope = _make_envelope()
        await client.append_events(
            "Test-abc", [envelope], expected_version=ExpectedVersion.NO_STREAM
        )

        with pytest.raises(StreamAlreadyExistsError) as exc_info:
            await client.append_events(
                "Test-abc", [_make_envelope(nonce=2)], expected_version=ExpectedVersion.NO_STREAM
            )
        assert exc_info.value.stream_name == "Test-abc"
        assert exc_info.value.actual_version == 1

    @pytest.mark.asyncio
    async def test_version_mismatch_raises_concurrency_conflict(self, client):
        """Non-zero expected_version mismatch raises ConcurrencyConflictError (not StreamAlreadyExistsError)."""
        from event_sourcing.core.errors import (
            ConcurrencyConflictError,
            StreamAlreadyExistsError,
        )

        envelope = _make_envelope()
        await client.append_events("Test-abc", [envelope], expected_version=0)

        with pytest.raises(ConcurrencyConflictError) as exc_info:
            await client.append_events("Test-abc", [_make_envelope(nonce=2)], expected_version=5)
        # Should NOT be StreamAlreadyExistsError
        assert not isinstance(exc_info.value, StreamAlreadyExistsError)


class TestRepositorySaveNew:
    """EventStoreRepository.save_new() uses NoStream semantics."""

    @pytest.fixture(autouse=True)
    def _set_test_env(self, monkeypatch: pytest.MonkeyPatch):
        monkeypatch.setenv("APP_ENVIRONMENT", "test")

    @pytest.fixture
    def memory_client(self):
        from event_sourcing.client.memory import MemoryEventStoreClient

        client = MemoryEventStoreClient()
        return client

    @pytest.fixture
    def repository(self, memory_client):
        from event_sourcing.core.repository import EventStoreRepository
        from tests.unit.test_aggregate import TestAggregate

        return EventStoreRepository(
            event_store_client=memory_client,
            aggregate_factory=TestAggregate,
            aggregate_type="Test",
        )

    @pytest.mark.asyncio
    async def test_save_new_succeeds_for_new_aggregate(self, repository):
        from tests.unit.test_aggregate import TestAggregate, TestEvent

        agg = TestAggregate()
        agg._initialize("unique-id-1")
        agg._raise_event(TestEvent(value="hello"))

        await repository.save_new(agg)
        assert not agg.has_uncommitted_events()

    @pytest.mark.asyncio
    async def test_save_new_raises_on_duplicate(self, repository):
        from event_sourcing.core.errors import StreamAlreadyExistsError
        from tests.unit.test_aggregate import TestAggregate, TestEvent

        agg1 = TestAggregate()
        agg1._initialize("unique-id-2")
        agg1._raise_event(TestEvent(value="first"))
        await repository.save_new(agg1)

        agg2 = TestAggregate()
        agg2._initialize("unique-id-2")
        agg2._raise_event(TestEvent(value="second"))

        with pytest.raises(StreamAlreadyExistsError):
            await repository.save_new(agg2)
