# Failure Handling

How the projection system handles errors and failures.

## Error Types

### Transient Errors

Temporary issues that may resolve on retry:
- Network timeouts
- Database connection issues
- Service unavailable
- Resource exhaustion

**Strategy:** Retry with exponential backoff

### Permanent Errors

Issues that won't resolve by retrying:
- Malformed event data
- Business logic errors
- Schema mismatches
- Bugs in projection code

**Strategy:** Send to Dead Letter Queue (DLQ)

## Retry Policy

Configure retry behavior:

```typescript
import { RetryPolicy } from '@event-sourcing-platform/typescript/projections';

// Exponential backoff (default)
const policy = RetryPolicy.exponentialBackoff({
  maxRetries: 3,        // Attempt up to 3 times
  initialDelayMs: 100,  // First retry after 100ms
  maxDelayMs: 30000,    // Cap at 30 seconds
  backoffMultiplier: 2, // Double delay each retry
});

// Delay sequence: 100ms → 200ms → 400ms → DLQ
```

### Retry Delay Calculation

```
delay = min(initialDelayMs × (backoffMultiplier ^ attempt), maxDelayMs)

Attempt 1: min(100 × 2^0, 30000) = 100ms
Attempt 2: min(100 × 2^1, 30000) = 200ms
Attempt 3: min(100 × 2^2, 30000) = 400ms
```

### Custom Retry Patterns

```typescript
// Fixed delay (no backoff)
RetryPolicy.fixedDelay(1000, 5); // 1 second delay, 5 retries

// No retries (fail immediately)
RetryPolicy.noRetry();

// Custom retryable error patterns
new RetryPolicy({
  maxRetries: 5,
  initialDelayMs: 500,
  maxDelayMs: 60000,
  backoffMultiplier: 2,
  retryablePatterns: ['timeout', 'connection', 'rate limit'],
});
```

## Dead Letter Queue (DLQ)

Events that fail permanently go to the DLQ:

```typescript
import { MemoryFailedEventStore } from '@event-sourcing-platform/typescript/projections';

const failedEventStore = new MemoryFailedEventStore();

// Configure in coordinator
const coordinator = new SubscriptionCoordinator({
  // ...
  failedEventStore,
});
```

### Failed Event Structure

```typescript
interface FailedEvent {
  id: string;                    // Unique failure record ID
  projectionName: string;        // Which projection failed
  eventId: string;               // The event's ID
  eventType: string;             // Event type
  globalNonce: number;           // Position in global stream
  aggregateId: string;           // Source aggregate
  aggregateType: string;         // Aggregate type
  payload: string;               // Serialized event data
  errorMessage: string;          // Error message
  errorStack?: string;           // Stack trace
  attemptCount: number;          // How many times tried
  firstFailedAt: Date;           // First failure time
  lastFailedAt: Date;            // Last failure time
  status: 'pending' | 'reprocessing' | 'resolved' | 'ignored';
}
```

### Managing Failed Events

```typescript
// Get failed events for a projection
const failed = await failedEventStore.getByProjection('order-summary', {
  status: 'pending',
  limit: 10,
});

// Mark as resolved (after fixing the issue)
await failedEventStore.markResolved(failedEvent.id);

// Mark as ignored (won't retry)
await failedEventStore.markIgnored(failedEvent.id, 'Known data issue, skipping');

// Clean up old resolved events
const deleted = await failedEventStore.cleanupResolved(
  new Date(Date.now() - 7 * 24 * 60 * 60 * 1000) // 7 days ago
);
```

## Error Handler Callbacks

Hook into error handling for monitoring:

```typescript
const coordinator = new SubscriptionCoordinator({
  // ...
  callbacks: {
    onEventProcessed: (projectionName, envelope, result) => {
      metrics.increment('projection.events_processed', {
        projection: projectionName,
        result: result,
      });
    },
    onError: (projectionName, envelope, error) => {
      alerting.notify('Projection failure', {
        projection: projectionName,
        eventId: envelope.metadata.eventId,
        error: error.message,
      });
    },
  },
});
```

## Handling Specific Errors

In your projection, return appropriate results:

```typescript
protected async processEvent(envelope, checkpointStore): Promise<ProjectionResult> {
  try {
    await this.updateReadModel(envelope);
    await this.saveCheckpoint(checkpointStore, envelope.metadata.globalNonce!);
    return ProjectionResult.SUCCESS;

  } catch (error) {
    if (this.isRetryable(error)) {
      // Will retry with backoff
      return ProjectionResult.RETRY;
    }

    if (this.shouldSkip(error)) {
      // Skip this event (still advances checkpoint)
      console.warn('Skipping malformed event', envelope.metadata.eventId);
      await this.saveCheckpoint(checkpointStore, envelope.metadata.globalNonce!);
      return ProjectionResult.SKIP;
    }

    // Permanent failure - goes to DLQ
    return ProjectionResult.FAILURE;
  }
}

private isRetryable(error: Error): boolean {
  const message = error.message.toLowerCase();
  return message.includes('timeout') || 
         message.includes('connection') ||
         message.includes('temporarily');
}

private shouldSkip(error: Error): boolean {
  return error.message.includes('unknown event version');
}
```

## Reprocessing Failed Events

After fixing a bug, reprocess failed events:

```typescript
// Get pending failed events
const pending = await failedEventStore.getByProjection('order-summary', {
  status: 'pending',
});

for (const failed of pending) {
  // Mark as reprocessing
  await failedEventStore.markReprocessing(failed.id);

  try {
    // Parse the stored payload
    const eventData = JSON.parse(failed.payload);
    
    // Reprocess through projection
    // (Implement based on your projection's needs)
    await projection.reprocess(eventData);

    // Mark as resolved
    await failedEventStore.markResolved(failed.id);
  } catch (error) {
    // Still failing - leave as pending
    console.error('Reprocess failed:', error);
  }
}
```

## Best Practices

### 1. Log Before DLQ

```typescript
callbacks: {
  onDLQ: (envelope, error) => {
    logger.error('Event sent to DLQ', {
      eventId: envelope.metadata.eventId,
      eventType: envelope.event.eventType,
      error: error.message,
      stack: error.stack,
    });
  },
}
```

### 2. Alert on DLQ Growth

```typescript
// Periodic health check
setInterval(async () => {
  const pending = await failedEventStore.getPendingCount();
  if (pending > 10) {
    alerting.warn('DLQ is growing', { pendingCount: pending });
  }
}, 60000);
```

### 3. Different Policies per Projection

```typescript
const coordinator = new SubscriptionCoordinator({
  defaultRetryPolicy: RetryPolicy.exponentialBackoff({ maxRetries: 3 }),
  retryPolicyOverrides: new Map([
    ['critical-projection', RetryPolicy.exponentialBackoff({ maxRetries: 10 })],
    ['optional-projection', RetryPolicy.noRetry()],
  ]),
});
```
