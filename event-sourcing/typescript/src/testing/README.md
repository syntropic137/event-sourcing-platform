# ES Test Kit - TypeScript

Testing toolkit for event-sourced applications.

## scenario() - Given-When-Then Testing

Test aggregate command handlers using the Given-When-Then pattern.

### Installation

```typescript
import { scenario } from '@syntropic137/event-sourcing-typescript/testing';
```

### Usage

```typescript
import { scenario } from '@syntropic137/event-sourcing-typescript/testing';

describe('OrderAggregate', () => {
  // Happy path: command produces events
  it('creates order', () => {
    scenario(OrderAggregate)
      .givenNoPriorActivity()
      .when(new CreateOrderCommand('order-1', 'customer-1'))
      .expectEvents([
        new OrderCreatedEvent('order-1', 'customer-1'),
      ]);
  });

  // Error path: business rule violated
  it('rejects duplicate order', () => {
    scenario(OrderAggregate)
      .given([
        new OrderCreatedEvent('order-1', 'customer-1'),
      ])
      .when(new CreateOrderCommand('order-1', 'customer-2'))
      .expectException(Error)
      .expectExceptionMessage('Order already exists');
  });

  // State verification
  it('tracks total correctly', () => {
    scenario(OrderAggregate)
      .given([
        new OrderCreatedEvent('order-1', 'customer-1'),
        new ItemAddedEvent('product-1', 2, 10.00),
      ])
      .when(new AddItemCommand('order-1', 'product-2', 1, 25.00))
      .expectState((aggregate) => {
        expect(aggregate.getTotal()).toBe(45.00);
      });
  });

  // Setup via commands instead of events
  it('ships confirmed order', () => {
    scenario(OrderAggregate)
      .givenCommands([
        new CreateOrderCommand('order-1', 'customer-1'),
        new AddItemCommand('order-1', 'product-1', 1, 10.00),
        new ConfirmOrderCommand('order-1'),
      ])
      .when(new ShipOrderCommand('order-1', 'TRACK123'))
      .expectEvents([
        new OrderShippedEvent('TRACK123'),
      ]);
  });
});
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
| `.expectSuccessfulHandlerExecution()` | Assert no exception |

### Type Safety

Full TypeScript support with generics:

```typescript
scenario(OrderAggregate)        // AggregateScenario<OrderAggregate>
  .given([...])                 // TestExecutor<OrderAggregate>
  .when(command)                // ResultValidator<OrderAggregate>
  .expectState((agg) => {       // agg typed as OrderAggregate
    agg.getTotal();             // Full autocomplete
  });
```

## Future Enhancements

- Replay testing (golden file testing)
- Invariant verification
- Projection testing
- Fixture loading (JSON/YAML test data)
