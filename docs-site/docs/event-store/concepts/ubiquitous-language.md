# Ubiquitous Language (Axon-aligned)

This glossary defines the shared vocabulary used across this experiment so engineers, product, and ops use the same terms consistently. Terms are aligned with Axon Framework conventions where applicable.

## Core Concepts

### Event Store Fundamentals
- **Event**
  - Immutable record of something that happened in the past.
  - Has two parts: `EventMetadata` and `EventData` (payload).

- **EventData**
  - The business payload of the event.
  - Encoded as bytes; `content_type` describes the encoding (e.g., `application/json`, `application/octet-stream`).

- **EventMetadata**
  - Describes the event's identity and context.
  - Common fields:
    - `eventType` — business event type name (e.g., `com.myco.order.OrderCreated` - Axon-aligned dotted name).
    - `schemaVersion` — integer version of the event type used for evolution/upcasting (maps to Axon `@Revision`).
    - `eventId` — unique identifier (UUID v7 recommended).
    - `contentType` — payload encoding.
    - `aggregateId` — the aggregate this event belongs to.
    - `aggregateType` — type/category of aggregate.
    - `aggregateNonce` — 1-based version within the aggregate (represents aggregate sequence).
    - `globalNonce` — monotonically increasing global index across all aggregates (Axon term: tracking token).
    - `timestamp` — when the event was recorded.
    - `correlationId` / `causationId` — trace causal chains across services.
    - `actorId` — who/what caused the event.
    - `tenantId` — optional multi-tenant discriminator.
    - `metadata` — free-form key/value map for cross-cutting data (maps to Axon `MetaData`).

### Aggregate Concepts
- **Aggregate (aggregateType)**: The consistency boundary that emits events.
- **Aggregate Identifier (aggregateId)**: Unique id of an aggregate instance.
- **Stream**: An ordered sequence of events for a specific aggregate, identified by `aggregateId`.
- **Aggregate Root**: The main entity within an aggregate that handles commands and emits events.

## Operations

- **Append**
  - Add one or more events to an aggregate's stream with optimistic concurrency.
  - Client provides: aggregateId, aggregateType, proposed aggregateNonce
  - Client provides: expectedAggregateNonce (current stream head; 0 ⇒ aggregate must not exist)
  - Store validates: proposed aggregateNonce == current_max + 1
  - Store assigns: globalNonce, timestamp (atomically)
  - Fails with concurrency conflict if nonce validation fails.

- **ReadStream**
  - Read a slice of events from an aggregate's stream.
  - Parameters:
    - `aggregateId` — target aggregate identifier.
    - `fromAggregateNonce` — 1-based starting point (inclusive).
    - `maxCount` — maximum number of events to return.
    - `forward` — true: ascending by aggregate nonce (forward reads); false: descending by aggregate nonce (backward reads).

- **Subscribe**
  - Server stream delivering:
    - Replay: matching historical events (filtered by `aggregateType` prefix and/or `fromGlobalNonce`).
    - Live: new events as they are appended.
  - `aggregateIdPrefix` can narrow to categories like `Order-`.
  - Uses `globalNonce` (tracking token) for checkpointing.

## Patterns

- **Event Sourcing**
  - Domain objects (aggregates) are reconstructed by replaying their event stream.
  - Commands produce events; events are the source of truth.

- **Command-Query Responsibility Segregation (CQRS)**
  - Commands (write operations) are separate from queries (read operations).
  - Event sourcing naturally enables CQRS.

- **Projection**
  - A derived read model built from events, optimized for queries, not commands.
  - Examples: reporting views, search indexes, materialized views.

- **Snapshot**
  - A persisted capture of an aggregate's state at a point in time to speed up rehydration.
  - Use when streams are long and replays are costly.
  - Storage format should be explicit and versioned.

- **Optimistic Concurrency Control**
  - Client proposes aggregateNonce based on current state knowledge.
  - Store validates proposed nonce == current_max + 1.
  - Competing writers get ConcurrencyError if they propose the same nonce.
  - On validation failure, clients reload state and retry with correct nonce.
  - **True Optimistic Concurrency**: Client controls sequencing, store ensures correctness.

- **Idempotency**
  - Repeating the same logical command should not produce duplicate events.
  - Typically achieved via command IDs recorded in metadata and checked by writers.

## Serialization

- **Content Types**
  - `application/json` for human-readable interop.
  - `application/octet-stream` for compact binary (e.g., prost-encoded protobuf messages).

- **Schema Evolution**
  - Use `schemaVersion` in metadata and migration strategies in projections.
  - Upcasters can transform older events on read.

## Naming Conventions

- `aggregateId`: `<AggregateType>-<business-id>` (e.g., `Order-123`).
- `eventType`: Dotted names (Java FQCN style), e.g., `com.myco.order.OrderCreated` (Axon-aligned).
- `aggregateType`: UpperCamelCase business aggregate names.

## Terminology Alignment

- `aggregateId` ↔ aggregate identifier
- `aggregateType` ↔ aggregate type
- `aggregateNonce` ↔ per-aggregate monotonic sequence (Axon term: sequence number)
- `globalNonce` ↔ tracking token
- `eventType` ↔ payload type (Axon's class name)
- `schemaVersion` ↔ Axon `@Revision`
- `metadata` ↔ Axon `MetaData`

## Non-Goals (for now)

- Global ordering guarantees across nodes/partitions beyond a single instance.
- Cross-aggregate transactions.
- Event encryption at rest.
- Advanced query capabilities beyond stream reading and global nonce subscription.

## References

- Code interfaces in this experiment:
  - `eventstore-core` traits and errors: `experiments/005-rust-event-store/eventstore-core/`
  - Memory backend: `experiments/005-rust-event-store/eventstore-backend-memory/`
  - gRPC service: `experiments/005-rust-event-store/eventstore-bin/`
- Axon Framework: https://docs.axoniq.io/reference-guide/
- Event Modeling: https://eventmodeling.org/
