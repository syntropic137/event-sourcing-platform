# ADR-007: Event Versioning and Upcasters

**Status:** ✅ Accepted  
**Date:** 2025-11-06  
**Decision Makers:** Architecture Team  
**Related:** Event Sourcing, Schema Evolution, Domain Events

## Context

In event sourcing, events are **immutable facts** stored forever in the event store. However, business requirements evolve, and we need to change event schemas over time:

- Add new required fields
- Add optional fields
- Remove fields (rare, but possible)
- Rename properties
- Change data types
- Restructure nested objects

### The Problem

Once an event is written to the event store, it cannot be changed. If we read an old event with a new event class definition, deserialization fails.

**Example:**
```typescript
// Version 1 (old events in store)
class ItemAddedEvent {
  constructor(
    public productId: string,
    public quantity: number
  ) {}
}

// Version 2 (current code)
class ItemAddedEvent {
  constructor(
    public productId: string,
    public quantity: number,
    public deviceFingerprint: string  // NEW FIELD - old events don't have this!
  ) {}
}

// Reading old event fails:
// Error: Cannot read property 'deviceFingerprint' of undefined
```

### Requirements

1. **Backward Compatibility:** Read old events with new code
2. **Forward Compatibility:** (Optional) Read new events with old code
3. **Clear Versioning:** Explicit version tracking
4. **Automated Migration:** Upcasting happens transparently
5. **Type Safety:** Compiler catches version mismatches
6. **VSA Validation:** Ensure upcasters exist for all versions

## Decision

We adopt **explicit event versioning** with string version identifiers (`'v1'`, `'v2'`, `'v3'`) and **upcasters** for migrating between versions.

### Version Format

**Primary:** Simple version strings (`'v1'`, `'v2'`, `'v3'`, ...)
- ✅ Clear and readable
- ✅ Easy to understand
- ✅ Covers 95% of use cases

**Optional:** Semantic versioning (`'1.0.0'`, `'2.1.0'`, ...)
- ✅ For complex domains with multiple teams
- ✅ Distinguishes breaking vs non-breaking changes
- ✅ Opt-in when needed

### Event Decorator Pattern

Events MUST be decorated with `@Event(name, version)`:

```typescript
@Event('ItemAdded', 'v2')  // eventType, version (required)
export class ItemAddedEvent extends DomainEvent {
  constructor(
    public readonly aggregateId: string,
    public readonly productId: string,
    public readonly quantity: number,
    public readonly deviceFingerprint: string  // Added in v2
  ) {
    super();
  }
}
```

### File Organization

```
domain/events/
├── ItemAddedEvent.ts              ← Current version (v2)
├── CartCreatedEvent.ts            ← Current version (v1)
├── CartSubmittedEvent.ts          ← Current version (v3)
│
├── _versioned/                    ← Old versions (deprecated)
│   ├── ItemAddedEvent.v1.ts
│   ├── CartSubmittedEvent.v1.ts
│   └── CartSubmittedEvent.v2.ts
│
└── _upcasters/                    ← Version migration logic
    ├── ItemAddedEvent.v1-v2.ts
    ├── CartSubmittedEvent.v1-v2.ts
    └── CartSubmittedEvent.v2-v3.ts
```

### Upcaster Pattern

Upcasters migrate old events to new schema:

```typescript
// domain/events/_upcasters/ItemAddedEvent.v1-v2.ts
import { EventUpcaster, Upcaster } from '@event-sourcing-platform/typescript';
import { ItemAddedEventV1 } from '../_versioned/ItemAddedEvent.v1';
import { ItemAddedEvent } from '../ItemAddedEvent';

@Upcaster('ItemAdded', { from: 'v1', to: 'v2' })
export class ItemAddedEventUpcasterV1V2 
  implements EventUpcaster<ItemAddedEventV1, ItemAddedEvent> {
  
  upcast(oldEvent: ItemAddedEventV1): ItemAddedEvent {
    return new ItemAddedEvent(
      oldEvent.aggregateId,
      oldEvent.productId,
      oldEvent.quantity,
      'default-fingerprint'  // Default value for new field
    );
  }
}
```

## Pattern Details

### 1. Creating a New Event (v1)

```bash
# VSA CLI command
vsa event add ItemAdded --context cart

# Generates:
# domain/events/ItemAddedEvent.ts
```

```typescript
// domain/events/ItemAddedEvent.ts
import { DomainEvent, Event } from '@event-sourcing-platform/typescript';

@Event('ItemAdded', 'v1')  // Starts at v1
export class ItemAddedEvent extends DomainEvent {
  constructor(
    public readonly aggregateId: string,
    public readonly productId: string,
    public readonly quantity: number
  ) {
    super();
  }
}
```

### 2. Versioning an Event (v1 → v2)

```bash
# VSA CLI command
vsa event version ItemAdded --to v2

# Prompts: What changed?
# User: Added deviceFingerprint field

# Generates:
# 1. Updates domain/events/ItemAddedEvent.ts (v2)
# 2. Moves old to domain/events/_versioned/ItemAddedEvent.v1.ts
# 3. Scaffolds domain/events/_upcasters/ItemAddedEvent.v1-v2.ts
```

**Step 1:** Old version moves to `_versioned/`:
```typescript
// domain/events/_versioned/ItemAddedEvent.v1.ts
import { DomainEvent, Event } from '@event-sourcing-platform/typescript';

@Event('ItemAdded', 'v1')
@Deprecated('Use ItemAddedEvent v2 - adds deviceFingerprint field')
export class ItemAddedEventV1 extends DomainEvent {
  constructor(
    public readonly aggregateId: string,
    public readonly productId: string,
    public readonly quantity: number
  ) {
    super();
  }
}
```

**Step 2:** Current version updates:
```typescript
// domain/events/ItemAddedEvent.ts
import { DomainEvent, Event } from '@event-sourcing-platform/typescript';

@Event('ItemAdded', 'v2')  // Version bumped
export class ItemAddedEvent extends DomainEvent {
  constructor(
    public readonly aggregateId: string,
    public readonly productId: string,
    public readonly quantity: number,
    public readonly deviceFingerprint: string  // NEW FIELD
  ) {
    super();
  }
}
```

**Step 3:** Upcaster is scaffolded:
```typescript
// domain/events/_upcasters/ItemAddedEvent.v1-v2.ts
import { EventUpcaster, Upcaster } from '@event-sourcing-platform/typescript';
import { ItemAddedEventV1 } from '../_versioned/ItemAddedEvent.v1';
import { ItemAddedEvent } from '../ItemAddedEvent';

@Upcaster('ItemAdded', { from: 'v1', to: 'v2' })
export class ItemAddedEventUpcasterV1V2 
  implements EventUpcaster<ItemAddedEventV1, ItemAddedEvent> {
  
  upcast(oldEvent: ItemAddedEventV1): ItemAddedEvent {
    // TODO: Implement migration logic
    return new ItemAddedEvent(
      oldEvent.aggregateId,
      oldEvent.productId,
      oldEvent.quantity,
      'TODO: provide default value'  // Developer fills this in
    );
  }
}
```

**Step 4:** Developer completes upcaster:
```typescript
@Upcaster('ItemAdded', { from: 'v1', to: 'v2' })
export class ItemAddedEventUpcasterV1V2 
  implements EventUpcaster<ItemAddedEventV1, ItemAddedEvent> {
  
  upcast(oldEvent: ItemAddedEventV1): ItemAddedEvent {
    return new ItemAddedEvent(
      oldEvent.aggregateId,
      oldEvent.productId,
      oldEvent.quantity,
      'default-fingerprint'  // Sensible default for old events
    );
  }
}
```

### 3. Framework Integration

When reading events from the store, the framework automatically applies upcasters:

```typescript
// Event store read operation
const events = await eventStore.readStream('cart-123');

// Framework detects versions and upcasts:
// 1. Read event envelope: { type: 'ItemAdded', version: 'v1', data: {...} }
// 2. Find upcaster: ItemAddedEventUpcasterV1V2
// 3. Apply upcaster: v1 → v2
// 4. Deserialize to current class: ItemAddedEvent (v2)
// 5. Return typed event to application

// Application code always works with current version
events.forEach((event: ItemAddedEvent) => {
  console.log(event.deviceFingerprint);  // Works even for old events!
});
```

### 4. Multiple Version Jumps

Upcasters can be chained:

```
v1 ──→ v2 ──→ v3
```

```typescript
// domain/events/_upcasters/CartSubmittedEvent.v1-v2.ts
@Upcaster('CartSubmitted', { from: 'v1', to: 'v2' })
export class CartSubmittedEventUpcasterV1V2 { ... }

// domain/events/_upcasters/CartSubmittedEvent.v2-v3.ts
@Upcaster('CartSubmitted', { from: 'v2', to: 'v3' })
export class CartSubmittedEventUpcasterV2V3 { ... }

// Framework automatically chains:
// Old v1 event → v2 → v3 (current)
```

## Upcaster Guidelines

### ✅ Good Practices

**1. Pure Functions**
```typescript
upcast(old: EventV1): EventV2 {
  // ✅ Pure transformation
  return new EventV2(
    old.field1,
    old.field2,
    'default-value'
  );
}
```

**2. Deterministic**
```typescript
upcast(old: EventV1): EventV2 {
  // ✅ Same input always produces same output
  const derivedValue = old.price * 1.1;  // OK: based on input
  return new EventV2(old.id, old.price, derivedValue);
}
```

**3. Default Values**
```typescript
upcast(old: EventV1): EventV2 {
  // ✅ Sensible defaults for new fields
  return new EventV2(
    old.id,
    old.amount,
    'USD',  // Default currency for old events
    false   // Default: not taxable
  );
}
```

**4. Data Transformation**
```typescript
upcast(old: EventV1): EventV2 {
  // ✅ Transform data structure
  return new EventV2(
    old.id,
    {  // Nested object in v2
      street: old.street,
      city: old.city,
      zip: old.zip
    }
  );
}
```

### ❌ Anti-Patterns

**1. Side Effects**
```typescript
upcast(old: EventV1): EventV2 {
  // ❌ NO side effects
  await database.save(old);  // WRONG
  console.log(old);          // WRONG (side effect)
  return new EventV2(...);
}
```

**2. Non-Deterministic**
```typescript
upcast(old: EventV1): EventV2 {
  // ❌ NO random values
  return new EventV2(
    old.id,
    Math.random()  // WRONG: different each time
  );
}
```

**3. External Dependencies**
```typescript
upcast(old: EventV1): EventV2 {
  // ❌ NO external lookups
  const price = await pricingService.getPrice(old.productId);  // WRONG
  return new EventV2(old.id, price);
}
```

**4. Time-Based Values**
```typescript
upcast(old: EventV1): EventV2 {
  // ❌ NO current time
  return new EventV2(
    old.id,
    new Date()  // WRONG: changes over time
  );
  
  // ✅ OK: Use event timestamp if available
  return new EventV2(
    old.id,
    old.timestamp  // From original event
  );
}
```

## Semantic Versioning (Optional)

For complex domains with multiple teams:

```typescript
@Event('OrderPlaced', '2.1.0')  // Major.Minor.Patch
export class OrderPlacedEvent extends DomainEvent {
  constructor(
    public readonly aggregateId: string,
    public readonly customerId: string,
    public readonly items: OrderItem[],
    public readonly totalAmount: number,
    public readonly currency: string,        // Added in v2.0.0
    public readonly taxAmount?: number       // Added in v2.1.0 (optional)
  ) {
    super();
  }
}
```

**Version History:**
- `1.0.0` - Initial version
- `2.0.0` - BREAKING: Added required `currency` field
- `2.1.0` - NON-BREAKING: Added optional `taxAmount` field

**Upcasters:**
```typescript
// Breaking change: require upcaster
@Upcaster('OrderPlaced', { from: '1.0.0', to: '2.0.0' })
export class OrderPlacedUpcasterV1ToV2 { ... }

// Non-breaking: optional field, no upcaster needed
// v2.0.0 → v2.1.0 (framework handles undefined → undefined)
```

## VSA Framework Support

### Configuration

```yaml
# vsa.yaml
domain:
  events:
    path: domain/events/
    
    versioning:
      enabled: true
      format: simple              # 'simple' ('v1', 'v2') or 'semver' ('1.0.0')
      require_upcasters: true     # Enforce upcasters for all version changes
      versioned_path: _versioned/
      upcasters_path: _upcasters/
      
      # Semantic versioning options (if format: semver)
      breaking_changes_only: true  # Only require upcasters for major versions
```

### CLI Commands

```bash
# Create new event (starts at v1)
vsa event add ItemAdded --context cart

# Version an event
vsa event version ItemAdded --to v2
# Prompts: What changed?
# Moves v1 to _versioned/, updates to v2, scaffolds upcaster

# List event versions
vsa event versions ItemAdded
# Output:
# ItemAdded
#   v2 (current) - domain/events/ItemAddedEvent.ts
#   v1 (deprecated) - domain/events/_versioned/ItemAddedEvent.v1.ts
#   Upcasters:
#     v1 → v2 - domain/events/_upcasters/ItemAddedEvent.v1-v2.ts

# Check for missing upcasters
vsa event check
# Output:
# ✓ ItemAddedEvent: v1 → v2 (upcaster found)
# ✗ CartCreatedEvent: v2 → v3 (missing upcaster)
# ✗ OrderPlacedEvent: v1 → v2 (missing upcaster)
```

### Validation

```bash
vsa validate

# Event versioning checks:
# ✓ All events have @Event decorator
# ✓ All events have version parameter
# ✓ Version format consistent ('v1' format)
# ✓ Old versions in _versioned/ folder
# ✓ Upcasters exist for all version changes
# ✗ ERROR: ItemAddedEvent v2 found but no v1 in _versioned/
# ✗ ERROR: CartSubmittedEvent v2→v3 missing upcaster
```

## Language-Specific Implementations

### TypeScript

```typescript
// domain/events/ItemAddedEvent.ts
import { DomainEvent, Event } from '@event-sourcing-platform/typescript';

@Event('ItemAdded', 'v2')
export class ItemAddedEvent extends DomainEvent {
  constructor(
    public readonly aggregateId: string,
    public readonly productId: string,
    public readonly quantity: number,
    public readonly deviceFingerprint: string
  ) {
    super();
  }
}

// domain/events/_upcasters/ItemAddedEvent.v1-v2.ts
import { EventUpcaster, Upcaster } from '@event-sourcing-platform/typescript';

@Upcaster('ItemAdded', { from: 'v1', to: 'v2' })
export class ItemAddedEventUpcasterV1V2 
  implements EventUpcaster<ItemAddedEventV1, ItemAddedEvent> {
  
  upcast(old: ItemAddedEventV1): ItemAddedEvent {
    return new ItemAddedEvent(
      old.aggregateId,
      old.productId,
      old.quantity,
      'default-fingerprint'
    );
  }
}
```

### Python

```python
# domain/events/item_added_event.py
from event_sourcing.decorators import event
from event_sourcing.core.event import DomainEvent
from dataclasses import dataclass

@event('ItemAdded', 'v2')
@dataclass(frozen=True)
class ItemAddedEvent(DomainEvent):
    aggregate_id: str
    product_id: str
    quantity: int
    device_fingerprint: str  # Added in v2

# domain/events/_upcasters/item_added_event_v1_v2.py
from event_sourcing.decorators import upcaster
from event_sourcing.core.upcaster import EventUpcaster

@upcaster('ItemAdded', from_version='v1', to_version='v2')
class ItemAddedEventUpcasterV1V2(EventUpcaster):
    def upcast(self, old_event: ItemAddedEventV1) -> ItemAddedEvent:
        return ItemAddedEvent(
            aggregate_id=old_event.aggregate_id,
            product_id=old_event.product_id,
            quantity=old_event.quantity,
            device_fingerprint='default-fingerprint'
        )
```

### Rust

```rust
// domain/events.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[event("ItemAdded", "v2")]
pub struct ItemAddedEvent {
    pub aggregate_id: String,
    pub product_id: String,
    pub quantity: u32,
    pub device_fingerprint: String,  // Added in v2
}

// domain/events/upcasters.rs
use event_sourcing::EventUpcaster;

pub struct ItemAddedEventUpcasterV1V2;

impl EventUpcaster for ItemAddedEventUpcasterV1V2 {
    type From = ItemAddedEventV1;
    type To = ItemAddedEvent;
    
    fn event_type(&self) -> &'static str {
        "ItemAdded"
    }
    
    fn from_version(&self) -> &'static str {
        "v1"
    }
    
    fn to_version(&self) -> &'static str {
        "v2"
    }
    
    fn upcast(&self, old: Self::From) -> Result<Self::To> {
        Ok(ItemAddedEvent {
            aggregate_id: old.aggregate_id,
            product_id: old.product_id,
            quantity: old.quantity,
            device_fingerprint: "default-fingerprint".to_string(),
        })
    }
}
```

## Consequences

### Positive

1. **Backward Compatibility** ✅
   - Old events readable with new code
   - Event store history preserved
   - No data migration required

2. **Explicit Versioning** ✅
   - Version in decorator (type-safe)
   - Clear version history
   - Easy to track changes

3. **Automated Migration** ✅
   - Upcasters applied transparently
   - Application always works with current version
   - No manual conversion needed

4. **Type Safety** ✅
   - Compiler catches version mismatches
   - Upcaster signatures enforced
   - Refactoring safe

5. **VSA Validation** ✅
   - Ensures upcasters exist
   - Validates version consistency
   - Catches missing migrations

### Negative

1. **Upcaster Maintenance** ⚠️
   - Must maintain upcasters forever
   - Cannot delete old event definitions
   - **Mitigation:** Keep in _versioned/ folder, clearly marked

2. **Performance Overhead** ⚠️
   - Upcasting adds deserialization cost
   - Chained upcasters (v1→v2→v3) cumulative
   - **Mitigation:** Upcasters are fast (pure functions), cache if needed

3. **Complexity** ⚠️
   - More files (current + versioned + upcasters)
   - Need to understand upcasting pattern
   - **Mitigation:** VSA CLI automates, clear documentation

### Neutral

1. **Version Format**
   - Simple ('v1', 'v2') covers most cases
   - Semantic versioning available if needed
   - Choose based on team needs

2. **Upcaster Testing**
   - Must test upcasters thoroughly
   - Same as testing any migration logic

## Testing Upcasters

```typescript
describe('ItemAddedEventUpcasterV1V2', () => {
  const upcaster = new ItemAddedEventUpcasterV1V2();
  
  it('should add default device fingerprint', () => {
    const v1Event = new ItemAddedEventV1(
      'cart-123',
      'product-456',
      2
    );
    
    const v2Event = upcaster.upcast(v1Event);
    
    expect(v2Event.aggregateId).toBe('cart-123');
    expect(v2Event.productId).toBe('product-456');
    expect(v2Event.quantity).toBe(2);
    expect(v2Event.deviceFingerprint).toBe('default-fingerprint');
  });
  
  it('should be deterministic', () => {
    const v1Event = new ItemAddedEventV1('cart-1', 'prod-1', 1);
    
    const result1 = upcaster.upcast(v1Event);
    const result2 = upcaster.upcast(v1Event);
    
    expect(result1).toEqual(result2);
  });
});
```

## Related ADRs

- ADR-005: Hexagonal Architecture for Event-Sourced Systems (event sourcing context)
- ADR-006: Domain Organization Pattern (event file organization)
- ADR-010: Decorator Patterns for Framework Integration (@Event decorator)

## References

- "Versioning in an Event Sourced System" - Greg Young
- "Understanding Event Sourcing" - Alexey Zimarev (Chapter on versioning)
- "Event Sourcing Patterns: Upcasting" - Martin Fowler
- Axon Framework - Event Upcasting documentation

---

**Last Updated:** 2025-11-06  
**Supersedes:** None  
**Superseded By:** None

