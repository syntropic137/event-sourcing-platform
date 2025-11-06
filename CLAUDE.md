# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Repository Overview

This is a comprehensive event sourcing platform organized around Domain-Driven Design principles. The platform provides both a low-level event store (Rust) and high-level event sourcing abstractions (multi-language SDKs), plus a Vertical Slice Architecture (VSA) manager tool for code organization.

## Key Architecture Principles

1. **Domain Focus**: Event Store and Event Sourcing define the rules of the event sourcing domain
2. **Living Documentation**: Examples demonstrate real applications with actual databases (no mocks)
3. **Progressive Learning**: Examples build from basic concepts (001) to complete systems (009)
4. **Multi-Language**: Rust for performance-critical components, TypeScript as primary SDK, Python planned
5. **Bounded Contexts**: VSA tool enforces vertical slice architecture with bounded contexts

## Project Structure

```
event-sourcing-platform/
├── event-store/              # Rust event store with gRPC API
│   ├── eventstore-core/         # Traits, errors, protobuf types
│   ├── eventstore-proto/        # Protobuf definitions
│   ├── eventstore-backend-memory/  # In-memory backend
│   ├── eventstore-backend-postgres/  # Postgres backend
│   ├── eventstore-bin/          # gRPC server binary
│   └── sdks/                    # Client SDKs (TS, Python, Rust)
├── event-sourcing/           # Event sourcing SDKs and patterns
│   ├── typescript/              # Primary SDK with decorators
│   └── rust/                    # Alpha SDK
├── vsa/                      # Vertical Slice Architecture Manager
│   ├── vsa-core/                # Core validation logic (Rust)
│   ├── vsa-cli/                 # CLI tool
│   └── vsa-wasm/                # WASM bindings for Node.js
├── examples/                 # TypeScript "living documentation"
│   ├── 001-basic-store-ts/      # Direct event store usage
│   ├── 002-simple-aggregate-ts/ # Aggregate decorators
│   ├── 003-multiple-aggregates-ts/
│   ├── 004-cqrs-patterns-ts/
│   ├── 005-projections-ts/
│   ├── 006-event-bus-ts/
│   ├── 007-inventory-complete-ts/
│   ├── 008-observability-ts/
│   └── 009-web-dashboard-ts/
├── dev-tools/                # Development infrastructure scripts
├── docs-site/                # Docusaurus documentation
└── infra-as-code/            # Terraform + Ansible (WIP)
```

## Essential Commands

### Building

```bash
# Build everything (Rust → Python → TypeScript)
make build

# Build specific components
make build-rust        # Event store + Rust SDKs + VSA
make build-typescript  # All TypeScript packages (via Turborepo)
make build-python      # Python SDKs (via uv)

# Build individual contexts
cd event-store && make build
cd event-sourcing && make build
cd vsa && cargo build --workspace
```

### Testing

```bash
# Run all tests
make test

# Component-specific tests
make test-event-store
make test-event-sourcing
make test-examples

# Fast tests (uses dev infrastructure, no testcontainers)
make test-fast
cd event-store && make test-fast
```

### Quality Assurance

```bash
# Fast QA (static checks + unit tests, no coverage)
make qa
make qa-fast

# Full QA (includes integration tests + coverage)
make qa-full

# Component-specific QA
make qa-event-store      # Auto-detects dev infrastructure
make qa-event-sourcing
make qa-examples
```

**Important QA Notes:**
- `make qa` and `make qa-fast` skip slow tests and coverage reports
- `make qa-full` runs complete suite including coverage (slower)
- Event store QA automatically detects running dev infrastructure and uses it instead of testcontainers for faster execution

### Development Infrastructure

The platform uses a fast development infrastructure system (via `dev-tools/dev` script):

```bash
# Initialize dev environment (first time only)
make dev-init

# Start infrastructure (Postgres + Redis)
make dev-start

# Stop infrastructure
make dev-stop

# Restart infrastructure
make dev-restart

# Clean all containers and data
make dev-clean

# Check infrastructure status
make dev-status

# View logs
make dev-logs        # All services
make dev-logs postgres  # Specific service

# Shell into container
make dev-shell postgres
```

**Infrastructure Benefits:**
- Project-isolated containers with unique hashes
- Persistent data across restarts
- Automatic port allocation
- Fast test execution (no testcontainers startup overhead)
- Shared across event-store tests and examples

### Running Examples

```bash
# Run specific examples (automatically builds dependencies)
make examples-001    # Basic event store usage
make examples-002    # Simple aggregate
make examples-003    # Multiple aggregates
make examples-004    # CQRS patterns
make examples-005    # Projections
make examples-006    # Event bus
make examples-007    # Inventory complete
make examples-008    # Observability
make examples-009    # Web dashboard

# Direct pnpm execution (more control)
pnpm --filter ./examples/001-basic-store-ts run start
pnpm --filter ./examples/001-basic-store-ts run start -- --memory  # Use in-memory client
```

### Event Store Server

```bash
# Run with memory backend (default)
cd event-store && make run

# Run with Postgres backend
cd event-store
BACKEND=postgres DATABASE_URL=postgres://dev:dev@localhost:5432/dev make run

# Smoke test server
cd event-store && make smoke

# Run in background
cd event-store && make run-bg
cd event-store && make stop
```

### TypeScript Development

```bash
# Install dependencies (workspace root)
pnpm -w install

# Build TypeScript packages (uses Turborepo)
pnpm build

# Build docs site only
pnpm build:docs

# Run all tests
pnpm test

# Lint (continues on error)
pnpm lint

# Clean build artifacts
pnpm clean
```

### Documentation

```bash
# Start Docusaurus dev server
make docs
make docs-start      # Same as docs

# Build static site
make docs-build

# Serve built site
make docs-serve

# Generate LLM-friendly API docs
make docs-generate-llm
```

### VSA (Vertical Slice Architecture Manager)

```bash
# Build VSA CLI
cd vsa && cargo build --release

# Example VSA usage (in a project with vsa.yaml)
vsa init --language typescript
vsa generate --context orders --feature place-order
vsa validate
vsa validate --watch
vsa list
vsa manifest
```

## Key Concepts

### Event Store Architecture

The Rust event store is the foundation:
- **Backend-agnostic**: Traits in `eventstore-core/` allow pluggable backends
- **Optimistic Concurrency**: Client-proposed sequence numbers (true OCC)
- **gRPC API**: Defined in `eventstore-proto/proto/`
- **Multiple Backends**: Memory (dev/test), Postgres (production)

### Event Sourcing SDK Patterns (TypeScript)

The TypeScript SDK (`event-sourcing/typescript/`) provides:

1. **AggregateRoot**: Base class for aggregates with automatic event replay
2. **@CommandHandler**: Decorator for business logic/validation methods that emit events
3. **@EventSourcingHandler**: Decorator for state mutation methods (replays events)
4. **Repository Pattern**: `RepositoryFactory` creates repositories with OCC tracking
5. **Concurrency Control**: `ConcurrencyConflictError` on stale aggregate saves
6. **Event Bus**: Cross-context communication via integration events

**Command and Event Pattern:**
```typescript
// Command (class with aggregateId)
class PlaceOrderCommand {
  constructor(
    public readonly aggregateId: string,
    public readonly items: Item[]
  ) {}
}

// Event
class OrderPlacedEvent extends BaseDomainEvent {
  readonly eventType = 'OrderPlaced' as const;
  readonly schemaVersion = 1 as const;
  
  constructor(public items: Item[], public totalAmount: number) {
    super();
  }
}

// Aggregate with @CommandHandler and @EventSourcingHandler
@Aggregate('Order')
class OrderAggregate extends AggregateRoot<OrderEvent> {
  private status = 'pending';
  private items: Item[] = [];

  getAggregateType(): string { return 'Order'; }

  // COMMAND HANDLER - Business logic and validation
  @CommandHandler('PlaceOrderCommand')
  placeOrder(command: PlaceOrderCommand): void {
    if (command.items.length === 0) {
      throw new Error('Order must have items');
    }
    if (this.id !== null) {
      throw new Error('Order already placed');
    }
    
    this.initialize(command.aggregateId);
    const totalAmount = command.items.reduce((sum, item) => sum + item.price, 0);
    this.apply(new OrderPlacedEvent(command.items, totalAmount));
  }

  // EVENT SOURCING HANDLER - State updates only (no validation)
  @EventSourcingHandler('OrderPlaced')
  private onPlaced(event: OrderPlacedEvent): void {
    this.status = 'placed';
    this.items = event.items;
  }
}

// Usage: Command Bus dispatches to aggregate
const aggregate = new OrderAggregate();
const command = new PlaceOrderCommand('order-123', [...items]);
aggregate.handleCommand(command);  // Dispatches to @CommandHandler
await repository.save(aggregate);
```

### Vertical Slice Architecture (VSA)

The VSA tool enforces:
- **Vertical Slices**: Each feature is self-contained (command, event, aggregate, tests)
- **Bounded Contexts**: Explicit boundaries via `vsa.yaml`
- **Integration Events**: Single source of truth in `_shared/integration-events/`
- **No Cross-Context Imports**: Contexts communicate only via integration events
- **Commands as Classes**: Commands must be classes with `aggregateId` property
- **Aggregates Handle Commands**: Use `@CommandHandler` on aggregate methods (no separate handler classes)

**Standard VSA Structure:**
```
src/contexts/orders/
├── place-order/
│   ├── PlaceOrderCommand.ts      # Command class with aggregateId
│   ├── OrderPlacedEvent.ts       # Event class extending BaseDomainEvent
│   ├── OrderAggregate.ts         # Aggregate with @CommandHandler methods
│   └── PlaceOrder.test.ts        # Tests aggregate directly
└── _subscribers/
    └── PaymentProcessedSubscriber.ts

src/_shared/integration-events/
└── orders/
    └── OrderPlaced.ts              # Integration event for other contexts
```

**Key Pattern Changes:**
- ❌ **OLD**: Separate `PlaceOrderHandler.ts` class
- ✅ **NEW**: `@CommandHandler` decorator on aggregate method
- Commands are classes (not interfaces) with `aggregateId`
- Tests call `aggregate.handleCommand(command)` directly
- Aggregates contain all business logic for their commands

## Development Workflow

### Adding New Features

1. **Start dev infrastructure** (if using Postgres):
   ```bash
   make dev-start
   ```

2. **For event store changes**:
   - Modify protobuf in `event-store/eventstore-proto/proto/`
   - Regenerate: `cd event-store && make gen-ts` (or `gen-py`)
   - Update backend implementations
   - Add tests in `eventstore-bin/tests/`
   - Run: `make qa-event-store`

3. **For TypeScript SDK changes**:
   - Modify `event-sourcing/typescript/src/`
   - Add tests in `event-sourcing/typescript/tests/`
   - Build: `cd event-sourcing/typescript && make build`
   - Test: `make test-event-sourcing`

4. **For new examples**:
   - Copy existing example structure
   - Follow progressive numbering (001, 002, etc.)
   - Update root `Makefile` with new `examples-NNN` target
   - Ensure example works with `--memory` flag

### Running Single Tests

```bash
# Rust (event store)
cd event-store
cargo test --package eventstore-core --test test_name

# TypeScript (SDK)
cd event-sourcing/typescript
pnpm test -- --testPathPattern=repository

# TypeScript (specific example)
cd examples/002-simple-aggregate-ts
pnpm test

# Jest with watch mode
pnpm test -- --watch
```

### Working with gRPC

```bash
# Generate TypeScript stubs
cd event-store && make gen-ts

# Generate Python stubs
cd event-store && make gen-py

# Test with grpcurl (server must be running)
cd event-store && make smoke

# Manual grpcurl call
grpcurl -plaintext \
  -import-path event-store/eventstore-proto/proto \
  -proto eventstore/v1/eventstore.proto \
  -d '{"aggregate_id":"Order-1","aggregate_type":"Order",...}' \
  localhost:50051 eventstore.v1.EventStore/Append
```

## Important Files

- **`Makefile`** (root): Orchestrates all builds and tests
- **`package.json`** (root): Defines pnpm workspaces and Turborepo scripts
- **`turbo.json`**: Turborepo build pipeline configuration
- **`event-store/Makefile`**: Event store-specific commands with dev infrastructure integration
- **`event-sourcing/Makefile`**: Coordinates SDK builds
- **`vsa.yaml`**: VSA configuration (in projects using VSA)
- **`dev-tools/dev`**: Development infrastructure management script

## Testing Philosophy

1. **No Mocks**: All examples use real infrastructure (event store + Postgres/Redis)
2. **Fast Tests**: Leverage dev infrastructure instead of testcontainers when available
3. **Progressive Complexity**: Tests in examples grow with example complexity
4. **Integration Over Unit**: Focus on end-to-end scenarios
5. **Coverage Targets**: Event store maintains 75%+ coverage (aiming for 85-90%)

## Common Patterns

### Optimistic Concurrency Control

Always handled by repositories:
```typescript
const account = await repository.load('Account-1');
account.deposit(100);
await repository.save(account);  // Throws ConcurrencyConflictError on conflict
```

### Event Replay

Automatic via `@EventSourcingHandler`:
```typescript
@EventSourcingHandler('AccountCredited')
private onCredited(event: AccountCredited): void {
  this.balance += event.amount;  // Applied during both raiseEvent and replay
}
```

### Integration Events (VSA)

Published by one context, subscribed by others:
```typescript
// In orders context
class OrderAggregate extends AggregateRoot {
  confirm(): void {
    this.raiseEvent(new OrderConfirmedIntegrationEvent(...));
  }
}

// In shipping context (_subscribers/)
class OrderConfirmedSubscriber {
  async handle(event: OrderConfirmedIntegrationEvent): Promise<void> {
    // Create shipment in shipping context
  }
}
```

## Troubleshooting

### Tests Fail with "Connection Refused"

Start dev infrastructure:
```bash
make dev-start
make dev-status  # Verify running
```

### Coverage Mismatched Data Error

Clean coverage artifacts:
```bash
cd event-store
cargo llvm-cov clean --workspace
make coverage
```

### TypeScript Build Errors

Ensure dependencies are installed and built in order:
```bash
pnpm -w install
pnpm build  # Uses Turborepo to handle build order
```

### gRPC Stub Generation Fails

Check protoc installation:
```bash
brew install protobuf  # macOS
```

Ensure SDK dependencies are installed:
```bash
cd event-store/sdks/sdk-ts && pnpm install
```

### Dev Infrastructure Port Conflicts

Clean and restart:
```bash
make dev-clean
make dev-init
make dev-start
```

## Performance Considerations

- Dev infrastructure is significantly faster than testcontainers (saves ~5-10s per test run)
- Turborepo caches TypeScript builds (use `--force` to rebuild)
- Rust incremental compilation is enabled (clean with `cargo clean`)
- Examples default to gRPC client; use `--memory` flag for faster in-memory testing

## Current Status

- ✅ **Event Store (Rust)**: Production-ready with memory and Postgres backends
- ✅ **TypeScript SDK**: Primary SDK, drives all examples
- 🔄 **Rust SDK**: Alpha stage, feature parity in progress
- 📋 **Python SDK**: Placeholder, awaiting implementation
- 🔄 **VSA Tool**: Planning phase, core validation logic implemented
- ✅ **Examples 001-009**: TypeScript examples are runnable
- 🚧 **Infrastructure & Docs**: Module scaffolding exists, content being built
