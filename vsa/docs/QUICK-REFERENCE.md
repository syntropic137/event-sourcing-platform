# VSA Quick Reference Card

**Version:** 0.7+  
**Last Updated:** 2026-02-02

---

## Directory Structure

```
contexts/<context>/
├── domain/
│   ├── aggregate_<name>/           # One folder per aggregate
│   │   ├── <Name>Aggregate.py      # THE root (one only, full name!)
│   │   ├── <Entity>.py             # Entities (have identity)
│   │   └── <ValueObject>.py        # Value objects (immutable)
│   │
│   ├── commands/                   # All commands
│   │   └── <Name>Command.py
│   │
│   ├── events/                     # All events
│   │   └── <Name>Event.py
│   │
│   ├── queries/                    # Query definitions
│   │   └── <Name>Query.py
│   │
│   └── _shared/                    # Shared VOs (across aggregates)
│
├── slices/
│   ├── <command_feature>/          # Command slices
│   │   ├── <Name>Handler.py
│   │   └── test_<feature>.py
│   │
│   └── <query_feature>/            # Query slices (projections)
│       ├── projection.py
│       ├── handler.py
│       └── test_<feature>.py
│
├── ports/                          # Interface definitions
│   └── <Name>Port.py
│
└── _shared/                        # Context-internal shared
```

---

## Naming Conventions

| Type | Pattern | Example |
|------|---------|---------|
| Bounded context | `snake_case` | `orchestration/` |
| Aggregate folder | `aggregate_<name>` | `aggregate_workspace/` |
| Aggregate root | `<Name>Aggregate.py` | `WorkspaceAggregate.py` |
| Entity | `<Name>.py` | `IsolationHandle.py` |
| Value Object | `<Name>.py` (frozen) | `SecurityPolicy.py` |
| Command | `<Name>Command.py` | `CreateWorkspaceCommand.py` |
| Event | `<Name>Event.py` | `WorkspaceCreatedEvent.py` |
| Slice (command) | `snake_case/` | `create_workspace/` |
| Slice (query) | `snake_case/` | `workspace_metrics/` |
| Port | `<Name>Port.py` | `WorkspaceRepositoryPort.py` |
| Test | `test_<feature>.py` | `test_create_workspace.py` |

---

## Key Definitions

| Term | Definition |
|------|------------|
| **Bounded Context** | Semantic boundary - groups concepts sharing same language |
| **Aggregate** | Consistency boundary - atomic/transactional changes |
| **Aggregate Root** | Entry point for all aggregate access (the `*Aggregate` class) |
| **Entity** | Object with identity, accessed through root only |
| **Value Object** | Immutable object, no identity, defined by attributes |
| **Command Slice** | Feature that modifies state (has `*Command.py`) |
| **Query Slice** | Feature that reads state (has `projection.py`) |

---

## Quick Validation Checklist

```
✅ Bounded context has aggregate_*/ folder(s)
✅ Each aggregate_*/ has exactly ONE *Aggregate.py
✅ Aggregate file has FULL name (not generic aggregate.py)
✅ Entities/VOs are in same folder as their aggregate root
✅ Projections live in owning context's slices/
✅ No imports across bounded context boundaries
✅ Domain has no outward dependencies (pure business logic)
✅ Commands in domain/commands/
✅ Events in domain/events/
```

---

## DDD Hierarchy

```
BOUNDED CONTEXT (semantic boundary)
│
├── AGGREGATE 1 (consistency boundary)
│   ├── AGGREGATE ROOT (entry point)
│   ├── ENTITIES (have identity)
│   └── VALUE OBJECTS (immutable)
│
├── AGGREGATE 2 (consistency boundary)
│   └── ...
│
└── SLICES
    ├── COMMAND SLICES (write)
    └── QUERY SLICES (read)
```

---

## CLI Commands

```bash
# Validate structure
vsa validate

# Watch mode
vsa validate --watch

# Generate manifest
vsa manifest --include-domain

# List contexts
vsa list

# Initialize new project
vsa init --language python
```

---

## Validation Error Examples

```
❌ "Not a valid bounded context"
   → Add aggregate_*/ folder with *Aggregate.py

❌ "Only one root per folder"
   → Remove extra *Aggregate.py files

❌ "Aggregate not in aggregate_* folder"
   → Move to domain/aggregate_<name>/<Name>Aggregate.py

❌ "Boundary violation"
   → Remove cross-context import, use events instead

❌ "Domain has external dependencies"
   → Move infrastructure code out of domain/
```

---

## Entity vs Value Object

```python
# ENTITY - has identity
@dataclass
class IsolationHandle:
    isolation_id: str       # ← Identity field
    isolation_type: str
    proxy_url: str | None

# VALUE OBJECT - immutable, no identity
@dataclass(frozen=True)     # ← Immutable!
class SecurityPolicy:
    memory_limit_mb: int
    cpu_limit: float
    network_enabled: bool
```

---

## References

- [ADR-020: Bounded Context & Aggregate Convention](../../docs/adrs/ADR-020-bounded-context-aggregate-convention.md)
- [ADR-019: VSA Standard Structure](../../docs/adrs/ADR-019-vsa-standard-structure.md)
- [Full README](../README.md)
