# eventstore-proto

Protobuf definitions and generated Rust types for the Event Store gRPC API.

- `.proto` sources in `proto/`
- Build script compiles protos with `prost` + `tonic`
- Reusable by server, backends, and SDKs

See `src/lib.rs` for the generated module layout.

---

## Protobuf 101 (quick explainer)

- **What it is**: Protocol Buffers ("protobuf") is a language-neutral, binary serialization format defined by `.proto` files. It describes
  - **Messages**: structured data you send/receive
  - **Services**: RPC endpoints (gRPC uses this to generate client/server code)

- **Why use it**:
  - Compact and fast on the wire (binary vs JSON)
  - Strongly typed contracts shared across services and languages
  - Automatic code generation for clients/servers

- **How it looks** (simplified example):

```proto
syntax = "proto3";
package eventstore.v1;

message AppendRequest { string stream_id = 1; }
message AppendResponse { uint64 last_global_nonce = 1; }

service EventStore {
  rpc Append(AppendRequest) returns (AppendResponse);
}
```

- **How it becomes Rust here**:
  - `build.rs` runs during `cargo build` and invokes vendored `protoc` via `protoc-bin-vendored`.
  - `tonic-build` + `prost-build` read files under `proto/` and generate Rust modules (messages + gRPC client/server types).
  - Generated code is compiled into this crate; consumers import types via the `eventstore_proto` lib.

- **Where to look in this repo**:
  - Protos: `eventstore-proto/proto/eventstore/v1/eventstore.proto`
  - Build logic: `eventstore-proto/build.rs` (calls `tonic_build::configure().compile_protos(...)`)
  - Generated module layout re-export: `eventstore-proto/src/lib.rs`

- **Using with Tonic (at a glance)**:

```rust
use eventstore_proto::eventstore::v1::event_store_client::EventStoreClient;
use eventstore_proto::eventstore::v1::AppendRequest;

# async fn demo() -> Result<(), Box<dyn std::error::Error>> {
let mut client = EventStoreClient::connect("http://localhost:50051").await?;
let req = tonic::Request::new(AppendRequest { stream_id: "Order-1".into() });
let _res = client.append(req).await?;
Ok(())
# }
```

That’s it—you define contracts in `.proto`, and `prost`/`tonic` generate the Rust types and gRPC plumbing used across the server, backends, and SDKs.
