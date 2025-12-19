# Runbook: Projection Lag

## Symptoms

- `es_projection_lag_events` metric is high or increasing
- Read models return stale data
- Users see outdated information

## Severity

| Lag | Severity | Response Time |
|-----|----------|---------------|
| < 100 events | Low | Monitor |
| 100-1000 events | Medium | Investigate within 1 hour |
| > 1000 events | High | Investigate immediately |
| Increasing continuously | Critical | Drop everything |

## Quick Diagnosis

### 1. Check Projection Health

```bash
curl http://service:3000/health/projections
```

Expected output:
```json
{
  "status": "healthy",
  "projections": {
    "OrderSummary": {
      "status": "healthy",
      "lag": 42,
      "lastCheckpoint": 98765,
      "failedEventCount": 0
    }
  }
}
```

### 2. Check Metrics

```promql
# Current lag
es_projection_lag_events

# Lag rate of change (should be negative if catching up)
rate(es_projection_lag_events[5m])

# Events being processed
rate(es_projection_events_processed_total[5m])

# Processing time
histogram_quantile(0.99, rate(es_projection_process_duration_seconds_bucket[5m]))
```

### 3. Check for Errors

```promql
# Error rate
rate(es_projection_errors_total[5m])

# DLQ size (indicates permanent failures)
es_dlq_size
```

## Common Causes

### 1. Slow Event Processing

**Diagnosis:**
```promql
histogram_quantile(0.99, rate(es_projection_process_duration_seconds_bucket[5m])) > 0.5
```

**Resolution:**
- Optimize projection logic
- Add database indexes
- Consider batching operations
- Add caching for repeated lookups

### 2. Database Issues

**Diagnosis:**
- Check database connection pool
- Check database CPU/memory
- Check for table locks

**Resolution:**
- Increase connection pool size
- Scale database resources
- Optimize queries
- Add indexes

### 3. High Event Volume

**Diagnosis:**
```promql
rate(es_events_appended_total[5m])  # Incoming rate
rate(es_projection_events_processed_total[5m])  # Processing rate
```

**Resolution:**
- Scale horizontally (multiple projection instances)
- Optimize projection logic
- Consider event filtering (skip irrelevant events)

### 4. Projection Errors Causing Retries

**Diagnosis:**
```promql
rate(es_retries_total[5m]) > 0
es_dlq_size > 0
```

**Resolution:**
- Check logs for error details
- Fix root cause of errors
- Reprocess failed events from DLQ

### 5. Checkpoint Issues

**Diagnosis:**
- Check if checkpoint is being saved
- Check checkpoint store health

**Resolution:**
- Ensure checkpoint store is available
- Check for checkpoint write errors

## Immediate Actions

### 1. If Lag is Critical (> 1000 events)

1. **Check if projection is running:**
   ```bash
   kubectl logs -f deployment/order-service | grep OrderSummary
   ```

2. **Check for blocking errors:**
   ```promql
   es_dlq_size{projection_name="OrderSummary"}
   ```

3. **Consider scaling:**
   ```bash
   kubectl scale deployment order-service --replicas=3
   ```

### 2. If Lag is Increasing Continuously

1. **Compare ingest vs process rate:**
   ```promql
   rate(es_events_appended_total[5m]) - rate(es_projection_events_processed_total[5m])
   ```

2. **If ingest > process consistently:** Optimize or scale

### 3. If There Are DLQ Events

1. **Check DLQ contents:**
   ```typescript
   const failed = await failedEventStore.getByProjection('OrderSummary');
   console.log(failed);
   ```

2. **Investigate and fix root cause**

3. **Reprocess or mark as resolved:**
   ```typescript
   await failedEventStore.markReprocessing(failedEventId);
   // or
   await failedEventStore.markResolved(failedEventId);
   ```

## Recovery Procedures

### Full Rebuild

If projection state is corrupted or lag is unrecoverable:

```typescript
// 1. Bump projection version
class OrderSummaryProjection extends CheckpointedProjection {
  getVersion() { return 2; } // Was 1
}

// 2. On startup, version mismatch triggers rebuild
// Projection will reset checkpoint to 0 and reprocess all events
```

### Partial Rebuild

If you know the problematic range:

```typescript
// Reset checkpoint to before the problem
await checkpointStore.save({
  projectionName: 'OrderSummary',
  projectionVersion: 1,
  position: 50000n, // Position before problems
  updatedAt: new Date(),
});

// Restart service - will catch up from position 50000
```

## Prevention

### Alerting

Set up alerts for:

```yaml
# Prometheus alert rules
groups:
  - name: projections
    rules:
      - alert: ProjectionLagHigh
        expr: es_projection_lag_events > 1000
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Projection {{ $labels.projection_name }} is lagging"
          
      - alert: ProjectionLagCritical
        expr: es_projection_lag_events > 5000
        for: 5m
        labels:
          severity: critical
          
      - alert: ProjectionDLQNotEmpty
        expr: es_dlq_size > 0
        for: 1m
        labels:
          severity: warning
        annotations:
          summary: "DLQ has failed events for {{ $labels.projection_name }}"
```

### Monitoring Dashboard

Key panels:
- Projection lag over time (line chart)
- Processing rate vs append rate (line chart)
- P99 processing latency (line chart)
- DLQ size (single stat with threshold coloring)
- Error rate by type (bar chart)

## Related

- [Projection Failure Handling](../../projections/failure-handling.md)
- [Rebuilding Projections](../../projections/rebuilding.md)
- [Health Monitoring](../../projections/health-monitoring.md)
