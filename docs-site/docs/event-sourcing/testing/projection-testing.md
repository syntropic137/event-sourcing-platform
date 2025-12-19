# Projection Testing

Projections build read models from events. Testing ensures they're correct and deterministic.

## Why Test Projections?

Projections must be:
- **Correct** — Events produce expected read model
- **Deterministic** — Same events always produce same state
- **Rebuild-safe** — Rebuilding from scratch produces same result

## TestableProjection Interface

Your projection must implement this interface for testing:

```typescript
interface TestableProjection<TState = unknown> {
  getName(): string;
  handleEvent(envelope: EventEnvelope): Promise<void> | void;
  getState(): TState;
  reset(): Promise<void> | void;
  getSubscribedEventTypes?(): Set<string> | string[] | null;
}
```

Example implementation:

```typescript
class OrderSummaryProjection implements TestableProjection<OrderSummaryState> {
  private orders = new Map<string, OrderSummary>();

  getName(): string {
    return 'OrderSummaryProjection';
  }

  getSubscribedEventTypes(): string[] {
    return ['OrderCreated', 'OrderShipped', 'OrderCancelled'];
  }

  async handleEvent(envelope: EventEnvelope): Promise<void> {
    const event = envelope.event;
    
    switch (event.eventType) {
      case 'OrderCreated':
        this.orders.set(envelope.metadata.aggregateId, {
          id: envelope.metadata.aggregateId,
          status: 'created',
          total: (event as OrderCreated).total,
        });
        break;
      case 'OrderShipped':
        const order = this.orders.get(envelope.metadata.aggregateId);
        if (order) order.status = 'shipped';
        break;
      // ... more cases
    }
  }

  getState(): OrderSummaryState {
    return {
      orders: Array.from(this.orders.values()),
      count: this.orders.size,
    };
  }

  reset(): void {
    this.orders.clear();
  }
}
```

## Using ProjectionTester

### Basic Processing

```typescript
import { ProjectionTester, loadFixture } from '@event-sourcing-platform/typescript/testing';
import { OrderSummaryProjection } from './OrderSummaryProjection';

const projection = new OrderSummaryProjection();
const tester = new ProjectionTester(projection);

const fixture = await loadFixture('./fixtures/order-events.json');
const result = await tester.processEvents(fixture.events);

expect(result.passed).toBe(true);
expect(result.finalState.count).toBe(5);
```

### Processing with Assertions

```typescript
const result = await tester.processAndAssert(fixture.events, {
  count: 5,
  orders: [
    { id: 'order-1', status: 'shipped' },
    // Partial matching - only checks specified fields
  ],
});

expect(result.passed).toBe(true);
```

### Verifying Determinism

Determinism ensures the same events always produce the same state:

```typescript
const result = await tester.verifyDeterminism(fixture.events, {
  iterations: 5, // Run 5 times and compare
});

expect(result.isDeterministic).toBe(true);

if (!result.isDeterministic) {
  console.log('Differences between iterations:');
  console.log(result.differences);
}
```

### Verifying Rebuild

Rebuild verification ensures resetting and replaying produces the same result:

```typescript
const result = await tester.verifyRebuild(fixture.events);

expect(result.passed).toBe(true);

if (!result.passed) {
  console.log('Original state:', result.originalState);
  console.log('Rebuilt state:', result.rebuiltState);
  console.log('Differences:', result.differences);
}
```

## Common Issues

### Non-Determinism Sources

1. **Timestamps**
   ```typescript
   // ❌ Bad: Uses current time
   this.lastUpdated = new Date();
   
   // ✅ Good: Uses event timestamp
   this.lastUpdated = new Date(envelope.metadata.timestamp);
   ```

2. **Random Values**
   ```typescript
   // ❌ Bad: Random ID
   this.internalId = crypto.randomUUID();
   
   // ✅ Good: Derived from event
   this.internalId = envelope.metadata.eventId;
   ```

3. **External Calls**
   ```typescript
   // ❌ Bad: Fetches external data
   this.exchangeRate = await fetchExchangeRate();
   
   // ✅ Good: Uses data from event
   this.exchangeRate = (event as PriceSet).exchangeRate;
   ```

4. **Unordered Collections**
   ```typescript
   // ❌ Bad: Object keys have no guaranteed order
   for (const key in this.items) { ... }
   
   // ✅ Good: Explicit ordering
   const sortedKeys = Object.keys(this.items).sort();
   for (const key of sortedKeys) { ... }
   ```

### Reset Not Clearing Everything

```typescript
// ❌ Bad: Incomplete reset
reset(): void {
  this.orders.clear();
  // Forgot to reset counters, caches, etc.
}

// ✅ Good: Complete reset
reset(): void {
  this.orders.clear();
  this.totalCount = 0;
  this.cache = new Map();
  this.lastProcessedPosition = 0;
}
```

## Test Patterns

### Test Event Filtering

```typescript
describe('OrderSummaryProjection', () => {
  it('ignores unsubscribed event types', async () => {
    const events = [
      { type: 'OrderCreated', version: 'v1', data: {} },
      { type: 'UserLoggedIn', version: 'v1', data: {} }, // Unrelated
      { type: 'OrderShipped', version: 'v1', data: {} },
    ];

    const result = await tester.processEvents(events);

    // Should only process 2 events (OrderCreated, OrderShipped)
    expect(result.eventsProcessed).toBe(2);
  });
});
```

### Test Idempotency

```typescript
describe('projection idempotency', () => {
  it('handles duplicate events gracefully', async () => {
    const events = [
      { type: 'OrderCreated', version: 'v1', data: { orderId: '123' } },
      { type: 'OrderCreated', version: 'v1', data: { orderId: '123' } }, // Duplicate
    ];

    const result = await tester.processEvents(events);

    // Should have 1 order, not 2
    expect(result.finalState.count).toBe(1);
  });
});
```

### Test Error Handling

```typescript
describe('projection error handling', () => {
  it('handles malformed events', async () => {
    const events = [
      { type: 'OrderCreated', version: 'v1', data: { orderId: '123' } },
      { type: 'OrderCreated', version: 'v1', data: {} }, // Missing orderId
    ];

    const result = await tester.processEvents(events);

    expect(result.errors.length).toBeGreaterThan(0);
    expect(result.errors[0].type).toBe('event_handling');
  });
});
```

## Best Practices

### 1. Test Empty State

```typescript
it('starts with empty state', async () => {
  const result = await tester.processEvents([]);
  expect(result.finalState.count).toBe(0);
  expect(result.finalState.orders).toEqual([]);
});
```

### 2. Test State Transitions

```typescript
it('tracks order status changes', async () => {
  const events = [
    { type: 'OrderCreated', ... },
    { type: 'OrderPaid', ... },
    { type: 'OrderShipped', ... },
  ];

  const result = await tester.processEvents(events);
  
  expect(result.finalState.orders[0].status).toBe('shipped');
  expect(result.finalState.orders[0].statusHistory).toEqual([
    'created', 'paid', 'shipped'
  ]);
});
```

### 3. Run Determinism in CI

```typescript
// In your CI test suite
it('projection is deterministic (CI)', async () => {
  const result = await tester.verifyDeterminism(events, {
    iterations: 10, // More iterations in CI
  });
  expect(result.isDeterministic).toBe(true);
}, 30000); // Longer timeout for CI
```

### 4. Test with Production Event Samples

If possible, test with anonymized production events:

```typescript
const productionFixtures = await loadFixturesFromDirectory(
  './fixtures/production-samples'
);

for (const fixture of productionFixtures) {
  it(`handles ${fixture.description}`, async () => {
    const result = await tester.processEvents(fixture.events);
    expect(result.passed).toBe(true);
  });
}
```
