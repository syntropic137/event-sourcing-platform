# ADR-014: Projection Checkpoint Architecture

**Status:** ✅ Accepted
**Date:** 2025-12-10
**Decision Makers:** Architecture Team
**Related:** ADR-009 (CQRS Pattern), Event Sourcing, Projections

## Context

### The Problem

Projections in event sourcing systems consume events from an event store and build read models. A critical requirement is tracking **which events have been processed** so that:

1. **Resumption** - After restart, projections resume from where they left off
2. **At-least-once delivery** - No events are skipped
3. **Independent rebuilds** - Projections can be rebuilt without affecting others

### Current Issues (December 2025)

During integration of the Event Sourcing Platform with AEF (Agentic Engineering Framework), we discovered several critical bugs:

#### Issue 1: No Per-Projection Position Tracking

The current `ProjectionManager.dispatch()` has no checkpoint mechanism:

```python
# Current: No checkpointing!
async def dispatch(self, envelope: EventEnvelope[Any]) -> None:
    for projection in self.projections.values():
        await projection.handle_event(envelope)
```

**Impact:** If the system restarts, there's no way to know which events were already processed.

#### Issue 2: Silent Exception Swallowing

```python
# Current: Swallows exceptions with print()!
try:
    await projection.handle_event(envelope)
except Exception as e:
    print(f"Error in projection {projection.get_name()}: {e}")  # ❌
```

**Problems:**
- Uses `print()` instead of structured logging
- Caller has no idea projection failed
- Position continues advancing even on failure
- Events are effectively lost

#### Issue 3: Global Position vs Per-Projection

In the AEF implementation, there was a single global position for all projections:

```python
# Single global position
self._last_position = envelope.metadata.global_nonce
await self._save_position()  # Saved even if some projections failed!
```

**Problems:**
- If one projection is slow, all are blocked
- If one projection fails, position is still saved
- Can't rebuild one projection independently

#### Issue 4: Position Drift

When the PostgreSQL container was restarted:
- Event Store (separate Rust gRPC service) kept all events
- Projection Store (PostgreSQL) lost checkpoint data
- System tried to resume from position 64 but projection data was empty
- Result: Events 0-63 were never re-processed

### Requirements

1. **Per-projection checkpoints** - Each projection tracks its own position
2. **Mandatory checkpoints** - Projections cannot opt out of position tracking
3. **Atomic updates** - Checkpoint advances only when projection succeeds
4. **Error visibility** - Failures are logged and returned, never swallowed
5. **Storage flexibility** - Checkpoints work with any storage backend
6. **Rebuild support** - Projections can be reset to position 0
7. **Scale** - Must support 1000s of concurrent agents/events

## Decision

We implement a **per-projection checkpoint architecture** with:

1. **`CheckpointedProjection`** - Base class that enforces checkpoint tracking
2. **`ProjectionCheckpointStore`** - Protocol for checkpoint persistence
3. **`ProjectionResult`** - Explicit return type for success/failure/skip
4. **`SubscriptionCoordinator`** - Manages subscription and routing

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     Event Store                              │
│  Global Stream: e0, e1, e2, e3, e4, e5, e6, ...             │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                 Subscription Coordinator                      │
│  • Single connection to event store                          │
│  • Reads from: min(all projection checkpoints)               │
│  • Routes by: aggregate_type / event_type                    │
│  • Handles errors: per-projection (don't stop others)        │
└─────────────────────────────────────────────────────────────┘
                              │
         ┌────────────────────┼────────────────────┐
         ▼                    ▼                    ▼
┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐
│ SessionProjection│ │WorkflowProjection│ │AnalyticsProj    │
│ checkpoint: 42  │  │ checkpoint: 40  │  │ checkpoint: 42  │
│ storage: SQL    │  │ storage: SQL    │  │ storage: Vector │
└─────────────────┘  └─────────────────┘  └─────────────────┘
         │                    │                    │
         └────────────────────┴────────────────────┘
                              │
                              ▼
              ┌───────────────────────────┐
              │  Checkpoint Store         │
              │  (always SQL - reliable)  │
              └───────────────────────────┘
```

### Key Design Choices

#### 1. Checkpoints Are Mandatory

```python
class CheckpointedProjection(ABC):
    """Base class with REQUIRED checkpoint tracking."""

    @abstractmethod
    async def handle_event(
        self,
        envelope: EventEnvelope,
        checkpoint_store: ProjectionCheckpointStore,
    ) -> ProjectionResult:
        """Handle event and return result.

        MUST return ProjectionResult - no void returns allowed.
        """
        ...
```

**Rationale:** Optional checkpoints lead to bugs. By making it mandatory, every projection author must consider durability.

#### 2. Checkpoint Store Separate from Projection Data

Even if a projection uses Vector DB or MongoDB, checkpoints are stored in a reliable SQL database:

```
Projection Data:  Vector DB, MongoDB, Redis, etc.
Checkpoints:      PostgreSQL (always)
```

**Rationale:**
- Checkpoints are tiny (projection_name, position, timestamp)
- SQL provides ACID guarantees
- Simplifies recovery - one place to check positions
- External stores may not support atomic transactions

#### 3. Explicit Result Types

```python
class ProjectionResult(Enum):
    SUCCESS = "success"   # Processed, advance checkpoint
    SKIP = "skip"         # Not relevant, advance checkpoint
    FAILURE = "failure"   # Error, DO NOT advance checkpoint
```

**Rationale:**
- Forces projection authors to handle all cases
- No silent failures
- Clear semantics for coordinator

#### 4. Atomic Checkpoint Updates

For SQL-backed projections, checkpoint and data update in same transaction:

```python
async def handle_event(
    self,
    envelope: EventEnvelope,
    checkpoint_store: ProjectionCheckpointStore,
) -> ProjectionResult:
    try:
        async with self.db.transaction():
            # Update projection data
            await self._update_read_model(envelope)

            # Update checkpoint (same transaction!)
            await checkpoint_store.save_checkpoint(
                ProjectionCheckpoint(
                    projection_name=self.get_name(),
                    global_position=envelope.metadata.global_nonce,
                    updated_at=datetime.now(UTC),
                )
            )
        return ProjectionResult.SUCCESS

    except Exception as e:
        logger.error("Projection failed", exc_info=True)
        return ProjectionResult.FAILURE
```

For external storage (Vector DB), checkpoint is saved AFTER successful update:

```python
async def handle_event(...) -> ProjectionResult:
    try:
        # 1. Update external store
        await self.vector_db.upsert(...)

        # 2. Only then save checkpoint
        await checkpoint_store.save_checkpoint(...)

        return ProjectionResult.SUCCESS
    except Exception:
        # Checkpoint NOT saved - will retry on restart
        return ProjectionResult.FAILURE
```

#### 5. Event Type Filtering

Projections declare which events they care about:

```python
class SessionListProjection(CheckpointedProjection):
    def get_subscribed_event_types(self) -> set[str]:
        return {"SessionStarted", "SessionCompleted", "OperationRecorded"}
```

The coordinator only dispatches relevant events:

```python
# Coordinator logic
for projection in projections:
    event_type = envelope.event.event_type
    subscribed = projection.get_subscribed_event_types()

    if subscribed is None or event_type in subscribed:
        result = await projection.handle_event(envelope, checkpoint_store)
    else:
        # Event not relevant, but still advance checkpoint
        await checkpoint_store.advance(projection.get_name(), position)
```

**Optimization:** At scale with 1000s of events/second, filtering reduces processing overhead.

#### 6. Rebuild Strategy

```python
async def rebuild_projection(self, projection_name: str) -> None:
    """Rebuild a projection from scratch."""
    projection = self.get_projection(projection_name)

    # 1. Delete checkpoint
    await self.checkpoint_store.delete_checkpoint(projection_name)

    # 2. Delete projection data
    await projection.clear_all_data()

    # 3. Re-process all events from position 0
    async for envelope in self.event_store.subscribe(from_global_nonce=0):
        await projection.handle_event(envelope, self.checkpoint_store)
```

**Use Cases:**
- Projection schema changed (new version)
- Projection data corrupted
- Bug fix requires reprocessing

## Implementation

### Phase 1: Core Abstractions

```python
# event_sourcing/core/checkpoint.py

from dataclasses import dataclass
from datetime import datetime
from enum import Enum
from typing import Protocol, runtime_checkable

@dataclass(frozen=True)
class ProjectionCheckpoint:
    """Immutable checkpoint for a projection."""
    projection_name: str
    global_position: int
    updated_at: datetime
    version: int = 1


class ProjectionResult(Enum):
    """Result of handling an event."""
    SUCCESS = "success"
    SKIP = "skip"
    FAILURE = "failure"


@runtime_checkable
class ProjectionCheckpointStore(Protocol):
    """Protocol for checkpoint persistence."""

    async def get_checkpoint(
        self, projection_name: str
    ) -> ProjectionCheckpoint | None:
        ...

    async def save_checkpoint(
        self, checkpoint: ProjectionCheckpoint
    ) -> None:
        ...

    async def delete_checkpoint(
        self, projection_name: str
    ) -> None:
        ...
```

### Phase 2: Updated Projection Base Class

```python
# event_sourcing/core/projection.py

from abc import ABC, abstractmethod
from event_sourcing.core.checkpoint import (
    ProjectionCheckpoint,
    ProjectionCheckpointStore,
    ProjectionResult,
)
from event_sourcing.core.event import EventEnvelope
import logging

logger = logging.getLogger(__name__)


class CheckpointedProjection(ABC):
    """Base class for projections with mandatory checkpoint tracking."""

    @abstractmethod
    def get_name(self) -> str:
        """Unique projection name for checkpoint tracking."""
        ...

    @abstractmethod
    def get_version(self) -> int:
        """Schema version - increment to trigger rebuild."""
        ...

    @abstractmethod
    def get_subscribed_event_types(self) -> set[str] | None:
        """Event types to receive, or None for all."""
        ...

    @abstractmethod
    async def handle_event(
        self,
        envelope: EventEnvelope,
        checkpoint_store: ProjectionCheckpointStore,
    ) -> ProjectionResult:
        """Handle event and return explicit result."""
        ...

    async def clear_all_data(self) -> None:
        """Clear projection data for rebuild.

        The default implementation does nothing. Override this method if your
        projection has persistent storage that must be cleared for a rebuild.
        """
        pass
```

### Phase 3: Subscription Coordinator

```python
# event_sourcing/subscriptions/coordinator.py

class SubscriptionCoordinator:
    """Manages event subscription across multiple projections."""

    def __init__(
        self,
        event_store: EventStoreClient,
        checkpoint_store: ProjectionCheckpointStore,
        projections: list[CheckpointedProjection],
    ) -> None:
        self.event_store = event_store
        self.checkpoint_store = checkpoint_store
        self.projections = {p.get_name(): p for p in projections}
        self.logger = logging.getLogger(__name__)

    async def start(self) -> None:
        """Start subscription from minimum checkpoint."""
        # Load all checkpoints
        min_position = await self._get_minimum_position()

        self.logger.info(
            "Starting subscription coordinator",
            extra={
                "from_position": min_position,
                "projection_count": len(self.projections),
            }
        )

        # Subscribe from minimum position
        async for envelope in self.event_store.subscribe(
            from_global_nonce=min_position
        ):
            await self._dispatch_event(envelope)

    async def _get_minimum_position(self) -> int:
        """Get minimum checkpoint position across all projections."""
        min_pos = 0

        for name in self.projections:
            checkpoint = await self.checkpoint_store.get_checkpoint(name)
            if checkpoint is None:
                return 0  # At least one projection needs full replay
            min_pos = min(min_pos, checkpoint.global_position)

        return min_pos + 1  # Start from next position

    async def _dispatch_event(self, envelope: EventEnvelope) -> None:
        """Dispatch event to relevant projections."""
        event_type = envelope.event.event_type
        position = envelope.metadata.global_nonce

        for name, projection in self.projections.items():
            subscribed = projection.get_subscribed_event_types()

            # Check if projection cares about this event type
            if subscribed is not None and event_type not in subscribed:
                # Still advance checkpoint (event not relevant)
                await self._advance_checkpoint(name, position)
                continue

            # Check if projection is already past this position
            checkpoint = await self.checkpoint_store.get_checkpoint(name)
            if checkpoint and checkpoint.global_position >= position:
                continue  # Already processed

            # Dispatch to projection
            try:
                result = await projection.handle_event(
                    envelope, self.checkpoint_store
                )

                if result == ProjectionResult.FAILURE:
                    self.logger.error(
                        "Projection failed to handle event",
                        extra={
                            "projection": name,
                            "event_type": event_type,
                            "position": position,
                        }
                    )
                    # DO NOT advance checkpoint - will retry on restart
                else:
                    # SUCCESS or SKIP - checkpoint should be saved by handler
                    pass

            except Exception as e:
                self.logger.error(
                    "Unexpected error in projection",
                    extra={
                        "projection": name,
                        "event_type": event_type,
                        "position": position,
                        "error": str(e),
                    },
                    exc_info=True,
                )
                # DO NOT advance checkpoint

    async def _advance_checkpoint(self, name: str, position: int) -> None:
        """Advance checkpoint for skipped events."""
        await self.checkpoint_store.save_checkpoint(
            ProjectionCheckpoint(
                projection_name=name,
                global_position=position,
                updated_at=datetime.now(UTC),
            )
        )
```

## Consequences

### Positive

1. **Reliability** ✅
   - No more position drift
   - At-least-once delivery guaranteed
   - Clear failure handling

2. **Observability** ✅
   - Structured logging (no more `print()`)
   - Per-projection status visible
   - Easy to detect slow/stuck projections

3. **Flexibility** ✅
   - Projections can use any storage
   - Independent rebuild capability
   - Event type filtering for performance

4. **Correctness** ✅
   - Atomic checkpoint updates
   - Explicit result types
   - Mandatory checkpoint tracking

### Negative

1. **Migration Required** ⚠️
   - Existing projections need updates
   - **Mitigation:** Migration guide provided below (Alpha - no backward compatibility needed)

2. **Slight Complexity** ⚠️
   - More concepts to understand
   - **Mitigation:** Clear documentation, examples

3. **Storage Dependency** ⚠️
   - Checkpoint store must be reliable SQL
   - **Mitigation:** Usually already have PostgreSQL

### Neutral

1. **Performance**
   - Checkpoint writes add overhead (~1-2ms per event)
   - Filtering reduces unnecessary processing
   - Net effect: approximately neutral

2. **Database Schema**
   - New `projection_checkpoints` table required
   - Simple migration, no data loss

## Alternatives Considered

### 1. In-Memory Checkpoints Only

**Rejected:** Loses positions on restart.

### 2. Checkpoint in Event Store

**Rejected:** Couples projection state to event store, harder to manage.

### 3. Optional Checkpoints

**Rejected:** Leads to bugs where developers forget to checkpoint.

### 4. Per-Projection Subscriptions

**Rejected:** N subscriptions = N connections, doesn't scale.

## Migration Guide

### Step 1: Update Projection Base Class

```python
# Before
class MyProjection(Projection):
    async def handle_event(self, envelope: EventEnvelope) -> None:
        ...

# After
class MyProjection(CheckpointedProjection):
    def get_name(self) -> str:
        return "my_projection"

    def get_version(self) -> int:
        return 1

    def get_subscribed_event_types(self) -> set[str]:
        return {"EventA", "EventB"}

    async def handle_event(
        self,
        envelope: EventEnvelope,
        checkpoint_store: ProjectionCheckpointStore,
    ) -> ProjectionResult:
        try:
            # ... process event ...
            await checkpoint_store.save_checkpoint(...)
            return ProjectionResult.SUCCESS
        except Exception:
            return ProjectionResult.FAILURE
```

### Step 2: Run Migrations

```bash
# Apply checkpoint table migration
psql -f migrations/20251210_projection_checkpoints.sql
```

### Step 3: Update Subscription Coordinator

Replace custom subscription service with `SubscriptionCoordinator`.

## Related ADRs

- **ADR-009:** CQRS Pattern Implementation (projections overview)
- **ADR-004:** Command Handlers in Aggregates
- **ADR-005:** Hexagonal Architecture

## References

- [Event Sourcing Made Simple](https://www.eventstore.com/)
- [Designing Data-Intensive Applications](https://dataintensive.net/) - Chapter 11
- [Building Event-Driven Microservices](https://www.oreilly.com/library/view/building-event-driven-microservices/9781492057888/)

---

**Last Updated:** 2025-12-10
**Supersedes:** None
**Superseded By:** None
