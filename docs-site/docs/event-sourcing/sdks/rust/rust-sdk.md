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
| Checkpointed projections | In progress |
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
