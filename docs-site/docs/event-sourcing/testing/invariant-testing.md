# Invariant Testing

Invariants are business rules that must always be true for an aggregate. Invariant testing verifies these rules hold after every event.

## What is an Invariant?

An invariant is a condition that should never be violated:
- "Balance must never be negative"
- "Closed accounts cannot have pending transactions"
- "Order total must equal sum of item prices"
- "Shipped orders cannot be cancelled"

## Defining Invariants

Use the `@Invariant` decorator on aggregate methods:

```typescript
import { AggregateRoot, Aggregate, EventSourcingHandler } from '@event-sourcing-platform/typescript';
import { Invariant } from '@event-sourcing-platform/typescript/testing';

@Aggregate('BankAccount')
class BankAccountAggregate extends AggregateRoot<BankAccountEvent> {
  private balance: number = 0;
  private isClosed: boolean = false;
  private pendingTransactions: Transaction[] = [];

  @Invariant('balance must never be negative')
  private checkBalanceNonNegative(): boolean {
    return this.balance >= 0;
  }

  @Invariant('closed accounts cannot have pending transactions')
  private checkClosedAccountClean(): boolean {
    return !(this.isClosed && this.pendingTransactions.length > 0);
  }

  @Invariant('balance must be finite', { severity: 'warning' })
  private checkBalanceFinite(): boolean {
    return Number.isFinite(this.balance);
  }

  // ... event handlers
}
```

### Invariant Method Requirements

Invariant methods must:
- Return `boolean` (true = holds, false = violated)
- Take no arguments
- Not modify aggregate state
- Be fast (called after every event)

### Severity Levels

- `'error'` (default): Test fails if violated
- `'warning'`: Logged but test passes

## Using InvariantChecker

### Basic Usage

```typescript
import { InvariantChecker, loadFixture } from '@event-sourcing-platform/typescript/testing';
import { BankAccountAggregate } from './BankAccountAggregate';

const checker = new InvariantChecker(BankAccountAggregate);
const fixture = await loadFixture('./fixtures/account-operations.json');

const result = await checker.verifyAfterEachEvent(fixture.events);

expect(result.passed).toBe(true);
expect(result.violations).toHaveLength(0);
```

### Checking Invariants Manually

```typescript
const aggregate = new BankAccountAggregate();
// ... apply some events

const results = checker.checkInvariants(aggregate);

for (const result of results) {
  console.log(`${result.description}: ${result.holds ? 'OK' : 'VIOLATED'}`);
}
```

### Adding Dynamic Invariants

You can add invariants without decorators:

```typescript
const checker = new InvariantChecker(BankAccountAggregate);

checker.addInvariant(
  'balance should not exceed 1 million',
  (aggregate) => aggregate.balance <= 1_000_000,
  'warning'
);

const result = await checker.verifyAfterEachEvent(fixture.events);
```

### Getting Snapshots

See invariant status after each event:

```typescript
const result = await checker.verifyAfterEachEvent(fixture.events, {
  includeSnapshots: true,
});

for (const snapshot of result.snapshots!) {
  console.log(`After event ${snapshot.eventIndex} (${snapshot.eventType}):`);
  for (const check of snapshot.results) {
    console.log(`  ${check.description}: ${check.holds ? '✓' : '✗'}`);
  }
}
```

## Verification Result

```typescript
interface InvariantVerificationResult {
  // Whether all error-level invariants held
  passed: boolean;

  // Number of invariants checked
  invariantCount: number;

  // Number of events processed
  eventCount: number;

  // All violations found
  violations: InvariantViolation[];

  // Snapshots at each event (if requested)
  snapshots?: InvariantSnapshot[];
}

interface InvariantViolation {
  eventIndex: number;
  eventType: string;
  invariantDescription: string;
  methodName: string;
  severity: 'error' | 'warning';
}
```

## Common Patterns

### Financial Aggregates

```typescript
@Aggregate('Account')
class AccountAggregate extends AggregateRoot<AccountEvent> {
  @Invariant('balance must never be negative')
  private checkBalance(): boolean {
    return this.balance >= 0;
  }

  @Invariant('credits minus debits must equal balance')
  private checkBalanceConsistency(): boolean {
    return this.totalCredits - this.totalDebits === this.balance;
  }
}
```

### State Machine Aggregates

```typescript
@Aggregate('Order')
class OrderAggregate extends AggregateRoot<OrderEvent> {
  @Invariant('shipped orders must have tracking number')
  private checkShippedHasTracking(): boolean {
    return this.status !== 'shipped' || this.trackingNumber !== undefined;
  }

  @Invariant('cancelled orders cannot be modified')
  private checkCancelledFinal(): boolean {
    // This would be checked by command handlers, but invariant ensures consistency
    return this.status !== 'cancelled' || this.lastModified === this.cancelledAt;
  }
}
```

### Collection Aggregates

```typescript
@Aggregate('Cart')
class CartAggregate extends AggregateRoot<CartEvent> {
  @Invariant('item quantities must be positive')
  private checkItemQuantities(): boolean {
    return this.items.every(item => item.quantity > 0);
  }

  @Invariant('total must match sum of items')
  private checkTotalConsistency(): boolean {
    const calculated = this.items.reduce(
      (sum, item) => sum + item.price * item.quantity,
      0
    );
    return Math.abs(this.total - calculated) < 0.01; // Float tolerance
  }
}
```

## Best Practices

### 1. Define Invariants Early

Define invariants when designing the aggregate, not as an afterthought.

### 2. Test Both Valid and Invalid Sequences

Create fixtures that should pass AND fixtures that should violate invariants:

```typescript
describe('BankAccount invariants', () => {
  it('passes for valid operations', async () => {
    const fixture = await loadFixture('./fixtures/valid-operations.json');
    const result = await checker.verifyAfterEachEvent(fixture.events);
    expect(result.passed).toBe(true);
  });

  it('detects overdraft violation', async () => {
    const fixture = await loadFixture('./fixtures/overdraft-attempt.json');
    const result = await checker.verifyAfterEachEvent(fixture.events);
    expect(result.passed).toBe(false);
    expect(result.violations[0].invariantDescription).toContain('negative');
  });
});
```

### 3. Keep Invariants Simple

Each invariant should check one thing:

```typescript
// ✅ Good: Single responsibility
@Invariant('balance must never be negative')
private checkBalance(): boolean {
  return this.balance >= 0;
}

// ❌ Bad: Multiple checks in one invariant
@Invariant('account must be valid')
private checkValid(): boolean {
  return this.balance >= 0 && !this.isClosed && this.owner !== null;
}
```

### 4. Use Warnings for Soft Limits

```typescript
@Invariant('balance should stay under limit', { severity: 'warning' })
private checkSoftLimit(): boolean {
  return this.balance <= this.softLimit;
}

@Invariant('balance must not exceed hard limit', { severity: 'error' })
private checkHardLimit(): boolean {
  return this.balance <= this.hardLimit;
}
```
