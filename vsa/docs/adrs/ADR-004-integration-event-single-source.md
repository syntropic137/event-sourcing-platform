# ADR-004: Integration Event Single Source of Truth

**Status:** Accepted  
**Date:** 2025-11-05  
**Deciders:** Architecture Team  
**Context:** Preventing event duplication across bounded contexts

## Context and Problem Statement

When bounded contexts communicate via integration events, how do we ensure:
- No duplicate event definitions?
- Consistent event schemas across contexts?
- Clear ownership of events?
- Easy discovery of events?

## Decision Drivers

- **No Duplication**: Events defined exactly once
- **Consistency**: Same event type everywhere
- **Ownership**: Clear which context publishes
- **Discovery**: Easy to find all integration events
- **Versioning**: Support event schema evolution
- **Type Safety**: Compiler/IDE support

## Considered Options

### Option 1: Events Defined by Publisher
Each context defines events it publishes
- ❌ Risk of duplication
- ❌ Subscribers might copy definitions
- ❌ Hard to discover all events

### Option 2: Shared Event Registry (Chosen)
Single source of truth in `_shared/integration-events/`
- ✅ Defined exactly once
- ✅ Organized by publisher
- ✅ Everyone imports from same location
- ✅ VSA validates no duplication

### Option 3: Event Schema Repository
Separate package/repository for events
- ✅ Clear separation
- ❌ Extra dependency
- ❌ Versioning complexity

## Decision Outcome

**Chosen option: Option 2 - Shared Event Registry**

### Structure

```
_shared/
  integration-events/
    warehouse/                   ← Published BY warehouse
      ProductStockChanged.ts
      ProductCreated.ts
      OrderShipped.ts
    
    sales/                       ← Published BY sales
      OrderPlaced.ts
      OrderCancelled.ts
      CustomerRegistered.ts
    
    payments/                    ← Published BY payments
      PaymentCompleted.ts
      PaymentFailed.ts
```

**Rationale**: Organization by publisher makes ownership clear

### Integration Event Definition

```typescript
// _shared/integration-events/warehouse/ProductStockChanged.ts

export class ProductStockChanged {
  readonly eventType = 'ProductStockChanged';
  readonly version = 1;  // For schema evolution
  
  constructor(
    public readonly productId: string,
    public readonly newQuantity: number,
    public readonly timestamp: Date,
  ) {}
}
```

**Key Properties:**
- Minimal fields (stable contract)
- Versioned for evolution
- Immutable (readonly)
- No framework dependency (pure data)

### Publisher Usage

```typescript
// contexts/warehouse/products/adjust-stock/AdjustStockHandler.ts

import { ProductStockChanged } from '../../../../_shared/integration-events/warehouse';

export class AdjustStockHandler {
  async handle(cmd: AdjustStockCommand) {
    // 1. Domain logic (persists domain events)
    await this.repository.save(aggregate);
    
    // 2. Publish integration event AFTER successful save
    await this.eventBus.publish(
      new ProductStockChanged(
        aggregate.id,
        aggregate.currentStock,
        new Date()
      )
    );
  }
}
```

### Subscriber Usage

```typescript
// contexts/sales/_subscribers/ProductStockChanged.handler.ts

import { ProductStockChanged } from '../../../_shared/integration-events/warehouse';
//     ^^^^^^^^^^^^^^^^^^^^^^^^ Same import, no duplication!

export class ProductStockChangedHandler {
  @SubscribesTo(ProductStockChanged)
  async handle(event: ProductStockChanged) {
    // Update local read model
    await this.productCatalog.updateStock(
      event.productId,
      event.newQuantity
    );
  }
}
```

**Key Benefit**: Type safety guaranteed, same definition everywhere

### Domain Events vs Integration Events

```
┌─────────────────────────────────────────────────────┐
│ Warehouse Context                                    │
│                                                      │
│ Domain Events (Internal, Rich)                      │
│ ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━                   │
│ products/adjust-stock/                              │
│   StockAdjustedEvent.ts                             │
│     - productId                                      │
│     - oldQuantity                                    │
│     - newQuantity                                    │
│     - adjustmentReason                               │
│     - warehouseLocation                              │
│     - adjustedBy                                     │
│     - costImpact                                     │
│     - ... more internal details                      │
│                                                      │
│ ↓ Transform                                          │
│                                                      │
│ Integration Event (External, Minimal)               │
│ ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━   │
│ _shared/integration-events/warehouse/               │
│   ProductStockChanged.ts                            │
│     - productId                                      │
│     - newQuantity                                    │
│     - timestamp                                      │
│                                                      │
└─────────────────────────────────────────────────────┘
```

**Rule**: Not every domain event becomes an integration event!

### VSA Validation

#### Rule 1: No Duplicate Definitions

```bash
vsa validate --check-duplication

❌ Duplicate integration event definition
   Event: ProductStockChanged
   Found in:
     - _shared/integration-events/warehouse/ProductStockChanged.ts ✓
     - contexts/sales/_shared/ProductStockChanged.ts ❌
   
   💡 Fix: Remove duplicate, import from _shared/integration-events/
```

#### Rule 2: Integration Events Only in Shared

```bash
❌ Integration event in wrong location
   Event: OrderPlaced
   Found: contexts/sales/orders/place-order/OrderPlaced.ts
   Should be: _shared/integration-events/sales/OrderPlaced.ts
   
   💡 Integration events must be in _shared/integration-events/{publisher}/
```

#### Rule 3: Publishers Match Config

```yaml
# vsa.yaml
bounded_contexts:
  - name: warehouse
    publishes: [ProductStockChanged, ProductCreated]
```

```bash
❌ Published event not declared in config
   Context: warehouse
   Event: OrderShipped (not in publishes list)
   
   💡 Add to vsa.yaml or remove publish call
```

### Positive Consequences

- **Zero Duplication**: Events defined exactly once
- **Type Safety**: TypeScript/Python types consistent
- **Clear Ownership**: Folder structure shows publisher
- **Easy Discovery**: All events in one place
- **Validation**: VSA catches duplication automatically
- **Refactoring**: Change event, all usages update
- **Documentation**: Single place to document events

### Negative Consequences

- **Coupling**: All contexts depend on shared folder
- **Versioning**: Breaking changes affect all subscribers
- **Monorepo**: Works best in monorepo setup

## Event Schema Evolution

### Versioning Strategy

```typescript
// Version 1
export class ProductStockChanged {
  readonly version = 1;
  constructor(
    public readonly productId: string,
    public readonly newQuantity: number,
  ) {}
}

// Version 2 (additive change)
export class ProductStockChanged {
  readonly version = 2;
  constructor(
    public readonly productId: string,
    public readonly newQuantity: number,
    public readonly timestamp?: Date,  // Optional for backward compatibility
  ) {}
}

// Version 3 (breaking change) → New event
export class ProductStockChangedV3 {
  readonly version = 3;
  // ... breaking changes
}
```

**Rule**: Additive changes OK, breaking changes need new event

### Migration Path

1. Publisher starts emitting V2
2. Subscribers handle both V1 and V2
3. After all subscribers updated, deprecate V1
4. Eventually remove V1 handling

## Configuration in vsa.yaml

```yaml
integration_events:
  path: ../_shared/integration-events/
  
  events:
    ProductStockChanged:
      publisher: warehouse
      subscribers: [sales, notifications]
      description: "Emitted when product stock quantity changes"
      version: 1
      
    OrderPlaced:
      publisher: sales
      subscribers: [warehouse, payments, notifications]
      description: "Emitted when customer places an order"
      version: 1
```

**Benefits:**
- Documents event flow
- Validates publishers/subscribers
- Tracks versions
- Generates manifest

## CLI Commands

### Generate Integration Event

```bash
vsa generate integration-event ProductStockChanged \
  --publisher warehouse \
  --subscribers sales,notifications

✅ Created _shared/integration-events/warehouse/ProductStockChanged.ts
✅ Updated vsa.yaml
```

### Generate Subscriber Handler

```bash
vsa generate subscriber ProductStockChanged --context sales

✅ Created contexts/sales/_subscribers/ProductStockChanged.handler.ts
```

### Validate Integration Events

```bash
vsa validate --integration-events

✅ ProductStockChanged
   Publisher: warehouse ✓
   Subscribers: sales ✓, notifications ✓

❌ OrderPlaced
   Publisher: sales ✓
   Subscriber: warehouse ✗ (handler not found)
```

## Comparison to Domain Events

| Aspect | Domain Event | Integration Event |
|--------|--------------|-------------------|
| **Location** | Operation folder | `_shared/integration-events/` |
| **Audience** | Single context | Multiple contexts |
| **Purpose** | Event sourcing | Context communication |
| **Details** | Rich, internal | Minimal, stable |
| **Changes** | Can change freely | Versioned carefully |
| **Naming** | `*Event.ts` | No "Event" suffix |

## Links

- [Domain Events vs Integration Events](https://enterprisecraftsmanship.com/posts/domain-events-vs-integration-events/)
- [Event-Driven Architecture](https://martinfowler.com/articles/201701-event-driven.html)
- [Schema Evolution](https://docs.confluent.io/platform/current/schema-registry/avro.html)

