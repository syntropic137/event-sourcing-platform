/**
 * ResultValidator - Validation phase (Then) of Given-When-Then testing
 *
 * Provides fluent assertions for verifying:
 * - Events emitted by command
 * - Exceptions thrown
 * - Aggregate state after command
 */

import { AggregateRoot } from '../../core/aggregate';
import { DomainEvent } from '../../core/event';
import { ScenarioAssertionError } from './errors';

/**
 * ResultValidator - Validates command execution results
 *
 * @template TAggregate - The aggregate type being tested
 */
export class ResultValidator<TAggregate extends AggregateRoot<DomainEvent>> {
  private readonly aggregate: TAggregate;
  private readonly error: Error | undefined;

  constructor(aggregate: TAggregate, error: Error | undefined) {
    this.aggregate = aggregate;
    this.error = error;
  }

  /**
   * Assert command executed successfully (no exception thrown)
   */
  expectSuccessfulHandlerExecution(): this {
    if (this.error) {
      throw new ScenarioAssertionError(
        `Expected successful execution but got error: ${this.error.name}: ${this.error.message}`
      );
    }

    return this;
  }

  /**
   * Assert specific events were emitted by the command
   *
   * @param expectedEvents - Array of expected events (order matters)
   */
  expectEvents(expectedEvents: DomainEvent[]): this {
    this.expectSuccessfulHandlerExecution();

    const actualEnvelopes = this.aggregate.getUncommittedEvents();
    const actualEvents = actualEnvelopes.map((e) => e.event);

    if (actualEvents.length !== expectedEvents.length) {
      throw new ScenarioAssertionError(
        `Expected ${expectedEvents.length} event(s) but got ${actualEvents.length}.\n` +
          `Expected types: [${expectedEvents.map((e) => e.eventType).join(', ')}]\n` +
          `Actual types: [${actualEvents.map((e) => e.eventType).join(', ')}]`
      );
    }

    for (let i = 0; i < expectedEvents.length; i++) {
      if (!this.eventsMatch(actualEvents[i], expectedEvents[i])) {
        throw new ScenarioAssertionError(
          `Event at index ${i} does not match.\n` +
            `Expected: ${this.formatEvent(expectedEvents[i])}\n` +
            `Actual: ${this.formatEvent(actualEvents[i])}`
        );
      }
    }

    return this;
  }

  /**
   * Assert no events were emitted by the command
   */
  expectNoEvents(): this {
    this.expectSuccessfulHandlerExecution();

    const events = this.aggregate.getUncommittedEvents();
    if (events.length > 0) {
      const eventTypes = events.map((e) => e.event.eventType);
      throw new ScenarioAssertionError(
        `Expected no events but got ${events.length}: [${eventTypes.join(', ')}]`
      );
    }

    return this;
  }

  /**
   * Assert a specific exception type was thrown
   *
   * @param errorType - The expected error class
   */
  expectException<TError extends Error>(errorType: new (...args: any[]) => TError): this {
    if (!this.error) {
      throw new ScenarioAssertionError(
        `Expected exception ${errorType.name} but command succeeded`
      );
    }

    if (!(this.error instanceof errorType)) {
      throw new ScenarioAssertionError(
        `Expected exception ${errorType.name} but got ${this.error.constructor.name}: ${this.error.message}`
      );
    }

    return this;
  }

  /**
   * Assert exception message matches (string contains or regex matches)
   *
   * @param message - String to check for containment, or RegExp to test
   */
  expectExceptionMessage(message: string | RegExp): this {
    if (!this.error) {
      throw new ScenarioAssertionError(
        `Expected exception with message but command succeeded`
      );
    }

    const matches =
      typeof message === 'string'
        ? this.error.message.includes(message)
        : message.test(this.error.message);

    if (!matches) {
      const expectedDesc =
        typeof message === 'string' ? `to contain "${message}"` : `to match ${message}`;
      throw new ScenarioAssertionError(
        `Expected exception message ${expectedDesc} but got "${this.error.message}"`
      );
    }

    return this;
  }

  /**
   * Assert aggregate state using a callback function
   *
   * @param assertion - Callback that receives the aggregate and can make assertions
   */
  expectState(assertion: (aggregate: TAggregate) => void): this {
    this.expectSuccessfulHandlerExecution();
    assertion(this.aggregate);
    return this;
  }

  /**
   * Get the aggregate after command execution (for custom assertions)
   */
  getAggregate(): TAggregate {
    return this.aggregate;
  }

  /**
   * Get the error that was thrown (if any)
   */
  getError(): Error | undefined {
    return this.error;
  }

  /**
   * Check if execution resulted in an error
   */
  hasError(): boolean {
    return this.error !== undefined;
  }

  /**
   * Compare two events for equality
   */
  private eventsMatch(actual: DomainEvent, expected: DomainEvent): boolean {
    // Compare event types first
    if (actual.eventType !== expected.eventType) {
      return false;
    }

    // Compare schema versions
    if (actual.schemaVersion !== expected.schemaVersion) {
      return false;
    }

    // Deep equality on data using JSON comparison
    const actualJson = actual.toJson?.() ?? actual;
    const expectedJson = expected.toJson?.() ?? expected;

    return JSON.stringify(actualJson) === JSON.stringify(expectedJson);
  }

  /**
   * Format an event for error messages
   */
  private formatEvent(event: DomainEvent): string {
    const data = event.toJson?.() ?? event;
    return `${event.eventType}(v${event.schemaVersion}): ${JSON.stringify(data, null, 2)}`;
  }
}
