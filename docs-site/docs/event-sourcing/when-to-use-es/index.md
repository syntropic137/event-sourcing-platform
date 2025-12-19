# When to Use Event Sourcing

Event Sourcing is a powerful pattern, but it's not the right choice for every domain. This guide helps you decide whether Event Sourcing is appropriate for your use case.

## The Core Question

> **Should I use Event Sourcing for my domain?**

The answer depends on whether the benefits outweigh the costs for your specific situation.

## Quick Decision Tree

```
┌─────────────────────────────────────────────────────────────┐
│  Is history/audit a core product requirement?               │
└─────────────────────┬───────────────────────────────────────┘
                      │
        ┌─────────────┴─────────────┐
        │                           │
       YES                          NO
        │                           │
        ▼                           ▼
┌───────────────────┐    ┌─────────────────────────────────┐
│ Consider ES       │    │ Do you need replay for          │
│                   │    │ debugging or compliance?        │
└─────────┬─────────┘    └───────────────┬─────────────────┘
          │                              │
          ▼                   ┌──────────┴──────────┐
┌─────────────────────────┐   │                     │
│ Do you need multiple    │  YES                    NO
│ read models from the    │   │                     │
│ same events?            │   ▼                     ▼
└───────────┬─────────────┘   │           ┌─────────────────────┐
            │             Consider ES     │ CRUD + Domain Events │
  ┌─────────┴─────────┐       │           │ is probably simpler  │
  │                   │       │           └─────────────────────┘
 YES                  NO      │
  │                   │       │
  ▼                   ▼       │
┌─────────────┐  ┌────────────┴───────────────────────────────┐
│ ES is a     │  │ ES is viable, but simpler options exist.   │
│ strong fit  │  │ Weigh the benefits against complexity.     │
│     ✅      │  └────────────────────────────────────────────┘
└─────────────┘
```

## When Event Sourcing Shines

Event Sourcing is an excellent choice when:

### 1. History IS the Product
- **Audit trails** — Financial systems, healthcare, legal compliance
- **Activity feeds** — "What happened" is user-facing
- **Undo/redo** — Users need to reverse actions
- **Time travel** — "Show me the state as of last Tuesday"

### 2. Multiple Read Models from Same Data
- **Different views** — Dashboard, reports, search, analytics all from same events
- **Polyglot persistence** — SQL for queries, Elasticsearch for search, graph DB for relationships
- **Microservices** — Each service builds its own projection

### 3. Complex Business Rules
- **State machines** — Order → Paid → Shipped → Delivered
- **Aggregate invariants** — Rules that must always hold
- **Conflict resolution** — Merge concurrent changes via event replay

### 4. Debugging and Troubleshooting
- **Reproduce bugs** — Replay exact event sequence that caused the issue
- **Root cause analysis** — See exactly what led to current state
- **What-if analysis** — Apply different logic to historical events

## When Event Sourcing is Overkill

Consider simpler alternatives when:

### 1. Simple CRUD is Sufficient
- **No audit requirements** — Just storing and retrieving data
- **Single read model** — One way to view the data
- **No business rules** — Simple validation, no complex state transitions

### 2. History Doesn't Matter
- **Overwrite is fine** — You don't care about previous values
- **No compliance needs** — No regulatory requirement to track changes
- **Ephemeral data** — Caches, sessions, temporary state

### 3. The Domain is Already Simple
- **Anemic models** — No behavior, just data
- **No concurrency concerns** — Single user, single process
- **Stable schema** — Rarely changes

## The Honest Trade-offs

### Benefits of Event Sourcing
| Benefit | Description |
|---------|-------------|
| Complete audit trail | Every change is recorded forever |
| Temporal queries | "What was the state at time T?" |
| Event-driven architecture | Natural fit for reactive systems |
| Debugging superpowers | Replay events to reproduce issues |
| Schema flexibility | Add new projections without migrations |
| Decoupling | Consumers don't depend on producers |

### Costs of Event Sourcing
| Cost | Description |
|------|-------------|
| Learning curve | Team needs to understand new patterns |
| Storage growth | Events accumulate forever (snapshots help) |
| Eventual consistency | Projections lag behind writes |
| Tooling needs | Requires projection management, replay tools |
| Complexity | More moving parts than simple CRUD |
| Upcasting | Schema changes require event migration |

## This Platform's Philosophy

This platform is designed for teams who have **decided that Event Sourcing is right for their domain**.

> "If you've decided ES is right for your domain, this platform gives you a reliable, testable, agent-friendly substrate with strong architectural guardrails."

We don't try to support both ES and CRUD+Events patterns. We're opinionated about ES and aim to make it excellent.

**If you're not sure ES is right for you:**
- Start with [Benefits and Costs](./benefits-and-costs.md) for a deeper analysis
- Read [ES vs Alternatives](./es-vs-alternatives.md) for pattern comparison
- Consider prototyping with our examples before committing

## Next Steps

- [Benefits and Costs](./benefits-and-costs.md) — Detailed trade-off analysis
- [ES vs Alternatives](./es-vs-alternatives.md) — Compare with other patterns
- [Adoption Guidance](./adoption-guidance.md) — How to adopt ES incrementally
