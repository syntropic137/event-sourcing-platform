# ADR-019: VSA Standard Structure for DDD/ES/Hexagonal Applications

**Status:** ✅ Accepted  
**Date:** 2025-01-22  
**Decision Makers:** Architecture Team  
**Supersedes:** Extends ADR-005, ADR-006, ADR-008  
**Related:** Domain-Driven Design, Event Sourcing, Hexagonal Architecture, CQRS, Vertical Slice Architecture

## Context

As the Event Sourcing Platform (ESP) and its VSA tooling mature, we need a **comprehensive, canonical standard** for structuring DDD/ES/Hexagonal applications. This ADR consolidates architectural decisions from multiple ADRs and provides the definitive reference for:

1. **Developers** building event-sourced applications
2. **AI agents** working with ESP codebases
3. **VSA tooling** (validator, visualizer, generators)
4. **Other projects** adopting ESP patterns

### The Need for Standardization

Previous ADRs established patterns, but gaps remained:

- **ADR-005** defined hexagonal principles but not concrete structure
- **ADR-006** documented domain organization but not full context structure
- **ADR-008** defined vertical slices but not their relationship to ports/infrastructure

### Key Questions This ADR Answers

1. **Where do events live?** Context root or domain folder?
2. **Do we need a `ports/` folder?** Reference implementation doesn't have one
3. **Where do buses go?** Application layer or infrastructure?
4. **What goes in `application/` vs `ports/` vs `infrastructure/`?**
5. **How do we organize slices internally?**
6. **How strict are naming conventions?**

### Authoritative Reference

This ADR is heavily influenced by **"Understanding Event Sourcing"** by Alexey Zimarev (eventsourcing-book), the canonical DDD/ES reference implementation. We document where we align with this reference and where we deliberately deviate.

## Decision

We adopt a **standardized folder structure** that balances:

- ✅ **Domain cohesion** (events in `domain/events/` - they ARE domain language)
- ✅ **Alignment** with TypeScript reference patterns (domain/commands, domain/events, slices with internal/)
- ✅ **Explicit hexagonal boundaries** (ports/ folder for discoverability)
- ✅ **Infrastructure clarity** (buses in infrastructure/, not application/)
- ✅ **Framework-agnostic** approach (no assumptions about Axon/Django/etc)

## Canonical Structure

```
contexts/{context-name}/
├── domain/                          ← DOMAIN LAYER (Layer 1)
│   ├── {Aggregate}Aggregate.*       ← Aggregates at root
│   ├── {Name}ValueObjects.*         ← Value objects (optional)
│   ├── commands/                    ← Commands (input to domain)
│   │   ├── {feature}/               ← Subfolder per feature (large domains)
│   │   │   └── {Feature}Command.*
│   │   └── {Feature}Command.*       ← Or flat (small domains)
│   ├── events/                      ← Events (output from domain)
│   │   ├── {Event}Event.*           ← Current versions
│   │   ├── versioned/               ← Historical versions
│   │   │   └── {Event}Event.v1.*
│   │   └── upcasters/               ← Event migrations
│   │       └── {Event}Event_v1_v2.*
│   └── queries/                     ← Query definitions
│       └── Get{Resource}Query.*
│
├── ports/                           ← INTERFACES ONLY (Layer 2)
│   ├── {Aggregate}RepositoryPort.*  ← Repository interfaces
│   ├── CommandBusPort.*             ← Bus interfaces
│   ├── EventBusPort.*
│   ├── QueryBusPort.*
│   └── {External}ServicePort.*      ← External service interfaces
│
├── application/                     ← APPLICATION ORCHESTRATION (Layer 3)
│   └── {Service}Service.*           ← Orchestration services (optional)
│
├── infrastructure/                  ← CONCRETE IMPLEMENTATIONS (Layer 4)
│   ├── buses/                       ← Message routing
│   │   ├── InMemoryCommandBus.*     ← Implements CommandBusPort
│   │   ├── InMemoryEventBus.*       ← Implements EventBusPort
│   │   └── InMemoryQueryBus.*       ← Implements QueryBusPort
│   ├── repositories/
│   │   └── {DB}{Aggregate}Repository.*
│   └── external/
│       └── {External}Integration.*
│
└── slices/                          ← VERTICAL FEATURES (Layer 5)
    └── {feature-name}/              ← One feature slice
        ├── internal/                ← Private implementation
        │   ├── Handler.*            ← Command/event handlers
        │   └── {Infra}.*            ← Slice-specific infrastructure
        ├── {Feature}ReadModel.*     ← Public API (query slices)
        ├── slice.yaml               ← Metadata
        └── test_*.*                 ← Feature tests
```

## Layer Definitions

### Layer 1: Domain (Complete Business Logic)

**Purpose:** Contains ALL business logic and domain language, completely isolated from external concerns.

**Location:** `domain/`

**Contains:**
- **Aggregates** (`*Aggregate.*`) - Consistency boundaries at root
- **Value Objects** (`*ValueObjects.*`) - Immutable domain values (optional)
- **Commands** (`domain/commands/*Command.*`) - Write operations (input to domain)
- **Events** (`domain/events/*Event.*`) - Domain facts (output from domain)
- **Queries** (`domain/queries/*Query.*`) - Read requests

**Rules:**
- ✅ MUST be pure (no external dependencies outside domain)
- ✅ MUST NOT import from: `ports/`, `application/`, `infrastructure/`, `slices/`
- ✅ CAN import from other domain subfolders (commands, events, aggregates, value_objects)
- ✅ CAN use framework decorators (`@aggregate`, `@command`, `@event`, etc.)
- ✅ Aggregates CAN import events (required to emit them)
- ✅ Events CAN import value objects (for structured data)

**Example:**
```python
# domain/WorkflowAggregate.py
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from ..domain.commands import CreateWorkflowCommand

@aggregate("Workflow")
class WorkflowAggregate(AggregateRoot):
    """Pure domain logic - no infrastructure dependencies."""
    
    @command_handler("CreateWorkflowCommand")
    def create(self, command: "CreateWorkflowCommand") -> None:
        # Business validation
        if not command.name:
            raise DomainError("Workflow name required")
        
        # Apply event (emitted, not imported)
        self.apply(WorkflowCreatedEvent(
            workflow_id=self.id,
            name=command.name
        ))
```

### Layer 2: Events (Immutable Contracts)

**Purpose:** Events are the **public API** of the write side, the **contract** between bounded contexts, and the **bridge** between write and read sides (CQRS).

**Location:** `events/` **at context root** (preferred) or `domain/events/` (legacy support for backward compatibility)

**Contains:**
- **Current Events** (`*Event.*`) - Latest versions
- **Old Versions** (`versioned/*Event.v1.*`) - Historical versions
- **Upcasters** (`upcasters/*Event_v1_v2.*`) - Migration logic

**Rules:**
- ✅ MUST be immutable (frozen dataclasses, readonly properties)
- ✅ MUST have `@event` decorator with version
- ✅ MUST NOT import from ANYTHING (pure data)
- ✅ MUST use past tense naming (`WorkflowCreatedEvent`)

**Rationale for Context Root:**
1. **Events are contracts**, not implementation details
2. **Events outlive** any specific domain implementation
3. **Integration events** are more discoverable here
4. **Separates** immutable history from mutable domain logic
5. **Matches reference** (eventsourcing-book places events at context root)

**Example:**
```python
# events/WorkflowCreatedEvent.py
from dataclasses import dataclass
from datetime import datetime

@event("WorkflowCreated", "v1")
@dataclass(frozen=True)
class WorkflowCreatedEvent:
    """Immutable fact - no imports, no logic."""
    workflow_id: str
    name: str
    created_at: datetime
    
# events/versioned/WorkflowCreatedEvent.v0.py
@event("WorkflowCreated", "v0")
@dataclass(frozen=True)
class WorkflowCreatedEventV0:
    """Historical version - preserved forever."""
    workflow_id: str
    name: str
    # (missing created_at field)

# events/upcasters/WorkflowCreatedEvent_v0_v1.py
def upcast_v0_to_v1(old_event: WorkflowCreatedEventV0) -> WorkflowCreatedEvent:
    """Migrate old events to new schema."""
    return WorkflowCreatedEvent(
        workflow_id=old_event.workflow_id,
        name=old_event.name,
        created_at=datetime.now()  # Default for old events
    )
```

### Layer 3: Ports (Interfaces / Contracts)

**Purpose:** Define what the application needs from infrastructure. This is the **hexagonal boundary**.

**Location:** `ports/`

**Contains:**
- **Repository Interfaces** (`*RepositoryPort.*`)
- **Bus Interfaces** (`CommandBusPort`, `EventBusPort`, `QueryBusPort`)
- **External Service Interfaces** (`*ServicePort.*`)

**Rules:**
- ✅ MUST end with `Port` suffix
- ✅ MUST be interfaces/protocols only (no implementation)
- ✅ CAN import from: `domain/`, `events/`
- ✅ MUST NOT import from: `application/`, `infrastructure/`, `slices/`

**Why Explicit `ports/` Folder?** (Deviation from Reference)

The eventsourcing-book reference does NOT have a `ports/` folder because:
- ✅ Uses **Axon Framework** (provides `CommandGateway`, `Repository`, etc.)
- ✅ Framework abstractions are sufficient
- ✅ Interfaces colocated in slices when needed

**We keep `ports/` because:**
1. **No full framework** - AEF doesn't use Axon/Django/etc with built-in abstractions
2. **Explicit hexagonal** - Makes architecture visible and enforceable
3. **Discoverability** - All contracts in one place (easier for AI agents, new developers)
4. **Validation** - VSA can enforce dependency rules more easily
5. **Consistency** - All interfaces follow same pattern (`*Port` suffix)

**Trade-offs Accepted:**
- ⚠️ Extra folder (not in reference)
- ⚠️ More indirection than colocated interfaces
- ✅ BUT: Worth it for explicit architecture and automated validation

**Example:**
```python
# ports/WorkflowRepositoryPort.py
from typing import Protocol
from ..domain import WorkflowAggregate

class WorkflowRepositoryPort(Protocol):
    """Interface for workflow persistence - no implementation here."""
    
    async def save(self, aggregate: WorkflowAggregate) -> None: ...
    async def load(self, workflow_id: str) -> WorkflowAggregate: ...

# ports/CommandBusPort.py
class CommandBusPort(Protocol):
    """Interface for command routing."""
    
    async def send(self, command: Any) -> None: ...
```

### Layer 4: Application (Orchestration Services)

**Purpose:** Application-level orchestration and coordination of domain logic. This is for **business process orchestration**, NOT technical infrastructure.

**Location:** `application/`

**Contains:**
- **Saga Coordinators** - Multi-aggregate workflows
- **Process Managers** - Long-running business processes
- **Application Services** - Injected into domain (e.g., DeviceFingerPrintCalculator from reference)

**Rules:**
- ✅ CAN import from: `domain/`, `events/`, `ports/`
- ✅ MUST NOT import from: `infrastructure/`, `slices/`
- ✅ Contains business orchestration, not technical plumbing

**Application vs Ports vs Infrastructure:**

| Layer | Purpose | Example | Implementation |
|-------|---------|---------|----------------|
| **application/** | Business orchestration | WorkflowSagaCoordinator | Concrete service |
| **ports/** | Contracts for infrastructure | CommandBusPort | Interface only |
| **infrastructure/** | Technical implementations | InMemoryCommandBus | Concrete implementation |

**Why Buses Are NOT in Application:**

❌ **NOT Application**: `application/CommandBus.py`  
✅ **Interface**: `ports/CommandBusPort.py`  
✅ **Implementation**: `infrastructure/buses/InMemoryCommandBus.py`

**Rationale:** Buses are **message routing** (technical plumbing), NOT **business orchestration**.

**Example:**
```python
# application/WorkflowSagaCoordinator.py
from ..ports import CommandBusPort, EventBusPort

class WorkflowSagaCoordinator:
    """Business process orchestration - coordinates multiple aggregates."""
    
    def __init__(self, command_bus: CommandBusPort, event_bus: EventBusPort):
        self.command_bus = command_bus
        self.event_bus = event_bus
    
    async def coordinate_workflow_execution(self, workflow_id: str) -> None:
        """Orchestrates multi-step workflow process."""
        # Listen for WorkflowCreatedEvent
        # Send CreateExecutionCommand
        # Wait for ExecutionCompletedEvent
        # Send FinalizeWorkflowCommand
        pass
```

### Layer 5: Infrastructure (Concrete Implementations)

**Purpose:** All concrete implementations of ports and external integrations.

**Location:** `infrastructure/`

**Contains:**
- **Buses** (`infrastructure/buses/`) - Command/Event/Query routing
- **Repositories** (`infrastructure/repositories/`) - Persistence implementations
- **External Integrations** (`infrastructure/external/`) - Third-party APIs

**Rules:**
- ✅ CAN import from: `domain/`, `events/`, `ports/`, `application/`
- ✅ MUST implement interfaces from `ports/`
- ✅ Contains ALL external dependencies (databases, message queues, etc.)

**Decision: Buses in Infrastructure**

**Location:** `infrastructure/buses/`  
**Interfaces:** `ports/CommandBusPort`, `ports/EventBusPort`, `ports/QueryBusPort`

**Why infrastructure, not application?**
1. Buses are **message routing** (technical concern)
2. Buses handle **serialization, transport, retries** (infrastructure concerns)
3. Application layer is for **business orchestration**, not message plumbing

**Example:**
```python
# infrastructure/buses/InMemoryCommandBus.py
from ...ports import CommandBusPort
from ...domain import WorkflowAggregate

class InMemoryCommandBus(CommandBusPort):
    """Concrete implementation of command routing."""
    
    def __init__(self, repository: WorkflowRepositoryPort):
        self.repository = repository
        self.handlers = {}
    
    async def send(self, command: Any) -> None:
        # Route command to aggregate
        aggregate = await self.repository.load(command.aggregate_id)
        aggregate.handle(command)
        await self.repository.save(aggregate)

# infrastructure/repositories/PostgresWorkflowRepository.py
import asyncpg
from ...domain import WorkflowAggregate
from ...ports import WorkflowRepositoryPort

class PostgresWorkflowRepository(WorkflowRepositoryPort):
    """Persistence implementation."""
    
    def __init__(self, connection_pool: asyncpg.Pool):
        self.pool = connection_pool
    
    async def save(self, aggregate: WorkflowAggregate) -> None:
        # Serialize and store events
        pass
```

### Layer 6: Slices (Vertical Features)

**Purpose:** Feature-isolated adapters that translate external protocols to domain operations.

**Location:** `slices/`

**Structure:**
```
slices/{feature-name}/
├── internal/                ← Private implementation
│   ├── Handler.*            ← Command/event handlers
│   └── Controller.*         ← REST/CLI/gRPC adapters
├── {Feature}ReadModel.*     ← Public API (query slices)
├── slice.yaml
└── test_*.*
```

**Rules:**
- ✅ MUST be in `slices/` subfolder (not at context root)
- ✅ MUST use `internal/` for private implementation
- ✅ Public read models at slice root
- ✅ CAN import from: `domain/`, `events/`, `ports/`, `application/`, `infrastructure/`
- ✅ MUST NOT import from other slices

**Rationale for `internal/` Subfolder:**
1. **Matches reference** (eventsourcing-book uses `{feature}/internal/`)
2. **Explicit public vs private** - Read models are public, handlers are private
3. **Better encapsulation** - Clear what's exposed vs implementation detail

**Example:**
```python
# slices/create_workflow/internal/Handler.py (PRIVATE)
from ....domain.commands import CreateWorkflowCommand
from ....ports import CommandBusPort

class CreateWorkflowHandler:
    """Private implementation - thin adapter."""
    
    def __init__(self, command_bus: CommandBusPort):
        self.command_bus = command_bus
    
    async def handle(self, name: str) -> str:
        command = CreateWorkflowCommand(
            workflow_id=uuid4(),
            name=name
        )
        await self.command_bus.send(command)
        return command.workflow_id

# slices/workflow_list/WorkflowListReadModel.py (PUBLIC)
@dataclass
class WorkflowListReadModel:
    """Public API - consumed by clients."""
    workflows: List[WorkflowSummary]
    total_count: int
```

## Dependency Rules (Strict)

### Allowed Dependencies

```
Layer 1: domain/        → (nothing - absolutely pure)
Layer 2: events/        → (nothing - pure data)
Layer 3: ports/         → domain, events
Layer 4: application/   → domain, events, ports
Layer 5: infrastructure/→ domain, events, ports, application
Layer 6: slices/        → domain, events, ports, application, infrastructure
                          ❌ NOT other slices
```

### Visual Dependency Flow

```
┌─────────────────────────────────────────────────────────┐
│                     SLICES (Layer 6)                     │
│                    Vertical Features                     │
│                           ▼                               │
│  ┌─────────────────────────────────────────────────┐    │
│  │        INFRASTRUCTURE (Layer 5)                  │    │
│  │        Concrete Implementations                  │    │
│  │                    ▼                             │    │
│  │  ┌────────────────────────────────────────┐     │    │
│  │  │    APPLICATION (Layer 4)               │     │    │
│  │  │    Orchestration Services              │     │    │
│  │  │                ▼                        │     │    │
│  │  │  ┌───────────────────────────────┐    │     │    │
│  │  │  │  PORTS (Layer 3)              │    │     │    │
│  │  │  │  Interfaces / Contracts       │    │     │    │
│  │  │  │            ▼                   │    │     │    │
│  │  │  │  ┌─────────────────────┐     │    │     │    │
│  │  │  │  │  EVENTS (Layer 2)   │     │    │     │    │
│  │  │  │  │  Immutable History  │     │    │     │    │
│  │  │  │  └─────────────────────┘     │    │     │    │
│  │  │  │            ▲                   │    │     │    │
│  │  │  │  ┌─────────────────────┐     │    │     │    │
│  │  │  │  │  DOMAIN (Layer 1)   │     │    │     │    │
│  │  │  │  │  Pure Business Logic│     │    │     │    │
│  │  │  │  └─────────────────────┘     │    │     │    │
│  │  │  └───────────────────────────────┘    │     │    │
│  │  └────────────────────────────────────────┘     │    │
│  └─────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────┘

All arrows point INWARD (hexagonal architecture principle)
Domain at center, zero outward dependencies
```

## Naming Conventions (Enforced by VSA)

### Required Suffixes

| Type | Suffix | Example | Location |
|------|--------|---------|----------|
| **Aggregate** | `*Aggregate` | `WorkflowAggregate.py` | `domain/` |
| **Command** | `*Command` | `CreateWorkflowCommand.py` | `domain/commands/` |
| **Event** | `*Event` | `WorkflowCreatedEvent.py` | `events/` |
| **Query** | `*Query` | `GetWorkflowDetailQuery.py` | `domain/queries/` |
| **Port** | `*Port` | `WorkflowRepositoryPort.py` | `ports/` |
| **Value Object** | `*ValueObjects` | `WorkflowValueObjects.py` | `domain/` (optional) |

### Examples

✅ **Good:**
```
domain/WorkflowAggregate.py
domain/commands/CreateWorkflowCommand.py
events/WorkflowCreatedEvent.py
ports/WorkflowRepositoryPort.py
ports/CommandBusPort.py
```

❌ **Bad:**
```
domain/Workflow.py                    ← Missing Aggregate suffix
domain/commands/CreateWorkflow.py     ← Missing Command suffix
domain/events/WorkflowCreated.py      ← Events at wrong location
ports/WorkflowRepository.py           ← Missing Port suffix
application/CommandBus.py             ← Wrong location (buses are infrastructure)
```

## Value Objects

### Location and Characteristics

**Location:** `domain/{Name}ValueObjects.*` (if complex/reusable)

**Characteristics:**
- ✅ Immutable (`frozen=True`, readonly properties, immutable structs)
- ✅ Compared by value, not identity
- ✅ Contain validation logic, minimal behavior
- ✅ Shared across aggregates and slices

**When to use separate file:**
- Complex value objects with validation
- Reused across multiple aggregates
- Need to be discoverable by VSA tools

**When to inline:**
- Simple value objects (just data)
- Used only in one aggregate
- Type aliases (`WorkflowId = str`)

**Example (Separate File):**
```python
# domain/WorkflowValueObjects.py
from dataclasses import dataclass
from typing import NewType

@dataclass(frozen=True)
class WorkflowName:
    """Value object with validation."""
    value: str
    
    def __post_init__(self):
        if not self.value or len(self.value) > 100:
            raise ValueError("Invalid workflow name")

WorkflowId = NewType('WorkflowId', str)
ExecutionId = NewType('ExecutionId', str)
```

**Example (Inline):**
```python
# domain/WorkflowAggregate.py
from typing import NewType

WorkflowId = NewType('WorkflowId', str)  # Simple type alias

@aggregate("Workflow")
class WorkflowAggregate(AggregateRoot):
    id: WorkflowId
    ...
```

## Decorator Validation

VSA can optionally validate framework decorators:

```python
# Aggregates MUST have @aggregate decorator
@aggregate("Workflow")
class WorkflowAggregate(AggregateRoot):
    pass

# Events MUST have @event decorator with version
@event("WorkflowCreated", "v1")
@dataclass(frozen=True)
class WorkflowCreatedEvent:
    pass

# Commands SHOULD have @command decorator (optional)
@command("CreateWorkflow")
@dataclass(frozen=True)
class CreateWorkflowCommand:
    pass
```

## Infrastructure Detection

Reserved folders (skipped during slice scanning):

- `domain/` (contains commands/, queries/ subfolders)
- `events/`
- `ports/`
- `application/`
- `infrastructure/`
- `repositories/` (if at context root - legacy)
- `services/` (if at context root - legacy)
- `adapters/` (if at context root - legacy)

## Language-Specific Patterns

### Python

```python
# domain/WorkflowAggregate.py
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from ..domain.commands import CreateWorkflowCommand

@aggregate("Workflow")
class WorkflowAggregate(AggregateRoot):
    @command_handler("CreateWorkflowCommand")
    def create(self, command: "CreateWorkflowCommand") -> None:
        self.apply(WorkflowCreatedEvent(...))

# events/WorkflowCreatedEvent.py
from dataclasses import dataclass

@event("WorkflowCreated", "v1")
@dataclass(frozen=True)
class WorkflowCreatedEvent:
    workflow_id: str
    name: str

# ports/WorkflowRepositoryPort.py
from typing import Protocol

class WorkflowRepositoryPort(Protocol):
    async def save(self, aggregate: WorkflowAggregate) -> None: ...

# infrastructure/buses/InMemoryCommandBus.py
from ...ports import CommandBusPort

class InMemoryCommandBus(CommandBusPort):
    async def send(self, command: Any) -> None:
        # Implementation
        pass
```

### TypeScript

```typescript
// domain/WorkflowAggregate.ts
import { Aggregate, AggregateRoot } from '@esp/framework';
import type { CreateWorkflowCommand } from './commands/CreateWorkflowCommand';

@Aggregate('Workflow')
export class WorkflowAggregate extends AggregateRoot {
  @CommandHandler('CreateWorkflowCommand')
  create(command: CreateWorkflowCommand): void {
    this.apply(new WorkflowCreatedEvent(...));
  }
}

// events/WorkflowCreatedEvent.ts
@Event('WorkflowCreated', 'v1')
export class WorkflowCreatedEvent {
  constructor(
    public readonly workflowId: string,
    public readonly name: string
  ) {}
}

// ports/WorkflowRepositoryPort.ts
export interface WorkflowRepositoryPort {
  save(aggregate: WorkflowAggregate): Promise<void>;
}

// infrastructure/buses/InMemoryCommandBus.ts
import { CommandBusPort } from '../../ports/CommandBusPort';

export class InMemoryCommandBus implements CommandBusPort {
  async send(command: any): Promise<void> {
    // Implementation
  }
}
```

### Rust

```rust
// domain/workflow_aggregate.rs
#[aggregate("Workflow")]
pub struct WorkflowAggregate {
    id: WorkflowId,
    name: String,
}

impl WorkflowAggregate {
    #[command_handler("CreateWorkflowCommand")]
    pub fn create(&mut self, command: CreateWorkflowCommand) -> Result<Vec<Event>> {
        self.apply(WorkflowCreatedEvent { ... })
    }
}

// events/workflow_created_event.rs
#[event("WorkflowCreated", "v1")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowCreatedEvent {
    pub workflow_id: String,
    pub name: String,
}

// ports/workflow_repository_port.rs
#[async_trait]
pub trait WorkflowRepositoryPort {
    async fn save(&self, aggregate: &WorkflowAggregate) -> Result<()>;
}

// infrastructure/buses/in_memory_command_bus.rs
use crate::ports::CommandBusPort;

pub struct InMemoryCommandBus {
    // ...
}

#[async_trait]
impl CommandBusPort for InMemoryCommandBus {
    async fn send(&self, command: Box<dyn Any>) -> Result<()> {
        // Implementation
    }
}
```

## Anti-Patterns

### ❌ Domain Importing Ports

```python
# domain/WorkflowAggregate.py
from ..ports import WorkflowRepositoryPort  # ❌ WRONG

class WorkflowAggregate:
    def __init__(self, repository: WorkflowRepositoryPort):  # ❌ Domain depends on ports
        self.repository = repository
```

**Fix:** Domain should be pure, repositories injected via application layer or bus.

### ❌ Commands/Events in Slice Folders

```
slices/create_workflow/
  ├── CreateWorkflowCommand.py  # ❌ Should be in domain/commands/
  └── WorkflowCreatedEvent.py   # ❌ Should be in events/
```

**Fix:** Centralize commands in `domain/commands/`, events in `events/` (context root).

### ❌ Slices Outside slices/ Folder

```
contexts/workflows/
  ├── cleanup/  # ❌ Should be slices/cleanup/
  └── slices/
```

**Fix:** ALL features must be in `slices/` subfolder.

### ❌ Business Logic in Slices

```python
# slices/create_workflow/internal/Handler.py
class CreateWorkflowHandler:
    async def handle(self, name: str) -> str:
        if len(name) > 100:  # ❌ Business validation belongs in aggregate
            raise ValueError("Name too long")
```

**Fix:** Slices are thin adapters, business logic goes in domain.

### ❌ Cross-Slice Imports

```python
# slices/create_workflow/internal/Handler.py
from ...get_workflow.WorkflowReadModel import WorkflowReadModel  # ❌ Cross-slice coupling
```

**Fix:** Share via domain/events/ports only, never import between slices.

### ❌ Buses in Application Layer

```
application/
  ├── CommandBus.py  # ❌ Buses are infrastructure
  └── EventBus.py    # ❌ Wrong location
```

**Fix:**
- Interface: `ports/CommandBusPort.py`
- Implementation: `infrastructure/buses/InMemoryCommandBus.py`

## Migration Path

### From Colocated Commands/Events

**Before:**
```
slices/create_workflow/
  ├── CreateWorkflowCommand.py
  ├── WorkflowCreatedEvent.py
  └── Handler.py
```

**After:**
```
domain/commands/CreateWorkflowCommand.py
events/WorkflowCreatedEvent.py
slices/create_workflow/internal/Handler.py  ← Imports from domain/events
```

**Steps:**
1. Move commands to `domain/commands/`
2. Move events to `events/` (context root)
3. Update imports in slices
4. Run tests

### From _shared/ Pattern

**Before:**
```
_shared/
  ├── WorkflowAggregate.py
  └── WorkflowValueObjects.py
```

**After:**
```
domain/
  ├── WorkflowAggregate.py
  └── WorkflowValueObjects.py
```

**Steps:**
1. Rename `_shared/` to `domain/`
2. Create `domain/commands/` and `domain/queries/`
3. Create `events/` at context root
4. Update all imports

### From Flat Structure

**Before:**
```
contexts/workflows/
  ├── WorkflowAggregate.py
  ├── CreateWorkflowCommand.py
  ├── WorkflowCreatedEvent.py
  ├── create_workflow.py
  └── get_workflow.py
```

**After:**
```
contexts/workflows/
  ├── domain/
  │   ├── WorkflowAggregate.py
  │   └── commands/CreateWorkflowCommand.py
  ├── events/WorkflowCreatedEvent.py
  └── slices/
      ├── create_workflow/internal/Handler.py
      └── get_workflow/WorkflowReadModel.py
```

## VSA Tooling Support

### Configuration (vsa.yaml)

```yaml
domain:
  path: domain/
  aggregates:
    pattern: '*Aggregate.{ts,py,rs}'
    require_suffix: true
  commands:
    path: domain/commands/
    require_suffix: true
  queries:
    path: domain/queries/
    require_suffix: true

events:
  path: events/  # Context root
  require_suffix: true
  require_version_decorator: true

ports:
  path: ports/
  require_suffix: true

application:
  path: application/

infrastructure:
  path: infrastructure/
  buses:
    path: infrastructure/buses/
  repositories:
    path: infrastructure/repositories/

slices:
  path: slices/
  require_internal_subfolder: true
  types: [command, query, saga]

validation:
  domain_purity: true
  event_isolation: true
  port_isolation: true
  no_cross_slice_imports: true
  naming_conventions: true
```

### CLI Commands

```bash
# Validate structure
vsa validate --strict

# Validate specific context
vsa validate --context workflows --strict

# Generate manifest with domain data
vsa manifest --include-domain > manifest.json

# Generate architecture documentation
vsa manifest --include-domain | vsa-visualizer

# Create new bounded context with standard structure
vsa context create workflow-engine
```

### Validation Output

```bash
$ vsa validate --strict

Validating contexts/workflows...

Structure:
  ✓ All aggregates in domain/ root
  ✓ All commands in domain/commands/
  ✓ All events in events/ (context root)
  ✓ All slices in slices/ subfolder
  
Naming:
  ✓ All ports end with Port suffix
  ✓ All aggregates end with Aggregate suffix
  ✓ All commands end with Command suffix
  ✓ All events end with Event suffix
  
Dependencies:
  ✓ Domain is pure (no external imports)
  ✓ Events are pure data (no imports)
  ✓ Ports only import domain/events
  ✓ No cross-slice imports
  ✗ ERROR: domain/WorkflowAggregate.py imports from ports/ (line 12)
  ✗ ERROR: slices/cleanup/ is at context root (should be in slices/)

2 errors, 0 warnings

Run 'vsa validate --fix' to auto-fix structure issues.
```

## Consequences

### Positive

1. **Discoverability** ✅
   - All domain concepts in predictable locations
   - AI agents and developers know where to look
   - VSA tooling can scan and validate automatically

2. **Maintainability** ✅
   - Strict dependency rules prevent architectural erosion
   - Hexagonal boundaries enforce separation of concerns
   - Easy to refactor infrastructure without touching domain

3. **Parallelizability** ✅
   - Slices can be worked on independently
   - Multiple developers/agents on different features
   - Domain changes coordinated centrally

4. **Testability** ✅
   - Pure domain, easy to unit test
   - Slices tested with mocked buses
   - Infrastructure tested separately

5. **Validation** ✅
   - VSA enforces standards automatically
   - CI fails on violations
   - No architectural drift

### Negative

1. **More Folders** ⚠️
   - More directories than simple structures
   - **Mitigation:** VSA generators create structure automatically

2. **Learning Curve** ⚠️
   - Developers need to understand layer purposes
   - **Mitigation:** Comprehensive documentation, examples, VSA guidance

3. **Indirection** ⚠️
   - Ports add layer between domain and infrastructure
   - **Mitigation:** Better than tight coupling, enables testing

### Neutral

1. **Framework Agnostic**
   - No assumptions about Axon/Django/etc
   - Requires explicit infrastructure setup
   - Trade-off: More boilerplate, but more control

2. **Explicit vs Implicit**
   - Reference uses framework abstractions (implicit)
   - We use explicit ports (visible boundaries)
   - Both valid, we choose explicit for validation

## Comparison to Reference Implementation

### Alignment with eventsourcing-book

| Aspect | Reference | This ADR | Rationale |
|--------|-----------|----------|-----------|
| **Events Location** | `events/` (context root) | ✅ `events/` (context root) | Matches - events are contracts |
| **Commands Location** | `domain/commands/{feature}/` | ✅ `domain/commands/` | Matches - supports subfolders |
| **Aggregates Location** | `domain/` root | ✅ `domain/` root | Matches - high visibility |
| **Slice Structure** | `{feature}/internal/` | ✅ `slices/{feature}/internal/` | Extended - added slices/ container |
| **Application Services** | `application/` | ✅ `application/` | Matches - orchestration services |
| **Ports Folder** | ❌ No `ports/` folder | ✅ `ports/` folder | **Deviation** - explicit hexagonal |
| **Buses Location** | N/A (framework) | ✅ `infrastructure/buses/` | Extended - framework-agnostic |
| **Read Models** | At feature root | ✅ At slice root | Matches - public API |

### Key Deviations Explained

**1. Explicit `ports/` Folder**
- **Why reference doesn't have it:** Axon Framework provides abstractions
- **Why we have it:** Framework-agnostic, explicit hexagonal, validation

**2. `slices/` Container Folder**
- **Why reference doesn't have it:** Features at context root (cleaner)
- **Why we have it:** Easier slice isolation validation, prevents infrastructure being treated as slice

**3. Bus Interfaces and Implementations**
- **Why reference doesn't show it:** Axon's CommandGateway/QueryGateway provided
- **Why we have it:** Explicit control, framework-agnostic, swappable implementations

## Related ADRs

- **ADR-004:** Command Handlers in Aggregates (aggregate patterns)
- **ADR-005:** Hexagonal Architecture for Event-Sourced Systems (foundational principles)
- **ADR-006:** Domain Organization Pattern (aggregate/command/event organization)
- **ADR-007:** Event Versioning and Upcasters (event evolution)
- **ADR-008:** Vertical Slices as Hexagonal Adapters (slice patterns)
- **ADR-009:** CQRS Pattern Implementation (command vs query separation)

This ADR **extends and consolidates** the above ADRs into one comprehensive reference.

## References

- **"Understanding Event Sourcing"** - Alexey Zimarev (primary reference)
- **"Hands-On Domain-Driven Design with .NET Core"** - Alexey Zimarev
- **"Domain-Driven Design"** - Eric Evans
- **"Implementing Domain-Driven Design"** - Vaughn Vernon
- **"Hexagonal Architecture"** - Alistair Cockburn
- **"Clean Architecture"** - Robert C. Martin
- **"Vertical Slice Architecture"** - Jimmy Bogard

---

**Last Updated:** 2025-01-22  
**Supersedes:** Extends ADR-005, ADR-006, ADR-008  
**Superseded By:** None

---

## Appendix: Decision Log

### Key Decisions Made

1. **Events at context root** (NOT `domain/events/`)
   - Reason: Events are contracts, not implementation
   - Source: eventsourcing-book pattern

2. **Explicit `ports/` folder** (deviation from reference)
   - Reason: Framework-agnostic, discoverability, validation
   - Trade-off: Extra folder vs architectural clarity

3. **Buses in `infrastructure/buses/`** (NOT `application/`)
   - Reason: Buses are message routing (technical), not business orchestration
   - Pattern: Interface in `ports/`, implementation in `infrastructure/`

4. **Slices in `slices/` container**
   - Reason: Easier validation, prevents infrastructure treated as slice
   - Pattern: `slices/{feature}/internal/` for private implementation

5. **Application layer separate from ports**
   - Reason: Different purposes (orchestration vs interfaces)
   - Pattern: Application uses ports, not the other way around

6. **Naming conventions enforced**
   - Reason: Discoverability, automated validation
   - Suffixes: *Aggregate, *Command, *Event, *Query, *Port

### Alternatives Considered

**Alternative 1: Events in `domain/events/`**
- ✅ Keeps all domain concepts together
- ❌ Events are contracts, not domain implementation
- ❌ Doesn't match reference implementation
- **Rejected:** Events at context root is canonical pattern

**Alternative 2: No `ports/` folder (colocated interfaces)**
- ✅ Less folders, matches reference
- ❌ Harder to discover all contracts
- ❌ Harder to validate hexagonal boundaries
- **Rejected:** Explicit ports better for framework-agnostic approach

**Alternative 3: Buses in `application/`**
- ✅ Simpler (one less layer)
- ❌ Mixes business orchestration with technical plumbing
- ❌ Buses are infrastructure concern
- **Rejected:** Clear separation of concerns more important

**Alternative 4: Features at context root (no `slices/` container)**
- ✅ Matches reference exactly
- ❌ Harder to distinguish features from infrastructure folders
- ❌ More complex validation rules
- **Accepted with caveat:** Make `slices/` optional in VSA config

---

**This ADR is the canonical reference for all ESP/VSA applications.**
