---
sidebar_position: 1
---

# VSA + Event Sourcing: The Complete Guide

Build production-ready event-sourced applications with Vertical Slice Architecture.

## Introduction

This guide shows you how to combine **Vertical Slice Architecture (VSA)** with **Event Sourcing** to build scalable, maintainable applications that capture every state change as an immutable event.

### Why Combine VSA with Event Sourcing?

**Vertical Slice Architecture** organizes code by business features, while **Event Sourcing** stores every state change as an event. Together, they create a powerful foundation for:

- **Complete Audit Trail** - Every change is recorded as an event
- **Time Travel** - Replay events to any point in time
- **Feature Isolation** - Each feature is self-contained
- **Scalability** - Independent scaling of read and write models
- **Testing** - Test features in isolation without infrastructure
- **Debugging** - See exact sequence of events that led to current state

### What You'll Learn

By the end of this guide, you'll be able to:

1. Set up a VSA project with event sourcing
2. Create aggregates with command and event handlers
3. Implement commands that validate business rules
4. Store and replay events from an event store
5. Build read models (projections) for queries
6. Handle cross-context communication with integration events
7. Test your event-sourced features
8. Deploy to production

---

## Core Concepts

### The Three Pillars

#### 1. Commands

**Commands** express intent - what you want to do.

```typescript
export class PlaceOrderCommand {
  constructor(
    public readonly aggregateId: string,
    public readonly customerId: string,
    public readonly items: Array<{ productId: string; quantity: number; price: number }>
  ) {}
}
```

**Key Points:**
- Commands are classes (not interfaces)
- Must have `aggregateId` property
- Present tense (`PlaceOrder`, not `OrderPlaced`)
- Immutable via `readonly`

#### 2. Events

**Events** record facts - what happened.

```typescript
export class OrderPlacedEvent extends BaseDomainEvent {
  readonly eventType = 'OrderPlaced' as const;
  readonly schemaVersion = 1 as const;

  constructor(
    public orderId: string,
    public customerId: string,
    public items: Array<{ productId: string; quantity: number; price: number }>,
    public totalAmount: number,
    public placedAt: Date
  ) {
    super();
  }
}
```

**Key Points:**
- Events extend `BaseDomainEvent`
- Past tense (`OrderPlaced`, not `PlaceOrder`)
- Immutable - never changed after creation
- May include computed values (like `totalAmount`)

#### 3. Aggregates

**Aggregates** enforce business rules and emit events.

```typescript
@Aggregate('Order')
export class OrderAggregate extends AggregateRoot<OrderEvent> {
  private status: OrderStatus = OrderStatus.Draft;
  private items: Array<{ productId: string; quantity: number; price: number }> = [];

  // COMMAND HANDLER - Validates and emits events
  @CommandHandler('PlaceOrderCommand')
  placeOrder(command: PlaceOrderCommand): void {
    // 1. Validate business rules
    if (this.status !== OrderStatus.Draft) {
      throw new Error('Order already placed');
    }
    if (command.items.length === 0) {
      throw new Error('Order must have items');
    }

    // 2. Initialize aggregate
    this.initialize(command.aggregateId);

    // 3. Emit event
    const totalAmount = command.items.reduce((sum, item) => sum + item.price * item.quantity, 0);
    this.apply(new OrderPlacedEvent(
      command.aggregateId,
      command.customerId,
      command.items,
      totalAmount,
      new Date()
    ));
  }

  // EVENT SOURCING HANDLER - Updates state only
  @EventSourcingHandler('OrderPlaced')
  private onOrderPlaced(event: OrderPlacedEvent): void {
    this.status = OrderStatus.Placed;
    this.items = event.items;
  }

  getAggregateType(): string {
    return 'Order';
  }
}
```

**Key Points:**
- Command handlers validate and emit events
- Event handlers update state only
- Clear separation of concerns

---

## Architecture

### High-Level Flow

```
┌─────────────┐      ┌────────────────┐      ┌──────────────┐      ┌─────────────┐
│   Client    │─────▶│  Command Bus   │─────▶│  Aggregate   │─────▶│ Event Store │
│             │      │                │      │ (@CommandHandler) │      │             │
└─────────────┘      └────────────────┘      └──────────────┘      └─────────────┘
                                                      │
                                                      ▼
                                              ┌──────────────┐
                                              │    Events    │
                                              │(@EventSourcingHandler)
                                              └──────────────┘
                                                      │
                                                      ▼
                                              ┌──────────────┐
                                              │  Projections │
                                              │(Read Models) │
                                              └──────────────┘
```

### Component Responsibilities

| Component | Responsibility | Can Do | Cannot Do |
|-----------|---------------|---------|-----------|
| **Command** | Express intent | Carry data | Contain logic |
| **@CommandHandler** | Validate business rules | Check state, emit events | Modify state directly |
| **Event** | Record facts | Carry data | Be modified |
| **@EventSourcingHandler** | Update state | Modify state | Validate, emit events |
| **Command Bus** | Route commands | Load aggregates, save events | Business logic |
| **Projection** | Build read models | Subscribe to events | Modify aggregates |

---

## Setup

### Step 1: Install Dependencies

```bash
# Create project
mkdir my-event-sourced-app
cd my-event-sourced-app

# Initialize package.json
npm init -y

# Install event sourcing platform
npm install @event-sourcing-platform/typescript

# Install development dependencies
npm install --save-dev typescript @types/node jest @types/jest ts-jest

# Initialize TypeScript
npx tsc --init
```

### Step 2: Configure VSA

Create `vsa.yaml`:

```yaml
version: 1
language: typescript
root: src/contexts

# Framework integration
framework:
  name: event-sourcing-platform
  aggregate_class: AggregateRoot
  aggregate_import: "@event-sourcing-platform/typescript"

# Define bounded contexts
bounded_contexts:
  - name: orders
    description: Order management
    publishes:
      - OrderPlaced
      - OrderCancelled
    subscribes: []

# Validation rules
validation:
  require_tests: true
  require_handler: false  # No separate handlers needed
  require_aggregate: true
```

### Step 3: Project Structure

```
my-event-sourced-app/
├── vsa.yaml
├── package.json
├── tsconfig.json
├── src/
│   ├── contexts/
│   │   └── orders/
│   │       ├── place-order/
│   │       │   ├── PlaceOrderCommand.ts
│   │       │   ├── OrderPlacedEvent.ts
│   │       │   ├── OrderAggregate.ts
│   │       │   └── PlaceOrder.test.ts
│   │       └── cancel-order/
│   ├── infrastructure/
│   │   ├── CommandBus.ts
│   │   ├── EventStore.ts
│   │   └── Logger.ts
│   └── index.ts
└── tests/
    └── integration/
```

---

## Implementation Guide

### Step 1: Define Bounded Contexts

Identify your business domains and their boundaries:

```yaml
# vsa.yaml
bounded_contexts:
  - name: orders
    description: Order processing
    publishes: [OrderPlaced, OrderCancelled]
    
  - name: shipping
    description: Shipment management
    subscribes: [OrderPlaced]  # Listen to orders context
    publishes: [ShipmentCreated]
```

### Step 2: Create Your First Aggregate

```typescript
// src/contexts/orders/place-order/OrderAggregate.ts
import { 
  Aggregate, 
  AggregateRoot, 
  CommandHandler, 
  EventSourcingHandler,
  BaseDomainEvent 
} from '@event-sourcing-platform/typescript';

// Define events
class OrderPlacedEvent extends BaseDomainEvent {
  readonly eventType = 'OrderPlaced' as const;
  readonly schemaVersion = 1 as const;

  constructor(
    public orderId: string,
    public customerId: string,
    public items: Array<{ productId: string; quantity: number; price: number }>,
    public totalAmount: number
  ) {
    super();
  }
}

type OrderEvent = OrderPlacedEvent;

// Define aggregate
@Aggregate('Order')
export class OrderAggregate extends AggregateRoot<OrderEvent> {
  private customerId: string | null = null;
  private items: Array<{ productId: string; quantity: number; price: number }> = [];
  private totalAmount: number = 0;

  getAggregateType(): string {
    return 'Order';
  }

  // Getters for read access
  getCustomerId(): string | null { return this.customerId; }
  getItems() { return [...this.items]; }
  getTotalAmount(): number { return this.totalAmount; }
}
```

### Step 3: Implement Command Handlers

Add command handling to your aggregate:

```typescript
import { PlaceOrderCommand } from './PlaceOrderCommand';

// Inside OrderAggregate class
@CommandHandler('PlaceOrderCommand')
placeOrder(command: PlaceOrderCommand): void {
  // 1. Validate business rules
  if (!command.items || command.items.length === 0) {
    throw new Error('Order must contain at least one item');
  }
  
  if (!command.customerId) {
    throw new Error('Customer ID is required');
  }
  
  if (this.id !== null) {
    throw new Error('Order already placed');
  }

  // 2. Initialize aggregate
  this.initialize(command.aggregateId);

  // 3. Calculate total
  const totalAmount = command.items.reduce(
    (sum, item) => sum + item.price * item.quantity,
    0
  );

  // 4. Apply event
  this.apply(new OrderPlacedEvent(
    command.aggregateId,
    command.customerId,
    command.items,
    totalAmount
  ));
}
```

### Step 4: Add Event Sourcing Handlers

Handle events to update aggregate state:

```typescript
// Inside OrderAggregate class
@EventSourcingHandler('OrderPlaced')
private onOrderPlaced(event: OrderPlacedEvent): void {
  // Update state only - NO validation
  this.customerId = event.customerId;
  this.items = event.items;
  this.totalAmount = event.totalAmount;
}
```

### Step 5: Set Up Repository

Create infrastructure for loading/saving aggregates:

```typescript
// src/infrastructure/EventStore.ts
import { 
  EventStoreClient, 
  EventStoreClientFactory,
  MemoryEventStoreClient,
  RepositoryFactory 
} from '@event-sourcing-platform/typescript';

export class EventStore {
  private client: EventStoreClient;
  
  async connect(): Promise<void> {
    // For development - use in-memory
    this.client = new MemoryEventStoreClient();
    
    // For production - use gRPC
    // this.client = EventStoreClientFactory.createGrpcClient({
    //   serverAddress: 'localhost:50051',
    //   tenantId: 'my-app'
    // });
    
    await this.client.connect();
  }

  getClient(): EventStoreClient {
    return this.client;
  }
}
```

### Step 6: Create Command Bus

Route commands to aggregates:

```typescript
// src/infrastructure/CommandBus.ts
import { RepositoryFactory, EventStoreClient, Repository } from '@event-sourcing-platform/typescript';
import { OrderAggregate } from '../contexts/orders/place-order/OrderAggregate';
import { PlaceOrderCommand } from '../contexts/orders/place-order/PlaceOrderCommand';

export class CommandBus {
  private repository: Repository<OrderAggregate>;

  constructor(eventStoreClient: EventStoreClient) {
    const factory = new RepositoryFactory(eventStoreClient);
    this.repository = factory.createRepository(
      () => new OrderAggregate(),
      'Order'
    );
  }

  async send(command: PlaceOrderCommand): Promise<void> {
    // Load or create aggregate
    let aggregate = await this.repository.load(command.aggregateId);
    if (!aggregate) {
      aggregate = new OrderAggregate();
    }

    // Dispatch to @CommandHandler
    (aggregate as any).handleCommand(command);

    // Save (includes optimistic concurrency check)
    await this.repository.save(aggregate);
  }
}
```

### Step 7: Build Read Models

Create projections for queries:

```typescript
// src/contexts/orders/list-orders/OrderListProjection.ts
import { EventBus } from '@event-sourcing-platform/typescript';
import { OrderPlacedEvent } from '../place-order/OrderPlacedEvent';

export interface OrderListItem {
  orderId: string;
  customerId: string;
  totalAmount: number;
  itemCount: number;
  placedAt: Date;
}

export class OrderListProjection {
  private orders: Map<string, OrderListItem> = new Map();

  constructor(eventBus: EventBus) {
    eventBus.subscribe('OrderPlaced', this.onOrderPlaced.bind(this));
  }

  private onOrderPlaced(event: OrderPlacedEvent): void {
    this.orders.set(event.orderId, {
      orderId: event.orderId,
      customerId: event.customerId,
      totalAmount: event.totalAmount,
      itemCount: event.items.length,
      placedAt: event.createdAt
    });
  }

  getAll(): OrderListItem[] {
    return Array.from(this.orders.values());
  }

  getByCustomer(customerId: string): OrderListItem[] {
    return Array.from(this.orders.values())
      .filter(order => order.customerId === customerId);
  }
}
```

### Step 8: Add Integration Events

Enable cross-context communication:

```typescript
// src/_shared/integration-events/orders/OrderPlacedIntegrationEvent.ts
import { BaseDomainEvent } from '@event-sourcing-platform/typescript';

export class OrderPlacedIntegrationEvent extends BaseDomainEvent {
  readonly eventType = 'OrderPlaced' as const;
  readonly schemaVersion = 1 as const;

  constructor(
    public orderId: string,
    public customerId: string,
    public totalAmount: number
  ) {
    super();
  }
}

// Publish from aggregate
@CommandHandler('PlaceOrderCommand')
placeOrder(command: PlaceOrderCommand): void {
  // ... validation and local event ...
  
  // Publish integration event for other contexts
  this.apply(new OrderPlacedIntegrationEvent(
    command.aggregateId,
    command.customerId,
    totalAmount
  ));
}
```

### Step 9: Implement Cross-Context Communication

Subscribe to integration events in other contexts:

```typescript
// src/contexts/shipping/create-shipment/OrderPlacedSubscriber.ts
import { OrderPlacedIntegrationEvent } from '../../../_shared/integration-events/orders';

export class OrderPlacedSubscriber {
  constructor(
    private commandBus: CommandBus,
    eventBus: EventBus
  ) {
    eventBus.subscribe('OrderPlaced', this.handle.bind(this));
  }

  async handle(event: OrderPlacedIntegrationEvent): Promise<void> {
    // Create shipment in shipping context
    const command = new CreateShipmentCommand(
      `shipment-${event.orderId}`,
      event.orderId,
      event.customerId
    );
    
    await this.commandBus.send(command);
  }
}
```

### Step 10: Add Tests

Test aggregates in isolation:

```typescript
// src/contexts/orders/place-order/PlaceOrder.test.ts
import { OrderAggregate } from './OrderAggregate';
import { PlaceOrderCommand } from './PlaceOrderCommand';

describe('PlaceOrder', () => {
  it('should place order successfully', () => {
    // Arrange
    const aggregate = new OrderAggregate();
    const command = new PlaceOrderCommand(
      'order-123',
      'customer-456',
      [
        { productId: 'p1', quantity: 2, price: 10.00 },
        { productId: 'p2', quantity: 1, price: 15.00 }
      ]
    );

    // Act
    (aggregate as any).handleCommand(command);

    // Assert
    const events = aggregate.getUncommittedEvents();
    expect(events).toHaveLength(1);
    expect(events[0].eventType).toBe('OrderPlaced');
    expect(events[0].totalAmount).toBe(35.00);
  });

  it('should reject orders with no items', () => {
    const aggregate = new OrderAggregate();
    const command = new PlaceOrderCommand('order-123', 'customer-456', []);

    expect(() => (aggregate as any).handleCommand(command)).toThrow(
      'Order must contain at least one item'
    );
  });

  it('should reject placing same order twice', () => {
    const aggregate = new OrderAggregate();
    const command = new PlaceOrderCommand('order-123', 'customer-456', [
      { productId: 'p1', quantity: 1, price: 10.00 }
    ]);

    (aggregate as any).handleCommand(command);

    expect(() => (aggregate as any).handleCommand(command)).toThrow(
      'Order already placed'
    );
  });
});
```

---

## Testing

### Unit Testing (Fast, Isolated)

Test aggregates without infrastructure:

```typescript
describe('OrderAggregate', () => {
  it('should calculate total correctly', () => {
    const aggregate = new OrderAggregate();
    const command = new PlaceOrderCommand('order-1', 'customer-1', [
      { productId: 'p1', quantity: 2, price: 10.00 },
      { productId: 'p2', quantity: 3, price: 5.00 }
    ]);

    (aggregate as any).handleCommand(command);

    expect(aggregate.getTotalAmount()).toBe(35.00); // 2*10 + 3*5
  });

  it('should enforce business rules', () => {
    const aggregate = new OrderAggregate();
    const invalidCommand = new PlaceOrderCommand('order-1', '', [
      { productId: 'p1', quantity: 1, price: 10.00 }
    ]);

    expect(() => (aggregate as any).handleCommand(invalidCommand)).toThrow();
  });
});
```

### Integration Testing (With Event Store)

Test the full flow:

```typescript
describe('Place Order Integration', () => {
  let eventStore: EventStore;
  let commandBus: CommandBus;

  beforeAll(async () => {
    eventStore = new EventStore();
    await eventStore.connect();
    commandBus = new CommandBus(eventStore.getClient());
  });

  it('should persist and reload order', async () => {
    // Place order
    const command = new PlaceOrderCommand('order-123', 'customer-456', [
      { productId: 'p1', quantity: 2, price: 10.00 }
    ]);
    await commandBus.send(command);

    // Reload from event store
    const repository = new RepositoryFactory(eventStore.getClient())
      .createRepository(() => new OrderAggregate(), 'Order');
    
    const aggregate = await repository.load('order-123');
    
    expect(aggregate).toBeDefined();
    expect(aggregate!.getCustomerId()).toBe('customer-456');
    expect(aggregate!.getTotalAmount()).toBe(20.00);
  });
});
```

### E2E Testing (Full System)

Test complete user flows:

```typescript
describe('Order Placement E2E', () => {
  it('should create order and shipment', async () => {
    // 1. Place order
    const placeOrderCommand = new PlaceOrderCommand(
      'order-123',
      'customer-456',
      [{ productId: 'p1', quantity: 1, price: 10.00 }]
    );
    await commandBus.send(placeOrderCommand);

    // 2. Verify shipment was created (via integration event)
    await waitFor(async () => {
      const shipment = await shipmentRepository.load('shipment-order-123');
      return shipment !== null;
    });

    // 3. Verify projections updated
    const orders = orderListProjection.getByCustomer('customer-456');
    expect(orders).toHaveLength(1);
  });
});
```

---

## Observability

### Structured Logging

Use Pino for structured, fast logging:

```typescript
// src/infrastructure/Logger.ts
import pino from 'pino';

export const logger = pino({
  level: process.env.LOG_LEVEL || 'info',
  transport: {
    target: 'pino-pretty',
    options: {
      colorize: true,
      translateTime: 'SYS:standard',
      ignore: 'pid,hostname'
    }
  }
});

// Usage in aggregates
@CommandHandler('PlaceOrderCommand')
placeOrder(command: PlaceOrderCommand): void {
  logger.info({ command }, 'Processing PlaceOrderCommand');
  
  try {
    // ... validation and event emission ...
    logger.info({ orderId: command.aggregateId }, 'Order placed successfully');
  } catch (error) {
    logger.error({ error, command }, 'Failed to place order');
    throw error;
  }
}
```

### Event Tracking

Log all events for debugging:

```typescript
class EventLogger {
  constructor(eventBus: EventBus) {
    eventBus.subscribeAll((event) => {
      logger.info({
        eventType: event.eventType,
        aggregateId: event.aggregateId,
        metadata: event.metadata
      }, 'Event emitted');
    });
  }
}
```

### Metrics

Track command and event throughput:

```typescript
class Metrics {
  private commandCounter = new Map<string, number>();
  private eventCounter = new Map<string, number>();

  recordCommand(commandType: string): void {
    const current = this.commandCounter.get(commandType) || 0;
    this.commandCounter.set(commandType, current + 1);
  }

  recordEvent(eventType: string): void {
    const current = this.eventCounter.get(eventType) || 0;
    this.eventCounter.set(eventType, current + 1);
  }

  getStats() {
    return {
      commands: Object.fromEntries(this.commandCounter),
      events: Object.fromEntries(this.eventCounter)
    };
  }
}
```

---

## Deployment

### Environment Configuration

```typescript
// config/config.ts
export const config = {
  eventStore: {
    mode: process.env.EVENT_STORE_MODE || 'memory',  // 'memory' | 'grpc'
    grpcAddress: process.env.EVENT_STORE_ADDR || 'localhost:50051',
    tenantId: process.env.EVENT_STORE_TENANT || 'default'
  },
  logging: {
    level: process.env.LOG_LEVEL || 'info',
    pretty: process.env.NODE_ENV !== 'production'
  }
};
```

### Docker Compose

```yaml
version: '3.8'

services:
  app:
    build: .
    environment:
      - EVENT_STORE_MODE=grpc
      - EVENT_STORE_ADDR=eventstore:50051
      - LOG_LEVEL=info
    depends_on:
      - eventstore
      - postgres

  eventstore:
    image: event-sourcing-platform/eventstore:latest
    environment:
      - BACKEND=postgres
      - DATABASE_URL=postgres://user:pass@postgres:5432/eventstore
    ports:
      - "50051:50051"
    depends_on:
      - postgres

  postgres:
    image: postgres:15-alpine
    environment:
      - POSTGRES_USER=user
      - POSTGRES_PASSWORD=pass
      - POSTGRES_DB=eventstore
    volumes:
      - postgres_data:/var/lib/postgresql/data

volumes:
  postgres_data:
```

### Health Checks

```typescript
// src/health.ts
export async function healthCheck(): Promise<boolean> {
  try {
    // Check event store connection
    await eventStoreClient.ping();
    
    // Check database connection
    await database.query('SELECT 1');
    
    return true;
  } catch (error) {
    logger.error({ error }, 'Health check failed');
    return false;
  }
}
```

---

## Common Patterns

### Pattern 1: Sagas (Process Managers)

Coordinate multi-step workflows:

```typescript
export class OrderSaga {
  constructor(
    private commandBus: CommandBus,
    eventBus: EventBus
  ) {
    eventBus.subscribe('OrderPlaced', this.onOrderPlaced.bind(this));
    eventBus.subscribe('PaymentProcessed', this.onPaymentProcessed.bind(this));
    eventBus.subscribe('PaymentFailed', this.onPaymentFailed.bind(this));
  }

  private async onOrderPlaced(event: OrderPlacedEvent): Promise<void> {
    await this.commandBus.send(new ProcessPaymentCommand(
      event.orderId,
      event.totalAmount
    ));
  }

  private async onPaymentProcessed(event: PaymentProcessedEvent): Promise<void> {
    await this.commandBus.send(new CreateShipmentCommand(
      event.orderId
    ));
  }

  private async onPaymentFailed(event: PaymentFailedEvent): Promise<void> {
    await this.commandBus.send(new CancelOrderCommand(
      event.orderId,
      'Payment failed'
    ));
  }
}
```

### Pattern 2: Event Versioning

Handle evolving event schemas:

```typescript
class OrderPlacedEventV2 extends BaseDomainEvent {
  readonly eventType = 'OrderPlaced' as const;
  readonly schemaVersion = 2 as const;  // Incremented version

  constructor(
    public orderId: string,
    public customerId: string,
    public items: Array<{ productId: string; quantity: number; price: number }>,
    public totalAmount: number,
    public currency: string  // New field in V2
  ) {
    super();
  }
}

// Handle both versions
@EventSourcingHandler('OrderPlaced')
private onOrderPlaced(event: OrderPlacedEventV1 | OrderPlacedEventV2): void {
  this.items = event.items;
  this.totalAmount = event.totalAmount;
  
  // Handle new field
  if ('currency' in event) {
    this.currency = event.currency;
  } else {
    this.currency = 'USD';  // Default for old events
  }
}
```

### Pattern 3: Optimistic Concurrency

Handle concurrent modifications:

```typescript
async function placeOrder(command: PlaceOrderCommand): Promise<void> {
  const maxRetries = 3;
  let attempt = 0;

  while (attempt < maxRetries) {
    try {
      let aggregate = await repository.load(command.aggregateId);
      if (!aggregate) {
        aggregate = new OrderAggregate();
      }

      (aggregate as any).handleCommand(command);
      await repository.save(aggregate);
      
      return;  // Success!
    } catch (error) {
      if (error instanceof ConcurrencyConflictError) {
        attempt++;
        if (attempt >= maxRetries) {
          throw new Error('Maximum retry attempts exceeded');
        }
        // Retry with exponential backoff
        await sleep(Math.pow(2, attempt) * 100);
      } else {
        throw error;
      }
    }
  }
}
```

---

## Troubleshooting

### Issue: "No @CommandHandler found"

**Cause:** Command type name doesn't match decorator.

**Solution:**
```typescript
// Command class name
export class PlaceOrderCommand { ... }

// Decorator MUST match
@CommandHandler('PlaceOrderCommand')  // ✅ Correct
placeOrder(command: PlaceOrderCommand) { ... }
```

### Issue: "Cannot raise events on aggregate without ID"

**Cause:** Forgot to call `this.initialize(aggregateId)`.

**Solution:**
```typescript
@CommandHandler('PlaceOrderCommand')
placeOrder(command: PlaceOrderCommand): void {
  this.initialize(command.aggregateId);  // ✅ Must call before apply()
  this.apply(new OrderPlacedEvent(...));
}
```

### Issue: Events not replaying correctly

**Cause:** Business logic in event handler.

**Solution:**
```typescript
// ❌ WRONG - Validation in event handler
@EventSourcingHandler('OrderPlaced')
private onOrderPlaced(event: OrderPlacedEvent): void {
  if (event.items.length === 0) {  // ❌ Don't validate here
    throw new Error('Invalid');
  }
  this.items = event.items;
}

// ✅ CORRECT - Only state updates
@EventSourcingHandler('OrderPlaced')
private onOrderPlaced(event: OrderPlacedEvent): void {
  this.items = event.items;  // ✅ Just update state
}
```

### Issue: Projections out of sync

**Cause:** Events processed out of order.

**Solution:** Use event sequence numbers:
```typescript
class OrderListProjection {
  private lastProcessedSequence = 0;

  onOrderPlaced(event: OrderPlacedEvent): void {
    if (event.sequenceNumber <= this.lastProcessedSequence) {
      return;  // Already processed
    }
    
    // Process event
    this.orders.set(event.orderId, {...});
    this.lastProcessedSequence = event.sequenceNumber;
  }
}
```

---

## Next Steps

### Build Your First Feature

1. **Identify a business capability** (e.g., "User Registration")
2. **Define commands** (RegisterUserCommand)
3. **Define events** (UserRegisteredEvent)
4. **Create aggregate** (UserAggregate with @CommandHandler)
5. **Add tests** (Test business rules)
6. **Build read model** (UserListProjection)
7. **Deploy** (Start with in-memory, move to event store)

### Learn More

- **VSA Documentation** - See sidebar for Vertical Slice Architecture guides
- **Event Sourcing Documentation** - See sidebar for core concepts and patterns
- **Examples** - Check the examples directory in the repository

### Get Help

- **GitHub Issues:** Report bugs and request features
- **Discussions:** Ask questions and share patterns
- **Examples:** Study working applications in `/examples`

---

## Summary

You've learned how to:

✅ Combine VSA with Event Sourcing  
✅ Create commands as classes with `aggregateId`  
✅ Build aggregates with `@CommandHandler` and `@EventSourcingHandler`  
✅ Validate business rules in command handlers  
✅ Update state in event sourcing handlers  
✅ Use repository pattern for loading/saving  
✅ Build read models with projections  
✅ Handle cross-context communication  
✅ Test aggregates in isolation  
✅ Deploy to production  

**Now go build something amazing!** 🚀

