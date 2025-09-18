# TypeScript SDK

The TypeScript SDK provides a type-safe, decorator-driven interface for interacting with the Event Store. Built with modern TypeScript features and optimized for Node.js and browser environments.

## 🚀 Installation

```bash
npm install @eventstore/sdk-ts
```

## 🎯 Quick Start

```typescript
import { EventStoreClient, Event } from '@eventstore/sdk-ts';

// Define your events
@Event(\{ type: 'com.myco.order.OrderCreated', revision: 1 \})
class OrderCreated {
  constructor(
    public readonly orderId: string,
    public readonly amount: number,
    public readonly currency: string = 'USD'
  ) {}
}

@Event(\{ type: 'com.myco.order.OrderShipped', revision: 1 \})
class OrderShipped {
  constructor(
    public readonly orderId: string,
    public readonly trackingNumber: string
  ) {}
}

// Create client
const client = new EventStoreClient(\{
  endpoint: 'localhost:50051',
  // credentials: \{ ... \} // Add auth as needed
\});

// Append events
await client.appendEvents(\{
  aggregateId: 'order-123',
  aggregateType: 'Order',
  expectedAggregateNonce: 0, // First event -> stream must not exist yet
  events: [
    new OrderCreated('order-123', 99.99, 'USD'),
    new OrderShipped('order-123', 'TRK123456')
  ]
\});
```

## 📚 API Reference

### Client Configuration

```typescript
interface EventStoreClientConfig {
  endpoint: string;
  credentials?: {
    username: string;
    password: string;
  };
  tls?: {
    caCertificate?: Buffer;
    clientCertificate?: Buffer;
    clientKey?: Buffer;
  };
  connectionTimeout?: number;
  requestTimeout?: number;
}
```

### Event Definition

```typescript
@Event({
  type: string,        // Fully qualified event type name
  revision: number,    // Schema revision for evolution
  contentType?: string // Optional: defaults to 'application/json'
})
class YourEvent {
  constructor(
    // Event properties
    public readonly prop1: string,
    public readonly prop2: number
  ) {}
}
```

### Append Operations

```typescript
// Append with optimistic concurrency: expect stream head to be 2
await client.appendEvents({
  tenantId: 'tenant-123',
  aggregateId: 'order-123',
  aggregateType: 'Order',
  expectedAggregateNonce: 2,
  events: [new OrderUpdated('order-123', { status: 'confirmed' })]
});

// Start a new stream (0 means the aggregate must not exist yet)
await client.appendEvents({
  tenantId: 'tenant-123',
  aggregateId: 'order-456',
  aggregateType: 'Order',
  expectedAggregateNonce: 0,
  events: [new OrderCreated('order-456', 149.99)]
});
```

### Read Operations

```typescript
// Read entire stream forward
const stream = await client.readStream({
  aggregateId: 'order-123',
  fromAggregateNonce: 1,  // Start from event 1
  forward: true,          // Read forward (ascending)
  maxCount: 100           // Limit results
});

// Read backward from latest
const recentEvents = await client.readStream({
  aggregateId: 'order-123',
  fromAggregateNonce: 10, // Start from event 10
  forward: false,         // Read backward (descending)
  maxCount: 5             // Get last 5 events
});
```

### Subscription Operations

```typescript
// Subscribe to all events from beginning
const subscription1 = await client.subscribe({
  tenantId: 'tenant-123',
  fromGlobalNonce: 0  // Start from beginning
});

// Subscribe to specific aggregate type
const subscription2 = await client.subscribe({
  tenantId: 'tenant-123',
  aggregateIdPrefix: 'Order-',  // Only Order aggregates
  fromGlobalNonce: 1000         // Start from global position 1000
});

// Process events
for await (const event of subscription1) {
  console.log(`Received: ${event.eventType}`, event.payload);

  // Acknowledge processing
  await subscription1.ack(event.globalNonce);
}
```

## 🔧 Advanced Usage

### Custom Event Serialization

```typescript
@Event({
  type: 'com.myco.order.ComplexEvent',
  revision: 1,
  contentType: 'application/protobuf'  // Use protobuf serialization
})
class ComplexEvent {
  constructor(
    public readonly data: Uint8Array,  // Raw protobuf bytes
    public readonly metadata: Map<string, any>
  ) {}
}
```

### Error Handling

```typescript
try {
  await client.appendEvents({
    tenantId: 'tenant-123',
    aggregateId: 'order-123',
    aggregateType: 'Order',
    expectedAggregateNonce: 5,
    events: [new OrderUpdated('order-123', { status: 'shipped' })]
  });
} catch (error) {
  if (error instanceof ConcurrencyError) {
    console.log('Concurrency conflict detected!');
    // Reload current state and retry
    const currentState = await loadCurrentState('order-123');
    await retryOperation(currentState);
  } else if (error instanceof ConnectionError) {
    console.log('Connection failed, will retry...');
    await retryWithBackoff();
  } else {
    console.error('Unexpected error:', error);
  }
}
```

### Connection Pooling

```typescript
// The client automatically manages connection pooling
const client = new EventStoreClient({
  endpoint: 'localhost:50051',
  connectionTimeout: 5000,
  requestTimeout: 30000,
  // Pool configuration is handled internally
});
```

### Message Context Propagation

```typescript
import { MessageContext } from '@eventstore/sdk-ts';

// Set correlation and causation IDs for request tracing
MessageContext.set({
  correlationId: 'req-123',
  causationId: 'cmd-456'
});

// All subsequent operations will include these IDs
await client.appendEvents({ /* ... */ });
```

## 🧪 Testing

### Unit Testing with Mocks

```typescript
import { EventStoreClient, MockEventStore } from '@eventstore/sdk-ts';

const mockStore = new MockEventStore();
const client = new EventStoreClient({ /* config */ }, mockStore);

// Setup mock expectations
mockStore.expectAppend({
  aggregateId: 'order-123',
  events: [/* expected events */]
});

// Run test
await client.appendEvents(/* test data */);

// Verify expectations
mockStore.verify();
```

### Integration Testing

```typescript
import { TestContainers } from '@eventstore/test-utils';

const container = await TestContainers.startEventStore();
const client = new EventStoreClient({
  endpoint: container.getEndpoint()
});

// Run integration tests against real EventStore
await client.appendEvents({ /* test data */ });
```

## 📊 Performance Considerations

### Batching Events

```typescript
// Instead of multiple small appends
await client.appendEvents({ events: [event1] });
await client.appendEvents({ events: [event2] });
await client.appendEvents({ events: [event3] });

// Batch them together
await client.appendEvents({
  events: [event1, event2, event3]
});
```

### Connection Reuse

```typescript
// Create client once, reuse across requests
const client = new EventStoreClient({ endpoint: 'localhost:50051' });

// Client maintains connection pool internally
await client.appendEvents(request1);
await client.appendEvents(request2);
```

### Subscription Optimization

```typescript
// Use specific prefixes for better performance
const subscription = await client.subscribe({
  tenantId: 'tenant-123',
  aggregateIdPrefix: 'Order-',  // Only Order events
  fromGlobalNonce: lastProcessedPosition
});
```

## 🐛 Troubleshooting

### Common Issues

**Connection Timeouts:**
```typescript
const client = new EventStoreClient({
  endpoint: 'localhost:50051',
  connectionTimeout: 10000,  // Increase if needed
  requestTimeout: 60000      // Increase for large payloads
});
```

**Memory Issues with Large Streams:**
```typescript
// Use pagination for large streams
const pageSize = 100;
let fromNonce = 1;

while (true) {
  const page = await client.readStream({
    aggregateId: 'large-aggregate',
    fromAggregateNonce: fromNonce,
    maxCount: pageSize,
    forward: true
  });

  if (page.events.length === 0) break;

  // Process page
  processEvents(page.events);

  fromNonce += page.events.length;
}
```

## 📚 Related Documentation

- **[SDK Overview](../overview/sdk-overview.md)** - General SDK architecture
- **[API Reference](../api-reference.md)** - Complete API documentation
- **[Optimistic Concurrency](../../implementation/concurrency-and-consistency.md)** - Concurrency details
- **[Event Model](../../concepts/event-model.md)** - Event structure specification
