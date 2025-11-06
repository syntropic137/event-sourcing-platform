# Event Sourcing SDK API Reference

This document provides a comprehensive reference for the Event Sourcing SDK APIs across all supported languages.

## 📋 Common API Surface

The Event Sourcing SDK provides high-level abstractions for building event-sourced applications. It builds on top of the Event Store to provide developer-friendly APIs for aggregates, commands, events, projections, and repositories.

### 🏗️ Core Concepts

#### **Aggregate**
A consistency boundary that processes commands and emits events. Aggregates maintain state by replaying their event history.

#### **Event**
An immutable fact about something that happened in the domain. Events are the source of truth.

#### **Repository**
Persistence abstraction for loading and saving aggregates using the event store.

#### **Projection**
Read model builder that subscribes to events and maintains denormalized views.

#### **Command**
An intention to change state, processed by aggregates to produce events.

#### **Query**
A request for information, handled by projections or read models.

---

## 📦 TypeScript SDK

### Installation

```bash
npm install @neurale/event-sourcing-ts
# or
pnpm add @neurale/event-sourcing-ts
```

### Client Setup

```typescript
import { EventStoreClientFactory, GrpcEventStoreAdapter } from '@neurale/event-sourcing-ts';

// Using gRPC Event Store
const client = EventStoreClientFactory.createGrpcClient({
  endpoint: 'localhost:50051',
  credentials: {
    username: 'user',
    password: 'pass'
  }
});

// Using in-memory store (for testing)
const memoryClient = EventStoreClientFactory.createMemoryClient();
```

---

## 🎯 Aggregate API

### `BaseAggregate<TEvent>`

Base class for event-sourced aggregates.

#### Properties

- **`id: AggregateId | null`** - The aggregate's unique identifier
- **`version: Version`** - Current version for optimistic concurrency control

#### Methods

##### `initialize(id: AggregateId): void`
Initialize a brand-new aggregate instance with an identifier. Must be called before raising the first event.

```typescript
class OrderAggregate extends BaseAggregate<OrderEvent> {
  place(orderId: string, customerId: string) {
    this.initialize(orderId); // Initialize before first event
    this.raiseEvent(new OrderPlaced(orderId, customerId));
  }
}
```

##### `raiseEvent(event: TEvent): void`
Raise a new domain event. The event will be applied immediately and added to uncommitted events.

```typescript
this.raiseEvent(new OrderPlaced(orderId, customerId));
```

##### `applyEvent(event: TEvent): void`
Abstract method to apply an event to update aggregate state. Must be implemented by subclasses.

```typescript
applyEvent(event: OrderEvent): void {
  if (event instanceof OrderPlaced) {
    this.status = 'placed';
    this.customerId = event.customerId;
  }
}
```

##### `getUncommittedEvents(): EventEnvelope<TEvent>[]`
Get all uncommitted events that haven't been persisted yet.

##### `markEventsAsCommitted(): void`
Mark all events as committed. Called by repository after successful persistence.

##### `hasUncommittedEvents(): boolean`
Check if the aggregate has any uncommitted events.

##### `rehydrate(events: EventEnvelope<TEvent>[]): void`
Rehydrate the aggregate from committed event history.

##### `getAggregateType(): string`
Abstract method to return the aggregate type name. Must be implemented by subclasses.

```typescript
getAggregateType(): string {
  return 'Order';
}
```

### `AggregateRoot<TEvent>`

Enhanced aggregate that automatically dispatches events to handler methods based on event type.

```typescript
import { AggregateRoot, EventSourcingHandler } from '@neurale/event-sourcing-ts';

class OrderAggregate extends AggregateRoot<OrderEvent> {
  private status: string = 'new';
  private customerId: string = '';

  getAggregateType(): string {
    return 'Order';
  }

  // Command method
  place(orderId: string, customerId: string) {
    this.initialize(orderId);
    this.raiseEvent(new OrderPlaced(orderId, customerId));
  }

  // Event handler - automatically called when OrderPlaced is applied
  @EventSourcingHandler(OrderPlaced)
  onOrderPlaced(event: OrderPlaced): void {
    this.status = 'placed';
    this.customerId = event.customerId;
  }
}
```

#### Decorators

##### `@EventSourcingHandler(EventClass)`
Decorator to mark a method as an event handler. The method will be automatically called when the specified event type is applied.

```typescript
@EventSourcingHandler(OrderPlaced)
onOrderPlaced(event: OrderPlaced): void {
  // Update aggregate state
}
```

---

## 📝 Event API

### `BaseDomainEvent`

Base class for domain events.

```typescript
import { BaseDomainEvent } from '@neurale/event-sourcing-ts';

class OrderPlaced extends BaseDomainEvent {
  constructor(
    public readonly orderId: string,
    public readonly customerId: string
  ) {
    super();
  }
}
```

### `EventSerializer`

Handles event serialization and deserialization.

#### Methods

##### `registerEvent(eventClass: EventClass): void`
Register an event class for serialization/deserialization.

```typescript
import { EventSerializer } from '@neurale/event-sourcing-ts';

const serializer = new EventSerializer();
serializer.registerEvent(OrderPlaced);
serializer.registerEvent(OrderShipped);
```

##### `serialize(event: DomainEvent): string`
Serialize an event to JSON string.

##### `deserialize(eventType: string, data: string): DomainEvent`
Deserialize an event from JSON string.

### `EventFactory`

Factory for creating event instances.

```typescript
import { EventFactory } from '@neurale/event-sourcing-ts';

const event = EventFactory.create(
  'OrderPlaced',
  { orderId: '123', customerId: 'cust-1' }
);
```

### Event Metadata

Events are wrapped in envelopes with metadata:

```typescript
interface EventEnvelope<TEvent extends DomainEvent> {
  event: TEvent;
  metadata: EventMetadata;
}

interface EventMetadata {
  eventId: string;
  eventType: string;
  aggregateId: string;
  aggregateType: string;
  aggregateVersion: number;
  timestamp: Date;
  correlationId?: string;
  causationId?: string;
}
```

---

## 🗄️ Repository API

### `Repository<TAggregate, TEvent>`

Interface for aggregate persistence.

```typescript
interface Repository<TAggregate extends Aggregate<TEvent>, TEvent extends DomainEvent> {
  load(aggregateId: AggregateId): Promise<TAggregate | null>;
  save(aggregate: TAggregate): Promise<void>;
}
```

#### Methods

##### `load(aggregateId: AggregateId): Promise<TAggregate | null>`
Load an aggregate by ID. Returns null if the aggregate doesn't exist.

```typescript
const order = await repository.load('order-123');
if (order) {
  // Process the order
}
```

##### `save(aggregate: TAggregate): Promise<void>`
Save an aggregate's uncommitted events to the event store.

```typescript
const order = new OrderAggregate();
order.place('order-123', 'customer-1');
await repository.save(order);
```

### `RepositoryFactory`

Factory for creating repository instances.

```typescript
import { RepositoryFactory } from '@neurale/event-sourcing-ts';

const factory = new RepositoryFactory(eventStoreClient);

const orderRepository = factory.create(
  OrderAggregate,
  'Order',
  serializer
);
```

#### Methods

##### `create<TAggregate, TEvent>(aggregateClass, aggregateType, serializer): Repository<TAggregate, TEvent>`
Create a repository instance for a specific aggregate type.

**Parameters:**
- `aggregateClass`: Constructor function for the aggregate
- `aggregateType`: String identifier for the aggregate type
- `serializer`: EventSerializer instance with registered events

---

## 📊 Projection API

### `Projection<TState>`

Base interface for projections that build read models from events.

```typescript
interface Projection<TState = any> {
  handleEvent(event: DomainEvent, metadata: EventMetadata): Promise<void>;
  getState(): TState;
  reset(): Promise<void>;
}
```

#### Methods

##### `handleEvent(event: DomainEvent, metadata: EventMetadata): Promise<void>`
Process an event and update the projection state.

```typescript
class OrderSummaryProjection implements Projection<OrderSummary> {
  private summary: OrderSummary = { totalOrders: 0, totalRevenue: 0 };

  async handleEvent(event: DomainEvent): Promise<void> {
    if (event instanceof OrderPlaced) {
      this.summary.totalOrders++;
      this.summary.totalRevenue += event.amount;
    }
  }

  getState(): OrderSummary {
    return this.summary;
  }

  async reset(): Promise<void> {
    this.summary = { totalOrders: 0, totalRevenue: 0 };
  }
}
```

##### `getState(): TState`
Get the current projection state.

##### `reset(): Promise<void>`
Reset the projection to its initial state.

---

## 🎯 Command API

### `Command`

Interface for commands that represent intentions to change state.

```typescript
interface Command {
  readonly commandId: string;
  readonly aggregateId: AggregateId;
  readonly timestamp: Date;
}
```

### `CommandHandler<TCommand, TAggregate>`

Interface for command handlers that process commands and update aggregates.

```typescript
interface CommandHandler<TCommand extends Command, TAggregate extends Aggregate> {
  handle(command: TCommand, aggregate: TAggregate): Promise<void>;
}
```

#### Example

```typescript
class PlaceOrderCommandHandler implements CommandHandler<PlaceOrderCommand, OrderAggregate> {
  async handle(command: PlaceOrderCommand, aggregate: OrderAggregate): Promise<void> {
    aggregate.place(command.orderId, command.customerId, command.items);
  }
}
```

### `CommandBus`

Interface for routing commands to their handlers. Provides centralized command dispatching with error handling.

```typescript
interface CommandBus {
  registerHandler<TCommand extends Command>(
    commandType: string,
    handler: CommandHandler<TCommand>
  ): void;
  
  send<TCommand extends Command>(command: TCommand): Promise<CommandResult>;
}
```

#### Methods

##### `registerHandler(commandType, handler): void`
Register a command handler for a specific command type.

##### `send(command): Promise<CommandResult>`
Send a command for processing. Returns a result indicating success or failure.

### `InMemoryCommandBus`

In-memory implementation of CommandBus for local command routing.

```typescript
import { InMemoryCommandBus } from '@neurale/event-sourcing-ts';

// Create command bus
const commandBus = new InMemoryCommandBus();

// Register handlers
commandBus.registerHandler('PlaceOrderCommand', new PlaceOrderCommandHandler(repository));
commandBus.registerHandler('CancelOrderCommand', new CancelOrderCommandHandler(repository));

// Send commands
const result = await commandBus.send(new PlaceOrderCommand({
  aggregateId: 'order-123',
  customerId: 'customer-1',
  items: [{ productId: 'prod-1', quantity: 2 }]
}));

if (result.success) {
  console.log('Command processed successfully');
  console.log('Events produced:', result.events);
} else {
  console.error('Command failed:', result.error);
}
```

#### Error Handling

The command bus automatically catches errors and returns them in the result:

```typescript
const result = await commandBus.send(command);

if (!result.success) {
  // Handle error
  switch (result.error) {
    case 'No handler registered':
      // Register missing handler
      break;
    case 'Validation failed':
      // Handle validation error
      break;
    default:
      // Handle other errors
  }
}
```

### `CommandResult<TEvent>`

Result of command execution with events and status.

```typescript
interface CommandResult<TEvent extends DomainEvent = DomainEvent> {
  readonly events: TEvent[];
  readonly success: boolean;
  readonly error?: string;
}
```

---

## 🔍 Query API

### `Query<TResult>`

Interface for queries that request information.

```typescript
interface Query<TResult = any> {
  readonly queryId: string;
  readonly timestamp: Date;
}
```

### `QueryHandler<TQuery, TResult>`

Interface for query handlers that process queries and return results.

```typescript
interface QueryHandler<TQuery extends Query<TResult>, TResult = any> {
  handle(query: TQuery): Promise<QueryResult<TResult>>;
}
```

### `QueryResult<TData>`

Wrapper for query results with metadata.

```typescript
interface QueryResult<TData = any> {
  data: TData;
  metadata: {
    queryId: string;
    executedAt: Date;
    executionTimeMs: number;
  };
}
```

#### Example

```typescript
class GetOrderSummaryQuery implements Query<OrderSummary> {
  readonly queryId: string = uuid();
  readonly timestamp: Date = new Date();
}

class GetOrderSummaryQueryHandler implements QueryHandler<GetOrderSummaryQuery, OrderSummary> {
  constructor(private projection: OrderSummaryProjection) {}

  async handle(query: GetOrderSummaryQuery): Promise<QueryResult<OrderSummary>> {
    const startTime = Date.now();
    const data = this.projection.getState();
    
    return {
      data,
      metadata: {
        queryId: query.queryId,
        executedAt: new Date(),
        executionTimeMs: Date.now() - startTime
      }
    };
  }
}
```

### `QueryBus`

Interface for routing queries to their handlers. Provides centralized query dispatching.

```typescript
interface QueryBus {
  registerHandler<TQuery extends Query>(
    queryType: string,
    handler: QueryHandler<TQuery>
  ): void;
  
  send<TQuery extends Query, TResult = unknown>(
    query: TQuery
  ): Promise<QueryResult<TResult>>;
}
```

#### Methods

##### `registerHandler(queryType, handler): void`
Register a query handler for a specific query type.

##### `send(query): Promise<QueryResult<TResult>>`
Execute a query and return the result.

### `InMemoryQueryBus`

In-memory implementation of QueryBus for local query routing.

```typescript
import { InMemoryQueryBus } from '@neurale/event-sourcing-ts';

// Create query bus
const queryBus = new InMemoryQueryBus();

// Register handlers
queryBus.registerHandler('GetOrderQuery', new GetOrderQueryHandler(projection));
queryBus.registerHandler('ListOrdersQuery', new ListOrdersQueryHandler(projection));

// Execute queries
const result = await queryBus.send(new GetOrderQuery({ orderId: 'order-123' }));

if (result.success) {
  console.log('Query result:', result.data);
} else {
  console.error('Query failed:', result.error);
}
```

#### Query Result Pattern

Queries always return a structured result with success status:

```typescript
const result = await queryBus.send(query);

if (result.success) {
  // Access data
  const orders = result.data;
  processOrders(orders);
} else {
  // Handle error
  console.error('Query failed:', result.error);
}
```

---

## 📊 Projection API (Advanced)

### `ProjectionManager`

Manages multiple projections and coordinates event processing across them.

```typescript
interface ProjectionManager {
  register<TEvent extends DomainEvent>(projection: Projection<TEvent>): void;
  processEvent<TEvent extends DomainEvent>(event: EventEnvelope<TEvent>): Promise<void>;
  start(): Promise<void>;
  stop(): Promise<void>;
}
```

#### Methods

##### `register(projection): void`
Register a projection for event processing.

##### `processEvent(event): Promise<void>`
Process an event through all registered projections.

##### `start(): Promise<void>`
Start the projection manager and begin processing events.

##### `stop(): Promise<void>`
Stop the projection manager.

### `InMemoryProjectionManager`

In-memory implementation for managing projections.

```typescript
import { InMemoryProjectionManager } from '@neurale/event-sourcing-ts';

// Create projection manager
const projectionManager = new InMemoryProjectionManager();

// Register projections
projectionManager.register(new OrderListProjection());
projectionManager.register(new OrderSummaryProjection());
projectionManager.register(new SalesReportProjection());

// Start processing
await projectionManager.start();

// Process events
await projectionManager.processEvent(eventEnvelope);

// Stop when done
await projectionManager.stop();
```

### `AutoDispatchProjection<TEvent>`

Base class for projections with automatic event routing using decorators.

```typescript
import { AutoDispatchProjection, ProjectionHandler } from '@neurale/event-sourcing-ts';

class OrderListProjection extends AutoDispatchProjection<OrderEvent> {
  private orders: Map<string, Order> = new Map();

  getName(): string {
    return 'OrderListProjection';
  }

  getVersion(): number {
    return 1;
  }

  // Automatically called when OrderPlaced event is processed
  @ProjectionHandler('OrderPlaced')
  async onOrderPlaced(envelope: EventEnvelope<OrderPlaced>): Promise<void> {
    const event = envelope.event;
    this.orders.set(event.orderId, {
      id: event.orderId,
      customerId: event.customerId,
      status: 'placed',
      createdAt: envelope.metadata.timestamp
    });
  }

  // Automatically called when OrderShipped event is processed
  @ProjectionHandler('OrderShipped')
  async onOrderShipped(envelope: EventEnvelope<OrderShipped>): Promise<void> {
    const order = this.orders.get(envelope.event.orderId);
    if (order) {
      order.status = 'shipped';
      order.shippedAt = envelope.metadata.timestamp;
    }
  }

  // Query methods
  getOrder(orderId: string): Order | undefined {
    return this.orders.get(orderId);
  }

  getAllOrders(): Order[] {
    return Array.from(this.orders.values());
  }
}
```

#### Benefits of AutoDispatchProjection

- **Automatic routing**: Events are routed to handler methods by type
- **Clean code**: Each event type has its own handler method
- **Type safety**: TypeScript ensures correct event types
- **Easy to maintain**: Adding new event handlers is straightforward

### `@ProjectionHandler` Decorator

Decorator to mark methods as event handlers in AutoDispatchProjection.

```typescript
@ProjectionHandler('EventTypeName')
async onEventName(envelope: EventEnvelope<EventType>): Promise<void> {
  // Handle event
}
```

---

## 🚨 Error Handling

### Error Types

```typescript
import {
  InvalidAggregateStateError,
  ConcurrencyError,
  EventSerializationError,
  RepositoryError
} from '@neurale/event-sourcing-ts';
```

#### `InvalidAggregateStateError`
Thrown when an aggregate is in an invalid state for the requested operation.

#### `ConcurrencyError`
Thrown when optimistic concurrency check fails during save.

#### `EventSerializationError`
Thrown when event serialization/deserialization fails.

#### `RepositoryError`
Thrown when repository operations fail.

### Error Handling Pattern

```typescript
try {
  await repository.save(aggregate);
} catch (error) {
  if (error instanceof ConcurrencyError) {
    // Handle concurrency conflict - reload and retry
    const latest = await repository.load(aggregate.id);
    // Retry logic
  } else if (error instanceof InvalidAggregateStateError) {
    // Handle invalid state
  } else {
    throw error;
  }
}
```

---

## 🧪 Testing Utilities

### `MemoryEventStoreClient`

In-memory event store for testing without external dependencies.

```typescript
import { MemoryEventStoreClient, RepositoryFactory } from '@neurale/event-sourcing-ts';

// Setup for tests
const memoryStore = new MemoryEventStoreClient();
const factory = new RepositoryFactory(memoryStore);
const repository = factory.create(OrderAggregate, 'Order', serializer);

// Test aggregate behavior
const order = new OrderAggregate();
order.place('order-123', 'customer-1');
await repository.save(order);

const loaded = await repository.load('order-123');
expect(loaded?.status).toBe('placed');
```

### Testing Pattern

```typescript
describe('OrderAggregate', () => {
  let repository: Repository<OrderAggregate, OrderEvent>;
  let serializer: EventSerializer;

  beforeEach(() => {
    const memoryStore = new MemoryEventStoreClient();
    serializer = new EventSerializer();
    serializer.registerEvent(OrderPlaced);
    serializer.registerEvent(OrderShipped);
    
    const factory = new RepositoryFactory(memoryStore);
    repository = factory.create(OrderAggregate, 'Order', serializer);
  });

  it('should place an order', async () => {
    const order = new OrderAggregate();
    order.place('order-123', 'customer-1');
    
    expect(order.status).toBe('placed');
    expect(order.hasUncommittedEvents()).toBe(true);
    
    await repository.save(order);
    
    const loaded = await repository.load('order-123');
    expect(loaded?.status).toBe('placed');
  });
});
```

---

## 🔐 Advanced Features

### Message Context

Track correlation and causation IDs for distributed tracing.

```typescript
import { MessageContext } from '@neurale/event-sourcing-ts';

// Set context before processing commands
MessageContext.set({
  correlationId: 'request-123',
  causationId: 'command-456'
});

// Context is automatically propagated to events
await repository.save(aggregate);
```

### Event Metadata Enrichment

Automatically enrich events with metadata:

```typescript
const envelope: EventEnvelope<OrderPlaced> = {
  event: new OrderPlaced('order-123', 'customer-1'),
  metadata: {
    eventId: uuid(),
    eventType: 'OrderPlaced',
    aggregateId: 'order-123',
    aggregateType: 'Order',
    aggregateVersion: 1,
    timestamp: new Date(),
    correlationId: MessageContext.getCorrelationId(),
    causationId: MessageContext.getCausationId()
  }
};
```

---

## 📚 Additional Resources

- **[TypeScript SDK Guide](./typescript/typescript-sdk.md)** - Complete TypeScript implementation guide
- **[SDK Overview](./overview/sdk-overview.md)** - Architecture and workflow


---

## 🚀 Future SDK Support

### Python SDK (Planned)
```python
from neurale.event_sourcing import BaseAggregate, Repository

class OrderAggregate(BaseAggregate):
    def place(self, order_id: str, customer_id: str):
        self.initialize(order_id)
        self.raise_event(OrderPlaced(order_id, customer_id))
```

### Rust SDK (Planned)
```rust
use neurale_event_sourcing::{BaseAggregate, Repository};

impl OrderAggregate {
    fn place(&mut self, order_id: String, customer_id: String) {
        self.initialize(order_id.clone());
        self.raise_event(OrderPlaced { order_id, customer_id });
    }
}
```

### Go SDK (Planned)
```go
import "github.com/neurale/event-sourcing-go"

func (o *OrderAggregate) Place(orderID, customerID string) {
    o.Initialize(orderID)
    o.RaiseEvent(OrderPlaced{OrderID: orderID, CustomerID: customerID})
}
```
