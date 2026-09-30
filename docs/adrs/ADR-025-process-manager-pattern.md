# ADR-025: ProcessManager Base Class (To-Do List Pattern)

**Status:** Accepted
**Date:** 2026-04-13
**Deciders:** Syntropic137 team

## Context

ESP provides `CheckpointedProjection` as the only base class for event
consumers. This works well for read models (pure state reconstruction
from events), but it does not support event consumers that need to produce
side effects (dispatch workflows, call external APIs, send notifications).

When a system built on ESP needs event-driven side effects, the developer
has two options:

1. Put side effects in a projection's `handle_event()` (wrong - causes
   replay storms because projections run during catch-up replay)
2. Build a bespoke processor with no framework support (error-prone,
   no replay safety guarantee, inconsistent across projects)

Option 1 is the path of least resistance, and it caused a critical bug
in Syntropic137: on service restart, the subscription coordinator
replays historical events through the dispatch projection, which
re-dispatches workflows for already-processed PRs, burning user funds.

## Decision

Add a `ProcessManager` base class to ESP that implements the Processor
To-Do List pattern (Martin Dilger, *Understanding Event Sourcing*, Ch. 37).

The ProcessManager has two clearly separated parts:

- **Projection side** (`handle_event()`) - writes to-do records. Called
  during both replay and live processing. Must be pure (no side effects).
- **Processor side** (`process_pending()`) - executes pending items.
  Called by the coordinator ONLY for live events. Must be idempotent.

The coordinator enforces the boundary: `process_pending()` is never
called while `is_catching_up` is True.

Additionally:
- `DispatchContext` is passed to all `handle_event()` calls with
  `is_catching_up` flag, using `global_nonce` from the event store as
  the durable boundary
- `CheckpointedProjection` gains `SIDE_EFFECTS_ALLOWED = False` (default)
- `ProcessManager` overrides to `SIDE_EFFECTS_ALLOWED = True`
- Built-in fitness functions check projection purity (whitelist-based)
  and ProcessManager structure

## Alternatives Considered

### Saga pattern

Sagas use compensation logic to undo failed steps in a distributed
transaction. This adds significant complexity (compensation handlers,
saga state machines, distributed rollback) that is unnecessary for our
use cases. The To-Do List pattern is simpler: retry pending items on
failure, don't compensate completed ones.

### Replay flag on EventEnvelope

Adding an `is_replay` field to `EventEnvelope` was considered. Rejected
because it pushes the responsibility to each projection instead of
enforcing it at the framework level. A projection could ignore the flag.
The ProcessManager approach enforces the boundary structurally.

### Blacklist-based purity checking

Checking projections against a list of banned modules (httpx, requests,
etc.). Rejected because blacklists are fragile - new side-effecting
libraries slip through. The whitelist approach (projections may only
import from allowed modules) is durable: anything not explicitly
allowed is a violation.

## Consequences

### Positive

- The replay storm bug becomes structurally impossible
- Developers have a correct base class for event-driven side effects
- The coordinator enforces the projection/processor boundary
- Fitness functions catch violations in CI
- Every project built on ESP inherits these guarantees

### Negative

- Existing projects must migrate side-effecting projections to ProcessManager
- The coordinator becomes slightly more complex (ProcessManager detection, process_pending gating)
- Projects must configure the whitelist for their domain modules

### Neutral

- ProcessManager extends CheckpointedProjection, so existing checkpoint infrastructure is reused
- The `context` parameter on `handle_event()` is optional for backwards compatibility

## References

- Martin Dilger, *Understanding Event Sourcing*, Ch. 37
- [CONSUMER-PATTERNS.md](../CONSUMER-PATTERNS.md) - full guide
- [ADR-014](ADR-014-projection-checkpoint-architecture.md) - checkpoint architecture
