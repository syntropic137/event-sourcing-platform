"""
TestExecutor - Execution phase (When) of Given-When-Then testing

Handles:
- Aggregate rehydration from given events
- Command execution
- Error capture
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Generic, TypeVar

from event_sourcing.core.event import DomainEvent, EventEnvelope, EventFactory
from event_sourcing.testing.scenario.result_validator import ResultValidator

if TYPE_CHECKING:
    from event_sourcing.core.aggregate import AggregateRoot
    from event_sourcing.core.command import Command

TAggregate = TypeVar("TAggregate", bound="AggregateRoot[DomainEvent]")


class TestExecutor(Generic[TAggregate]):
    """
    TestExecutor - Executes commands against a pre-configured aggregate.

    Handles the "When" phase of Given-When-Then testing.
    """

    def __init__(
        self,
        aggregate_class: type[TAggregate],
        given_events: list[DomainEvent],
        injectables: dict[str, object],  # OBJRATCHET: DI container holds arbitrary service types
    ) -> None:
        self._aggregate_class = aggregate_class
        self._given_events = given_events
        self._injectables = injectables

    def when(self, command: Command) -> ResultValidator[TAggregate]:
        """
        Execute a command against the aggregate.

        Args:
            command: The command to execute

        Returns:
            ResultValidator for making assertions
        """
        aggregate = self._aggregate_class()

        # Inject dependencies if the aggregate supports it
        self._inject_dependencies(aggregate)

        # Rehydrate from given events
        if self._given_events:
            envelopes = [
                self._create_envelope(event, aggregate, index + 1)
                for index, event in enumerate(self._given_events)
            ]
            aggregate.rehydrate(envelopes)
            aggregate.mark_events_as_committed()

        # Execute command and capture result
        error: Exception | None = None
        try:
            self._execute_command(aggregate, command)
        except Exception as e:
            error = e

        return ResultValidator(aggregate, error)

    def _create_envelope(
        self, event: DomainEvent, aggregate: TAggregate, nonce: int
    ) -> EventEnvelope[DomainEvent]:
        """Create an event envelope with test metadata."""
        aggregate_id = self._extract_aggregate_id(event) or "test-aggregate"
        return EventFactory.create(
            event=event,
            aggregate_id=aggregate_id,
            aggregate_type=aggregate.get_aggregate_type(),
            aggregate_nonce=nonce,
        )

    def _extract_aggregate_id(self, event: DomainEvent) -> str | None:
        """Try to extract aggregate ID from event if it has one."""
        # Check common property names for aggregate ID
        for attr_name in [
            "aggregate_id",
            "id",
            "order_id",
            "cart_id",
            "account_id",
            "customer_id",
        ]:
            if hasattr(event, attr_name):
                value = getattr(event, attr_name)
                if isinstance(value, str):
                    return value
        return None

    def _execute_command(self, aggregate: TAggregate, command: Command) -> None:
        """Execute command using the aggregate's _handle_command method."""
        handle_cmd = getattr(aggregate, "_handle_command", None)
        if handle_cmd is not None and callable(handle_cmd):
            handle_cmd(command)
        else:
            raise RuntimeError(
                f"Aggregate {aggregate.get_aggregate_type()} does not have a "
                "_handle_command method. Make sure it extends AggregateRoot "
                "and has @command_handler decorated methods."
            )

    def _inject_dependencies(self, aggregate: TAggregate) -> None:
        """Inject dependencies into aggregate if it supports dependency injection."""
        if not self._injectables:
            return

        # Check if aggregate has a set_dependencies method
        set_deps = getattr(aggregate, "set_dependencies", None)
        if set_deps is not None and callable(set_deps):
            set_deps(self._injectables)

        # Also try to inject via property names matching injectable type names
        for type_name, resource in self._injectables.items():
            property_name = self._to_snake_case(type_name)
            if hasattr(aggregate, property_name):
                setattr(aggregate, property_name, resource)

    def _to_snake_case(self, pascal_case: str) -> str:
        """Convert PascalCase type name to snake_case property name."""
        import re

        # Insert underscore before uppercase letters and convert to lowercase
        s1 = re.sub("(.)([A-Z][a-z]+)", r"\1_\2", pascal_case)
        return re.sub("([a-z0-9])([A-Z])", r"\1_\2", s1).lower()
