# Projection Ops

Production-ready infrastructure for running projections reliably.

## Overview

Projections consume events from the event store and build read models. In production, they need:

- **Checkpoints** — Track which events have been processed
- **Failure handling** — Retry transient errors, DLQ permanent failures
- **Coordination** — Manage multiple projections efficiently
- **Monitoring** — Health checks and lag visibility

This module provides all of these.

## Quick Start

```typescript
import {
  CheckpointedProjection,
  SubscriptionCoordinator,
  PostgresCheckpointStore,
  MemoryFailedEventStore,
  RetryPolicy,
  ProjectionResult,
} from '@event-sourcing-platform/typescript/projections';

// 1. Define a projection
class OrderSummaryProjection extends CheckpointedProjection {
  private orders = new Map<string, OrderSummary>();

  getName() { return 'order-summary'; }
  getVersion() { return 1; }
  getSubscribedEventTypes() { return new Set(['OrderCreated', 'OrderShipped']); }

  protected async processEvent(envelope, checkpointStore) {
    const event = envelope.event;
    
    switch (event.eventType) {
      case 'OrderCreated':
        this.orders.set(envelope.metadata.aggregateId, {
          id: envelope.metadata.aggregateId,
          status: 'created',
        });
        break;
      case 'OrderShipped':
        const order = this.orders.get(envelope.metadata.aggregateId);
        if (order) order.status = 'shipped';
        break;
    }

    // Save checkpoint after successful processing
    await this.saveCheckpoint(checkpointStore, envelope.metadata.globalNonce);
    return ProjectionResult.SUCCESS;
  }
}

// 2. Set up the coordinator
const coordinator = new SubscriptionCoordinator({
  eventStore,
  checkpointStore: new PostgresCheckpointStore({ client: pool }),
  projections: [new OrderSummaryProjection()],
  failedEventStore: new MemoryFailedEventStore(),
  defaultRetryPolicy: RetryPolicy.exponentialBackoff({ maxRetries: 3 }),
});

// 3. Start processing
await coordinator.start();

// 4. On shutdown
await coordinator.stop();
```

## Architecture

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
│  • Routes by: event type                                     │
│  • Handles errors: retry + DLQ                               │
└─────────────────────────────────────────────────────────────┘
                              │
         ┌────────────────────┼────────────────────┐
         ▼                    ▼                    ▼
┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐
│ Projection A    │  │ Projection B    │  │ Projection C    │
│ checkpoint: 42  │  │ checkpoint: 40  │  │ checkpoint: 42  │
└─────────────────┘  └─────────────────┘  └─────────────────┘
         │                    │                    │
         └────────────────────┴────────────────────┘
                              │
                              ▼
              ┌───────────────────────────┐
              │    Checkpoint Store       │
              │    (PostgreSQL)           │
              └───────────────────────────┘
```

## Key Concepts

### CheckpointedProjection

Base class for projections with mandatory checkpoint tracking:

```typescript
abstract class CheckpointedProjection {
  abstract getName(): string;
  abstract getVersion(): number;
  abstract getSubscribedEventTypes(): Set<string> | null;
  protected abstract processEvent(envelope, checkpointStore): Promise<ProjectionResult>;
}
```

### ProjectionResult

Return values from `processEvent`:

| Result | Meaning | Checkpoint |
|--------|---------|------------|
| `SUCCESS` | Processed successfully | Advanced |
| `SKIP` | Not relevant to this projection | Advanced |
| `RETRY` | Transient error, retry with backoff | Not advanced |
| `FAILURE` | Permanent error, send to DLQ | Not advanced |

### RetryPolicy

Configurable retry behavior:

```typescript
// Exponential backoff (default)
RetryPolicy.exponentialBackoff({ maxRetries: 3 });

// Fixed delay
RetryPolicy.fixedDelay(1000, 5); // 1 second, 5 retries

// No retries
RetryPolicy.noRetry();
```

## Next Steps

- [Checkpoints](./checkpoints.md) — Checkpoint architecture details
- [Failure Handling](./failure-handling.md) — DLQ and retry patterns
- [Health Monitoring](./health-monitoring.md) — Lag and alerting
- [Rebuilding](./rebuilding.md) — Rebuild projections from scratch
