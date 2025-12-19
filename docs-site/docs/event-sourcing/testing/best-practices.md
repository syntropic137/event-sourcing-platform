# Testing Best Practices

Patterns and anti-patterns for testing Event Sourcing applications.

## General Principles

### 1. Test Behavior, Not Implementation

```typescript
// ✅ Good: Tests business behavior
it('calculates order total correctly', async () => {
  const fixture = await loadFixture('./fixtures/order-with-discount.json');
  const result = await tester.replayAndAssert(fixture, {
    total: 85.00, // 100 - 15% discount
  });
  expect(result.success).toBe(true);
});

// ❌ Bad: Tests internal state
it('sets _discountApplied to true', async () => {
  // Testing private implementation details
});
```

### 2. Use Realistic Fixtures

```typescript
// ✅ Good: Realistic data
{
  "events": [
    {
      "type": "OrderCreated",
      "data": {
        "orderId": "ord_2024_00123",
        "customerId": "cust_456",
        "items": [
          { "productId": "SKU-1234", "name": "Widget Pro", "price": 49.99 }
        ]
      }
    }
  ]
}

// ❌ Bad: Placeholder data
{
  "events": [
    {
      "type": "OrderCreated",
      "data": {
        "orderId": "test",
        "customerId": "test",
        "items": [{ "productId": "x", "price": 1 }]
      }
    }
  ]
}
```

### 3. Test Edge Cases

Create fixtures for:
- Empty aggregates
- Maximum sizes
- Boundary conditions
- Error scenarios
- Concurrent modifications

```
fixtures/
├── order-empty.json
├── order-max-items.json
├── order-zero-total.json
├── order-negative-adjustment.json
└── order-concurrent-updates.json
```

## Aggregate Testing Patterns

### Pattern: State Machine Coverage

```typescript
describe('Order state machine', () => {
  const states = ['draft', 'submitted', 'paid', 'shipped', 'delivered', 'cancelled'];
  
  for (const fromState of states) {
    for (const toState of states) {
      if (isValidTransition(fromState, toState)) {
        it(`transitions from ${fromState} to ${toState}`, async () => {
          const fixture = await loadFixture(`./fixtures/order-${fromState}-to-${toState}.json`);
          const result = await tester.replayAndAssert(fixture);
          expect(result.success).toBe(true);
        });
      }
    }
  }
});
```

### Pattern: Invariant Matrix

```typescript
describe('Account invariants under all operations', () => {
  const operations = ['deposit', 'withdraw', 'transfer', 'close'];
  
  for (const op of operations) {
    it(`maintains invariants after ${op}`, async () => {
      const fixture = await loadFixture(`./fixtures/account-${op}.json`);
      const result = await checker.verifyAfterEachEvent(fixture.events);
      expect(result.passed).toBe(true);
    });
  }
});
```

### Pattern: Error Case Testing

```typescript
describe('Order error handling', () => {
  it('rejects adding items to shipped order', async () => {
    const aggregate = new OrderAggregate();
    // Replay to shipped state
    await tester.replay(shippedOrderFixture.events);
    
    expect(() => {
      aggregate.addItem({ productId: 'new-item', price: 10 });
    }).toThrow('Cannot modify shipped order');
  });
});
```

## Projection Testing Patterns

### Pattern: Subscription Coverage

```typescript
describe('Projection handles all subscribed events', () => {
  const projection = new OrderSummaryProjection();
  const subscribedTypes = projection.getSubscribedEventTypes();
  
  for (const eventType of subscribedTypes) {
    it(`handles ${eventType} events`, async () => {
      const fixture = await loadFixture(`./fixtures/event-${eventType}.json`);
      const result = await tester.processEvents(fixture.events);
      expect(result.errors).toHaveLength(0);
    });
  }
});
```

### Pattern: Cross-Aggregate Projection

```typescript
describe('Dashboard projection (multiple aggregates)', () => {
  it('aggregates data from orders and customers', async () => {
    const events = [
      { type: 'CustomerRegistered', data: { customerId: 'c1' } },
      { type: 'OrderCreated', data: { orderId: 'o1', customerId: 'c1' } },
      { type: 'OrderShipped', data: { orderId: 'o1' } },
    ];
    
    const result = await tester.processEvents(events);
    
    expect(result.finalState).toEqual({
      customers: [{ id: 'c1', orderCount: 1 }],
      orders: [{ id: 'o1', status: 'shipped' }],
    });
  });
});
```

## Anti-Patterns to Avoid

### Anti-Pattern: Testing Decorators Directly

```typescript
// ❌ Bad: Tests decorator implementation
it('stores metadata on class', () => {
  expect(OrderAggregate[INVARIANT_METADATA]).toBeDefined();
});

// ✅ Good: Tests behavior
it('detects invariant violations', async () => {
  const result = await checker.verifyAfterEachEvent(invalidFixture.events);
  expect(result.violations.length).toBeGreaterThan(0);
});
```

### Anti-Pattern: Mocking Event Store

```typescript
// ❌ Bad: Mocks infrastructure
const mockEventStore = {
  append: jest.fn(),
  read: jest.fn().mockResolvedValue([]),
};

// ✅ Good: Uses MemoryEventStoreClient
const eventStore = new MemoryEventStoreClient();
```

### Anti-Pattern: Hardcoded Expected State

```typescript
// ❌ Bad: Hardcoded values that duplicate logic
expect(result.state.total).toBe(100 * 0.9 * 1.1); // 90% then 10% tax

// ✅ Good: Clear expectations from fixture
const fixture = await loadFixture('./fixtures/order-with-discount-and-tax.json');
expect(result.state.total).toBe(fixture.expectedState.total);
```

### Anti-Pattern: Ignoring Event Order

```typescript
// ❌ Bad: Order-independent assertions
expect(result.state.items).toContain('item-1');
expect(result.state.items).toContain('item-2');

// ✅ Good: Order matters in ES
expect(result.state.items).toEqual(['item-1', 'item-2']);
expect(result.state.lastAddedItem).toBe('item-2');
```

## Fixture Management

### Organizing Fixtures

```
fixtures/
├── aggregates/
│   ├── order/
│   │   ├── lifecycle/
│   │   │   ├── happy-path.json
│   │   │   └── with-cancellation.json
│   │   └── edge-cases/
│   │       ├── empty-order.json
│   │       └── max-items.json
│   └── account/
│       └── ...
├── projections/
│   ├── order-summary/
│   │   └── ...
│   └── analytics/
│       └── ...
└── integration/
    └── end-to-end/
        └── ...
```

### Fixture Versioning

When events evolve:

```
fixtures/
├── v1/
│   └── order-lifecycle.json  # Uses v1 events
└── v2/
    └── order-lifecycle.json  # Uses v2 events with new fields
```

### Generating Fixtures from Production

```typescript
// Script to generate fixtures from production events (anonymized)
async function generateFixtures(aggregateId: string) {
  const events = await eventStore.readStream(aggregateId);
  
  const anonymized = events.map(e => ({
    type: e.eventType,
    version: `v${e.schemaVersion}`,
    data: anonymizeData(e.data),
  }));
  
  await saveFixture(
    createFixture({
      description: `Generated from ${aggregateId}`,
      aggregateType: events[0]?.aggregateType,
      events: anonymized,
    }),
    `./fixtures/generated/${aggregateId}.json`
  );
}
```

## CI/CD Integration

### Run Full Test Suite

```yaml
# GitHub Actions example
test:
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v3
    - run: npm ci
    - run: npm test
    
    # ES-specific tests with more iterations
    - run: npm run test:determinism -- --iterations=10
    - run: npm run test:replay -- --all-fixtures
```

### Fixture Validation

```yaml
# Validate fixtures on PR
validate-fixtures:
  runs-on: ubuntu-latest
  steps:
    - run: npm run validate-fixtures
```

### Performance Baseline

```typescript
describe('Performance baselines', () => {
  it('replays 1000 events in under 1 second', async () => {
    const start = Date.now();
    await tester.replay(largeFixture.events);
    const duration = Date.now() - start;
    
    expect(duration).toBeLessThan(1000);
  });
});
```
