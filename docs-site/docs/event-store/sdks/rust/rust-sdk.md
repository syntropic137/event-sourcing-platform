# Rust SDK

The Rust SDK provides a native, zero-copy, async-first interface for interacting with the Event Store. Built with Rust's ownership system and optimized for performance and safety.

## 🚀 Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
eventstore-sdk-rs = "0.1"
```

## 🎯 Quick Start

```rust
use eventstore_sdk_rs::{EventStoreClient, Event};
use serde::{Deserialize, Serialize};

#[derive(Event, Serialize, Deserialize)]
#[event(type = "com.myco.order.OrderCreated", revision = 1)]
struct OrderCreated {
    pub order_id: String,
    pub amount: f64,
    pub currency: String,
}

#[derive(Event, Serialize, Deserialize)]
#[event(type = "com.myco.order.OrderShipped", revision = 1)]
struct OrderShipped {
    pub order_id: String,
    pub tracking_number: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create client
    let client = EventStoreClient::new("localhost:50051")
        // .with_credentials("user", "pass")  // Add auth as needed
        .build()?;

    // Append events
    client.append_events(
        "order-123",
        "Order",
        vec![
            Event::new(OrderCreated {
                order_id: "order-123".to_string(),
                amount: 99.99,
                currency: "USD".to_string(),
            }),
            Event::new(OrderShipped {
                order_id: "order-123".to_string(),
                tracking_number: "TRK123456".to_string(),
            }),
        ],
        eventstore_sdk_rs::ExpectedVersion::Exact(0), // First event, expect version 0
    ).await?;

    Ok(())
}
```

## 📚 API Reference

### Client Configuration

```rust
use eventstore_sdk_rs::EventStoreClient;

let client = EventStoreClient::new("localhost:50051")
    .with_credentials("user", "pass")
    .with_tls_config(TlsConfig {
        ca_certificate: Some(ca_cert),
        client_certificate: Some(client_cert),
        client_key: Some(client_key),
    })
    .with_connection_timeout(Duration::from_secs(5))
    .with_request_timeout(Duration::from_secs(30))
    .build()?;
```

### Event Definition

```rust
use eventstore_sdk_rs::Event;
use serde::{Deserialize, Serialize};

#[derive(Event, Serialize, Deserialize)]
#[event(
    type = "com.myco.order.OrderCreated",
    revision = 1,
    content_type = "application/json"  // optional
)]
struct OrderCreated {
    pub order_id: String,
    pub amount: f64,
    pub currency: String,
}
```

### Append Operations

```rust
use eventstore_sdk_rs::ExpectedVersion;

// Append with exact version expectation
client.append_events(
    "order-123",
    "Order",
    vec![Event::new(order_updated)],
    ExpectedVersion::Exact(2), // Expect current version to be 2
).await?;

// Append with no concurrency check
client.append_events(
    "order-123",
    "Order",
    vec![Event::new(order_cancelled)],
    ExpectedVersion::Any,
).await?;

// Append only if aggregate doesn't exist
client.append_events(
    "order-456",
    "Order",
    vec![Event::new(order_created)],
    ExpectedVersion::NoAggregate,
).await?;
```

### Read Operations

```rust
// Read entire stream forward
let stream = client.read_stream(
    "order-123",
    Some(1),     // Start from event 1
    Some(100),   // Limit to 100 events
    true,        // Forward (ascending)
).await?;

// Read backward from latest
let recent_events = client.read_stream(
    "order-123",
    Some(10),    // Start from event 10
    Some(5),     // Get last 5 events
    false,       // Backward (descending)
).await?;
```

### Subscription Operations

```rust
use futures::StreamExt;

// Subscribe to all events from beginning
let mut subscription1 = client.subscribe(
    None,           // No aggregate prefix filter
    Some(0),        // Start from global nonce 0
).await?;

// Subscribe to specific aggregate type
let mut subscription2 = client.subscribe(
    Some("Order-"),  // Only Order aggregates
    Some(1000),      // Start from global nonce 1000
).await?;

// Process events
while let Some(event) = subscription1.next().await {
    println!("Received: {}", event.event_type);
    println!("Payload: {:?}", event.payload);

    // Acknowledge processing
    subscription1.ack(event.global_nonce).await?;
}
```

## 🔧 Advanced Usage

### Custom Event Serialization

```rust
use eventstore_sdk_rs::Event;
use prost::Message; // For protobuf

#[derive(Event, Message)]
#[event(
    type = "com.myco.order.ComplexEvent",
    revision = 1,
    content_type = "application/protobuf"
)]
struct ComplexEvent {
    #[prost(bytes, tag = "1")]
    pub data: Vec<u8>,  // Raw protobuf bytes
    #[prost(map = "string, message", tag = "2")]
    pub metadata: std::collections::HashMap<String, prost_types::Value>,
}
```

### Error Handling

```rust
use eventstore_sdk_rs::{Error, ConcurrencyError};

match client.append_events(request).await {
    Ok(response) => {
        println!(
            "Success! Last committed nonce: {}",
            response.last_aggregate_nonce
        );
    }
    Err(Error::Concurrency(err)) => {
        println!("Concurrency conflict: {:?}", err);
        // Reload current state and retry
        let current_state = load_current_state("order-123").await?;
        retry_operation(current_state).await?;
    }
    Err(Error::Connection(err)) => {
        println!("Connection error: {:?}", err);
        // Retry with backoff
        retry_with_backoff(request).await?;
    }
    Err(err) => {
        println!("Other error: {:?}", err);
    }
}
```

### Connection Pooling

```rust
// Client automatically manages connection pooling
let client = EventStoreClient::new("localhost:50051")
    .with_max_connections(10)  // Configure pool size
    .build()?;

// Connection pool is managed internally
// No manual connection management needed
```

### Message Context Propagation

```rust
use eventstore_sdk_rs::MessageContext;

// Set correlation and causation IDs for request tracing
MessageContext::set(
    "req-123".to_string(),
    Some("cmd-456".to_string()),
);

// All subsequent operations will include these IDs
client.append_events(request).await?;
```

## 🧪 Testing

### Unit Testing with Mocks

```rust
use eventstore_sdk_rs::testing::MockEventStore;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_append_events() {
        let mock_store = MockEventStore::new();
        let client = EventStoreClient::new("mock")
            .with_store(mock_store.clone())
            .build()
            .unwrap();

        // Setup mock expectations
        mock_store.expect_append(|req| {
            req.aggregate_id == "order-123" &&
            req.events.len() == 1
        });

        // Run test
        let result = client.append_events(/* test data */).await;

        // Verify expectations
        assert!(result.is_ok());
        mock_store.verify();
    }
}
```

### Integration Testing

```rust
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;

#[tokio::test]
async fn test_integration() {
    // Start EventStore in test container
    let container = Postgres::default()
        .with_tag("latest")
        .start()
        .await
        .unwrap();

    let endpoint = format!(
        "localhost:{}",
        container.get_host_port_ipv4(50051).await.unwrap()
    );

    // Create client
    let client = EventStoreClient::new(&endpoint).build().unwrap();

    // Run integration tests
    client.append_events(/* test data */).await.unwrap();
    let stream = client.read_stream("test-123", None, None, true).await.unwrap();
    assert_eq!(stream.events.len(), 1);
}
```

## 📊 Performance Considerations

### Zero-Copy Operations

```rust
// The SDK uses zero-copy deserialization where possible
let stream = client.read_stream(aggregate_id, None, None, true).await?;

// Events are deserialized lazily, avoiding unnecessary allocations
for event in stream.events {
    // Only deserialize when needed
    let order: OrderCreated = event.deserialize()?;
    process_order(&order);
}
```

### Connection Reuse

```rust
// Create client once, clone for different operations
let client = EventStoreClient::new("localhost:50051").build()?;

// Client maintains connection pool internally
// All operations share the same connection pool
let result1 = client.append_events(request1).await?;
let result2 = client.append_events(request2).await?;
```

### Batch Operations

```rust
// Batch multiple events for better performance
let events = vec![
    Event::new(event1),
    Event::new(event2),
    Event::new(event3),
];

client.append_events(
    "order-123",
    "Order",
    events,
    ExpectedVersion::Exact(0)
).await?;
```

## 🐛 Troubleshooting

### Common Issues

**Compilation Errors:**
```rust
// Ensure all event types implement required traits
#[derive(Event, Serialize, Deserialize, Debug, Clone)]
#[event(type = "com.myco.order.OrderCreated", revision = 1)]
struct OrderCreated {
    // Fields must be public for serialization
    pub order_id: String,
    pub amount: f64,
}
```

**Memory Issues:**
```rust
// Use streaming for large aggregates
let mut stream = client.read_stream(
    "large-aggregate",
    Some(1),
    Some(100),  // Process in chunks
    true
).await?;

// Process chunk by chunk
while let Some(chunk) = stream.next().await {
    process_chunk(&chunk.events).await?;
}
```

**Connection Pool Exhaustion:**
```rust
// Increase connection pool size for high throughput
let client = EventStoreClient::new("localhost:50051")
    .with_max_connections(50)  // Increase for high load
    .with_connection_timeout(Duration::from_secs(10))
    .build()?;
```

## 🏎️ Performance Optimization

### Custom Allocators

```rust
// Use jemallocator for better performance
use jemallocator::Jemalloc;

#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;
```

### Async Runtime Tuning

```rust
// Configure Tokio runtime for optimal performance
#[tokio::main(flavor = "multi_thread", worker_threads = 8)]
async fn main() {
    // Your application code
}
```

### Metrics and Monitoring

```rust
use eventstore_sdk_rs::metrics::MetricsCollector;

// Enable metrics collection
let metrics = MetricsCollector::new();
client.register_metrics(metrics);

// Access metrics
println!("Request latency: {:?}", metrics.request_latency());
println!("Error rate: {}", metrics.error_rate());
```

## 📚 Related Documentation

- **[SDK Overview](../overview/sdk-overview.md)** - General SDK architecture
- **[API Reference](../api-reference.md)** - Complete API documentation
- **[TypeScript SDK](../typescript/typescript-sdk.md)** - TypeScript implementation
- **[Python SDK](../python/python-sdk.md)** - Python implementation
- **[Optimistic Concurrency](../../implementation/concurrency-and-consistency.md)** - Concurrency details
- **[Event Model](../../concepts/event-model.md)** - Event envelope specification
