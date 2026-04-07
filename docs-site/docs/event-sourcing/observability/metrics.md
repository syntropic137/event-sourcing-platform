# Metrics

The metrics module provides Prometheus-compatible metrics for monitoring Event Sourcing applications.

## Quick Start

```typescript
import {
  counter,
  gauge,
  histogram,
  getMetrics,
  esInstrumentation,
} from '@syntropic137/event-sourcing-typescript/observability';

// Create custom metrics
const ordersCreated = counter(
  'orders_created_total',
  'Total orders created',
  ['customer_type']
);

ordersCreated.inc({ customer_type: 'premium' });

// Use ES instrumentation
await esInstrumentation.instrumentCommand('PlaceOrder', 'Order', async () => {
  // Automatically records es_command_duration_seconds
  await handleCommand(command);
});

// Expose metrics endpoint
app.get('/metrics', async (req, res) => {
  const metrics = await getMetrics();
  res.contentType('text/plain').send(metrics);
});
```

## Metric Types

### Counter

Monotonically increasing value:

```typescript
const counter = counter('requests_total', 'Total requests', ['method', 'path']);

counter.inc({ method: 'POST', path: '/orders' });
counter.add(5, { method: 'GET', path: '/products' });
```

### Gauge

Value that can go up and down:

```typescript
const gauge = gauge('active_connections', 'Active connections');

gauge.set(42);
gauge.inc();  // 43
gauge.dec();  // 42
gauge.add(10); // 52
```

### Histogram

Distribution of values:

```typescript
const histogram = histogram(
  'request_duration_seconds',
  'Request duration',
  [0.1, 0.5, 1, 2, 5], // Buckets
  ['method']
);

histogram.observe(0.35, { method: 'POST' });

// Or use timer
const endTimer = histogram.startTimer({ method: 'GET' });
await doWork();
endTimer(); // Records duration automatically
```

## Standard ES Metrics

Use these standard names for consistency:

### Event Store Operations

```typescript
import {
  recordEventsAppended,
  startAppendTimer,
  startAggregateLoadTimer,
  startCommandTimer,
} from '@syntropic137/event-sourcing-typescript/observability';

// Record events appended
recordEventsAppended('Order', ['OrderPlaced', 'OrderItemAdded'], 'tenant-123');

// Time append operation
const endAppend = startAppendTimer('Order');
await eventStore.append(stream, events);
endAppend();

// Time aggregate loading
const endLoad = startAggregateLoadTimer('Order');
const aggregate = await repository.load(orderId);
endLoad();

// Time command execution
const endCommand = startCommandTimer('PlaceOrder', 'Order');
await aggregate.handleCommand(command);
endCommand();
```

### Projection Operations

```typescript
import {
  recordProjectionEventProcessed,
  setProjectionLag,
  recordProjectionError,
  startProjectionProcessTimer,
  setDlqSize,
} from '@syntropic137/event-sourcing-typescript/observability';

// Record event processed
recordProjectionEventProcessed('OrderSummary', 'OrderPlaced');

// Update lag
setProjectionLag('OrderSummary', currentPosition - projectionPosition);

// Record error
recordProjectionError('OrderSummary', 'TimeoutError');

// Time processing
const endProcess = startProjectionProcessTimer('OrderSummary');
await projection.handleEvent(event);
endProcess();

// Update DLQ size
setDlqSize('OrderSummary', failedEvents.length);
```

## ESInstrumentation Helper

Convenience class for common patterns:

```typescript
import { esInstrumentation } from '@syntropic137/event-sourcing-typescript/observability';

// Instrument save operation
await esInstrumentation.instrumentSave(
  'Order',                    // aggregate type
  ['OrderPlaced'],            // event types
  'tenant-123',               // tenant ID (optional)
  async () => {
    await eventStore.append(stream, events);
  }
);

// Instrument load operation
const aggregate = await esInstrumentation.instrumentLoad(
  'Order',
  async () => repository.load(orderId)
);

// Instrument command
await esInstrumentation.instrumentCommand(
  'PlaceOrder',
  'Order',
  async () => aggregate.handleCommand(command)
);

// Instrument projection
await esInstrumentation.instrumentProjection(
  'OrderSummary',
  'OrderPlaced',
  async () => projection.handleEvent(event)
);
```

## Metric Names Reference

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `es_events_appended_total` | Counter | aggregate_type, event_type, tenant_id | Total events appended |
| `es_events_append_duration_seconds` | Histogram | aggregate_type | Time to append events |
| `es_aggregate_load_duration_seconds` | Histogram | aggregate_type | Time to load aggregate |
| `es_command_duration_seconds` | Histogram | command_type, aggregate_type | Command execution time |
| `es_projection_events_processed_total` | Counter | projection_name, event_type | Events processed |
| `es_projection_lag_events` | Gauge | projection_name | Events behind head |
| `es_projection_errors_total` | Counter | projection_name, error_type | Processing errors |
| `es_projection_process_duration_seconds` | Histogram | projection_name | Event processing time |
| `es_dlq_size` | Gauge | projection_name | DLQ size |
| `es_retries_total` | Counter | projection_name | Retry attempts |

## Prometheus Integration

### Expose Metrics Endpoint

```typescript
import express from 'express';
import { getMetrics } from '@syntropic137/event-sourcing-typescript/observability';

const app = express();

app.get('/metrics', async (req, res) => {
  const metrics = await getMetrics();
  res.contentType('text/plain; version=0.0.4').send(metrics);
});
```

Output format:

```prometheus
# HELP es_events_appended_total Total number of events appended
# TYPE es_events_appended_total counter
es_events_appended_total{aggregate_type="Order",event_type="OrderPlaced",tenant_id=""} 42

# HELP es_command_duration_seconds Time to execute a command
# TYPE es_command_duration_seconds histogram
es_command_duration_seconds_bucket{command_type="PlaceOrder",aggregate_type="Order",le="0.1"} 38
es_command_duration_seconds_bucket{command_type="PlaceOrder",aggregate_type="Order",le="0.5"} 41
es_command_duration_seconds_bucket{command_type="PlaceOrder",aggregate_type="Order",le="+Inf"} 42
es_command_duration_seconds_sum{command_type="PlaceOrder",aggregate_type="Order"} 8.234
es_command_duration_seconds_count{command_type="PlaceOrder",aggregate_type="Order"} 42
```

### Prometheus Scrape Config

```yaml
scrape_configs:
  - job_name: 'order-service'
    static_configs:
      - targets: ['order-service:3000']
    metrics_path: '/metrics'
    scrape_interval: 15s
```

## Custom Metrics Registry

For advanced use cases (e.g., external metrics library):

```typescript
import { 
  MetricsRegistry, 
  setGlobalRegistry 
} from '@syntropic137/event-sourcing-typescript/observability';

class PrometheusClientRegistry implements MetricsRegistry {
  // Wrap prom-client or similar
  counter(name, help, labelNames) { /* ... */ }
  gauge(name, help, labelNames) { /* ... */ }
  histogram(name, help, buckets, labelNames) { /* ... */ }
  async getMetrics() { /* ... */ }
  resetMetrics() { /* ... */ }
}

setGlobalRegistry(new PrometheusClientRegistry());
```

## Grafana Dashboard

Example queries for a Grafana dashboard:

### Request Rate
```promql
rate(es_events_appended_total[5m])
```

### Command Latency (p99)
```promql
histogram_quantile(0.99, rate(es_command_duration_seconds_bucket[5m]))
```

### Projection Lag
```promql
es_projection_lag_events
```

### Error Rate
```promql
rate(es_projection_errors_total[5m])
```

### DLQ Size Alert
```promql
es_dlq_size > 0
```

## Testing

Reset metrics between tests:

```typescript
import { resetMetrics, resetGlobalRegistry } from '@syntropic137/event-sourcing-typescript/observability';

beforeEach(() => {
  resetMetrics(); // Reset values, keep definitions
  // or
  resetGlobalRegistry(); // Full reset
});
```

## Best Practices

1. **Use standard ES metric names** for consistency
2. **Include meaningful labels** (but not too many — cardinality matters)
3. **Set appropriate histogram buckets** for your latencies
4. **Alert on projection lag** — it indicates falling behind
5. **Monitor DLQ size** — non-zero means events need attention
6. **Use rate() for counters** in queries
7. **Set SLOs** based on command/event latencies
