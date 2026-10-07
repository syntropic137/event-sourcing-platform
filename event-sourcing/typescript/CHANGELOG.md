# Changelog

All notable changes to the TypeScript Event Sourcing SDK will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Gateway credentials (ADR-024, #302): `EventStoreClientConfig.connection`
  (and `GrpcEventStoreConfig.connection`) passes TLS, `auth` (Basic, Bearer,
  token provider) and `allowInsecureCredentials` to the event store TS SDK.
  `serverAddress` accepts `http://` and `https://`. Credentials go on every
  call and are refused over plaintext to a non-loopback host unless allowed.
- `EventStoreAuthenticationError` (code `EVENT_STORE_UNAUTHENTICATED`) for
  rejected credentials; `streamExists` now throws it instead of returning
  `false`. `connect()` on a gRPC client rejects on a bad connection config.

### Breaking Changes

#### Cross-language event envelope (ADR-027, #382)

- **Payload holds only event fields.** `BaseDomainEvent.toJson()` and the gRPC
  adapter no longer write `eventType`/`schemaVersion` into the payload; they
  are envelope metadata. Code that read them from `toJson()` must use
  `event.eventType`/`event.schemaVersion`.
- **Decoding is by `(eventType, schemaVersion)`.** The `EventSerializer`
  registry keeps every registered version (`@Event` registers the class's
  `schemaVersion`). A stored event of a registered type at a version with no
  class, after upcasting, throws `UnknownEventVersionError` instead of being
  assigned onto the class of another version.
- **Read errors are typed and never skipped.** A payload that is not JSON or
  not a JSON object throws `EventPayloadError` (it was silently read as `{}`);
  a stored event without metadata throws (it was dropped); a non-JSON content
  type throws `UnsupportedContentTypeError`. All extend `EventDecodeError` and
  are rethrown by `readEvents`/`readAll` unwrapped.
- An event without `schemaVersion` is written as `event_version = 1` (was 0).

Migration: none for stored data. Payloads written by 0.17 and earlier still
decode: readers drop their `eventType`/`schemaVersion` keys. If a class's
`schemaVersion` was bumped without keeping the old class, register the old
class or an upcaster.

#### Added

- `Upcasters` (`register(type, from, to, fn)`, `rename(fromType, from, toType, to, fn)`),
  same semantics as the Rust SDK; `upcasters` option on `GrpcEventStoreAdapter`
  and `EventStoreClientFactory.createGrpcClient`; `EventSerializer.deserialize(json, options)`.
- `decodeEvent`, `EventSerializer.resolveEventClass(type, version)`,
  `registeredVersions(type)`, `registerEvent(type, class, version?)`.
- `EventMetadata.storedEventType`/`storedEventVersion` (as stored; set on read).
- Golden fixture tests decoding the TypeScript, Python, Rust and legacy
  TypeScript encodings (`tests/xlang-golden.test.ts`).

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
import { AutoDispatchAggregate } from '@syntropic137/event-sourcing-typescript';
class MyAggregate extends AutoDispatchAggregate<MyEvent> { }

// After
import { AggregateRoot } from '@syntropic137/event-sourcing-typescript';
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

