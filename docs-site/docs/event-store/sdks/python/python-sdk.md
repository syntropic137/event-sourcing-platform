# Python SDK

The Python SDK provides an async-first, type-safe interface for interacting with the Event Store. Built with modern Python features and optimized for asyncio applications.

## 🚀 Installation

```bash
pip install eventstore-sdk-py
```

## 🎯 Quick Start

```python
import asyncio
from dataclasses import dataclass
from eventstore import EventStoreClient, event

# Define your events
@event(type="com.myco.order.OrderCreated", revision=1)
@dataclass
class OrderCreated:
    order_id: str
    amount: float
    currency: str = "USD"

@event(type="com.myco.order.OrderShipped", revision=1)
@dataclass
class OrderShipped:
    order_id: str
    tracking_number: str

async def main():
    # Create client
    client = EventStoreClient(
        endpoint="localhost:50051",
        # credentials={"username": "user", "password": "pass"}  # Add auth as needed
    )

    # Append events
    await client.append_events(
        tenant_id="tenant-123",
        aggregate_id="order-123",
        aggregate_type="Order",
        expected_aggregate_nonce=0,  # First event -> stream must not exist
        events=[
            OrderCreated(order_id="order-123", amount=99.99, currency="USD"),
            OrderShipped(order_id="order-123", tracking_number="TRK123456")
        ]
    )

if __name__ == "__main__":
    asyncio.run(main())
```

## 📚 API Reference

### Client Configuration

```python
from eventstore import EventStoreClient

client = EventStoreClient(
    endpoint="localhost:50051",
    credentials={
        "username": "user",
        "password": "pass"
    },
    tls={
        "ca_certificate": ca_cert_bytes,
        "client_certificate": client_cert_bytes,
        "client_key": client_key_bytes
    },
    connection_timeout=5.0,
    request_timeout=30.0
)
```

### Event Definition

```python
from eventstore import event
from dataclasses import dataclass
from typing import Optional

@event(
    type="com.myco.order.OrderCreated",
    revision=1,
    content_type="application/json"  # optional
)
@dataclass
class OrderCreated:
    order_id: str
    amount: float
    currency: Optional[str] = "USD"
```

### Append Operations

```python
# Append with optimistic concurrency (expect current head to be 2)
await client.append_events(
    tenant_id="tenant-123",
    aggregate_id="order-123",
    aggregate_type="Order",
    expected_aggregate_nonce=2,
    events=[OrderUpdated(order_id="order-123", status="confirmed")]
)

# Start a new stream (0 means the aggregate must not exist yet)
await client.append_events(
    tenant_id="tenant-123",
    aggregate_id="order-456",
    aggregate_type="Order",
    expected_aggregate_nonce=0,
    events=[OrderCreated(order_id="order-456", amount=149.99)]
)
```

### Read Operations

```python
# Read entire stream forward
stream = await client.read_stream(
    aggregate_id="order-123",
    from_aggregate_nonce=1,  # Start from event 1
    forward=True,            # Read forward (ascending)
    max_count=100            # Limit results
)

# Read backward from latest
recent_events = await client.read_stream(
    aggregate_id="order-123",
    from_aggregate_nonce=10,  # Start from event 10
    forward=False,            # Read backward (descending)
    max_count=5               # Get last 5 events
)
```

### Subscription Operations

```python
# Subscribe to all events from beginning
subscription1 = await client.subscribe(
    tenant_id="tenant-123",
    from_global_nonce=0  # Start from beginning
)

# Subscribe to specific aggregate id prefix
subscription2 = await client.subscribe(
    tenant_id="tenant-123",
    aggregate_id_prefix="Order-",  # Only Order aggregates
    from_global_nonce=1000         # Start from global position 1000
)

# Process events
async for event in subscription1:
    print(f"Received: {event.event_type}", event.payload)

    # Acknowledge processing
    await subscription1.ack(event.global_nonce)
```

## 🔧 Advanced Usage

### Custom Event Serialization

```python
from eventstore import event
import json

@event(
    type="com.myco.order.ComplexEvent",
    revision=1,
    content_type="application/protobuf"  # Use protobuf serialization
)
@dataclass
class ComplexEvent:
    data: bytes  # Raw protobuf bytes
    metadata: dict[str, any]
```

### Error Handling

```python
from eventstore import ConcurrencyError, ConnectionError

try:
    await client.append_events(request)
except ConcurrencyError as e:
    print("Concurrency conflict detected!")
    # Reload current state and retry
    current_state = await load_current_state("order-123")
    await retry_operation(current_state)
except ConnectionError as e:
    print("Connection failed, will retry...")
    await retry_with_backoff()
except Exception as e:
    print(f"Unexpected error: {e}")
```

### Context Managers

```python
# Client automatically manages connections
async with EventStoreClient(endpoint="localhost:50051") as client:
    await client.append_events(request)
```

### Message Context Propagation

```python
from eventstore import MessageContext

# Set correlation and causation IDs for request tracing
MessageContext.set(
    correlation_id="req-123",
    causation_id="cmd-456"
)

# All subsequent operations will include these IDs
await client.append_events(request)
```

## 🧪 Testing

### Unit Testing with Mocks

```python
import pytest
from eventstore.testing import MockEventStore

@pytest.fixture
def mock_store():
    return MockEventStore()

@pytest.fixture
def client(mock_store):
    return EventStoreClient(endpoint="mock", store=mock_store)

def test_append_events(client, mock_store):
    # Setup mock expectations
    mock_store.expect_append({
        "aggregate_id": "order-123",
        "events": [/* expected events */]
    })

    # Run test
    await client.append_events(request)

    # Verify expectations
    mock_store.verify()
```

### Integration Testing

```python
import pytest_asyncio
from eventstore.testing import EventStoreContainer

@pytest.fixture
async def eventstore_container():
    container = EventStoreContainer()
    await container.start()
    yield container
    await container.stop()

@pytest.fixture
async def client(eventstore_container):
    return EventStoreClient(
        endpoint=eventstore_container.get_endpoint()
    )

async def test_integration(client):
    # Run integration tests against real EventStore
    await client.append_events(request)
    stream = await client.read_stream(aggregate_id="test-123")
    assert len(stream.events) == 1
```

## 📊 Performance Considerations

### Connection Pooling

```python
# The client automatically manages connection pooling
client = EventStoreClient(
    endpoint="localhost:50051",
    # Pool configuration is handled internally
)
```

### Batching Events

```python
# Instead of multiple small appends
await client.append_events({"events": [event1]})
await client.append_events({"events": [event2]})
await client.append_events({"events": [event3]})

# Batch them together
await client.append_events({
    "events": [event1, event2, event3]
})
```

### Async Best Practices

```python
# Use asyncio.gather for concurrent operations
results = await asyncio.gather(
    client.append_events(request1),
    client.append_events(request2),
    client.read_stream(aggregate_id="agg1"),
    client.read_stream(aggregate_id="agg2")
)
```

## 🐛 Troubleshooting

### Common Issues

**Connection Pool Exhaustion:**
```python
# Increase connection pool size
client = EventStoreClient(
    endpoint="localhost:50051",
    max_connections=20  # Increase if needed
)
```

**Timeout Issues:**
```python
# Adjust timeouts for your use case
client = EventStoreClient(
    endpoint="localhost:50051",
    connection_timeout=10.0,  # Connection timeout
    request_timeout=60.0      # Request timeout
)
```

**Memory Issues with Large Streams:**
```python
# Use pagination for large streams
async def read_large_stream(aggregate_id: str):
    page_size = 100
    from_nonce = 1

    while True:
        page = await client.read_stream(
            aggregate_id=aggregate_id,
            from_aggregate_nonce=from_nonce,
            max_count=page_size,
            forward=True
        )

        if not page.events:
            break

        # Process page
        await process_events(page.events)
        from_nonce += len(page.events)
```

## 📚 Related Documentation

- **[SDK Overview](../overview/sdk-overview.md)** - General SDK architecture
- **[API Reference](../api-reference.md)** - Complete API documentation
- **[TypeScript SDK](../typescript/typescript-sdk.md)** - TypeScript implementation
- **[Optimistic Concurrency](../../implementation/concurrency-and-consistency.md)** - Concurrency details
- **[Event Model](../../concepts/event-model.md)** - Event envelope specification
