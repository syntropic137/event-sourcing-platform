# ADR-003: Bounded Context Structure

**Status:** Accepted  
**Date:** 2025-11-05  
**Deciders:** Architecture Team  
**Context:** How to organize vertical slices within bounded contexts

## Context and Problem Statement

Vertical slice architecture works best with bounded contexts from Domain-Driven Design. How should we structure:
- Bounded contexts in the file system?
- Vertical slices within contexts?
- Communication between contexts?
- Shared code within and across contexts?

## Decision Drivers

- **DDD Alignment**: Follow Domain-Driven Design principles
- **Event Sourcing**: Support event-driven communication
- **Clear Boundaries**: Explicit context boundaries
- **Autonomy**: Each context should be independent
- **Searchability**: Easy to find features
- **Scalability**: Support microservices extraction

## Considered Options

### Option A: Flat Structure with Prefixes
```
vertical-slices/
  warehouse.inventory.create-product/
  warehouse.shipping.ship-order/
  sales.orders.place-order/
```

### Option B: Explicit Context Folders (Chosen)
```
contexts/
  warehouse/
    inventory/
      create-product/
    shipping/
      ship-order/
  sales/
    orders/
      place-order/
```

### Option C: Contexts as Top-Level Packages
```
warehouse/
  vertical-slices/
    inventory/
sales/
  vertical-slices/
    orders/
```

## Decision Outcome

**Chosen option: Option B - Explicit Context Folders**

### Structure Definition

```
src/
  contexts/                      ← All bounded contexts here
    {context}/                   ← One bounded context
      {feature-area}/            ← Domain area (products, orders, etc.)
        {operation}/             ← Specific operation (contains *Command.*)
          - {Operation}Command.{ext}
          - {Event}Event.{ext}
          - {Operation}Handler.{ext}
          - {Operation}.test.{ext}
      
      _subscribers/              ← Handlers for OTHER contexts' events
        {Event}.handler.{ext}
      
      _shared/                   ← Internal to this context only
        domain/
        repositories/
        projections/
  
  _shared/                       ← Cross-context shared (minimal!)
    integration-events/          ← Single source of truth
      {publisher-context}/
        {IntegrationEvent}.{ext}
    infrastructure/
```

### Nesting Rules

1. **Unlimited Depth**: Support as many levels as needed
2. **Operation Detection**: Folder containing `*Command.*` files is an operation
3. **Flexibility**: Can have 2-3 levels or more

**Examples:**
```
contexts/warehouse/products/create-product/           ← 4 levels
contexts/sales/place-order/                           ← 3 levels  
contexts/warehouse/inventory/products/create-product/ ← 5 levels (if needed)
```

### Special Folders

#### `_subscribers/` - Integration Event Handlers
- Contains handlers for events from OTHER contexts
- One handler per integration event
- Naming: `{EventName}.handler.{ext}`

#### `_shared/` - Context-Internal Shared
- Code shared WITHIN a bounded context
- NOT accessible by other contexts
- Aggregates, repositories, projections, utilities

#### `_shared/integration-events/` - Cross-Context Events
- ONLY location for integration events
- Organized by publisher context
- Single source of truth (no duplication)

### Communication Between Contexts

```
┌─────────────────────────────────────────┐
│ Warehouse Context                        │
│   products/adjust-stock/                 │
│     Handler publishes:                   │
│       ProductStockChanged                │
└────────────────┬────────────────────────┘
                 │
                 ↓ Via Event Bus
     _shared/integration-events/
       warehouse/ProductStockChanged.ts
                 │
                 ↓ Subscribe
┌────────────────┴────────────────────────┐
│ Sales Context                            │
│   _subscribers/                          │
│     ProductStockChanged.handler.ts       │
└──────────────────────────────────────────┘
```

**Key Rule**: Contexts communicate ONLY via integration events, never direct imports

### Positive Consequences

- **Clear Boundaries**: Each context is a folder, easy to see
- **Autonomy**: Contexts can evolve independently
- **Event-Driven**: Forced to use events for cross-context communication
- **Microservices Ready**: Easy to extract context into separate service
- **Team Ownership**: Teams can own entire contexts
- **Searchability**: Clear hierarchy makes finding code easy

### Negative Consequences

- **More Folders**: Deeper nesting than flat structure
- **Path Length**: File paths can be longer
- **Duplication**: Read models might duplicate data from other contexts

## Boundary Enforcement

### VSA Tool Validates

1. **No Direct Cross-Context Imports**
   ```typescript
   // ❌ FORBIDDEN
   import { GetProductQuery } from '../../../warehouse/products/get-product';
   
   // ✅ CORRECT: Use integration events
   ```

2. **Integration Events Single Source**
   ```typescript
   // ✅ CORRECT: Import from shared
   import { ProductStockChanged } from '../../../_shared/integration-events/warehouse';
   ```

3. **Subscribers Match Declarations**
   ```yaml
   # vsa.yaml
   bounded_contexts:
     - name: sales
       subscribes: [ProductStockChanged]  # Must have handler in _subscribers/
   ```

### Validation Output Example

```
❌ Boundary Violation in sales/orders/place-order/PlaceOrderHandler.ts:5

   5: import { GetProductQuery } from '../../../warehouse/products/get-product';
      ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   
   Direct import across bounded contexts (sales → warehouse)
   
   💡 Suggestion: Use events instead
      1. Warehouse publishes ProductStockChanged events
      2. Sales maintains local product read model
      3. Query local read model instead
```

## Integration with Event Sourcing

### Domain Events (Internal)
- Stay within operation folder
- Used for event sourcing
- Rich with domain details
- Can change freely

### Integration Events (External)
- Live in `_shared/integration-events/`
- Minimal, stable contract
- Versioned carefully
- Published to other contexts

### Example Flow

```typescript
// 1. Domain event (internal to warehouse)
// contexts/warehouse/products/adjust-stock/StockAdjustedEvent.ts
export class StockAdjustedEvent extends BaseDomainEvent {
  // Rich details: who, why, cost impact, etc.
}

// 2. Handler transforms and publishes integration event
// contexts/warehouse/products/adjust-stock/AdjustStockHandler.ts
await this.repository.save(aggregate);  // Persists domain events

await this.eventBus.publish(
  new ProductStockChanged({              // Integration event
    productId: aggregate.id,
    newQuantity: aggregate.currentStock,
    timestamp: new Date(),
  })
);

// 3. Other contexts subscribe
// _shared/integration-events/warehouse/ProductStockChanged.ts
export class ProductStockChanged { /* minimal contract */ }

// 4. Sales context handles it
// contexts/sales/_subscribers/ProductStockChanged.handler.ts
@SubscribesTo(ProductStockChanged)
async handle(event: ProductStockChanged) {
  // Update local read model
}
```

## Migration Path

### Greenfield Project
1. Start with bounded contexts from day 1
2. Use VSA to generate structure
3. Enforce boundaries from the start

### Brownfield Project (Future)
1. Start with single context
2. Add more contexts gradually
3. Extract features into contexts
4. Use VSA to validate structure

## Examples

### Simple E-Commerce System
```
contexts/
  catalog/           ← Products, categories
  orders/            ← Order processing
  payments/          ← Payment handling
  shipping/          ← Shipment tracking
  notifications/     ← Email/SMS
```

### Warehouse Management System
```
contexts/
  warehouse/         ← Inventory, locations
  shipping/          ← Outbound shipments
  receiving/         ← Inbound shipments
  reporting/         ← Analytics
```

## Links

- [Domain-Driven Design by Eric Evans](https://www.domainlanguage.com/ddd/)
- [Bounded Context Pattern](https://martinfowler.com/bliki/BoundedContext.html)
- [Context Mapping](https://www.infoq.com/articles/ddd-contextmapping/)
- [Event-Driven Architecture](https://martinfowler.com/articles/201701-event-driven.html)

