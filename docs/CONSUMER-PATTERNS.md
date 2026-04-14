# Event Consumer Patterns

This guide defines the two types of event consumers in ESP and the rules
for each. Every event consumer must be exactly one of these types.

## Projection (Read Model)

A projection builds a derived view from events. It is **read-only** and
**replay-safe**: running the entire event store through a projection
1000 times must produce the same result with zero external calls.

**Base class:** `CheckpointedProjection` (or `AutoDispatchProjection`)

**Rules:**
- No HTTP clients, no message publishing, no container launches
- No imports from infrastructure or external service modules
- `SIDE_EFFECTS_ALLOWED = False` (default)
- Must implement: `get_name()`, `get_version()`, `get_subscribed_event_types()`, `handle_event()`
- Returns `ProjectionResult` (SUCCESS / SKIP / FAILURE)
- Checkpoint is saved atomically with projection data update

**When to use:** Building read models, dashboards, query views, search
indexes, or any derived state that can be rebuilt from events.

**Test:** Replay the entire event store from position 0. Assert zero
side effects, zero external calls, zero network traffic.

---

## ProcessManager (To-Do List Pattern)

A ProcessManager reacts to events with side effects. It uses the
Processor To-Do List pattern (Martin Dilger, *Understanding Event
Sourcing*, Ch. 37) which splits the work into two parts with a hard
boundary between them.

**Base class:** `ProcessManager`

**Two sides:**

| Side | Method | When called | Side effects? |
|------|--------|-------------|---------------|
| Projection | `handle_event()` | Always (replay + live) | Never |
| Processor | `process_pending()` | Live only (never during replay) | Yes (idempotent) |

**Rules:**
- `handle_event()` writes to-do records only (pure, same rules as Projection)
- `process_pending()` executes pending items (idempotent, live-only)
- `get_idempotency_key()` returns a stable dedup key per to-do item
- `SIDE_EFFECTS_ALLOWED = True`
- The coordinator enforces the boundary: `process_pending()` is never
  called while `is_catching_up` is True

**When to use:** Dispatching workflows, sending notifications, calling
external APIs, or any action that should happen once per event and
survive restarts.

**Flow:**
```
Event arrives
  -> handle_event() writes to-do record (always, replay-safe)
  -> if live: process_pending() reads and executes pending items
  -> process_pending() marks items as done
  -> on crash: restart, re-read pending items, resume
```

**Test:** Replay 100 events in catch-up mode. Assert `process_pending()`
called 0 times. Send 1 live event. Assert `process_pending()` called
1 time.

---

## Decision Table

| Scenario | Pattern | Example |
|----------|---------|---------|
| Build a read model | Projection | Order summary, dashboard metrics |
| Build a search index | Projection | Full-text search, analytics view |
| Dispatch a workflow on event | ProcessManager | Trigger-fired workflow dispatch |
| Send notification on event | ProcessManager | Email, Slack, webhook notification |
| Call external API on event | ProcessManager | GitHub status update, billing |
| Multi-step orchestration | ProcessManager | Phase-by-phase workflow execution |

---

## Anti-Patterns

### Projection with side effects

A projection that dispatches commands, calls external APIs, or creates
infrastructure during `handle_event()`. This is the single most
dangerous pattern because projections run during replay.

**Why it happens:** The framework only has `CheckpointedProjection` as
a base class, so developers put side effects there.

**Fix:** Use `ProcessManager` instead. The projection side writes
to-do records. The processor side executes them.

### Imperative orchestration

An async function that awaits multiple steps in sequence:
```python
# WRONG
async def execute(workflow):
    for phase in workflow.phases:
        workspace = await provision(phase)
        result = await run_agent(workspace)
```

**Fix:** Use the ProcessManager pattern with a to-do list projection.
The aggregate decides "what's next" via events. The processor executes
the next step.

### In-memory dedup for correctness

Using `asyncio.Lock` or in-memory sets to prevent duplicate work.
Lost on restart, doesn't work across instances.

**Fix:** Use durable dedup (Postgres-backed) via `get_idempotency_key()`
and `ExpectedVersion.NoStream`.

---

## DispatchContext

The coordinator passes a `DispatchContext` to every `handle_event()` call:

```python
@dataclass(frozen=True)
class DispatchContext:
    is_catching_up: bool       # True during catch-up, False for live
    global_nonce: int          # Position of current event
    live_boundary_nonce: int   # Head position at subscribe time
```

- `is_catching_up = True`: historical event, replay mode
- `is_catching_up = False`: live event, just appended to the store
- The boundary is determined by snapshotting the head `global_nonce`
  from the event store before subscribing

Projections generally ignore the context (they are pure either way).
ProcessManagers may use it for logging or metrics.

---

## Testing

### Projection tests

Use the existing `AutoDispatchProjection` pattern with
`MemoryCheckpointStore`:

```python
async def test_order_summary():
    projection = OrderSummaryProjection(store=memory_store)
    checkpoint_store = MemoryCheckpointStore()
    
    result = await projection.handle_event(envelope, checkpoint_store)
    assert result == ProjectionResult.SUCCESS
    assert await memory_store.get("order-1") == expected_summary
```

### ProcessManager tests

Use `ProcessManagerScenario`:

```python
from event_sourcing.testing import ProcessManagerScenario

async def test_dispatch_manager_replay():
    manager = WorkflowDispatchManager(store=mock_store)
    scenario = ProcessManagerScenario(manager)
    
    await scenario.given_events([trigger_fired_event])
    
    # Projection side ran, processor side did NOT
    assert scenario.process_pending_call_count == 0

async def test_dispatch_manager_live():
    manager = WorkflowDispatchManager(store=mock_store)
    scenario = ProcessManagerScenario(manager)
    
    result = await scenario.when_live_event(trigger_fired_event)
    
    # Both sides ran
    assert result == ProjectionResult.SUCCESS
```

### Idempotency tests

Use `IdempotencyVerifier`:

```python
from event_sourcing.testing import IdempotencyVerifier

async def test_dispatch_is_idempotent():
    manager = WorkflowDispatchManager(store=mock_store)
    # Set up some pending items first...
    
    verifier = IdempotencyVerifier(manager)
    result = await verifier.verify(expected_first_pass=3)
    
    assert result.is_idempotent  # second pass processes 0 items
```

---

## References

- Martin Dilger, *Understanding Event Sourcing*, Ch. 37: Processor To-Do List
- Event Modeling: https://eventmodeling.org/posts/what-is-event-modeling/
- To-Do List + Passage of Time: https://event-driven.io/en/to_do_list_and_passage_of_time_patterns_combined/
- ADR-025: ProcessManager Pattern (this repo)
- ADR-014: Projection Checkpoint Architecture (this repo)
