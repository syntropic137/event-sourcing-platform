# Testing Event-Sourced Applications

Event Sourcing applications require specialized testing patterns that differ from traditional CRUD testing. This guide covers the ES Test Kit provided by the platform.

## Why Specialized Testing?

In Event Sourcing:
- **State is derived** — You can't just check database rows
- **History matters** — The sequence of events affects final state
- **Invariants must hold** — Business rules must be true after every event
- **Projections must be deterministic** — Same events = same read model
- **Replay must work** — Aggregates must rehydrate correctly

## ES Test Kit Overview

The platform provides a testing module with three main utilities:

| Utility | Purpose | Use Case |
|---------|---------|----------|
| `ReplayTester` | Golden replay testing | Verify aggregate state from known events |
| `InvariantChecker` | Invariant testing | Verify business rules hold after each event |
| `ProjectionTester` | Projection testing | Verify projection correctness and determinism |

## Installation

The testing utilities are part of the TypeScript SDK:

```typescript
import {
  loadFixture,
  ReplayTester,
  InvariantChecker,
  ProjectionTester,
} from '@event-sourcing-platform/typescript/testing';
```

:::note
Testing utilities are in a separate import path to avoid bundling them in production code.
:::

## Quick Start

### Golden Replay Test

```typescript
import { loadFixture, ReplayTester } from '@event-sourcing-platform/typescript/testing';
import { OrderAggregate } from './OrderAggregate';

describe('OrderAggregate', () => {
  it('calculates total correctly after adding items', async () => {
    const fixture = await loadFixture('./fixtures/order-with-items.json');
    const tester = new ReplayTester(OrderAggregate);
    
    const result = await tester.replayAndAssert(fixture);
    
    expect(result.success).toBe(true);
    expect(result.state.total).toBe(150.00);
  });
});
```

### Invariant Test

```typescript
import { InvariantChecker } from '@event-sourcing-platform/typescript/testing';
import { BankAccountAggregate } from './BankAccountAggregate';

describe('BankAccountAggregate invariants', () => {
  it('balance never goes negative', async () => {
    const checker = new InvariantChecker(BankAccountAggregate);
    const fixture = await loadFixture('./fixtures/account-operations.json');
    
    const result = await checker.verifyAfterEachEvent(fixture.events);
    
    expect(result.passed).toBe(true);
    expect(result.violations).toHaveLength(0);
  });
});
```

### Projection Test

```typescript
import { ProjectionTester } from '@event-sourcing-platform/typescript/testing';
import { OrderSummaryProjection } from './OrderSummaryProjection';

describe('OrderSummaryProjection', () => {
  it('is deterministic', async () => {
    const projection = new OrderSummaryProjection();
    const tester = new ProjectionTester(projection);
    const fixture = await loadFixture('./fixtures/many-orders.json');
    
    const result = await tester.verifyDeterminism(fixture.events);
    
    expect(result.isDeterministic).toBe(true);
  });
});
```

## Test Pyramid for ES

```
                    ┌───────────────────┐
                    │   End-to-End      │
                    │   (Full replay    │
                    │    with infra)    │
                    └─────────┬─────────┘
                              │
              ┌───────────────┴───────────────┐
              │      Projection Tests         │
              │   (Determinism, rebuild)      │
              └───────────────┬───────────────┘
                              │
      ┌───────────────────────┴───────────────────────┐
      │              Invariant Tests                   │
      │   (Business rules hold after each event)      │
      └───────────────────────┬───────────────────────┘
                              │
┌─────────────────────────────┴─────────────────────────────┐
│                    Golden Replay Tests                     │
│        (Known events → expected aggregate state)           │
└───────────────────────────────────────────────────────────┘
```

## Next Steps

- [Golden Replay Testing](./golden-replays.md) — Fixture-based testing in depth
- [Invariant Testing](./invariant-testing.md) — Business rule verification
- [Projection Testing](./projection-testing.md) — Projection correctness
- [Best Practices](./best-practices.md) — Patterns and anti-patterns
