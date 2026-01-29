# ADR-015: ES Test Kit Architecture

**Status:** ✅ Accepted  
**Date:** 2025-12-19 (Updated: 2026-01-28)  
**Decision Makers:** Platform Team  
**Related:** ADR-004 (Command Handlers), ADR-014 (Projection Checkpoints)

## Context

### Why ES Applications Need Specialized Testing

Event Sourcing applications have fundamentally different testing needs than traditional CRUD apps:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                 CRUD vs EVENT SOURCING TESTING                          │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  CRUD Application              Event Sourcing Application               │
│  ─────────────────             ──────────────────────────               │
│                                                                         │
│  ✓ Check database rows         ✗ State is derived from events          │
│  ✓ Mock repositories           ✗ History sequence matters              │
│  ✓ Simple state assertions     ✗ Invariants must hold always           │
│  ✓ Unit test services          ✗ Must test command → event flow        │
│  ✓ Integration test APIs       ✗ Must verify projections are correct   │
│                                                                         │
│  Testing Focus:                Testing Focus:                           │
│  • Does it save correctly?     • Does this command produce right events?│
│  • Does it query correctly?    • Does replay produce correct state?     │
│                                • Do invariants hold after every event?  │
│                                • Are projections deterministic?         │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

### The Problem

Event Sourcing applications require specialized testing patterns that differ from traditional CRUD testing:

1. **State is derived** — You can't just check database rows
2. **History matters** — The sequence of events affects final state
3. **Invariants must hold** — Business rules must be true after every event
4. **Projections must be deterministic** — Same events = same read model
5. **Replay must work** — Aggregates must rehydrate correctly
6. **Command behavior matters** — Commands must produce correct events

Currently, the platform provides:
- Basic Jest tests in examples
- `MemoryEventStoreClient` for in-memory testing
- Examples that serve as living documentation

What's missing:
- **Given-When-Then testing harness** for command-focused aggregate testing
- Reusable testing harness for common patterns
- Golden replay fixtures (known events → expected state)
- Invariant testing framework
- Projection determinism verification

### Requirements

1. **Given-When-Then Testing** — Test aggregates with: given events → when command → expect events/exception
2. **Golden Replays** — Load events from fixtures, replay, assert state
3. **Invariant Testing** — Define aggregate invariants, verify after each event
4. **Property Testing** — Generate random command sequences, verify invariants hold
5. **Projection Testing** — Verify projections are deterministic across rebuilds
6. **Minimal Boilerplate** — Tests should be concise and readable
7. **Framework Agnostic** — Work with Jest, Vitest, pytest, Mocha, etc.
8. **Multi-Language** — Consistent API across TypeScript and Python

## Decision

We will create a **test kit module** that provides:

### 1. Given-When-Then Scenario Testing (NEW)

The `scenario()` function provides a fluent API for testing aggregate behavior using the **Given-When-Then** pattern, inspired by Axon Framework's `AggregateTestFixture`.

**Design Principles:**
- **Readability First** — Tests are "written by AI, read by humans"
- **Fluent API** — Chainable methods with good IDE autocomplete
- **Formatter-Friendly** — Structure creates readability (no reliance on blank lines)
- **Explicit Errors** — Typed exception expectations

#### TypeScript

```typescript
import { scenario } from '@neurale/event-sourcing/testing';

// Happy path: command produces events
scenario(OrderAggregate)
  .given([
    new CartCreatedEvent('order-1'),
    new ItemAddedEvent('order-1', 'item-1', 29.99),
  ])
  .when(new SubmitCartCommand('order-1'))
  .expectEvents([
    new CartSubmittedEvent('order-1', 29.99),
  ]);

// Error path: business rule violation
scenario(OrderAggregate)
  .givenNoPriorActivity()
  .when(new SubmitCartCommand('order-1'))
  .expectException(BusinessRuleViolationError)
  .expectExceptionMessage('Cannot submit empty cart');

// Verify aggregate state after command
scenario(OrderAggregate)
  .given([
    new CartCreatedEvent('order-1'),
  ])
  .when(new AddItemCommand('order-1', 'item-1', 29.99))
  .expectState((state) => {
    expect(state.itemCount).toBe(1);
  });
```

#### Python

```python
from event_sourcing.testing import scenario

# Happy path: command produces events
scenario(OrderAggregate) \
    .given([
        CartCreatedEvent(aggregate_id='order-1'),
        ItemAddedEvent(aggregate_id='order-1', item_id='item-1', price=29.99),
    ]) \
    .when(SubmitCartCommand(aggregate_id='order-1')) \
    .expect_events([
        CartSubmittedEvent(aggregate_id='order-1', total=29.99),
    ])

# Error path: business rule violation
scenario(OrderAggregate) \
    .given_no_prior_activity() \
    .when(SubmitCartCommand(aggregate_id='order-1')) \
    .expect_exception(BusinessRuleViolationError) \
    .expect_exception_message('Cannot submit empty cart')
```

#### Scenario API Reference

| Phase | TypeScript | Python | Description |
|-------|------------|--------|-------------|
| Setup | `scenario(AggregateClass)` | `scenario(AggregateClass)` | Create test scenario |
| Given | `.given([events])` | `.given([events])` | Set up prior events |
| Given | `.givenNoPriorActivity()` | `.given_no_prior_activity()` | No prior events |
| Given | `.givenCommands([cmds])` | `.given_commands([cmds])` | Generate events from commands |
| When | `.when(command)` | `.when(command)` | Execute command |
| Then | `.expectEvents([events])` | `.expect_events([events])` | Assert events emitted |
| Then | `.expectNoEvents()` | `.expect_no_events()` | Assert no events |
| Then | `.expectException(Error)` | `.expect_exception(Error)` | Assert exception type |
| Then | `.expectExceptionMessage(msg)` | `.expect_exception_message(msg)` | Assert exception message |
| Then | `.expectState(callback)` | `.expect_state(callback)` | Assert aggregate state |
| Config | `.registerInjectableResource(r)` | `.register_injectable_resource(r)` | Inject dependencies |

### 2. Golden Replay Testing

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

## ES Test Kit - Complete Toolkit Overview

The ES Test Kit is a **cohesive collection of testing tools** designed specifically for event-sourced applications. Each tool has a specific purpose and answers a specific testing question.

### Tool Purpose Matrix

| Tool | What It Does | Testing Question | When to Use |
|------|--------------|------------------|-------------|
| **`scenario()`** | Given-When-Then command testing | "Does this command produce the right events?" | Every command handler |
| **`ReplayTester`** | Replay events and verify state | "Does replaying events produce correct state?" | Regression tests, migrations |
| **`InvariantChecker`** | Verify business rules after events | "Do invariants hold after every event?" | Critical business rules |
| **`PropertyTester`** | Random command sequences | "Do invariants hold for ANY command sequence?" | Complex aggregates |
| **`ProjectionTester`** | Test projection event handling | "Does the projection produce correct read model?" | Every projection |
| **`loadFixture()`** | Load test data from files | N/A (utility) | Shared test scenarios |

### Testing Strategy by Concern

```
┌─────────────────────────────────────────────────────────────────────────┐
│                     WHAT ARE YOU TESTING?                               │
└─────────────────────────────────────────────────────────────────────────┘
                                │
        ┌───────────────────────┼───────────────────────┐
        │                       │                       │
        ▼                       ▼                       ▼
┌───────────────┐      ┌───────────────┐      ┌───────────────┐
│   COMMANDS    │      │    STATE      │      │  PROJECTIONS  │
│   (Behavior)  │      │   (History)   │      │ (Read Models) │
└───────┬───────┘      └───────┬───────┘      └───────┬───────┘
        │                      │                      │
        ▼                      ▼                      ▼
   scenario()            ReplayTester          ProjectionTester
   InvariantChecker      PropertyTester
```

### Test Pyramid for ES Applications

```
                    ┌─────────┐
                   ╱ Property  ╲        ← Few (slow, find edge cases)
                  ╱   Tests    ╲
                 ╱──────────────╲
                ╱ Invariant     ╲       ← Some (verify business rules)
               ╱   Tests         ╲
              ╱───────────────────╲
             ╱ Projection Tests    ╲    ← Some (verify read models)
            ╱───────────────────────╲
           ╱  scenario() Tests       ╲  ← Many (fast, every command)
          ╱   (Given-When-Then)       ╲
         ╱─────────────────────────────╲
        ╱  Replay Tests (Regression)    ╲ ← Some (catch regressions)
       ╱─────────────────────────────────╲
```

## Architecture

### TypeScript Module Structure

```
event-sourcing/typescript/src/testing/
├── index.ts                    # Public exports
├── scenario/                   # NEW: Given-When-Then testing
│   ├── index.ts                # Exports
│   ├── aggregate-scenario.ts   # Main scenario class
│   ├── test-executor.ts        # When phase (execute command)
│   ├── result-validator.ts     # Then phase (assertions)
│   └── errors/                 # Scenario-specific errors
│       └── scenario-errors.ts
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

### Python Module Structure

```
event-sourcing/python/src/event_sourcing/testing/
├── __init__.py                 # Public exports
├── scenario/                   # Given-When-Then testing
│   ├── __init__.py
│   ├── aggregate_scenario.py   # Main scenario class
│   ├── test_executor.py        # When phase
│   ├── result_validator.py     # Then phase
│   └── matchers/
│       └── event_matchers.py
├── fixtures/
│   └── test_fixture.py         # Load/save fixtures
├── replay/
│   └── replay_tester.py        # Core replay logic
└── invariants/
    └── invariant_checker.py    # Runtime verification
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

### Phase 0: Given-When-Then Scenario Testing (Priority) ✅
- [x] TypeScript `scenario()` implementation
- [x] TypeScript `AggregateScenario`, `TestExecutor`, `ResultValidator` classes
- [ ] TypeScript event matchers (future enhancement)
- [x] Python `scenario()` implementation  
- [x] Python `AggregateScenario`, `TestExecutor`, `ResultValidator` classes
- [ ] Python event matchers (future enhancement)
- [x] Tests for both implementations
- [ ] Documentation and examples

### Phase 1: Core Replay Testing
- [x] `loadFixture()` and fixture types ✅
- [x] `ReplayTester` with state extraction ✅
- [x] Basic assertions (deep equality) ✅

### Phase 2: Invariant Testing
- [x] `@Invariant` decorator ✅
- [x] `InvariantChecker` runtime verification ✅
- [ ] Integration with replay testing

### Phase 3: Property Testing
- [ ] `PropertyTester` with fast-check integration
- [ ] Command generators
- [ ] Shrinking for minimal failing cases

### Phase 4: Projection Testing
- [x] `ProjectionTester` harness ✅
- [ ] Determinism verification
- [ ] Rebuild testing

### Phase 5: Documentation
- [ ] Testing guide in docs-site
- [ ] Examples for each pattern
- [ ] Best practices

## References

- [Axon Framework AggregateTestFixture](https://docs.axoniq.io/axon-framework-reference/4.12/testing/commands-events/) — Inspiration for Given-When-Then pattern
- [Reference: eventsourcing-book](../../reference/eventsourcing-book/) — Kotlin examples using Axon
- [Property-Based Testing with fast-check](https://github.com/dubzzz/fast-check)
- [Event Sourcing Testing Patterns](https://www.eventstore.com/blog/testing-event-sourced-systems)
- [ADR-004: Command Handlers in Aggregates](./ADR-004-command-handlers-in-aggregates.md)

---

**Last Updated:** 2026-01-28
