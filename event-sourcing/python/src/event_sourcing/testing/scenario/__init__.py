"""
Scenario Testing - Given-When-Then for Event-Sourced Aggregates

Provides a fluent API for testing aggregate command handlers in isolation,
inspired by Axon Framework's AggregateTestFixture.

Example:
    >>> from event_sourcing.testing import scenario
    >>>
    >>> # Happy path: command produces events
    >>> scenario(OrderAggregate) \\
    ...     .given([
    ...         CartCreatedEvent(cart_id='order-1'),
    ...         ItemAddedEvent(cart_id='order-1', item_id='item-1', price=29.99),
    ...     ]) \\
    ...     .when(SubmitCartCommand(aggregate_id='order-1')) \\
    ...     .expect_events([
    ...         CartSubmittedEvent(cart_id='order-1', total=29.99),
    ...     ])
    >>>
    >>> # Error path: business rule violation
    >>> scenario(OrderAggregate) \\
    ...     .given_no_prior_activity() \\
    ...     .when(SubmitCartCommand(aggregate_id='order-1')) \\
    ...     .expect_exception(BusinessRuleViolationError) \\
    ...     .expect_exception_message('Cannot submit empty cart')
"""

from event_sourcing.testing.scenario.aggregate_scenario import (
    AggregateScenario,
    scenario,
)
from event_sourcing.testing.scenario.errors import (
    ScenarioAssertionError,
    ScenarioExecutionError,
)
from event_sourcing.testing.scenario.result_validator import ResultValidator
from event_sourcing.testing.scenario.test_executor import TestExecutor

__all__ = [
    "scenario",
    "AggregateScenario",
    "TestExecutor",
    "ResultValidator",
    "ScenarioAssertionError",
    "ScenarioExecutionError",
]
