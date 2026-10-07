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

Notes:

- A v0.16.x server has the #337 fix but predates `GetServerInfo`, so it reads
  as legacy. If you must accept v0.16.x, verify the deployment some other way
  (image digest pin) and document why; the helpers cannot prove it.
- Both built-in backends (memory, postgres) advertise
  `commit_ordered_global_nonce`. A custom backend advertises nothing unless it
  overrides `EventStore::capabilities()`.

### Planned flags (not yet advertised)

These are reserved names for fixes that are in review but not yet on `main`.
Each fix's PR adds its flag to the backend's `capabilities()` when it lands.

| Capability (planned) | Guarantee | Issue / PR | Expected version |
|---|---|---|---|
| `subscription_errors_surfaced` | A failed Postgres subscription query ends the stream with `UNAVAILABLE` instead of yielding an empty result. | #350 / #356 | v0.17.0 |
| `undecodable_events_surfaced` | An undecodable stored event ends the stream / read with `DATA_LOSS` at its position instead of being skipped. | #351 / #359 | v0.17.0 |

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
