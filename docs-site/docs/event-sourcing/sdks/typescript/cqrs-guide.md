---
sidebar_position: 5
---

# Complete CQRS Guide

A comprehensive guide to implementing Command Query Responsibility Segregation (CQRS) with the Event Sourcing SDK.

## 🎯 What is CQRS?

**Command Query Responsibility Segregation (CQRS)** is a pattern that separates read and write operations into different models:

- **Commands** - Change state (writes)
- **Queries** - Read state (reads)
- **Events** - Record what happened
- **Projections** - Build optimized read models

### Why Use CQRS?

**Benefits:**
- **Scalability** - Scale reads and writes independently
- **Performance** - Optimize read models for specific queries
- **Flexibility** - Multiple views of the same data
- **Clarity** - Clear separation of concerns
- **Audit Trail** - Complete history through events

**When to Use:**
- Complex business logic with many read patterns
- High read-to-write ratios
- Need for multiple denormalized views
- Audit and compliance requirements
- Event-driven architectures

**When NOT to Use:**
- Simple CRUD applications
- Small datasets with simple queries
- Teams unfamiliar with the pattern
- No performance or scalability concerns

## 🏗️ CQRS Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        Application                           │
└─────────────────────────────────────────────────────────────┘
                          │
          ┌───────────────┴───────────────┐
          │                               │
          ▼                               ▼
┌──────────────────┐            ┌──────────────────┐
│  WRITE SIDE      │            │   READ SIDE      │
│  (Commands)      │            │   (Queries)      │
│                  │            │                  │
│  CommandBus      │            │   QueryBus       │
│      │           │            │      │           │
│      ▼           │            │      ▼           │
│  CommandHandler  │            │  QueryHandler    │
│      │           │            │      │           │
│      ▼           │            │      ▼           │
│  Aggregate       │            │  Projection      │
│      │           │            │      ▲           │
│      ▼           │            │      │           │
│  Events ─────────┼────────────┼──────┘           │
│      │           │            │                  │
│      ▼           │            │                  │
│  Event Store     │            │                  │
└──────────────────┘            └──────────────────┘
```

## 📝 Command Side: Writing Data

Commands represent **intentions to change state**. They are processed by command handlers which update aggregates and produce events.

### Step 1: Define Commands

Commands should be named with imperative verbs (PlaceOrder, CancelOrder, UpdateAddress).

```typescript
import { Command } from '@neurale/event-sourcing-ts';

// Command interface
interface PlaceOrderCommand extends Command {
  aggregateId: string;  // Order ID
  customerId: string;
  items: Array<{
    productId: string;
    quantity: number;
    price: number;
  }>;
}

interface CancelOrderCommand extends Command {
  aggregateId: string;  // Order ID
  reason: string;
}

interface ShipOrderCommand extends Command {
  aggregateId: string;  // Order ID
  trackingNumber: string;
  carrier: string;
}
```

### Step 2: Create Command Handlers

Command handlers contain the business logic for processing commands.

```typescript
import { CommandHandler, CommandResult } from '@neurale/event-sourcing-ts';

class PlaceOrderCommandHandler implements CommandHandler<PlaceOrderCommand, OrderEvent> {
  constructor(
    private repository: Repository<OrderAggregate, OrderEvent>
  ) {}

  async handle(command: PlaceOrderCommand): Promise<CommandResult<OrderEvent>> {
    try {
      // Validate command
      if (command.items.length === 0) {
        return {
          events: [],
          success: false,
          error: 'Order must have at least one item'
        };
      }

      // Create new aggregate
      const order = new OrderAggregate();
      order.place(command.aggregateId, command.customerId, command.items);

      // Save to repository
      await this.repository.save(order);

      // Return success with events
      return {
        events: order.getUncommittedEvents().map(e => e.event),
        success: true
      };
    } catch (error) {
      return {
        events: [],
        success: false,
        error: error instanceof Error ? error.message : 'Unknown error'
      };
    }
  }
}

class CancelOrderCommandHandler implements CommandHandler<CancelOrderCommand, OrderEvent> {
  constructor(
    private repository: Repository<OrderAggregate, OrderEvent>
  ) {}

  async handle(command: CancelOrderCommand): Promise<CommandResult<OrderEvent>> {
    try {
      // Load existing aggregate
      const order = await this.repository.load(command.aggregateId);
      
      if (!order) {
        return {
          events: [],
          success: false,
          error: `Order ${command.aggregateId} not found`
        };
      }

      // Execute business logic
      order.cancel(command.reason);

      // Save changes
      await this.repository.save(order);

      return {
        events: order.getUncommittedEvents().map(e => e.event),
        success: true
      };
    } catch (error) {
      return {
        events: [],
        success: false,
        error: error instanceof Error ? error.message : 'Unknown error'
      };
    }
  }
}
```

### Step 3: Setup CommandBus

The CommandBus routes commands to their handlers.

```typescript
import { InMemoryCommandBus } from '@neurale/event-sourcing-ts';

// Create command bus
const commandBus = new InMemoryCommandBus();

// Register handlers
commandBus.registerHandler(
  'PlaceOrderCommand',
  new PlaceOrderCommandHandler(orderRepository)
);

commandBus.registerHandler(
  'CancelOrderCommand',
  new CancelOrderCommandHandler(orderRepository)
);

commandBus.registerHandler(
  'ShipOrderCommand',
  new ShipOrderCommandHandler(orderRepository)
);
```

### Step 4: Send Commands

```typescript
// Place an order
const placeOrderCommand: PlaceOrderCommand = {
  aggregateId: 'order-123',
  customerId: 'customer-1',
  items: [
    { productId: 'prod-1', quantity: 2, price: 29.99 },
    { productId: 'prod-2', quantity: 1, price: 49.99 }
  ]
};

const result = await commandBus.send(placeOrderCommand);

if (result.success) {
  console.log('Order placed successfully');
  console.log('Events produced:', result.events.length);
} else {
  console.error('Failed to place order:', result.error);
}

// Cancel the order
const cancelCommand: CancelOrderCommand = {
  aggregateId: 'order-123',
  reason: 'Customer requested cancellation'
};

const cancelResult = await commandBus.send(cancelCommand);
```

### Command Validation

Add validation before processing:

```typescript
class PlaceOrderCommandHandler implements CommandHandler<PlaceOrderCommand, OrderEvent> {
  async handle(command: PlaceOrderCommand): Promise<CommandResult<OrderEvent>> {
    // Validate command
    const errors = this.validate(command);
    if (errors.length > 0) {
      return {
        events: [],
        success: false,
        error: errors.join(', ')
      };
    }

    // Process command...
  }

  private validate(command: PlaceOrderCommand): string[] {
    const errors: string[] = [];

    if (!command.aggregateId) {
      errors.push('Order ID is required');
    }

    if (!command.customerId) {
      errors.push('Customer ID is required');
    }

    if (command.items.length === 0) {
      errors.push('Order must have at least one item');
    }

    for (const item of command.items) {
      if (item.quantity <= 0) {
        errors.push(`Invalid quantity for product ${item.productId}`);
      }
      if (item.price < 0) {
        errors.push(`Invalid price for product ${item.productId}`);
      }
    }

    return errors;
  }
}
```

## 🔍 Query Side: Reading Data

Queries request information from read models (projections). They don't change state.

### Step 1: Define Queries

Queries should be named with question words (GetOrder, ListOrders, FindCustomer).

```typescript
import { Query } from '@neurale/event-sourcing-ts';

interface GetOrderQuery extends Query {
  orderId: string;
}

interface ListOrdersQuery extends Query {
  customerId?: string;
  status?: string;
  limit?: number;
  offset?: number;
}

interface GetOrderSummaryQuery extends Query {
  // No parameters - returns overall summary
}
```

### Step 2: Create Query Handlers

Query handlers retrieve data from projections.

```typescript
import { QueryHandler, QueryResult } from '@neurale/event-sourcing-ts';

class GetOrderQueryHandler implements QueryHandler<GetOrderQuery, Order> {
  constructor(private projection: OrderListProjection) {}

  async handle(query: GetOrderQuery): Promise<QueryResult<Order>> {
    try {
      const order = this.projection.getOrder(query.orderId);

      if (!order) {
        return {
          data: null as any,
          success: false,
          error: `Order ${query.orderId} not found`
        };
      }

      return {
        data: order,
        success: true
      };
    } catch (error) {
      return {
        data: null as any,
        success: false,
        error: error instanceof Error ? error.message : 'Unknown error'
      };
    }
  }

  canHandle(queryType: string): boolean {
    return queryType === 'GetOrderQuery';
  }
}

class ListOrdersQueryHandler implements QueryHandler<ListOrdersQuery, Order[]> {
  constructor(private projection: OrderListProjection) {}

  async handle(query: ListOrdersQuery): Promise<QueryResult<Order[]>> {
    try {
      let orders = this.projection.getAllOrders();

      // Apply filters
      if (query.customerId) {
        orders = orders.filter(o => o.customerId === query.customerId);
      }

      if (query.status) {
        orders = orders.filter(o => o.status === query.status);
      }

      // Apply pagination
      const offset = query.offset || 0;
      const limit = query.limit || 100;
      orders = orders.slice(offset, offset + limit);

      return {
        data: orders,
        success: true
      };
    } catch (error) {
      return {
        data: [],
        success: false,
        error: error instanceof Error ? error.message : 'Unknown error'
      };
    }
  }

  canHandle(queryType: string): boolean {
    return queryType === 'ListOrdersQuery';
  }
}
```

### Step 3: Setup QueryBus

```typescript
import { InMemoryQueryBus } from '@neurale/event-sourcing-ts';

// Create query bus
const queryBus = new InMemoryQueryBus();

// Register handlers
queryBus.registerHandler(
  'GetOrderQuery',
  new GetOrderQueryHandler(orderListProjection)
);

queryBus.registerHandler(
  'ListOrdersQuery',
  new ListOrdersQueryHandler(orderListProjection)
);

queryBus.registerHandler(
  'GetOrderSummaryQuery',
  new GetOrderSummaryQueryHandler(orderSummaryProjection)
);
```

### Step 4: Execute Queries

```typescript
// Get a single order
const getQuery: GetOrderQuery = {
  orderId: 'order-123'
};

const result = await queryBus.send<GetOrderQuery, Order>(getQuery);

if (result.success) {
  console.log('Order:', result.data);
  console.log('Status:', result.data.status);
} else {
  console.error('Query failed:', result.error);
}

// List orders for a customer
const listQuery: ListOrdersQuery = {
  customerId: 'customer-1',
  status: 'placed',
  limit: 10
};

const listResult = await queryBus.send<ListOrdersQuery, Order[]>(listQuery);

if (listResult.success) {
  console.log(`Found ${listResult.data.length} orders`);
  listResult.data.forEach(order => {
    console.log(`- ${order.id}: ${order.status}`);
  });
}
```

## 📊 Projection Side: Building Read Models

Projections listen to events and build optimized read models for queries.

### Step 1: Create Projections

Use `AutoDispatchProjection` for automatic event routing:

```typescript
import { AutoDispatchProjection, ProjectionHandler, EventEnvelope } from '@neurale/event-sourcing-ts';

class OrderListProjection extends AutoDispatchProjection<OrderEvent> {
  private orders: Map<string, Order> = new Map();

  getName(): string {
    return 'OrderListProjection';
  }

  getVersion(): number {
    return 1;
  }

  // Event handlers - automatically called by event type
  @ProjectionHandler('OrderPlaced')
  async onOrderPlaced(envelope: EventEnvelope<OrderPlaced>): Promise<void> {
    const event = envelope.event;
    this.orders.set(event.orderId, {
      id: event.orderId,
      customerId: event.customerId,
      items: event.items,
      status: 'placed',
      totalAmount: this.calculateTotal(event.items),
      createdAt: envelope.metadata.timestamp,
      updatedAt: envelope.metadata.timestamp
    });
  }

  @ProjectionHandler('OrderShipped')
  async onOrderShipped(envelope: EventEnvelope<OrderShipped>): Promise<void> {
    const order = this.orders.get(envelope.event.orderId);
    if (order) {
      order.status = 'shipped';
      order.trackingNumber = envelope.event.trackingNumber;
      order.shippedAt = envelope.metadata.timestamp;
      order.updatedAt = envelope.metadata.timestamp;
    }
  }

  @ProjectionHandler('OrderCancelled')
  async onOrderCancelled(envelope: EventEnvelope<OrderCancelled>): Promise<void> {
    const order = this.orders.get(envelope.event.orderId);
    if (order) {
      order.status = 'cancelled';
      order.cancellationReason = envelope.event.reason;
      order.cancelledAt = envelope.metadata.timestamp;
      order.updatedAt = envelope.metadata.timestamp;
    }
  }

  // Query methods
  getOrder(orderId: string): Order | undefined {
    return this.orders.get(orderId);
  }

  getAllOrders(): Order[] {
    return Array.from(this.orders.values());
  }

  getOrdersByCustomer(customerId: string): Order[] {
    return Array.from(this.orders.values())
      .filter(order => order.customerId === customerId);
  }

  getOrdersByStatus(status: string): Order[] {
    return Array.from(this.orders.values())
      .filter(order => order.status === status);
  }

  private calculateTotal(items: Array<{ price: number; quantity: number }>): number {
    return items.reduce((sum, item) => sum + item.price * item.quantity, 0);
  }
}

class OrderSummaryProjection extends AutoDispatchProjection<OrderEvent> {
  private summary = {
    totalOrders: 0,
    totalRevenue: 0,
    ordersByStatus: {} as Record<string, number>
  };

  getName(): string {
    return 'OrderSummaryProjection';
  }

  getVersion(): number {
    return 1;
  }

  @ProjectionHandler('OrderPlaced')
  async onOrderPlaced(envelope: EventEnvelope<OrderPlaced>): Promise<void> {
    this.summary.totalOrders++;
    const total = envelope.event.items.reduce(
      (sum, item) => sum + item.price * item.quantity,
      0
    );
    this.summary.totalRevenue += total;
    this.incrementStatus('placed');
  }

  @ProjectionHandler('OrderShipped')
  async onOrderShipped(envelope: EventEnvelope<OrderShipped>): Promise<void> {
    this.decrementStatus('placed');
    this.incrementStatus('shipped');
  }

  @ProjectionHandler('OrderCancelled')
  async onOrderCancelled(envelope: EventEnvelope<OrderCancelled>): Promise<void> {
    this.decrementStatus('placed');
    this.incrementStatus('cancelled');
  }

  getSummary() {
    return { ...this.summary };
  }

  private incrementStatus(status: string): void {
    this.summary.ordersByStatus[status] = 
      (this.summary.ordersByStatus[status] || 0) + 1;
  }

  private decrementStatus(status: string): void {
    this.summary.ordersByStatus[status] = 
      Math.max(0, (this.summary.ordersByStatus[status] || 0) - 1);
  }
}
```

### Step 2: Setup ProjectionManager

```typescript
import { InMemoryProjectionManager } from '@neurale/event-sourcing-ts';

// Create projection manager
const projectionManager = new InMemoryProjectionManager();

// Create and register projections
const orderListProjection = new OrderListProjection();
const orderSummaryProjection = new OrderSummaryProjection();

projectionManager.register(orderListProjection);
projectionManager.register(orderSummaryProjection);

// Start processing
await projectionManager.start();
```

### Step 3: Process Events

Events from aggregates are automatically processed by projections:

```typescript
// After saving an aggregate, process its events through projections
const order = new OrderAggregate();
order.place('order-123', 'customer-1', items);

await orderRepository.save(order);

// Process events through projections
const events = order.getUncommittedEvents();
for (const envelope of events) {
  await projectionManager.processEvent(envelope);
}
```

## 🔄 Complete Integration Example

Here's how all the pieces work together:

```typescript
import {
  InMemoryCommandBus,
  InMemoryQueryBus,
  InMemoryProjectionManager,
  EventStoreClientFactory,
  RepositoryFactory,
  EventSerializer
} from '@neurale/event-sourcing-ts';

// 1. Setup Event Store and Repository
const eventStoreClient = EventStoreClientFactory.createMemoryClient();
await eventStoreClient.connect();

const serializer = new EventSerializer();
serializer.registerEvent(OrderPlaced);
serializer.registerEvent(OrderShipped);
serializer.registerEvent(OrderCancelled);

const repositoryFactory = new RepositoryFactory(eventStoreClient);
const orderRepository = repositoryFactory.create(
  OrderAggregate,
  'Order',
  serializer
);

// 2. Setup Projections
const projectionManager = new InMemoryProjectionManager();
const orderListProjection = new OrderListProjection();
const orderSummaryProjection = new OrderSummaryProjection();

projectionManager.register(orderListProjection);
projectionManager.register(orderSummaryProjection);
await projectionManager.start();

// 3. Setup Command Bus
const commandBus = new InMemoryCommandBus();
commandBus.registerHandler(
  'PlaceOrderCommand',
  new PlaceOrderCommandHandler(orderRepository, projectionManager)
);
commandBus.registerHandler(
  'CancelOrderCommand',
  new CancelOrderCommandHandler(orderRepository, projectionManager)
);

// 4. Setup Query Bus
const queryBus = new InMemoryQueryBus();
queryBus.registerHandler(
  'GetOrderQuery',
  new GetOrderQueryHandler(orderListProjection)
);
queryBus.registerHandler(
  'ListOrdersQuery',
  new ListOrdersQueryHandler(orderListProjection)
);
queryBus.registerHandler(
  'GetOrderSummaryQuery',
  new GetOrderSummaryQueryHandler(orderSummaryProjection)
);

// 5. Use the system
async function placeAndQueryOrder() {
  // Send command
  const placeResult = await commandBus.send({
    aggregateId: 'order-123',
    customerId: 'customer-1',
    items: [{ productId: 'prod-1', quantity: 2, price: 29.99 }]
  } as PlaceOrderCommand);

  if (!placeResult.success) {
    console.error('Failed to place order:', placeResult.error);
    return;
  }

  console.log('Order placed successfully');

  // Query the order
  const queryResult = await queryBus.send({
    orderId: 'order-123'
  } as GetOrderQuery);

  if (queryResult.success) {
    console.log('Order details:', queryResult.data);
  }

  // Query summary
  const summaryResult = await queryBus.send({} as GetOrderSummaryQuery);
  if (summaryResult.success) {
    console.log('Order summary:', summaryResult.data);
  }
}

await placeAndQueryOrder();
```

## 🧪 Testing CQRS Components

### Testing Command Handlers

```typescript
describe('PlaceOrderCommandHandler', () => {
  let handler: PlaceOrderCommandHandler;
  let repository: Repository<OrderAggregate, OrderEvent>;

  beforeEach(() => {
    const memoryStore = new MemoryEventStoreClient();
    const serializer = new EventSerializer();
    serializer.registerEvent(OrderPlaced);
    
    const factory = new RepositoryFactory(memoryStore);
    repository = factory.create(OrderAggregate, 'Order', serializer);
    
    handler = new PlaceOrderCommandHandler(repository);
  });

  it('should place an order successfully', async () => {
    const command: PlaceOrderCommand = {
      aggregateId: 'order-123',
      customerId: 'customer-1',
      items: [{ productId: 'prod-1', quantity: 2, price: 29.99 }]
    };

    const result = await handler.handle(command);

    expect(result.success).toBe(true);
    expect(result.events).toHaveLength(1);
    expect(result.events[0]).toBeInstanceOf(OrderPlaced);
  });

  it('should reject orders with no items', async () => {
    const command: PlaceOrderCommand = {
      aggregateId: 'order-123',
      customerId: 'customer-1',
      items: []
    };

    const result = await handler.handle(command);

    expect(result.success).toBe(false);
    expect(result.error).toContain('at least one item');
  });
});
```

### Testing Query Handlers

```typescript
describe('GetOrderQueryHandler', () => {
  let handler: GetOrderQueryHandler;
  let projection: OrderListProjection;

  beforeEach(() => {
    projection = new OrderListProjection();
    handler = new GetOrderQueryHandler(projection);
  });

  it('should return order when it exists', async () => {
    // Setup projection with test data
    await projection.onOrderPlaced({
      event: new OrderPlaced('order-123', 'customer-1', []),
      metadata: { timestamp: new Date(), /* ... */ }
    } as EventEnvelope<OrderPlaced>);

    const query: GetOrderQuery = { orderId: 'order-123' };
    const result = await handler.handle(query);

    expect(result.success).toBe(true);
    expect(result.data.id).toBe('order-123');
  });

  it('should return error when order not found', async () => {
    const query: GetOrderQuery = { orderId: 'nonexistent' };
    const result = await handler.handle(query);

    expect(result.success).toBe(false);
    expect(result.error).toContain('not found');
  });
});
```

### Testing Projections

```typescript
describe('OrderListProjection', () => {
  let projection: OrderListProjection;

  beforeEach(() => {
    projection = new OrderListProjection();
  });

  it('should add order when OrderPlaced', async () => {
    const event = new OrderPlaced('order-123', 'customer-1', [
      { productId: 'prod-1', quantity: 2, price: 29.99 }
    ]);

    await projection.onOrderPlaced({
      event,
      metadata: { timestamp: new Date(), /* ... */ }
    } as EventEnvelope<OrderPlaced>);

    const order = projection.getOrder('order-123');
    expect(order).toBeDefined();
    expect(order?.status).toBe('placed');
    expect(order?.totalAmount).toBe(59.98);
  });

  it('should update status when OrderShipped', async () => {
    // Place order first
    await projection.onOrderPlaced({
      event: new OrderPlaced('order-123', 'customer-1', []),
      metadata: { timestamp: new Date(), /* ... */ }
    } as EventEnvelope<OrderPlaced>);

    // Ship order
    await projection.onOrderShipped({
      event: new OrderShipped('order-123', 'TRACK-123', 'UPS'),
      metadata: { timestamp: new Date(), /* ... */ }
    } as EventEnvelope<OrderShipped>);

    const order = projection.getOrder('order-123');
    expect(order?.status).toBe('shipped');
    expect(order?.trackingNumber).toBe('TRACK-123');
  });
});
```

## 🎯 Best Practices

### Command Design

1. **Make commands explicit** - Each command should have a clear purpose
2. **Include all necessary data** - Commands should be self-contained
3. **Validate early** - Validate commands before processing
4. **Use value objects** - Encapsulate complex data in value objects
5. **Keep commands immutable** - Commands should not change after creation

### Query Design

1. **Design for specific use cases** - Each query should serve a specific need
2. **Return DTOs** - Don't expose domain models directly
3. **Support pagination** - Always support paging for list queries
4. **Add filtering** - Allow clients to filter results
5. **Keep queries simple** - Complex queries indicate missing projections

### Projection Design

1. **One projection per view** - Each UI view should have its own projection
2. **Denormalize aggressively** - Optimize for read performance
3. **Handle all relevant events** - Don't miss events that affect the view
4. **Make projections idempotent** - Handle duplicate events gracefully
5. **Version projections** - Track schema versions for migrations

### Integration Patterns

1. **Process events immediately** - Update projections right after commands
2. **Handle failures gracefully** - Retry failed projection updates
3. **Monitor projection lag** - Track how far behind projections are
4. **Use eventual consistency** - Accept that reads may be slightly stale
5. **Provide feedback** - Return command results to users

### Performance Tips

1. **Cache projection results** - Cache frequently accessed data
2. **Batch event processing** - Process multiple events together
3. **Use indexes** - Index projection data for fast queries
4. **Separate hot and cold data** - Archive old data
5. **Monitor query performance** - Track slow queries

## 🔗 Next Steps

- **[API Reference](../api-reference.md)** - Detailed API documentation
- **[TypeScript SDK Guide](./typescript-sdk.md)** - Complete SDK guide
- **[SDK Overview](../overview/sdk-overview.md)** - Architecture and patterns

## 📚 Additional Resources

- **Event Sourcing Patterns** - Martin Fowler's event sourcing guide
- **CQRS Journey** - Microsoft's CQRS patterns and practices
- **Domain-Driven Design** - Eric Evans' DDD book
