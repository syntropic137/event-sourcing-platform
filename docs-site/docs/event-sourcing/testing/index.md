# Testing Event-Sourced Applications

Event Sourcing applications require specialized testing patterns that differ from traditional CRUD testing.

## Why Specialized Testing?

In Event Sourcing:
- **State is derived** — You can't just check database rows
- **History matters** — The sequence of events affects final state
- **Commands produce events** — Test that the right events are emitted

## Scenario Testing (Given-When-Then)

The `scenario()` function is the primary tool for testing aggregate command handlers. It implements the Given-When-Then pattern from BDD.

### Installation

**TypeScript:**
```typescript
import { scenario } from '@neuralempowerment/event-sourcing-typescript/testing';
```

**Python:**
```python
from event_sourcing.testing import scenario
```

### Quick Example

**TypeScript:**
```typescript
import { scenario } from '@neuralempowerment/event-sourcing-typescript/testing';

describe('OrderAggregate', () => {
  it('should create order', () => {
    scenario(OrderAggregate)
      .givenNoPriorActivity()
      .when(new CreateOrderCommand('order-123', 'customer-456'))
      .expectEvents([
        new OrderCreatedEvent('order-123', 'customer-456'),
      ]);
  });

  it('should reject duplicate order', () => {
    scenario(OrderAggregate)
      .given([
        new OrderCreatedEvent('order-123', 'customer-456'),
      ])
      .when(new CreateOrderCommand('order-123', 'customer-789'))
      .expectException(Error)
      .expectExceptionMessage('Order already exists');
  });

  it('should track state correctly', () => {
    scenario(OrderAggregate)
      .given([
        new OrderCreatedEvent('order-123', 'customer-456'),
        new ItemAddedEvent('item-1', 2, 10.00),
      ])
      .when(new AddItemCommand('order-123', 'item-2', 1, 25.00))
      .expectState((aggregate) => {
        expect(aggregate.getItemCount()).toBe(3);
        expect(aggregate.getTotal()).toBe(45.00);
      });
  });
});
```

**Python:**
```python
from event_sourcing.testing import scenario

def test_create_order():
    scenario(OrderAggregate) \
        .given_no_prior_activity() \
        .when(CreateOrderCommand(aggregate_id='order-123', customer_id='customer-456')) \
        .expect_events([
            OrderCreatedEvent(order_id='order-123', customer_id='customer-456'),
        ])

def test_reject_duplicate():
    scenario(OrderAggregate) \
        .given([
            OrderCreatedEvent(order_id='order-123', customer_id='customer-456'),
        ]) \
        .when(CreateOrderCommand(aggregate_id='order-123', customer_id='customer-789')) \
        .expect_exception(BusinessRuleViolationError) \
        .expect_exception_message('Order already exists')
```

### API Reference

| Method | Description |
|--------|-------------|
| `scenario(AggregateClass)` | Create scenario for aggregate type |
| `.given([events])` | Set up prior events |
| `.givenNoPriorActivity()` | Start with fresh aggregate |
| `.givenCommands([commands])` | Set up via commands |
| `.when(command)` | Execute command under test |
| `.expectEvents([events])` | Assert events emitted |
| `.expectNoEvents()` | Assert no events emitted |
| `.expectException(ErrorClass)` | Assert exception type |
| `.expectExceptionMessage(msg)` | Assert exception message |
| `.expectState(callback)` | Assert aggregate state |

See [Scenario Testing](./scenario-testing.md) for the complete API reference.

## Best Practices

### Test Every Command Handler

Each command should have:
- Happy path test (events emitted)
- Error path tests (business rules enforced)
- State verification where useful

### Use Real Event Types

```typescript
// ✅ Good - type-safe
.expectEvents([new OrderCreatedEvent('order-1', 'customer-1')])

// ❌ Bad - no type checking
.expectEvents([{ eventType: 'OrderCreated', orderId: 'order-1' }])
```

### Document Business Rules Through Tests

```typescript
describe('Order Business Rules', () => {
  it('should not allow adding items after confirmation', () => { ... });
  it('should not allow confirming with zero items', () => { ... });
  it('should not allow shipping unconfirmed order', () => { ... });
});
```

## Future Enhancements

Potential additions to the ES Test Kit:

- **Replay Testing** — Verify aggregate state from known event sequences (golden file testing)
- **Invariant Testing** — Verify business rules hold after each event
- **Projection Testing** — Test read model correctness and determinism
- **Fixture Loading** — Reusable JSON/YAML test data
- **Event Matchers** — Flexible event comparison (partial matching, wildcards)
