# TypeScript SDK Guide

Complete guide to building event-sourced applications with the TypeScript SDK.

## 📦 Installation

```bash
npm install @neurale/event-sourcing-ts
# or
pnpm add @neurale/event-sourcing-ts
```

## 🚀 Quick Start

### 1. Setup Event Store Client

```typescript
import { EventStoreClientFactory } from '@neurale/event-sourcing-ts';

// Production: gRPC client
const client = EventStoreClientFactory.createGrpcClient({
  endpoint: 'localhost:50051',
  credentials: {
    username: 'user',
    password: 'pass'
  }
});

// Development/Testing: In-memory client
const memoryClient = EventStoreClientFactory.createMemoryClient();
```

### 2. Define Events

```typescript
import { BaseDomainEvent } from '@neurale/event-sourcing-ts';

class OrderPlaced extends BaseDomainEvent {
  constructor(
    public readonly orderId: string,
    public readonly customerId: string,
    public readonly items: Array<{ productId: string; quantity: number; price: number }>
  ) {
    super();
  }
}

class OrderShipped extends BaseDomainEvent {
  constructor(
    public readonly orderId: string,
    public readonly trackingNumber: string
  ) {
    super();
  }
}

class OrderCancelled extends BaseDomainEvent {
  constructor(
    public readonly orderId: string,
    public readonly reason: string
  ) {
    super();
  }
}
```

### 3. Create Aggregate

```typescript
import { AggregateRoot, EventSourcingHandler } from '@neurale/event-sourcing-ts';

type OrderEvent = OrderPlaced | OrderShipped | OrderCancelled;

class OrderAggregate extends AggregateRoot<OrderEvent> {
  private status: 'new' | 'placed' | 'shipped' | 'cancelled' = 'new';
  private customerId: string = '';
  private items: Array<{ productId: string; quantity: number; price: number }> = [];
  private trackingNumber?: string;

  getAggregateType(): string {
    return 'Order';
  }

  // Commands
  place(orderId: string, customerId: string, items: typeof this.items) {
    if (items.length === 0) {
      throw new Error('Order must have at least one item');
    }
    this.initialize(orderId);
    this.raiseEvent(new OrderPlaced(orderId, customerId, items));
  }

  ship(trackingNumber: string) {
    if (this.status !== 'placed') {
      throw new Error('Can only ship placed orders');
    }
    this.raiseEvent(new OrderShipped(this.id!, trackingNumber));
  }

  cancel(reason: string) {
    if (this.status === 'shipped') {
      throw new Error('Cannot cancel shipped orders');
    }
    if (this.status === 'cancelled') {
      throw new Error('Order already cancelled');
    }
    this.raiseEvent(new OrderCancelled(this.id!, reason));
  }

  // Event Handlers
  @EventSourcingHandler(OrderPlaced)
  onOrderPlaced(event: OrderPlaced): void {
    this.status = 'placed';
    this.customerId = event.customerId;
    this.items = event.items;
  }

  @EventSourcingHandler(OrderShipped)
  onOrderShipped(event: OrderShipped): void {
    this.status = 'shipped';
    this.trackingNumber = event.trackingNumber;
  }

  @EventSourcingHandler(OrderCancelled)
  onOrderCancelled(event: OrderCancelled): void {
    this.status = 'cancelled';
  }

  // Getters
  getStatus() { return this.status; }
  getCustomerId() { return this.customerId; }
  getItems() { return [...this.items]; }
  getTrackingNumber() { return this.trackingNumber; }
}
```

### 4. Setup Repository

```typescript
import { EventSerializer, RepositoryFactory } from '@neurale/event-sourcing-ts';

// Register events
const serializer = new EventSerializer();
serializer.registerEvent(OrderPlaced);
serializer.registerEvent(OrderShipped);
serializer.registerEvent(OrderCancelled);

// Create repository
const factory = new RepositoryFactory(client);
const orderRepository = factory.create(
  OrderAggregate,
  'Order',
  serializer
);
```

### 5. Use the Repository

```typescript
// Create new order
const order = new OrderAggregate();
order.place('order-123', 'customer-1', [
  { productId: 'prod-1', quantity: 2, price: 29.99 },
  { productId: 'prod-2', quantity: 1, price: 49.99 }
]);
await orderRepository.save(order);

// Load existing order
const loaded = await orderRepository.load('order-123');
if (loaded) {
  console.log('Order status:', loaded.getStatus());
  
  // Ship the order
  loaded.ship('TRACK-123456');
  await orderRepository.save(loaded);
}
```

## 🏗️ Aggregate Patterns

### Pattern 1: AggregateRoot (Recommended)

Uses decorators to automatically route events to handlers:

```typescript
class OrderAggregate extends AggregateRoot<OrderEvent> {
  @EventSourcingHandler(OrderPlaced)
  onOrderPlaced(event: OrderPlaced): void {
    // Update state
  }
}
```

**Pros:**
- Clean separation of commands and event handlers
- Type-safe event routing
- Easy to understand and maintain

**Cons:**
- Requires decorator support
- Slightly more boilerplate

### Pattern 2: BaseAggregate with Manual Dispatch

Manually dispatch events in `applyEvent`:

```typescript
class OrderAggregate extends BaseAggregate<OrderEvent> {
  applyEvent(event: OrderEvent): void {
    if (event instanceof OrderPlaced) {
      this.onOrderPlaced(event);
    } else if (event instanceof OrderShipped) {
      this.onOrderShipped(event);
    } else if (event instanceof OrderCancelled) {
      this.onOrderCancelled(event);
    }
  }

  private onOrderPlaced(event: OrderPlaced): void {
    this.status = 'placed';
    // ...
  }
}
```

**Pros:**
- No decorators needed
- Full control over dispatch logic
- Can add cross-cutting concerns

**Cons:**
- More boilerplate
- Easy to forget to handle events

### Pattern 3: State Machine Aggregate

Model complex state transitions:

```typescript
class OrderAggregate extends AggregateRoot<OrderEvent> {
  private state: OrderState = new NewOrderState();

  place(orderId: string, customerId: string, items: LineItem[]) {
    this.state.place(this, orderId, customerId, items);
  }

  ship(trackingNumber: string) {
    this.state.ship(this, trackingNumber);
  }

  @EventSourcingHandler(OrderPlaced)
  onOrderPlaced(event: OrderPlaced): void {
    this.state = new PlacedOrderState();
    // ...
  }
}

interface OrderState {
  place(aggregate: OrderAggregate, ...args: any[]): void;
  ship(aggregate: OrderAggregate, trackingNumber: string): void;
  cancel(aggregate: OrderAggregate, reason: string): void;
}

class NewOrderState implements OrderState {
  place(aggregate: OrderAggregate, orderId: string, customerId: string, items: LineItem[]) {
    aggregate.initialize(orderId);
    aggregate.raiseEvent(new OrderPlaced(orderId, customerId, items));
  }

  ship() {
    throw new Error('Cannot ship order that has not been placed');
  }

  cancel() {
    throw new Error('Cannot cancel order that has not been placed');
  }
}
```

## 📝 Event Design

### Event Naming

Use past tense and be specific:

```typescript
// ✅ Good
class OrderPlaced extends BaseDomainEvent { }
class PaymentProcessed extends BaseDomainEvent { }
class InventoryReserved extends BaseDomainEvent { }

// ❌ Bad
class PlaceOrder extends BaseDomainEvent { }  // Command, not event
class Order extends BaseDomainEvent { }        // Not specific
class OrderEvent extends BaseDomainEvent { }   // Too generic
```

### Event Data

Include all necessary information:

```typescript
// ✅ Good - Complete information
class OrderPlaced extends BaseDomainEvent {
  constructor(
    public readonly orderId: string,
    public readonly customerId: string,
    public readonly items: LineItem[],
    public readonly totalAmount: number,
    public readonly currency: string,
    public readonly placedAt: Date
  ) {
    super();
  }
}

// ❌ Bad - Missing important data
class OrderPlaced extends BaseDomainEvent {
  constructor(public readonly orderId: string) {
    super();
  }
}
```

### Event Versioning

Plan for schema evolution:

```typescript
// Version 1
class OrderPlacedV1 extends BaseDomainEvent {
  constructor(
    public readonly orderId: string,
    public readonly customerId: string
  ) {
    super();
  }
}

// Version 2 - Added items
class OrderPlacedV2 extends BaseDomainEvent {
  constructor(
    public readonly orderId: string,
    public readonly customerId: string,
    public readonly items: LineItem[]
  ) {
    super();
  }
}

// Upcaster to migrate old events
class OrderPlacedUpcaster {
  upcast(v1: OrderPlacedV1): OrderPlacedV2 {
    return new OrderPlacedV2(
      v1.orderId,
      v1.customerId,
      [] // Default empty items for old events
    );
  }
}
```

## 🗄️ Repository Usage

### Basic Operations

```typescript
// Create new aggregate
const order = new OrderAggregate();
order.place('order-123', 'customer-1', items);
await repository.save(order);

// Load existing aggregate
const loaded = await repository.load('order-123');
if (!loaded) {
  throw new Error('Order not found');
}

// Modify and save
loaded.ship('TRACK-123');
await repository.save(loaded);
```

### Concurrency Handling

```typescript
async function shipOrderWithRetry(orderId: string, trackingNumber: string, maxRetries = 3) {
  for (let attempt = 0; attempt < maxRetries; attempt++) {
    try {
      const order = await repository.load(orderId);
      if (!order) {
        throw new Error('Order not found');
      }

      order.ship(trackingNumber);
      await repository.save(order);
      return; // Success
    } catch (error) {
      if (error instanceof ConcurrencyError && attempt < maxRetries - 1) {
        // Retry with exponential backoff
        await new Promise(resolve => setTimeout(resolve, Math.pow(2, attempt) * 100));
        continue;
      }
      throw error;
    }
  }
}
```

### Transaction Pattern

```typescript
async function processOrder(orderId: string, customerId: string, items: LineItem[]) {
  const order = new OrderAggregate();
  
  try {
    // Execute business logic
    order.place(orderId, customerId, items);
    
    // Validate
    if (order.hasUncommittedEvents()) {
      // Save atomically
      await repository.save(order);
      console.log('Order placed successfully');
    }
  } catch (error) {
    console.error('Failed to place order:', error);
    throw error;
  }
}
```

## 📊 Building Projections

### Simple Projection

```typescript
import { Projection, DomainEvent, EventMetadata } from '@neurale/event-sourcing-ts';

interface OrderSummary {
  totalOrders: number;
  totalRevenue: number;
  averageOrderValue: number;
}

class OrderSummaryProjection implements Projection<OrderSummary> {
  private summary: OrderSummary = {
    totalOrders: 0,
    totalRevenue: 0,
    averageOrderValue: 0
  };

  async handleEvent(event: DomainEvent, metadata: EventMetadata): Promise<void> {
    if (event instanceof OrderPlaced) {
      this.summary.totalOrders++;
      const orderTotal = event.items.reduce((sum, item) => 
        sum + item.price * item.quantity, 0
      );
      this.summary.totalRevenue += orderTotal;
      this.summary.averageOrderValue = 
        this.summary.totalRevenue / this.summary.totalOrders;
    }
  }

  getState(): OrderSummary {
    return { ...this.summary };
  }

  async reset(): Promise<void> {
    this.summary = {
      totalOrders: 0,
      totalRevenue: 0,
      averageOrderValue: 0
    };
  }
}
```

### Projection with Persistence

```typescript
class OrderListProjection implements Projection<Order[]> {
  private orders: Map<string, Order> = new Map();

  async handleEvent(event: DomainEvent, metadata: EventMetadata): Promise<void> {
    if (event instanceof OrderPlaced) {
      this.orders.set(event.orderId, {
        id: event.orderId,
        customerId: event.customerId,
        status: 'placed',
        items: event.items,
        createdAt: metadata.timestamp
      });
    } else if (event instanceof OrderShipped) {
      const order = this.orders.get(event.orderId);
      if (order) {
        order.status = 'shipped';
        order.shippedAt = metadata.timestamp;
      }
    } else if (event instanceof OrderCancelled) {
      const order = this.orders.get(event.orderId);
      if (order) {
        order.status = 'cancelled';
        order.cancelledAt = metadata.timestamp;
      }
    }
  }

  getState(): Order[] {
    return Array.from(this.orders.values());
  }

  async reset(): Promise<void> {
    this.orders.clear();
  }

  // Query methods
  getOrderById(orderId: string): Order | undefined {
    return this.orders.get(orderId);
  }

  getOrdersByCustomer(customerId: string): Order[] {
    return Array.from(this.orders.values())
      .filter(order => order.customerId === customerId);
  }

  getOrdersByStatus(status: string): Order[] {
    return Array.from(this.orders.values())
      .filter(order => order.status === status);
  }
}
```

## 🧪 Testing

### Unit Testing Aggregates

```typescript
import { describe, it, expect, beforeEach } from 'vitest';

describe('OrderAggregate', () => {
  let order: OrderAggregate;

  beforeEach(() => {
    order = new OrderAggregate();
  });

  describe('place', () => {
    it('should place a new order', () => {
      const items = [{ productId: 'prod-1', quantity: 2, price: 29.99 }];
      
      order.place('order-123', 'customer-1', items);
      
      expect(order.id).toBe('order-123');
      expect(order.getStatus()).toBe('placed');
      expect(order.getCustomerId()).toBe('customer-1');
      expect(order.hasUncommittedEvents()).toBe(true);
      
      const events = order.getUncommittedEvents();
      expect(events).toHaveLength(1);
      expect(events[0].event).toBeInstanceOf(OrderPlaced);
    });

    it('should reject orders with no items', () => {
      expect(() => {
        order.place('order-123', 'customer-1', []);
      }).toThrow('Order must have at least one item');
    });
  });

  describe('ship', () => {
    beforeEach(() => {
      order.place('order-123', 'customer-1', [
        { productId: 'prod-1', quantity: 1, price: 29.99 }
      ]);
      order.markEventsAsCommitted();
    });

    it('should ship a placed order', () => {
      order.ship('TRACK-123');
      
      expect(order.getStatus()).toBe('shipped');
      expect(order.getTrackingNumber()).toBe('TRACK-123');
    });

    it('should reject shipping non-placed orders', () => {
      const newOrder = new OrderAggregate();
      
      expect(() => {
        newOrder.ship('TRACK-123');
      }).toThrow('Can only ship placed orders');
    });
  });
});
```

### Integration Testing with Repository

```typescript
describe('OrderAggregate Integration', () => {
  let repository: Repository<OrderAggregate, OrderEvent>;
  let serializer: EventSerializer;

  beforeEach(() => {
    const memoryStore = new MemoryEventStoreClient();
    serializer = new EventSerializer();
    serializer.registerEvent(OrderPlaced);
    serializer.registerEvent(OrderShipped);
    serializer.registerEvent(OrderCancelled);
    
    const factory = new RepositoryFactory(memoryStore);
    repository = factory.create(OrderAggregate, 'Order', serializer);
  });

  it('should persist and load orders', async () => {
    // Create and save
    const order = new OrderAggregate();
    order.place('order-123', 'customer-1', [
      { productId: 'prod-1', quantity: 2, price: 29.99 }
    ]);
    await repository.save(order);

    // Load and verify
    const loaded = await repository.load('order-123');
    expect(loaded).not.toBeNull();
    expect(loaded?.getStatus()).toBe('placed');
    expect(loaded?.getCustomerId()).toBe('customer-1');
  });

  it('should handle full order lifecycle', async () => {
    // Place order
    const order = new OrderAggregate();
    order.place('order-123', 'customer-1', [
      { productId: 'prod-1', quantity: 1, price: 29.99 }
    ]);
    await repository.save(order);

    // Ship order
    const loaded1 = await repository.load('order-123');
    loaded1!.ship('TRACK-123');
    await repository.save(loaded1!);

    // Verify final state
    const loaded2 = await repository.load('order-123');
    expect(loaded2?.getStatus()).toBe('shipped');
    expect(loaded2?.getTrackingNumber()).toBe('TRACK-123');
  });
});
```

### Testing Projections

```typescript
describe('OrderSummaryProjection', () => {
  let projection: OrderSummaryProjection;

  beforeEach(() => {
    projection = new OrderSummaryProjection();
  });

  it('should calculate order summary', async () => {
    const event1 = new OrderPlaced('order-1', 'customer-1', [
      { productId: 'prod-1', quantity: 2, price: 29.99 }
    ]);
    const event2 = new OrderPlaced('order-2', 'customer-2', [
      { productId: 'prod-2', quantity: 1, price: 49.99 }
    ]);

    await projection.handleEvent(event1, createMockMetadata());
    await projection.handleEvent(event2, createMockMetadata());

    const summary = projection.getState();
    expect(summary.totalOrders).toBe(2);
    expect(summary.totalRevenue).toBe(109.97);
    expect(summary.averageOrderValue).toBeCloseTo(54.985);
  });
});
```

## 🔐 Advanced Features

### Message Context

```typescript
import { MessageContext } from '@neurale/event-sourcing-ts';

// Set context for request tracking
MessageContext.set({
  correlationId: 'request-123',
  causationId: 'command-456'
});

// Context is automatically propagated to events
await repository.save(order);

// Events will have correlationId and causationId in metadata
```

### Custom Event Metadata

```typescript
class OrderPlaced extends BaseDomainEvent {
  constructor(
    public readonly orderId: string,
    public readonly customerId: string,
    public readonly items: LineItem[],
    public readonly metadata?: {
      ipAddress?: string;
      userAgent?: string;
      sessionId?: string;
    }
  ) {
    super();
  }
}
```

## 📚 Next Steps

- **[API Reference](../api-reference.md)** - Complete API documentation
- **[SDK Overview](../overview/sdk-overview.md)** - Architecture and patterns

