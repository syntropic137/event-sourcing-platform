# ADR-015: ES Test Kit Architecture

**Status:** 📋 Proposed  
**Date:** 2025-12-19  
**Decision Makers:** Platform Team  
**Related:** ADR-004 (Command Handlers), ADR-014 (Projection Checkpoints)

## Context

### The Problem

Event Sourcing applications require specialized testing patterns that differ from traditional CRUD testing:

1. **State is derived** — You can't just check database rows
2. **History matters** — The sequence of events affects final state
3. **Invariants must hold** — Business rules must be true after every event
4. **Projections must be deterministic** — Same events = same read model
5. **Replay must work** — Aggregates must rehydrate correctly

Currently, the platform provides:
- Basic Jest tests in examples
- `MemoryEventStoreClient` for in-memory testing
- Examples that serve as living documentation

What's missing:
- Reusable testing harness for common patterns
- Golden replay fixtures (known events → expected state)
- Invariant testing framework
- Projection determinism verification

### Requirements

1. **Golden Replays** — Load events from fixtures, replay, assert state
2. **Invariant Testing** — Define aggregate invariants, verify after each event
3. **Property Testing** — Generate random command sequences, verify invariants hold
4. **Projection Testing** — Verify projections are deterministic across rebuilds
5. **Minimal Boilerplate** — Tests should be concise and readable
6. **Framework Agnostic** — Work with Jest, Vitest, Mocha, etc.

## Decision

We will create a **test kit module** that provides:

### 1. Golden Replay Testing

```typescript
import { ReplayTester, loadFixture } from '@event-sourcing-platform/typescript/testing';

describe('OrderAggregate', () => {
  it('calculates total correctly', async () => {
    const fixture = await loadFixture('./fixtures/order-with-items.json');
    const tester = new ReplayTester(OrderAggregate);
    
    const result = await tester.replay(fixture.events);
    
    expect(result.state.total).toBe(150.00);
    expect(result.state.itemCount).toBe(3);
  });
});
```

**Fixture format:**
```json
{
  "description": "Order with 3 items",
  "aggregateType": "Order",
  "events": [
    {
      "type": "OrderCreated",
      "version": "v1",
      "data": { "orderId": "order-123", "customerId": "cust-456" }
    },
    {
      "type": "ItemAdded",
      "version": "v1", 
      "data": { "productId": "prod-1", "price": 50.00 }
    }
  ],
  "expectedState": {
    "total": 150.00,
    "itemCount": 3
  }
}
```

### 2. Invariant Testing

```typescript
import { InvariantChecker, Invariant } from '@event-sourcing-platform/typescript/testing';

@Aggregate('BankAccount')
class BankAccountAggregate extends AggregateRoot<BankAccountEvent> {
  private balance: number = 0;
  
  @Invariant('balance must never be negative')
  checkBalance(): boolean {
    return this.balance >= 0;
  }
}

describe('BankAccountAggregate invariants', () => {
  it('balance never goes negative', async () => {
    const checker = new InvariantChecker(BankAccountAggregate);
    const fixture = await loadFixture('./fixtures/account-operations.json');
    
    const result = await checker.verifyAfterEachEvent(fixture.events);
    
    expect(result.violations).toHaveLength(0);
  });
});
```

### 3. Property Testing Integration

```typescript
import { PropertyTester } from '@event-sourcing-platform/typescript/testing';
import * as fc from 'fast-check';

describe('BankAccountAggregate properties', () => {
  it('invariants hold for random command sequences', async () => {
    const tester = new PropertyTester(BankAccountAggregate);
    
    const result = await tester.checkInvariants({
      commandGenerators: {
        OpenAccount: fc.record({ initialBalance: fc.nat() }),
        Deposit: fc.record({ amount: fc.nat() }),
        Withdraw: fc.record({ amount: fc.nat() }),
      },
      numRuns: 100,
    });
    
    expect(result.passed).toBe(true);
  });
});
```

### 4. Projection Testing

```typescript
import { ProjectionTester } from '@event-sourcing-platform/typescript/testing';

describe('OrderSummaryProjection', () => {
  it('is deterministic', async () => {
    const tester = new ProjectionTester(OrderSummaryProjection);
    const events = await loadFixture('./fixtures/many-orders.json');
    
    const result = await tester.verifyDeterminism({
      events: events.events,
      iterations: 3,
    });
    
    expect(result.isDeterministic).toBe(true);
  });
  
  it('handles rebuild correctly', async () => {
    const tester = new ProjectionTester(OrderSummaryProjection);
    
    // Process events
    await tester.process(events1);
    const stateAfterInitial = tester.getState();
    
    // Reset and replay
    await tester.reset();
    await tester.process(events1);
    const stateAfterRebuild = tester.getState();
    
    expect(stateAfterRebuild).toEqual(stateAfterInitial);
  });
});
```

## Architecture

### Module Structure

```
event-sourcing/typescript/src/testing/
├── index.ts                    # Public exports
├── fixtures/
│   ├── test-fixture.ts         # Load/save fixtures
│   ├── fixture-types.ts        # TypeScript types for fixtures
│   └── fixture-generator.ts    # Record events during tests
├── replay/
│   ├── replay-tester.ts        # Core replay logic
│   └── state-assertions.ts     # Deep equality, partial matching
├── invariants/
│   ├── invariant-decorator.ts  # @Invariant decorator
│   ├── invariant-checker.ts    # Runtime verification
│   └── property-testing.ts     # fast-check integration
└── projections/
    ├── projection-tester.ts    # Projection test harness
    └── determinism-checker.ts  # Verify same events = same state
```

### Key Design Choices

#### 1. Framework Agnostic

The test kit provides utilities, not a test runner. It works with any testing framework:

```typescript
// Works with Jest
expect(result.passed).toBe(true);

// Works with Vitest
expect(result.passed).toBe(true);

// Works with Node assert
assert.strictEqual(result.passed, true);
```

#### 2. Fixtures are JSON/YAML

Fixtures are data files, not code. This enables:
- Version control of test data
- Sharing fixtures across tests
- Generating fixtures from production events (anonymized)
- Non-developers can create test cases

#### 3. Invariant Decorator is Optional

Invariants can be defined via decorator OR passed to the checker:

```typescript
// Via decorator (preferred)
@Invariant('balance >= 0')
checkBalance(): boolean { return this.balance >= 0; }

// Via checker (for testing external aggregates)
checker.addInvariant('balance >= 0', (agg) => agg.balance >= 0);
```

#### 4. Property Testing is Opt-In

Property testing requires `fast-check` as a peer dependency. Basic testing works without it.

## Consequences

### Positive

1. **Consistent Testing Patterns** ✅
   - All ES tests follow the same structure
   - New team members learn one approach

2. **Catch Regressions** ✅
   - Golden replays detect when behavior changes
   - Invariant checks catch business rule violations

3. **Confidence in Refactoring** ✅
   - Property tests explore edge cases humans miss
   - Projection tests verify rebuild correctness

4. **Documentation Through Tests** ✅
   - Fixtures serve as examples of valid event sequences
   - Tests document expected behavior

### Negative

1. **Learning Curve** ⚠️
   - New concepts to learn (invariants, property testing)
   - **Mitigation:** Comprehensive docs, simple examples first

2. **Fixture Maintenance** ⚠️
   - Fixtures must be updated when event schemas change
   - **Mitigation:** Upcasters apply to fixtures too

3. **Test Performance** ⚠️
   - Property tests can be slow (many iterations)
   - **Mitigation:** Configure iteration count, run in CI only

## Alternatives Considered

### 1. Use Existing Libraries

**Rejected:** No library specifically targets ES patterns. Building on top of generic tools adds friction.

### 2. Test Runner Integration

**Rejected:** Coupling to Jest/Vitest limits flexibility. Utilities work everywhere.

### 3. Code-Based Fixtures

**Rejected:** Data files are more portable and can be generated from production.

## Implementation Plan

### Phase 1: Core Replay Testing
- [ ] `loadFixture()` and fixture types
- [ ] `ReplayTester` with state extraction
- [ ] Basic assertions (deep equality)

### Phase 2: Invariant Testing
- [ ] `@Invariant` decorator
- [ ] `InvariantChecker` runtime verification
- [ ] Integration with replay testing

### Phase 3: Property Testing
- [ ] `PropertyTester` with fast-check integration
- [ ] Command generators
- [ ] Shrinking for minimal failing cases

### Phase 4: Projection Testing
- [ ] `ProjectionTester` harness
- [ ] Determinism verification
- [ ] Rebuild testing

### Phase 5: Documentation
- [ ] Testing guide in docs-site
- [ ] Examples for each pattern
- [ ] Best practices

## References

- [Property-Based Testing with fast-check](https://github.com/dubzzz/fast-check)
- [Event Sourcing Testing Patterns](https://www.eventstore.com/blog/testing-event-sourced-systems)
- [ADR-004: Command Handlers in Aggregates](./ADR-004-command-handlers-in-aggregates.md)

---

**Last Updated:** 2025-12-19
