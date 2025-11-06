# ADR-009: CQRS Pattern Implementation

**Status:** ✅ Accepted  
**Date:** 2025-11-06  
**Decision Makers:** Architecture Team  
**Related:** CQRS, Event Sourcing, Hexagonal Architecture

## Context

Event sourcing systems naturally lead to **Command-Query Responsibility Segregation (CQRS)** because:

1. **Different Concerns:** Writing (commands) and reading (queries) have different requirements
2. **Different Optimization:** Writes need consistency, reads need performance
3. **Different Models:** Write model (aggregates) vs read models (projections)
4. **Event-Driven:** Events are the bridge between write and read sides

### The Problem Without CQRS

Traditional CRUD systems use the same model for reads and writes:

```typescript
// Same model for everything
class Cart {
  async addItem(item: Item): Promise<void> {
    this.items.push(item);
    await database.save(this);  // Write
  }
  
  async getItems(): Promise<Item[]> {
    return await database.load(this.id);  // Read
  }
}
```

**Problems:**
- ❌ Complex queries slow down writes
- ❌ Write model doesn't match read needs
- ❌ Can't scale reads independently
- ❌ Single database must handle both concerns

### Requirements

1. **Clear Separation:** Commands and queries are distinct
2. **Independent Scaling:** Scale reads and writes independently
3. **Optimized Models:** Write model optimized for consistency, read models for queries
4. **Event-Driven:** Read models built from events
5. **Type Safety:** Compiler enforces CQRS boundaries

## Decision

We implement **strict CQRS** with complete separation of command and query sides, connected via domain events.

### CQRS Architecture

```
                    WRITE SIDE                           READ SIDE
┌───────────────────────────────────┐    Events    ┌──────────────────────────────┐
│                                   │      │       │                              │
│  Command Slices                   │      │       │  Query Slices                │
│  ├─ AddItemController       (HTTP)│      │       │  ├─ CartItemsController      │
│  ├─ RemoveItemController    (CLI) │      ▼       │  │    (HTTP → QueryBus)      │
│  └─ SubmitCartController    (gRPC)│  ┌────────┐  │  └─ CartSummaryController    │
│           │                        │  │  Event │  │           │                  │
│           ▼                        │  │   Bus  │  │           ▼                  │
│      CommandBus                    │  └────────┘  │      QueryBus                │
│           │                        │      │       │           │                  │
│           ▼                        │      │       │           ▼                  │
│   ┌──────────────┐                │      │       │   ┌──────────────────┐      │
│   │  Aggregates  │                │      │       │   │   Projections    │      │
│   │  (Domain)    │ ───Events────  │      └──────→│   │  (Read Models)   │      │
│   └──────────────┘                │              │   └──────────────────┘      │
│           │                        │              │                              │
│           ▼                        │              │                              │
│    Event Store                     │              │    Read Database             │
│    (Write DB)                      │              │    (Optimized for queries)   │
└───────────────────────────────────┘              └──────────────────────────────┘
```

### Core Principles

1. **Commands** - Change state (write)
2. **Queries** - Retrieve data (read)
3. **Events** - Bridge between sides
4. **Never Mix** - A method is either command or query, never both

## Implementation

### 1. Write Side (Commands)

**Components:**
- Command classes
- Command slices (adapters)
- CommandBus
- Aggregates (domain)
- Event Store

**Flow:**
```
HTTP Request → Controller → CommandBus → Aggregate → Events → Event Store
```

**Example:**
```typescript
// domain/commands/AddItemCommand.ts
export class AddItemCommand {
  constructor(
    public readonly aggregateId: string,
    public readonly productId: string,
    public readonly quantity: number,
    public readonly price: number
  ) {}
}

// slices/add-item/AddItemController.ts
@RestController()
@Route('/api/carts')
export class AddItemController {
  constructor(private commandBus: CommandBus) {}

  @Post('/:cartId/items')
  async addItem(
    @Param('cartId') cartId: string,
    @Body() request: AddItemRequest
  ): Promise<void> {  // ← void (no data returned)
    const command = new AddItemCommand(
      cartId,
      request.productId,
      request.quantity,
      request.price
    );
    
    await this.commandBus.send(command);
    // Returns void or acknowledgment only
  }
}

// infrastructure/CommandBus.ts
export class CommandBus {
  constructor(private repository: Repository<AggregateRoot>) {}

  async send<T extends Command>(command: T): Promise<void> {
    // 1. Load aggregate
    let aggregate = await this.repository.load(command.aggregateId);
    if (!aggregate) {
      aggregate = this.createAggregate(command);
    }

    // 2. Dispatch to @CommandHandler (domain)
    aggregate.handleCommand(command);

    // 3. Save events to event store
    await this.repository.save(aggregate);

    // 4. Publish events to event bus
    const events = aggregate.getUncommittedEvents();
    events.forEach(event => this.eventBus.publish(event));
  }
}
```

**Command Rules:**
- ✅ Return `void` or `{ success: boolean }`
- ✅ Change state
- ✅ Use CommandBus
- ❌ NO data queries
- ❌ NO returning domain data
- ❌ NO direct reads

### 2. Read Side (Queries)

**Components:**
- Query classes
- Query slices (adapters)
- QueryBus
- Projections (read models)
- Query handlers
- Read database

**Flow:**
```
HTTP Request → Controller → QueryBus → Query Handler → Projection → Data
```

**Example:**
```typescript
// domain/queries/GetCartItemsQuery.ts
export class GetCartItemsQuery {
  constructor(public readonly cartId: string) {}
}

// slices/cart-items/CartItemsProjection.ts
export class CartItemsProjection {
  private items = new Map<string, CartItem[]>();

  @EventHandler('ItemAdded')
  onItemAdded(event: ItemAddedEvent): void {
    const cartItems = this.items.get(event.aggregateId) || [];
    cartItems.push({
      productId: event.productId,
      quantity: event.quantity,
      price: event.price,
      addedAt: event.timestamp
    });
    this.items.set(event.aggregateId, cartItems);
  }

  @EventHandler('ItemRemoved')
  onItemRemoved(event: ItemRemovedEvent): void {
    const cartItems = this.items.get(event.aggregateId) || [];
    const index = cartItems.findIndex(i => i.productId === event.productId);
    if (index >= 0) {
      cartItems.splice(index, 1);
    }
  }

  getCartItems(cartId: string): CartItem[] {
    return this.items.get(cartId) || [];
  }
}

// slices/cart-items/CartItemsQueryHandler.ts
export class CartItemsQueryHandler {
  constructor(private projection: CartItemsProjection) {}

  handle(query: GetCartItemsQuery): CartItemsView {
    const items = this.projection.getCartItems(query.cartId);
    return {
      cartId: query.cartId,
      items: items,
      totalItems: items.reduce((sum, item) => sum + item.quantity, 0),
      totalPrice: items.reduce((sum, item) => sum + (item.price * item.quantity), 0)
    };
  }
}

// slices/cart-items/CartItemsController.ts
@RestController()
@Route('/api/carts')
export class CartItemsController {
  constructor(private queryBus: QueryBus) {}

  @Get('/:cartId/items')
  async getItems(
    @Param('cartId') cartId: string
  ): Promise<CartItemsView> {  // ← Returns data
    const query = new GetCartItemsQuery(cartId);
    return await this.queryBus.send(query);
  }
}

// infrastructure/QueryBus.ts
export class QueryBus {
  private handlers = new Map<string, QueryHandler>();

  async send<T extends Query, R>(query: T): Promise<R> {
    const queryType = query.constructor.name;
    const handler = this.handlers.get(queryType);
    
    if (!handler) {
      throw new Error(`No query handler found for: ${queryType}`);
    }
    
    return handler.handle(query);
  }

  registerHandler(queryType: string, handler: QueryHandler): void {
    this.handlers.set(queryType, handler);
  }
}
```

**Query Rules:**
- ✅ Return data
- ✅ NO state changes
- ✅ Use QueryBus
- ✅ Read from projections
- ❌ NO commands
- ❌ NO aggregate loading
- ❌ NO event store writes

### 3. Event Bridge

Events connect the write and read sides:

```typescript
// infrastructure/EventBus.ts
export class EventBus {
  private subscribers = new Map<string, EventSubscriber[]>();

  publish(event: DomainEvent): void {
    const eventType = event.constructor.name;
    const subscribers = this.subscribers.get(eventType) || [];
    
    subscribers.forEach(subscriber => {
      subscriber.onEvent(event);
    });
  }

  subscribe(eventType: string, subscriber: EventSubscriber): void {
    const subscribers = this.subscribers.get(eventType) || [];
    subscribers.push(subscriber);
    this.subscribers.set(eventType, subscribers);
  }
}

// Projections subscribe to events
eventBus.subscribe('ItemAdded', cartItemsProjection);
eventBus.subscribe('ItemRemoved', cartItemsProjection);

// When command completes:
// 1. Events saved to event store
// 2. Events published to event bus
// 3. Projections receive events
// 4. Projections update read models
// 5. Queries return updated data
```

## CQRS in Vertical Slices

### Command Slice Structure

```
slices/add-item/              ← WRITE OPERATION
├── AddItemController.ts      (HTTP → Command)
├── AddItem.e2e.test.ts
└── slice.yaml

# slice.yaml
type: command                 ← Marked as command slice
command: AddItemCommand
aggregate: CartAggregate
uses: CommandBus
returns: void
```

### Query Slice Structure

```
slices/cart-items/            ← READ OPERATION
├── CartItemsProjection.ts    (Events → Read Model)
├── CartItemsQueryHandler.ts  (Query → Data)
├── CartItemsController.ts    (HTTP → Query)
└── slice.yaml

# slice.yaml
type: query                   ← Marked as query slice
query: GetCartItemsQuery
projection: CartItemsProjection
subscribes_to:
  - ItemAddedEvent
  - ItemRemovedEvent
uses: QueryBus
returns: CartItemsView
```

## Multiple Read Models

Same events can power multiple projections:

```
                        ItemAddedEvent
                              │
                ┌─────────────┼─────────────┐
                │             │             │
                ▼             ▼             ▼
         CartItemsProjection  CartSummaryProjection  AnalyticsProjection
                │             │             │
                ▼             ▼             ▼
           List Items    Summary Stats   Business Intelligence
```

```typescript
// slices/cart-items/CartItemsProjection.ts
@EventHandler('ItemAdded')
onItemAdded(event: ItemAddedEvent): void {
  // Detailed item list
  this.items.get(event.aggregateId).push({...event});
}

// slices/cart-summary/CartSummaryProjection.ts
@EventHandler('ItemAdded')
onItemAdded(event: ItemAddedEvent): void {
  // Just counts and totals
  const summary = this.summaries.get(event.aggregateId);
  summary.itemCount++;
  summary.totalPrice += event.price * event.quantity;
}

// slices/analytics/AnalyticsProjection.ts
@EventHandler('ItemAdded')
onItemAdded(event: ItemAddedEvent): void {
  // Analytics data
  this.productStats.get(event.productId).timesAdded++;
  this.productStats.get(event.productId).revenue += event.price * event.quantity;
}
```

## Eventual Consistency

Read models are **eventually consistent**:

```
t=0  Command: AddItem
t=1  Event: ItemAdded saved to event store
t=2  Event published to event bus
t=3  Projection receives event
t=4  Projection updates read model
t=5  Query returns updated data
```

**Time gap:** ~milliseconds typically, but not guaranteed

**Implications:**
- ✅ Reads don't block writes
- ✅ Writes don't block reads
- ✅ Independent scaling
- ⚠️ Read after write might not see change immediately

**Mitigation Strategies:**

1. **Optimistic UI Updates:**
```typescript
// Client-side
await addItemToCart(item);
ui.show(item);  // Show immediately, don't wait for query
```

2. **Return Command ID:**
```typescript
@Post('/:cartId/items')
async addItem(...): Promise<{ commandId: string }> {
  const commandId = uuid();
  await this.commandBus.send(command, commandId);
  return { commandId };
}

// Client polls for completion or uses WebSocket
```

3. **Event Subscriptions:**
```typescript
// Client subscribes to events
websocket.subscribe('cart-123', (event) => {
  if (event.type === 'ItemAdded') {
    ui.update(event.data);
  }
});
```

## VSA Framework Support

### Configuration

```yaml
# vsa.yaml
cqrs:
  enforce_separation: true      # Commands != Queries
  
  commands:
    must_use: CommandBus
    returns: void                # Or acknowledgment only
    no_data_queries: true
  
  queries:
    must_use: QueryBus
    returns: data                # Must return data
    no_state_changes: true
    require_projection: true
  
  validation:
    no_mixed_slices: true       # Slice is command OR query, not both
    projections_event_driven: true
```

### CLI Commands

```bash
# Create command slice
vsa slice add add-item --type command
# Enforces: CommandBus usage, void return, no queries

# Create query slice
vsa slice add cart-items --type query
# Enforces: QueryBus usage, projection, event handlers, data return

# Validate CQRS separation
vsa validate
# Checks:
# ✓ Command slices use CommandBus
# ✓ Query slices use QueryBus
# ✓ No mixed slices
# ✓ Commands return void
# ✓ Queries return data
# ✓ Projections subscribe to events
# ✗ ERROR: AddItemController returns data (should return void)
# ✗ ERROR: CartItemsController calls CommandBus (should use QueryBus)
```

## Language-Specific Implementations

### TypeScript

```typescript
// Command
export class AddItemCommand {
  constructor(
    public readonly aggregateId: string,
    public readonly productId: string
  ) {}
}

// Query
export class GetCartItemsQuery {
  constructor(public readonly cartId: string) {}
}

// View (query result)
export interface CartItemsView {
  cartId: string;
  items: CartItem[];
  totalItems: number;
}
```

### Python

```python
# Command
@dataclass(frozen=True)
class AddItemCommand:
    aggregate_id: str
    product_id: str

# Query
@dataclass(frozen=True)
class GetCartItemsQuery:
    cart_id: str

# View (query result)
@dataclass
class CartItemsView:
    cart_id: str
    items: List[CartItem]
    total_items: int
```

### Rust

```rust
// Command
#[derive(Debug, Clone)]
pub struct AddItemCommand {
    pub aggregate_id: String,
    pub product_id: String,
}

// Query
#[derive(Debug, Clone)]
pub struct GetCartItemsQuery {
    pub cart_id: String,
}

// View (query result)
#[derive(Debug, Clone, Serialize)]
pub struct CartItemsView {
    pub cart_id: String,
    pub items: Vec<CartItem>,
    pub total_items: usize,
}
```

## Consequences

### Positive

1. **Separation of Concerns** ✅
   - Writes optimized for consistency
   - Reads optimized for performance
   - Clear responsibilities

2. **Independent Scaling** ✅
   - Scale read and write databases independently
   - Add read replicas without affecting writes
   - Cache read models aggressively

3. **Optimized Read Models** ✅
   - Denormalized for fast queries
   - Multiple projections for different views
   - Pre-computed aggregations

4. **Event-Driven** ✅
   - Natural fit for event sourcing
   - Read models built from events
   - Easy to add new projections

5. **Type Safety** ✅
   - Commands and queries are distinct types
   - Compiler enforces separation
   - VSA validates CQRS rules

### Negative

1. **Eventual Consistency** ⚠️
   - Reads might be stale
   - Need to handle in UI
   - **Mitigation:** Optimistic updates, event subscriptions

2. **More Complexity** ⚠️
   - Two models instead of one
   - Need to understand projections
   - **Mitigation:** Clear patterns, VSA enforcement

3. **Data Duplication** ⚠️
   - Event store + read database
   - Multiple projections
   - **Mitigation:** Storage is cheap, performance is valuable

### Neutral

1. **Projection Management**
   - Need to rebuild projections if corrupted
   - Can be done by replaying events

2. **Multiple Databases**
   - Event store for writes
   - Optimized database for reads (PostgreSQL, Redis, Elasticsearch)
   - Choose based on query needs

## Related ADRs

- ADR-004: Command Handlers in Aggregates (command side)
- ADR-005: Hexagonal Architecture for Event-Sourced Systems (architecture context)
- ADR-006: Domain Organization Pattern (commands and queries in domain)
- ADR-008: Vertical Slices as Hexagonal Adapters (slice organization)

## References

- "CQRS" - Greg Young
- "CQRS Documents" - Udi Dahan
- "Understanding Event Sourcing" - Alexey Zimarev (CQRS chapter)
- "Implementing Domain-Driven Design" - Vaughn Vernon (CQRS patterns)

---

**Last Updated:** 2025-11-06  
**Supersedes:** None  
**Superseded By:** None

