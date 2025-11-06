# SDK Overview

The Event Sourcing SDK provides high-level abstractions for building event-sourced applications. It sits on top of the Event Store and provides developer-friendly APIs for implementing domain-driven design patterns.

## 🏗️ Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Your Application                          │
│  ┌────────────┐  ┌────────────┐  ┌────────────┐            │
│  │ Aggregates │  │ Commands   │  │ Projections│            │
│  └────────────┘  └────────────┘  └────────────┘            │
└─────────────────────────────────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│              Event Sourcing SDK (This Layer)                 │
│  ┌────────────┐  ┌────────────┐  ┌────────────┐            │
│  │ Repository │  │ Serializer │  │ Event Bus  │            │
│  └────────────┘  └────────────┘  └────────────┘            │
└─────────────────────────────────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│                    Event Store                               │
│  ┌────────────┐  ┌────────────┐  ┌────────────┐            │
│  │ gRPC API   │  │ PostgreSQL │  │ Streams    │            │
│  └────────────┘  └────────────┘  └────────────┘            │
└─────────────────────────────────────────────────────────────┘
```

## 🎯 Core Abstractions

### Aggregates
**Purpose:** Encapsulate business logic and maintain consistency boundaries

**Key Features:**
- Event sourcing lifecycle management
- Optimistic concurrency control
- Automatic event application
- State rehydration from history

**When to Use:**
- Modeling business entities with complex behavior
- Enforcing business rules and invariants
- Maintaining transactional consistency

### Repositories
**Purpose:** Persist and retrieve aggregates using the event store

**Key Features:**
- Load aggregates by ID
- Save uncommitted events
- Handle concurrency conflicts
- Event serialization/deserialization

**When to Use:**
- Saving aggregate changes
- Loading aggregate state
- Managing persistence concerns

### Events
**Purpose:** Represent immutable facts about what happened

**Key Features:**
- Strongly-typed event classes
- Automatic serialization
- Metadata enrichment
- Event registration

**When to Use:**
- Capturing state changes
- Building audit trails
- Enabling event-driven architecture

### Projections
**Purpose:** Build read models from event streams

**Key Features:**
- Subscribe to event streams
- Maintain denormalized views
- Support multiple read models
- Reset and rebuild capability

**When to Use:**
- Creating optimized query models
- Building analytics and reports
- Implementing CQRS patterns

### Commands
**Purpose:** Represent intentions to change state

**Key Features:**
- Strongly-typed command classes
- Command handler pattern
- Validation and authorization hooks

**When to Use:**
- Modeling user intentions
- Implementing use cases
- Separating reads from writes (CQRS)

### Queries
**Purpose:** Request information from read models

**Key Features:**
- Strongly-typed query classes
- Query handler pattern
- Result metadata

**When to Use:**
- Querying projections
- Implementing CQRS read side
- Building APIs

## 🔄 Typical Workflow

### 1. Define Events

```typescript
class OrderPlaced extends BaseDomainEvent {
  constructor(
    public readonly orderId: string,
    public readonly customerId: string,
    public readonly items: LineItem[]
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
```

### 2. Create Aggregate

```typescript
class OrderAggregate extends AggregateRoot<OrderEvent> {
  private status: string = 'new';
  private customerId: string = '';
  private items: LineItem[] = [];

  getAggregateType(): string {
    return 'Order';
  }

  // Command: Place order
  place(orderId: string, customerId: string, items: LineItem[]) {
    // Validate business rules
    if (items.length === 0) {
      throw new Error('Order must have at least one item');
    }

    // Initialize and raise event
    this.initialize(orderId);
    this.raiseEvent(new OrderPlaced(orderId, customerId, items));
  }

  // Command: Ship order
  ship(trackingNumber: string) {
    if (this.status !== 'placed') {
      throw new Error('Can only ship placed orders');
    }
    this.raiseEvent(new OrderShipped(this.id!, trackingNumber));
  }

  // Event handlers
  @EventSourcingHandler(OrderPlaced)
  onOrderPlaced(event: OrderPlaced): void {
    this.status = 'placed';
    this.customerId = event.customerId;
    this.items = event.items;
  }

  @EventSourcingHandler(OrderShipped)
  onOrderShipped(event: OrderShipped): void {
    this.status = 'shipped';
  }
}
```

### 3. Setup Repository

```typescript
// Register events
const serializer = new EventSerializer();
serializer.registerEvent(OrderPlaced);
serializer.registerEvent(OrderShipped);

// Create event store client
const client = EventStoreClientFactory.createGrpcClient({
  endpoint: 'localhost:50051'
});

// Create repository
const factory = new RepositoryFactory(client);
const orderRepository = factory.create(
  OrderAggregate,
  'Order',
  serializer
);
```

### 4. Process Commands

```typescript
// Create new order
const order = new OrderAggregate();
order.place('order-123', 'customer-1', [
  { productId: 'prod-1', quantity: 2, price: 29.99 }
]);
await orderRepository.save(order);

// Load and ship order
const loaded = await orderRepository.load('order-123');
if (loaded) {
  loaded.ship('TRACK-123');
  await orderRepository.save(loaded);
}
```

### 5. Build Projections

```typescript
class OrderSummaryProjection implements Projection<OrderSummary> {
  private summary: OrderSummary = {
    totalOrders: 0,
    totalRevenue: 0,
    ordersByStatus: {}
  };

  async handleEvent(event: DomainEvent): Promise<void> {
    if (event instanceof OrderPlaced) {
      this.summary.totalOrders++;
      this.summary.totalRevenue += this.calculateTotal(event.items);
      this.incrementStatus('placed');
    } else if (event instanceof OrderShipped) {
      this.decrementStatus('placed');
      this.incrementStatus('shipped');
    }
  }

  getState(): OrderSummary {
    return this.summary;
  }

  async reset(): Promise<void> {
    this.summary = {
      totalOrders: 0,
      totalRevenue: 0,
      ordersByStatus: {}
    };
  }

  private calculateTotal(items: LineItem[]): number {
    return items.reduce((sum, item) => sum + item.price * item.quantity, 0);
  }

  private incrementStatus(status: string): void {
    this.summary.ordersByStatus[status] = 
      (this.summary.ordersByStatus[status] || 0) + 1;
  }

  private decrementStatus(status: string): void {
    this.summary.ordersByStatus[status] = 
      (this.summary.ordersByStatus[status] || 0) - 1;
  }
}
```

## 🎨 Design Patterns

### Command Pattern
Separate command definition from execution:

```typescript
interface PlaceOrderCommand extends Command {
  orderId: string;
  customerId: string;
  items: LineItem[];
}

class PlaceOrderCommandHandler implements CommandHandler<PlaceOrderCommand, OrderAggregate> {
  constructor(private repository: Repository<OrderAggregate, OrderEvent>) {}

  async handle(command: PlaceOrderCommand): Promise<void> {
    const order = new OrderAggregate();
    order.place(command.orderId, command.customerId, command.items);
    await this.repository.save(order);
  }
}
```

### CQRS Pattern
Separate write model (aggregates) from read model (projections):

```typescript
// Write side: Commands → Aggregates → Events
await commandHandler.handle(new PlaceOrderCommand(...));

// Read side: Queries → Projections → Results
const summary = await queryHandler.handle(new GetOrderSummaryQuery());
```

### Event Bus Pattern
Decouple aggregates using event-driven communication:

```typescript
eventBus.subscribe(OrderPlaced, async (event) => {
  // Send notification
  await notificationService.sendOrderConfirmation(event);
  
  // Update inventory
  await inventoryService.reserveItems(event.items);
});
```

## 🚀 Best Practices

### 1. Keep Aggregates Small
- Focus on a single consistency boundary
- Avoid loading multiple aggregates in a transaction
- Use event-driven communication between aggregates

### 2. Design Events Carefully
- Events are immutable and permanent
- Use past tense naming (OrderPlaced, not PlaceOrder)
- Include all necessary data in events
- Version events for schema evolution

### 3. Handle Concurrency
- Always expect and handle ConcurrencyError
- Implement retry logic with exponential backoff
- Consider eventual consistency patterns

### 4. Test Thoroughly
- Use in-memory store for fast unit tests
- Test aggregate behavior with given-when-then pattern
- Test projections with event replay
- Test concurrency scenarios

### 5. Monitor and Observe
- Track event processing latency
- Monitor projection lag
- Alert on concurrency conflicts
- Log command execution

## 📚 Learn More

- **[API Reference](../api-reference.md)** - Complete API documentation
- **[TypeScript Guide](../typescript/typescript-sdk.md)** - TypeScript-specific guide


