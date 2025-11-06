# ADR-005: Hexagonal Architecture for Event-Sourced Systems

**Status:** ✅ Accepted  
**Date:** 2025-11-06  
**Decision Makers:** Architecture Team  
**Related:** Event Sourcing, Hexagonal Architecture, Domain-Driven Design

## Context

Our event sourcing platform needs a clear architectural pattern that:

1. **Isolates Domain Logic:** Business rules must be independent of infrastructure
2. **Supports Multiple Adapters:** Same domain logic accessible via REST, CLI, gRPC, etc.
3. **Enables Testing:** Domain can be tested without external dependencies
4. **Facilitates Evolution:** Easy to swap infrastructure without touching domain
5. **Integrates with Event Sourcing:** Natural fit for event-sourced systems

Traditional layered architectures often lead to:
- ❌ Domain logic leaking into presentation/infrastructure layers
- ❌ Tight coupling to frameworks and databases
- ❌ Difficulty testing business logic in isolation
- ❌ Hard to add new interfaces (REST vs CLI vs gRPC)

**Hexagonal Architecture** (Ports & Adapters pattern) solves these problems by placing the domain at the center and treating all external concerns as adapters.

## Decision

We adopt **Hexagonal Architecture** as the foundational pattern for all event-sourced applications in this platform.

### Core Principle

> "Dependencies point INWARD toward the domain. The domain has NO outward dependencies."

```
┌─────────────────────────────────────────────────────────────┐
│  ADAPTERS (Primary - Driving)                                │
│  REST API │ CLI │ gRPC │ GraphQL │ Message Consumers        │
│                          ▼                                    │
│  ┌──────────────────────────────────────────────────────┐  │
│  │  APPLICATION SERVICES (Ports)                         │  │
│  │  CommandBus │ QueryBus │ EventBus │ Repository        │  │
│  │                          ▼                             │  │
│  │  ┌──────────────────────────────────────────┐        │  │
│  │  │  DOMAIN (Core - Pure Business Logic)     │        │  │
│  │  │  • Aggregates (@CommandHandler methods)  │        │  │
│  │  │  • Commands (what we want to happen)     │        │  │
│  │  │  • Events (what happened)                │        │  │
│  │  │  • Business Rules & Invariants           │        │  │
│  │  └──────────────────────────────────────────┘        │  │
│  │                          ▲                             │  │
│  └──────────────────────────────────────────────────────┘  │
│                          ▲                                    │
│  ADAPTERS (Secondary - Driven)                               │
│  EventStore │ PostgreSQL │ Redis │ Kafka │ Email │ S3       │
└─────────────────────────────────────────────────────────────┘
```

### Three Layers

#### 1. Domain Layer (Center Hexagon)

**Purpose:** Pure business logic, completely isolated from infrastructure.

**Contains:**
- **Aggregates** - Consistency boundaries with `@CommandHandler` methods (ADR-004)
- **Commands** - Intentions to change state
- **Queries** - Requests for data
- **Events** - Immutable facts about what happened
- **Value Objects** - Domain concepts without identity
- **Business Rules** - Invariants and validation

**Rules:**
- ✅ NO dependencies on infrastructure
- ✅ NO framework imports (except decorators)
- ✅ NO database concerns
- ✅ NO HTTP/CLI/protocol concerns
- ✅ Pure business logic only

**Structure:**
```
domain/
├── CartAggregate.ts          ← Aggregates with business logic
├── InventoryAggregate.ts
├── commands/                  ← Command definitions
│   ├── AddItemCommand.ts
│   └── RemoveItemCommand.ts
├── queries/                   ← Query definitions
│   └── GetCartItemsQuery.ts
└── events/                    ← Domain events
    ├── ItemAddedEvent.ts
    └── CartCreatedEvent.ts
```

#### 2. Application Layer (Ports)

**Purpose:** Orchestrate domain objects, define interfaces (ports) for adapters.

**Contains:**
- **CommandBus** - Routes commands to aggregates
- **QueryBus** - Routes queries to query handlers
- **EventBus** - Publishes domain events to subscribers
- **Repository Interface** - Port for persistence (implemented by adapters)
- **Use Case Services** - Coordinate multiple aggregates (if needed)

**Rules:**
- ✅ Can depend on domain
- ✅ Defines interfaces (ports) that adapters implement
- ✅ NO concrete infrastructure implementations
- ✅ Thin coordination layer

**Structure:**
```
infrastructure/               ← Application services (ports)
├── CommandBus.ts            ← Routes commands → aggregates
├── QueryBus.ts              ← Routes queries → handlers
├── EventBus.ts              ← Publishes events → subscribers
└── Repository.ts            ← Interface (port) for persistence
```

#### 3. Adapter Layer (Outside Hexagon)

**Purpose:** Translate external protocols to/from domain language.

**Two Types:**

**Primary Adapters (Driving)** - Drive the application:
- REST API controllers
- CLI commands
- gRPC services
- GraphQL resolvers
- Message consumers (Kafka, RabbitMQ)
- WebSocket handlers

**Secondary Adapters (Driven)** - Driven by application:
- Event store implementation
- Database adapters (PostgreSQL, MongoDB)
- Message publishers (Kafka, RabbitMQ)
- Email service
- File storage (S3, filesystem)
- External APIs

**Rules:**
- ✅ Thin translation layer (< 50 lines recommended)
- ✅ Can depend on application layer AND domain
- ✅ NO business logic
- ✅ Just translate protocol ↔ domain

**Structure:**
```
slices/                      ← Primary adapters (organized as vertical slices)
├── add-item/
│   ├── AddItemController.ts      (REST: HTTP → Command)
│   └── AddItemCLI.ts             (CLI: args → Command)
└── cart-items/
    ├── CartItemsController.ts    (REST: HTTP → Query)
    └── CartItemsQueryHandler.ts  (Query handler + projection)
```

### Dependency Rules

```
Adapters ──→ Application Services ──→ Domain
   ↓               ↓                     ↓
(HTTP)        (CommandBus)         (Aggregates)
(CLI)         (QueryBus)           (Commands)
(gRPC)        (EventBus)           (Events)
```

**Allowed:**
- ✅ Adapters import from Application + Domain
- ✅ Application imports from Domain
- ✅ Domain imports NOTHING (pure)

**Forbidden:**
- ❌ Domain importing from Application
- ❌ Domain importing from Adapters
- ❌ Application importing from Adapters

## Event Sourcing Integration

Hexagonal Architecture is a perfect fit for event sourcing:

### 1. Aggregates as Domain Core

```typescript
// domain/CartAggregate.ts
@Aggregate('Cart')
export class CartAggregate extends AggregateRoot<CartEvent> {
  private items: Map<string, CartItem> = new Map();
  
  // COMMAND HANDLER (domain logic)
  @CommandHandler('AddItemCommand')
  addItem(command: AddItemCommand): void {
    // Business validation
    if (this.items.size >= 3) {
      throw new Error('Cart limited to 3 items');
    }
    
    // Apply event
    this.apply(new ItemAddedEvent(
      command.cartId,
      command.productId,
      command.quantity
    ));
  }
  
  // EVENT SOURCING HANDLER (state updates)
  @EventSourcingHandler('ItemAdded')
  private onItemAdded(event: ItemAddedEvent): void {
    this.items.set(event.productId, {
      productId: event.productId,
      quantity: event.quantity
    });
  }
}
```

### 2. Repository as Application Port

```typescript
// infrastructure/Repository.ts (interface/port)
export interface Repository<T> {
  load(aggregateId: string): Promise<T | null>;
  save(aggregate: T): Promise<void>;
}

// Implemented by adapter (event store)
export class EventStoreRepository<T> implements Repository<T> {
  // Concrete implementation using event store
}
```

### 3. CommandBus Orchestrates

```typescript
// infrastructure/CommandBus.ts (application service)
export class CommandBus {
  constructor(
    private repository: Repository<AggregateRoot>
  ) {}
  
  async send<T extends Command>(command: T): Promise<void> {
    // Load aggregate from event store
    let aggregate = await this.repository.load(command.aggregateId);
    if (!aggregate) {
      aggregate = this.createAggregate(command);
    }
    
    // Dispatch to @CommandHandler (domain logic)
    aggregate.handleCommand(command);
    
    // Save new events to event store
    await this.repository.save(aggregate);
  }
}
```

### 4. Adapters Are Thin

```typescript
// slices/add-item/AddItemController.ts (adapter)
@RestController()
@Route('/api/carts')
export class AddItemController {
  constructor(private commandBus: CommandBus) {}  // Application service
  
  @Post('/:cartId/items')
  async addItem(
    @Param('cartId') cartId: string,
    @Body() request: AddItemRequest
  ): Promise<void> {
    // Just translate HTTP → Command
    const command = new AddItemCommand(
      cartId,
      request.productId,
      request.quantity
    );
    
    // Application service handles the rest
    await this.commandBus.send(command);
  }
}
```

## Rationale

### 1. **Domain Isolation**

Business logic is completely isolated in the domain layer. No framework dependencies, no database concerns, no HTTP knowledge.

**Benefits:**
- Easy to understand (just business rules)
- Easy to test (no mocks needed)
- Easy to evolve (change frameworks without touching domain)

### 2. **Multiple Adapters**

Same domain logic can be accessed via different interfaces:

```
REST API ──┐
CLI        ├──→ CommandBus ──→ CartAggregate.addItem()
gRPC       │
Kafka      ┘
```

Add new interface = Add new adapter. Domain unchanged.

### 3. **Testability**

Test domain in complete isolation:

```typescript
describe('CartAggregate', () => {
  it('should limit cart to 3 items', () => {
    const cart = new CartAggregate();
    
    // Add 3 items (OK)
    cart.handleCommand(new AddItemCommand('cart-1', 'product-1', 1));
    cart.handleCommand(new AddItemCommand('cart-1', 'product-2', 1));
    cart.handleCommand(new AddItemCommand('cart-1', 'product-3', 1));
    
    // Try to add 4th item (should fail)
    expect(() => {
      cart.handleCommand(new AddItemCommand('cart-1', 'product-4', 1));
    }).toThrow('Cart limited to 3 items');
  });
});
```

No HTTP server, no database, no event store. Pure business logic testing.

### 4. **Event Sourcing Alignment**

Event sourcing naturally fits hexagonal architecture:
- **Aggregates** = Domain core
- **Event Store** = Secondary adapter (persistence)
- **API/CLI** = Primary adapters (driving)
- **Projections** = Secondary adapters (read models)

### 5. **Clear Responsibilities**

Every layer has one job:
- **Domain:** Business logic
- **Application:** Orchestration
- **Adapters:** Protocol translation

No confusion about where code belongs.

## Consequences

### Positive

1. **Domain Purity** ✅
   - Business logic completely isolated
   - Framework-agnostic
   - Easy to understand and maintain

2. **Multiple Interfaces** ✅
   - Same logic accessible via REST, CLI, gRPC
   - Add new adapters without touching domain

3. **Testability** ✅
   - Domain tested in pure isolation
   - Adapters tested with integration tests
   - Clear testing strategy

4. **Event Sourcing Fit** ✅
   - Aggregates are natural domain objects
   - Event store is just another adapter
   - Projections as adapters

5. **Team Scalability** ✅
   - Clear boundaries enable parallel development
   - Domain team vs Adapter teams
   - AI agents can work on isolated slices

### Negative

1. **More Structure** ⚠️
   - Three layers instead of one
   - More files and folders
   - **Mitigation:** VSA CLI generates structure automatically

2. **Learning Curve** ⚠️
   - Developers need to understand hexagonal pattern
   - **Mitigation:** Clear documentation and examples

3. **Indirection** ⚠️
   - Commands go through CommandBus instead of direct calls
   - **Mitigation:** Type safety and better traceability offset this

### Neutral

1. **File Organization**
   - More structured than traditional MVC
   - Clear mental model once learned

2. **Repository Pattern**
   - Necessary for event sourcing anyway
   - Hexagonal makes it explicit

## Language-Specific Implementations

### TypeScript

```typescript
// domain/CartAggregate.ts
@Aggregate('Cart')
export class CartAggregate extends AggregateRoot<CartEvent> {
  @CommandHandler('AddItemCommand')
  addItem(command: AddItemCommand): void { /* domain logic */ }
}

// infrastructure/CommandBus.ts
export class CommandBus {
  async send(command: Command): Promise<void> { /* orchestration */ }
}

// slices/add-item/AddItemController.ts
@RestController()
export class AddItemController {
  constructor(private commandBus: CommandBus) {}
  
  @Post('/:cartId/items')
  async addItem(@Body() req: AddItemRequest): Promise<void> {
    await this.commandBus.send(new AddItemCommand(...));
  }
}
```

### Python

```python
# domain/cart_aggregate.py
@aggregate('Cart')
class CartAggregate(AggregateRoot):
    @command_handler('AddItemCommand')
    def add_item(self, command: AddItemCommand) -> None:
        # domain logic
        pass

# infrastructure/command_bus.py
class CommandBus:
    async def send(self, command: Command) -> None:
        # orchestration
        pass

# slices/add_item/add_item_controller.py
@rest_controller()
class AddItemController:
    def __init__(self, command_bus: CommandBus):
        self.command_bus = command_bus
    
    @post("/{cart_id}/items")
    async def add_item(self, cart_id: str, request: AddItemRequest) -> None:
        await self.command_bus.send(AddItemCommand(...))
```

### Rust

```rust
// domain/cart_aggregate.rs
#[derive(Default)]
pub struct CartAggregate { /* state */ }

#[async_trait]
impl AggregateRoot for CartAggregate {
    type Command = CartCommand;
    
    async fn handle_command(&self, command: Self::Command) -> Result<Vec<Event>> {
        // domain logic
    }
}

// infrastructure/command_bus.rs
pub struct CommandBus {
    repository: Arc<dyn Repository>
}

impl CommandBus {
    pub async fn send(&self, command: Command) -> Result<()> {
        // orchestration
    }
}

// slices/add_item/controller.rs
pub struct AddItemController {
    command_bus: Arc<CommandBus>
}

impl AddItemController {
    pub async fn add_item(&self, cart_id: String, req: AddItemRequest) -> Result<()> {
        self.command_bus.send(AddItemCommand { ... }).await
    }
}
```

## VSA Framework Integration

The VSA framework enforces hexagonal architecture through validation:

```yaml
# vsa.yaml
architecture: hexagonal-event-sourced-vsa

validation:
  architecture:
    enforce_hexagonal: true
    no_business_logic_in_slices: true
    domain_read_only_from_slices: true
    require_command_bus_usage: true
```

**VSA Validates:**
- ✓ Domain has no outward imports
- ✓ Slices only contain thin adapters
- ✓ Business logic only in aggregates
- ✓ Commands routed through CommandBus
- ✓ Aggregates in domain/, not in slices/

## Examples

All examples in `/vsa/examples/` follow this hexagonal pattern:

- **01-todo-list-ts** - Simple hexagonal structure
- **02-library-management-ts** - Multiple contexts, hexagonal
- **05-todo-list-py** - Python hexagonal implementation

## Migration Guide

For existing projects moving to hexagonal architecture:

1. **Identify Domain Logic**
   - Extract business rules from controllers/handlers
   - Move to aggregate `@CommandHandler` methods

2. **Create Domain Layer**
   - Move aggregates to `domain/`
   - Move commands to `domain/commands/`
   - Move events to `domain/events/`

3. **Create Application Layer**
   - Implement CommandBus
   - Implement QueryBus
   - Implement Repository interface

4. **Refactor to Adapters**
   - Convert controllers to thin adapters
   - Remove business logic from controllers
   - Use CommandBus/QueryBus for orchestration

5. **Validate with VSA**
   ```bash
   vsa validate
   ```

## Related ADRs

- ADR-004: Command Handlers in Aggregates (domain layer pattern)
- ADR-006: Domain Organization Pattern (domain folder structure)
- ADR-008: Vertical Slices as Hexagonal Adapters (adapter organization)
- ADR-009: CQRS Pattern Implementation (command/query separation)

## References

- "Hexagonal Architecture" - Alistair Cockburn
- "Clean Architecture" - Robert C. Martin
- "Domain-Driven Design" - Eric Evans
- "Understanding Event Sourcing" - Alexey Zimarev
- "Growing Object-Oriented Software, Guided by Tests" - Freeman & Pryce

---

**Last Updated:** 2025-11-06  
**Supersedes:** None  
**Superseded By:** None

