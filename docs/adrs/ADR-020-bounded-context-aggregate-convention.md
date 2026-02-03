# ADR-020: Bounded Context and Aggregate Convention

**Status:** Accepted  
**Date:** 2026-02-02  
**Deciders:** Architecture Team  
**Context:** Establishing clear conventions for bounded contexts, aggregates, and their relationships

## Context and Problem Statement

Domain-Driven Design concepts are often misunderstood, leading to:
- Projection-only modules treated as bounded contexts
- Unclear aggregate boundaries
- Orphan entities without clear ownership
- Difficulty visualizing system structure

We need clear, enforceable conventions that:
1. Make DDD concepts visible in the file system
2. Can be validated by the VSA tool
3. Prevent architectural drift
4. Help developers move fast without breaking boundaries

## Decision Drivers

- **Visibility**: Architectural boundaries should be visible in the file system
- **Enforceability**: VSA tool can validate the structure
- **Clarity**: Developers understand where code belongs
- **Maintainability**: Prevent coupling and orphan code
- **Scalability**: Support future extraction to microservices

## Decisions

### Decision 1: Multiple Aggregates per Bounded Context

**A bounded context CAN and SHOULD contain multiple aggregates when they share the same domain language.**

```
BOUNDED CONTEXT: orchestration
│
├── AGGREGATE: Workspace        (consistency boundary #1)
├── AGGREGATE: Workflow         (consistency boundary #2)
└── AGGREGATE: WorkflowExecution (consistency boundary #3)
```

**Rationale:**
- Bounded context = semantic boundary (ubiquitous language)
- Aggregate = consistency boundary (atomic changes)
- These are DIFFERENT concepts at different levels

**Why separate aggregates within one context?**
- Different lifecycles (Workflow is long-lived, Execution is transient)
- Different consistency needs
- Different access patterns
- Communicate via domain events, not direct references

### Decision 2: Aggregate Folder Convention

**Each aggregate lives in a folder named `aggregate_<name>/`**

```
context/domain/
├── aggregate_execution/                    # All sort together alphabetically
│   ├── WorkflowExecutionAggregate.py       # THE root (full name, not generic)
│   ├── PhaseExecution.py                   # Entity
│   └── ExecutionStatus.py                  # Value Object
│
├── aggregate_workflow/
│   ├── WorkflowAggregate.py
│   └── PhaseDefinition.py
│
├── aggregate_workspace/
│   ├── WorkspaceAggregate.py
│   ├── IsolationHandle.py
│   └── SecurityPolicy.py
│
├── commands/
├── events/
└── _shared/
```

**Convention Rules:**
1. Folder name: `aggregate_<name>/` (lowercase, snake_case)
2. Root file: `<Name>Aggregate.py` (PascalCase, FULL name)
3. Only ONE `*Aggregate.py` file per `aggregate_*` folder
4. Entities and VOs co-located in same folder as their root
5. Shared VOs go in `_shared/` (truly shared across aggregates only)

**Rationale:**
- `aggregate_` prefix makes all aggregates sort together in file explorers
- Full file names (not `aggregate.py`) make editor tabs useful
- One root per folder enforces consistency boundary
- Co-location prevents orphan entities

### Decision 3: Aggregate Root IS the Entry Point

**The `*Aggregate.py` class IS the aggregate root. All access goes through it.**

```python
# WorkspaceAggregate.py - THIS IS THE ROOT
class WorkspaceAggregate(AggregateRoot):
    """Workspace aggregate root.
    
    All external access to the Workspace aggregate goes through this class.
    Internal entities (IsolationHandle) are accessed only through methods here.
    """
    
    @command_handler("CreateWorkspaceCommand")
    def create(self, command: CreateWorkspaceCommand) -> None:
        # Business logic here
        self._apply(WorkspaceCreatedEvent(...))
```

**Entities are accessed through the root:**
```python
# IsolationHandle is an ENTITY within the Workspace aggregate
# It has identity (isolation_id) but is accessed through WorkspaceAggregate

class WorkspaceAggregate(AggregateRoot):
    def __init__(self):
        self.isolation_handle: IsolationHandle | None = None
    
    def record_isolation_started(self, isolation_id: str, ...):
        # Only the root creates/modifies entities
        self.isolation_handle = IsolationHandle(isolation_id, ...)
```

### Decision 4: Entity vs Value Object Distinction

**Entities have identity. Value Objects do not.**

| Type | Identity | Mutability | Example |
|------|----------|------------|---------|
| Entity | Has unique ID field | Can change | `IsolationHandle(isolation_id="...")` |
| Value Object | Defined by attributes | Immutable | `SecurityPolicy(memory_limit=2048)` |

**In Python:**
```python
# ENTITY - has identity
@dataclass
class IsolationHandle:
    isolation_id: str  # ← Identity field
    isolation_type: str
    proxy_url: str | None

# VALUE OBJECT - immutable, no identity
@dataclass(frozen=True)  # ← Immutable!
class SecurityPolicy:
    memory_limit_mb: int
    cpu_limit: float
    network_enabled: bool
```

### Decision 5: Projections Are Query Slices Within Bounded Contexts

**Projections belong in `slices/` of the bounded context that owns the data.**

```
✅ CORRECT: sessions/slices/session_cost/projection.py
   → Session cost is owned by sessions context

❌ WRONG: costs/slices/session_cost/projection.py
   → "costs" is not a bounded context (no aggregates)
```

**Rationale:**
- A projection is a vertical slice (a feature)
- It reads events from a specific domain
- It belongs where those events originate

### Decision 6: Bounded Context MUST Have Aggregates

**A directory is only a valid bounded context if it contains `aggregate_*` folders.**

```
✅ VALID: orchestration/ contains aggregate_workspace/, aggregate_workflow/
❌ INVALID: metrics/ has projections but no aggregates
```

**VSA Validation:**
```
⚠️ WARNING: 'metrics' has no aggregates - not a valid bounded context
   Projections should live in the bounded context that owns the data.
   Consider moving to: orchestration/slices/ or sessions/slices/
```

## Consequences

### Positive

- **Visible architecture**: Boundaries are visible in file system
- **Enforceable rules**: VSA tool can validate structure
- **Clear ownership**: No orphan entities or projections
- **Tab-friendly**: Full file names work in editor tabs
- **Sorted together**: All `aggregate_*` folders group naturally
- **Scalable**: Contexts can become microservices

### Negative

- **More folders**: Deeper nesting than flat structure
- **Migration effort**: Existing code needs restructuring
- **Learning curve**: Developers must understand conventions

## Validation Rules (VSA Tool)

| Rule | Check | Error Message |
|------|-------|---------------|
| BC has aggregates | `aggregate_*/` exists | "Not a valid bounded context" |
| One root per aggregate | Count `*Aggregate.py` | "Only one root per folder" |
| Root in folder | Root in `aggregate_*` | "Move to aggregate_* folder" |
| Entities co-located | Entity in same folder as root | "Move entity to aggregate folder" |
| No cross-context imports | Import analysis | "Boundary violation" |

## Examples

### Complete Bounded Context Structure

```
contexts/orchestration/
├── domain/
│   ├── aggregate_execution/
│   │   ├── WorkflowExecutionAggregate.py   # Root
│   │   ├── PhaseExecution.py               # Entity
│   │   └── ExecutionStatus.py              # Value Object
│   │
│   ├── aggregate_workflow/
│   │   ├── WorkflowAggregate.py            # Root
│   │   └── PhaseDefinition.py              # Value Object
│   │
│   ├── aggregate_workspace/
│   │   ├── WorkspaceAggregate.py           # Root
│   │   ├── IsolationHandle.py              # Entity
│   │   ├── SecurityPolicy.py               # Value Object
│   │   └── ExecutionResult.py              # Value Object
│   │
│   ├── commands/
│   │   ├── CreateWorkspaceCommand.py
│   │   ├── ExecuteWorkflowCommand.py
│   │   └── ...
│   │
│   ├── events/
│   │   ├── WorkspaceCreatedEvent.py
│   │   ├── WorkflowStartedEvent.py
│   │   └── ...
│   │
│   └── _shared/
│       └── identifiers.py                  # Shared IDs
│
├── slices/
│   ├── create_workspace/                   # Command slice
│   ├── execute_workflow/                   # Command slice
│   ├── workspace_metrics/                  # Query slice
│   ├── dashboard_metrics/                  # Query slice
│   └── ...
│
└── ports/
    └── ...
```

## Links

- [ADR-003: Bounded Context Structure](./ADR-003-bounded-context-structure.md)
- [ADR-019: VSA Standard Structure](./ADR-019-vsa-standard-structure.md)
- [Domain-Driven Design by Eric Evans](https://www.domainlanguage.com/ddd/)
- [Aggregates in DDD](https://martinfowler.com/bliki/DDD_Aggregate.html)
