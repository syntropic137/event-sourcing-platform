/**
 * TestExecutor - Execution phase (When) of Given-When-Then testing
 *
 * Handles:
 * - Aggregate rehydration from given events
 * - Command execution
 * - Error capture
 */

import { AggregateRoot } from '../../core/aggregate';
import { DomainEvent, EventEnvelope, EventFactory } from '../../core/event';
import { ResultValidator } from './result-validator';

/**
 * TestExecutor - Executes commands against a pre-configured aggregate
 *
 * @template TAggregate - The aggregate type being tested
 */
export class TestExecutor<TAggregate extends AggregateRoot<DomainEvent>> {
  private readonly AggregateClass: new () => TAggregate;
  private readonly givenEvents: DomainEvent[];
  private readonly injectables: Map<string, unknown>;

  constructor(
    AggregateClass: new () => TAggregate,
    givenEvents: DomainEvent[],
    injectables: Map<string, unknown>
  ) {
    this.AggregateClass = AggregateClass;
    this.givenEvents = givenEvents;
    this.injectables = injectables;
  }

  /**
   * Execute a command against the aggregate
   *
   * @param command - The command to execute
   * @returns ResultValidator for making assertions
   */
  when<TCommand extends object>(command: TCommand): ResultValidator<TAggregate> {
    const aggregate = new this.AggregateClass();

    // Inject dependencies if the aggregate supports it
    this.injectDependencies(aggregate);

    // Rehydrate from given events
    if (this.givenEvents.length > 0) {
      const envelopes = this.givenEvents.map((event, index) =>
        this.createEnvelope(event, aggregate, index + 1)
      );
      aggregate.rehydrate(envelopes);
      aggregate.markEventsAsCommitted();
    }

    // Execute command and capture result
    let error: Error | undefined;
    try {
      this.executeCommand(aggregate, command);
    } catch (e) {
      error = e as Error;
    }

    return new ResultValidator(aggregate, error);
  }

  /**
   * Create an event envelope with test metadata
   */
  private createEnvelope(
    event: DomainEvent,
    aggregate: TAggregate,
    nonce: number
  ): EventEnvelope<DomainEvent> {
    return EventFactory.create(event, {
      aggregateId: this.extractAggregateId(event) ?? 'test-aggregate',
      aggregateType: aggregate.getAggregateType(),
      aggregateNonce: nonce,
    });
  }

  /**
   * Try to extract aggregate ID from event if it has one
   */
  private extractAggregateId(event: DomainEvent): string | undefined {
    // Check common property names for aggregate ID
    const eventAny = event as unknown as Record<string, unknown>;
    return (
      (eventAny.aggregateId as string) ??
      (eventAny.id as string) ??
      (eventAny.orderId as string) ??
      (eventAny.accountId as string) ??
      (eventAny.customerId as string) ??
      (eventAny.cartId as string)
    );
  }

  /**
   * Execute command using the aggregate's handleCommand method
   */
  private executeCommand<TCommand extends object>(aggregate: TAggregate, command: TCommand): void {
    // Access the protected handleCommand method via cast
    const aggregateWithHandler = aggregate as unknown as {
      handleCommand: (command: TCommand) => void;
    };

    if (typeof aggregateWithHandler.handleCommand === 'function') {
      aggregateWithHandler.handleCommand(command);
    } else {
      throw new Error(
        `Aggregate ${aggregate.getAggregateType()} does not have a handleCommand method. ` +
          `Make sure it extends AggregateRoot and has @CommandHandler decorated methods.`
      );
    }
  }

  /**
   * Inject dependencies into aggregate if it supports dependency injection
   */
  private injectDependencies(aggregate: TAggregate): void {
    if (this.injectables.size === 0) {
      return;
    }

    // Check if aggregate has a setDependencies method
    const aggregateWithDI = aggregate as unknown as {
      setDependencies?: (dependencies: Map<string, unknown>) => void;
    };

    if (typeof aggregateWithDI.setDependencies === 'function') {
      aggregateWithDI.setDependencies(this.injectables);
    }

    // Also try to inject via property names matching injectable type names
    for (const [typeName, resource] of this.injectables) {
      const propertyName = this.toCamelCase(typeName);
      if (propertyName in aggregate) {
        (aggregate as Record<string, unknown>)[propertyName] = resource;
      }
    }
  }

  /**
   * Convert PascalCase type name to camelCase property name
   */
  private toCamelCase(pascalCase: string): string {
    return pascalCase.charAt(0).toLowerCase() + pascalCase.slice(1);
  }
}
