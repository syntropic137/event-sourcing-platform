/**
 * AggregateScenario - Given-When-Then testing for event-sourced aggregates
 *
 * Inspired by Axon Framework's AggregateTestFixture, this provides a fluent API
 * for testing aggregate command handlers in isolation.
 *
 * @example
 * ```typescript
 * scenario(OrderAggregate)
 *   .given([
 *     new CartCreatedEvent('order-1'),
 *     new ItemAddedEvent('order-1', 'item-1', 29.99),
 *   ])
 *   .when(new SubmitCartCommand('order-1'))
 *   .expectEvents([
 *     new CartSubmittedEvent('order-1', 29.99),
 *   ]);
 * ```
 */

import { AggregateRoot } from '../../core/aggregate';
import { DomainEvent } from '../../core/event';
import { TestExecutor } from './test-executor';

/**
 * AggregateScenario - Entry point for Given-When-Then aggregate testing
 *
 * @template TAggregate - The aggregate type being tested
 */
export class AggregateScenario<TAggregate extends AggregateRoot<DomainEvent>> {
  private readonly AggregateClass: new () => TAggregate;
  private readonly injectables: Map<string, unknown> = new Map();

  constructor(AggregateClass: new () => TAggregate) {
    this.AggregateClass = AggregateClass;
  }

  /**
   * Register a resource that can be injected into command handlers
   *
   * @param resource - The resource to inject (matched by type name)
   * @returns this for chaining
   *
   * @example
   * ```typescript
   * scenario(OrderAggregate)
   *   .registerInjectableResource(mockPricingService)
   *   .given([...])
   *   .when(new AddItemCommand(...))
   *   .expectEvents([...]);
   * ```
   */
  registerInjectableResource<T extends object>(resource: T): this {
    const typeName = resource.constructor.name;
    this.injectables.set(typeName, resource);
    return this;
  }

  /**
   * Start given phase with no prior events (new aggregate)
   *
   * @returns TestExecutor for the When phase
   *
   * @example
   * ```typescript
   * scenario(OrderAggregate)
   *   .givenNoPriorActivity()
   *   .when(new CreateOrderCommand('order-1'))
   *   .expectEvents([new OrderCreatedEvent('order-1')]);
   * ```
   */
  givenNoPriorActivity(): TestExecutor<TAggregate> {
    return this.given([]);
  }

  /**
   * Start given phase with prior events
   *
   * @param events - Array of events representing prior history
   * @returns TestExecutor for the When phase
   *
   * @example
   * ```typescript
   * scenario(OrderAggregate)
   *   .given([
   *     new CartCreatedEvent('order-1'),
   *     new ItemAddedEvent('order-1', 'item-1', 29.99),
   *   ])
   *   .when(new SubmitCartCommand('order-1'))
   *   .expectEvents([new CartSubmittedEvent('order-1', 29.99)]);
   * ```
   */
  given(events: DomainEvent[]): TestExecutor<TAggregate> {
    return new TestExecutor(this.AggregateClass, events, this.injectables);
  }

  /**
   * Start given phase with commands (events will be generated)
   *
   * This is useful when you want to set up the aggregate using commands
   * rather than directly specifying events. The commands are executed
   * and their resulting events are used as the "given" state.
   *
   * @param commands - Array of commands to execute
   * @returns TestExecutor for the When phase
   *
   * @example
   * ```typescript
   * scenario(OrderAggregate)
   *   .givenCommands([
   *     new CreateOrderCommand('order-1'),
   *     new AddItemCommand('order-1', 'item-1', 29.99),
   *   ])
   *   .when(new SubmitCartCommand('order-1'))
   *   .expectEvents([new CartSubmittedEvent('order-1', 29.99)]);
   * ```
   */
  givenCommands(commands: object[]): TestExecutor<TAggregate> {
    const aggregate = new this.AggregateClass();

    // Inject dependencies
    for (const [typeName, resource] of this.injectables) {
      const propertyName = typeName.charAt(0).toLowerCase() + typeName.slice(1);
      if (propertyName in aggregate) {
        (aggregate as Record<string, unknown>)[propertyName] = resource;
      }
    }

    // Execute each command to generate events
    const aggregateWithHandler = aggregate as unknown as {
      handleCommand: (command: object) => void;
    };

    for (const command of commands) {
      aggregateWithHandler.handleCommand(command);
    }

    // Extract generated events
    const events = aggregate.getUncommittedEvents().map((e) => e.event);

    return new TestExecutor(this.AggregateClass, events, this.injectables);
  }
}

/**
 * Factory function to create a new aggregate test scenario
 *
 * @param AggregateClass - The aggregate class to test
 * @returns AggregateScenario for fluent configuration
 *
 * @example
 * ```typescript
 * import { scenario } from '@syntropic137/event-sourcing-typescript/testing';
 *
 * // Happy path
 * scenario(OrderAggregate)
 *   .given([new CartCreatedEvent('order-1')])
 *   .when(new AddItemCommand('order-1', 'item-1', 29.99))
 *   .expectEvents([new ItemAddedEvent('order-1', 'item-1', 29.99)]);
 *
 * // Error path
 * scenario(OrderAggregate)
 *   .givenNoPriorActivity()
 *   .when(new SubmitCartCommand('order-1'))
 *   .expectException(BusinessRuleViolationError)
 *   .expectExceptionMessage('Cannot submit empty cart');
 * ```
 */
export function scenario<TAggregate extends AggregateRoot<DomainEvent>>(
  AggregateClass: new () => TAggregate
): AggregateScenario<TAggregate> {
  return new AggregateScenario(AggregateClass);
}
