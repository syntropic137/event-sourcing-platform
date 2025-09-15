# Event Sourcing SDKs

This directory contains language-specific SDKs that provide high-level event sourcing abstractions and patterns. These SDKs build on top of the event-store to provide developer-friendly APIs for implementing event sourcing in applications.

- [Event Sourcing SDKs](#event-sourcing-sdks)
  - [Overview](#overview)
  - [Architecture](#architecture)
  - [Available SDKs](#available-sdks)
    - [Rust SDK (`rust/`)](#rust-sdk-rust)
    - [TypeScript SDK (`typescript/`)](#typescript-sdk-typescript)
    - [Python SDK (`python/`)](#python-sdk-python)
  - [Common Patterns](#common-patterns)
    - [Aggregate Lifecycle](#aggregate-lifecycle)
    - [Repository Pattern](#repository-pattern)
    - [Event Store Integration](#event-store-integration)
  - [Development](#development)
    - [Build All SDKs](#build-all-sdks)
    - [Test All SDKs](#test-all-sdks)
    - [Run QA Checks](#run-qa-checks)
    - [SDK-Specific Commands](#sdk-specific-commands)
  - [Dependencies](#dependencies)
    - [Event Store](#event-store)
    - [Language Requirements](#language-requirements)
  - [Testing Strategy](#testing-strategy)
    - [Unit Tests](#unit-tests)
    - [Integration Tests](#integration-tests)
    - [Cross-SDK Tests](#cross-sdk-tests)
  - [Examples](#examples)
  - [Roadmap](#roadmap)
    - [Phase 1: Core Functionality](#phase-1-core-functionality)
    - [Phase 2: Advanced Features](#phase-2-advanced-features)
    - [Phase 3: Ecosystem Integration](#phase-3-ecosystem-integration)
  - [Contributing](#contributing)


## Overview

The Event Sourcing SDKs provide:

- **Aggregate Abstractions**: Base classes and traits for event-sourced aggregates
- **Command Handling**: Patterns for processing commands and emitting events
- **Event Application**: `@EventSourcingHandler` patterns for state evolution
- **Repository Pattern**: Loading and saving aggregates with optimistic concurrency
- **Projection Management**: Building and maintaining read models
- **Rich Type Safety**: Language-appropriate type systems and error handling

## Architecture

Each SDK is designed to:

1. **Connect to Event Store**: Use the gRPC API to store and retrieve events
2. **Provide Abstractions**: Hide low-level details behind developer-friendly APIs
3. **Enforce Patterns**: Guide developers toward event sourcing best practices
4. **Enable Testing**: Support unit testing aggregates without the event store
5. **Scale Gracefully**: Handle performance and concurrency concerns

## Available SDKs

### Rust SDK (`rust/`)

**Status:** 🔄 In Development  
**Target:** Native Rust applications requiring maximum performance

Features:
- Zero-cost abstractions over the event store
- Async/await support with tokio
- Type-safe event handling with enums
- Integration with Rust's ownership system
- Performance-optimized serialization

```rust
use event_sourcing::prelude::*;

#[derive(Aggregate)]
struct OrderAggregate {
    id: AggregateId,
    status: OrderStatus,
    version: u64,
}

impl OrderAggregate {
    async fn submit(&mut self, cmd: SubmitOrder) -> Result<Vec<Event>, Error> {
        if self.status != OrderStatus::New {
            return Err(Error::InvalidState("Order already submitted"));
        }
        
        Ok(vec![Event::OrderSubmitted {
            order_id: cmd.order_id,
            customer_id: cmd.customer_id,
        }])
    }

    #[event_handler]
    fn on_order_submitted(&mut self, event: OrderSubmitted) {
        self.status = OrderStatus::Submitted;
    }
}
```

### TypeScript SDK (`typescript/`)

**Status:** 🔄 In Development  
**Target:** Node.js applications and web frontends

Features:
- Full TypeScript type safety
- Decorator-based event handling
- Promise-based async operations
- Integration with popular web frameworks
- Browser and Node.js compatibility

```typescript
import { Aggregate, EventHandler, Command } from '@event-sourcing/typescript';

class OrderAggregate extends Aggregate {
  private status: OrderStatus = OrderStatus.New;

  async submit(cmd: SubmitOrder): Promise<Event[]> {
    if (this.status !== OrderStatus.New) {
      throw new Error('Order already submitted');
    }

    return [new OrderSubmitted(cmd.orderId, cmd.customerId)];
  }

  @EventHandler(OrderSubmitted)
  onOrderSubmitted(event: OrderSubmitted) {
    this.status = OrderStatus.Submitted;
  }
}
```

### Python SDK (`python/`)

**Status:** 🔄 In Development  
**Target:** Python applications with Django/FastAPI integration

Features:
- Pythonic async/await patterns
- Dataclass-based event definitions
- Integration with popular Python frameworks
- Type hints and runtime validation
- Protocol buffer integration

```python
from event_sourcing import Aggregate, event_handler
from dataclasses import dataclass

@dataclass
class OrderSubmitted:
    order_id: str
    customer_id: str

class OrderAggregate(Aggregate):
    def __init__(self):
        super().__init__()
        self.status = OrderStatus.NEW

    async def submit(self, cmd: SubmitOrder) -> list[Event]:
        if self.status != OrderStatus.NEW:
            raise ValueError("Order already submitted")
        
        return [OrderSubmitted(cmd.order_id, cmd.customer_id)]

    @event_handler
    def on_order_submitted(self, event: OrderSubmitted):
        self.status = OrderStatus.SUBMITTED
```

## Common Patterns

All SDKs implement these core patterns:

### Aggregate Lifecycle

1. **Load**: Retrieve events from store and replay to current state
2. **Command**: Process command and decide what events to emit
3. **Apply**: Apply events to evolve aggregate state
4. **Save**: Persist new events with optimistic concurrency check

### Repository Pattern

```
Repository<T> where T: Aggregate {
  async load(id: AggregateId) -> T
  async save(aggregate: T) -> Result<()>
}
```

### Event Store Integration

All SDKs use the event-store gRPC API:
- `Append` - Store new events with concurrency checks
- `ReadStream` - Load aggregate events for rehydration
- `Subscribe` - Build projections from event streams

## Development

### Build All SDKs

```bash
make build
```

### Test All SDKs

```bash
make test
```

### Run QA Checks

```bash
make qa
```

### SDK-Specific Commands

```bash
# Rust SDK
make rust
make test-rust
make qa-rust

# TypeScript SDK  
make typescript
make test-typescript
make qa-typescript

# Python SDK
make python
make test-python
make qa-python
```

## Dependencies

### Event Store

All SDKs depend on the event-store being available:

```bash
# Start event store
cd ../event-store
make run

# Or use the root makefile
cd ..
make start-services
```

### Using the gRPC Event Store from TypeScript

The TypeScript SDK includes a thin adapter over the Event Store TS gRPC client.

```ts
import { EventStoreClientFactory } from '@event-sourcing-platform/typescript';

const client = EventStoreClientFactory.createGrpcClient({ serverAddress: 'localhost:50051' });
await client.connect(); // no-op, for symmetry

// Use with RepositoryFactory or directly (basic store example)
```

### Language Requirements

- **Rust**: Latest stable Rust toolchain
- **TypeScript**: Node.js 18+ with TypeScript 5+
- **Python**: Python 3.8+ with asyncio support

## Testing Strategy

### Unit Tests
- Aggregate behavior without event store
- Command validation and event emission
- Event application and state evolution
- Repository mock implementations

### Integration Tests  
- End-to-end aggregate lifecycle
- Event store connectivity
- Concurrency and error handling
- Performance benchmarks

### Cross-SDK Tests
- Event compatibility between languages
- Protocol buffer serialization consistency
- API parity validation

## Examples

Each SDK includes example applications:

- **Basic Aggregate**: Simple order processing
- **Complex Workflow**: Multi-step business processes  
- **Projection Building**: Read model construction
- **Framework Integration**: Web framework usage

## Roadmap

### Phase 1: Core Functionality
- [x] Basic aggregate abstractions
- [ ] Repository pattern implementation
- [ ] Event store integration
- [ ] Command/event patterns

### Phase 2: Advanced Features
- [ ] Projection management
- [ ] Saga/process managers
- [ ] Event upcasting/migration
- [ ] Performance optimizations

### Phase 3: Ecosystem Integration
- [ ] Framework-specific integrations
- [ ] Middleware and plugins
- [ ] Monitoring and observability
- [ ] Developer tooling

## Contributing

Please see the main project contributing guidelines. Each SDK follows language-specific conventions:

- **Rust**: Follow Rust API guidelines and use `cargo fmt`/`cargo clippy`
- **TypeScript**: Use Prettier and ESLint with strict TypeScript settings
- **Python**: Follow PEP 8 with Black formatting and type hints

---

**The Event Sourcing SDKs provide the high-level abstractions needed to build robust event-sourced applications while maintaining the flexibility and performance of the underlying event store.**
