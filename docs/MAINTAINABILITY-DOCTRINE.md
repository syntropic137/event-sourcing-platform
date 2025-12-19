# Maintainability Doctrine

**Version:** 1.0.0  
**Last Updated:** 2025-12-19

This document captures the engineering philosophy that prevents systems from becoming unmaintainable. It supplements the [Platform Philosophy](./PLATFORM-PHILOSOPHY.md) with practical principles for long-term sustainability.

---

## The Goal

> **Never rebuild because of accidental complexity. Only evolve because the domain truly changed.**

Systems become unmaintainable through two failure modes:

| Type | Description | Example |
|------|-------------|---------|
| **Accidental Complexity** | Too many concepts, layers, inconsistent patterns, hidden coupling | Rewrites forced by technical debt |
| **Essential Complexity** | Domain is genuinely hard; requirements change; scale constraints emerge | Natural evolution as business grows |

Overengineering often tries to prevent essential complexity but creates accidental complexity. This doctrine helps us avoid that trap.

---

## The Five Pillars

### 1. Ruthless Constraints + Clear Boundaries

**Principle:** Every module should be simple enough to hold in your head.

**Rules:**
- **1-3 core concepts per module** — If you need more, split the module
- **Explicit ownership** — Each module owns its data, invariants, and behavior
- **No shared mutable state** — Cross-module communication via immutable events only
- **Dependencies point inward** — Domain has zero outward dependencies

**How This Platform Enforces It:**
- VSA tool validates slice isolation (no cross-slice imports)
- Hexagonal architecture enforces layer boundaries (ADR-005)
- Aggregates are explicit consistency boundaries (ADR-004)
- Slices must be thin adapters (< 50 lines recommended)

**Red Flags:**
- ❌ Module needs a 10-minute explanation
- ❌ Changes require updates in 5+ files
- ❌ Circular dependencies
- ❌ "Utility" modules that everyone imports

---

### 2. Limit Concept Surface Area

**Principle:** A system becomes unmaintainable when you can't hold it in your head.

**Rules:**
- **Few message types** — Commands, Events, Queries. That's it.
- **Few lifecycle states** — Events are immutable. Aggregates derive state.
- **Few ways to do the same thing** — One pattern per use case
- **Concept budgets** — Each module should export a small, focused API

**Concept Budget Guidelines:**

| Module Type | Max Public Exports | Max Internal Concepts |
|-------------|-------------------|----------------------|
| Core abstraction | 5-10 | 15-20 |
| Feature module | 3-7 | 10-15 |
| Utility module | 3-5 | 5-10 |

**How This Platform Enforces It:**
- Three message primitives: Command, Event, Query
- Convention over configuration (ADR-002)
- Strong naming conventions (see ADR-INDEX)
- Module entry points explicitly curate exports

**Red Flags:**
- ❌ Index file exports 50+ symbols
- ❌ Multiple ways to achieve the same result
- ❌ "Config object" with 20+ options
- ❌ Base classes with 10+ methods to override

---

### 3. Evolution Strategy Baked In

**Principle:** Plan for change, but don't over-engineer for hypotheticals.

**Required Escape Hatches:**
- **Schema/versioning rules** — Events are versioned from day one (ADR-007)
- **Migration tooling** — Upcasters transform old events to new schema
- **Backfill/replay tooling** — Rebuild projections from event history
- **Staged rollouts** — Blue-green projections for zero-downtime changes

**How This Platform Enforces It:**
- `@Event` decorator requires version (ADR-007)
- Upcaster pattern for schema evolution
- Projection version tracking enables automatic rebuilds
- Checkpoint stores allow reprocessing from any position

**Red Flags:**
- ❌ Breaking changes without migration path
- ❌ "We'll figure out versioning later"
- ❌ Implicit schema assumptions
- ❌ No way to replay/rebuild state

---

### 4. High-Signal Tests (Not Lots of Tests)

**Principle:** Confidence to change comes from the right tests, not more tests.

**Test Pyramid for ES:**

| Level | Purpose | Coverage |
|-------|---------|----------|
| **Invariant tests** | Business rules hold after any event sequence | Core domain |
| **Golden replay tests** | Known events produce expected state | Critical paths |
| **Projection tests** | Determinism + correctness | Read models |
| **Contract tests** | API boundaries don't break | Integration points |
| **E2E tests** | Full workflows with real infra | Happy paths only |

**How This Platform Enforces It:**
- ES Test Kit: `ReplayTester`, `InvariantChecker`, `ProjectionTester`
- `@Invariant` decorator for business rule verification
- `verifyDeterminism()` and `verifyRebuild()` for projections
- Testing philosophy: "No mocks" — real infrastructure (CLAUDE.md)

**Red Flags:**
- ❌ High test count but low confidence to refactor
- ❌ Tests that break with implementation changes
- ❌ Mocking everything (integration bugs slip through)
- ❌ No property tests for edge cases

---

### 5. Operability > Elegance

**Principle:** Maintainability dies when debugging is hell.

**Required Capabilities:**

| Capability | Purpose | Platform Support |
|------------|---------|------------------|
| Structured logs | Searchable, parseable | ADR-017 (conventions) |
| Correlation IDs | Trace request flows | Event envelope metadata |
| Metrics | Understand system health | ADR-017 (conventions) |
| "Why did this happen?" traces | Debug production issues | ADR-017 (conventions) |
| Dead-letter queues | Handle permanent failures | `FailedEventStore` |
| Retry policies | Handle transient failures | `RetryPolicy` |
| Health checks | Know when things are broken | `ProjectionHealthChecker` |

**How This Platform Enforces It:**
- Event envelope includes `correlationId`, `causationId`, `actorId`
- Projection ops: DLQ, retries, health checks (ADR-016)
- Observability conventions documented (ADR-017)

**Red Flags:**
- ❌ "It works on my machine"
- ❌ Silent failures with no trace
- ❌ No way to reprocess failed events
- ❌ Metrics added after problems, not before

---

## The Abstraction Rule

> **No feature is allowed to introduce a new abstraction unless it deletes one, or proves it removes a class of bugs.**

This forces complexity paydown as the system grows.

**Questions to ask before adding an abstraction:**

1. What existing abstraction does this replace or simplify?
2. What class of bugs does this prevent?
3. Can this be done with existing patterns instead?
4. Will this still make sense in 2 years?

**Examples:**

| ✅ Good | ❌ Bad |
|---------|--------|
| `CheckpointedProjection` base class removes boilerplate checkpoint logic | Generic "middleware" system for hypothetical future needs |
| `@Invariant` decorator standardizes what was ad-hoc checks | AbstractFactoryProviderBuilder pattern |
| `ProjectionResult` enum replaces implicit success/throw patterns | Configuration object with 30 options "for flexibility" |

---

## Allowed Patterns

These patterns are approved for use in this platform:

| Pattern | When to Use | Reference |
|---------|-------------|-----------|
| Event Sourcing | Events are source of truth | Core platform |
| CQRS | Separate read/write models | ADR-009 |
| Vertical Slices | Feature isolation | ADR-008 |
| Hexagonal Architecture | Infrastructure isolation | ADR-005 |
| Decorator Pattern | Metadata/cross-cutting concerns | ADR-010 |
| Repository Pattern | Aggregate persistence | ADR-004 |
| Upcaster Pattern | Event schema evolution | ADR-007 |

**Patterns to avoid:**

| Pattern | Why |
|---------|-----|
| Generic middleware chains | Hidden control flow, hard to debug |
| Multi-level inheritance | Fragile, hard to understand |
| Annotation-driven magic | Implicit behavior is confusing |
| Plugin architectures | Usually premature; add when proven needed |
| Abstract factories | Rarely needed; adds indirection |

---

## Periodic Review Checklist

Run this checklist quarterly or before major releases:

### Concept Surface Area Audit

```
□ Count public exports per module (target: < 15 per module)
□ Identify "utility" modules that are imported everywhere
□ Check for multiple ways to do the same thing
□ Review "god objects" or "god modules"
```

### Boundary Health Check

```
□ Run VSA validation: `vsa validate`
□ Check for cross-slice dependencies
□ Verify domain has no outward imports
□ Review adapter thickness (< 50 lines)
```

### Evolution Readiness

```
□ All events have version decorators
□ Upcasters exist for deprecated versions
□ Projections have version tracking
□ Replay/rebuild has been tested recently
```

### Test Confidence

```
□ Invariant tests cover core business rules
□ Golden replays cover critical paths
□ Projection determinism verified
□ Can refactor with confidence
```

### Operability

```
□ Correlation IDs flow through the system
□ DLQ is monitored
□ Health checks return meaningful status
□ Recent production issues were debuggable
```

---

## Applying This to Decisions

When making architecture decisions, use this framework:

```
┌─────────────────────────────────────────────────────────────────┐
│ Does this add a new abstraction?                                │
│                                                                 │
│   YES ──▶ Does it DELETE an existing one OR                    │
│           prevent a CLASS of bugs?                              │
│                                                                 │
│           YES ──▶ Proceed, document in ADR                      │
│           NO  ──▶ STOP. Find simpler solution.                  │
│                                                                 │
│   NO ──▶ Does it increase concept surface area?                 │
│                                                                 │
│          YES ──▶ Can it be hidden internally?                   │
│                                                                 │
│                  YES ──▶ Proceed, keep internal                 │
│                  NO  ──▶ Reconsider. Is there a simpler way?    │
│                                                                 │
│          NO ──▶ Proceed                                         │
└─────────────────────────────────────────────────────────────────┘
```

---

## Summary

**The mindset that prevents rewrites:**

> "I want systems that can be refactored continuously without fear."

Sometimes rebuilding is rational (new domain, wrong product direction). But rebuilding because the codebase is a brittle hairball is avoidable.

**This doctrine ensures:**

1. ✅ Boundaries prevent hidden coupling
2. ✅ Small concepts keep systems understandable
3. ✅ Evolution paths prevent breaking changes
4. ✅ High-signal tests enable confident refactoring
5. ✅ Operability enables debugging without suffering

**The key insight:**

> The right amount of complexity is the minimum needed for the current task.

---

## Related Documents

- [Platform Philosophy](./PLATFORM-PHILOSOPHY.md) — What this platform IS and IS NOT
- [ADR Index](./adrs/ADR-INDEX.md) — Architectural decisions and their rationale
- [Testing Documentation](../docs-site/docs/event-sourcing/testing/) — ES Test Kit usage
- [Projection Ops](../docs-site/docs/event-sourcing/projections/) — Operational infrastructure

---

**Changelog:**

| Date | Version | Changes |
|------|---------|---------|
| 2025-12-19 | 1.0.0 | Initial doctrine created |
