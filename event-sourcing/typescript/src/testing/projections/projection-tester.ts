/**
 * ProjectionTester - Test projections for correctness and determinism
 */

import { DomainEvent, EventEnvelope } from '../../core/event';
import { FixtureEvent } from '../fixtures/fixture-types';
import { createDiff, StateDifference } from '../replay/state-assertions';

/**
 * Interface that projections must implement for testing
 */
export interface TestableProjection<TState = unknown> {
  /** Get the projection name */
  getName(): string;

  /** Handle an event */
  handleEvent(envelope: EventEnvelope<DomainEvent>): Promise<void> | void;

  /** Get the current state */
  getState(): TState;

  /** Reset the projection to initial state */
  reset(): Promise<void> | void;

  /** Get subscribed event types (optional - if not provided, receives all events) */
  getSubscribedEventTypes?(): Set<string> | string[] | null;
}

/**
 * Result of projection testing
 */
export interface ProjectionTestResult {
  /** Whether all tests passed */
  passed: boolean;

  /** Final state of the projection */
  finalState: unknown;

  /** Number of events processed */
  eventsProcessed: number;

  /** Any errors that occurred */
  errors: ProjectionTestError[];
}

/**
 * Error during projection testing
 */
export interface ProjectionTestError {
  /** Type of error */
  type: 'event_handling' | 'state_mismatch' | 'determinism';

  /** Error message */
  message: string;

  /** Event index (if applicable) */
  eventIndex?: number;

  /** Event type (if applicable) */
  eventType?: string;

  /** Original error (if applicable) */
  originalError?: Error;
}

/**
 * Result of determinism verification
 */
export interface DeterminismResult {
  /** Whether the projection is deterministic */
  isDeterministic: boolean;

  /** Number of iterations run */
  iterations: number;

  /** States from each iteration */
  states: unknown[];

  /** Differences found between iterations (if any) */
  differences: StateDifference[];
}

/**
 * Options for ProjectionTester
 */
export interface ProjectionTesterOptions {
  /** Custom event factory */
  eventFactory?: (fixtureEvent: FixtureEvent) => DomainEvent;
}

/**
 * Default event factory
 */
function defaultEventFactory(fixtureEvent: FixtureEvent): DomainEvent {
  return {
    eventType: fixtureEvent.type,
    schemaVersion: parseInt(fixtureEvent.version.replace('v', ''), 10) || 1,
    toJson: () => fixtureEvent.data,
    ...fixtureEvent.data,
  } as DomainEvent;
}

/**
 * ProjectionTester - Test projections for correctness and determinism
 *
 * @example
 * ```typescript
 * const tester = new ProjectionTester(new OrderSummaryProjection());
 *
 * // Test basic processing
 * const result = await tester.processEvents(events);
 * expect(result.passed).toBe(true);
 *
 * // Verify determinism
 * const determinism = await tester.verifyDeterminism(events, { iterations: 3 });
 * expect(determinism.isDeterministic).toBe(true);
 * ```
 */
export class ProjectionTester<TState = unknown> {
  private readonly projection: TestableProjection<TState>;
  private readonly options: Required<ProjectionTesterOptions>;

  constructor(
    projection: TestableProjection<TState>,
    options: ProjectionTesterOptions = {}
  ) {
    this.projection = projection;
    this.options = {
      eventFactory: options.eventFactory ?? defaultEventFactory,
    };
  }

  /**
   * Process events through the projection
   */
  async processEvents(
    events: FixtureEvent[],
    aggregateId?: string
  ): Promise<ProjectionTestResult> {
    const errors: ProjectionTestError[] = [];
    let eventsProcessed = 0;

    // Reset projection to clean state
    await this.projection.reset();

    const subscribedTypes = this.getSubscribedTypes();

    for (let i = 0; i < events.length; i++) {
      const fixtureEvent = events[i];

      // Skip if projection doesn't care about this event type
      if (subscribedTypes && !subscribedTypes.has(fixtureEvent.type)) {
        continue;
      }

      const event = this.options.eventFactory(fixtureEvent);
      const id = aggregateId ?? `test-aggregate-${Date.now()}`;

      const envelope: EventEnvelope<DomainEvent> = {
        event,
        metadata: {
          eventId: fixtureEvent.metadata?.eventId ?? `event-${i}`,
          timestamp: fixtureEvent.metadata?.timestamp ?? new Date().toISOString(),
          recordedTimestamp: new Date().toISOString(),
          aggregateNonce: i + 1,
          aggregateId: id,
          aggregateType: 'TestAggregate',
          globalNonce: i + 1,
          contentType: 'application/json',
          headers: {},
          customMetadata: {},
        },
      };

      try {
        await this.projection.handleEvent(envelope);
        eventsProcessed++;
      } catch (error) {
        errors.push({
          type: 'event_handling',
          message: `Failed to handle event: ${(error as Error).message}`,
          eventIndex: i,
          eventType: fixtureEvent.type,
          originalError: error as Error,
        });
      }
    }

    return {
      passed: errors.length === 0,
      finalState: this.projection.getState(),
      eventsProcessed,
      errors,
    };
  }

  /**
   * Process events and verify final state matches expected
   */
  async processAndAssert(
    events: FixtureEvent[],
    expectedState: Partial<TState>,
    aggregateId?: string
  ): Promise<ProjectionTestResult> {
    const result = await this.processEvents(events, aggregateId);

    const differences = createDiff(
      expectedState as Record<string, unknown>,
      result.finalState as Record<string, unknown>
    );

    if (differences.length > 0) {
      result.passed = false;
      result.errors.push({
        type: 'state_mismatch',
        message: `State mismatch: ${differences.map((d) => `${d.path}: expected ${JSON.stringify(d.expected)}, got ${JSON.stringify(d.actual)}`).join('; ')}`,
      });
    }

    return result;
  }

  /**
   * Verify that the projection is deterministic (same events = same state)
   */
  async verifyDeterminism(
    events: FixtureEvent[],
    options: { iterations?: number; aggregateId?: string } = {}
  ): Promise<DeterminismResult> {
    const { iterations = 3, aggregateId } = options;
    const states: unknown[] = [];

    for (let i = 0; i < iterations; i++) {
      const result = await this.processEvents(events, aggregateId);
      states.push(JSON.parse(JSON.stringify(result.finalState))); // Deep clone
    }

    // Compare all states to the first one
    const differences: StateDifference[] = [];
    const firstState = states[0] as Record<string, unknown>;

    for (let i = 1; i < states.length; i++) {
      const iterDiffs = createDiff(firstState, states[i] as Record<string, unknown>);
      if (iterDiffs.length > 0) {
        differences.push(
          ...iterDiffs.map((d) => ({
            ...d,
            path: `iteration[${i}].${d.path}`,
          }))
        );
      }
    }

    return {
      isDeterministic: differences.length === 0,
      iterations,
      states,
      differences,
    };
  }

  /**
   * Verify rebuild produces same state
   */
  async verifyRebuild(
    events: FixtureEvent[],
    aggregateId?: string
  ): Promise<{
    passed: boolean;
    originalState: unknown;
    rebuiltState: unknown;
    differences: StateDifference[];
  }> {
    // First pass
    const firstResult = await this.processEvents(events, aggregateId);
    const originalState = JSON.parse(JSON.stringify(firstResult.finalState));

    // Reset and replay
    const secondResult = await this.processEvents(events, aggregateId);
    const rebuiltState = secondResult.finalState;

    const differences = createDiff(
      originalState as Record<string, unknown>,
      rebuiltState as Record<string, unknown>
    );

    return {
      passed: differences.length === 0,
      originalState,
      rebuiltState,
      differences,
    };
  }

  /**
   * Get current projection state
   */
  getState(): TState {
    return this.projection.getState();
  }

  /**
   * Reset projection to initial state
   */
  async reset(): Promise<void> {
    await this.projection.reset();
  }

  /**
   * Get projection name
   */
  getName(): string {
    return this.projection.getName();
  }

  /**
   * Get subscribed event types as a Set
   */
  private getSubscribedTypes(): Set<string> | null {
    if (!this.projection.getSubscribedEventTypes) {
      return null;
    }

    const types = this.projection.getSubscribedEventTypes();
    if (types === null) {
      return null;
    }

    return types instanceof Set ? types : new Set(types);
  }
}

/**
 * Convenience function to create a ProjectionTester
 */
export function createProjectionTester<TState = unknown>(
  projection: TestableProjection<TState>,
  options?: ProjectionTesterOptions
): ProjectionTester<TState> {
  return new ProjectionTester(projection, options);
}
