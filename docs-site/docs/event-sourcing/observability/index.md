# Observability

The observability module provides tracing, metrics, and structured logging for Event Sourcing applications. It follows conventions from [ADR-017](/docs/adrs/ADR-017-observability-conventions.md).

## Quick Start

```typescript
import {
  // Tracing
  TracingContext,
  Traced,
  runWithContext,
  
  // Metrics
  esInstrumentation,
  getMetrics,
  
  // Logging
  StructuredLogger,
  info,
} from '@event-sourcing-platform/typescript/observability';
```

## Core Concepts

### Correlation and Causation Flow

Observability data flows through the entire request lifecycle:

```
HTTP Request                    Command                     Event                       Projection
X-Correlation-ID: abc-123  -->  correlationId: abc-123  --> correlationId: abc-123  --> Span: correlationId=abc-123
                               causationId: request-xyz     causationId: cmd-456         eventId=evt-789
```

The event envelope already contains:
- `correlationId` — Links related operations across services
- `causationId` — Links cause and effect (what caused this event?)
- `actorId` — Who initiated the action
- `tenantId` — Multi-tenancy isolation

The observability module provides utilities to:
1. **Extract** these IDs from incoming requests
2. **Propagate** them through commands and events
3. **Inject** them into spans, metrics, and logs

## Module Overview

| Component | Purpose | Key Exports |
|-----------|---------|-------------|
| [Tracing](./tracing.md) | Distributed request tracing | `TracingContext`, `@Traced`, `runWithContext` |
| [Metrics](./metrics.md) | Prometheus-compatible metrics | `counter`, `gauge`, `histogram`, `esInstrumentation` |
| [Logging](./logging.md) | Structured JSON logging | `StructuredLogger`, `info`, `error` |

## Minimal Setup

### 1. Extract Context from Requests

```typescript
import { TracingContext, runWithContext } from '@event-sourcing-platform/typescript/observability';

// In your HTTP middleware
app.use((req, res, next) => {
  const ctx = TracingContext.fromRequest(req);
  
  runWithContext(ctx, () => {
    next();
  });
});
```

### 2. Instrument Key Operations

```typescript
import { Traced, esInstrumentation } from '@event-sourcing-platform/typescript/observability';

class OrderService {
  @Traced('OrderService.placeOrder')
  async placeOrder(command: PlaceOrderCommand): Promise<void> {
    // Span automatically created with correlation IDs
    
    await esInstrumentation.instrumentCommand(
      'PlaceOrder',
      'Order',
      async () => {
        // Metrics automatically recorded
        await this.repository.save(aggregate);
      }
    );
  }
}
```

### 3. Use Structured Logging

```typescript
import { info, error, forComponent } from '@event-sourcing-platform/typescript/observability';

const logger = forComponent('OrderService');

logger.info('Order placed', { orderId: '123', total: 150.00 });
// Output: {"timestamp":"...","level":"info","component":"OrderService","correlationId":"abc-123",...}
```

### 4. Expose Metrics Endpoint

```typescript
import { getMetrics } from '@event-sourcing-platform/typescript/observability';

app.get('/metrics', async (req, res) => {
  const metrics = await getMetrics();
  res.contentType('text/plain').send(metrics);
});
```

## OpenTelemetry Integration

For production, integrate with OpenTelemetry:

```typescript
import { trace } from '@opentelemetry/api';
import { 
  createOTelTracer, 
  setGlobalTracer 
} from '@event-sourcing-platform/typescript/observability';

// At application startup
const otelTracer = trace.getTracer('my-service', '1.0.0');
const tracer = createOTelTracer({
  tracer: otelTracer,
  serviceName: 'order-service',
});

setGlobalTracer(tracer);
```

See [Tracing](./tracing.md) for detailed setup.

## Standard ES Metrics

The module defines standard metric names for consistency:

| Metric | Type | Description |
|--------|------|-------------|
| `es_events_appended_total` | Counter | Events appended to streams |
| `es_events_append_duration_seconds` | Histogram | Time to append events |
| `es_aggregate_load_duration_seconds` | Histogram | Time to load aggregates |
| `es_command_duration_seconds` | Histogram | Command execution time |
| `es_projection_events_processed_total` | Counter | Events processed by projections |
| `es_projection_lag_events` | Gauge | How far behind projections are |
| `es_projection_errors_total` | Counter | Projection processing errors |
| `es_dlq_size` | Gauge | Dead letter queue size |

See [Metrics](./metrics.md) for details.

## Log Levels for ES Operations

Follow these conventions for consistent logging:

| Operation | Success Level | Failure Level |
|-----------|--------------|---------------|
| Command received | DEBUG | - |
| Command executed | INFO | ERROR |
| Event appended | DEBUG | ERROR |
| Aggregate loaded | DEBUG | WARN (not found) / ERROR |
| Projection event processed | DEBUG | WARN (retry) / ERROR (DLQ) |
| Projection checkpoint saved | DEBUG | ERROR |

See [Logging](./logging.md) for details.

## Next Steps

- [Tracing Guide](./tracing.md) — Distributed tracing setup
- [Metrics Guide](./metrics.md) — Prometheus metrics
- [Logging Guide](./logging.md) — Structured logging
- [Runbooks](./runbooks/projection-lag.md) — Operational guides
