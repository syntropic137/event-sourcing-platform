# ADR-016: Projection Failure Handling

**Status:** 📋 Proposed  
**Date:** 2025-12-19  
**Decision Makers:** Platform Team  
**Related:** ADR-014 (Projection Checkpoints), ADR-009 (CQRS Pattern)

## Context

### The Problem

Projections consume events from the event store and build read models. In production, projections can fail due to:

1. **Transient errors** — Network issues, database timeouts, resource exhaustion
2. **Permanent errors** — Bugs in projection code, malformed events, invariant violations
3. **Schema mismatches** — Events that don't match expected structure

Currently, ADR-014 defines checkpointing but doesn't address:
- What happens when a projection fails to process an event?
- How are transient vs permanent errors distinguished?
- How are failed events retried or dead-lettered?
- How is failure visibility achieved?

### Requirements

1. **Retry transient errors** — Network issues should be retried with backoff
2. **Dead-letter permanent errors** — Unrecoverable errors should not block the stream
3. **Per-projection handling** — One projection's failure shouldn't affect others
4. **Visibility** — Ops must know when projections are failing
5. **Reprocessing** — Failed events should be replayable after fixes
6. **Ordering preservation** — Within a projection, event order must be maintained

## Decision

We implement a **failure handling system** with:

### 1. Projection Result Types

Projections return explicit results:

```typescript
enum ProjectionResult {
  SUCCESS = 'success',     // Processed, advance checkpoint
  SKIP = 'skip',           // Not relevant, advance checkpoint
  RETRY = 'retry',         // Transient error, retry with backoff
  FAILURE = 'failure',     // Permanent error, send to DLQ
}
```

### 2. Retry Policy

Configurable retry with exponential backoff:

```typescript
interface RetryPolicy {
  maxRetries: number;           // Max attempts before DLQ
  initialDelayMs: number;       // First retry delay
  maxDelayMs: number;           // Cap on delay
  backoffMultiplier: number;    // Delay multiplier per retry
  retryableErrors?: string[];   // Error types to retry (optional filter)
}

// Default policy
const defaultRetryPolicy: RetryPolicy = {
  maxRetries: 3,
  initialDelayMs: 100,
  maxDelayMs: 30000,
  backoffMultiplier: 2,
};

// Delay calculation: min(initialDelayMs * (backoffMultiplier ^ attempt), maxDelayMs)
// Attempt 1: 100ms
// Attempt 2: 200ms
// Attempt 3: 400ms (then DLQ)
```

### 3. Dead Letter Queue (DLQ)

Failed events go to a DLQ for later analysis/reprocessing:

```typescript
interface FailedEvent {
  id: string;                    // Unique ID for this failure record
  projectionName: string;        // Which projection failed
  eventId: string;               // The failed event's ID
  eventType: string;             // The failed event's type
  globalNonce: number;           // Position in global stream
  aggregateId: string;           // The aggregate that produced the event
  aggregateType: string;         // Type of aggregate
  payload: string;               // Serialized event data
  errorMessage: string;          // Error message
  errorStack?: string;           // Stack trace
  attemptCount: number;          // How many times we tried
  firstFailedAt: Date;           // When it first failed
  lastFailedAt: Date;            // Most recent failure
  status: 'pending' | 'reprocessing' | 'resolved' | 'ignored';
}

interface FailedEventStore {
  save(event: FailedEvent): Promise<void>;
  getByProjection(projectionName: string): Promise<FailedEvent[]>;
  getByStatus(status: string): Promise<FailedEvent[]>;
  markReprocessing(id: string): Promise<void>;
  markResolved(id: string): Promise<void>;
  markIgnored(id: string, reason: string): Promise<void>;
  reprocessEvent(id: string): Promise<ProjectionResult>;
}
```

### 4. Error Handler Configuration

Each projection can have custom error handling:

```typescript
interface ProjectionErrorHandler {
  retryPolicy: RetryPolicy;
  deadLetterStore: FailedEventStore;
  onRetry?: (event: EventEnvelope, attempt: number, error: Error) => void;
  onDLQ?: (event: EventEnvelope, error: Error) => void;
  onSuccess?: (event: EventEnvelope) => void;
  isRetryable?: (error: Error) => boolean;
}

// Usage in coordinator
const coordinator = new SubscriptionCoordinator({
  projections: [orderSummary, inventoryLevels],
  defaultErrorHandler: {
    retryPolicy: RetryPolicy.exponentialBackoff({ maxRetries: 3 }),
    deadLetterStore: postgresFailedStore,
    onDLQ: (event, error) => {
      metrics.increment('projection.dlq', { projection: event.projectionName });
      alerting.notify('Projection failure', { event, error });
    },
  },
  // Per-projection overrides
  errorHandlerOverrides: {
    'critical-projection': {
      retryPolicy: RetryPolicy.exponentialBackoff({ maxRetries: 10 }),
      onDLQ: (event, error) => pagerDuty.alert('Critical projection failure'),
    },
  },
});
```

### 5. Subscription Coordinator with Failure Handling

```typescript
class SubscriptionCoordinator {
  async processEvent(
    envelope: EventEnvelope,
    projection: CheckpointedProjection,
  ): Promise<void> {
    const handler = this.getErrorHandler(projection.getName());
    let attempt = 0;
    
    while (attempt <= handler.retryPolicy.maxRetries) {
      try {
        const result = await projection.handleEvent(envelope, this.checkpointStore);
        
        switch (result) {
          case ProjectionResult.SUCCESS:
          case ProjectionResult.SKIP:
            handler.onSuccess?.(envelope);
            return;
            
          case ProjectionResult.RETRY:
            attempt++;
            if (attempt <= handler.retryPolicy.maxRetries) {
              const delay = this.calculateDelay(handler.retryPolicy, attempt);
              handler.onRetry?.(envelope, attempt, new Error('Explicit retry requested'));
              await this.sleep(delay);
            }
            break;
            
          case ProjectionResult.FAILURE:
            await this.sendToDLQ(envelope, projection, new Error('Permanent failure'));
            return;
        }
      } catch (error) {
        if (handler.isRetryable?.(error) ?? this.isTransientError(error)) {
          attempt++;
          if (attempt <= handler.retryPolicy.maxRetries) {
            const delay = this.calculateDelay(handler.retryPolicy, attempt);
            handler.onRetry?.(envelope, attempt, error);
            await this.sleep(delay);
          }
        } else {
          await this.sendToDLQ(envelope, projection, error);
          return;
        }
      }
    }
    
    // Exhausted retries
    await this.sendToDLQ(envelope, projection, new Error('Max retries exceeded'));
  }
  
  private isTransientError(error: Error): boolean {
    const transientPatterns = [
      'ECONNREFUSED', 'ETIMEDOUT', 'ECONNRESET',
      'connection', 'timeout', 'unavailable',
    ];
    return transientPatterns.some(p => 
      error.message.toLowerCase().includes(p.toLowerCase())
    );
  }
}
```

### 6. Health Checks

```typescript
interface ProjectionHealth {
  projectionName: string;
  status: 'healthy' | 'degraded' | 'unhealthy';
  lastProcessedPosition: number;
  currentHeadPosition: number;
  lag: number;
  lastProcessedAt: Date;
  failedEventCount: number;
  recentErrors: string[];
}

class ProjectionHealthChecker {
  async getHealth(projectionName: string): Promise<ProjectionHealth> {
    const checkpoint = await this.checkpointStore.get(projectionName);
    const head = await this.eventStore.getCurrentPosition();
    const failedEvents = await this.failedEventStore.getByProjection(projectionName);
    
    const lag = head - (checkpoint?.globalPosition ?? 0);
    const status = this.calculateStatus(lag, failedEvents.length);
    
    return {
      projectionName,
      status,
      lastProcessedPosition: checkpoint?.globalPosition ?? 0,
      currentHeadPosition: head,
      lag,
      lastProcessedAt: checkpoint?.updatedAt ?? new Date(0),
      failedEventCount: failedEvents.length,
      recentErrors: failedEvents.slice(0, 5).map(e => e.errorMessage),
    };
  }
  
  private calculateStatus(lag: number, failedCount: number): string {
    if (failedCount > 0) return 'unhealthy';
    if (lag > 1000) return 'degraded';
    if (lag > 100) return 'degraded';
    return 'healthy';
  }
}
```

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     Event Store                              │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                 Subscription Coordinator                      │
│                                                               │
│  ┌─────────────────────────────────────────────────────┐    │
│  │ For each event:                                       │    │
│  │   1. Check if projection needs this event            │    │
│  │   2. Try to process                                   │    │
│  │   3. On transient error → retry with backoff         │    │
│  │   4. On permanent error → send to DLQ                │    │
│  │   5. On success → advance checkpoint                  │    │
│  └─────────────────────────────────────────────────────┘    │
│                                                               │
└───────────────────────────┬───────────────────────────────────┘
                            │
         ┌──────────────────┼──────────────────┐
         ▼                  ▼                  ▼
┌─────────────────┐ ┌─────────────────┐ ┌─────────────────┐
│ Projection A    │ │ Projection B    │ │ Projection C    │
│ (healthy)       │ │ (degraded)      │ │ (unhealthy)     │
│ lag: 5          │ │ lag: 150        │ │ lag: 50         │
│ failed: 0       │ │ failed: 0       │ │ failed: 3       │
└─────────────────┘ └─────────────────┘ └─────────────────┘
                                               │
                                               ▼
                                    ┌─────────────────────┐
                                    │ Dead Letter Queue   │
                                    │ (Failed Event Store)│
                                    │                     │
                                    │ • Event 1 (pending) │
                                    │ • Event 2 (pending) │
                                    │ • Event 3 (pending) │
                                    └─────────────────────┘
```

## Consequences

### Positive

1. **Resilience** ✅
   - Transient errors are retried automatically
   - Permanent errors don't block processing

2. **Visibility** ✅
   - Failed events are tracked in DLQ
   - Health checks show projection status
   - Hooks enable alerting

3. **Recovery** ✅
   - Failed events can be reprocessed after fixes
   - Per-projection isolation prevents cascade failures

4. **Flexibility** ✅
   - Configurable retry policies
   - Per-projection error handling
   - Custom retry logic via hooks

### Negative

1. **Ordering Complexity** ⚠️
   - DLQ events are out of order with main stream
   - **Mitigation:** Reprocess in order, or accept eventual consistency

2. **Storage** ⚠️
   - DLQ can grow if errors persist
   - **Mitigation:** Alerts on DLQ size, regular cleanup

3. **Complexity** ⚠️
   - More moving parts to understand
   - **Mitigation:** Good defaults, clear documentation

## Alternatives Considered

### 1. Stop on First Error

**Rejected:** One bad event would block all subsequent events.

### 2. Skip and Log

**Rejected:** Events would be silently lost.

### 3. External Queue (Kafka, RabbitMQ)

**Rejected:** Adds operational complexity. Internal DLQ is simpler for most cases.

## Implementation Plan

### Phase 1: Core Types
- [ ] `ProjectionResult` enum
- [ ] `RetryPolicy` interface and defaults
- [ ] `FailedEvent` interface

### Phase 2: DLQ Implementation
- [ ] `FailedEventStore` interface
- [ ] PostgreSQL implementation
- [ ] In-memory implementation for testing

### Phase 3: Coordinator Integration
- [ ] Update `SubscriptionCoordinator` with retry logic
- [ ] Add error handler configuration
- [ ] Implement backoff calculation

### Phase 4: Health Checks
- [ ] `ProjectionHealth` interface
- [ ] `ProjectionHealthChecker` implementation
- [ ] Status calculation logic

### Phase 5: Tooling
- [ ] CLI/API for viewing DLQ
- [ ] CLI/API for reprocessing
- [ ] Metrics and alerting hooks

## References

- [ADR-014: Projection Checkpoint Architecture](./ADR-014-projection-checkpoint-architecture.md)
- [Error Handling in Event-Driven Systems](https://microservices.io/patterns/reliability/circuit-breaker.html)
- [Dead Letter Queue Pattern](https://docs.aws.amazon.com/AWSSimpleQueueService/latest/SQSDeveloperGuide/sqs-dead-letter-queues.html)

---

**Last Updated:** 2025-12-19
