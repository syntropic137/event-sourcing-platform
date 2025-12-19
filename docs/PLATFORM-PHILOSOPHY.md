# Event Sourcing Platform Philosophy

This document captures the core philosophy and design principles of the Event Sourcing Platform.

---

## Mission Statement

> **If you've decided Event Sourcing is right for your domain, this platform gives you a reliable, testable, agent-friendly substrate with strong architectural guardrails.**

We don't try to be everything to everyone. We're opinionated about Event Sourcing and aim to make it excellent.

---

## Core Principles

### 1. ES-Focused, Not General Event-Driven

**What this means:**
- Events ARE the source of truth (not state tables)
- Replay is a first-class operation
- Aggregates derive state from events
- Projections are built from event streams

**What we don't support:**
- CRUD + Domain Events (outbox pattern)
- State-first persistence with event decoration
- General message bus/queue patterns

**Why:**
CRUD+Events and full Event Sourcing are fundamentally different storage paradigms. Trying to support both would muddy the purpose and add complexity for marginal benefit. We chose to be excellent at one thing rather than mediocre at two.

---

### 2. Simple + Reliable

**What this means:**
- Prefer boring, proven patterns over clever innovations
- Reliability comes from guarantees, not optionality
- Fewer moving parts = fewer failure modes
- If it's not tested, it doesn't work

**Core guarantees we provide:**
- Idempotency (producer via idempotency_key, consumer via checkpoints)
- Deterministic projections (same events → same state)
- Schema evolution (versioning + upcasters)
- Replay as first-class workflow
- Failure containment (DLQ + reprocess)

**What we avoid:**
- "Flexible" options that lead to misuse
- Complex configuration when convention suffices
- Features that can't be tested or monitored

---

### 3. Agent-Friendly / Parallel Development

**What this means:**
- Strong conventions so agents (human or AI) can "slot into" known patterns
- Vertical slices are self-contained and independently testable
- Cross-slice coordination happens through events, not shared state
- Agents don't need global understanding to implement a feature

**How we achieve this:**
- VSA tool enforces architectural boundaries
- Naming conventions reduce ambiguity
- Each slice has a predictable structure
- Integration events are the only cross-boundary contract

---

### 4. Strong Architectural Guardrails

**What this means:**
- The VSA tool validates structure at development time
- Conventions are enforced, not suggested
- Violations are caught before code review
- Architecture is documented in code (vsa.yaml, decorators)

**What we enforce:**
- Domain has zero outward dependencies
- Slices are isolated (no cross-slice imports)
- Business logic stays in domain (thin adapters)
- Events are versioned from day one
- Tests are required for features

---

### 5. Testability as a Feature

**What this means:**
- Every ES pattern should be testable in isolation
- Golden replays verify aggregate behavior
- Invariants are checked automatically
- Projections can be verified for determinism
- Tests use real infrastructure (no mocks by default)

**Test pyramid for ES:**
1. **Invariant tests** — Aggregate business rules hold after any event sequence
2. **Golden replay tests** — Known events produce expected state
3. **Projection tests** — Projections are deterministic and correct
4. **End-to-end tests** — Full workflows with real infrastructure

---

## What This Platform IS

| Characteristic | Description |
|----------------|-------------|
| Event Store | Append-only log with optimistic concurrency |
| ES SDK | Aggregates, commands, events, projections |
| Replay Harness | Rehydrate aggregates from events |
| Projection System | Build read models from event streams |
| Test Kit | Golden replays, invariants, projection verification |
| Ops Tools | Checkpoints, DLQ, backfill, health checks |
| Architecture Tool | VSA validation and enforcement |
| Documentation | Guides, examples, ADRs |

---

## What This Platform is NOT

| Not This | Why |
|----------|-----|
| General message bus | Use Kafka, RabbitMQ, or NATS for that |
| CRUD framework | Use any ORM for state-first persistence |
| Outbox pattern library | Different paradigm; plenty of options exist |
| Event-driven microservices kit | We're focused on ES within a bounded context |
| Low-code/no-code tool | We serve developers who write code |

---

## Design Decisions

### Why Rust for the Event Store?

- Performance and reliability for the storage layer
- Memory safety without garbage collection
- Single binary deployment
- gRPC for polyglot clients

### Why TypeScript as Primary SDK?

- Large ecosystem of developers
- Strong typing with decorators
- Good fit for web applications
- Examples are immediately runnable

### Why VSA (Vertical Slice Architecture)?

- Enables parallel development
- Clear ownership boundaries
- Reduces merge conflicts
- Natural fit for ES (slices = features = event streams)

### Why Convention Over Configuration?

- Reduces cognitive load
- Agents can follow patterns without extensive context
- Less configuration means fewer mistakes
- Validation is simpler with consistent structure

---

## The Three Layers

Inspired by our design discussions, we organize concerns into three layers:

### 1. Platform Kernel (95% Reusable)

The only part we try to make universal:
- Event envelope (id, time, actor, tenant, correlation, schema version)
- Append API + idempotency keys
- Subscription/consumer API
- Replay runner + snapshots
- Tracing/logging conventions
- Test kit

### 2. App Policy (Shared but Configurable)

Configurable per application:
- Authorization model
- Tenancy + data isolation rules
- PII handling + redaction
- Retention / compaction
- Consistency model for projections

### 3. Domain Slices (Not Shared)

Everything business-specific stays local:
- Command/event definitions
- Aggregate invariants
- Projections
- UI/API adapters

This boundary prevents "foundation" from becoming "overengineering."

---

## Trade-offs We Accept

| We Accept | To Get |
|-----------|--------|
| ES complexity | Audit, replay, projections |
| Learning curve | Long-term flexibility |
| Eventual consistency | CQRS separation |
| Upfront structure | Safe parallel development |
| Strict conventions | Agent-friendly patterns |

---

## When NOT to Use This Platform

Be honest about when this platform isn't the right fit:

- **Simple CRUD** — Use an ORM
- **History doesn't matter** — Use state-first persistence
- **Team won't invest in learning ES** — Patterns will be misused
- **Need event-driven but not ES** — Use outbox pattern + message broker
- **Extreme low-latency requirements** — Custom solutions may be needed

See [When to Use ES](/docs/event-sourcing/when-to-use-es/) for detailed guidance.

---

## Evolution Principles

As the platform evolves:

1. **Don't add features without use cases** — Solve real problems
2. **Don't break working patterns** — Stability over novelty
3. **Document decisions in ADRs** — Future us will thank present us
4. **Keep the kernel small** — Resist feature creep
5. **Test everything** — If it's not tested, it's not done

---

## Summary

This platform exists to make Event Sourcing **simple, reliable, and agent-friendly**.

We're not trying to support every event pattern. We're trying to be the best choice when you've decided ES is right for your domain.

If you share this philosophy, welcome. Let's build great things.

---

**Last Updated:** 2025-12-19
