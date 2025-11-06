# Aggregates

Aggregates are the cornerstone of event sourcing - they encapsulate business logic and ensure consistency within a bounded context.

## What is an Aggregate?

An **Aggregate** is a cluster of domain objects that can be treated as a single unit. In event sourcing, aggregates:

- **Process commands** and decide whether to accept or reject them
- **Emit events** when state changes occur
- **Maintain consistency** within their boundary
- **Have a unique identity** (aggregate ID)

## Anatomy of an Aggregate

```typescript
@Aggregate
class OrderAggregate extends AggregateRoot {
  private status: OrderStatus = OrderStatus.Draft;
  private items: LineItem[] = [];
  private totalAmount: number = 0;

  // Command handler - processes business commands
  @CommandHandler(PlaceOrderCommand)
  place(command: PlaceOrderCommand) {
    // Business rule validation
    if (command.items.length === 0) {
      throw new Error("Order must have at least one item");
    }

    // Calculate total
    const total = command.items.reduce((sum, item) => sum + item.price * item.quantity, 0);

    // Emit event
    this.apply(new OrderPlaced({
      orderId: command.orderId,
      customerId: command.customerId,
      items: command.items,
      totalAmount: total
    }));
  }

  // Event sourcing handler - applies events to aggregate state
  @EventSourcingHandler(OrderPlaced)
  private onOrderPlaced(event: OrderPlaced) {
    this.status = OrderStatus.Placed;
    this.items = event.items;
    this.totalAmount = event.totalAmount;
  }

  @CommandHandler(ShipOrderCommand)
  ship(command: ShipOrderCommand) {
    // Business rule validation
    if (this.status !== OrderStatus.Placed) {
      throw new Error("Only placed orders can be shipped");
    }

    this.apply(new OrderShipped({
      orderId: this.aggregateId,
      trackingNumber: command.trackingNumber,
      shippedAt: new Date()
    }));
  }

  @EventSourcingHandler(OrderShipped)
  private onOrderShipped(event: OrderShipped) {
    this.status = OrderStatus.Shipped;
    this.trackingNumber = event.trackingNumber;
    this.shippedAt = event.shippedAt;
  }
}
```

## Handler Types & Decorators

### Core Decorators

#### `@Aggregate`
Marks a class as an aggregate root - the entry point for all operations on the aggregate.

```typescript
@Aggregate
class OrderAggregate extends AggregateRoot {
  // Aggregate implementation
}
```

#### `@CommandHandler(CommandType)`
Handles business commands - the "write side" operations that can change aggregate state.

```typescript
@CommandHandler(PlaceOrderCommand)
place(command: PlaceOrderCommand) {
  // Business logic and validation
  // Emits events via this.apply()
}
```

#### `@EventSourcingHandler(EventType)`
Applies events to aggregate state during reconstruction from event store.

```typescript
@EventSourcingHandler(OrderPlaced)
private onOrderPlaced(event: OrderPlaced) {
  // Update internal state only
  // NO business logic or validation
  // NO side effects
}
```

### Handler Responsibilities

| Handler Type | Purpose | Can Do | Cannot Do |
|--------------|---------|---------|-----------|
| `@CommandHandler` | Process business commands | Validate, apply business rules, emit events | Directly modify state |
| `@EventSourcingHandler` | Apply events to state | Update internal fields | Validate, emit events, side effects |

### Command vs Event Flow

```typescript
// 1. Command comes in
const command = new PlaceOrderCommand(orderId, customerId, items);

// 2. Command handler processes it
@CommandHandler(PlaceOrderCommand)
place(command: PlaceOrderCommand) {
  // Business validation
  if (command.items.length === 0) {
    throw new Error("Order must have at least one item");
  }
  
  // Emit event (not direct state change)
  this.apply(new OrderPlaced({...}));
}

// 3. Event sourcing handler applies the event
@EventSourcingHandler(OrderPlaced)
private onOrderPlaced(event: OrderPlaced) {
  // Direct state change (no validation)
  this.status = OrderStatus.Placed;
  this.items = event.items;
}
```

## Key Principles

### 1. Single Responsibility
Each aggregate should have one reason to change - it should represent one cohesive business concept.

```typescript
// ✅ Good - Order aggregate handles order lifecycle
class OrderAggregate {
  place() { /* ... */ }
  ship() { /* ... */ }
  cancel() { /* ... */ }
}

// ❌ Bad - Mixed responsibilities
class OrderCustomerAggregate {
  placeOrder() { /* ... */ }
  updateCustomerEmail() { /* ... */ } // Different bounded context
}
```

### 2. Consistency Boundary
All invariants within an aggregate must be maintained consistently.

```typescript
class InventoryAggregate {
  private stockLevel: number = 0;
  private reservedStock: number = 0;

  reserve(quantity: number) {
    // Business invariant: can't reserve more than available
    if (this.stockLevel - this.reservedStock < quantity) {
      throw new Error("Insufficient stock available");
    }

    this.raiseEvent(new StockReserved({ quantity }));
  }
}
```

### 3. Event-Driven State Changes
Aggregates change state only through events, never directly.

```typescript
@Aggregate
class AccountAggregate extends AggregateRoot {
  private balance: number = 0;

  @CommandHandler(DepositMoneyCommand)
  deposit(command: DepositMoneyCommand) {
    // ❌ Don't modify state directly
    // this.balance += command.amount;

    // ✅ Emit event instead
    this.apply(new MoneyDeposited({ 
      accountId: this.aggregateId,
      amount: command.amount 
    }));
  }

  @EventSourcingHandler(MoneyDeposited)
  private onMoneyDeposited(event: MoneyDeposited) {
    // State changes happen in event sourcing handlers
    this.balance += event.amount;
  }
}
```

## Commands & Events

### Command Structure
Commands represent intentions to change state. Commands should be **classes** (not interfaces) with an `aggregateId` property:

```typescript
// Commands are imperative (what should happen)
// Use classes with aggregateId for proper command dispatching
class PlaceOrderCommand {
  constructor(
    public readonly aggregateId: string,  // Required for all commands
    public readonly customerId: string,
    public readonly items: LineItem[]
  ) {}
}

class ShipOrderCommand {
  constructor(
    public readonly aggregateId: string,  // Required for all commands
    public readonly trackingNumber: string
  ) {}
}

class DepositMoneyCommand {
  constructor(
    public readonly aggregateId: string,  // Required for all commands
    public readonly amount: number
  ) {}
}
```

**Why classes?** The `@CommandHandler` decorator uses the command's constructor name for routing, and `aggregateId` is required for the repository to load/save the correct aggregate.

### Event Structure
Events represent facts about what happened:

```typescript
// Events are past tense (what did happen)
interface OrderPlaced {
  eventType: 'OrderPlaced';
  orderId: string;
  customerId: string;
  items: LineItem[];
  totalAmount: number;
  placedAt: Date;
}

interface OrderShipped {
  eventType: 'OrderShipped';
  orderId: string;
  trackingNumber: string;
  shippedAt: Date;
}
```

### Command → Event Flow

```typescript
@Aggregate
class OrderAggregate extends AggregateRoot {
  @CommandHandler(PlaceOrderCommand)
  handle(command: PlaceOrderCommand) {
    // 1. Validate business rules
    if (command.items.length === 0) {
      throw new Error("Order must have items");
    }

    // 2. Calculate derived data
    const totalAmount = command.items.reduce((sum, item) => 
      sum + (item.price * item.quantity), 0);

    // 3. Apply event (this triggers the event sourcing handler)
    this.apply(new OrderPlaced({
      orderId: command.orderId,
      customerId: command.customerId,
      items: command.items,
      totalAmount,
      placedAt: new Date()
    }));
  }

  @EventSourcingHandler(OrderPlaced)
  private on(event: OrderPlaced) {
    // 4. Update aggregate state (no business logic here)
    this.orderId = event.orderId;
    this.customerId = event.customerId;
    this.items = event.items;
    this.totalAmount = event.totalAmount;
    this.status = OrderStatus.Placed;
  }
}
```

## Aggregate Lifecycle

### 1. Creation
```typescript
const order = new OrderAggregate();
order.place("order-123", "customer-456", items);
```

### 2. Loading from Events
```typescript
const order = await orderRepository.load("order-123");
// Aggregate is reconstructed by replaying all events
```

### 3. Processing Commands
```typescript
order.ship("TRACK123");
await orderRepository.save(order);
```

### 4. Persistence
```typescript
// Only new events are persisted
const newEvents = order.getUncommittedEvents();
await eventStore.append("Order-order-123", newEvents);
```

## Best Practices

### Keep Aggregates Small
- **Focus on consistency** - only include what must be consistent together
- **Avoid large object graphs** - prefer references to other aggregates
- **Single aggregate per transaction** - don't modify multiple aggregates in one transaction

### Use Meaningful Business Language
```typescript
// ✅ Business-focused methods
class OrderAggregate {
  place() { /* ... */ }
  ship() { /* ... */ }
  cancel() { /* ... */ }
}

// ❌ Technical CRUD methods
class OrderAggregate {
  create() { /* ... */ }
  update() { /* ... */ }
  delete() { /* ... */ }
}
```

### Validate Business Rules
```typescript
class OrderAggregate {
  ship() {
    if (this.status !== OrderStatus.Placed) {
      throw new Error("Only placed orders can be shipped");
    }

    if (this.items.length === 0) {
      throw new Error("Cannot ship empty order");
    }

    this.raiseEvent(new OrderShipped({ 
      shippedAt: new Date(),
      trackingNumber: generateTrackingNumber()
    }));
  }
}
```

## Common Patterns

### State Machine Aggregates
```typescript
class OrderAggregate {
  private status: OrderStatus = OrderStatus.Draft;

  place() {
    this.validateTransition(OrderStatus.Placed);
    this.raiseEvent(new OrderPlaced(/* ... */));
  }

  ship() {
    this.validateTransition(OrderStatus.Shipped);
    this.raiseEvent(new OrderShipped(/* ... */));
  }

  private validateTransition(newStatus: OrderStatus) {
    const validTransitions = {
      [OrderStatus.Draft]: [OrderStatus.Placed],
      [OrderStatus.Placed]: [OrderStatus.Shipped, OrderStatus.Cancelled],
      [OrderStatus.Shipped]: [OrderStatus.Delivered],
      // ...
    };

    if (!validTransitions[this.status]?.includes(newStatus)) {
      throw new Error(`Invalid transition from ${this.status} to ${newStatus}`);
    }
  }
}
```

### Aggregate Factories
```typescript
class OrderAggregateFactory {
  static createFromCustomerCart(customerId: string, cart: ShoppingCart): OrderAggregate {
    const order = new OrderAggregate();
    order.place(
      generateOrderId(),
      customerId,
      cart.items
    );
    return order;
  }
}
```

## Testing Aggregates

```typescript
describe('OrderAggregate', () => {
  it('should place order with valid items', () => {
    // Arrange
    const order = new OrderAggregate();
    const items = [{ productId: 'p1', quantity: 2, price: 10 }];

    // Act
    order.place('order-123', 'customer-456', items);

    // Assert
    const events = order.getUncommittedEvents();
    expect(events).toHaveLength(1);
    expect(events[0]).toBeInstanceOf(OrderPlaced);
    expect(events[0].totalAmount).toBe(20);
  });

  it('should reject empty orders', () => {
    // Arrange
    const order = new OrderAggregate();

    // Act & Assert
    expect(() => {
      order.place('order-123', 'customer-456', []);
    }).toThrow('Order must have at least one item');
  });
});
```

## Next Steps

- Learn about [Events](./events) and how they capture state changes
- Explore Repositories for aggregate persistence
- See [Examples](./examples/) for hands-on aggregate implementations
