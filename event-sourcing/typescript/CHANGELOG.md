# Changelog

All notable changes to the TypeScript Event Sourcing SDK will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Breaking Changes

#### Removed AutoDispatchAggregate Class

**What Changed:**
- The `AutoDispatchAggregate` class has been removed
- Its functionality has been merged directly into `AggregateRoot`
- This simplifies the aggregate inheritance hierarchy from 3 levels to 2 levels

**Why:**
- `AutoDispatchAggregate` represented unnecessary duplication
- All production aggregates should extend `AggregateRoot`
- Simpler API with clearer production path

**Migration Required:**

```typescript
// Before
import { AutoDispatchAggregate } from '@event-sourcing-platform/typescript';
class MyAggregate extends AutoDispatchAggregate<MyEvent> { }

// After
import { AggregateRoot } from '@event-sourcing-platform/typescript';
class MyAggregate extends AggregateRoot<MyEvent> { }
```

**No behavioral changes** - just update imports and class names.

**See:** [Migration Guide](../../docs-site/docs/event-sourcing/guides/migration-autodispatch-to-aggregateroot.md) | [ADR-005](../../docs-site/docs/adrs/ADR-005-remove-autodispatch-aggregate.md)

### Changed

- `AggregateRoot` now includes all automatic event dispatching functionality
- `AggregateRoot` now directly extends `BaseAggregate` instead of `AutoDispatchAggregate`

### Removed

- **BREAKING:** `AutoDispatchAggregate` class
- **BREAKING:** `AutoDispatchAggregate` export from public API

---

## [0.1.0] - 2025-11-05

### Added

Initial release of the TypeScript Event Sourcing SDK.

**Core Features:**
- Event sourcing aggregate abstractions
  - `BaseAggregate` - Low-level manual event handling
  - `AutoDispatchAggregate` - Automatic event dispatching via decorators
  - `AggregateRoot` - Production-ready with command handlers
- Command handling infrastructure
  - `@CommandHandler` decorator for aggregate methods
  - `@EventSourcingHandler` decorator for event handlers
  - `@Aggregate` decorator for aggregate metadata
- Repository pattern implementation
  - Optimistic concurrency control
  - Event stream management
  - Aggregate rehydration from events
- Event store client adapters
  - gRPC client for production event store
  - In-memory client for testing and development
- Domain event abstractions
  - `BaseDomainEvent` base class
  - Event serialization/deserialization
  - Event envelope with metadata
- Query abstractions
  - `Query` and `QueryHandler` interfaces
  - Projection pattern support
- Error handling
  - Custom error types for event sourcing scenarios
  - Concurrency conflict detection

**Documentation:**
- Comprehensive README with examples
- API reference documentation
- TypeScript type definitions
- Example projects demonstrating patterns

**Testing:**
- Unit tests for all core functionality
- Integration tests with event store
- Concurrency and lifecycle tests

---

## Legend

- `Added` - New features
- `Changed` - Changes in existing functionality
- `Deprecated` - Soon-to-be removed features
- `Removed` - Removed features
- `Fixed` - Bug fixes
- `Security` - Security fixes
- `Breaking` - Breaking changes requiring migration

[Unreleased]: https://github.com/yourusername/event-sourcing-platform/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/yourusername/event-sourcing-platform/releases/tag/v0.1.0

