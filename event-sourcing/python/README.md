# Python Event Sourcing SDK

High-level abstractions for building event-sourced systems on top of the platform event store.

## Features

- ✅ **Type-Safe** - Full type hints with Pydantic validation
- ✅ **Decorator-Based** - Clean syntax with `@event_sourcing_handler` decorators
- ✅ **Async-First** - Modern async/await patterns throughout
- ✅ **VSA Compatible** - Works with VSA CLI for code generation
- ✅ **Optimistic Concurrency** - Built-in version conflict detection
- ✅ **uv Compatible** - Fast dependency management with uv

## Installation

### Using uv (recommended)

```bash
uv add event-sourcing-python
```

### Using pip

```bash
pip install event-sourcing-python
```

## Quick Start

### Define Events

```python
from event_sourcing import DomainEvent

class AccountCredited(DomainEvent):
    event_type = "AccountCredited"
    amount: float
    balance: float
```

### Create Aggregates

```python
from event_sourcing import AggregateRoot, event_sourcing_handler

class AccountAggregate(AggregateRoot[AccountEvent]):
    def __init__(self) -> None:
        super().__init__()
        self.balance = 0.0
    
    def get_aggregate_type(self) -> str:
        return "Account"
    
    def credit(self, amount: float) -> None:
        if amount <= 0:
            raise ValueError("Amount must be positive")
        self._raise_event(AccountCredited(
            amount=amount,
            balance=self.balance + amount
        ))
    
    @event_sourcing_handler("AccountCredited")
    def on_credited(self, event: AccountCredited) -> None:
        self.balance = event.balance
```

### Use with Repository

```python
from event_sourcing import RepositoryFactory, EventStoreClientFactory

# For testing with in-memory client
client = EventStoreClientFactory.create_memory_client()
await client.connect()

repo = RepositoryFactory(client).create_repository(
    AccountAggregate,
    "Account"
)

# Create and save
account = AccountAggregate()
account.credit(100.0)
await repo.save(account)

# Load and modify
loaded = await repo.load(account.id)
loaded.credit(50.0)
await repo.save(loaded)
```

### Use with gRPC (Production)

```python
from event_sourcing import RepositoryFactory, EventStoreClientFactory

# Connect to gRPC event store
client = EventStoreClientFactory.create_grpc_client(
    host="eventstore.example.com",
    port=50051,
    tenant_id="my-tenant"
)
await client.connect()

# Use the same repository pattern
repo = RepositoryFactory(client).create_repository(
    AccountAggregate,
    "Account"
)

# All operations work the same
account = AccountAggregate()
account.credit(100.0)
await repo.save(account)
```

## VSA Integration

Generate Python code with VSA CLI:

```bash
# Initialize Python project
vsa init --language python --root src/contexts

# Generate feature
vsa generate --context accounts --feature credit-account --interactive

# Validate structure
vsa validate
```

## Testing

```bash
# Run all tests
make test

# Run unit tests only
make test-unit

# Run with coverage
uv run pytest --cov
```

## Type Checking

```bash
# Run mypy
make type-check
```

## Linting & Formatting

```bash
# Lint code
make lint

# Format code
make format
```

## Development

### Setup

```bash
# Install dependencies
make setup

# Or with uv directly
uv sync --all-extras
```

### QA

```bash
# Quick QA (lint + type-check + unit tests)
make qa-fast

# Full QA (includes integration tests)
make qa
```

## Architecture

The SDK follows Domain-Driven Design and Event Sourcing patterns:

- **Aggregates** - Consistency boundaries that process commands and emit events
- **Events** - Immutable facts representing state changes
- **Repositories** - Load and save aggregates with optimistic concurrency
- **Commands** - Intentions to change state
- **Queries** - Read models for projections

## Current Status

**Milestone 1 Complete** ✅
- Core SDK foundation (events, aggregates, decorators)
- Type-safe with Pydantic and mypy
- Comprehensive unit tests
- QA tooling (ruff, black, pytest)

**Milestone 2 Complete** ✅
- Repository pattern implementation
- In-memory event store client for testing
- Optimistic concurrency control
- Integration tests

**Milestone 3 Complete** ✅
- gRPC event store client for production
- Proto stub generation
- Full event serialization/deserialization
- Multi-tenancy support

**Coming Next:**
- Milestone 4: CQRS patterns (commands/queries)
- Milestone 5: Projections for read models
- Milestone 6: VSA Python templates
- Milestone 7: Banking system example

## Documentation

- [Project Plan](../../PROJECT-PLAN_20251105_python-event-sourcing-sdk.md)
- [TypeScript SDK](../typescript/) - Reference implementation
- [Event Store](../../event-store/) - gRPC event store

## License

MIT

