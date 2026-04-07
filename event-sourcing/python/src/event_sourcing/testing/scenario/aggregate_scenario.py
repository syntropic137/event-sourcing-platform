"""
AggregateScenario - Given-When-Then testing for event-sourced aggregates

Inspired by Axon Framework's AggregateTestFixture, this provides a fluent API
for testing aggregate command handlers in isolation.

Example:
    >>> scenario(OrderAggregate) \\
    ...     .given([
    ...         CartCreatedEvent(cart_id='order-1'),
    ...         ItemAddedEvent(cart_id='order-1', item_id='item-1', price=29.99),
    ...     ]) \\
    ...     .when(SubmitCartCommand(aggregate_id='order-1')) \\
    ...     .expect_events([
    ...         CartSubmittedEvent(cart_id='order-1', total=29.99),
    ...     ])
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Generic, TypeVar

if TYPE_CHECKING:
    from event_sourcing.core.aggregate import AggregateRoot
    from event_sourcing.core.command import Command
    from event_sourcing.core.event import DomainEvent

from event_sourcing.testing.scenario.test_executor import TestExecutor

TAggregate = TypeVar("TAggregate", bound="AggregateRoot[DomainEvent]")


class AggregateScenario(Generic[TAggregate]):
    """
    AggregateScenario - Entry point for Given-When-Then aggregate testing.

    Provides a fluent API for setting up test scenarios with prior events,
    executing commands, and verifying results.
    """

    def __init__(self, aggregate_class: type[TAggregate]) -> None:
        self._aggregate_class = aggregate_class
        self._injectables: dict[str, object] = {}  # OBJRATCHET: DI container holds arbitrary service types

    def register_injectable_resource(self, resource: object) -> AggregateScenario[TAggregate]:  # OBJRATCHET: DI accepts any service type
        """
        Register a resource that can be injected into command handlers.

        Args:
            resource: The resource to inject (matched by type name)

        Returns:
            self for chaining

        Example:
            >>> scenario(OrderAggregate) \\
            ...     .register_injectable_resource(mock_pricing_service) \\
            ...     .given([...]) \\
            ...     .when(AddItemCommand(...)) \\
            ...     .expect_events([...])
        """
        type_name = type(resource).__name__
        self._injectables[type_name] = resource
        return self

    def given_no_prior_activity(self) -> TestExecutor[TAggregate]:
        """
        Start given phase with no prior events (new aggregate).

        Returns:
            TestExecutor for the When phase

        Example:
            >>> scenario(OrderAggregate) \\
            ...     .given_no_prior_activity() \\
            ...     .when(CreateOrderCommand(aggregate_id='order-1')) \\
            ...     .expect_events([OrderCreatedEvent(order_id='order-1')])
        """
        return self.given([])

    def given(self, events: list[DomainEvent]) -> TestExecutor[TAggregate]:
        """
        Start given phase with prior events.

        Args:
            events: Array of events representing prior history

        Returns:
            TestExecutor for the When phase

        Example:
            >>> scenario(OrderAggregate) \\
            ...     .given([
            ...         CartCreatedEvent(cart_id='order-1'),
            ...         ItemAddedEvent(cart_id='order-1', item_id='item-1', price=29.99),
            ...     ]) \\
            ...     .when(SubmitCartCommand(aggregate_id='order-1')) \\
            ...     .expect_events([CartSubmittedEvent(cart_id='order-1', total=29.99)])
        """
        return TestExecutor(self._aggregate_class, events, self._injectables)

    def given_commands(self, commands: list[Command]) -> TestExecutor[TAggregate]:
        """
        Start given phase with commands (events will be generated).

        This is useful when you want to set up the aggregate using commands
        rather than directly specifying events. The commands are executed
        and their resulting events are used as the "given" state.

        Args:
            commands: Array of commands to execute

        Returns:
            TestExecutor for the When phase

        Example:
            >>> scenario(OrderAggregate) \\
            ...     .given_commands([
            ...         CreateOrderCommand(aggregate_id='order-1'),
            ...         AddItemCommand(aggregate_id='order-1', item_id='item-1', price=29.99),
            ...     ]) \\
            ...     .when(SubmitCartCommand(aggregate_id='order-1')) \\
            ...     .expect_events([CartSubmittedEvent(cart_id='order-1', total=40.00)])
        """
        aggregate = self._aggregate_class()

        # Inject dependencies
        for type_name, resource in self._injectables.items():
            # Convert to snake_case for Python
            import re

            s1 = re.sub("(.)([A-Z][a-z]+)", r"\1_\2", type_name)
            property_name = re.sub("([a-z0-9])([A-Z])", r"\1_\2", s1).lower()

            if hasattr(aggregate, property_name):
                setattr(aggregate, property_name, resource)

        # Execute each command to generate events
        for command in commands:
            handle_cmd = getattr(aggregate, "_handle_command", None)
            if handle_cmd is not None and callable(handle_cmd):
                handle_cmd(command)

        # Extract generated events
        events = [e.event for e in aggregate.get_uncommitted_events()]

        return TestExecutor(self._aggregate_class, events, self._injectables)


def scenario(aggregate_class: type[TAggregate]) -> AggregateScenario[TAggregate]:
    """
    Factory function to create a new aggregate test scenario.

    Args:
        aggregate_class: The aggregate class to test

    Returns:
        AggregateScenario for fluent configuration

    Example:
        >>> from event_sourcing.testing import scenario
        >>>
        >>> # Happy path
        >>> scenario(OrderAggregate) \\
        ...     .given([CartCreatedEvent(cart_id='order-1')]) \\
        ...     .when(AddItemCommand(aggregate_id='order-1', item_id='item-1', price=29.99)) \\
        ...     .expect_events([ItemAddedEvent(cart_id='order-1', item_id='item-1', price=29.99)])
        >>>
        >>> # Error path
        >>> scenario(OrderAggregate) \\
        ...     .given_no_prior_activity() \\
        ...     .when(SubmitCartCommand(aggregate_id='order-1')) \\
        ...     .expect_exception(BusinessRuleViolationError) \\
        ...     .expect_exception_message('Cannot submit empty cart')
    """
    return AggregateScenario(aggregate_class)
