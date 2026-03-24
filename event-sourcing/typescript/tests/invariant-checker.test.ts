/**
 * Tests for InvariantChecker — verifies aggregate invariants during event replay
 */

import {
  AggregateRoot,
  AggregateDecorator as Aggregate,
  EventSourcingHandler,
  BaseDomainEvent,
} from '../src';
import { Invariant } from '../src/testing/invariants/invariant-decorator';
import { InvariantChecker } from '../src/testing/invariants/invariant-checker';
import type { FixtureEvent } from '../src/testing/fixtures/fixture-types';

// ---------------------------------------------------------------------------
// Test Domain: Account with balance invariant
// ---------------------------------------------------------------------------

class BalanceChangedEvent extends BaseDomainEvent {
  readonly eventType = 'BalanceChanged' as const;
  readonly schemaVersion = 1 as const;
  constructor(public readonly amount: number) {
    super();
  }
}

type AccountEvent = BalanceChangedEvent;

@Aggregate('TestAccount')
class TestAccountAggregate extends AggregateRoot<AccountEvent> {
  private _balance = 0;

  @EventSourcingHandler('BalanceChanged')
  private onBalanceChanged(event: BalanceChangedEvent): void {
    this._balance += event.amount;
  }

  @Invariant('Balance must be non-negative')
  balanceNonNegative(): boolean {
    return this._balance >= 0;
  }

  get balance(): number {
    return this._balance;
  }
}

// ---------------------------------------------------------------------------
// Event factory: converts FixtureEvent → DomainEvent for the checker
// ---------------------------------------------------------------------------

function accountEventFactory(fixtureEvent: FixtureEvent): AccountEvent {
  if (fixtureEvent.type === 'BalanceChanged') {
    return new BalanceChangedEvent(fixtureEvent.data.amount as number);
  }
  throw new Error(`Unknown event type: ${fixtureEvent.type}`);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function balanceEvent(amount: number): FixtureEvent {
  return { type: 'BalanceChanged', version: 'v1', data: { amount } };
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe('InvariantChecker', () => {
  it('verifies invariants hold on initial (empty) aggregate state', async () => {
    const checker = new InvariantChecker(TestAccountAggregate, {
      eventFactory: accountEventFactory,
      includeSnapshots: true,
    });

    const result = await checker.verifyAfterEachEvent([]);

    expect(result.passed).toBe(true);
    expect(result.violations).toHaveLength(0);
    expect(result.eventCount).toBe(0);
    expect(result.invariantCount).toBe(1);
    // Initial snapshot should be present
    expect(result.snapshots).toBeDefined();
    expect(result.snapshots!).toHaveLength(1); // just the initial check
    expect(result.snapshots![0].eventIndex).toBe(-1);
  });

  it('verifies invariants after each event', async () => {
    const checker = new InvariantChecker(TestAccountAggregate, {
      eventFactory: accountEventFactory,
      includeSnapshots: true,
    });

    const events: FixtureEvent[] = [
      balanceEvent(100),
      balanceEvent(-30),
      balanceEvent(50),
    ];

    const result = await checker.verifyAfterEachEvent(events);

    expect(result.passed).toBe(true);
    expect(result.violations).toHaveLength(0);
    expect(result.eventCount).toBe(3);
    // initial + 3 events = 4 snapshots
    expect(result.snapshots!).toHaveLength(4);
  });

  it('detects violations when invariant fails', async () => {
    const checker = new InvariantChecker(TestAccountAggregate, {
      eventFactory: accountEventFactory,
    });

    const events: FixtureEvent[] = [
      balanceEvent(50),
      balanceEvent(-100), // drives balance to -50 → invariant fails
    ];

    const result = await checker.verifyAfterEachEvent(events);

    expect(result.passed).toBe(false);
    expect(result.violations.length).toBeGreaterThanOrEqual(1);
    expect(result.violations[0].eventIndex).toBe(1);
    expect(result.violations[0].eventType).toBe('BalanceChanged');
    expect(result.violations[0].invariantDescription).toBe('Balance must be non-negative');
  });

  it('stops on first violation when stopOnFirstViolation is true', async () => {
    const checker = new InvariantChecker(TestAccountAggregate, {
      eventFactory: accountEventFactory,
      stopOnFirstViolation: true,
    });

    const events: FixtureEvent[] = [
      balanceEvent(-10),  // violation
      balanceEvent(-20),  // would also violate, but should not be checked
    ];

    const result = await checker.verifyAfterEachEvent(events);

    expect(result.passed).toBe(false);
    // Only the first violation is recorded; the second event is not processed
    expect(result.violations).toHaveLength(1);
    expect(result.violations[0].eventIndex).toBe(0);
  });

  it('includes snapshots when includeSnapshots is true', async () => {
    const checker = new InvariantChecker(TestAccountAggregate, {
      eventFactory: accountEventFactory,
      includeSnapshots: true,
    });

    const events: FixtureEvent[] = [balanceEvent(10)];
    const result = await checker.verifyAfterEachEvent(events);

    expect(result.snapshots).toBeDefined();
    // initial state + 1 event = 2 snapshots
    expect(result.snapshots!).toHaveLength(2);
    expect(result.snapshots![0].eventIndex).toBe(-1); // initial
    expect(result.snapshots![1].eventIndex).toBe(0);
    expect(result.snapshots![1].eventType).toBe('BalanceChanged');
    expect(result.snapshots![1].allPass).toBe(true);
  });

  it('does not include snapshots by default', async () => {
    const checker = new InvariantChecker(TestAccountAggregate, {
      eventFactory: accountEventFactory,
    });

    const result = await checker.verifyAfterEachEvent([balanceEvent(10)]);

    expect(result.snapshots).toBeUndefined();
  });

  it('checks additional (non-decorated) invariants', async () => {
    const checker = new InvariantChecker(TestAccountAggregate, {
      eventFactory: accountEventFactory,
      additionalInvariants: [
        {
          description: 'Balance must be under 1000',
          check: (agg) => (agg as TestAccountAggregate).balance < 1000,
        },
      ],
    });

    const events: FixtureEvent[] = [
      balanceEvent(500),
      balanceEvent(600), // total 1100 → additional invariant fails
    ];

    const result = await checker.verifyAfterEachEvent(events);

    expect(result.passed).toBe(false);
    expect(result.invariantCount).toBe(2); // 1 decorated + 1 additional
    const violation = result.violations.find(
      (v) => v.invariantDescription === 'Balance must be under 1000',
    );
    expect(violation).toBeDefined();
    expect(violation!.eventIndex).toBe(1);
  });

  it('addInvariant method appends invariants at runtime', async () => {
    const checker = new InvariantChecker(TestAccountAggregate, {
      eventFactory: accountEventFactory,
    });

    checker.addInvariant(
      'Balance must be even',
      (agg) => (agg as TestAccountAggregate).balance % 2 === 0,
      'warning',
    );

    const events: FixtureEvent[] = [balanceEvent(3)]; // balance = 3 (odd) → warning

    const result = await checker.verifyAfterEachEvent(events);

    // Warnings do not cause result.passed to be false
    expect(result.passed).toBe(true);
    expect(result.violations).toHaveLength(1);
    expect(result.violations[0].severity).toBe('warning');
  });
});
