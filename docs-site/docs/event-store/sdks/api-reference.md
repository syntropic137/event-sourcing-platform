# Event Store SDK API Reference

This document provides a comprehensive reference for all Event Store SDK APIs across all supported languages.

## 📋 Common API Surface

All SDKs provide the same core functionality with language-appropriate idioms:

### 🔧 Client Operations

#### `appendEvents(request)`
Appends events to an aggregate with optimistic concurrency control.

**Parameters:**
- `tenantId` (string): Tenant/partition routing key
- `aggregateId` (string): Target aggregate identifier
- `aggregateType` (string): Aggregate type for routing
- `events` (Event[]): Array of events to append
- `expectedAggregateNonce` (number): Optimistic concurrency check. Use `0` for brand new streams; otherwise supply the current stream head.

**Returns:** `{ lastAggregateNonce, lastGlobalNonce }`

#### `readStream(request)`
Reads events from an aggregate stream.

**Parameters:**
- `aggregateId` (string): Target aggregate identifier
- `fromAggregateNonce` (number): Starting sequence number (inclusive)
- `maxCount` (number): Maximum events to return
- `forward` (boolean): Read direction (true = ascending, false = descending)

**Returns:** `{ events[], isEnd, nextFromAggregateNonce }`

#### `subscribe(request)`
Creates a real-time subscription to event streams.

**Parameters:**
- `tenantId` (string): Tenant/partition routing key
- `aggregateIdPrefix` (string): Filter by aggregate id prefix (optional)
- `fromGlobalNonce` (number): Starting global nonce

**Returns:** AsyncIterator of events

### 🎯 Event Definition

All SDKs use decorators/annotations for event definition:

```typescript
@Event(\{
  type: 'com.myco.domain.EventName',
  revision: 1,
  contentType: 'application/json' // optional
\})
class EventName {
  constructor(public readonly field: string) {}
}
```

## 📦 Language-Specific APIs

### TypeScript SDK

#### Installation
```bash
npm install @eventstore/sdk-ts
```

#### Client Creation
```typescript
import { EventStoreClient } from '@eventstore/sdk-ts';

const client = new EventStoreClient(\{
  endpoint: 'localhost:50051',
  credentials: \{
    username: 'user',
    password: 'pass'
  \},
  tls: \{
    caCertificate: Buffer.from(caCert),
    clientCertificate: Buffer.from(clientCert),
    clientKey: Buffer.from(clientKey)
  \}
\});
```

#### Event Decorators
```typescript
import { Event } from '@eventstore/sdk-ts';

@Event(\{
  type: 'com.myco.order.OrderCreated',
  revision: 1
\})
class OrderCreated {
  constructor(
    public readonly orderId: string,
    public readonly amount: number
  ) {}
}
```

#### Error Types
```typescript
import {
  ConcurrencyError,
  ConnectionError,
  ValidationError
} from '@eventstore/sdk-ts';

try {
  await client.appendEvents(request);
} catch (error) {
  if (error instanceof ConcurrencyError) {
    // Handle optimistic concurrency failure
  }
}
```

### Python SDK

#### Installation
```bash
pip install eventstore-sdk-py
```

#### Client Creation
```python
from eventstore import EventStoreClient

client = EventStoreClient(
    endpoint="localhost:50051",
    credentials={
        "username": "user",
        "password": "pass"
    }
)
```

#### Event Definition
```python
from eventstore import event

@event(type="com.myco.order.OrderCreated", revision=1)
@dataclass
class OrderCreated:
    order_id: str
    amount: float
```

#### Async Operations
```python
async with client:
    # All operations are async
    response = await client.append_events(request)
```

### Rust SDK

#### Installation
```bash
cargo add eventstore-sdk-rs
```

#### Client Creation
```rust
use eventstore_sdk_rs::EventStoreClient;

let client = EventStoreClient::new("localhost:50051")
    .with_credentials("user", "pass")
    .build()?;
```

#### Event Definition
```rust
use eventstore_sdk_rs::Event;

#[derive(Event)]
#[event(type = "com.myco.order.OrderCreated", revision = 1)]
struct OrderCreated {
    pub order_id: String,
    pub amount: f64,
}
```

#### Error Handling
```rust
use eventstore_sdk_rs::{Error, ConcurrencyError};

match client.append_events(request).await {
    Ok(response) => println!("Success: {:?}", response),
    Err(Error::Concurrency(err)) => {
        // Handle optimistic concurrency failure
        println!("Concurrency conflict: {:?}", err);
    }
    Err(err) => println!("Other error: {:?}", err),
}
```

## 🔐 Authentication & Security

### TLS Configuration

**TypeScript:**
```typescript
const client = new EventStoreClient({
  endpoint: 'localhost:50051',
  tls: {
    caCertificate: Buffer.from(caCert),
    clientCertificate: Buffer.from(clientCert),
    clientKey: Buffer.from(clientKey)
  }
});
```

**Python:**
```python
client = EventStoreClient(
    endpoint="localhost:50051",
    tls={
        "ca_certificate": ca_cert_bytes,
        "client_certificate": client_cert_bytes,
        "client_key": client_key_bytes
    }
)
```

**Rust:**
```rust
let client = EventStoreClient::new("localhost:50051")
    .with_tls_config(TlsConfig {
        ca_certificate: ca_cert,
        client_certificate: client_cert,
        client_key: client_key,
    })
    .build()?;
```

### Connection Management

All SDKs provide:
- Automatic connection pooling
- Connection retry logic
- Configurable timeouts
- Health checks

## 📊 Performance & Monitoring

### Metrics (TypeScript)
```typescript
import { MetricsCollector } from '@eventstore/sdk-ts';

const metrics = new MetricsCollector();
client.registerMetrics(metrics);

// Access metrics
console.log('Request latency:', metrics.getLatency());
console.log('Error rate:', metrics.getErrorRate());
```

### Observability (All Languages)
- Request/response logging
- Performance metrics
- Error tracking
- Health status endpoints

## 🔄 Message Context

### Correlation & Causation IDs

**TypeScript:**
```typescript
import { MessageContext } from '@eventstore/sdk-ts';

MessageContext.set({
  correlationId: 'req-123',
  causationId: 'cmd-456'
});
```

**Python:**
```python
from eventstore import MessageContext

MessageContext.set(
    correlation_id="req-123",
    causation_id="cmd-456"
)
```

**Rust:**
```rust
use eventstore_sdk_rs::MessageContext;

MessageContext::set(
    correlation_id: "req-123",
    causation_id: "cmd-456"
);
```

## 🧪 Testing APIs

### Mock Clients

**TypeScript:**
```typescript
import { MockEventStore } from '@eventstore/sdk-ts';

const mockStore = new MockEventStore();
const client = new EventStoreClient(config, mockStore);

// Setup expectations
mockStore.expect_append(request);
```

**Python:**
```python
from eventstore.testing import MockEventStore

mock_store = MockEventStore()
client = EventStoreClient(config, mock_store)

# Setup expectations
mock_store.expect_append(request)
```

**Rust:**
```rust
use eventstore_sdk_rs::testing::MockEventStore;

let mock_store = MockEventStore::new();
let client = EventStoreClient::new(config).with_store(mock_store);

// Setup expectations
mock_store.expect_append(request);
```

## 📋 Type Definitions

### Common Types

```typescript
interface EventStoreClientConfig {
  endpoint: string;
  credentials?: {
    username: string;
    password: string;
  };
  tls?: TlsConfig;
  connectionTimeout?: number;
  requestTimeout?: number;
}

interface AppendRequest {
  tenantId: string;
  aggregateId: string;
  aggregateType: string;
  events: Event[];
  expectedAggregateNonce: number;
}

interface ReadStreamRequest {
  tenantId: string;
  aggregateId: string;
  fromAggregateNonce: number;
  maxCount: number;
  forward: boolean;
}

interface SubscribeRequest {
  tenantId: string;
  aggregateIdPrefix?: string;
  fromGlobalNonce: number;
}
```

## 🚨 Error Reference

### Common Errors

- **ConcurrencyError**: Optimistic concurrency conflict
- **ConnectionError**: Network or connection issues
- **ValidationError**: Invalid request parameters
- **TimeoutError**: Request exceeded timeout
- **AuthenticationError**: Invalid credentials
- **AuthorizationError**: Insufficient permissions

### Error Handling Patterns

```typescript
// TypeScript error handling
try {
  await client.appendEvents(request);
} catch (error) {
  switch (error.constructor) {
    case ConcurrencyError:
      await handleConcurrencyConflict(error, request);
      break;
    case ConnectionError:
      await retryWithBackoff(request);
      break;
    default:
      throw error;
  }
}
```

## 📚 Additional Resources

- **[SDK Overview](./overview/sdk-overview.md)** - Architecture and workflow
- **[TypeScript SDK](./typescript/typescript-sdk.md)** - Complete TypeScript guide
- **[Python SDK](./python/python-sdk.md)** - Python implementation guide
- **[Rust SDK](./rust/rust-sdk.md)** - Rust native implementation
- **[Event Model](../concepts/event-model.md)** - Event envelope specification
- **[Optimistic Concurrency](../implementation/concurrency-and-consistency.md)** - Concurrency details
