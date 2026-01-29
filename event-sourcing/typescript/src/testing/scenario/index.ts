/**
 * Scenario Testing - Given-When-Then for Event-Sourced Aggregates
 *
 * Provides a fluent API for testing aggregate command handlers in isolation,
 * inspired by Axon Framework's AggregateTestFixture.
 *
 * @example
 * ```typescript
 * import { scenario } from '@neuralempowerment/event-sourcing-typescript/testing';
 *
 * // Happy path: command produces events
 * scenario(OrderAggregate)
 *   .given([
 *     new CartCreatedEvent('order-1'),
 *     new ItemAddedEvent('order-1', 'item-1', 29.99),
 *   ])
 *   .when(new SubmitCartCommand('order-1'))
 *   .expectEvents([
 *     new CartSubmittedEvent('order-1', 29.99),
 *   ]);
 *
 * // Error path: business rule violation
 * scenario(OrderAggregate)
 *   .givenNoPriorActivity()
 *   .when(new SubmitCartCommand('order-1'))
 *   .expectException(BusinessRuleViolationError)
 *   .expectExceptionMessage('Cannot submit empty cart');
 * ```
 */

// Main exports
export { scenario, AggregateScenario } from './aggregate-scenario';
export { TestExecutor } from './test-executor';
export { ResultValidator } from './result-validator';

// Error exports
export { ScenarioAssertionError, ScenarioExecutionError } from './errors';
