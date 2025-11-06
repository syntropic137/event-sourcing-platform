# ADR-006: Domain Organization Pattern

**Status:** ✅ Accepted  
**Date:** 2025-11-06  
**Decision Makers:** Architecture Team  
**Related:** Domain-Driven Design, Event Sourcing, Hexagonal Architecture

## Context

In our Hexagonal Event-Sourced Architecture (ADR-005), the **domain layer** is the core of the hexagon. It contains all business logic and must be organized consistently across projects to:

1. **Enable Discovery:** VSA CLI and tools must find aggregates, commands, and events
2. **Prevent Duplication:** Aggregates are shared across features, not duplicated
3. **Support Multiple Aggregates:** Contexts may contain multiple aggregates working together
4. **Facilitate Versioning:** Events need clear organization for version management
5. **Align with DDD:** Follow Domain-Driven Design principles

### The Problem: Where Do Things Live?

Previous examples had inconsistent patterns:
- ❌ Aggregates scattered across feature folders
- ❌ Commands mixed with aggregates or in separate locations
- ❌ Events duplicated or unclear ownership
- ❌ No clear pattern for multiple aggregates per context

### Requirements from eventsourcing-book Analysis

From studying the "Understanding Event Sourcing" book's implementation:
- ✅ `domain/` folder contains ALL aggregates (shared)
- ✅ Commands organized by feature under `domain/commands/`
- ✅ Events in dedicated `domain/events/` folder
- ✅ Multiple aggregates per context supported
- ✅ Clear separation from infrastructure concerns

## Decision

We adopt a **standardized domain folder structure** where the domain layer is completely self-contained and organized by DDD concepts.

### Domain Folder Structure

```
contexts/{context-name}/
└── domain/                          ← DOMAIN LAYER (hexagon center)
    ├── {Aggregate}Aggregate.ts      ← Aggregates (at root, shared)
    ├── {Another}Aggregate.ts        ← Multiple aggregates allowed
    │
    ├── commands/                    ← Commands (organized by feature)
    │   ├── {Feature}Command.ts
    │   └── {AnotherFeature}Command.ts
    │
    ├── queries/                     ← Query definitions (CQRS read)
    │   ├── Get{Resource}Query.ts
    │   └── List{Resources}Query.ts
    │
    └── events/                      ← Domain events (versioned)
        ├── {Event}Event.ts          ← Current version events
        ├── {Another}Event.ts
        ├── _versioned/              ← Old event versions
        │   ├── {Event}Event.v1.ts
        │   └── {Event}Event.v2.ts
        └── _upcasters/              ← Event migration logic
            ├── {Event}Event.v1-v2.ts
            └── {Event}Event.v2-v3.ts
```

### Concrete Example: Cart Context

```
contexts/cart/
└── domain/
    ├── CartAggregate.ts           ← Main aggregate
    ├── InventoryAggregate.ts      ← Supporting aggregate
    ├── PricingAggregate.ts        ← Another supporting aggregate
    │
    ├── commands/
    │   ├── AddItemCommand.ts
    │   ├── RemoveItemCommand.ts
    │   ├── ClearCartCommand.ts
    │   ├── SubmitCartCommand.ts
    │   ├── PublishCartCommand.ts
    │   ├── ChangeInventoryCommand.ts
    │   └── ChangePriceCommand.ts
    │
    ├── queries/
    │   ├── GetCartQuery.ts
    │   ├── GetCartItemsQuery.ts
    │   └── GetCartSummaryQuery.ts
    │
    └── events/
        ├── CartCreatedEvent.ts      (@Event('CartCreated', 'v1'))
        ├── ItemAddedEvent.ts        (@Event('ItemAdded', 'v2'))
        ├── ItemRemovedEvent.ts
        ├── CartSubmittedEvent.ts
        ├── CartPublishedEvent.ts
        ├── InventoryChangedEvent.ts
        ├── PriceChangedEvent.ts
        │
        ├── _versioned/
        │   └── ItemAddedEvent.v1.ts
        │
        └── _upcasters/
            └── ItemAddedEvent.v1-v2.ts
```

## Pattern Details

### 1. Aggregates (Domain Root)

**Location:** `domain/{Aggregate}Aggregate.{ts,py,rs}`

**Characteristics:**
- ✅ Root level of domain folder
- ✅ Multiple aggregates allowed per context
- ✅ Named with `Aggregate` suffix
- ✅ Contain `@CommandHandler` methods (ADR-004)
- ✅ Contain `@EventSourcingHandler` methods
- ✅ Pure business logic only
- ✅ Shared across ALL features/slices

**Example:**
```typescript
// domain/CartAggregate.ts
import { Aggregate, AggregateRoot, CommandHandler, EventSourcingHandler } from '@event-sourcing-platform/typescript';
import { AddItemCommand } from './commands/AddItemCommand';
import { ItemAddedEvent } from './events/ItemAddedEvent';

@Aggregate('Cart')
export class CartAggregate extends AggregateRoot<CartEvent> {
  private items: Map<string, CartItem> = new Map();
  private submitted: boolean = false;

  @CommandHandler('AddItemCommand')
  addItem(command: AddItemCommand): void {
    // Business validation
    if (this.submitted) {
      throw new Error('Cannot add items to submitted cart');
    }
    if (this.items.size >= 3) {
      throw new Error('Cart limited to 3 items');
    }

    // Initialize aggregate if new
    if (!this.id) {
      this.initialize(command.aggregateId);
      this.apply(new CartCreatedEvent(command.aggregateId));
    }

    // Apply event
    this.apply(new ItemAddedEvent(
      command.aggregateId,
      command.productId,
      command.quantity,
      command.price
    ));
  }

  @EventSourcingHandler('ItemAdded')
  private onItemAdded(event: ItemAddedEvent): void {
    this.items.set(event.productId, {
      productId: event.productId,
      quantity: event.quantity,
      price: event.price
    });
  }

  getAggregateType(): string {
    return 'Cart';
  }
}
```

**Multiple Aggregates Pattern:**
```typescript
// domain/CartAggregate.ts - Main aggregate for cart lifecycle
@Aggregate('Cart')
export class CartAggregate extends AggregateRoot<CartEvent> {
  // Cart-specific business logic
}

// domain/InventoryAggregate.ts - Separate consistency boundary
@Aggregate('Inventory')
export class InventoryAggregate extends AggregateRoot<InventoryEvent> {
  // Inventory-specific business logic
}

// domain/PricingAggregate.ts - Another consistency boundary
@Aggregate('Pricing')
export class PricingAggregate extends AggregateRoot<PricingEvent> {
  // Pricing-specific business logic
}
```

### 2. Commands (Write Intentions)

**Location:** `domain/commands/{Feature}Command.{ts,py,rs}`

**Characteristics:**
- ✅ Organized in `domain/commands/` folder
- ✅ Named with `Command` suffix
- ✅ Classes (not interfaces)
- ✅ Contain `aggregateId` property
- ✅ Immutable (readonly properties)
- ✅ No business logic (just data)

**Example:**
```typescript
// domain/commands/AddItemCommand.ts
export class AddItemCommand {
  constructor(
    public readonly aggregateId: string,  // Required: which aggregate
    public readonly productId: string,
    public readonly quantity: number,
    public readonly price: number
  ) {}
}
```

**Optional: Organize by Feature Subdirectories**

For large domains:
```
domain/commands/
├── add-item/
│   └── AddItemCommand.ts
├── remove-item/
│   └── RemoveItemCommand.ts
└── publish-cart/
    └── PublishCartCommand.ts
```

### 3. Queries (Read Requests)

**Location:** `domain/queries/{QueryName}Query.{ts,py,rs}`

**Characteristics:**
- ✅ Organized in `domain/queries/` folder
- ✅ Named with `Query` suffix
- ✅ Define what data to retrieve
- ✅ No business logic
- ✅ CQRS read side

**Example:**
```typescript
// domain/queries/GetCartItemsQuery.ts
export class GetCartItemsQuery {
  constructor(
    public readonly cartId: string
  ) {}
}

// domain/queries/ListCartsQuery.ts
export class ListCartsQuery {
  constructor(
    public readonly customerId?: string,
    public readonly status?: CartStatus,
    public readonly limit?: number,
    public readonly offset?: number
  ) {}
}
```

### 4. Events (Immutable Facts)

**Location:** `domain/events/{Event}Event.{ts,py,rs}`

**Characteristics:**
- ✅ Organized in `domain/events/` folder
- ✅ Named with `Event` suffix
- ✅ Decorated with `@Event(name, version)`
- ✅ Immutable (readonly properties)
- ✅ Represent past tense facts
- ✅ Never deleted (history is immutable)

**Example:**
```typescript
// domain/events/ItemAddedEvent.ts
import { DomainEvent, Event } from '@event-sourcing-platform/typescript';

@Event('ItemAdded', 'v2')
export class ItemAddedEvent extends DomainEvent {
  constructor(
    public readonly aggregateId: string,
    public readonly productId: string,
    public readonly quantity: number,
    public readonly price: number,
    public readonly deviceFingerprint: string  // Added in v2
  ) {
    super();
  }
}
```

**Versioned Events:**
```
domain/events/
├── ItemAddedEvent.ts          ← Current (v2)
├── _versioned/                ← Old versions
│   └── ItemAddedEvent.v1.ts   ← Previous version
└── _upcasters/                ← Migrations
    └── ItemAddedEvent.v1-v2.ts
```

See ADR-007 for complete event versioning details.

## Rationale

### 1. **Aggregates at Domain Root**

**Why:** Aggregates are the most important domain objects. Placing them at the root makes them highly visible and easy to find.

```
domain/
├── CartAggregate.ts       ← Immediately visible
├── InventoryAggregate.ts  ← Clear what aggregates exist
└── commands/              ← Supporting concepts organized below
```

**Benefits:**
- Easy to discover all aggregates
- Clear what consistency boundaries exist
- VSA CLI can scan for `*Aggregate.*` files
- Shared across all features (no duplication)

### 2. **Commands Organized by Feature**

**Why:** Commands represent features/use cases. Organizing them separately makes it clear what operations the domain supports.

```
domain/commands/
├── AddItemCommand.ts       ← Clear list of capabilities
├── RemoveItemCommand.ts
└── SubmitCartCommand.ts
```

**Benefits:**
- Clear list of domain capabilities
- Easy to find command for a feature
- Can organize into subdirectories if needed
- Maps to vertical slices (one slice per command)

### 3. **Queries Separate from Commands**

**Why:** CQRS principle - reads and writes are different concerns.

```
domain/
├── commands/   ← Write side
└── queries/    ← Read side
```

**Benefits:**
- Clear CQRS separation
- Different optimization strategies
- Queries can be added without affecting commands
- Read models independent of write models

### 4. **Events Versioned and Organized**

**Why:** Event sourcing requires careful event management and versioning.

```
domain/events/
├── ItemAddedEvent.ts      ← Current versions
├── _versioned/            ← Historical versions
└── _upcasters/            ← Migration logic
```

**Benefits:**
- Clear event history
- Version evolution tracked
- Upcasters colocated with events
- VSA can validate version consistency

### 5. **Multiple Aggregates Supported**

**Why:** DDD contexts often need multiple aggregates with different consistency boundaries.

**Example: Cart Context**
- `CartAggregate` - Cart lifecycle and items
- `InventoryAggregate` - Product stock levels
- `PricingAggregate` - Product pricing

Each has its own:
- Consistency boundary
- Commands
- Events
- Business rules

They coordinate through:
- Domain events (eventual consistency)
- Sagas (process managers)

## VSA Framework Support

### VSA Configuration

```yaml
# vsa.yaml
domain:
  path: domain/
  
  aggregates:
    path: domain/
    pattern: '*Aggregate.{ts,py,rs}'
    require_aggregate_suffix: true
  
  commands:
    path: domain/commands/
    pattern: '*Command.{ts,py,rs}'
    require_command_suffix: true
    require_aggregate_id: true
  
  queries:
    path: domain/queries/
    pattern: '*Query.{ts,py,rs}'
    require_query_suffix: true
  
  events:
    path: domain/events/
    pattern: '*Event.{ts,py,rs}'
    require_event_suffix: true
    require_version_decorator: true
    versioned_path: _versioned/
    upcasters_path: _upcasters/
```

### VSA CLI Commands

```bash
# Create new aggregate
vsa aggregate add Cart --context cart
# Generates: contexts/cart/domain/CartAggregate.ts

# Create new command
vsa command add AddItem --context cart --aggregate Cart
# Generates: contexts/cart/domain/commands/AddItemCommand.ts

# Create new query
vsa query add GetCartItems --context cart
# Generates: contexts/cart/domain/queries/GetCartItemsQuery.ts

# Create new event
vsa event add ItemAdded --context cart
# Generates: contexts/cart/domain/events/ItemAddedEvent.ts (@Event('ItemAdded', 'v1'))

# List all domain objects
vsa domain list --context cart
# Output:
# Aggregates:
#   - CartAggregate
#   - InventoryAggregate
#   - PricingAggregate
# Commands: 7
# Queries: 3
# Events: 8
```

### VSA Validation

```bash
vsa validate

# Checks:
# ✓ All aggregates in domain/ root
# ✓ All commands in domain/commands/
# ✓ All queries in domain/queries/
# ✓ All events in domain/events/
# ✓ Naming conventions followed
# ✓ Event versions consistent
# ✓ Upcasters exist for version changes
# ✗ ERROR: TaskAggregate found in slices/create-task/ (should be in domain/)
```

## Language-Specific Patterns

### TypeScript

```typescript
// domain/CartAggregate.ts
@Aggregate('Cart')
export class CartAggregate extends AggregateRoot<CartEvent> {
  @CommandHandler('AddItemCommand')
  addItem(command: AddItemCommand): void { /* ... */ }
}

// domain/commands/AddItemCommand.ts
export class AddItemCommand {
  constructor(
    public readonly aggregateId: string,
    public readonly productId: string
  ) {}
}

// domain/events/ItemAddedEvent.ts
@Event('ItemAdded', 'v2')
export class ItemAddedEvent extends DomainEvent {
  constructor(
    public readonly aggregateId: string,
    public readonly productId: string
  ) { super(); }
}
```

### Python

```python
# domain/cart_aggregate.py
@aggregate('Cart')
class CartAggregate(AggregateRoot):
    @command_handler('AddItemCommand')
    def add_item(self, command: AddItemCommand) -> None:
        # ...

# domain/commands/add_item_command.py
from dataclasses import dataclass

@dataclass(frozen=True)
class AddItemCommand:
    aggregate_id: str
    product_id: str

# domain/events/item_added_event.py
@event('ItemAdded', 'v2')
@dataclass(frozen=True)
class ItemAddedEvent(DomainEvent):
    aggregate_id: str
    product_id: str
```

### Rust

```rust
// domain/cart_aggregate.rs
pub struct CartAggregate {
    // state
}

#[async_trait]
impl AggregateRoot for CartAggregate {
    type Command = CartCommand;
    
    async fn handle_command(&self, command: Self::Command) -> Result<Vec<Event>> {
        // ...
    }
}

// domain/commands.rs
#[derive(Debug, Clone)]
pub enum CartCommand {
    AddItem {
        aggregate_id: String,
        product_id: String,
    },
}

// domain/events.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
#[event("ItemAdded", "v2")]
pub struct ItemAddedEvent {
    pub aggregate_id: String,
    pub product_id: String,
}
```

## Consequences

### Positive

1. **Discoverability** ✅
   - All domain objects in predictable locations
   - Easy to find what exists
   - VSA CLI can scan and validate

2. **No Duplication** ✅
   - Aggregates at domain root (shared)
   - Commands organized centrally
   - Events single source of truth

3. **Multiple Aggregates** ✅
   - Clear pattern for multiple aggregates
   - Each has own consistency boundary
   - Coordinate through events

4. **Version Management** ✅
   - Events versioned with clear structure
   - Old versions preserved in _versioned/
   - Upcasters colocated in _upcasters/

5. **CQRS Support** ✅
   - Clear separation of commands and queries
   - Different optimization strategies
   - Read models independent

### Negative

1. **More Folders** ⚠️
   - Four subdirectories (commands, queries, events, _versioned)
   - **Mitigation:** VSA CLI generates structure

2. **Aggregate Discovery** ⚠️
   - Need to know to look in domain/ root
   - **Mitigation:** Clear documentation, VSA validation

### Neutral

1. **Feature to Command Mapping**
   - Commands map 1:1 to features typically
   - Slices organized around commands

2. **Event Organization**
   - All events in one folder
   - Can grow large in big domains
   - Consider subdirectories if needed

## Migration Path

### From Scattered Aggregates

**Before:**
```
slices/
├── create-task/
│   └── TaskAggregate.ts      ❌ Wrong location
└── complete-task/
    └── CompleteTaskCommand.ts ❌ Wrong location
```

**After:**
```
domain/
├── TaskAggregate.ts           ✅ Shared at root
└── commands/
    ├── CreateTaskCommand.ts   ✅ Centralized
    └── CompleteTaskCommand.ts ✅ Centralized

slices/
├── create-task/
│   └── CreateTaskController.ts ✅ Just adapter
└── complete-task/
    └── CompleteTaskController.ts ✅ Just adapter
```

**Steps:**
1. Create `domain/` folder
2. Move aggregates to `domain/` root
3. Create `domain/commands/` and move commands
4. Create `domain/events/` and move events
5. Update imports in slices
6. Run `vsa validate`

## Related ADRs

- ADR-004: Command Handlers in Aggregates (command handler pattern)
- ADR-005: Hexagonal Architecture for Event-Sourced Systems (domain as core)
- ADR-007: Event Versioning and Upcasters (event organization)
- ADR-008: Vertical Slices as Hexagonal Adapters (how slices use domain)
- ADR-009: CQRS Pattern Implementation (command vs query)

## References

- "Domain-Driven Design" - Eric Evans (aggregate patterns)
- "Implementing Domain-Driven Design" - Vaughn Vernon (aggregate design)
- "Understanding Event Sourcing" - Alexey Zimarev (implementation patterns)
- "Hands-On Domain-Driven Design with .NET Core" - Alexey Zimarev

---

**Last Updated:** 2025-11-06  
**Supersedes:** None  
**Superseded By:** None

