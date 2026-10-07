---
sidebar_label: Server compatibility
---

# Server Compatibility and Capability Flags

A client talking to the event store cannot see which server build is listening.
Several correctness fixes change server behavior without changing the wire
format, so a client pinned to a valid `eventstore-proto` version can still be
talking to a store that lacks a fix. When the failure mode is silence (events
quietly missing), the client needs to check at connect time.

## `GetServerInfo`

Since **v0.17.0** the `EventStore` service has a `GetServerInfo` RPC:

```proto
rpc GetServerInfo (GetServerInfoRequest) returns (GetServerInfoResponse);

message GetServerInfoResponse {
  string server_version        = 1; // e.g. "0.17.0"
  string api_version           = 2; // e.g. "eventstore.v1"
  string backend               = 3; // "memory" | "postgres" | "unknown"
  repeated string capabilities = 4; // named guarantees, see below
}
```

**Older servers** do not know the method and answer gRPC `UNIMPLEMENTED`. The
SDK helpers map that to a *legacy* result: unknown version, **no capabilities**.
Every requirement check therefore fails closed against a pre-0.17.0 server.
Other errors (for example `UNAVAILABLE`) are surfaced as errors, never as
"legacy".

Prefer capability checks over version checks: a capability names the guarantee
you depend on, not the release that shipped it. Unknown capability names must
be ignored by clients.

## Capability registry

| Capability | Guarantee | Fix | Server version that has the fix | Advertised from |
|---|---|---|---|---|
| `commit_ordered_global_nonce` | Global nonces become visible in commit order (per tenant), so a cursor that has moved past nonce N never misses a nonce below N that commits later. | #337 | v0.16.0 | v0.17.0 |
| `subscription_errors_surfaced` | A subscription that cannot keep delivering ends with an error status (Postgres query failure: `UNAVAILABLE`, naming `resume from global_nonce N`) instead of an empty result or a silently ended stream. The cursor is never advanced past undelivered events. | #350 / #356 | v0.17.0 | v0.17.0 |
| `undecodable_events_surfaced` | A stored event the server cannot decode ends the subscription or read with `DATA_LOSS` at its position (also in trailing metadata). Earlier events are delivered, later ones are not, and the bad event is never skipped. | #351 / #359 | v0.17.0 | v0.17.0 |

Notes:

- A v0.16.x server has the #337 fix but predates `GetServerInfo`, so it reads
  as legacy. If you must accept v0.16.x, verify the deployment some other way
  (image digest pin) and document why; the helpers cannot prove it.
- Both built-in backends advertise all three flags. A custom backend
  advertises nothing unless it overrides `EventStore::capabilities()`.
- **Postgres** provides each guarantee through the fixes listed above.
- **Memory** provides them as follows:
  - `commit_ordered_global_nonce`: live events are published under the append
    lock, so subscribers see them in global nonce order, and the replay/live
    handoff has no gap or duplicate.
  - `subscription_errors_surfaced`: memory runs no backend queries; the only
    way a subscription can stop delivering is a lagged receiver (more than the
    broadcast buffer behind). That ends the stream with `RESOURCE_EXHAUSTED`
    (resubscribe from your checkpoint) instead of silently skipping events.
  - `undecodable_events_surfaced`: events are held as decoded protobuf
    messages, so there is no decode step that could fail or skip an event.
    The guarantee holds by construction.
- Each flag is checked against behavior in tests, not only as a string:
  commit order (`eventstore-backend-memory/tests/live_order.rs`,
  `eventstore-backend-postgres/tests/it_commit_order.rs`), subscription errors
  (`it_subscribe_faults.rs`, `eventstore-bin/tests/subscribe_errors.rs`,
  memory lag test), undecodable events (`it_subscribe_undecodable.rs`,
  `subscribe_errors.rs`).

## Who is exposed by #337

The #337 release notes describe live subscribers, but the mechanism is the
**cursor**, not the subscription. Any reader that pages forward by global nonce
was exposed on a pre-v0.16.0 store, including plain catch-up reads:

```rust
let mut from = 0_u64;
loop {
    let page = client.read_all(ReadAllRequest { tenant_id, from_global_nonce: from, max_count, forward: true }).await?;
    // ... consume page.events ...
    if page.is_end || page.events.is_empty() { break; }
    from = page.next_from_global_nonce;
}
```

A nonce that committed after the cursor had moved past it was never returned
by a later page. The read succeeded, no error was reported, and the projection
was built from fewer events than were committed. Projections built by paging
`ReadAll` against a pre-v0.16.0 store should be rebuilt once the store is
upgraded.

## SDK helpers

All SDKs expose the same three operations. `server_info` never fails for a
legacy server; the `require_*` helpers raise a typed `CompatibilityError`.

**Rust** (`eventstore-sdk-rs`):

```rust
use eventstore_sdk_rs::{capabilities, EventStore};

let mut client = EventStore::connect("localhost:50051").await?;
client
    .require_capabilities(&[capabilities::COMMIT_ORDERED_GLOBAL_NONCE])
    .await?; // Err downcasts to CompatibilityError
let info = client.server_info().await?; // info.is_legacy() for pre-0.17.0
```

**TypeScript** (`@eventstore/sdk-ts`):

```ts
import { Capabilities, EventStoreClientTS } from "@eventstore/sdk-ts";

const client = new EventStoreClientTS("localhost:50051");
await client.requireCapabilities([Capabilities.COMMIT_ORDERED_GLOBAL_NONCE]);
const info = await client.serverInfo(); // info.legacy for pre-0.17.0
```

**Python** (`event_sourcing`):

```python
from event_sourcing.client import Capabilities, GrpcEventStoreClient

client = GrpcEventStoreClient(address="localhost:50051")
await client.connect()
await client.require_capabilities([Capabilities.COMMIT_ORDERED_GLOBAL_NONCE])
info = await client.server_info()  # info.is_legacy for pre-0.17.0
```

To warn instead of refusing, call `server_info()` and check
`missing_capabilities(...)` yourself.

## Adding a capability

1. Add the constant to `eventstore_proto::capabilities` (and the TS / Python
   registries).
2. Return it from each backend's `capabilities()` that provides the guarantee.
3. Add a row to the registry table above with the fix and version.
4. Never rename or reuse a released name.
