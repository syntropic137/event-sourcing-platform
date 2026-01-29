"""
ResultValidator - Validation phase (Then) of Given-When-Then testing

Provides fluent assertions for verifying:
- Events emitted by command
- Exceptions thrown
- Aggregate state after command
"""

from __future__ import annotations

import re
from collections.abc import Callable
from typing import TYPE_CHECKING, Generic, TypeVar

from event_sourcing.core.event import DomainEvent
from event_sourcing.testing.scenario.errors import ScenarioAssertionError

if TYPE_CHECKING:
    from event_sourcing.core.aggregate import AggregateRoot

TAggregate = TypeVar("TAggregate", bound="AggregateRoot[Any]")


class ResultValidator(Generic[TAggregate]):
    """
    ResultValidator - Validates command execution results.

    Provides a fluent API for asserting on the results of command execution
    in Given-When-Then style tests.
    """

    def __init__(
        self, aggregate: TAggregate, error: Exception | None = None
    ) -> None:
        self._aggregate = aggregate
        self._error = error

    def expect_successful_handler_execution(self) -> ResultValidator[TAggregate]:
        """
        Assert command executed successfully (no exception thrown).

        Returns:
            self for chaining
        """
        if self._error:
            raise ScenarioAssertionError(
                f"Expected successful execution but got error: "
                f"{type(self._error).__name__}: {self._error}"
            )

        return self

    def expect_events(
        self, expected_events: list[DomainEvent]
    ) -> ResultValidator[TAggregate]:
        """
        Assert specific events were emitted by the command.

        Args:
            expected_events: Array of expected events (order matters)

        Returns:
            self for chaining
        """
        self.expect_successful_handler_execution()

        actual_envelopes = self._aggregate.get_uncommitted_events()
        actual_events = [e.event for e in actual_envelopes]

        if len(actual_events) != len(expected_events):
            expected_types = [self._get_event_type(e) for e in expected_events]
            actual_types = [self._get_event_type(e) for e in actual_events]
            raise ScenarioAssertionError(
                f"Expected {len(expected_events)} event(s) but got {len(actual_events)}.\n"
                f"Expected types: [{', '.join(expected_types)}]\n"
                f"Actual types: [{', '.join(actual_types)}]"
            )

        for i, (actual, expected) in enumerate(
            zip(actual_events, expected_events, strict=True)
        ):
            if not self._events_match(actual, expected):
                raise ScenarioAssertionError(
                    f"Event at index {i} does not match.\n"
                    f"Expected: {self._format_event(expected)}\n"
                    f"Actual: {self._format_event(actual)}"
                )

        return self

    def expect_no_events(self) -> ResultValidator[TAggregate]:
        """
        Assert no events were emitted by the command.

        Returns:
            self for chaining
        """
        self.expect_successful_handler_execution()

        events = self._aggregate.get_uncommitted_events()
        if events:
            event_types = [self._get_event_type(e.event) for e in events]
            raise ScenarioAssertionError(
                f"Expected no events but got {len(events)}: [{', '.join(event_types)}]"
            )

        return self

    def expect_exception(
        self, error_type: type[Exception]
    ) -> ResultValidator[TAggregate]:
        """
        Assert a specific exception type was thrown.

        Args:
            error_type: The expected error class

        Returns:
            self for chaining
        """
        self._validated = True

        if self._error is None:
            raise ScenarioAssertionError(
                f"Expected exception {error_type.__name__} but command succeeded"
            )

        if not isinstance(self._error, error_type):
            raise ScenarioAssertionError(
                f"Expected exception {error_type.__name__} but got "
                f"{type(self._error).__name__}: {self._error}"
            )

        return self

    def expect_exception_message(
        self, message: str | re.Pattern[str]
    ) -> ResultValidator[TAggregate]:
        """
        Assert exception message matches (string contains or regex matches).

        Args:
            message: String to check for containment, or compiled regex to test

        Returns:
            self for chaining
        """
        if self._error is None:
            raise ScenarioAssertionError(
                "Expected exception with message but command succeeded"
            )

        error_message = str(self._error)

        if isinstance(message, re.Pattern):
            matches = message.search(error_message) is not None
            expected_desc = f"to match {message.pattern}"
        else:
            matches = message in error_message
            expected_desc = f'to contain "{message}"'

        if not matches:
            raise ScenarioAssertionError(
                f'Expected exception message {expected_desc} but got "{error_message}"'
            )

        return self

    def expect_state(
        self, assertion: Callable[[TAggregate], None]
    ) -> ResultValidator[TAggregate]:
        """
        Assert aggregate state using a callback function.

        Args:
            assertion: Callback that receives the aggregate and can make assertions

        Returns:
            self for chaining
        """
        self.expect_successful_handler_execution()
        assertion(self._aggregate)
        return self

    def get_aggregate(self) -> TAggregate:
        """Get the aggregate after command execution (for custom assertions)."""
        return self._aggregate

    def get_error(self) -> Exception | None:
        """Get the error that was thrown (if any)."""
        return self._error

    def has_error(self) -> bool:
        """Check if execution resulted in an error."""
        return self._error is not None

    def _events_match(self, actual: DomainEvent, expected: DomainEvent) -> bool:
        """Compare two events for equality."""
        # Compare event types first
        actual_type = self._get_event_type(actual)
        expected_type = self._get_event_type(expected)

        if actual_type != expected_type:
            return False

        # Compare schema versions if available
        actual_version = getattr(actual, "schema_version", 1)
        expected_version = getattr(expected, "schema_version", 1)

        if actual_version != expected_version:
            return False

        # Deep equality on model data
        actual_dict = actual.model_dump() if hasattr(actual, "model_dump") else vars(actual)
        expected_dict = (
            expected.model_dump() if hasattr(expected, "model_dump") else vars(expected)
        )

        return actual_dict == expected_dict

    def _get_event_type(self, event: DomainEvent) -> str:
        """Get the event type from an event."""
        if hasattr(event, "event_type"):
            event_type = event.event_type
            if isinstance(event_type, str):
                return event_type
        return type(event).__name__

    def _format_event(self, event: DomainEvent) -> str:
        """Format an event for error messages."""
        event_type = self._get_event_type(event)
        version = getattr(event, "schema_version", 1)
        data = event.model_dump() if hasattr(event, "model_dump") else vars(event)
        return f"{event_type}(v{version}): {data}"
