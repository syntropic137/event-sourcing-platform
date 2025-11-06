---
sidebar_position: 3
---

# Integration Events

Learn how bounded contexts communicate using integration events as a single source of truth.

## What are Integration Events?

**Integration events** are messages that cross bounded context boundaries. They represent facts that one context wants to communicate to others.

### Domain Events vs Integration Events

```
┌─────────────────────────────────────┐
│ Catalog Context                      │
│                                      │
│ Domain Event (Internal, Rich):      │
│   ProductAddedEvent                  │
│   - productId                        │
│   - name, description                │
│   - price, cost, margin             │
│   - supplier, supplierContact       │
│   - internalNotes                   │
│   - addedBy, addedAt                │
│                                      │
│         │                            │
│         ↓ Transform                  │
│                                      │
│ Integration Event (External, Minimal)│
│   ProductAdded                       │
│   - productId                        │
│   - name                             │
│   - price                            │
│   - timestamp                        │
└──────────────┬───────────────────────┘
               │
               ↓ Publish via Event Bus
    _shared/integration-events/
        catalog/ProductAdded.ts
               │
               ↓ Subscribe
┌──────────────┴───────────────────────┐
│ Orders Context                        │
│                                       │
│ Subscriber:                           │
│   ProductAdded.handler.ts            │
│   Updates local product read model    │
└───────────────────────────────────────┘
```

| Aspect | Domain Event | Integration Event |
|--------|--------------|-------------------|
| **Purpose** | Event sourcing | Context communication |
| **Audience** | Single context | Multiple contexts |
| **Location** | Feature folder | `_shared/integration-events/` |
| **Details** | Rich, internal | Minimal, stable |
| **Changes** | Can change freely | Versioned carefully |
| **Naming** | `*Event.ts` | No "Event" suffix |

## Single Source of Truth

### Problem: Event Duplication

**❌ Without single source:**

```
contexts/catalog/add-product/ProductAdded.ts     ← Definition 1
contexts/orders/subscribers/ProductAdded.ts      ← Definition 2 (duplicate!)
contexts/inventory/subscribers/ProductAdded.ts   ← Definition 3 (duplicate!)
```

**Problems:**
- Definitions drift apart
- Hard to update schema
- Type mismatches
- No clear ownership

### Solution: Shared Location

**✅ With single source:**

```
_shared/integration-events/
  catalog/                          ← Published BY catalog
    ProductAdded.ts                 ← Defined ONCE
  
contexts/catalog/add-product/
  - Imports from _shared and publishes

contexts/orders/_subscribers/
  - Imports from _shared and subscribes

contexts/inventory/_subscribers/
  - Imports from _shared and subscribes
```

**Benefits:**
- ✅ Defined exactly once
- ✅ Everyone uses same definition
- ✅ Type-safe across contexts
- ✅ Easy to update schema
- ✅ Clear ownership

## Structure

### Organization

```
_shared/
└── integration-events/
    ├── catalog/            ← Events published BY catalog
    │   ├── ProductAdded.ts
    │   ├── ProductRemoved.ts
    │   └── ProductUpdated.ts
    │
    ├── orders/             ← Events published BY orders
    │   ├── OrderPlaced.ts
    │   ├── OrderCancelled.ts
    │   └── OrderShipped.ts
    │
    └── inventory/          ← Events published BY inventory
        ├── StockAdjusted.ts
        ├── StockReserved.ts
        └── StockReleased.ts
```

**Rule:** Organized by PUBLISHER, not subscriber.

### Event Definition

```typescript title="_shared/integration-events/catalog/ProductAdded.ts"
/**
 * Published when a new product is added to the catalog.
 * 
 * @publisher catalog
 * @subscribers orders, inventory, notifications
 * @version 1
 */
export class ProductAdded {
  readonly eventType = 'ProductAdded';
  readonly version = 1;
  
  constructor(
    public readonly productId: string,
    public readonly name: string,
    public readonly price: number,
    public readonly timestamp: Date,
  ) {}
}
```

**Best practices:**
- Minimal fields (stable contract)
- Past tense name
- Versioned for evolution
- Immutable (readonly)
- Well documented

## Publishing Events

### From Handler

```typescript title="contexts/catalog/add-product/AddProductHandler.ts"
import { ProductAdded } from '../../../../_shared/integration-events/catalog';

export class AddProductHandler {
  constructor(
    private eventStore: EventStore,
    private eventBus: EventBus,
  ) {}

  async handle(command: AddProductCommand): Promise<void> {
    // 1. Execute domain logic
    const aggregate = new ProductAggregate();
    aggregate.addProduct(command);
    
    // 2. Persist domain events
    await this.eventStore.save(aggregate);
    
    // 3. Publish integration event AFTER successful save
    await this.eventBus.publish(
      new ProductAdded(
        aggregate.id,
        aggregate.name,
        aggregate.price,
        new Date()
      )
    );
  }
}
```

**Key points:**
- Publish AFTER persisting domain events
- Transform from rich domain event to minimal integration event
- Only publish what other contexts need

### Error Handling

```typescript
async handle(command: AddProductCommand): Promise<void> {
  try {
    // 1. Persist domain events
    await this.eventStore.save(aggregate);
    
    // 2. Publish integration event
    await this.eventBus.publish(event);
  } catch (error) {
    // If publish fails, event store has record
    // Can retry from outbox pattern
    logger.error('Failed to publish ProductAdded', error);
    throw error;
  }
}
```

## Subscribing to Events

### Create Subscriber

```typescript title="contexts/orders/_subscribers/ProductAdded.handler.ts"
import { ProductAdded } from '../../../_shared/integration-events/catalog';

export class ProductAddedHandler {
  constructor(
    private productReadModel: ProductReadModel,
  ) {}

  async handle(event: ProductAdded): Promise<void> {
    // Update local read model
    await this.productReadModel.add({
      id: event.productId,
      name: event.name,
      price: event.price,
      available: true,
      lastUpdated: event.timestamp,
    });
    
    console.log(`Product ${event.productId} added to orders catalog`);
  }
}
```

**Responsibilities:**
- Update local read models
- Trigger follow-up actions
- Handle idempotently (event may arrive multiple times)

### Idempotency

```typescript
async handle(event: ProductAdded): Promise<void> {
  // Check if already processed
  const existing = await this.productReadModel.findById(event.productId);
  if (existing) {
    console.log(`Product ${event.productId} already exists, skipping`);
    return; // Idempotent
  }
  
  // Process event
  await this.productReadModel.add({...});
}
```

## Configuration

### In `vsa.yaml`

```yaml
bounded_contexts:
  - name: catalog
    description: Product catalog
    publishes:
      - ProductAdded
      - ProductRemoved
      - ProductUpdated
    subscribes: []
  
  - name: orders
    description: Order processing
    publishes:
      - OrderPlaced
      - OrderCancelled
    subscribes:
      - ProductAdded      # From catalog
      - ProductRemoved    # From catalog
      - StockAdjusted     # From inventory
  
  - name: inventory
    description: Stock management
    publishes:
      - StockAdjusted
      - StockReserved
    subscribes:
      - OrderPlaced       # From orders

integration_events:
  path: ../_shared/integration-events/
  
  events:
    ProductAdded:
      publisher: catalog
      subscribers: [orders, inventory, notifications]
      description: "Product added to catalog"
      version: 1
    
    OrderPlaced:
      publisher: orders
      subscribers: [inventory, shipping, notifications]
      description: "Customer placed an order"
      version: 1
```

## Validation

### VSA Validates

```bash
$ vsa validate --integration-events

✅ ProductAdded
   Publisher: catalog ✓
   Defined in: _shared/integration-events/catalog/ ✓
   Subscribers: 
     - orders: ✓ (handler found)
     - inventory: ✓ (handler found)
     - notifications: ✓ (handler found)

❌ OrderPlaced
   Publisher: orders ✓
   Defined in: _shared/integration-events/orders/ ✓
   Subscribers:
     - inventory: ✗ (handler not found)
     - shipping: ✓ (handler found)
   
   💡 Missing: contexts/inventory/_subscribers/OrderPlaced.handler.ts
```

### Common Violations

#### Violation 1: Duplicate Definition

```bash
❌ Duplicate event definition
   Event: ProductAdded
   Found in:
     - _shared/integration-events/catalog/ProductAdded.ts ✓
     - contexts/orders/_shared/ProductAdded.ts ✗
   
   💡 Fix: Remove duplicate, import from _shared/
```

#### Violation 2: Wrong Location

```bash
❌ Integration event in wrong location
   Event: ProductAdded
   Location: contexts/catalog/add-product/ProductAdded.ts
   Should be: _shared/integration-events/catalog/ProductAdded.ts
   
   💡 Integration events must be in _shared/integration-events/
```

#### Violation 3: Missing Handler

```bash
❌ Missing subscriber handler
   Context: orders
   Event: ProductRemoved (subscribed in vsa.yaml)
   Expected: contexts/orders/_subscribers/ProductRemoved.handler.ts
   
   💡 Create handler: vsa generate subscriber ProductRemoved --context orders
```

## Event Schema Evolution

### Version Strategy

```typescript
// Version 1
export class ProductAdded {
  readonly version = 1;
  
  constructor(
    public readonly productId: string,
    public readonly name: string,
    public readonly price: number,
  ) {}
}

// Version 2 - Additive change (backward compatible)
export class ProductAdded {
  readonly version = 2;
  
  constructor(
    public readonly productId: string,
    public readonly name: string,
    public readonly price: number,
    public readonly category?: string,  // Optional for compatibility
  ) {}
}

// Version 3 - Breaking change (new event)
export class ProductAddedV3 {
  readonly version = 3;
  // Breaking changes...
}
```

**Rules:**
- **Additive changes** → Increase version, make new fields optional
- **Breaking changes** → Create new event type

### Handling Multiple Versions

```typescript
async handle(event: ProductAdded): Promise<void> {
  if (event.version === 1) {
    await this.handleV1(event);
  } else if (event.version === 2) {
    await this.handleV2(event);
  } else {
    throw new Error(`Unsupported version: ${event.version}`);
  }
}

private async handleV1(event: ProductAdded): Promise<void> {
  // Handle V1 format
  await this.productReadModel.add({
    id: event.productId,
    name: event.name,
    price: event.price,
    category: 'Uncategorized',  // Default for V1
  });
}

private async handleV2(event: ProductAdded): Promise<void> {
  // Handle V2 format
  await this.productReadModel.add({
    id: event.productId,
    name: event.name,
    price: event.price,
    category: event.category || 'Uncategorized',
  });
}
```

## Event Bus Implementation

### Simple In-Memory

```typescript
export class InMemoryEventBus {
  private handlers = new Map<string, Function[]>();

  subscribe(eventType: string, handler: Function): void {
    if (!this.handlers.has(eventType)) {
      this.handlers.set(eventType, []);
    }
    this.handlers.get(eventType)!.push(handler);
  }

  async publish(event: any): Promise<void> {
    const handlers = this.handlers.get(event.eventType) || [];
    await Promise.all(handlers.map(h => h(event)));
  }
}
```

### With Message Queue

```typescript
export class RabbitMQEventBus {
  constructor(private connection: Connection) {}

  async publish(event: any): Promise<void> {
    const channel = await this.connection.createChannel();
    await channel.assertExchange('integration-events', 'topic');
    
    channel.publish(
      'integration-events',
      event.eventType,
      Buffer.from(JSON.stringify(event))
    );
  }

  async subscribe(eventType: string, handler: Function): Promise<void> {
    const channel = await this.connection.createChannel();
    await channel.assertExchange('integration-events', 'topic');
    
    const queue = await channel.assertQueue('', { exclusive: true });
    await channel.bindQueue(queue.queue, 'integration-events', eventType);
    
    channel.consume(queue.queue, async (msg) => {
      if (msg) {
        const event = JSON.parse(msg.content.toString());
        await handler(event);
        channel.ack(msg);
      }
    });
  }
}
```

## Patterns

### Pattern 1: Event Enrichment

```typescript
// Publisher enriches event with useful data
await this.eventBus.publish(
  new OrderPlaced(
    order.id,
    order.customerId,
    order.items,
    order.totalAmount,
    order.shippingAddress,  // Subscribers need this
    new Date()
  )
);
```

### Pattern 2: Event Transformation

```typescript
// Transform rich domain event to lean integration event
const domainEvent = aggregate.getUncommittedEvents()[0];

const integrationEvent = new ProductAdded(
  domainEvent.productId,
  domainEvent.name,
  domainEvent.price,
  new Date()
  // Exclude internal fields
);
```

### Pattern 3: Eventual Consistency

```typescript
// Orders context maintains eventual view of products
class ProductReadModel {
  // Updated by ProductAdded events from catalog
  // May be slightly out of sync (eventual consistency)
  async findById(id: string): Promise<Product | null> {
    return this.db.products.findOne({ id });
  }
}
```

## Testing

### Unit Test: Publisher

```typescript
describe('AddProductHandler', () => {
  it('should publish ProductAdded event', async () => {
    const eventBus = new MockEventBus();
    const handler = new AddProductHandler(eventStore, eventBus);
    
    await handler.handle(addProductCommand);
    
    const publishedEvents = eventBus.getPublishedEvents();
    expect(publishedEvents).toHaveLength(1);
    expect(publishedEvents[0]).toBeInstanceOf(ProductAdded);
    expect(publishedEvents[0].productId).toBe(command.productId);
  });
});
```

### Unit Test: Subscriber

```typescript
describe('ProductAddedHandler', () => {
  it('should update read model', async () => {
    const readModel = new InMemoryProductReadModel();
    const handler = new ProductAddedHandler(readModel);
    
    await handler.handle(new ProductAdded('p1', 'Laptop', 999, new Date()));
    
    const product = await readModel.findById('p1');
    expect(product).toBeDefined();
    expect(product.name).toBe('Laptop');
  });
});
```

### Integration Test: End-to-End

```typescript
describe('Product Integration Flow', () => {
  it('should propagate product to orders context', async () => {
    // 1. Add product in catalog context
    await catalogHandler.handle(addProductCommand);
    
    // 2. Wait for event propagation
    await waitFor(() => 
      ordersReadModel.findById(product.id) !== null
    );
    
    // 3. Verify product available in orders
    const product = await ordersReadModel.findById(productId);
    expect(product).toBeDefined();
    expect(product.name).toBe('Laptop');
  });
});
```

## CLI Commands

### Generate Integration Event

```bash
$ vsa generate integration-event ProductRemoved \
    --publisher catalog \
    --subscribers orders,inventory

✅ Created _shared/integration-events/catalog/ProductRemoved.ts
✅ Updated vsa.yaml
```

### Generate Subscriber

```bash
$ vsa generate subscriber ProductRemoved --context orders

✅ Created contexts/orders/_subscribers/ProductRemoved.handler.ts
```

### List Integration Events

```bash
$ vsa list integration-events

Integration Events:
  ProductAdded (catalog → orders, inventory)
  ProductRemoved (catalog → orders, inventory)
  OrderPlaced (orders → inventory, shipping)
  StockAdjusted (inventory → orders)
```

## Best Practices

### ✅ Do's

- Define events in `_shared/integration-events/`
- Keep events minimal and stable
- Version events for schema evolution
- Publish AFTER persisting domain events
- Make subscribers idempotent
- Use past tense names

### ❌ Don'ts

- Duplicate event definitions
- Include internal details in integration events
- Publish before persisting
- Assume synchronous processing
- Use integration events within a single context
- Change events without versioning

## Next Steps

- **[Convention Over Configuration](./convention-over-configuration)** - Standard patterns
- **[Event Sourcing Integration](../advanced/event-sourcing-integration)** - With event store
- **Examples** - Check `vsa/examples/02-library-management-ts` in the repository

## Resources

- [Domain Events vs Integration Events](https://enterprisecraftsmanship.com/posts/domain-events-vs-integration-events/)
- [Event-Driven Architecture](https://martinfowler.com/articles/201701-event-driven.html)
- [Schema Evolution](https://docs.confluent.io/platform/current/schema-registry/avro.html)

---

**Ready to implement?** Check out the Library Management example (`vsa/examples/02-library-management-ts`) in the repository to see integration events in a real system.

