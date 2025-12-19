# Structured Logging

The logging module provides structured JSON logging with automatic context enrichment from tracing.

## Quick Start

```typescript
import {
  StructuredLogger,
  info,
  error,
  forComponent,
} from '@event-sourcing-platform/typescript/observability';

// Use global logger
info('Order placed', { orderId: '123', total: 150.00 });

// Create component-specific logger
const logger = forComponent('OrderService');
logger.info('Processing order', { orderId: '123' });
```

## Output Formats

### JSON (Default)

```typescript
import { StructuredLogger, ConsoleJsonOutput } from '@event-sourcing-platform/typescript/observability';

const logger = new StructuredLogger({
  output: new ConsoleJsonOutput(),
});

logger.info('Order placed', { orderId: '123' });
```

Output:
```json
{"timestamp":"2025-12-19T10:30:00.000Z","level":"info","message":"Order placed","orderId":"123"}
```

### Pretty (Development)

```typescript
import { StructuredLogger, ConsolePrettyOutput } from '@event-sourcing-platform/typescript/observability';

const logger = new StructuredLogger({
  output: new ConsolePrettyOutput(),
  minLevel: 'debug',
});

logger.debug('Loading aggregate', { aggregateId: 'order-123' });
```

Output:
```
10:30:00.000 DEBUG Loading aggregate (abc12345) {"aggregateId":"order-123"}
```

## Log Levels

| Level | Priority | Use For |
|-------|----------|---------|
| `trace` | 0 | Extremely detailed debugging |
| `debug` | 1 | Development debugging info |
| `info` | 2 | Normal operation events |
| `warn` | 3 | Something unexpected but not an error |
| `error` | 4 | Errors that need attention |
| `fatal` | 5 | Critical errors, system shutting down |

```typescript
const logger = new StructuredLogger({
  minLevel: 'info', // Only info and above are logged
});

logger.debug('This will not appear');
logger.info('This will appear');
```

## Automatic Context Enrichment

When running within a `TracingContext`, logs are automatically enriched:

```typescript
import { 
  runWithContext, 
  TracingContext, 
  info 
} from '@event-sourcing-platform/typescript/observability';

const ctx = TracingContext.create({
  correlationId: 'abc-123',
  actorId: 'user-456',
});

runWithContext(ctx, () => {
  info('Order placed', { orderId: '789' });
  // Output includes correlationId and actorId automatically
});
```

Output:
```json
{
  "timestamp": "2025-12-19T10:30:00.000Z",
  "level": "info",
  "message": "Order placed",
  "correlationId": "abc-123",
  "actorId": "user-456",
  "orderId": "789"
}
```

## Child Loggers

Create loggers with additional context:

```typescript
const baseLogger = forComponent('OrderService');

const orderLogger = baseLogger.child({
  orderId: 'order-123',
  customerId: 'cust-456',
});

orderLogger.info('Processing payment');
// All logs include orderId and customerId
```

## ES Log Conventions

Follow these conventions from ADR-017:

| Operation | Success | Failure |
|-----------|---------|---------|
| Command received | DEBUG | - |
| Command executed | INFO | ERROR |
| Event appended | DEBUG | ERROR |
| Aggregate loaded | DEBUG | WARN (not found) |
| Projection processed | DEBUG | WARN (retry) / ERROR (DLQ) |
| Checkpoint saved | DEBUG | ERROR |

Use the pre-built helpers:

```typescript
import { 
  ESLogMessages, 
  forComponent 
} from '@event-sourcing-platform/typescript/observability';

const logger = forComponent('OrderAggregate');

// Command executed
const { message, context, level } = ESLogMessages.commandExecuted('PlaceOrder', 'order-123');
logger[level](message, context);

// Projection failed
const fail = ESLogMessages.projectionEventFailed('OrderSummary', 'OrderPlaced', error);
logger[fail.level](fail.message, fail.context);
```

## Configuration

### Global Logger Setup

```typescript
import {
  configureGlobalLogger,
  ConsoleJsonOutput,
  ConsolePrettyOutput,
} from '@event-sourcing-platform/typescript/observability';

// Production
configureGlobalLogger({
  minLevel: 'info',
  output: new ConsoleJsonOutput(),
  enrichFromContext: true,
});

// Development
configureGlobalLogger({
  minLevel: 'debug',
  output: new ConsolePrettyOutput(),
  enrichFromContext: true,
});
```

### Environment-Based Setup

```typescript
const isDev = process.env.NODE_ENV === 'development';

configureGlobalLogger({
  minLevel: isDev ? 'debug' : 'info',
  output: isDev ? new ConsolePrettyOutput() : new ConsoleJsonOutput(),
});
```

## Custom Output

Implement `LogOutput` for custom destinations:

```typescript
import { LogOutput, LogEntry } from '@event-sourcing-platform/typescript/observability';

class FileOutput implements LogOutput {
  write(entry: LogEntry): void {
    fs.appendFileSync('app.log', JSON.stringify(entry) + '\n');
  }
}

class CloudWatchOutput implements LogOutput {
  constructor(private client: CloudWatchLogsClient) {}
  
  write(entry: LogEntry): void {
    // Send to CloudWatch
  }
}
```

## Testing

Use `CollectorOutput` to capture logs:

```typescript
import { 
  StructuredLogger, 
  CollectorOutput 
} from '@event-sourcing-platform/typescript/observability';

const collector = new CollectorOutput();
const logger = new StructuredLogger({ output: collector });

logger.info('Test message', { key: 'value' });

expect(collector.entries).toHaveLength(1);
expect(collector.entries[0].message).toBe('Test message');
expect(collector.findByMessage('Test')).toBeDefined();
expect(collector.findByLevel('error')).toHaveLength(0);
```

Or disable logging:

```typescript
import { NoOpOutput } from '@event-sourcing-platform/typescript/observability';

const logger = new StructuredLogger({ output: new NoOpOutput() });
```

## Integration with External Loggers

Wrap existing loggers:

```typescript
import pino from 'pino';
import { Logger, LogContext } from '@event-sourcing-platform/typescript/observability';

class PinoLogger implements Logger {
  private pino = pino();
  
  info(message: string, context?: LogContext): void {
    this.pino.info(context, message);
  }
  // ... other methods
}
```

## Best Practices

1. **Always use structured logging** (JSON in production)
2. **Include request context** via TracingContext
3. **Use component loggers** for easy filtering
4. **Follow ES log conventions** for consistency
5. **Log at appropriate levels** (don't over-log)
6. **Include actionable context** (IDs, counts, durations)
7. **Don't log sensitive data** (passwords, tokens, PII)
8. **Use child loggers** for request-scoped context

### Example: Request Handler

```typescript
import { TracingContext, runWithContext, forComponent } from '@event-sourcing-platform/typescript/observability';

const logger = forComponent('OrderController');

async function handleCreateOrder(req, res) {
  const ctx = TracingContext.fromRequest(req);
  
  return runWithContext(ctx, async () => {
    logger.info('Creating order', { 
      customerId: req.body.customerId,
      itemCount: req.body.items.length,
    });
    
    try {
      const order = await orderService.createOrder(req.body);
      logger.info('Order created', { orderId: order.id });
      return res.json(order);
    } catch (error) {
      logger.error('Failed to create order', {
        error: error.message,
        customerId: req.body.customerId,
      });
      throw error;
    }
  });
}
```
