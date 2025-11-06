# ADR-008: Vertical Slices as Hexagonal Adapters

**Status:** ✅ Accepted  
**Date:** 2025-11-06  
**Decision Makers:** Architecture Team  
**Related:** Hexagonal Architecture, Vertical Slice Architecture, Feature Organization

## Context

Our platform uses **Hexagonal Architecture** (ADR-005) where the domain is at the center and external concerns are adapters. We also need **feature isolation** for:

1. **Parallel Development:** Multiple developers working on different features without conflicts
2. **AI Agent Parallelization:** Assign isolated features to different AI coding agents
3. **Feature Clarity:** Each feature's code is colocated and self-contained
4. **Independent Deployment:** Features can evolve independently

### The Challenge: Combining Hexagonal and Vertical Slice Architecture

**Hexagonal says:** Organize by layer (Domain, Application, Adapters)  
**VSA says:** Organize by feature (vertical slices)

**How do we reconcile these?**

### Key Insight

```
Hexagonal Architecture = WHAT (architectural rules, dependency flow)
Vertical Slice Architecture = HOW (organizing the adapter layer)
```

**Solution:** Vertical slices ARE the hexagonal adapters, organized by feature.

## Decision

We organize the **adapter layer** using **vertical slices**, where each slice is a feature-isolated adapter that translates external protocols to domain commands/queries.

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  VERTICAL SLICES (Feature-Organized Adapters)               │
│  ├─ add-item/         ← Command slice (REST/CLI/gRPC)       │
│  ├─ remove-item/      ← Command slice                       │
│  ├─ cart-items/       ← Query slice (read model)            │
│  └─ publish-cart/     ← Saga slice (event handler)          │
│           │                                                  │
│           ▼                                                  │
│  ┌──────────────────────────────────────┐                  │
│  │  INFRASTRUCTURE (App Services)       │                  │
│  │  CommandBus, QueryBus, EventBus      │                  │
│  └──────────────────────────────────────┘                  │
│           │                                                  │
│           ▼                                                  │
│  ┌──────────────────────────────────────┐                  │
│  │  DOMAIN (Core - Shared)              │                  │
│  │  Aggregates, Commands, Events        │                  │
│  └──────────────────────────────────────┘                  │
└─────────────────────────────────────────────────────────────┘
```

### Slice Structure

```
contexts/cart/
├── domain/                          ← SHARED (read-only from slices)
│   ├── CartAggregate.ts
│   ├── commands/
│   └── events/
│
├── slices/                          ← VERTICAL SLICES (adapters)
│   ├── add-item/                    ← COMMAND SLICE
│   │   ├── AddItemController.ts     (REST: HTTP → CommandBus)
│   │   ├── AddItemCLI.ts            (CLI: args → CommandBus)
│   │   ├── AddItem.e2e.test.ts      (Feature test)
│   │   └── slice.yaml               (Metadata)
│   │
│   ├── remove-item/                 ← COMMAND SLICE
│   │   ├── RemoveItemController.ts
│   │   └── slice.yaml
│   │
│   ├── cart-items/                  ← QUERY SLICE
│   │   ├── CartItemsProjection.ts   (Event → Read Model)
│   │   ├── CartItemsQueryHandler.ts (Query → Data)
│   │   ├── CartItemsController.ts   (REST: HTTP → QueryBus)
│   │   └── slice.yaml
│   │
│   └── publish-cart/                ← SAGA SLICE
│       ├── PublishCartSaga.ts       (Event → Command + Kafka)
│       ├── KafkaPublisher.ts        (External integration)
│       └── slice.yaml
│
└── infrastructure/                  ← SHARED APP SERVICES
    ├── CommandBus.ts
    ├── QueryBus.ts
    └── EventBus.ts
```

## Slice Types

### 1. Command Slice (Write Operations)

**Purpose:** Translate external protocols → Commands → Domain

**Structure:**
```
slices/add-item/
├── AddItemController.ts    ← REST API adapter
├── AddItemCLI.ts           ← CLI adapter (optional)
├── AddItemGrpcService.ts   ← gRPC adapter (optional)
├── AddItem.e2e.test.ts     ← End-to-end test
└── slice.yaml              ← Metadata
```

**Example:**
```typescript
// slices/add-item/AddItemController.ts
import { RestController, Post, Body, Param, Route } from '@vsa/adapters';
import { CommandBus } from '../../infrastructure/CommandBus';
import { AddItemCommand } from '../../domain/commands/AddItemCommand';

@RestController()
@Route('/api/carts')
export class AddItemController {
  constructor(private commandBus: CommandBus) {}

  @Post('/:cartId/items')
  async addItem(
    @Param('cartId') cartId: string,
    @Body() request: AddItemRequest
  ): Promise<void> {
    // THIN ADAPTER: Just translate HTTP → Command
    const command = new AddItemCommand(
      cartId,
      request.productId,
      request.quantity,
      request.price
    );
    
    // Application service handles the rest
    await this.commandBus.send(command);
  }
}

interface AddItemRequest {
  productId: string;
  quantity: number;
  price: number;
}
```

**Slice Metadata:**
```yaml
# slices/add-item/slice.yaml
name: add-item
type: command
description: Add item to shopping cart
aggregate: CartAggregate
command: AddItemCommand
events:
  - ItemAddedEvent
  - CartCreatedEvent (conditional)

adapters:
  rest:
    controller: AddItemController
    route: POST /api/carts/:cartId/items
  cli:
    controller: AddItemCLI
    command: cart:add-item
  grpc:
    service: AddItemGrpcService
    method: CartService.AddItem

dependencies:
  domain:
    - CartAggregate
    - AddItemCommand
  infrastructure:
    - CommandBus

tests:
  - AddItem.e2e.test.ts

validation:
  max_lines: 50
  no_business_logic: true
```

### 2. Query Slice (Read Operations)

**Purpose:** Build read models from events, serve queries

**Structure:**
```
slices/cart-items/
├── CartItemsProjection.ts      ← Event → Read Model
├── CartItemsQueryHandler.ts    ← Query → Data
├── CartItemsController.ts      ← REST: HTTP → QueryBus
└── slice.yaml
```

**Example:**
```typescript
// slices/cart-items/CartItemsProjection.ts
import { EventHandler } from '@vsa/adapters';
import { ItemAddedEvent } from '../../domain/events/ItemAddedEvent';
import { ItemRemovedEvent } from '../../domain/events/ItemRemovedEvent';

export class CartItemsProjection {
  private items = new Map<string, CartItem[]>();

  @EventHandler('ItemAdded')
  onItemAdded(event: ItemAddedEvent): void {
    const cartItems = this.items.get(event.aggregateId) || [];
    cartItems.push({
      productId: event.productId,
      quantity: event.quantity,
      price: event.price
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
      totalItems: items.reduce((sum, item) => sum + item.quantity, 0)
    };
  }
}

// slices/cart-items/CartItemsController.ts
@RestController()
@Route('/api/carts')
export class CartItemsController {
  constructor(private queryBus: QueryBus) {}

  @Get('/:cartId/items')
  async getItems(@Param('cartId') cartId: string): Promise<CartItemsView> {
    const query = new GetCartItemsQuery(cartId);
    return await this.queryBus.send(query);
  }
}
```

### 3. Saga Slice (Process Manager / Orchestration)

**Purpose:** React to events, coordinate multiple aggregates, handle external integrations

**Structure:**
```
slices/publish-cart/
├── PublishCartSaga.ts      ← Event → Command + External
├── KafkaPublisher.ts       ← External integration
└── slice.yaml
```

**Example:**
```typescript
// slices/publish-cart/PublishCartSaga.ts
import { EventHandler, SagaHandler } from '@vsa/adapters';
import { CartSubmittedEvent } from '../../domain/events/CartSubmittedEvent';
import { PublishCartCommand } from '../../domain/commands/PublishCartCommand';

export class PublishCartSaga {
  constructor(
    private commandBus: CommandBus,
    private kafkaPublisher: KafkaPublisher
  ) {}

  @EventHandler('CartSubmitted')
  @SagaHandler('publish-cart')
  async onCartSubmitted(event: CartSubmittedEvent): Promise<void> {
    try {
      // Publish to external system
      await this.kafkaPublisher.publish('cart-submitted', {
        cartId: event.aggregateId,
        items: event.items,
        totalPrice: event.totalPrice
      });
      
      // Send command back to domain to mark as published
      await this.commandBus.send(new MarkCartPublishedCommand(
        event.aggregateId
      ));
    } catch (error) {
      // Send command to mark publication failed
      await this.commandBus.send(new MarkCartPublicationFailedCommand(
        event.aggregateId,
        error.message
      ));
    }
  }
}
```

## Slice Isolation Rules

### ✅ Allowed

```typescript
// 1. Import from domain (read-only)
import { AddItemCommand } from '../../domain/commands/AddItemCommand';
import { CartAggregate } from '../../domain/CartAggregate';

// 2. Import from infrastructure (app services)
import { CommandBus } from '../../infrastructure/CommandBus';

// 3. Import framework adapters
import { RestController, Post } from '@vsa/adapters';
```

### ❌ Forbidden

```typescript
// 1. NO imports from other slices
import { RemoveItemController } from '../remove-item/RemoveItemController';  // ❌

// 2. NO business logic in slices
export class AddItemController {
  async addItem(request: AddItemRequest): Promise<void> {
    if (cart.items.length >= 3) {  // ❌ Business logic belongs in aggregate
      throw new Error('Cart full');
    }
  }
}

// 3. NO direct aggregate access
const cart = new CartAggregate();  // ❌ Use CommandBus
cart.addItem(command);             // ❌

// 4. NO shared code between slices (except domain + infrastructure)
// Each slice is self-contained
```

## Thin Adapter Principle

Slices MUST be thin (<50 lines recommended):

```typescript
// ✅ GOOD: Thin adapter
@RestController()
export class AddItemController {
  constructor(private commandBus: CommandBus) {}

  @Post('/:cartId/items')
  async addItem(
    @Param('cartId') cartId: string,
    @Body() req: AddItemRequest
  ): Promise<void> {
    const command = new AddItemCommand(cartId, req.productId, req.quantity, req.price);
    await this.commandBus.send(command);
  }
}
// ✅ ~10 lines - perfect!

// ❌ BAD: Business logic in adapter
@RestController()
export class AddItemController {
  @Post('/:cartId/items')
  async addItem(@Param('cartId') cartId: string, @Body() req: AddItemRequest): Promise<void> {
    // ❌ Validation (belongs in aggregate)
    if (!req.productId || req.quantity <= 0) {
      throw new Error('Invalid input');
    }
    
    // ❌ Business rules (belongs in aggregate)
    const cart = await this.repository.load(cartId);
    if (cart.items.length >= 3) {
      throw new Error('Cart full');
    }
    
    // ❌ Direct manipulation (bypass aggregate)
    cart.items.push({ productId: req.productId, quantity: req.quantity });
    await this.repository.save(cart);
  }
}
// ❌ Too much logic, violates hexagonal principles
```

## AI Agent Parallelization

### Isolation Enables Parallel Development

```
Agent 1: slices/add-item/       ← Isolated, can work independently
Agent 2: slices/cart-items/     ← Isolated, can work independently
Agent 3: slices/publish-cart/   ← Isolated, can work independently
Agent 4: domain/                ← Domain (coordinates with all)
```

**No Conflicts:**
- Agents work on different slices simultaneously
- Domain is read-only for slice agents
- Infrastructure is shared (no changes typically)

### VSA Agent Assignment

```bash
# Assign slice to AI agent
vsa agent assign add-item --agent claude-1
# Agent only sees:
# - slices/add-item/ (read-write)
# - domain/ (read-only)
# - infrastructure/ (read-only)

vsa agent assign cart-items --agent claude-2
# Different agent, different slice, no conflicts!

# Assign domain work to specialized agent
vsa agent assign domain --agent claude-3
# This agent can modify domain, slices read-only
```

## Multi-Adapter Support

Same slice can have multiple adapters for different protocols:

```
slices/add-item/
├── adapters/
│   ├── AddItemRestController.ts    ← REST API
│   ├── AddItemCLI.ts               ← Command-line
│   ├── AddItemGrpcService.ts       ← gRPC
│   └── AddItemGraphQLResolver.ts   ← GraphQL
├── AddItem.e2e.test.ts
└── slice.yaml

# All adapters call same CommandBus → Same domain logic!
```

## VSA Framework Support

### Configuration

```yaml
# vsa.yaml
slices:
  path: slices/
  types: [command, query, saga]
  
  command:
    pattern: '{name}/*Controller.{ts,py,rs}'
    must_use: CommandBus
    max_lines: 50
    require_tests: true
    no_business_logic: true
  
  query:
    pattern: '{name}/*Controller.{ts,py,rs}'
    require_projection: true
    must_use: QueryBus
    max_lines: 50
    require_tests: true
  
  saga:
    pattern: '{name}/*Saga.{ts,py,rs}'
    must_use: EventBus
    can_send_commands: true
    require_error_handling: true

validation:
  slices_isolated: true
  no_cross_slice_imports: true
  domain_read_only_from_slices: true
  thin_adapters: true
```

### CLI Commands

```bash
# Create command slice
vsa slice add add-item --type command --adapter rest
# Generates:
# - slices/add-item/AddItemController.ts
# - slices/add-item/AddItem.e2e.test.ts
# - slices/add-item/slice.yaml

# Create query slice
vsa slice add cart-items --type query --adapter rest
# Generates:
# - slices/cart-items/CartItemsProjection.ts
# - slices/cart-items/CartItemsQueryHandler.ts
# - slices/cart-items/CartItemsController.ts
# - slices/cart-items/slice.yaml

# Add adapter to existing slice
vsa adapter add add-item --type cli
# Generates:
# - slices/add-item/AddItemCLI.ts
# Updates slice.yaml

# List all slices
vsa slice list
# Output:
# Command Slices:
#   - add-item (REST, CLI)
#   - remove-item (REST)
#   - submit-cart (REST)
# Query Slices:
#   - cart-items (REST)
#   - cart-summary (REST)
# Saga Slices:
#   - publish-cart

# List all routes
vsa routes list
# Output:
# POST   /api/carts/:cartId/items           → add-item
# DELETE /api/carts/:cartId/items/:itemId   → remove-item
# GET    /api/carts/:cartId/items           → cart-items
# POST   /api/carts/:cartId/submit          → submit-cart
```

### Validation

```bash
vsa validate

# Slice isolation checks:
# ✓ All slices in slices/ folder
# ✓ No cross-slice imports
# ✓ Domain imports are read-only
# ✓ All command slices use CommandBus
# ✓ All query slices use QueryBus
# ✓ All adapters < 50 lines
# ✗ ERROR: add-item/AddItemController.ts contains business logic (line 42)
# ✗ ERROR: cart-items imports from remove-item (cross-slice dependency)
# ✗ ERROR: AddItemController is 87 lines (exceeds 50 line limit)
```

## Language-Specific Implementations

### TypeScript

```typescript
// slices/add-item/AddItemController.ts
import { RestController, Post, Body, Param, Route } from '@vsa/adapters';
import { CommandBus } from '../../infrastructure/CommandBus';
import { AddItemCommand } from '../../domain/commands/AddItemCommand';

@RestController()
@Route('/api/carts')
export class AddItemController {
  constructor(private commandBus: CommandBus) {}

  @Post('/:cartId/items')
  async addItem(@Param('cartId') cartId: string, @Body() req: AddItemRequest): Promise<void> {
    await this.commandBus.send(new AddItemCommand(cartId, req.productId, req.quantity, req.price));
  }
}
```

### Python

```python
# slices/add_item/add_item_controller.py
from vsa.adapters import rest_controller, post, body, param, route
from infrastructure.command_bus import CommandBus
from domain.commands.add_item_command import AddItemCommand

@rest_controller()
@route("/api/carts")
class AddItemController:
    def __init__(self, command_bus: CommandBus):
        self.command_bus = command_bus
    
    @post("/{cart_id}/items")
    async def add_item(self, cart_id: str, request: AddItemRequest) -> None:
        command = AddItemCommand(cart_id, request.product_id, request.quantity, request.price)
        await self.command_bus.send(command)
```

### Rust

```rust
// slices/add_item/controller.rs
use vsa_adapters::{RestController, Post, Body, Param, Route};
use crate::infrastructure::command_bus::CommandBus;
use crate::domain::commands::AddItemCommand;

#[rest_controller]
#[route("/api/carts")]
pub struct AddItemController {
    command_bus: Arc<CommandBus>,
}

impl AddItemController {
    #[post("/{cart_id}/items")]
    pub async fn add_item(&self, cart_id: String, request: AddItemRequest) -> Result<()> {
        let command = AddItemCommand {
            aggregate_id: cart_id,
            product_id: request.product_id,
            quantity: request.quantity,
            price: request.price,
        };
        self.command_bus.send(command).await
    }
}
```

## Consequences

### Positive

1. **Feature Isolation** ✅
   - Each slice is self-contained
   - No cross-slice dependencies
   - Easy to understand feature scope

2. **Parallel Development** ✅
   - Multiple developers on different slices
   - No merge conflicts
   - Independent feature evolution

3. **AI Agent Parallelization** ✅
   - Assign slices to different AI agents
   - Clear boundaries
   - No conflicts

4. **Hexagonal Compliance** ✅
   - Slices are thin adapters
   - Domain remains pure
   - Dependencies point inward

5. **Testability** ✅
   - Each slice has own e2e tests
   - Domain tested separately
   - Integration tests per slice

### Negative

1. **More Files** ⚠️
   - One folder per feature
   - **Mitigation:** Clear organization, easy to navigate

2. **Adapter Duplication** ⚠️
   - Multiple adapters may have similar code
   - **Mitigation:** Thin adapters minimize duplication

3. **Learning Curve** ⚠️
   - Developers need to understand slice boundaries
   - **Mitigation:** Clear documentation, VSA validation

### Neutral

1. **Slice Granularity**
   - One slice per command/query typically
   - Can combine if features are tightly related

2. **Infrastructure Sharing**
   - CommandBus, QueryBus shared across slices
   - This is intentional (application services)

## Related ADRs

- ADR-005: Hexagonal Architecture for Event-Sourced Systems (architectural foundation)
- ADR-006: Domain Organization Pattern (domain shared by slices)
- ADR-009: CQRS Pattern Implementation (command vs query slices)
- ADR-010: Decorator Patterns for Framework Integration (adapter decorators)

## References

- "Vertical Slice Architecture" - Jimmy Bogard
- "Hexagonal Architecture" - Alistair Cockburn
- "Clean Architecture" - Robert C. Martin
- "Feature Slices for ASP.NET Core" - Jimmy Bogard

---

**Last Updated:** 2025-11-06  
**Supersedes:** None  
**Superseded By:** None

