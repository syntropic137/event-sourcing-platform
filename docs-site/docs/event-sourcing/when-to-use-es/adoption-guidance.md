# Adoption Guidance

How to adopt Event Sourcing incrementally and successfully.

## Before You Start

### 1. Validate the Decision

Before adopting ES, confirm:
- [ ] History/audit is a genuine requirement (not "nice to have")
- [ ] Team is willing to invest in learning ES patterns
- [ ] You have time for the initial learning curve
- [ ] The domain has meaningful business logic (not just data storage)

### 2. Start Small

Don't event-source your entire system at once. Pick **one bounded context** that:
- Has clear audit/history requirements
- Is relatively isolated (few dependencies)
- Has a motivated team member to champion it
- Is not on a critical deadline

### 3. Learn the Patterns First

Before writing code:
- Understand aggregates, commands, events, projections
- Review the [examples](../examples/) in this platform
- Do a "paper exercise" — model your domain with events on paper/whiteboard

---

## Adoption Strategies

### Strategy 1: Greenfield (New System)

**Best for:** New projects starting from scratch

**Approach:**
1. Design your domain with Event Modeling or Event Storming
2. Identify aggregates and their events
3. Start with the core aggregate, build outward
4. Use this platform from day one

**Tips:**
- Don't over-engineer — start with one projection per aggregate
- Get something working end-to-end quickly
- Add sophistication (multiple projections, sagas) as needed

---

### Strategy 2: New Bounded Context

**Best for:** Adding ES to an existing system in a new area

**Approach:**
1. Create a new bounded context (separate from existing CRUD code)
2. Integrate via events or API calls
3. Existing code doesn't need to change immediately

**Example:**
```
Existing System (CRUD)          New Context (ES)
┌─────────────────────┐        ┌─────────────────────┐
│  Users              │        │  Subscriptions      │
│  Products           │◄───────│  (Event Sourced)    │
│  (traditional CRUD) │ events │                     │
└─────────────────────┘        └─────────────────────┘
```

**Tips:**
- Keep the boundary clean — communicate via well-defined events/APIs
- Don't try to share aggregates across old and new code
- This is often the safest approach

---

### Strategy 3: Strangler Fig (Gradual Migration)

**Best for:** Migrating an existing CRUD domain to ES

**Approach:**
1. Add event publishing to existing CRUD code (outbox pattern)
2. Build new ES-based projections that consume those events
3. Gradually shift reads to new projections
4. Eventually, make events the source of truth

**Phases:**

```
Phase 1: Add Events (no behavior change)
┌─────────────────────┐
│  CRUD Code          │──────► Events (for consumers)
│  (still writes DB)  │
└─────────────────────┘

Phase 2: Build Projections
┌─────────────────────┐        ┌───────────────┐
│  CRUD Code          │──────►│  Projections   │
│  (still writes DB)  │ events│  (new views)   │
└─────────────────────┘        └───────────────┘

Phase 3: Flip Source of Truth
┌─────────────────────┐        ┌───────────────┐
│  Event Store        │◄─────│  Commands       │
│  (source of truth)  │       │  (new writes)   │
└─────────────────────┘        └───────────────┘
         │
         ▼
┌─────────────────────┐
│  Projections        │
│  (derived views)    │
└─────────────────────┘
```

**Tips:**
- This is the slowest but safest approach
- Each phase is independently valuable
- You can stop at any phase if needed

---

## Common Pitfalls

### Pitfall 1: Event-Sourcing Everything

**Problem:** Treating ES as a religion, applying it to every domain.

**Solution:** Use ES where it adds value. Simple CRUD is fine for simple domains.

---

### Pitfall 2: Fat Events

**Problem:** Events that contain too much data, duplicating aggregate state.

**Solution:** Events should capture what happened, not the entire state. Derive state by replaying.

```typescript
// ❌ Bad: Fat event
{ type: 'OrderUpdated', order: { ...entireOrderObject } }

// ✅ Good: Focused event
{ type: 'ItemAddedToOrder', productId: '123', quantity: 2 }
```

---

### Pitfall 3: Ignoring Projections Until Later

**Problem:** Building only aggregates, then struggling to query data.

**Solution:** Build at least one projection from the start. You'll need it for UX.

---

### Pitfall 4: No Versioning From Day One

**Problem:** Not versioning events, then struggling when schemas change.

**Solution:** Use the `@Event('EventType', 'v1')` decorator from the start.

```typescript
// Always version your events
@Event('OrderPlaced', 'v1')
export class OrderPlacedEvent extends BaseDomainEvent {
  // ...
}
```

---

### Pitfall 5: Under-Investing in Tooling

**Problem:** Running ES in production without proper monitoring, backfill, or error handling.

**Solution:** Use this platform's projection ops features (checkpoints, DLQ, health checks).

---

## Success Metrics

How to know your ES adoption is going well:

| Metric | Good Sign | Warning Sign |
|--------|-----------|--------------|
| Debugging time | Faster (replay events) | Slower (can't find issues) |
| New feature time | Getting faster | Getting slower |
| Projection lag | Consistently low | Growing or unpredictable |
| Team confidence | "I can see what happened" | "I don't know what's going on" |
| Schema changes | Handled via upcasters | Causing outages |

---

## Getting Help

If you're struggling with ES adoption:

1. **Review the examples** — They demonstrate real patterns
2. **Read the ADRs** — They explain why things are designed as they are
3. **Start simpler** — Remove complexity until it works, then add back
4. **Ask for help** — ES has a learning curve; it's okay to need guidance

---

## Summary

1. **Validate first** — Make sure ES is right for your domain
2. **Start small** — One bounded context, one aggregate
3. **Learn patterns** — Invest in understanding before coding
4. **Use tooling** — This platform provides the substrate you need
5. **Iterate** — Add sophistication as you learn

Event Sourcing pays dividends over time. The initial investment is worth it for the right domains.

---

**Back to:** [When to Use ES](./index.md)
