# Architecture Decision Records - Index

**Status:** 📘 Master Reference
**Last Updated:** 2026-02-02

This document provides a comprehensive overview of all architectural decisions for the **Hexagonal Event-Sourced Vertical Slice Architecture** pattern.

---

## 📚 Table of Contents

1. [Architecture Overview](#architecture-overview)
2. [ADR Summary](#adr-summary)
3. [Core Concepts](#core-concepts)
4. [Architectural Layers](#architectural-layers)
5. [Decision Tree](#decision-tree)
6. [Implementation Guide](#implementation-guide)
7. [Quick Reference](#quick-reference)

---

## Architecture Overview

The **Hexagonal Event-Sourced VSA** pattern combines three powerful architectural patterns:

```
┌─────────────────────────────────────────────────────────────────┐
│                    HEXAGONAL ARCHITECTURE                       │
│                   (Ports & Adapters Pattern)                    │
│                                                                  │
│  ┌────────────────────────────────────────────────────────┐   │
│  │              VERTICAL SLICE ARCHITECTURE                │   │
│  │            (Feature-Based Organization)                 │   │
│  │                                                          │   │
│  │    ┌──────────────────────────────────────────────┐   │   │
│  │    │          EVENT SOURCING                       │   │   │
│  │    │    (Event-Driven State Management)           │   │   │
│  │    │                                               │   │   │
│  │    │  • Aggregates as Consistency Boundaries     │   │   │
│  │    │  • Commands for Intent                       │   │   │
│  │    │  • Events as Facts                           │   │   │
│  │    │  • Event Versioning & Upcasters              │   │   │
│  │    │                                               │   │   │
│  │    └──────────────────────────────────────────────┘   │   │
│  │                                                          │   │
│  └────────────────────────────────────────────────────────┘   │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Key Principles

1. **Domain Isolation** - Pure business logic with no infrastructure dependencies
2. **Slice Isolation** - Features are completely independent vertical slices
3. **Inward Dependencies** - All dependencies point toward the domain core
4. **Event Sourcing** - All state changes captured as immutable events
5. **CQRS** - Strict separation between commands (writes) and queries (reads)

---

## ADR Summary

| ADR | Title | Status | Purpose |
|-----|-------|--------|---------|
| [ADR-004](./ADR-004-command-handlers-in-aggregates.md) | Command Handlers in Aggregates | ✅ Accepted | Defines aggregate structure with integrated command handlers |
| [ADR-005](./ADR-005-hexagonal-architecture-event-sourcing.md) | Hexagonal Architecture | ✅ Accepted | Establishes hexagonal layers and dependency rules |
| [ADR-006](./ADR-006-domain-organization-pattern.md) | Domain Organization | ✅ Accepted | Specifies domain/ folder structure |
| [ADR-007](./ADR-007-event-versioning-upcasters.md) | Event Versioning | ✅ Accepted | Defines event versioning strategy and upcasters |
| [ADR-008](./ADR-008-vertical-slices-hexagonal-adapters.md) | Vertical Slices | ✅ Accepted | Defines slices as thin hexagonal adapters |
| [ADR-009](./ADR-009-cqrs-pattern-implementation.md) | CQRS Pattern | ✅ Accepted | Formalizes CQRS implementation |
| [ADR-010](./ADR-010-decorator-patterns-framework.md) | Decorator Patterns | ✅ Accepted | Documents framework integration decorators |
| [ADR-011](./ADR-011-ts-proto-v1-retention.md) | ts-proto v1 Retention | ✅ Accepted | Decision to retain ts-proto v1.172.0 vs v2 upgrade |
| [ADR-014](./ADR-014-projection-checkpoint-architecture.md) | Projection Checkpoints | ✅ Accepted | Per-projection checkpoint tracking for reliable event processing |
| [ADR-015](./ADR-015-es-test-kit-architecture.md) | ES Test Kit Architecture | ✅ Accepted | Reusable testing harness for ES applications (incl. Given-When-Then scenario testing) |
| [ADR-016](./ADR-016-projection-failure-handling.md) | Projection Failure Handling | 📋 Proposed | DLQ, retry policies, and error handling for projections |
| [ADR-017](./ADR-017-observability-conventions.md) | Observability Conventions | ✅ Accepted | Tracing, metrics, and logging standards |
| [ADR-018](./ADR-018-renovate-bot-adoption.md) | Renovate Bot Adoption | 📋 Proposed | Migration from Dependabot to Renovate for dependency management |
| [ADR-019](./ADR-019-vsa-standard-structure.md) | **VSA Standard Structure** | ✅ Accepted | **CANONICAL reference for all VSA applications - comprehensive structure standard** |
|| [ADR-020](./ADR-020-bounded-context-aggregate-convention.md) | **Bounded Context & Aggregate Convention** | ✅ Accepted | **Defines aggregate_<name>/ folder convention, entity/VO rules, BC requirements** |
| [ADR-021](./ADR-021-expected-version-and-set-based-validation.md) | ExpectedVersion & Set-Based Validation | ✅ Accepted | NoStream semantics, stream-per-unique-value pattern, StreamAlreadyExistsError |
| [ADR-022](./ADR-022-strict-python-typing.md) | **Strict Python Typing Strategy** | ✅ Accepted | **Multi-layer type safety for Python SDK: pyright strict + ruff ANN401 + object ratchet** |
| [ADR-023](./ADR-023-event-type-registry.md) | **Event Type Registry** | ✅ Accepted | **Global registry for resolving concrete event types from wire format — auto-populated by @event decorator** |

---

## Core Concepts

### 1. Hexagonal Architecture (ADR-005)

**What:** Architectural pattern that isolates domain logic from infrastructure.

**Why:** Enables domain-driven design, testability, and technology-agnostic core.

**How:** Three layers with strict dependency rules:

```
┌──────────────────────────────────────┐
│     ADAPTERS (Hexagon Outside)       │
│  • REST Controllers                   │
│  • CLI Commands                       │
│  • gRPC Services                      │
│  • GraphQL Resolvers                  │
└─────────────┬────────────────────────┘
              │ ⬇ Depends on
┌─────────────┴────────────────────────┐
│   INFRASTRUCTURE (Application)       │
│  • CommandBus                         │
│  • QueryBus                           │
│  • EventBus                           │
│  • Repositories                       │
└─────────────┬────────────────────────┘
              │ ⬇ Depends on
┌─────────────┴────────────────────────┐
│     DOMAIN (Hexagon Core)            │
│  • Aggregates                         │
│  • Commands                           │
│  • Queries                            │
│  • Events                             │
│  • Business Logic                     │
└──────────────────────────────────────┘
```

**Key Rule:** Dependencies point INWARD only. Domain has ZERO outward dependencies.

---

### 2. Domain Organization (ADR-006)

**What:** Standard structure for domain/ folder.

**Why:** Consistent organization enables quick navigation and AI-assisted development.

**How:**

```
domain/
├── TaskAggregate.ts          ← Aggregates (with @CommandHandler methods)
├── CartAggregate.ts          ← Multiple aggregates OK
│
├── commands/                 ← Command definitions
│   ├── tasks/               ← Organized by feature
│   │   ├── CreateTaskCommand.ts
│   │   └── CompleteTaskCommand.ts
│   └── cart/
│       └── AddItemCommand.ts
│
├── queries/                  ← Query definitions
│   ├── GetTaskByIdQuery.ts
│   └── GetCartSummaryQuery.ts
│
└── events/                   ← Event definitions
    ├── TaskCreatedEvent.ts   ← Current version (with @Event decorator)
    ├── TaskCompletedEvent.ts
    │
    ├── _versioned/           ← Old event versions
    │   └── TaskCreatedEvent_v1.ts  (@Deprecated)
    │
    └── _upcasters/           ← Event migration logic
        └── TaskCreatedEvent_Upcaster_v1_v2.ts
```

---

### 3. Event Versioning (ADR-007)

**What:** Strategy for evolving event schemas over time.

**Why:** Events are immutable facts in event sourcing; schema changes require versioning.

**How:**

1. **Version Format:** String-based versions (`'v1'`, `'v2'`, `'v3'`) by default
2. **Decorator:** `@Event('TaskCreated', 'v2')` required on all events
3. **Old Versions:** Moved to `_versioned/` folder with `@Deprecated('v2')`
4. **Upcasters:** Migration logic in `_upcasters/` folder with `@Upcaster` decorator

**Example:**

```typescript
// events/TaskCreatedEvent.ts (CURRENT)
@Event('TaskCreated', 'v2')
export class TaskCreatedEvent {
  aggregateId: string;
  title: string;
  assignee: string;  // NEW in v2
}

// events/_versioned/TaskCreatedEvent_v1.ts (OLD)
@Event('TaskCreated', 'v1')
@Deprecated('v2')
export class TaskCreatedEvent_v1 {
  aggregateId: string;
  title: string;
}

// events/_upcasters/TaskCreatedEvent_Upcaster_v1_v2.ts
@Upcaster('TaskCreated', { from: 'v1', to: 'v2' })
export class TaskCreatedEventUpcaster {
  upcast(event: TaskCreatedEvent_v1): TaskCreatedEvent {
    return {
      ...event,
      assignee: 'unassigned',  // Default value for new field
    };
  }
}
```

---

### 4. Vertical Slices (ADR-008)

**What:** Features organized as isolated, independently deployable vertical slices.

**Why:** Enables parallel development, clear ownership, and minimal coupling.

**How:**

```
slices/
├── create-task/              ← Command slice (WRITE)
│   ├── CreateTaskController.ts
│   ├── CreateTaskController.test.ts
│   └── slice.yaml
│
├── get-task/                 ← Query slice (READ)
│   ├── GetTaskController.ts
│   ├── TaskProjection.ts
│   ├── TaskProjection.test.ts
│   └── slice.yaml
│
└── order-fulfillment-saga/   ← Saga slice (PROCESS)
    ├── OrderFulfillmentSaga.ts
    ├── OrderFulfillmentSaga.test.ts
    └── slice.yaml
```

**Slice Types:**

| Type | Purpose | Uses | Emits | Contains |
|------|---------|------|-------|----------|
| **Command** | Handles commands (write) | CommandBus | N/A | Controller, Request/Response DTOs |
| **Query** | Handles queries (read) | QueryBus | N/A | Controller, Projection, Query Handler |
| **Saga** | Orchestrates processes | EventBus | Commands | Event Handlers, State Machine |

**Key Rules:**
- ✅ Slices can import from `domain/` (read-only)
- ✅ Slices can import from `infrastructure/`
- ❌ Slices CANNOT import from other slices
- ❌ Slices CANNOT contain business logic
- ✅ Slices must be < 50 lines (thin adapters)

---

### 5. CQRS Pattern (ADR-009)

**What:** Strict separation between commands (writes) and queries (reads).

**Why:** Enables independent scaling, optimization, and evolution of read/write models.

**How:**

```
┌─────────────────────────────────────────────────────────────┐
│                      WRITE SIDE                             │
│                                                              │
│  REST API ──▶ CommandController ──▶ CommandBus ──▶ Aggregate│
│                                                       │      │
│                                                       ▼      │
│                                                    Events    │
│                                                       │      │
└───────────────────────────────────────────────────────┼──────┘
                                                        │
                                                        ▼
                                                   Event Bus
                                                        │
                                                        ▼
┌───────────────────────────────────────────────────────┼──────┐
│                      READ SIDE                        │      │
│                                                        │      │
│  Projection ◀────────────────────────────────────Events     │
│       │                                                      │
│       ▼                                                      │
│  Read Model (Optimized for queries)                         │
│       │                                                      │
│       ▼                                                      │
│  QueryHandler ◀── QueryBus ◀── QueryController ◀── REST API│
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

**Key Rules:**
- Commands use `CommandBus`
- Queries use `QueryBus` or query projections directly
- Commands return `void` (or command result metadata)
- Queries return data
- Write model (Aggregate) ≠ Read model (Projection)

---

### 6. Decorator Patterns (ADR-010)

**What:** Decorators for framework integration and metadata extraction.

**Why:** Enables auto-discovery, routing, and reduces boilerplate.

**How:**

#### Event Decorators

```typescript
@Event('TaskCreated', 'v2')
export class TaskCreatedEvent { }

@Deprecated('v3')
export class OldEvent { }

@Upcaster('TaskCreated', { from: 'v1', to: 'v2' })
export class TaskCreatedEventUpcaster { }
```

#### Controller Decorators

```typescript
// REST Controller
@RestController('/api/tasks')
export class CreateTaskController {
  @Post('/')
  @Route('/create')
  async handle(@Body() request: CreateTaskRequest) { }
}

// CLI Controller
@CliController()
export class CreateTaskCli {
  @Command('create-task')
  @Argument('title', { required: true })
  @Option('due-date', { short: 'd' })
  async handle() { }
}

// gRPC Controller
@GrpcController('TaskService')
export class TaskGrpcController {
  @GrpcMethod('CreateTask')
  async createTask() { }
}
```

---

## Architectural Layers

### Layer 1: Domain (Core)

**Location:** `domain/`
**Dependencies:** NONE (zero outward dependencies)
**Contains:**
- Aggregates (with `@CommandHandler` methods)
- Commands
- Queries
- Events (with versioning)
- Pure business logic

**Rules:**
- ✅ Can define interfaces (ports)
- ❌ Cannot import from infrastructure
- ❌ Cannot import from slices
- ❌ Cannot import external libraries (except utilities)

**References:** ADR-004, ADR-006

---

### Layer 2: Infrastructure (Application Services)

**Location:** `infrastructure/`
**Dependencies:** Domain
**Contains:**
- CommandBus
- QueryBus
- EventBus
- Repositories (implementations)
- External service clients

**Rules:**
- ✅ Can import from domain
- ❌ Cannot import from slices
- ✅ Can use external libraries

**References:** ADR-005

---

### Layer 3: Adapters (Slices)

**Location:** `slices/`
**Dependencies:** Domain (read-only), Infrastructure
**Contains:**
- Controllers (REST, CLI, gRPC, GraphQL)
- Request/Response DTOs
- Projections (for query slices)
- Query Handlers (for query slices)
- Saga Event Handlers (for saga slices)

**Rules:**
- ✅ Can import from domain (read-only)
- ✅ Can import from infrastructure
- ❌ Cannot import from other slices
- ❌ Cannot contain business logic
- ✅ Must be thin (< 50 lines)

**References:** ADR-008, ADR-009

---

## Decision Tree

Use this flowchart to determine where code belongs:

```
┌─────────────────────────────────────┐
│  Is this BUSINESS LOGIC?            │
│  (validation, calculations, rules)  │
└──────────┬──────────────────────────┘
           │
           ├─ YES ──▶ domain/
           │          └─ Aggregate methods
           │
           └─ NO
              │
              ┌─────────────────────────────────┐
              │  Does it TRANSLATE external     │
              │  protocols to domain concepts?  │
              └──────────┬──────────────────────┘
                         │
                         ├─ YES ──▶ slices/
                         │          └─ Controller
                         │
                         └─ NO
                            │
                            ┌─────────────────────────────┐
                            │  Does it COORDINATE or      │
                            │  ROUTE domain operations?   │
                            └──────────┬──────────────────┘
                                       │
                                       ├─ YES ──▶ infrastructure/
                                       │          └─ CommandBus, QueryBus
                                       │
                                       └─ NO ──▶ Review architecture!
```

---

## Implementation Guide

### Step 1: Define Domain

1. Create aggregates in `domain/`
2. Add commands in `domain/commands/{feature}/`
3. Add events in `domain/events/`
4. Implement `@CommandHandler` methods in aggregates
5. Version events with `@Event('EventType', 'v1')`

### Step 2: Setup Infrastructure

1. Create `CommandBus` in `infrastructure/`
2. Create `QueryBus` in `infrastructure/`
3. Create `EventBus` in `infrastructure/`
4. Wire up aggregate discovery and command routing

### Step 3: Create Slices

1. Create slice directory in `slices/{feature}/`
2. Add controller (REST/CLI/gRPC)
3. Add `slice.yaml` metadata
4. Inject `CommandBus` or `QueryBus`
5. Translate external request → command/query
6. Dispatch via bus

### Step 4: Add Projections (Query Slices)

1. Create projection in query slice
2. Subscribe to relevant events via `EventBus`
3. Build read model
4. Add query handler
5. Wire query handler to `QueryBus`

### Step 5: Validate Architecture

```bash
vsa validate --config vsa.yaml
```

---

## Quick Reference

### File Naming Conventions

| Type | Pattern | Example |
|------|---------|---------|
| Aggregate | `{Name}Aggregate.{ext}` | `TaskAggregate.ts` |
| Command | `{Name}Command.{ext}` | `CreateTaskCommand.ts` |
| Query | `{Name}Query.{ext}` | `GetTaskByIdQuery.ts` |
| Event | `{Name}Event.{ext}` | `TaskCreatedEvent.ts` |
| Controller | `{Name}Controller.{ext}` | `CreateTaskController.ts` |
| Projection | `{Name}Projection.{ext}` | `TaskProjection.ts` |
| Upcaster | `{Event}_Upcaster_{From}_{To}.{ext}` | `TaskCreatedEvent_Upcaster_v1_v2.ts` |

### Required Decorators

| Decorator | Target | Required | Purpose |
|-----------|--------|----------|---------|
| `@Event(type, version)` | Event class | ✅ Yes | Event versioning |
| `@Deprecated(version)` | Old event | ⚠️ Recommended | Mark old versions |
| `@Upcaster(event, versions)` | Upcaster class | ✅ Yes | Event migration |
| `@RestController(path)` | Controller class | ⚠️ Optional | REST routing |
| `@CliController()` | Controller class | ⚠️ Optional | CLI routing |
| `@CommandHandler` | Aggregate method | ✅ Yes | Command routing |

### Validation Rules

| Rule Code | Description | Severity |
|-----------|-------------|----------|
| `HEX001` | Domain has no outward imports | ❌ Error |
| `HEX002` | Slices are isolated (no cross-slice imports) | ❌ Error |
| `HEX003` | No business logic in slices | ⚠️ Warning |
| `DOM001` | Aggregates in domain/ folder | ❌ Error |
| `DOM002` | Commands in domain/commands/ | ❌ Error |
| `EVT001` | Events have @Event decorator | ❌ Error |
| `EVT002` | Events have version parameter | ❌ Error |
| `EVT003` | Upcaster exists for version change | ❌ Error |
| `CQRS001` | Command slices use CommandBus | ⚠️ Warning |
| `CQRS002` | Query slices use QueryBus | ⚠️ Warning |
| `SLICE001` | Slice is thin (< 50 lines) | ⚠️ Warning |

---

## See Also

- [Platform Philosophy](../PLATFORM-PHILOSOPHY.md) — What this platform IS and IS NOT
- [Maintainability Doctrine](../MAINTAINABILITY-DOCTRINE.md) — Engineering principles for sustainability
- [VSA Configuration Reference](../../vsa/examples/vsa.reference.yaml)
- [Slice Metadata Reference](../../vsa/examples/slice.reference.yaml)

---

## Changelog

| Date | Version | Changes |
|------|---------|---------|
| 2025-11-06 | 1.0.0 | Initial ADR index created |
| 2025-12-02 | 1.1.0 | Added ADR-011: ts-proto v1 retention decision |
| 2025-12-10 | 1.2.0 | Added ADR-014: Projection checkpoint architecture for reliable event processing |
| 2025-12-22 | 2.0.0 | **Added ADR-019: Comprehensive VSA Standard Structure (canonical reference)** |
| 2026-01-28 | 2.1.0 | **Updated ADR-015: Added Given-When-Then scenario testing (TypeScript + Python)** |
|| 2026-02-02 | 2.2.0 | **Added ADR-020: Bounded Context & Aggregate Convention (aggregate_<name>/ folders)** |
| 2026-04-07 | 2.3.0 | **Added ADR-023: Event Type Registry (auto-populated by @event, resolves concrete types from wire format)** |

---

**Next Steps:**
1. ✅ Review all ADRs
2. ✅ Create reference schemas (vsa.yaml, slice.yaml)
3. 🔄 Refactor vsa-core to implement validation
4. 🔄 Update examples to follow pattern
5. ⏳ Create tutorial documentation
