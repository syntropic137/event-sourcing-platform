# Event Sourcing vs Alternatives

This page compares Event Sourcing with other persistence and event patterns. Understanding the alternatives helps you make an informed decision.

## Pattern Comparison

### Pattern 1: Traditional CRUD (State-Based)

**How it works:**
Store the current state in database tables. Updates overwrite previous values.

```
┌─────────────────────────────────────────┐
│  Database Table: orders                 │
├─────────────────────────────────────────┤
│  id: order-123                          │
│  status: shipped        ← overwrites    │
│  total: 150.00          ← overwrites    │
│  updated_at: 2024-02-01                 │
└─────────────────────────────────────────┘
```

**Pros:**
- Simple mental model
- Familiar to most developers
- Easy to query current state
- Lower operational complexity

**Cons:**
- No history (unless you build it separately)
- Hard to answer "how did we get here?"
- Schema changes require migrations
- Debugging requires logs + guesswork

**When to use:**
- Simple domains with no audit requirements
- Prototypes and MVPs
- When history genuinely doesn't matter

---

### Pattern 2: CRUD + Audit Log

**How it works:**
Store current state AND append changes to a separate audit log table.

```
┌─────────────────────────────────────────┐
│  orders table (source of truth)         │
├─────────────────────────────────────────┤
│  id: order-123, status: shipped         │
└─────────────────────────────────────────┘
                    +
┌─────────────────────────────────────────┐
│  audit_log table (history)              │
├─────────────────────────────────────────┤
│  { entity: order-123, action: created } │
│  { entity: order-123, action: paid }    │
│  { entity: order-123, action: shipped } │
└─────────────────────────────────────────┘
```

**Pros:**
- Get history with minimal changes to existing code
- Current state queries remain simple
- Audit log can be async (eventual)

**Cons:**
- Audit log can drift from reality (not transactional)
- No replay capability (audit log is "for humans")
- Two places to keep in sync
- Limited debugging value

**When to use:**
- Retrofitting audit to existing CRUD apps
- Simple compliance requirements
- When you don't need replay

---

### Pattern 3: CRUD + Domain Events (Outbox Pattern)

**How it works:**
Store current state AND publish domain events via an outbox table (transactionally).

```
┌─────────────────────────────────────────┐
│  orders table (source of truth)         │
├─────────────────────────────────────────┤
│  id: order-123, status: shipped         │
└─────────────────────────────────────────┘
                    +
┌─────────────────────────────────────────┐
│  outbox table (published events)        │
├─────────────────────────────────────────┤
│  { type: OrderCreated, ... }            │
│  { type: OrderPaid, ... }               │
│  { type: OrderShipped, ... }            │
└─────────────────────────────────────────┘
        │
        ▼ (relay to message broker)
┌─────────────────────────────────────────┐
│  Other services consume events          │
└─────────────────────────────────────────┘
```

**Pros:**
- Event-driven architecture without full ES
- State queries are simple (just read the table)
- Transactional event publishing (outbox pattern)
- Other services can react to events

**Cons:**
- State is source of truth, events are derivative
- No replay capability (can't rebuild state from events)
- Still need migrations for schema changes
- Events must match state (two things to keep in sync)

**When to use:**
- Event-driven microservices without ES complexity
- When you want event-driven communication but not ES
- Gradual migration toward ES (stepping stone)

---

### Pattern 4: Event Sourcing

**How it works:**
Events ARE the source of truth. State is derived by replaying events.

```
┌─────────────────────────────────────────┐
│  Event Store (source of truth)          │
├─────────────────────────────────────────┤
│  { type: OrderCreated, orderId: 123 }   │
│  { type: ItemAdded, productId: 456 }    │
│  { type: OrderPaid, amount: 150 }       │
│  { type: OrderShipped, carrier: UPS }   │
└─────────────────────────────────────────┘
        │
        ▼ (replay to build state)
┌─────────────────────────────────────────┐
│  Aggregate (derived state)              │
├─────────────────────────────────────────┤
│  status: shipped, total: 150            │
└─────────────────────────────────────────┘
        │
        ▼ (project to read models)
┌─────────────────────────────────────────┐
│  Projections (optimized views)          │
├─────────────────────────────────────────┤
│  OrderSummaryProjection                 │
│  CustomerOrderHistoryProjection         │
│  InventoryProjection                    │
└─────────────────────────────────────────┘
```

**Pros:**
- Complete history by design
- Replay for debugging, testing, migration
- Multiple projections from same events
- Time travel queries
- Natural CQRS separation

**Cons:**
- Learning curve
- Eventual consistency for projections
- Requires proper tooling (this platform provides it)
- Upcasting for schema changes

**When to use:**
- History is a core product requirement
- Multiple read models needed
- Complex business rules with state machines
- Debugging and compliance are critical

---

## Side-by-Side Comparison

| Aspect | CRUD | CRUD + Audit | CRUD + Outbox | Event Sourcing |
|--------|------|--------------|---------------|----------------|
| Source of truth | State table | State table | State table | **Events** |
| History | ❌ Lost | ⚠️ Separate | ⚠️ Separate | ✅ Built-in |
| Replay | ❌ No | ❌ No | ❌ No | ✅ Yes |
| Multiple projections | ❌ No | ❌ No | ⚠️ Limited | ✅ Yes |
| Time travel | ❌ No | ❌ No | ❌ No | ✅ Yes |
| Event-driven | ❌ No | ❌ No | ✅ Yes | ✅ Yes |
| Complexity | Low | Low | Medium | Higher |
| Schema changes | Migrations | Migrations | Migrations | Upcasters |
| Debugging | Logs | Audit log | Events (partial) | Full replay |

---

## This Platform's Scope

This platform is designed for **Event Sourcing** (Pattern 4).

We don't support:
- Pattern 1 (CRUD) — Use any ORM
- Pattern 2 (CRUD + Audit) — Use database triggers or audit libraries
- Pattern 3 (CRUD + Outbox) — Use Debezium, transactional outbox libraries

If you're unsure which pattern fits your needs:
- **Start with CRUD** if you're building a prototype
- **Add Outbox** when you need event-driven communication
- **Migrate to ES** when you need replay, projections, or history as a product

---

## Migration Path

If you're currently using CRUD and want to adopt ES:

### Step 1: Identify ES-Worthy Domains
Not everything needs ES. Pick domains where history matters.

### Step 2: Dual-Write During Transition
Write to both old state table and new event store. Verify consistency.

### Step 3: Flip the Source of Truth
Once confident, make events the source of truth. State becomes a projection.

### Step 4: Deprecate Old State Tables
Keep for rollback safety, then remove.

---

**Next:** [Adoption Guidance](./adoption-guidance.md) — How to adopt ES incrementally
