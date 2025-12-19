/**
 * InvariantChecker - Verify aggregate invariants during testing
 */

import { AggregateRoot } from '../../core/aggregate';
import { DomainEvent, EventEnvelope } from '../../core/event';
import { FixtureEvent } from '../fixtures/fixture-types';
import {
  InvariantMetadata,
  InvariantAwareConstructor,
  getInvariants,
  INVARIANT_METADATA,
} from './invariant-decorator';

/**
 * Result of checking a single invariant
 */
export interface InvariantCheckResult {
  /** Invariant description */
  description: string;

  /** Method name */
  methodName: string;

  /** Whether the invariant holds */
  holds: boolean;

  /** Severity level */
  severity: 'error' | 'warning';

  /** Error message if invariant was violated */
  errorMessage?: string;
}

/**
 * Result of checking all invariants at a point in time
 */
export interface InvariantSnapshot {
  /** Event index that triggered this check (-1 for initial state) */
  eventIndex: number;

  /** Event type (if applicable) */
  eventType?: string;

  /** Results of all invariant checks */
  results: InvariantCheckResult[];

  /** Whether all error-level invariants hold */
  allErrorsPass: boolean;

  /** Whether all invariants (including warnings) hold */
  allPass: boolean;
}

/**
 * Overall result of invariant verification
 */
export interface InvariantVerificationResult {
  /** Whether all invariants held throughout the event sequence */
  passed: boolean;

  /** Total number of invariants checked */
  invariantCount: number;

  /** Total number of events processed */
  eventCount: number;

  /** All violations found */
  violations: InvariantViolation[];

  /** Snapshots at each event (if requested) */
  snapshots?: InvariantSnapshot[];
}

/**
 * A single invariant violation
 */
export interface InvariantViolation {
  /** Event index that caused the violation */
  eventIndex: number;

  /** Event type that caused the violation */
  eventType: string;

  /** Invariant that was violated */
  invariantDescription: string;

  /** Method name */
  methodName: string;

  /** Severity */
  severity: 'error' | 'warning';
}

/**
 * Options for the InvariantChecker
 */
export interface InvariantCheckerOptions {
  /** Custom event factory */
  eventFactory?: (fixtureEvent: FixtureEvent) => DomainEvent;

  /** Whether to include full snapshots in results */
  includeSnapshots?: boolean;

  /** Whether to stop on first violation */
  stopOnFirstViolation?: boolean;

  /** Additional invariants to check (beyond decorated ones) */
  additionalInvariants?: Array<{
    description: string;
    check: (aggregate: AggregateRoot<DomainEvent>) => boolean;
    severity?: 'error' | 'warning';
  }>;
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
 * InvariantChecker - Verify that aggregate invariants hold after each event
 *
 * @example
 * ```typescript
 * const checker = new InvariantChecker(BankAccountAggregate);
 * const fixture = await loadFixture('./fixtures/account-operations.json');
 *
 * const result = await checker.verifyAfterEachEvent(fixture.events);
 *
 * expect(result.passed).toBe(true);
 * expect(result.violations).toHaveLength(0);
 * ```
 */
export class InvariantChecker<TAggregate extends AggregateRoot<DomainEvent>> {
  private readonly AggregateClass: new () => TAggregate;
  private readonly options: Required<InvariantCheckerOptions>;
  private readonly invariants: Map<string, InvariantMetadata>;
  private readonly additionalInvariants: InvariantCheckerOptions['additionalInvariants'];

  constructor(
    AggregateClass: new () => TAggregate,
    options: InvariantCheckerOptions = {}
  ) {
    this.AggregateClass = AggregateClass;
    this.options = {
      eventFactory: options.eventFactory ?? defaultEventFactory,
      includeSnapshots: options.includeSnapshots ?? false,
      stopOnFirstViolation: options.stopOnFirstViolation ?? false,
      additionalInvariants: options.additionalInvariants ?? [],
    };

    // Get decorated invariants from the class
    this.invariants = getInvariants(AggregateClass as unknown as InvariantAwareConstructor);
    this.additionalInvariants = options.additionalInvariants ?? [];
  }

  /**
   * Check all invariants on an aggregate instance
   */
  checkInvariants(aggregate: TAggregate): InvariantCheckResult[] {
    const results: InvariantCheckResult[] = [];

    // Check decorated invariants
    for (const [methodName, metadata] of this.invariants) {
      const method = (aggregate as Record<string, unknown>)[methodName];

      if (typeof method !== 'function') {
        results.push({
          description: metadata.description,
          methodName,
          holds: false,
          severity: metadata.severity,
          errorMessage: `Invariant method '${methodName}' is not a function`,
        });
        continue;
      }

      try {
        const holds = (method as () => boolean).call(aggregate);
        results.push({
          description: metadata.description,
          methodName,
          holds: Boolean(holds),
          severity: metadata.severity,
          errorMessage: holds ? undefined : `Invariant violated: ${metadata.description}`,
        });
      } catch (error) {
        results.push({
          description: metadata.description,
          methodName,
          holds: false,
          severity: metadata.severity,
          errorMessage: `Invariant check threw error: ${(error as Error).message}`,
        });
      }
    }

    // Check additional invariants
    for (const additional of this.additionalInvariants ?? []) {
      try {
        const holds = additional.check(aggregate);
        results.push({
          description: additional.description,
          methodName: '<additional>',
          holds: Boolean(holds),
          severity: additional.severity ?? 'error',
          errorMessage: holds ? undefined : `Invariant violated: ${additional.description}`,
        });
      } catch (error) {
        results.push({
          description: additional.description,
          methodName: '<additional>',
          holds: false,
          severity: additional.severity ?? 'error',
          errorMessage: `Invariant check threw error: ${(error as Error).message}`,
        });
      }
    }

    return results;
  }

  /**
   * Verify invariants after each event in a sequence
   */
  async verifyAfterEachEvent(
    events: FixtureEvent[],
    aggregateId?: string
  ): Promise<InvariantVerificationResult> {
    const violations: InvariantViolation[] = [];
    const snapshots: InvariantSnapshot[] = [];
    const aggregate = new this.AggregateClass();

    const invariantCount = this.invariants.size + (this.additionalInvariants?.length ?? 0);

    // Check initial state
    const initialResults = this.checkInvariants(aggregate);
    const initialSnapshot: InvariantSnapshot = {
      eventIndex: -1,
      results: initialResults,
      allErrorsPass: initialResults.every((r) => r.holds || r.severity !== 'error'),
      allPass: initialResults.every((r) => r.holds),
    };

    if (this.options.includeSnapshots) {
      snapshots.push(initialSnapshot);
    }

    // Process each event
    for (let i = 0; i < events.length; i++) {
      const fixtureEvent = events[i];
      const event = this.options.eventFactory(fixtureEvent);
      const id = aggregateId ?? `test-aggregate-${Date.now()}`;

      // Create envelope and apply event
      const envelope: EventEnvelope<DomainEvent> = {
        event,
        metadata: {
          eventId: fixtureEvent.metadata?.eventId ?? `event-${i}`,
          timestamp: fixtureEvent.metadata?.timestamp ?? new Date().toISOString(),
          recordedTimestamp: new Date().toISOString(),
          aggregateNonce: i + 1,
          aggregateId: id,
          aggregateType: aggregate.getAggregateType(),
          contentType: 'application/json',
          headers: {},
          customMetadata: {},
        },
      };

      try {
        aggregate.applyEvent(event);
      } catch (error) {
        violations.push({
          eventIndex: i,
          eventType: fixtureEvent.type,
          invariantDescription: `Event application failed: ${(error as Error).message}`,
          methodName: '<apply>',
          severity: 'error',
        });

        if (this.options.stopOnFirstViolation) {
          break;
        }
        continue;
      }

      // Check invariants after this event
      const results = this.checkInvariants(aggregate);

      const snapshot: InvariantSnapshot = {
        eventIndex: i,
        eventType: fixtureEvent.type,
        results,
        allErrorsPass: results.every((r) => r.holds || r.severity !== 'error'),
        allPass: results.every((r) => r.holds),
      };

      if (this.options.includeSnapshots) {
        snapshots.push(snapshot);
      }

      // Record violations
      for (const result of results) {
        if (!result.holds) {
          violations.push({
            eventIndex: i,
            eventType: fixtureEvent.type,
            invariantDescription: result.description,
            methodName: result.methodName,
            severity: result.severity,
          });
        }
      }

      if (this.options.stopOnFirstViolation && violations.length > 0) {
        break;
      }
    }

    // Determine if passed (only error-level violations count)
    const errorViolations = violations.filter((v) => v.severity === 'error');

    return {
      passed: errorViolations.length === 0,
      invariantCount,
      eventCount: events.length,
      violations,
      snapshots: this.options.includeSnapshots ? snapshots : undefined,
    };
  }

  /**
   * Add an additional invariant to check (beyond decorated ones)
   */
  addInvariant(
    description: string,
    check: (aggregate: TAggregate) => boolean,
    severity: 'error' | 'warning' = 'error'
  ): this {
    (this.additionalInvariants ?? []).push({
      description,
      check: check as (aggregate: AggregateRoot<DomainEvent>) => boolean,
      severity,
    });
    return this;
  }
}

/**
 * Convenience function to create an InvariantChecker
 */
export function createInvariantChecker<TAggregate extends AggregateRoot<DomainEvent>>(
  AggregateClass: new () => TAggregate,
  options?: InvariantCheckerOptions
): InvariantChecker<TAggregate> {
  return new InvariantChecker(AggregateClass, options);
}
