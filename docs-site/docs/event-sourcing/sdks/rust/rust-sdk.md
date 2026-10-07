# Rust SDK (Alpha)

The Rust event sourcing SDK (`event-sourcing/rust`, crate `event-sourcing-rust`) is in alpha. APIs may change between minor versions.

## Status

| Area | Status |
|------|--------|
| `Aggregate` / `AggregateRoot` traits, commands, events | Supported |
| `EventStoreClient` (gRPC, layered on the low-level `eventstore-sdk-rs` client) | Supported |
| `EventStoreRepository`: `load`, `save`, `exists` | Supported |
| Optimistic concurrency with typed `Error::ConcurrencyConflict` | Supported |
| Idempotent retry of saves with unknown outcome | Supported |
| Checkpointed projection runner (catch-up, live, resume, rebuild) | Supported |
| Postgres projection store (`postgres` feature) | Supported |
| Process-manager processor (live-only side effects) | Supported |
| Cross-language wire format (reads and writes TypeScript/Python streams) | Supported |
| Upcasters (on load and in projections) | Supported |
| Snapshots, authenticated clients | Planned |

## Events and aggregates

Events use the cross-language envelope (ADR-027): the stored payload is a JSON object with only the event's fields, and `event_type` and `event_version` are event metadata. The TypeScript and Python SDKs use the same envelope, so any SDK can read any stream.

```rust
use event_sourcing_rust::prelude::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderPlaced { pub order_id: String, pub total: i64 }
impl EventSchema for OrderPlaced {
    const EVENT_TYPE: &'static str = "OrderPlaced"; // stable name, shared with TS/Python
    const EVENT_VERSION: u32 = 1;                   // default 1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderShipped {} // an event without data: empty struct, payload `{}`
impl EventSchema for OrderShipped {
    const EVENT_TYPE: &'static str = "OrderShipped";
}

// Generates the enum, `DomainEvent` (encode flat body, decode by dispatching on
// type and version) and `From<OrderPlaced>` etc. Duplicate (type, version)
// pairs and invalid names are compile errors.
event_sourcing_rust::event_enum! {
    #[derive(Debug, Clone)]
    pub enum OrderEvent {
        Placed(OrderPlaced),
        Shipped(OrderShipped),
    }
}

impl Aggregate for Order {
    type Event = OrderEvent;
    type Error = Error;
    const AGGREGATE_TYPE: &'static str = "Order"; // required; never a Rust type path
    // aggregate_id, version, apply_event ...
}
```

- `AGGREGATE_TYPE` is required and is part of the stream identity shared with TypeScript (`@Aggregate('Order')`) and Python (`get_aggregate_type()`). It must be an ASCII letter followed by letters, digits, `_` or `.`; no `-` (the other SDKs split stream names on it). Checked at compile time.
- Field names are the JSON keys. Use `#[serde(rename_all = "camelCase")]` when the stream is shared with code using other names. Don't use `deny_unknown_fields` on events read from TypeScript streams (the TS SDK also writes `eventType`/`schemaVersion` into the payload).
- Decoding never guesses: an unknown type is `Error::UnknownEventType`, a known type at an unknown version is `Error::UnknownEventVersion`, a payload that does not match is `Error::EventDecode`, a non-JSON `content_type` is `Error::UnsupportedContentType`. A failed decode fails the load; it is never skipped.

### Upcasting

Bump `EVENT_VERSION` when a schema changes and register a step from the old version. Steps run on the raw JSON before decoding and chain (v1 to v2 to v3); `rename` maps an old event type to a new one.

```rust
let upcasters = Upcasters::new()
    .register("OrderPlaced", 1, 2, |mut body| {
        body["currency"] = "EUR".into();
        Ok(body)
    })
    .rename("OrderSent", 1, "OrderShipped", 1, Ok);

let repo = EventStoreRepository::<Order>::new(store.clone(), "tenant-a")
    .with_upcasters(upcasters.clone());
let runner = ProjectionRunner::new(store, projection_store, OrderSummary, "tenant-a")
    .with_upcasters(upcasters); // applied before `handles()` and `handle()`
```

In a projection handler, decode with `event.decode::<OrderEvent>()` (or a single schema, `event.decode::<OrderPlaced>()`).

## Repository

```rust
use std::sync::Arc;
use event_sourcing_rust::prelude::*;

let client = EventStoreClient::connect("127.0.0.1:50051").await?;
let repo = EventStoreRepository::<Order>::new(Arc::new(client), "tenant-a");

// Create
let mut order = AggregateInstance::new("order-1".into(), Order::default());
order.execute(OrderCommand::Place { /* ... */ }).await?;
repo.save(&mut order).await?;

// Load, change, save
let mut order = repo.load("order-1").await?.expect("exists");
order.execute(OrderCommand::Ship).await?;
repo.save(&mut order).await?;
```

Events carry `Aggregate::AGGREGATE_TYPE`; loading a stream whose stored aggregate type differs is an error. Streams are addressed by `(tenant, aggregate_id)`, so aggregate IDs must be unique within a tenant. `AggregateInstance::execute` applies a command's events to a clone of the aggregate and commits them only if all apply, so aggregates must be `Clone`. Saving to an existing stream verifies its stored aggregate type.

### Save semantics

`save` appends all pending events as one atomic batch, with the instance's `committed_version()` as the expected stream revision.

| Outcome | Result | Pending events |
|---------|--------|----------------|
| Acknowledged | `Ok(())`, `committed_version()` advances | Cleared |
| Stale writer | `Err(Error::ConcurrencyConflict { expected, actual })` | Kept. Discard the instance, reload, re-run the command |
| Unknown outcome (transport failure, timeout, lost ack; `Error::is_transient()`) | Retried per `RetryPolicy` (default 3 attempts); error returned if exhausted | Kept. Calling `save` again is safe |

Retries never duplicate events. A pending batch is immutable once recorded (event IDs, timestamps, nonces), every attempt sends the same idempotency key, and if a retry is rejected because the stream already moved, the repository compares event IDs at the expected position and treats its own committed batch as success. Conflicts are never retried automatically: the events were decided against stale state.

Use `RetryPolicy::none()` to handle retries yourself.

## Projections

`ProjectionRunner` drives one `CheckpointedProjection` over one tenant's global log (ADR-014):

```rust
let store = Arc::new(PostgresProjectionStore::new(pool)); // `postgres` feature
store.migrate().await?;
let mut runner = ProjectionRunner::new(Arc::new(client), store, OrderSummary, "tenant-a");
let cancel = CancellationToken::new();
runner.run(cancel.clone()).await?; // catch-up, then live, until cancelled or an error
```

- **Checkpoint identity**: tenant + projection name + projection version + feed (aggregate-id prefix). Tenants, projections, and versions never share a position.
- **Catch-up then live**: history up to the head observed at start is replayed with `DispatchContext::is_catching_up = true`, then the runner subscribes for live events.
- **Resume and duplicates**: the committed position is loaded on start; anything at or below it is skipped, so duplicate delivery is harmless.
- **Atomic commit**: per event the runner calls `begin`, `handle(&mut tx, ..)`, then `commit(tx, key, position)`. `PostgresProjectionStore` (and `InMemoryProjectionStore`) commit read-model writes and the checkpoint in one transaction; a handler error rolls both back. Commits that do not advance the checkpoint are rejected, fencing off a second runner on the same key.
- **External read models** (search, vector stores): use `ExternalCheckpoints`. The checkpoint is saved after the handler; a crash in between redelivers the event, so handlers must be idempotent (upsert by event id).
- **Ordering requirement**: the event store must deliver live events in global-nonce commit order (`commit_ordered_global_nonce`, #366). A live event below the last applied position that is not provably a duplicate stops the runner with `Error::OutOfOrderDelivery` instead of being skipped.
- **Feed prefixes** may not contain `\`, `%` or `_` until subscription prefixes are escaped by the backend (#361); `run` rejects them.
- **Errors propagate**: handler failures (`Error::ProjectionFailed`), commit failures, and subscription stream errors or end-of-stream stop the runner with an error. The checkpoint stays at the last committed event.
- **Rebuild**: `runner.rebuild()` resets that key's data and checkpoint only (in one transaction for transactional stores; checkpoint first for external ones). Stop other runners on the key first. Bump `version()` to build a new read model next to the old one.
- **Side effects**: projections must be pure. Write to-do records in `handle` and attach a `LiveProcessor` with `with_live_processor`; it runs on its own task and is woken only by committed live events, never during replay (process-manager pattern, ADR-025). Failed passes retry with backoff, and one pass runs when going live (after replay) to resume items stranded by a crash; disable with `drain_pending_on_live_start(false)`. `process_pending` must be idempotent.

## Examples

```bash
cd event-sourcing/rust
cargo run --example basic_aggregate
cargo run --example order_processing
cargo run --example repository   # live gRPC event store (in-process unless EVENT_STORE_ADDR is set)
```

## Related

- **[TypeScript SDK](../typescript/typescript-sdk.md)**
- **[API Reference](../api-reference.md)**
- **[Event Store Rust SDK](/docs/event-store/sdks/rust/rust-sdk.md)** - Low-level event store client
