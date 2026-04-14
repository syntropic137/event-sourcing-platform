"""
ES Test Kit - Testing Toolkit for Event Sourcing

A comprehensive, purpose-built testing toolkit for event-sourced applications.
Each tool addresses a specific testing concern unique to event sourcing.

Tools:
    - scenario(): Given-When-Then command testing
    - ProcessManagerScenario: Test projection side of ProcessManagers
    - IdempotencyVerifier: Verify process_pending() is idempotent
    - ReplayTester: Golden replay state verification (TODO)
    - InvariantChecker: Business rule verification (TODO)

Example:
    >>> from event_sourcing.testing import scenario
    >>>
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

from event_sourcing.testing.process_manager_scenario import (
    IdempotencyResult,
    IdempotencyVerifier,
    ProcessManagerScenario,
)
from event_sourcing.testing.scenario import (
    AggregateScenario,
    ResultValidator,
    ScenarioAssertionError,
    ScenarioExecutionError,
    TestExecutor,
    scenario,
)

__all__ = [
    # Scenario testing (Given-When-Then)
    "scenario",
    "AggregateScenario",
    "TestExecutor",
    "ResultValidator",
    "ScenarioAssertionError",
    "ScenarioExecutionError",
    # ProcessManager testing
    "ProcessManagerScenario",
    "IdempotencyVerifier",
    "IdempotencyResult",
]
