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
| Supervised runner (reconnect with backoff, DATA_LOSS halt, health, capability guard) | Supported |
| TLS, timeouts, keepalive, Basic/Bearer credentials (`ClientConfig`) | Supported |
| Cross-language wire format (reads and writes TypeScript/Python streams) | Supported |
| Upcasters (on load and in projections) | Supported |
| Snapshots | Planned |

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

## Connecting

`EventStoreClient::connect(addr)` accepts `host:port` (plaintext), `http://host:port`, or `https://host:port` (TLS, verified against the OS trust store). Use `connect_with(ClientConfig)` for everything else:

```rust
use std::time::Duration;
use event_sourcing_rust::client::{ClientConfig, EventStoreClient, TlsConfig, capabilities};

let client = EventStoreClient::connect_with(
    ClientConfig::new("https://events.internal:8443")
        .tls(
            TlsConfig::new()
                .ca_certificate_pem(std::fs::read("ca.pem")?) // replaces OS roots unless .with_system_roots(true)
                .domain_name("events.internal"),              // when connecting by IP or through a tunnel
                // .client_identity_pem(cert_pem, key_pem)     // mutual TLS
        )
        .basic_auth("app", std::env::var("ESP_GATEWAY_PASSWORD")?)
        .connect_timeout(Duration::from_secs(5))
        .request_timeout(Duration::from_secs(10)),
)
.await?;

// Fail fast if the server lacks a guarantee you rely on.
client.require_capabilities(&[capabilities::COMMIT_ORDERED_GLOBAL_NONCE]).await?;
```

| Setting | Default | Notes |
|---------|---------|-------|
| `connect_timeout` | 10s | TCP + TLS + HTTP/2 handshake |
| `request_timeout` | 30s | Each unary call (also sent as `grpc-timeout`) and *opening* a subscription. Never applied to an open subscription stream. Expiry is `DEADLINE_EXCEEDED` (`is_transient()`) |
| `http2_keepalive(interval, timeout)` | 30s / 10s | Ends a subscription whose connection died with `UNAVAILABLE` |
| `keepalive_while_idle` | true | Needed for subscriptions (hyper treats a connection carrying only an open stream as idle) |
| `tcp_keepalive` | 60s | |
| `lazy_connect` | false | Connect on first RPC |

Credentials go in the `authorization` header: `basic_auth(user, pass)` is what the ADR-024 nginx gateway (HTTP Basic Auth on its external port) expects; `bearer_token(t)` and `token_provider(p)` send `Bearer <token>` (use `SharedToken` and call `set` from a refresh task to rotate without reconnecting). Credentials are refused over plaintext to non-loopback hosts unless you set `allow_insecure_credentials(true)`; the gateway has no TLS yet (#301), so only do that on a trusted network. `Debug` output never contains secrets.

Configuration errors are `Error::Config`; failed capability/version checks are `Error::Incompatible`.

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
- **Errors propagate**: handler failures (`Error::ProjectionFailed`), commit failures, and subscription stream errors or end-of-stream stop `run` with an error. The checkpoint stays at the last committed event. Use `run_supervised` (below) to reconnect.
- **Rebuild**: `runner.rebuild()` resets that key's data and checkpoint only (in one transaction for transactional stores; checkpoint first for external ones). Stop other runners on the key first. Bump `version()` to build a new read model next to the old one.
- **Side effects**: projections must be pure. Write to-do records in `handle` and attach a `LiveProcessor` with `with_live_processor`; it runs on its own task and is woken only by committed live events, never during replay (process-manager pattern, ADR-025). Failed passes retry with backoff, and one pass runs when going live (after replay) to resume items stranded by a crash; disable with `drain_pending_on_live_start(false)`. `process_pending` must be idempotent.
- **Processor panics** never go unnoticed: by default a panicking pass stops the runner with `Error::LiveProcessorPanicked`; `on_processor_panic(ProcessorPanicPolicy::Restart)` logs it and retries the pass with the processor backoff instead.

### Supervision

Services run the runner under `run_supervised`, which owns reconnects:

```rust
let mut runner = ProjectionRunner::new(Arc::new(client), store, OrderSummary, "tenant-a")
    .with_live_processor(notifier);
let health = runner.health(); // watch::Receiver<RunnerHealth> for readiness/metrics
let policy = BackoffPolicy::new(Duration::from_millis(500), Duration::from_secs(30));
match runner.run_supervised(cancel, policy).await {
    Ok(RunExit::Cancelled { .. }) => {}
    Err(Error::DataLoss { global_nonce, .. }) => { /* alert: operator action needed (ADR-026) */ }
    Err(err) => { /* bug or misconfiguration: handler, fencing, incompatible server, panic */ }
}
```

| Failure | Behavior |
|---------|----------|
| `UNAVAILABLE`, `RESOURCE_EXHAUSTED` (lagging subscription), `DEADLINE_EXCEEDED`, transport errors, Postgres projection-store connection loss | Reconnect from the persisted checkpoint after a jittered exponential backoff (`BackoffPolicy`: initial, cap, multiplier, jitter, optional `with_max_retries`). The backoff resets after progress. Committed events are skipped on redelivery: nothing lost, nothing applied twice. |
| `DATA_LOSS` (undecodable stored event, position from the `esp-undecodable-global-nonce` trailer) | Never retried with backoff, never skipped. The runner applies every valid event before it, halts with the checkpoint just below it, logs one `ERROR` per position, sets `RunnerHealth::halted_at` and holds the `LiveProcessor`. Default: returns `Error::DataLoss { global_nonce, .. }`. With `with_undecodable_recheck(interval)` it stays halted and re-checks at that fixed pace, resuming on its own once the row is repaired or the checkpoint was moved past it. |
| Handler or upcast error (`ProjectionFailed`), fencing (`CheckpointFenced`), incompatible server (`Incompatible`), `OutOfOrderDelivery`, processor panic, decode errors | Stop with the typed error. |

Cancellation is prompt, including during a backoff or re-check wait.

**Operator recovery from `DATA_LOSS`** (ADR-026): repair the row or deploy an event store that decodes it; or, if it is unrecoverable, set this consumer's checkpoint to `global_nonce` (it resumes at `global_nonce + 1`), then restart the runner (or wait for the re-check). If the undecodable event is the tenant head, the head probe uses its position as the live boundary, so a runner moved past it still goes live.

**Health** (`RunnerHealth`): `state` (`Starting`, `CatchingUp`, `Live`, `Backoff { attempt, delay }`, `Halted { global_nonce }`, `Stopped`, `Failed`), `position`, `live_boundary`, `lag()` (while catching up), `halted_at`, `last_error`, `consecutive_failures`, `restarts`, and `is_healthy()` for readiness probes.

**Capability guard**: `run`, `catch_up` and every supervised reconnect first require the server to advertise `commit_ordered_global_nonce`, `subscription_errors_surfaced` and `undecodable_events_surfaced` (`REQUIRED_CAPABILITIES`). Legacy servers, and custom `EventStorePort` decorators that do not forward `server_info`, are refused with `Error::Incompatible` (not retried). Narrow the set with `with_required_capabilities([...])` or opt out with `without_capability_check()`.

## Examples

```bash
cd event-sourcing/rust
cargo run --example basic_aggregate
cargo run --example order_processing
cargo run --example repository   # live gRPC event store (in-process unless EVENT_STORE_ADDR is set)
cargo run --example supervised_projection   # projection service: supervision, live processor, health
```

## Related

- **[TypeScript SDK](../typescript/typescript-sdk.md)**
- **[API Reference](../api-reference.md)**
- **[Event Store Rust SDK](/docs/event-store/sdks/rust/rust-sdk.md)** - Low-level event store client
