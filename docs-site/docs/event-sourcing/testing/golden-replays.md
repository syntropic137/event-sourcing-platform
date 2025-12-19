# Golden Replay Testing

Golden replay testing verifies that known event sequences produce expected aggregate states. It's the foundation of ES testing.

## What is a Golden Replay?

A "golden" test fixture contains:
1. A sequence of events (the input)
2. The expected aggregate state (the output)

The test replays events against the aggregate and verifies the final state matches expectations.

## Fixture Format

Fixtures are JSON (or YAML) files:

```json
{
  "description": "Order with 3 items totaling $150",
  "aggregateType": "Order",
  "aggregateId": "order-123",
  "events": [
    {
      "type": "OrderCreated",
      "version": "v1",
      "data": {
        "orderId": "order-123",
        "customerId": "cust-456"
      }
    },
    {
      "type": "ItemAdded",
      "version": "v1",
      "data": {
        "productId": "prod-1",
        "name": "Widget",
        "price": 50.00,
        "quantity": 1
      }
    },
    {
      "type": "ItemAdded",
      "version": "v1",
      "data": {
        "productId": "prod-2",
        "name": "Gadget",
        "price": 100.00,
        "quantity": 1
      }
    }
  ],
  "expectedState": {
    "status": "draft",
    "total": 150.00,
    "itemCount": 2
  },
  "expectedVersion": 3,
  "tags": ["order", "happy-path"]
}
```

## Loading Fixtures

```typescript
import { loadFixture, loadFixturesFromDirectory, loadFixturesByTags } from '@event-sourcing-platform/typescript/testing';

// Load a single fixture
const fixture = await loadFixture('./fixtures/order-lifecycle.json');

// Load all fixtures from a directory
const allFixtures = await loadFixturesFromDirectory('./fixtures');

// Load fixtures by tags
const orderFixtures = await loadFixturesByTags('./fixtures', ['order']);
```

## Using ReplayTester

### Basic Replay

```typescript
import { ReplayTester, loadFixture } from '@event-sourcing-platform/typescript/testing';
import { OrderAggregate } from './OrderAggregate';

const fixture = await loadFixture('./fixtures/order-with-items.json');
const tester = new ReplayTester(OrderAggregate);

const result = await tester.replay(fixture.events);

console.log(result.state);    // The aggregate's state after replay
console.log(result.version);  // The aggregate's version
console.log(result.success);  // Whether replay succeeded
```

### Replay with Assertions

```typescript
const result = await tester.replayAndAssert(fixture);

if (!result.success) {
  console.log('Failures:');
  console.log(result.errors);
  console.log(result.stateComparison?.differences);
}

expect(result.success).toBe(true);
```

### Step-by-Step Replay

For debugging, you can see the state after each event:

```typescript
const steps = await tester.replayStepByStep(fixture.events);

for (const step of steps) {
  console.log(`After event ${step.eventIndex} (${step.event.type}):`);
  console.log(step.state);
}
```

## Custom Event Factory

If your events need special construction:

```typescript
import { ReplayTester, EventFactory } from '@event-sourcing-platform/typescript/testing';

const customFactory: EventFactory = (fixtureEvent) => {
  // Create your domain event from fixture data
  switch (fixtureEvent.type) {
    case 'OrderCreated':
      return new OrderCreatedEvent(fixtureEvent.data);
    case 'ItemAdded':
      return new ItemAddedEvent(fixtureEvent.data);
    default:
      throw new Error(`Unknown event type: ${fixtureEvent.type}`);
  }
};

const tester = new ReplayTester(OrderAggregate, {
  eventFactory: customFactory,
});
```

## Custom State Extraction

If your aggregate doesn't have a `getState()` method:

```typescript
const tester = new ReplayTester(OrderAggregate, {
  stateExtractor: (aggregate) => ({
    status: aggregate.status,
    total: aggregate.total,
    itemCount: aggregate.items.length,
  }),
});
```

## Generating Fixtures

You can generate fixtures from test runs:

```typescript
import { saveFixture, createFixture } from '@event-sourcing-platform/typescript/testing';

// During a test or from production events
const events = [
  { type: 'OrderCreated', version: 'v1', data: { orderId: '123' } },
  // ... more events
];

const fixture = createFixture({
  description: 'Generated from production order-123',
  aggregateType: 'Order',
  events,
  expectedState: { status: 'completed' },
  tags: ['generated', 'production'],
});

await saveFixture(fixture, './fixtures/order-123.json');
```

## Best Practices

### 1. Name Fixtures Descriptively

```
fixtures/
├── order-empty.json
├── order-single-item.json
├── order-multiple-items.json
├── order-cancelled.json
└── order-completed-lifecycle.json
```

### 2. Use Tags for Organization

```json
{
  "tags": ["order", "happy-path", "v2-events"]
}
```

Then filter in tests:
```typescript
const happyPathFixtures = await loadFixturesByTags('./fixtures', ['happy-path']);
```

### 3. Test Edge Cases

Create fixtures for:
- Empty aggregates
- Maximum capacity
- Error conditions
- State machine transitions

### 4. Version Your Fixtures

When event schemas change, update fixtures or create new ones:
```
fixtures/
├── v1/
│   └── order-lifecycle.json
└── v2/
    └── order-lifecycle.json
```

### 5. Partial State Matching

`expectedState` uses partial matching — only specified fields are checked:

```json
{
  "expectedState": {
    "status": "shipped"
  }
}
```

This passes even if the aggregate has other fields like `total`, `items`, etc.
