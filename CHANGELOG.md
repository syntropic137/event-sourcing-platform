# Changelog

All notable changes to the Event Sourcing Platform will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0-beta] - 2025-09-20

### 🎯 Major Milestone: Complete Event Sourcing Platform

This release represents a major milestone with a complete, production-ready event sourcing platform featuring comprehensive examples, documentation, and robust infrastructure.

### ✨ Added

#### Examples
- **9 Progressive TypeScript Examples** covering basic to advanced patterns:
  - `001-basic-store-ts`: Basic event store usage and connectivity
  - `002-simple-aggregate-ts`: Single aggregate with command/event patterns
  - `003-multiple-aggregates-ts`: Customer and Order aggregates with relationships
  - `004-cqrs-patterns-ts`: Banking system with command/query separation
  - `005-projections-ts`: E-commerce analytics with multiple projection types
  - `006-event-bus-ts`: Cross-aggregate communication with event bus
  - `007-inventory-complete-ts`: Complete inventory management system
  - `008-observability-ts`: System monitoring and metrics collection
  - `009-web-dashboard-ts`: Live HTML dashboard with real-time projections

#### Documentation
- **Comprehensive Event Sourcing Documentation**:
  - Event sourcing overview and core concepts
  - Quickstart guide with step-by-step examples
  - Detailed aggregates guide with patterns and best practices
  - Events documentation covering domain events and serialization
  - Projections guide for building read models
  - PostgreSQL and gRPC integration guide
  - Examples index with progressive learning path

#### Infrastructure
- **Docker Compose for Examples**: `docker-compose.examples.yml` for running examples infrastructure
- **Enhanced Dev Tools**: Improved development infrastructure with better error handling
- **QA Pipeline**: Comprehensive quality assurance with coverage reporting

### 🔧 Fixed

#### Event Store
- **Coverage Instrumentation**: Resolved `cargo-llvm-cov` instrumentation mismatch causing QA failures
- **Error Detection**: Improved error detection to distinguish warnings from fatal errors
- **Dev Infrastructure Integration**: Fixed coverage tests to work with development infrastructure
- **Test Reliability**: Added `CARGO_TEST_FLAGS` support for better test control

#### TypeScript SDK
- **Missing Exports**: Added missing exports for classes used in examples:
  - `EventSerializer` and `BaseDomainEvent` from `core/event`
  - `RepositoryFactory` from `core/repository`
  - `AggregateRoot` from `core/aggregate`
- **API Consistency**: Fixed type vs class exports for better API consistency

#### Examples
- **API Usage**: Corrected Example 002 to use command objects instead of individual parameters
- **Command Patterns**: Updated to use proper `CommandHandler` pattern with command interfaces
- **Error Handling**: Improved error handling and logging consistency across all examples

### 🚀 Improved

#### Development Experience
- **Progressive Learning Path**: Examples build complexity from basic concepts to production patterns
- **Real-World Patterns**: Banking, e-commerce, and inventory management examples
- **Visual Dashboard**: Web dashboard makes event sourcing tangible and observable
- **Infrastructure Support**: Full PostgreSQL, Redis, and gRPC event store integration

#### Quality Assurance
- **Robust Testing**: All examples tested with both memory and gRPC modes
- **Coverage Reporting**: Working line-by-line coverage analysis
- **Static Analysis**: Complete fmt, clippy, and type checking across the platform
- **CI/CD Ready**: Reliable QA pipeline suitable for continuous integration

### 📚 Technical Details

#### Architecture Progression
The examples demonstrate a clear progression through event sourcing concepts:
```
001: Basic Store → 002: Simple Aggregate → 003: Multiple Aggregates →
004: CQRS Patterns → 005: Projections → 006: Event Bus →
007: Inventory System → 008: Observability → 009: Web Dashboard
```

#### Infrastructure Components
- **Event Store**: Rust-based gRPC event store with PostgreSQL backend
- **TypeScript SDK**: Comprehensive SDK with aggregates, commands, events, and repositories
- **Development Tools**: Docker-based infrastructure for rapid development
- **Examples**: Progressive TypeScript examples with comprehensive READMEs

#### Quality Metrics
- **Test Coverage**: Comprehensive test coverage with detailed reporting
- **Static Analysis**: All code passes formatting, linting, and type checking
- **Documentation**: Complete documentation covering theory and practice
- **Examples**: 9 working examples demonstrating real-world patterns

### 🎓 Learning Outcomes

This release provides a complete learning path for event sourcing:
- **Conceptual Understanding**: Clear documentation of event sourcing principles
- **Practical Implementation**: Step-by-step examples building complexity
- **Production Patterns**: Real-world business logic and architectural patterns
- **Visual Understanding**: Interactive dashboard showing event sourcing in action

### 🔄 Migration Guide

For users upgrading from previous versions:
1. Update TypeScript imports to use the newly exported classes
2. Update Example 002 usage to use command objects instead of individual parameters
3. Review the new documentation for updated patterns and best practices
4. Test examples with both memory and gRPC modes

---

## [0.4.0-beta] - 2025-12-05

### 🎯 Major Feature: ReadAll RPC for Global Event Stream

This release adds a proper `ReadAll` RPC for batch reading events from a global position, replacing the fragile Subscribe-based approach for projection catch-up.

### ✨ Added

#### Event Store
- **ReadAll RPC**: New gRPC operation for paginated reads from global position
  - `ReadAllRequest`: Parameters for `from_global_nonce`, `max_count`, `forward`
  - `ReadAllResponse`: Events array with explicit `is_end` flag and `next_from_global_nonce`
- **Rust Backend**: `read_all` implemented in EventStore trait, Postgres, and Memory backends
- **SDK-RS**: `read_all` method in Rust SDK

#### TypeScript SDK
- **event-sourcing SDK**: `readAll` method in EventStoreClient interface
- **sdk-ts**: Low-level `readAll` implementation
- **MemoryEventStoreClient**: Test environment guard for safer test isolation

#### Python SDK
- **event-sourcing SDK**: `read_all` method in EventStoreClient interface
- **sdk-py**: Low-level `read_all` implementation
- **MemoryEventStoreClient**: Test environment guard (per ADR-004)

#### Documentation
- **ADR-012**: ReadAll Global Stream design decision (Accepted)

### 🧪 Testing
- Unit tests for `read_all` in TypeScript SDK
- Unit tests for `read_all` in Python SDK
- Rust backend unit tests for `read_all`

### 🔄 Migration Guide

For projection catch-up, replace Subscribe-based heuristics with ReadAll:

```python
# Before (fragile)
async for response in stub.Subscribe(request):
    if consecutive_keepalives >= 10:
        break  # Heuristic-based end detection

# After (reliable)
events, is_end, next_pos = await client.read_all(from_position, max_count=100)
for event in events:
    await process(event)
if is_end:
    break  # Explicit end signal
```

---

## [0.2.1] - 2025-12-05 (event-store crates)

### 🐛 Fixed

#### Event Store Subscribe
- **Cursor Advancement Bug (ADR-013)**: Fixed critical bug where the `Subscribe` gRPC method advanced the cursor position **before** events were yielded to clients
  - Previously, if a connection dropped mid-batch, events could be lost because the cursor already pointed past undelivered events
  - Now cursor only advances after an event has been successfully yielded
  - Affects both Replay and Live phases of the subscription

### ✨ Added

#### Testing
- **Regression Tests**: Added tests to verify correct subscription behavior:
  - `postgres_subscribe_yields_all_events_in_order`
  - `postgres_subscribe_live_phase_yields_all_batch_events`

#### Documentation
- **ADR-013**: Subscribe Cursor Advancement After Yield (Accepted)

### 📦 Packages Updated
- `eventstore-proto`: 0.2.0 → 0.2.1
- `eventstore-core`: 0.2.0 → 0.2.1
- `eventstore-backend-postgres`: 0.2.0 → 0.2.1
- `eventstore-backend-memory`: 0.2.0 → 0.2.1
- `eventstore-bin`: 0.2.0 → 0.2.1
- `eventstore-sdk-rs`: 0.2.0 → 0.2.1

---

## [Unreleased]

### 🚧 In Development
- Advanced projection patterns
- Event store clustering and high availability
- Performance benchmarking and optimization

---

*For more details on any release, see the corresponding git tags and commit history.*
