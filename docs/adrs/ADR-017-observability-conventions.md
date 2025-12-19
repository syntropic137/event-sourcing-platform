# ADR-017: Observability Conventions

**Status:** ✅ Accepted  
**Date:** 2025-12-19  
**Decision Makers:** Platform Team  
**Related:** ADR-005 (Hexagonal Architecture), Event Store Proto

## Context

### The Problem

The Event Sourcing Platform has observability fields in the event envelope:
- `correlation_id` — Links related operations
- `causation_id` — Links cause and effect
- `actor_id` — Who initiated the action
- `headers` — Extensible metadata (tracing, etc.)

However, there's no documented convention for:
- How these fields should be populated
- How they flow through command → event → projection
- How to integrate with OpenTelemetry or other tracing systems
- What metrics should be exposed
- What logging patterns to follow

### Requirements

1. **Tracing** — Correlation IDs flow through the entire request lifecycle
2. **Metrics** — Standard ES metrics for monitoring
3. **Logging** — Structured logging with consistent context
4. **Framework Agnostic** — Work with OpenTelemetry, Datadog, etc.
5. **Opt-In Complexity** — Simple by default, powerful when needed

## Decision

We define **observability conventions** for the platform:

### 1. Correlation and Causation Flow

```
┌─────────────────────────────────────────────────────────────────────┐
│ HTTP Request                                                         │
│ X-Correlation-ID: abc-123                                           │
└───────────────────────────┬─────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────────────┐
│ Command                                                              │
│ correlationId: abc-123                                              │
│ causationId: null (or request ID)                                   │
└───────────────────────────┬─────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────────────┐
│ Event                                                                │
│ correlationId: abc-123 (inherited from command)                     │
│ causationId: command-xyz (the command that caused this event)      │
└───────────────────────────┬─────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────────────┐
│ Projection Handler                                                   │
│ Span: "OrderSummaryProjection.handleOrderPlaced"                    │
│ Attributes: correlationId=abc-123, eventId=evt-789                  │
└─────────────────────────────────────────────────────────────────────┘
```

### 2. Tracing Context Utility

```typescript
import { TracingContext } from '@event-sourcing-platform/typescript/observability';

// Create context from incoming request
const ctx = TracingContext.fromRequest(req);

// Context flows to commands
const command = new PlaceOrderCommand(...);
command.metadata = ctx.toCommandMetadata();

// Events inherit context
// (Automatic via aggregate.apply())

// Projections extract context
class OrderProjection {
  handleOrderPlaced(event: OrderPlaced): void {
    const ctx = TracingContext.fromEvent(event);
    const span = ctx.startSpan('OrderProjection.handleOrderPlaced');
    try {
      // ... process event
      span.setStatus('ok');
    } catch (error) {
      span.setStatus('error', error.message);
      throw error;
    } finally {
      span.end();
    }
  }
}
```

### 3. OpenTelemetry Integration

```typescript
import { OTelIntegration } from '@event-sourcing-platform/typescript/observability';
import { trace } from '@opentelemetry/api';

// Configure once at startup
OTelIntegration.configure({
  serviceName: 'order-service',
  tracer: trace.getTracer('event-sourcing-platform'),
});

// Automatic span creation via decorator
class OrderProjection {
  @Traced('OrderProjection.handleOrderPlaced')
  handleOrderPlaced(event: OrderPlaced): void {
    // Span automatically created with:
    // - correlationId from event
    // - eventId, eventType as attributes
    // - aggregate info as attributes
  }
}
```

### 4. Standard Metrics

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `es_events_appended_total` | Counter | `aggregate_type`, `event_type`, `tenant_id` | Total events appended |
| `es_events_append_duration_seconds` | Histogram | `aggregate_type` | Time to append events |
| `es_aggregate_load_duration_seconds` | Histogram | `aggregate_type` | Time to load/rehydrate aggregate |
| `es_command_duration_seconds` | Histogram | `command_type`, `aggregate_type` | Time to execute command |
| `es_projection_events_processed_total` | Counter | `projection_name`, `event_type` | Events processed by projection |
| `es_projection_lag_events` | Gauge | `projection_name` | Events behind head |
| `es_projection_errors_total` | Counter | `projection_name`, `error_type` | Projection processing errors |
| `es_dlq_size` | Gauge | `projection_name` | Dead letter queue size |

```typescript
import { Metrics } from '@event-sourcing-platform/typescript/observability';

// Automatic metric recording
class InstrumentedRepository {
  async save(aggregate: AggregateRoot): Promise<void> {
    const timer = Metrics.startTimer('es_events_append_duration_seconds', {
      aggregate_type: aggregate.getAggregateType(),
    });
    
    try {
      await this.inner.save(aggregate);
      Metrics.increment('es_events_appended_total', {
        aggregate_type: aggregate.getAggregateType(),
        event_type: aggregate.getUncommittedEvents().map(e => e.eventType).join(','),
      });
    } finally {
      timer.end();
    }
  }
}
```

### 5. Structured Logging

```typescript
import { Logger } from '@event-sourcing-platform/typescript/observability';

const logger = Logger.forComponent('OrderAggregate');

// Logs include context automatically
logger.info('Order placed', {
  orderId: command.orderId,
  customerId: command.customerId,
  // correlationId, actorId added automatically from context
});

// Output (JSON):
{
  "level": "info",
  "message": "Order placed",
  "component": "OrderAggregate",
  "orderId": "order-123",
  "customerId": "cust-456",
  "correlationId": "abc-123",
  "actorId": "user-789",
  "timestamp": "2025-12-19T10:30:00.000Z"
}
```

### 6. Log Levels for ES Operations

| Operation | Success Level | Failure Level |
|-----------|--------------|---------------|
| Command received | DEBUG | - |
| Command executed | INFO | ERROR |
| Event appended | DEBUG | ERROR |
| Aggregate loaded | DEBUG | WARN (not found) / ERROR |
| Projection event processed | DEBUG | WARN (retry) / ERROR (DLQ) |
| Projection checkpoint saved | DEBUG | ERROR |

## Architecture

### Module Structure

```
event-sourcing/typescript/src/observability/
├── index.ts                    # Public exports
├── tracing/
│   ├── tracing-context.ts      # Context propagation
│   ├── traced-decorator.ts     # @Traced decorator
│   └── opentelemetry.ts        # OTel integration
├── metrics/
│   ├── metrics.ts              # Metric recording
│   ├── metric-types.ts         # Type definitions
│   └── prometheus.ts           # Prometheus export
└── logging/
    ├── logger.ts               # Structured logger
    └── context-enrichment.ts   # Auto-add correlation IDs
```

### Header Conventions

The `headers` field in event metadata carries observability data:

```typescript
// Reserved header keys
const HEADER_KEYS = {
  TRACE_PARENT: 'traceparent',      // W3C Trace Context
  TRACE_STATE: 'tracestate',        // W3C Trace Context
  BAGGAGE: 'baggage',               // W3C Baggage
  REQUEST_ID: 'x-request-id',       // Original request ID
  USER_AGENT: 'user-agent',         // Client info
  CLIENT_VERSION: 'x-client-version', // SDK version
};
```

## Consequences

### Positive

1. **End-to-End Tracing** ✅
   - Correlation IDs link requests to events to projections
   - Debugging distributed flows is possible

2. **Operational Visibility** ✅
   - Standard metrics show system health
   - Alerting on lag, errors, DLQ size

3. **Consistent Logging** ✅
   - Structured logs with context
   - Easy to search and analyze

4. **Framework Flexibility** ✅
   - Works with OpenTelemetry, Prometheus, any structured logger
   - Vendor agnostic

### Negative

1. **Overhead** ⚠️
   - Tracing adds small latency
   - **Mitigation:** Make tracing configurable/sampling

2. **Learning Curve** ⚠️
   - New utilities to learn
   - **Mitigation:** Good defaults, gradual adoption

3. **Configuration** ⚠️
   - OTel setup can be complex
   - **Mitigation:** Simple getting-started examples

## Alternatives Considered

### 1. No Conventions (Let Users Decide)

**Rejected:** Inconsistency across projects, harder to debug.

### 2. Vendor-Specific Integration

**Rejected:** Lock-in to specific APM vendor.

### 3. Mandatory Tracing

**Rejected:** Too heavy for simple use cases.

## Implementation Plan

### Phase 1: Tracing Context
- [ ] `TracingContext` utility
- [ ] Context propagation through command → event
- [ ] `@Traced` decorator

### Phase 2: Metrics
- [ ] Metric types and registration
- [ ] Instrumented repository wrapper
- [ ] Projection lag metrics

### Phase 3: Logging
- [ ] Structured logger
- [ ] Context enrichment
- [ ] Log level conventions

### Phase 4: OpenTelemetry
- [ ] OTel integration module
- [ ] Span creation helpers
- [ ] Example with Jaeger

### Phase 5: Documentation
- [ ] Observability guide in docs-site
- [ ] Runbook templates
- [ ] Dashboard examples (Grafana)

## References

- [W3C Trace Context](https://www.w3.org/TR/trace-context/)
- [OpenTelemetry JavaScript](https://opentelemetry.io/docs/instrumentation/js/)
- [Prometheus Naming Conventions](https://prometheus.io/docs/practices/naming/)
- [Structured Logging Best Practices](https://www.honeycomb.io/blog/structured-logging)

---

**Last Updated:** 2025-12-19
