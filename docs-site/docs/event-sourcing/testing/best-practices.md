# Testing Best Practices

Patterns for testing Event Sourcing applications with the `scenario()` API.

## Test Behavior, Not Implementation

```typescript
// ✅ Good: Tests business behavior
it('applies discount correctly', () => {
  scenario(OrderAggregate)
    .given([
      new OrderCreatedEvent('order-1', 'customer-1'),
      new ItemAddedEvent('product-1', 1, 100.00),
    ])
    .when(new ApplyDiscountCommand('order-1', 15))
    .expectState((agg) => {
      expect(agg.getTotal()).toBe(85.00);
    });
});

// ❌ Bad: Tests internal state
it('sets _discountApplied to true', () => {
  // Testing private implementation details
});
```

## Cover All Command Handlers

Each command handler needs:
- Happy path (command produces expected events)
- Error paths (each business rule violation)
- State verification (aggregate state is correct)

```typescript
describe('AddItemCommand', () => {
  it('adds item to order', () => {
    scenario(OrderAggregate)
      .given([new OrderCreatedEvent('order-1', 'customer-1')])
      .when(new AddItemCommand('order-1', 'product-1', 2, 10.00))
      .expectEvents([new ItemAddedEvent('product-1', 2, 10.00)]);
  });

  it('rejects adding to confirmed order', () => {
    scenario(OrderAggregate)
      .given([
        new OrderCreatedEvent('order-1', 'customer-1'),
        new OrderConfirmedEvent('order-1'),
      ])
      .when(new AddItemCommand('order-1', 'product-1', 2, 10.00))
      .expectException(Error)
      .expectExceptionMessage('Cannot add items to confirmed order');
  });

  it('updates total correctly', () => {
    scenario(OrderAggregate)
      .given([
        new OrderCreatedEvent('order-1', 'customer-1'),
        new ItemAddedEvent('product-1', 2, 10.00),
      ])
      .when(new AddItemCommand('order-1', 'product-2', 1, 25.00))
      .expectState((agg) => {
        expect(agg.getTotal()).toBe(45.00);
      });
  });
});
```

## Test State Machine Transitions

For aggregates with state machines, test valid and invalid transitions:

```typescript
describe('Order state machine', () => {
  it('allows: draft → confirmed', () => {
    scenario(OrderAggregate)
      .given([
        new OrderCreatedEvent('order-1', 'customer-1'),
        new ItemAddedEvent('product-1', 1, 10.00),
      ])
      .when(new ConfirmOrderCommand('order-1'))
      .expectEvents([new OrderConfirmedEvent('order-1')]);
  });

  it('rejects: draft → shipped (must confirm first)', () => {
    scenario(OrderAggregate)
      .given([new OrderCreatedEvent('order-1', 'customer-1')])
      .when(new ShipOrderCommand('order-1', 'TRACK123'))
      .expectException(Error)
      .expectExceptionMessage('Cannot ship unconfirmed order');
  });

  it('rejects: cancelled → confirmed', () => {
    scenario(OrderAggregate)
      .given([
        new OrderCreatedEvent('order-1', 'customer-1'),
        new OrderCancelledEvent('order-1', 'Customer request'),
      ])
      .when(new ConfirmOrderCommand('order-1'))
      .expectException(Error)
      .expectExceptionMessage('Cannot confirm cancelled order');
  });
});
```

## Use givenCommands for Complex Setup

When setup requires many events, use commands instead:

```typescript
// ✅ Cleaner
scenario(OrderAggregate)
  .givenCommands([
    new CreateOrderCommand('order-1', 'customer-1'),
    new AddItemCommand('order-1', 'product-1', 2, 10.00),
    new AddItemCommand('order-1', 'product-2', 1, 25.00),
    new ConfirmOrderCommand('order-1'),
  ])
  .when(new ShipOrderCommand('order-1', 'TRACK123'))
  .expectEvents([new OrderShippedEvent('TRACK123')]);

// ❌ Verbose - manually constructing all events
scenario(OrderAggregate)
  .given([
    new OrderCreatedEvent('order-1', 'customer-1'),
    new ItemAddedEvent('product-1', 2, 10.00),
    new ItemAddedEvent('product-2', 1, 25.00),
    new OrderConfirmedEvent('order-1'),
  ])
  .when(new ShipOrderCommand('order-1', 'TRACK123'))
```

## Use Real Event Types

Always use actual event class instances for type safety:

```typescript
// ✅ Good - compiler catches mismatches
.expectEvents([new OrderCreatedEvent('order-1', 'customer-1')])

// ❌ Bad - no type checking, easy to make mistakes
.expectEvents([{ eventType: 'OrderCreated', orderId: 'order-1' }])
```

## Name Tests as Business Rules

Test names should document expected behavior:

```typescript
describe('Order Business Rules', () => {
  it('should not allow confirming order with no items', () => { ... });
  it('should not allow adding items after confirmation', () => { ... });
  it('should not allow shipping before confirmation', () => { ... });
  it('should not allow cancelling after shipment', () => { ... });
});
```

## Keep Tests Fast

Scenario tests are pure in-memory with no I/O. Run them frequently:

```bash
# Run on every save during development
pnpm test --watch
```
