# Health Monitoring

Monitor projection health to detect issues early.

## Health Metrics

### Lag

Events behind the current head position:

```typescript
lag = currentHeadPosition - lastProcessedPosition
```

| Lag | Status | Meaning |
|-----|--------|---------|
| 0-100 | Healthy | Normal operation |
| 100-1000 | Degraded | Processing slowly |
| 1000+ | Unhealthy | Significantly behind |

### Failed Event Count

Events in the DLQ:

| Count | Status |
|-------|--------|
| 0 | Healthy |
| Any | Unhealthy |

### Staleness

Time since last event was processed:

| Age | Status |
|-----|--------|
| < 1 min | Healthy |
| 1-5 min | Degraded (if lag > 0) |
| > 5 min | Investigate |

## Using ProjectionHealthChecker

```typescript
import {
  ProjectionHealthChecker,
  MemoryCheckpointStore,
  MemoryFailedEventStore,
} from '@syntropic137/event-sourcing-typescript/projections';

const healthChecker = new ProjectionHealthChecker({
  checkpointStore,
  failedEventStore,
  positionProvider: eventStore, // Must have getCurrentPosition()
  thresholds: {
    degradedLagThreshold: 100,
    unhealthyLagThreshold: 1000,
    maxHealthyAgeMs: 60000, // 1 minute
  },
});
```

## Checking Health

### Single Projection

```typescript
const health = await healthChecker.getHealth('order-summary');

console.log(health);
// {
//   projectionName: 'order-summary',
//   status: 'healthy',
//   lastProcessedPosition: 42,
//   currentHeadPosition: 45,
//   lag: 3,
//   lastProcessedAt: Date,
//   failedEventCount: 0,
//   recentErrors: [],
// }
```

### All Projections

```typescript
const allHealth = await healthChecker.getAllHealth();

for (const health of allHealth) {
  if (health.status !== 'healthy') {
    console.warn(`Projection ${health.projectionName} is ${health.status}`);
  }
}
```

### Summary

```typescript
const summary = await healthChecker.getHealthSummary();

console.log(summary);
// {
//   overall: 'healthy',
//   healthy: 3,
//   degraded: 0,
//   unhealthy: 0,
//   totalLag: 15,
//   totalFailedEvents: 0,
// }
```

## Health Check Endpoint

Create an HTTP health check endpoint:

```typescript
import { createHealthCheckResponse } from '@syntropic137/event-sourcing-typescript/projections';

app.get('/health/projections', async (req, res) => {
  const response = await createHealthCheckResponse(healthChecker);
  
  const statusCode = response.status === 'pass' ? 200 :
                     response.status === 'warn' ? 200 :
                     503;
  
  res.status(statusCode).json(response);
});
```

Response format (RFC Health Check):

```json
{
  "status": "pass",
  "checks": {
    "projections:lag": {
      "status": "pass",
      "observedValue": 15,
      "observedUnit": "events"
    },
    "projections:failed": {
      "status": "pass",
      "observedValue": 0,
      "observedUnit": "events"
    }
  }
}
```

## Alerting

### Lag Alerts

```typescript
async function checkLagAlerts() {
  const allHealth = await healthChecker.getAllHealth();
  
  for (const health of allHealth) {
    if (health.lag > 500) {
      alerting.warn('Projection lag high', {
        projection: health.projectionName,
        lag: health.lag,
      });
    }
    
    if (health.lag > 2000) {
      alerting.critical('Projection critically behind', {
        projection: health.projectionName,
        lag: health.lag,
      });
    }
  }
}

// Check every minute
setInterval(checkLagAlerts, 60000);
```

### DLQ Alerts

```typescript
async function checkDLQAlerts() {
  const pending = await failedEventStore.getPendingCount();
  
  if (pending > 0) {
    alerting.error('Events in DLQ', { count: pending });
  }
}
```

### Staleness Alerts

```typescript
async function checkStalenessAlerts() {
  const allHealth = await healthChecker.getAllHealth();
  
  for (const health of allHealth) {
    const age = Date.now() - health.lastProcessedAt.getTime();
    
    if (age > 300000 && health.lag > 0) { // 5 minutes
      alerting.warn('Projection appears stuck', {
        projection: health.projectionName,
        lastProcessedAge: age,
        lag: health.lag,
      });
    }
  }
}
```

## Metrics

Export metrics for Prometheus/Grafana:

```typescript
// Using prom-client or similar
const lagGauge = new Gauge({
  name: 'es_projection_lag_events',
  help: 'Events behind head position',
  labelNames: ['projection'],
});

const failedGauge = new Gauge({
  name: 'es_projection_failed_events',
  help: 'Events in DLQ',
  labelNames: ['projection'],
});

async function updateMetrics() {
  const allHealth = await healthChecker.getAllHealth();
  
  for (const health of allHealth) {
    lagGauge.set({ projection: health.projectionName }, health.lag);
    failedGauge.set({ projection: health.projectionName }, health.failedEventCount);
  }
}

setInterval(updateMetrics, 10000);
```

## Dashboard Queries (Grafana)

```promql
# Lag by projection
es_projection_lag_events

# Total lag across all projections  
sum(es_projection_lag_events)

# Projections with lag > 100
es_projection_lag_events > 100

# Failed events by projection
es_projection_failed_events

# Rate of events processed per second
rate(es_projection_events_processed_total[5m])
```

## Custom Health Thresholds

```typescript
const healthChecker = new ProjectionHealthChecker({
  checkpointStore,
  failedEventStore,
  positionProvider: eventStore,
  thresholds: {
    degradedLagThreshold: 50,     // Lower threshold
    unhealthyLagThreshold: 500,   // Lower threshold
    maxHealthyAgeMs: 30000,       // 30 seconds
  },
});
```

Adjust thresholds based on:
- Event volume
- Processing speed requirements
- Business criticality
