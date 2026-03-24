/**
 * ReplayTester - Core golden replay testing functionality
 *
 * Replays events against an aggregate and verifies the resulting state.
 */

import { AggregateRoot } from '../../core/aggregate';
import { DomainEvent, EventEnvelope } from '../../core/event';
import { TestFixture, FixtureEvent, ExpectedState } from '../fixtures/fixture-types';
import { createDiff, StateDifference } from './state-assertions';

/**
 * Result of a replay operation
 */
export interface ReplayResult<TAggregate extends AggregateRoot<DomainEvent>> {
  /** Whether the replay was successful */
  success: boolean;

  /** The aggregate after replay */
  aggregate: TAggregate;

  /** The aggregate's state (extracted via getState if available) */
  state: Record<string, unknown>;

  /** The aggregate's version after replay */
  version: number;

  /** Number of events replayed */
  eventCount: number;

  /** Any errors that occurred */
  errors: ReplayError[];

  /** State comparison result (if expectedState was provided) */
  stateComparison?: StateComparisonResult;
}

/**
 * Error that occurred during replay
 */
export interface ReplayError {
  /** Event index that caused the error */
  eventIndex: number;

  /** Event type */
  eventType: string;

  /** Error message */
  message: string;

  /** Original error */
  originalError?: Error;
}

/**
 * Result of comparing actual state to expected state
 */
export interface StateComparisonResult {
  /** Whether the states match */
  matches: boolean;

  /** Differences found (if any) */
  differences: StateDifference[];
}

// Re-export StateDifference from state-assertions for backwards compatibility
export type { StateDifference } from './state-assertions';

/**
 * Options for the ReplayTester
 */
export interface ReplayTesterOptions {
  /** Custom event factory for creating domain events from fixture data */
  eventFactory?: EventFactory;

  /** Whether to stop on first error */
  stopOnError?: boolean;

  /** Custom state extractor (if aggregate doesn't have getState) */
  stateExtractor?: <T extends AggregateRoot<DomainEvent>>(aggregate: T) => Record<string, unknown>;
}

/**
 * Factory function to create domain events from fixture event data
 */
export type EventFactory = (fixtureEvent: FixtureEvent) => DomainEvent;

/**
 * Default event factory that creates simple event objects
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
 * Try extracting state via getState() method
 */
function tryGetState(aggregate: object): Record<string, unknown> | null {
  if ('getState' in aggregate && typeof (aggregate as Record<string, unknown>).getState === 'function') {
    return (aggregate as { getState: () => unknown }).getState() as Record<string, unknown>;
  }
  return null;
}

/**
 * Try extracting state via toJSON() method
 */
function tryToJSON(aggregate: object): Record<string, unknown> | null {
  if ('toJSON' in aggregate && typeof (aggregate as Record<string, unknown>).toJSON === 'function') {
    return (aggregate as { toJSON: () => unknown }).toJSON() as Record<string, unknown>;
  }
  return null;
}

/**
 * Extract state from prototype getters and own properties
 */
function extractPublicProperties(aggregate: object): Record<string, unknown> {
  const state: Record<string, unknown> = {};
  const prototype = Object.getPrototypeOf(aggregate);
  const descriptors = Object.getOwnPropertyDescriptors(prototype);

  for (const [key, descriptor] of Object.entries(descriptors)) {
    if (descriptor.get && key !== 'id' && key !== 'version' && key !== 'aggregateId') {
      try {
        state[key] = (aggregate as Record<string, unknown>)[key];
      } catch {
        // Skip properties that throw
      }
    }
  }

  for (const key of Object.keys(aggregate)) {
    if (!key.startsWith('_') && key !== 'id' && key !== 'version') {
      state[key] = (aggregate as Record<string, unknown>)[key];
    }
  }

  return state;
}

/**
 * Default state extractor - tries common patterns in order
 */
function defaultStateExtractor<T extends AggregateRoot<DomainEvent>>(
  aggregate: T
): Record<string, unknown> {
  return tryGetState(aggregate) ?? tryToJSON(aggregate) ?? extractPublicProperties(aggregate);
}

/**
 * ReplayTester - Test aggregates by replaying events from fixtures
 *
 * @example
 * ```typescript
 * const tester = new ReplayTester(OrderAggregate);
 * const fixture = await loadFixture('./fixtures/order-lifecycle.json');
 * const result = await tester.replay(fixture.events);
 *
 * expect(result.success).toBe(true);
 * expect(result.state.status).toBe('shipped');
 * ```
 */
export class ReplayTester<TAggregate extends AggregateRoot<DomainEvent>> {
  private readonly AggregateClass: new () => TAggregate;
  private readonly options: Required<ReplayTesterOptions>;

  constructor(
    AggregateClass: new () => TAggregate,
    options: ReplayTesterOptions = {}
  ) {
    this.AggregateClass = AggregateClass;
    this.options = {
      eventFactory: options.eventFactory ?? defaultEventFactory,
      stopOnError: options.stopOnError ?? true,
      stateExtractor: options.stateExtractor ?? defaultStateExtractor,
    };
  }

  /**
   * Replay events and return the resulting aggregate state
   */
  async replay(events: FixtureEvent[], aggregateId?: string): Promise<ReplayResult<TAggregate>> {
    const aggregate = new this.AggregateClass();
    const errors: ReplayError[] = [];
    let eventCount = 0;

    // Create event envelopes from fixture events
    const envelopes: EventEnvelope<DomainEvent>[] = events.map((fixtureEvent, index) => {
      const event = this.options.eventFactory(fixtureEvent);
      const id = aggregateId ?? `test-aggregate-${Date.now()}`;

      return {
        event,
        metadata: {
          eventId: fixtureEvent.metadata?.eventId ?? `event-${index}`,
          timestamp: fixtureEvent.metadata?.timestamp ?? new Date().toISOString(),
          recordedTimestamp: new Date().toISOString(),
          aggregateNonce: index + 1,
          aggregateId: id,
          aggregateType: aggregate.getAggregateType(),
          tenantId: fixtureEvent.metadata?.tenantId,
          correlationId: fixtureEvent.metadata?.correlationId,
          causationId: fixtureEvent.metadata?.causationId,
          actorId: fixtureEvent.metadata?.actorId,
          contentType: 'application/json',
          headers: {},
          customMetadata: {},
        },
      };
    });

    // Rehydrate aggregate from events
    try {
      aggregate.rehydrate(envelopes);
      eventCount = events.length;
    } catch (error) {
      errors.push({
        eventIndex: eventCount,
        eventType: events[eventCount]?.type ?? 'unknown',
        message: (error as Error).message,
        originalError: error as Error,
      });
    }

    const state = this.options.stateExtractor(aggregate);

    return {
      success: errors.length === 0,
      aggregate,
      state,
      version: aggregate.version,
      eventCount,
      errors,
    };
  }

  /**
   * Replay events and assert the resulting state matches expectations
   */
  async replayAndAssert(
    fixture: TestFixture,
    expectedState?: ExpectedState
  ): Promise<ReplayResult<TAggregate>> {
    const result = await this.replay(fixture.events, fixture.aggregateId);

    const stateToCheck = expectedState ?? fixture.expectedState;

    if (stateToCheck) {
      const differences = createDiff(stateToCheck, result.state);
      result.stateComparison = {
        matches: differences.length === 0,
        differences,
      };

      if (!result.stateComparison.matches) {
        result.success = false;
      }
    }

    // Check version if specified
    if (fixture.expectedVersion !== undefined) {
      if (result.version !== fixture.expectedVersion) {
        result.success = false;
        result.errors.push({
          eventIndex: -1,
          eventType: 'version-check',
          message: `Expected version ${fixture.expectedVersion}, got ${result.version}`,
        });
      }
    }

    return result;
  }

  /**
   * Replay events one at a time, returning intermediate states
   */
  async replayStepByStep(
    events: FixtureEvent[],
    aggregateId?: string
  ): Promise<Array<{ eventIndex: number; event: FixtureEvent; state: Record<string, unknown> }>> {
    const steps: Array<{ eventIndex: number; event: FixtureEvent; state: Record<string, unknown> }> = [];

    for (let i = 0; i < events.length; i++) {
      const eventsToReplay = events.slice(0, i + 1);
      const result = await this.replay(eventsToReplay, aggregateId);

      steps.push({
        eventIndex: i,
        event: events[i],
        state: { ...result.state },
      });
    }

    return steps;
  }
}

/**
 * Convenience function to create a ReplayTester
 */
export function createReplayTester<TAggregate extends AggregateRoot<DomainEvent>>(
  AggregateClass: new () => TAggregate,
  options?: ReplayTesterOptions
): ReplayTester<TAggregate> {
  return new ReplayTester(AggregateClass, options);
}
