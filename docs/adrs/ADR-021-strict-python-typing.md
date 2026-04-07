# ADR-021: Strict Python Typing Strategy

**Status:** Accepted
**Date:** 2026-04-06
**Context:** Event Sourcing Platform Python SDK

## Context

Python's type system is opt-in and permissive by default. The `Any` type silently disables type checking wherever it appears — a single `Any` in a function signature propagates through every caller, silently erasing type safety. For a library consumed by strictly-typed downstream projects, untyped interfaces create cascading tech debt.

The Syntropic137 platform consumes the ESP Python SDK and enforces `ruff ANN401` (ban `Any` in annotations). Approximately 42% of Syntropic137's `noqa: ANN401` suppressions (98 of 231) originate from untyped ESP interfaces. More critically, Syntropic137's type pipeline flows from Python through to TypeScript:

```
Pydantic models (Python) → OpenAPI spec (auto) → TypeScript types (auto) → CLI (tsc --strict)
```

Any `Any` that leaks into a Pydantic model generates `{ [key: string]: unknown }` in the OpenAPI spec, breaking end-to-end type safety. **We need Python to feel like TypeScript in strict mode.**

## Decision

We adopt a multi-layer type safety strategy that mirrors TypeScript's `--strict` flag, using multiple complementary tools since no single Python tool provides equivalent coverage.

### Layer Architecture

| Layer | Tool | What It Catches | TypeScript Equivalent |
|-------|------|-----------------|----------------------|
| **Annotations** | `ruff ANN` + `ANN401` | Explicit `Any`, missing annotations | `noImplicitAny` |
| **Type checking** | `pyright strict` | Implicit `Any`, unsafe ops, missing returns | `tsc --strict` |
| **Import hygiene** | `ruff TC` | Runtime vs type-checking imports | `import type` |
| **API boundary** | Pydantic `frozen=True, extra="forbid"` | Untyped request/response fields | Zod / io-ts |
| **Generated types** | proto stubs + `--pyi_out` | Drift between proto and Python | `openapi-typescript` |
| **`object` ratchet** | `make type-ratchet` | Unjustified `object`, count growth | N/A (TypeScript has no `object` escape) |
| **CI gates** | All above run as blocking CI steps | Regressions | `tsc` in CI |

### Core Principles

1. **`Any` is a bug, not a convenience.** Every `Any` must be replaced with the most specific type the situation allows. See [Edge Cases](#edge-cases-beyond-any) for the decision tree.

2. **`object` is not a free pass either.** While `object` is better than `Any` (it forces narrowing before use), it should only be used when no protocol or concrete type applies. If you find yourself writing `param: object`, ask whether a Protocol exists first.

3. **Protocols over ABCs for interfaces.** Structural typing (duck typing done right) — consumers don't need to inherit from our base classes.

4. **Generic over specific.** `Repository[T]` not `Repository`. `EventEnvelope[DomainEvent]` not `EventEnvelope[Any]`.

5. **`TYPE_CHECKING` for import cycles.** String annotations (via `from __future__ import annotations`) avoid circular imports without resorting to `Any`.

6. **Suppress, don't weaken.** Existing violations get targeted `# type: ignore[rule]` with a comment explaining why. New violations are blocked in CI.

7. **Generated code is excluded.** Proto-generated stubs and the gRPC adapter (which interacts with untyped proto objects) are excluded from strict checking. Their public APIs remain fully typed.

### Why Pyright Over mypy

We replace mypy with pyright as the type checker:

- **Faster** — Written in TypeScript/Node, significantly faster than mypy (written in Python)
- **Better Protocol conformance** — Superior checking of Protocol structural subtyping
- **Better generic inference** — More accurate inference of complex generic types
- **Catches more implicit `Any`** — Even mypy strict misses cases that pyright strict catches (e.g., untyped third-party returns)
- **IDE integration** — Powers Pylance in VS Code, providing consistent IDE and CI behavior
- **Active development** — Microsoft maintains pyright with frequent releases

### Configuration

**Pyright** (`pyproject.toml`):
```toml
[tool.pyright]
pythonVersion = "3.11"
typeCheckingMode = "strict"
include = ["src"]
exclude = ["src/event_sourcing/proto", "src/event_sourcing/client/grpc_client.py"]
```

The gRPC client adapter is excluded because proto-generated stubs lack type information. Its public API (return types) is fully typed — the "unknown" types are confined to internal proto interactions.

**Ruff** (`pyproject.toml`):
```toml
[tool.ruff.lint]
select = [
    "E", "W", "F", "I", "B", "C4", "UP",
    "ANN",  # flake8-annotations — ANN401 bans Any
    "TC",   # flake8-type-checking — TYPE_CHECKING import hygiene
]
```

## Edge Cases: Beyond `Any`

Banning `Any` is step one. The harder question is: **what replaces it?** The naive answer — `object` everywhere — passes the linter but doesn't help callers. This section documents the decision tree for every category of `Any` we encountered and the principled replacement.

### Decision Tree

```
Do you have a concrete type?
  YES → Use it (str, int, EventEnvelope[DomainEvent], etc.)
  NO  ↓

Does a Protocol exist (or should one)?
  YES → Use the Protocol (Command, EventStoreClient, AsyncConnectionPool)
  NO  ↓

Is the value a known structure?
  YES → Use TypedDict or dataclass
  NO  ↓

Is the value constrained to a finite union?
  YES → Use the union (str | int | list[str])
  NO  ↓

Is this a decorator/metaprogramming boundary?
  YES → Use Callable[..., object] + reportFunctionMemberAccess=warning
  NO  ↓

Last resort: object (with a comment explaining why)
```

### Category 1: Protocol Interfaces

**Problem:** Circular imports force `param: Any` as a workaround.
**Anti-pattern:** `event_store_client: Any  # EventStoreClient`
**Solution:** `TYPE_CHECKING` guard + string annotations.

```python
from __future__ import annotations
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from event_sourcing.client.event_store import EventStoreClient

class EventStoreRepository:
    def __init__(self, event_store_client: EventStoreClient) -> None: ...
```

This gives full type safety at check time while avoiding circular imports at runtime. Pyright and ruff both understand `TYPE_CHECKING` guards.

**Applied to:** `EventStoreClient`, `BaseAggregate`, `Callable`, `AsyncIterator`

### Category 2: Infrastructure Protocols (pool, conn)

**Problem:** Third-party types (asyncpg) aren't always available, and concrete types create hard dependencies.
**Anti-pattern:** `pool: Any`, `conn: Any`
**Solution:** Define structural Protocols that match the interface we actually use.

```python
class AsyncConnection(Protocol):
    async def execute(self, query: str, *args: object) -> str: ...
    async def fetchrow(self, query: str, *args: object) -> _Row | None: ...
    async def fetch(self, query: str, *args: object) -> list[_Row]: ...

class AsyncConnectionPool(Protocol):
    @asynccontextmanager
    def acquire(self) -> AsyncIterator[AsyncConnection]: ...
```

Any pool/connection that quacks like asyncpg will satisfy the protocol. No import of asyncpg required.

### Category 3: Polymorphic Dispatch (commands)

**Problem:** Command handlers accept different command types, tempting `command: Any`.
**Anti-pattern:** `def _handle_command(self, command: Any) -> None`
**Solution:** Use the existing `Command` protocol. Commands in this system always have `aggregate_id`.

```python
from event_sourcing.core.command import Command

def _handle_command(self, command: Command) -> None: ...
```

The `Command` protocol defines the structural contract. Callers know exactly what shape to pass. This is the TypeScript equivalent of `interface Command { aggregateId: string }` — specific enough to be useful, generic enough to accept any command implementation.

**Why not `object`?** `object` tells the caller nothing. `Command` tells them "pass something with `aggregate_id: str`." That's the difference between `unknown` and a typed interface in TypeScript.

### Category 4: Known Structures (status dicts, error details)

**Problem:** Methods return `dict[str, Any]` for structured data with a known shape.
**Anti-pattern:** `def get_status() -> dict[str, Any]`
**Solution:** Use `TypedDict` for known structures.

```python
class ProjectionStatus(TypedDict):
    name: str
    version: int
    subscribed_types: list[str] | None
    checkpoint: CheckpointStatus

class CheckpointStatus(TypedDict):
    position: int | None
    updated_at: str | None
    version: int | None
```

This is the Python equivalent of TypeScript's `interface ProjectionStatus { name: string; version: number; ... }`. The caller knows exactly what keys exist and what types they are.

**Applied to:** `SubscriptionCoordinator.get_projection_status()`, error `details` dicts

### Category 5: Error Details

**Problem:** Base error class has `details: dict[str, Any]` for diagnostic data.
**Anti-pattern:** `dict[str, Any]` when the actual values are always `str | int | list[str]`
**Solution:** Constrain the union to what's actually stored.

```python
# Covers all actual error detail values in the codebase
ErrorDetails = dict[str, str | int | list[str]]
```

Each error subclass (`AggregateNotFoundError`, `ConcurrencyConflictError`, etc.) already has typed instance attributes. The `details` dict is a convenience for logging/serialization — its values are always strings, ints, or string lists.

**Enforcement:** Pyright's strict mode catches dict invariance issues — passing `dict[str, str]` where `dict[str, str | int | list[str]]` is expected requires explicit annotation, which keeps the types visible.

### Category 6: Event Metadata (custom_metadata)

**Problem:** `EventMetadata.custom_metadata` carries arbitrary user-defined key-value pairs.
**Anti-pattern:** `dict[str, Any]` — anything goes
**Solution:** `dict[str, str]` — metadata values are always strings in practice.

```python
custom_metadata: dict[str, str] = Field(default_factory=dict)
```

Event metadata travels through gRPC (protobuf `map<string, string>`), through JSON serialization, and through the OpenAPI pipeline. At every stage, values are strings. If a caller needs structured data in metadata, they should serialize to a string (JSON) explicitly — just like HTTP headers.

**Why not keep `dict[str, object]`?** `object` is the top type — it tells you nothing about what's inside. It's `Any` with extra steps: you've passed the linter but the caller still doesn't know what to put in. `dict[str, str]` is honest about what the system actually handles.

### Category 7: Decorator Metaprogramming

**Problem:** Decorators dynamically attach attributes to functions/classes. Pyright can't verify `func._event_type` exists.
**Anti-pattern:** `F = TypeVar("F", bound=Callable[..., Any])`
**Solution:** Use `Callable[..., object]` and set `reportFunctionMemberAccess = "warning"`.

```python
F = TypeVar("F", bound=Callable[..., object])

def event_sourcing_handler(event_type: str) -> Callable[[F], F]:
    def decorator(func: F) -> F:
        func._event_type = event_type  # type: ignore[attr-defined]
        return func
    return decorator
```

Dynamic attribute attachment is inherently untyped in Python — no type checker can verify it. We accept `# type: ignore[attr-defined]` on the decorator definition (write side) and set `reportFunctionMemberAccess` to warning (read side). The alternative would be a descriptor protocol, which adds complexity without safety since the attribute is always set by the decorator.

### Category 8: Generic Type Erasure

**Problem:** `EventEnvelope[Any]` when the concrete event type is erased.
**Anti-pattern:** Using `Any` as a generic parameter when the base type is known.
**Solution:** Use the base type: `EventEnvelope[DomainEvent]`.

```python
# Before: event type erased
async def dispatch(self, envelope: EventEnvelope[Any]) -> None: ...

# After: bounded by the base type
async def dispatch(self, envelope: EventEnvelope[DomainEvent]) -> None: ...
```

`DomainEvent` is the base of all events — `EventEnvelope[DomainEvent]` accepts any event subtype through covariance. This is equivalent to TypeScript's `EventEnvelope<DomainEvent>` rather than `EventEnvelope<any>`.

## Type Patterns Summary

| Pattern | Before | After | Why |
|---------|--------|-------|-----|
| Client param (circular import) | `client: Any` | `client: EventStoreClient` via TYPE_CHECKING | Protocol exists, use it |
| DB pool/connection | `pool: Any` | `pool: AsyncConnectionPool` (Protocol) | Define the interface we use |
| Command dispatch | `command: Any` | `command: Command` (Protocol) | Protocol exists, use it |
| Event envelope generic | `EventEnvelope[Any]` | `EventEnvelope[DomainEvent]` | Base type, not erased type |
| Projection status | `dict[str, Any]` | `ProjectionStatus` (TypedDict) | Known structure |
| Error details | `dict[str, Any]` | `dict[str, str \| int \| list[str]]` | Finite value union |
| Event custom metadata | `dict[str, Any]` | `dict[str, str]` | Always strings (proto/JSON) |
| Decorator TypeVars | `Callable[..., Any]` | `Callable[..., object]` | Top type + warning for dynamic attrs |
| Query bus return | `-> Any` | `-> object` | Top type (callers must narrow) |
| TypeVar bounds | `bound=AggregateRoot[Any]` | `bound="AggregateRoot[DomainEvent]"` | Base type, not erased |

## What the Tools Catch

| Mistake | Caught By | How |
|---------|-----------|-----|
| Writing `param: Any` | **ruff ANN401** | Lint error, blocks CI |
| Implicit `Any` from untyped library | **pyright strict** | `reportUnknownMemberType` |
| Missing return type | **ruff ANN201** + **pyright** | Both catch it |
| `Any` leaking through generics | **pyright strict** | `reportUnknownVariableType` |
| Runtime import used only for types | **ruff TC001/TC003** | Move to `TYPE_CHECKING` block |
| `dict[str, str]` passed as `dict[str, str \| int]` | **pyright strict** | Dict invariance check |
| Missing type narrowing on `object` | **pyright strict** | Forces `isinstance` / `cast` before use |
| New unjustified `object` annotation | **`make type-ratchet`** | Requires `# OBJRATCHET:` comment; count capped |
| `object` count increasing | **`make type-ratchet`** | `OBJECT_RATCHET_MAX` enforced in CI |
| `object` used where Protocol fits | **Code review** + **ratchet** | Reviewer asks "is there a Protocol?"; ratchet prevents silent growth |

### The `object` Ratchet

No linter catches the `object` cop-out automatically — you can pass the linter by replacing `Any` with `object` without adding any real type safety. To prevent this from becoming the new escape hatch, we enforce a **ratchet**:

1. **Every `object` type annotation in `src/` must have a `# OBJRATCHET:` justification comment.** The comment explains why `object` is the correct choice (not just the easy one). Any unjustified `object` annotation fails CI.

2. **The total count of `OBJRATCHET` annotations is capped.** The current maximum is set in `Makefile` as `OBJECT_RATCHET_MAX`. New `object` usages beyond this limit fail CI — you must either replace an existing `object` with a concrete type first, or justify why the cap should increase.

3. **The ratchet only goes down.** When you replace an `object` with a Protocol or concrete type, remove the `OBJRATCHET` comment. Periodically lower the max to match the actual count.

This runs as `make type-ratchet` and is included in both `make qa` and `make qa-fast`.

**Current `OBJRATCHET` inventory (15):**

| Location | Justification |
|----------|---------------|
| `aggregate.py` — `_get_event_handlers() -> dict[str, Callable[..., object]]` | Event handlers mutate state; return type is unused |
| `query.py` — `QueryBus.send() -> object` | Return type varies per query; callers downcast |
| `query.py` — `handler: QueryHandler[object]` | Handler result type erased at bus level |
| `commands.py` — `F = TypeVar("F", bound=Callable[..., object])` | Decorator preserves any callable signature |
| `commands.py` — `get_command_metadata(command_class: type[object])` | Accepts any class for metadata inspection |
| `events.py` — `F = TypeVar("F", bound=Callable[..., object])` | Decorator preserves any callable signature |
| `events.py` — `get_event_metadata(event_class: type[object])` | Accepts any class for metadata inspection |
| `postgres_checkpoint.py` — `_Row.__getitem__() -> object` | DB row values are heterogeneous |
| `postgres_checkpoint.py` — `AsyncConnection.execute(*args: object)` | SQL params are heterogeneous |
| `postgres_checkpoint.py` — `AsyncConnection.fetchrow(*args: object)` | SQL params are heterogeneous |
| `postgres_checkpoint.py` — `AsyncConnection.fetch(*args: object)` | SQL params are heterogeneous |
| `aggregate_scenario.py` — `_injectables: dict[str, object]` | DI container holds arbitrary service types |
| `aggregate_scenario.py` — `register_injectable_resource(resource: object)` | DI accepts any service type |
| `test_executor.py` — `injectables: dict[str, object]` | DI container holds arbitrary service types |

## Consequences

### Positive

- All ESP Python SDK packages run pyright strict + ruff ANN401 in CI (blocking)
- New `Any` annotations are blocked — must use concrete types or documented exceptions
- Downstream consumers (Syntropic137) get fully typed interfaces, eliminating ~98 `noqa: ANN401` suppressions
- Pydantic → OpenAPI → TypeScript pipeline maintains end-to-end type safety
- IDE experience is consistent with CI (pyright powers both)
- Edge cases are documented with principled replacements, not just "use object"

### Negative

- Proto-generated code and gRPC adapter require exclusion from strict mode (standard practice)
- Decorator patterns that dynamically set attributes generate pyright warnings (acceptable, set to warning level)
- Some Python patterns (dynamic dispatch, `setattr` in decorators) require `getattr`-based workarounds to satisfy the type checker
- `dict[str, str]` for metadata is more restrictive than the previous `dict[str, Any]` — callers who want structured values must serialize to string

### Neutral

- mypy is removed — teams using mypy in other projects can continue to do so, but this SDK uses pyright
- `from __future__ import annotations` is added to most files (standard Python 3.11+ practice)
- `QueryBus.send() -> object` is the one place where `object` is correct — the bus doesn't know the return type, and callers must narrow based on the query they sent (same as TypeScript's `unknown`)
