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
| Snapshots, upcasting, authenticated clients | Planned |

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

Events are stored as JSON, so `Aggregate::Event` must implement `Serialize` and `DeserializeOwned`. Override `Aggregate::aggregate_type` (or call `with_aggregate_type`) to record a stable type name. Streams are addressed by `(tenant, aggregate_id)`, so aggregate IDs must be unique within a tenant.

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
- **Errors propagate**: handler failures (`Error::ProjectionFailed`), commit failures, and subscription stream errors or end-of-stream stop the runner with an error. The checkpoint stays at the last committed event.
- **Rebuild**: `runner.rebuild()` resets that key's data and checkpoint only. Bump `version()` to build a new read model next to the old one.
- **Side effects**: projections must be pure. Write to-do records in `handle` and attach a `LiveProcessor` with `with_live_processor`; it runs on its own task and is woken only by committed live events, never during replay (process-manager pattern, ADR-025).

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
