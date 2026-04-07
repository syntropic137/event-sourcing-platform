# Distributed Tracing

Distributed tracing enables end-to-end visibility into request flows through your Event Sourcing application.

## Core Concepts

### TracingContext

`TracingContext` carries correlation and causation IDs through the system:

```typescript
import { TracingContext } from '@syntropic137/event-sourcing-typescript/observability';

// Create from HTTP request
const ctx = TracingContext.fromRequest(req);

// Create manually
const ctx = TracingContext.create({
  correlationId: 'my-correlation-id',
  actorId: 'user-123',
  tenantId: 'tenant-abc',
});

// Access data
console.log(ctx.correlationId);  // "my-correlation-id"
console.log(ctx.actorId);        // "user-123"
```

### Context Propagation

Use `runWithContext` to propagate tracing context through async operations:

```typescript
import { 
  TracingContext, 
  runWithContext, 
  getCurrentContext 
} from '@syntropic137/event-sourcing-typescript/observability';

app.use(async (req, res, next) => {
  const ctx = TracingContext.fromRequest(req);
  
  await runWithContext(ctx, async () => {
    // All code here has access to the context
    const current = getCurrentContext();
    console.log(current.correlationId); // Available automatically
    
    await next();
  });
});
```

### The @Traced Decorator

Automatically create spans around methods:

```typescript
import { Traced } from '@syntropic137/event-sourcing-typescript/observability';

class OrderService {
  @Traced('OrderService.placeOrder')
  async placeOrder(command: PlaceOrderCommand): Promise<void> {
    // Span is automatically:
    // - Created at method entry
    // - Populated with correlation/causation IDs from context
    // - Marked as error if exception thrown
    // - Ended on method completion
  }
}
```

With options:

```typescript
@Traced({
  name: 'OrderService.placeOrder',
  kind: 'server',
  attributes: { 'service.name': 'order-service' },
  extractAttributes: (args) => ({
    'order.customer_id': (args[0] as PlaceOrderCommand).customerId,
  }),
})
async placeOrder(command: PlaceOrderCommand): Promise<void> {
  // ...
}
```

## Manual Span Creation

For fine-grained control:

```typescript
import { 
  startSpan, 
  withSpan, 
  withSpanAsync 
} from '@syntropic137/event-sourcing-typescript/observability';

// Manual span management
const span = startSpan('processOrder', {
  kind: 'internal',
  attributes: { 'order.id': orderId },
});

try {
  await processOrder(orderId);
  span.setStatus('ok');
} catch (error) {
  span.setStatus('error', error.message);
  span.recordException(error);
  throw error;
} finally {
  span.end();
}

// Or use helper
await withSpanAsync('processOrder', async (span) => {
  span.setAttribute('order.id', orderId);
  await processOrder(orderId);
  // Span automatically ended and status set
});
```

## OpenTelemetry Integration

### Basic Setup

```typescript
import { NodeSDK } from '@opentelemetry/sdk-node';
import { getNodeAutoInstrumentations } from '@opentelemetry/auto-instrumentations-node';
import { OTLPTraceExporter } from '@opentelemetry/exporter-trace-otlp-http';
import { trace } from '@opentelemetry/api';
import { createOTelTracer, setGlobalTracer } from '@syntropic137/event-sourcing-typescript/observability';

// Initialize OpenTelemetry
const sdk = new NodeSDK({
  traceExporter: new OTLPTraceExporter({
    url: 'http://localhost:4318/v1/traces',
  }),
  instrumentations: [getNodeAutoInstrumentations()],
});

sdk.start();

// Bridge to ES observability module
const otelTracer = trace.getTracer('order-service', '1.0.0');
const tracer = createOTelTracer({
  tracer: otelTracer,
  serviceName: 'order-service',
});

setGlobalTracer(tracer);
```

### With Jaeger

```bash
# Start Jaeger
docker run -d --name jaeger \
  -e COLLECTOR_OTLP_ENABLED=true \
  -p 16686:16686 \
  -p 4318:4318 \
  jaegertracing/all-in-one:latest
```

Then use OTLP exporter pointing to `http://localhost:4318/v1/traces`.

## Header Conventions

Standard headers for propagation:

| Header | Purpose |
|--------|---------|
| `X-Correlation-ID` | Links related operations |
| `X-Causation-ID` | Links cause and effect |
| `X-Actor-ID` | Who initiated the operation |
| `X-Tenant-ID` | Multi-tenancy isolation |
| `traceparent` | W3C Trace Context |
| `tracestate` | W3C Trace Context state |

### Extracting from Request

```typescript
const ctx = TracingContext.fromRequest(req);
// Automatically extracts all standard headers
```

### Propagating to Outgoing Requests

```typescript
const headers = ctx.toHeaders();
// Returns: { 'x-correlation-id': '...', 'x-causation-id': '...', ... }

await fetch('http://other-service/api', {
  headers: headers,
});
```

## Context Flow Through ES

### Command → Event

```typescript
// In command handler
class OrderAggregate {
  @CommandHandler('PlaceOrderCommand')
  placeOrder(command: PlaceOrderCommand): void {
    // Events inherit context from the aggregate's apply
    this.apply(new OrderPlacedEvent(command.items));
    // Event will have correlationId, causationId from current context
  }
}
```

### Event → Projection

```typescript
class OrderProjection {
  @Traced('OrderProjection.handleOrderPlaced')
  handleOrderPlaced(event: OrderPlacedEvent): void {
    // Extract context from event
    const ctx = TracingContext.fromEvent(event);
    // ctx.correlationId = original request correlation
    // ctx.causationId = event ID that caused this
  }
}
```

## W3C Trace Context

For interoperability with other systems:

```typescript
import { 
  parseTraceparent, 
  generateTraceparent 
} from '@syntropic137/event-sourcing-typescript/observability';

// Parse incoming header
const parsed = parseTraceparent('00-abc123...-def456...-01');
// { traceId: 'abc123...', parentId: 'def456...', traceFlags: 1 }

// Generate for outgoing
const traceparent = generateTraceparent(parsed?.traceId);
// '00-abc123...-new-span-id-01'
```

## Testing

Use no-op implementations in tests:

```typescript
import { 
  resetGlobalTracer, 
  NoOpTracer 
} from '@syntropic137/event-sourcing-typescript/observability';

beforeEach(() => {
  resetGlobalTracer(); // Uses NoOpTracer
});
```

Or capture spans for assertions:

```typescript
import { setGlobalTracer, Span } from '@syntropic137/event-sourcing-typescript/observability';

const capturedSpans: Span[] = [];

class CapturingTracer implements Tracer {
  startSpan(name: string): Span {
    const span = new NoOpSpan(name);
    capturedSpans.push(span);
    return span;
  }
  // ... other methods
}

setGlobalTracer(new CapturingTracer());
```

## Best Practices

1. **Always use TracingContext.fromRequest()** at entry points
2. **Wrap all async operations** with `runWithContext` for propagation
3. **Use @Traced on key methods** (service methods, event handlers)
4. **Include business attributes** in spans (order ID, customer ID)
5. **Set meaningful span names** (Component.operation format)
6. **Record exceptions** for debugging
7. **Don't trace everything** — focus on boundaries and key operations
