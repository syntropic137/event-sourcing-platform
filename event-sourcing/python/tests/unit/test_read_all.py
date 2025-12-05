"""Tests for read_all functionality in event store clients."""

import os

import pytest

# Ensure test environment for MemoryEventStoreClient
os.environ["APP_ENVIRONMENT"] = "test"

from event_sourcing.client.memory import MemoryEventStoreClient
from event_sourcing.core.event import DomainEvent, EventEnvelope, EventMetadata


class TestEvent(DomainEvent):
    """Test event for unit tests."""

    event_type: str = "TestEvent"
    value: str = ""


def make_envelope(aggregate_id: str, nonce: int, value: str) -> EventEnvelope[TestEvent]:
    """Create a test event envelope."""
    return EventEnvelope(
        event=TestEvent(value=value),
        metadata=EventMetadata(
            event_id=f"evt-{aggregate_id}-{nonce}",
            aggregate_id=aggregate_id,
            aggregate_type="TestAggregate",
            aggregate_nonce=nonce,
        ),
    )


@pytest.fixture
def memory_client() -> MemoryEventStoreClient:
    """Create a fresh memory client for each test."""
    return MemoryEventStoreClient()


class TestReadAll:
    """Tests for read_all method."""

    @pytest.mark.asyncio
    async def test_read_all_returns_events_in_global_order(
        self, memory_client: MemoryEventStoreClient
    ) -> None:
        """Test that read_all returns events ordered by global nonce."""
        # Append to two different streams
        await memory_client.append_events(
            "TestAggregate-agg1",
            [make_envelope("agg1", 1, "first")],
            expected_version=0,
        )
        await memory_client.append_events(
            "TestAggregate-agg2",
            [make_envelope("agg2", 1, "second")],
            expected_version=0,
        )
        await memory_client.append_events(
            "TestAggregate-agg1",
            [make_envelope("agg1", 2, "third")],
            expected_version=1,
        )

        # Read all events
        events, is_end, next_pos = await memory_client.read_all(
            from_global_nonce=0, max_count=10, forward=True
        )

        assert len(events) == 3
        assert is_end is True

        # Verify global ordering
        global_nonces = [e.metadata.global_nonce for e in events]
        assert global_nonces == [0, 1, 2]

    @pytest.mark.asyncio
    async def test_read_all_pagination(
        self, memory_client: MemoryEventStoreClient
    ) -> None:
        """Test that read_all supports pagination."""
        # Append 5 events
        for i in range(5):
            await memory_client.append_events(
                f"TestAggregate-agg{i}",
                [make_envelope(f"agg{i}", 1, f"event-{i}")],
                expected_version=0,
            )

        # Read first page (2 events)
        page1, is_end1, next_pos1 = await memory_client.read_all(
            from_global_nonce=0, max_count=2, forward=True
        )
        assert len(page1) == 2
        assert is_end1 is False

        # Read second page
        page2, is_end2, next_pos2 = await memory_client.read_all(
            from_global_nonce=next_pos1, max_count=2, forward=True
        )
        assert len(page2) == 2
        assert is_end2 is False

        # Read final page
        page3, is_end3, _ = await memory_client.read_all(
            from_global_nonce=next_pos2, max_count=2, forward=True
        )
        assert len(page3) == 1
        assert is_end3 is True

    @pytest.mark.asyncio
    async def test_read_all_empty_store(
        self, memory_client: MemoryEventStoreClient
    ) -> None:
        """Test that read_all returns is_end=True for empty store."""
        events, is_end, _ = await memory_client.read_all(
            from_global_nonce=0, max_count=10, forward=True
        )

        assert len(events) == 0
        assert is_end is True

    @pytest.mark.asyncio
    async def test_read_all_events_from_deprecated_wrapper(
        self, memory_client: MemoryEventStoreClient
    ) -> None:
        """Test that read_all_events_from still works as a wrapper."""
        # Append some events
        await memory_client.append_events(
            "TestAggregate-agg1",
            [make_envelope("agg1", 1, "first")],
            expected_version=0,
        )
        await memory_client.append_events(
            "TestAggregate-agg1",
            [make_envelope("agg1", 2, "second")],
            expected_version=1,
        )

        # Use deprecated method (exclusive start)
        events = await memory_client.read_all_events_from(
            after_global_nonce=0, limit=10
        )

        assert len(events) == 1  # Only events after 0, so starting from 1


class TestMemoryClientEnvironmentGuard:
    """Tests for the test environment guard."""

    def test_raises_error_in_non_test_environment(self) -> None:
        """Test that MemoryEventStoreClient raises error outside test env."""
        original_env = os.environ.get("APP_ENVIRONMENT")
        try:
            os.environ["APP_ENVIRONMENT"] = "development"
            # Import fresh to trigger the guard
            from importlib import reload

            from event_sourcing.client import memory

            reload(memory)

            with pytest.raises(RuntimeError, match="can only be used in test environment"):
                memory.MemoryEventStoreClient()
        finally:
            if original_env:
                os.environ["APP_ENVIRONMENT"] = original_env
            else:
                os.environ["APP_ENVIRONMENT"] = "test"
            # Reload to restore normal behavior
            from importlib import reload

            from event_sourcing.client import memory

            reload(memory)
