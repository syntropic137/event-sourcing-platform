"""Tests for aggregate module."""

import pytest

from event_sourcing.core.aggregate import AggregateRoot
from event_sourcing.core.errors import InvalidAggregateStateError
from event_sourcing.core.event import DomainEvent
from event_sourcing.decorators import event_sourcing_handler


# Test events
class TestEvent(DomainEvent):
    """Test event."""

    event_type = "TestEvent"
    value: str


class AnotherEvent(DomainEvent):
    """Another test event."""

    event_type = "AnotherEvent"
    count: int


# Test aggregate
class TestAggregate(AggregateRoot[TestEvent | AnotherEvent]):
    """Test aggregate for unit tests."""

    def __init__(self) -> None:
        super().__init__()
        self.value: str = ""
        self.count: int = 0

    def get_aggregate_type(self) -> str:
        return "TestAggregate"

    @event_sourcing_handler("TestEvent")
    def on_test_event(self, event: TestEvent) -> None:
        self.value = event.value

    @event_sourcing_handler("AnotherEvent")
    def on_another_event(self, event: AnotherEvent) -> None:
        self.count = event.count


class TestBaseAggregate:
    """Tests for BaseAggregate."""

    def test_initialization(self):
        """Should initialize with default values."""
        agg = TestAggregate()

        assert agg.id is None
        assert agg.version == 0
        assert not agg.has_uncommitted_events()

    def test_initialize_aggregate(self):
        """Should set aggregate ID."""
        agg = TestAggregate()
        agg._initialize("test-123")

        assert agg.id == "test-123"
        assert agg.version == 0

    def test_cannot_change_aggregate_id(self):
        """Should not allow changing aggregate ID."""
        agg = TestAggregate()
        agg._initialize("test-123")

        with pytest.raises(InvalidAggregateStateError) as exc:
            agg._initialize("test-456")

        assert "Cannot change aggregate ID" in str(exc.value)

    def test_raise_event_without_id_fails(self):
        """Should fail to raise event without aggregate ID."""
        agg = TestAggregate()

        with pytest.raises(InvalidAggregateStateError) as exc:
            agg._raise_event(TestEvent(value="test"))

        assert "without an ID" in str(exc.value)

    def test_raise_event(self):
        """Should raise and apply event."""
        agg = TestAggregate()
        agg._initialize("test-123")

        agg._raise_event(TestEvent(value="hello"))

        assert agg.value == "hello"
        assert agg.version == 1
        assert agg.has_uncommitted_events()
        assert len(agg.get_uncommitted_events()) == 1

    def test_multiple_events(self):
        """Should handle multiple events."""
        agg = TestAggregate()
        agg._initialize("test-123")

        agg._raise_event(TestEvent(value="first"))
        agg._raise_event(AnotherEvent(count=5))
        agg._raise_event(TestEvent(value="second"))

        assert agg.value == "second"
        assert agg.count == 5
        assert agg.version == 3
        assert len(agg.get_uncommitted_events()) == 3

    def test_mark_events_as_committed(self):
        """Should clear uncommitted events."""
        agg = TestAggregate()
        agg._initialize("test-123")
        agg._raise_event(TestEvent(value="test"))

        assert agg.has_uncommitted_events()

        agg.mark_events_as_committed()

        assert not agg.has_uncommitted_events()
        assert len(agg.get_uncommitted_events()) == 0
        assert agg.version == 1  # Version should remain

    def test_rehydrate_from_events(self):
        """Should rehydrate aggregate from event history."""
        # Create aggregate and generate events
        agg1 = TestAggregate()
        agg1._initialize("test-123")
        agg1._raise_event(TestEvent(value="first"))
        agg1._raise_event(AnotherEvent(count=10))
        agg1._raise_event(TestEvent(value="last"))

        events = agg1.get_uncommitted_events()

        # Create new aggregate and rehydrate
        agg2 = TestAggregate()
        agg2.rehydrate(events)

        assert agg2.id == "test-123"
        assert agg2.version == 3
        assert agg2.value == "last"
        assert agg2.count == 10
        assert not agg2.has_uncommitted_events()

    def test_rehydrate_resets_state(self):
        """Rehydration should reset aggregate state."""
        agg = TestAggregate()
        agg._initialize("test-123")
        agg._raise_event(TestEvent(value="original"))

        # Get events
        events = agg.get_uncommitted_events()

        # Rehydrate (should reset)
        agg.rehydrate(events)

        assert agg.id == "test-123"
        assert agg.version == 1
        assert not agg.has_uncommitted_events()


class TestAggregateRoot:
    """Tests for AggregateRoot automatic dispatching."""

    def test_aggregate_id_property(self):
        """Should provide aggregate_id property."""
        agg = TestAggregate()
        agg._initialize("test-123")

        assert agg.aggregate_id == "test-123"
        assert agg.aggregate_id == agg.id

    def test_event_handler_dispatch(self):
        """Should automatically dispatch to decorated handlers."""
        agg = TestAggregate()
        agg._initialize("test-123")

        # Raise event through _raise_event
        agg._raise_event(TestEvent(value="dispatched"))

        assert agg.value == "dispatched"

    def test_apply_convenience_method(self):
        """Should provide _apply() convenience method."""
        agg = TestAggregate()
        agg._initialize("test-123")

        agg._apply(TestEvent(value="applied"))

        assert agg.value == "applied"
        assert agg.version == 1

    def test_get_event_handlers(self):
        """Should discover decorated event handlers."""
        handlers = TestAggregate._get_event_handlers()

        assert "TestEvent" in handlers
        assert "AnotherEvent" in handlers
        assert len(handlers) == 2

    def test_handle_unknown_event(self, caplog):
        """Should log warning for unknown events."""

        class UnknownEvent(DomainEvent):
            event_type = "UnknownEvent"
            data: str

        agg = TestAggregate()
        agg._initialize("test-123")

        # Apply unknown event directly
        agg.apply_event(UnknownEvent(data="test"))  # type: ignore

        # Should log warning
        assert "No handler found" in caplog.text


class TestEventHandlerDecorator:
    """Tests for @event_sourcing_handler decorator."""

    def test_decorator_attaches_metadata(self):
        """Decorator should attach event type metadata."""

        class DecoratedAggregate(AggregateRoot[TestEvent]):
            def get_aggregate_type(self) -> str:
                return "Decorated"

            @event_sourcing_handler("MyEvent")
            def my_handler(self, event: TestEvent) -> None:
                pass

        # Check method has metadata
        assert hasattr(DecoratedAggregate.my_handler, "_event_type")
        assert DecoratedAggregate.my_handler._event_type == "MyEvent"  # type: ignore

    def test_multiple_handlers_per_aggregate(self):
        """Should support multiple event handlers."""

        class MultiHandlerAggregate(AggregateRoot[TestEvent | AnotherEvent]):
            def get_aggregate_type(self) -> str:
                return "Multi"

            @event_sourcing_handler("Event1")
            def handler1(self, event: TestEvent) -> None:
                pass

            @event_sourcing_handler("Event2")
            def handler2(self, event: AnotherEvent) -> None:
                pass

        handlers = MultiHandlerAggregate._get_event_handlers()
        assert len(handlers) == 2
        assert "Event1" in handlers
        assert "Event2" in handlers

