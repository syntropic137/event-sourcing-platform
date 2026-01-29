# Scenario Testing (Given-When-Then)

The `scenario()` function is the primary tool for testing aggregate command handlers. It implements the classic **Given-When-Then** pattern from Behavior-Driven Development (BDD), adapted specifically for Event Sourcing.

## Mental Model

```
┌──────────────────────────────────────────────────────────────────┐
│                    scenario(OrderAggregate)                       │
│                                                                   │
│  GIVEN                    WHEN                   THEN             │
│  ─────                    ────                   ────             │
│                                                                   │
│  "Past events that        "Command to           "Expected         │
│   already happened"        execute now"          outcomes"        │
│                                                                   │
│  ┌─────────────────┐     ┌─────────────┐      ┌──────────────┐   │
│  │ OrderCreated    │     │ AddItem     │      │ Events?      │   │
│  │ ItemAdded       │ ──▶ │ Command     │ ──▶  │ Exception?   │   │
│  │ ItemAdded       │     │             │      │ State?       │   │
│  └─────────────────┘     └─────────────┘      └──────────────┘   │
│                                                                   │
└──────────────────────────────────────────────────────────────────┘
```

## Basic Usage

### TypeScript

```typescript
import { scenario } from '@neuralempowerment/event-sourcing-typescript/testing';

describe('OrderAggregate', () => {
  it('should emit OrderCreatedEvent when creating a new order', () => {
    scenario(OrderAggregate)
      .givenNoPriorActivity()
      .when(new CreateOrderCommand('order-123', 'customer-456'))
      .expectEvents([
        new OrderCreatedEvent('order-123', 'customer-456'),
      ]);
  });
});
```

### Python

```python
from event_sourcing.testing import scenario

def test_create_order():
    scenario(OrderAggregate) \
        .given_no_prior_activity() \
        .when(CreateOrderCommand(aggregate_id='order-123', customer_id='customer-456')) \
        .expect_events([
            OrderCreatedEvent(order_id='order-123', customer_id='customer-456'),
        ])
```

## API Reference

### Creating a Scenario

| Method | Description |
|--------|-------------|
| `scenario(AggregateClass)` | Create a new scenario for the given aggregate type |

### Given Phase (Setup)

| Method | Description |
|--------|-------------|
| `.given([events])` | Replay these events to set up initial aggregate state |
| `.givenNoPriorActivity()` | Start with a fresh, uninitialized aggregate |
| `.givenCommands([commands])` | Set up state by executing these commands first |

### When Phase (Action)

| Method | Description |
|--------|-------------|
| `.when(command)` | Execute this command against the aggregate |

### Then Phase (Assertions)

| Method | Description |
|--------|-------------|
| `.expectEvents([events])` | Assert exactly these events were emitted |
| `.expectNoEvents()` | Assert no events were emitted |
| `.expectException(ErrorClass)` | Assert this exception type was thrown |
| `.expectExceptionMessage(msg)` | Assert exception message contains this string |
| `.expectState(callback)` | Assert aggregate state using a callback function |
| `.expectSuccessfulHandlerExecution()` | Assert no exception was thrown |

## Common Patterns

### Happy Path Testing

Test that commands produce expected events:

```typescript
it('should add item to order', () => {
  scenario(OrderAggregate)
    .given([
      new OrderCreatedEvent('order-1', 'customer-1'),
    ])
    .when(new AddItemCommand('order-1', 'product-1', 2, 10.00))
    .expectEvents([
      new ItemAddedEvent('product-1', 2, 10.00),
    ]);
});
```

### Error Path Testing

Test that business rules are enforced:

```typescript
it('should reject adding item to cancelled order', () => {
  scenario(OrderAggregate)
    .given([
      new OrderCreatedEvent('order-1', 'customer-1'),
      new OrderCancelledEvent('order-1', 'Customer request'),
    ])
    .when(new AddItemCommand('order-1', 'product-1', 2, 10.00))
    .expectException(Error)
    .expectExceptionMessage('Cannot add items to cancelled order');
});
```

### State Verification

Verify aggregate state after command execution:

```typescript
it('should calculate total correctly', () => {
  scenario(OrderAggregate)
    .given([
      new OrderCreatedEvent('order-1', 'customer-1'),
      new ItemAddedEvent('product-1', 2, 10.00),
      new ItemAddedEvent('product-2', 1, 15.00),
    ])
    .when(new AddItemCommand('order-1', 'product-3', 3, 5.00))
    .expectState((aggregate) => {
      expect(aggregate.getItemCount()).toBe(6);
      expect(aggregate.getTotal()).toBe(50.00);
    });
});
```

### Using givenCommands for Complex Setup

Instead of manually constructing events, use commands:

```typescript
it('should ship confirmed order', () => {
  scenario(OrderAggregate)
    .givenCommands([
      new CreateOrderCommand('order-1', 'customer-1'),
      new AddItemCommand('order-1', 'product-1', 2, 10.00),
      new ConfirmOrderCommand('order-1'),
    ])
    .when(new ShipOrderCommand('order-1', 'TRACK123'))
    .expectEvents([
      new OrderShippedEvent('TRACK123'),
    ]);
});
```

### Multiple Assertions

Chain multiple assertions together:

```typescript
it('should emit event and update state correctly', () => {
  scenario(OrderAggregate)
    .given([
      new OrderCreatedEvent('order-1', 'customer-1'),
    ])
    .when(new AddItemCommand('order-1', 'product-1', 2, 10.00))
    .expectEvents([
      new ItemAddedEvent('product-1', 2, 10.00),
    ])
    .expectState((aggregate) => {
      expect(aggregate.getTotal()).toBe(20.00);
    });
});
```

## Type Safety

Both TypeScript and Python implementations are fully typed:

### TypeScript

```typescript
// Full IntelliSense and type checking
scenario(OrderAggregate)        // Returns: AggregateScenario<OrderAggregate>
  .given([...])                 // Returns: TestExecutor<OrderAggregate>
  .when(command)                // Returns: ResultValidator<OrderAggregate>
  .expectState((agg) => {       // agg is typed as OrderAggregate
    agg.getTotal();             // Full autocomplete
  });
```

### Python

```python
# Full type hints - passes mypy --strict
scenario(OrderAggregate) \      # Type: AggregateScenario[OrderAggregate]
    .given([...]) \             # Type: TestExecutor[OrderAggregate]
    .when(command) \            # Returns ResultValidator[OrderAggregate]
    .expect_state(lambda agg: ...)  # agg is typed as OrderAggregate
```

## Best Practices

### 1. Test Every Command Handler

Each command handler should have at least:
- One happy path test
- Tests for each business rule violation
- State verification where appropriate

### 2. Use Real Event Types

Always use actual event class instances, not raw objects:

```typescript
// ✅ Good - type-safe
.expectEvents([new OrderCreatedEvent('order-1', 'customer-1')])

// ❌ Bad - no type checking
.expectEvents([{ eventType: 'OrderCreated', orderId: 'order-1' }])
```

### 3. Test Business Rules Explicitly

Document business rules through test names:

```typescript
describe('Order Business Rules', () => {
  it('should not allow adding items after order is confirmed', () => { ... });
  it('should not allow confirming order with zero items', () => { ... });
  it('should not allow shipping unconfirmed order', () => { ... });
});
```

### 4. Use givenCommands for Complex Setup

When setting up complex state, `givenCommands()` is cleaner than manually constructing events:

```typescript
// ✅ Cleaner
scenario(OrderAggregate)
  .givenCommands([
    new CreateOrderCommand(...),
    new AddItemCommand(...),
    new ConfirmOrderCommand(...),
  ])
  .when(new ShipOrderCommand(...))

// ❌ Verbose
scenario(OrderAggregate)
  .given([
    new OrderCreatedEvent(...),
    new ItemAddedEvent(...),
    new OrderConfirmedEvent(...),
  ])
  .when(new ShipOrderCommand(...))
```

### 5. Keep Tests Fast

Scenario tests are designed to be fast:
- No database required
- No I/O operations
- Pure in-memory execution

Run them frequently during development.

## Error Messages

The scenario testing provides clear error messages:

```
ScenarioAssertionError: Expected 2 events but got 1

Expected events:
  1. OrderCreatedEvent { orderId: 'order-1', customerId: 'customer-1' }
  2. ItemAddedEvent { productId: 'product-1', quantity: 2 }

Actual events:
  1. OrderCreatedEvent { orderId: 'order-1', customerId: 'customer-1' }
```

```
ScenarioAssertionError: Expected exception Error but none was thrown
```

```
ScenarioAssertionError: Expected exception message to contain 'invalid'
  but got: 'Order must have at least one item'
```

## See Also

- [Best Practices](./best-practices) — Testing patterns and examples
