---
sidebar_position: 2
---

# Bounded Contexts

Learn how to organize vertical slices into bounded contexts using Domain-Driven Design principles.

## What is a Bounded Context?

A **bounded context** is an explicit boundary within which a domain model exists. It defines where specific terms, rules, and models apply. Different contexts can have different models for the same concept.

### Example: "Product" in Different Contexts

```
Catalog Context:
  Product = {id, name, description, images, price}
  
Inventory Context:
  Product = {id, sku, quantity, location, reorderPoint}
  
Shipping Context:
  Product = {id, weight, dimensions, shippingClass}
```

**Key Insight:** Same concept, different models based on context needs.

## Why Bounded Contexts?

### Without Contexts (Monolithic Model)

```
src/
├── models/
│   └── Product.ts     ← One model for everything
├── services/
│   ├── CatalogService.ts
│   ├── InventoryService.ts
│   └── ShippingService.ts
```

**Problems:**
- 🚫 Bloated models with fields from all concerns
- 🚫 Changes ripple everywhere
- 🚫 No clear boundaries
- 🚫 Teams step on each other
- 🚫 Can't scale independently

### With Contexts (Separated Models)

```
src/contexts/
├── catalog/           ← Owns Product model for catalog
│   └── ...
├── inventory/         ← Owns Product model for inventory
│   └── ...
└── shipping/          ← Owns Product model for shipping
    └── ...
```

**Benefits:**
- ✅ Each context has focused models
- ✅ Changes stay localized
- ✅ Clear boundaries
- ✅ Teams work independently
- ✅ Can scale/deploy separately

## Structure in VSA

### File System Organization

```
src/contexts/
├── catalog/                  ← Bounded Context 1
│   ├── add-product/         ← Vertical Slice
│   │   ├── AddProductCommand.ts
│   │   ├── ProductAddedEvent.ts
│   │   ├── AddProductHandler.ts
│   │   └── AddProduct.test.ts
│   ├── update-product/      ← Another Slice
│   │   └── ...
│   └── _shared/             ← Shared within catalog only
│       └── ProductAggregate.ts
│
├── orders/                   ← Bounded Context 2
│   ├── place-order/
│   ├── cancel-order/
│   ├── _subscribers/        ← Event handlers from OTHER contexts
│   │   └── ProductStockChanged.handler.ts
│   └── _shared/
│
└── inventory/                ← Bounded Context 3
    ├── adjust-stock/
    ├── reserve-stock/
    └── _shared/
```

### Configuration

Define contexts in `vsa.yaml`:

```yaml
version: 1
language: typescript
root: src/contexts

bounded_contexts:
  - name: catalog
    description: Product catalog management
    publishes:
      - ProductAdded
      - ProductRemoved
    subscribes: []
  
  - name: orders
    description: Order processing
    publishes:
      - OrderPlaced
      - OrderCancelled
    subscribes:
      - ProductStockChanged
  
  - name: inventory
    description: Stock management
    publishes:
      - ProductStockChanged
    subscribes:
      - OrderPlaced
```

## Communication Between Contexts

### ❌ Direct Imports (Forbidden)

```typescript
// orders/place-order/PlaceOrderHandler.ts

// ❌ FORBIDDEN: Direct cross-context import
import { GetProductQuery } from '../../../catalog/get-product/GetProductQuery';

export class PlaceOrderHandler {
  async handle(command: PlaceOrderCommand) {
    // ❌ Violates bounded context boundary
    const product = await this.catalogQuery.execute(new GetProductQuery());
  }
}
```

**Why forbidden:**
- Creates tight coupling
- Can't deploy independently
- Changes in catalog break orders
- Defeats purpose of bounded contexts

### ✅ Integration Events (Correct)

```typescript
// Step 1: Inventory publishes event
// contexts/inventory/adjust-stock/AdjustStockHandler.ts
await this.eventBus.publish(
  new ProductStockChanged({
    productId: aggregate.id,
    newQuantity: aggregate.quantity,
    timestamp: new Date()
  })
);

// Step 2: Orders subscribes to event
// contexts/orders/_subscribers/ProductStockChanged.handler.ts
import { ProductStockChanged } from '../../../_shared/integration-events/inventory';

export class ProductStockChangedHandler {
  async handle(event: ProductStockChanged) {
    // Update local read model
    await this.productCatalog.updateStock(
      event.productId,
      event.newQuantity
    );
  }
}
```

**Benefits:**
- Loose coupling
- Asynchronous communication
- Can deploy independently
- Changes localized

## Context Boundaries

### Enforcing Boundaries

VSA validates boundaries automatically:

```bash
$ vsa validate

❌ Boundary Violation
   File: contexts/orders/place-order/PlaceOrderHandler.ts
   Line: 5
   
   import { GetProductQuery } from '../../../catalog/get-product';
   
   ⚠️  Direct import across bounded contexts (orders → catalog)
   
   💡 Suggestion:
      1. Catalog publishes ProductAdded event
      2. Orders maintains local product read model
      3. Query local read model instead
```

### Read Models

Each context maintains its own read models:

```typescript
// Catalog Context: Full product data
interface CatalogProduct {
  id: string;
  name: string;
  description: string;
  images: string[];
  price: number;
  category: string;
}

// Orders Context: Only what orders need
interface OrderProduct {
  id: string;
  name: string;      // For display
  price: number;     // For total calculation
  available: boolean; // For validation
}

// Shipping Context: Only what shipping needs
interface ShippingProduct {
  id: string;
  weight: number;
  dimensions: { length, width, height };
  shippingClass: string;
}
```

**Key Point:** Each context has its own model updated via events.

## Context Mapping

### Types of Relationships

#### 1. Publisher-Subscriber

```
Inventory (Publisher) ──event──> Orders (Subscriber)
```

```yaml
# vsa.yaml
bounded_contexts:
  - name: inventory
    publishes: [ProductStockChanged]
  
  - name: orders
    subscribes: [ProductStockChanged]
```

#### 2. Orchestrator-Worker

```
Orders (Orchestrator)
  ├──command──> Payments (Worker)
  ├──command──> Inventory (Worker)
  └──command──> Shipping (Worker)
```

#### 3. Shared Kernel

```
contexts/
  catalog/
  orders/
_shared/              ← Minimal shared code
  integration-events/
  infrastructure/
```

**Rule:** Keep shared kernel minimal!

## Example: E-Commerce System

### Context Boundaries

```
┌──────────────────┐     ┌──────────────────┐
│   Catalog        │     │   Orders         │
│                  │     │                  │
│  Add Product     │     │  Place Order     │
│  Update Product  │     │  Cancel Order    │
│  Remove Product  │────>│  View Orders     │
│                  │     │                  │
└──────────────────┘     └────────┬─────────┘
         │                        │
         │ ProductAdded           │ OrderPlaced
         │ ProductRemoved         │
         ↓                        ↓
┌──────────────────┐     ┌──────────────────┐
│   Inventory      │     │   Shipping       │
│                  │     │                  │
│  Adjust Stock    │<────│  Ship Order      │
│  Reserve Stock   │     │  Track Package   │
│  Release Stock   │     │                  │
└──────────────────┘     └──────────────────┘
         │
         │ StockChanged
         ↓
┌──────────────────┐
│  Notifications   │
│                  │
│  Send Email      │
│  Send SMS        │
└──────────────────┘
```

### Configuration

```yaml
bounded_contexts:
  - name: catalog
    description: Product catalog
    publishes: [ProductAdded, ProductRemoved, ProductUpdated]
    subscribes: []
  
  - name: orders
    description: Order processing
    publishes: [OrderPlaced, OrderCancelled]
    subscribes: [ProductAdded, ProductRemoved, StockChanged]
  
  - name: inventory
    description: Stock management
    publishes: [StockChanged, StockReserved]
    subscribes: [OrderPlaced, OrderCancelled]
  
  - name: shipping
    description: Shipment tracking
    publishes: [ShipmentCreated, ShipmentDelivered]
    subscribes: [OrderPlaced]
  
  - name: notifications
    description: Customer notifications
    publishes: []
    subscribes: [OrderPlaced, ShipmentDelivered, StockChanged]
```

## Special Folders

### `_subscribers/`

Handlers for integration events from OTHER contexts:

```
contexts/orders/
├── place-order/        ← Regular feature
├── cancel-order/       ← Regular feature
└── _subscribers/       ← Event handlers
    ├── ProductStockChanged.handler.ts
    └── PaymentCompleted.handler.ts
```

**Naming:** `{EventName}.handler.{ext}`

### `_shared/`

Code shared WITHIN a context:

```
contexts/orders/
├── place-order/
├── cancel-order/
└── _shared/            ← Shared within orders only
    ├── OrderAggregate.ts
    ├── OrderValidator.ts
    └── projections/
        └── OrderSummary.ts
```

**Rule:** Not accessible by other contexts!

### `_shared/integration-events/`

Single source of truth for integration events:

```
_shared/
└── integration-events/
    ├── catalog/        ← Published BY catalog
    │   ├── ProductAdded.ts
    │   └── ProductRemoved.ts
    ├── orders/         ← Published BY orders
    │   └── OrderPlaced.ts
    └── inventory/      ← Published BY inventory
        └── StockChanged.ts
```

**Rule:** Events defined exactly once!

## Domain Events vs Integration Events

### Domain Events (Internal)

```typescript
// contexts/catalog/add-product/ProductAddedEvent.ts
export class ProductAddedEvent {
  // Rich internal details
  productId: string;
  name: string;
  description: string;
  price: number;
  cost: number;              // Internal
  supplier: string;          // Internal
  margin: number;            // Internal
  addedBy: string;           // Internal
  internalNotes: string;     // Internal
}
```

**Characteristics:**
- Rich with internal details
- Lives in feature folder
- Can change freely
- Used for event sourcing

### Integration Events (External)

```typescript
// _shared/integration-events/catalog/ProductAdded.ts
export class ProductAdded {
  // Minimal stable contract
  productId: string;
  name: string;
  price: number;
  timestamp: Date;
  // No internal details!
}
```

**Characteristics:**
- Minimal stable contract
- Lives in `_shared/integration-events/`
- Versioned carefully
- Published to other contexts

## Best Practices

### 1. Keep Contexts Focused

```
✅ Good: Focused contexts
- catalog   (product info)
- orders    (order processing)
- inventory (stock management)

❌ Bad: Bloated contexts
- products  (catalog + inventory + pricing + suppliers)
```

### 2. Minimize Shared Code

```
✅ Good: Minimal sharing
_shared/
└── integration-events/

❌ Bad: Excessive sharing
_shared/
├── models/
├── services/
├── repositories/
└── utilities/
```

### 3. Use Events for Communication

```
✅ Good: Event-driven
Inventory ──StockChanged──> Orders
Orders    ──OrderPlaced──> Inventory

❌ Bad: Direct calls
Orders calls Inventory.checkStock()
Inventory calls Orders.getOrder()
```

### 4. Each Context Owns Its Data

```
✅ Good: Separate databases
catalog_db    (catalog owns)
orders_db     (orders owns)
inventory_db  (inventory owns)

❌ Bad: Shared database
shared_db
├── products
├── orders
└── stock
```

## Testing Bounded Contexts

### Unit Tests (Within Context)

```typescript
// contexts/orders/place-order/PlaceOrder.test.ts
describe('PlaceOrder', () => {
  it('should place order successfully', async () => {
    // Test within orders context only
  });
});
```

### Integration Tests (Cross-Context)

```typescript
// tests/integration/OrderPlacement.test.ts
describe('Order Placement Flow', () => {
  it('should reserve stock when order placed', async () => {
    // 1. Place order (orders context)
    await orderHandler.handle(placeOrderCmd);
    
    // 2. Verify stock reserved (inventory context)
    const stock = await inventoryQuery.getStock(productId);
    expect(stock.reserved).toBe(5);
  });
});
```

## Migration Strategy

### Starting Fresh

```bash
# 1. Define contexts
vsa init --language typescript

# 2. Configure vsa.yaml
# Add bounded contexts

# 3. Generate features
vsa generate catalog add-product
vsa generate orders place-order
vsa generate inventory adjust-stock

# 4. Implement features
# 5. Define integration events
# 6. Connect contexts via events
```

### Refactoring Existing Code

```bash
# 1. Identify natural boundaries
# Look for: Different teams, different concerns, different change rates

# 2. Create context folders
mkdir -p src/contexts/{catalog,orders,inventory}

# 3. Move features one at a time
# Start with least dependent features

# 4. Replace direct calls with events
# Gradually introduce event-driven communication

# 5. Validate boundaries
vsa validate
```

## Real-World Example: Library Management

```
contexts/
├── catalog/              ← Book information
│   ├── add-book/
│   ├── update-book/
│   └── remove-book/
│
├── lending/              ← Borrowing/returning
│   ├── borrow-book/
│   ├── return-book/
│   ├── mark-overdue/
│   └── _subscribers/
│       └── BookRemoved.handler.ts
│
├── members/              ← Member management
│   ├── register-member/
│   ├── suspend-member/
│   └── _subscribers/
│       └── BookOverdue.handler.ts
│
└── notifications/        ← Alerts and reminders
    ├── send-overdue-notice/
    └── _subscribers/
        ├── BookBorrowed.handler.ts
        ├── BookOverdue.handler.ts
        └── MemberSuspended.handler.ts
```

**Event Flow:**
1. `lending` publishes `BookBorrowed` → `notifications` sends email
2. `lending` publishes `BookOverdue` → `members` updates member status
3. `catalog` publishes `BookRemoved` → `lending` cancels reservations

## Next Steps

- **[Integration Events](./integration-events)** - Deep dive into cross-context communication
- **[Convention Over Configuration](./convention-over-configuration)** - Standard patterns
- **Examples** - Check `vsa/examples/02-library-management-ts` in the repository

## Resources

- [Bounded Context - Martin Fowler](https://martinfowler.com/bliki/BoundedContext.html)
- [Domain-Driven Design - Eric Evans](https://www.domainlanguage.com/ddd/)
- [Context Mapping](https://www.infoq.com/articles/ddd-contextmapping/)

---

**Ready for more?** Learn about [Integration Events](./integration-events) for cross-context communication.

