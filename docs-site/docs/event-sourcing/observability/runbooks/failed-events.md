# Runbook: Failed Events (DLQ)

## Symptoms

- `es_dlq_size` metric > 0
- Projection health shows failed events
- Data inconsistency in read models
- Users report missing or incorrect data

## Severity

| DLQ Size | Severity | Response Time |
|----------|----------|---------------|
| 1-10 | Low | Investigate within 24 hours |
| 10-100 | Medium | Investigate within 4 hours |
| > 100 | High | Investigate within 1 hour |
| Growing rapidly | Critical | Investigate immediately |

## Quick Diagnosis

### 1. Check DLQ Size

```promql
es_dlq_size
```

### 2. Check Health Endpoint

```bash
curl http://service:3000/health/projections | jq
```

### 3. Get Failed Event Details

```typescript
import { MemoryFailedEventStore } from '@event-sourcing-platform/typescript/projections';

// Get all failed events for a projection
const failed = await failedEventStore.getByProjection('OrderSummary');

for (const event of failed) {
  console.log({
    id: event.id,
    eventType: event.envelope.metadata?.eventType,
    errorMessage: event.errorMessage,
    firstAttemptAt: event.firstAttemptAt,
    lastAttemptAt: event.lastAttemptAt,
    attemptCount: event.attemptCount,
  });
}
```

## Common Causes

### 1. Code Bug in Projection Handler

**Diagnosis:**
- Same event type failing consistently
- Error message indicates logic error (null reference, type error)

**Resolution:**
1. Fix the bug in projection code
2. Deploy fix
3. Reprocess failed events

### 2. External Service Unavailable

**Diagnosis:**
- Error message indicates timeout or connection refused
- Errors correlate with external service issues

**Resolution:**
1. Wait for external service to recover
2. Reprocess failed events

### 3. Data Integrity Issue

**Diagnosis:**
- Error message indicates constraint violation
- Missing foreign key or duplicate key

**Resolution:**
1. Investigate data state
2. Fix underlying data issue
3. Potentially rebuild projection

### 4. Schema Mismatch

**Diagnosis:**
- Error parsing event payload
- Old event version incompatible with new code

**Resolution:**
1. Add upcaster for old event version
2. Deploy fix
3. Reprocess failed events

## Investigation Steps

### 1. Get Error Details

```typescript
const failed = await failedEventStore.getByStatus('pending');

// Group by error type
const byError = {};
for (const event of failed) {
  const key = event.errorMessage.split('\n')[0];
  byError[key] = (byError[key] || 0) + 1;
}
console.log(byError);
```

### 2. Get Event Context

```typescript
const failedEvent = failed[0];
console.log({
  correlationId: failedEvent.envelope.metadata?.correlationId,
  aggregateId: failedEvent.envelope.metadata?.aggregateId,
  eventType: failedEvent.envelope.metadata?.eventType,
  payload: failedEvent.envelope.payload,
});
```

### 3. Trace the Request

Use correlation ID to find related logs:

```bash
# Search logs
grep "correlationId\":\"abc-123" /var/log/app/*.log

# Or in centralized logging
# Kibana: correlationId: "abc-123"
```

### 4. Reproduce Locally

```typescript
// Load the exact event
const event = failedEvent.envelope;

// Create projection instance
const projection = new OrderSummaryProjection();

// Try to handle
try {
  await projection.handleEvent(event);
} catch (error) {
  console.error('Reproduction:', error);
}
```

## Resolution Procedures

### Reprocess After Fix

After deploying a fix:

```typescript
// 1. Get failed events
const failed = await failedEventStore.getByProjection('OrderSummary');

// 2. Mark for reprocessing
for (const event of failed) {
  await failedEventStore.markReprocessing(event.id);
}

// 3. Reprocess (coordinator handles this automatically on next poll)
// Or manually:
for (const event of failed) {
  try {
    await projection.handleEvent(event.envelope);
    await failedEventStore.markResolved(event.id);
  } catch (error) {
    // Still failing - will go back to DLQ
    await failedEventStore.add({
      ...event,
      errorMessage: error.message,
      attemptCount: event.attemptCount + 1,
      lastAttemptAt: new Date(),
    });
  }
}
```

### Ignore Events

If events are invalid and can be safely ignored:

```typescript
// Review events first!
const failed = await failedEventStore.getByProjection('OrderSummary');

for (const event of failed) {
  // Only ignore if you're sure!
  await failedEventStore.markIgnored(event.id, 'Invalid event from migration');
}
```

### Full Rebuild

If DLQ has too many events or state is corrupted:

```typescript
// 1. Clear DLQ (optional - may want to keep for analysis)
await failedEventStore.clear();

// 2. Bump projection version
class OrderSummaryProjection extends CheckpointedProjection {
  getVersion() { return 2; }
}

// 3. Clear read model state
await database.query('TRUNCATE order_summaries');

// 4. Restart service - projection rebuilds from scratch
```

## Prevention

### Code Review Checklist

Before deploying projection changes:
- [ ] Handle all possible event payloads
- [ ] Handle null/undefined fields gracefully
- [ ] Handle missing entities (upsert patterns)
- [ ] Handle duplicate processing (idempotency)
- [ ] Test with production-like data

### Error Handling Best Practices

```typescript
class OrderSummaryProjection extends CheckpointedProjection {
  protected async processEvent(
    envelope: EventEnvelope<DomainEvent>,
    checkpointStore: ProjectionCheckpointStore
  ): Promise<ProjectionResult> {
    
    // Log context for debugging
    this.logger.debug('Processing event', {
      eventType: envelope.metadata?.eventType,
      aggregateId: envelope.metadata?.aggregateId,
    });
    
    try {
      // Your logic here
      
      await this.saveCheckpoint(checkpointStore, envelope.metadata!.globalNonce);
      return ProjectionResult.SUCCESS;
      
    } catch (error) {
      // Add context to error
      this.logger.error('Failed to process event', {
        eventType: envelope.metadata?.eventType,
        error: error.message,
      });
      
      // Transient vs permanent
      if (isTransientError(error)) {
        return ProjectionResult.RETRY;
      }
      
      return ProjectionResult.FAILURE;
    }
  }
}
```

### Monitoring

Set up alerts:

```yaml
groups:
  - name: dlq
    rules:
      - alert: DLQNotEmpty
        expr: es_dlq_size > 0
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "DLQ has {{ $value }} failed events"
          
      - alert: DLQGrowing
        expr: rate(es_dlq_size[5m]) > 0
        for: 10m
        labels:
          severity: critical
        annotations:
          summary: "DLQ is growing - investigate immediately"
```

## Related

- [Failure Handling](../../projections/failure-handling.md)
- [Projection Lag Runbook](./projection-lag.md)
- [Rebuilding Projections](../../projections/rebuilding.md)
