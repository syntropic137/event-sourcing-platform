# Event Store Proto, Codegen, Clients, and V1 Design Notes

This document explains what the `.proto` file is, how gRPC stubs are generated for Rust/TypeScript/Python, payload strategies, backend injection (in-memory vs Postgres), migrations, subscriptions, and testing/operational defaults for v1.

## Overview
- The API surface is defined once in `eventstore/v1/eventstore.proto` (see plan in `PROJECT-PLAN_RUST-EVENT-STORE_20250826.md`).
- We generate language clients from `.proto` so Rust, TS, and Python can interoperate against the same service.
- Backend is injected via a Rust `EventStore` trait, allowing in-memory or Postgres with the same gRPC facade.

- [Event Store Proto, Codegen, Clients, and V1 Design Notes](#event-store-proto-codegen-clients-and-v1-design-notes)
  - [Overview](#overview)
  - [What is a .proto (Protocol Buffers) file?](#what-is-a-proto-protocol-buffers-file)
  - [Rust codegen: build.rs + tonic-build](#rust-codegen-buildrs--tonic-build)
  - [TypeScript and Python stubs](#typescript-and-python-stubs)
  - [Event payload strategies: Protobuf vs Opaque bytes](#event-payload-strategies-protobuf-vs-opaque-bytes)
  - [Backend injection (in-memory vs Postgres)](#backend-injection-in-memory-vs-postgres)
  - [Postgres migrations (Rust-based options)](#postgres-migrations-rust-based-options)
  - [Subscribe implementation v1](#subscribe-implementation-v1)
  - [Filter semantics (Subscribe)](#filter-semantics-subscribe)
  - [Backpressure and windowing defaults](#backpressure-and-windowing-defaults)
  - [IDs and timestamps](#ids-and-timestamps)
  - [TDD with Given-When-Then (G/W/T)](#tdd-with-given-when-then-gwt)
  - [Transport and tenancy posture](#transport-and-tenancy-posture)
  - [Client SDKs: minimal v1 scope](#client-sdks-minimal-v1-scope)
  - [Suggested monorepo structure (sub-crates)](#suggested-monorepo-structure-sub-crates)
  - [Notes against the plan](#notes-against-the-plan)
  - [Getting Started](#getting-started)
  - [Pluggable backends (Memory, Postgres, Kurrent future)](#pluggable-backends-memory-postgres-kurrent-future)


## What is a .proto (Protocol Buffers) file?
- It declares message types and gRPC services.
- Example (from the plan): package `eventstore.v1` defines `EventMetadata`, `EventData`, and service `EventStore` with RPCs `Append`, `ReadStream`, `Subscribe`.
- Code generators (protoc plugins) produce typed client/server code per language.

## Rust codegen: build.rs + tonic-build
- `tonic-build` invokes `protoc` at compile time (via `build.rs`) to generate Rust types and service traits.
- Your Rust crate then `include`s the generated code: `tonic::include_proto!("eventstore.v1");` matching the `package` in `.proto`.
- Result: you get Rust structs for messages and a trait to implement for the gRPC server.

## TypeScript and Python stubs
- TypeScript: use `ts-proto` (preferred for idiomatic TS types) or `@grpc/grpc-js` with official protoc plugins.
- Python: use `grpcio-tools` to generate `_pb2.py` (messages) and `_pb2_grpc.py` (clients/servers).
- All generated from the same `eventstore/v1/eventstore.proto`.

## Event payload strategies: Protobuf vs Opaque bytes
- Protobuf payloads: encode domain event messages as Protobuf and place bytes in `EventData.payload`. Pros: schema evolution, strong typing across languages.
- Opaque bytes: any format (JSON, Avro, custom) with metadata `type_name` + `content_type` guiding consumers. Pros: flexibility; Cons: less type-safety.
- Your proto already models payload as raw bytes plus metadata, so both are supported.
- V1 recommendation: default to Protobuf-encoded payloads for core services; allow opaque for experimental producers (same API).

## Backend injection (in-memory vs Postgres)
- Define `EventStore` trait (as in the plan). Implement:
  - `InMemoryStore` for fast local dev and tests.
  - `PostgresEventStore` for persistence.
- Wire either backend into the gRPC service at startup by passing `Arc<dyn EventStore>` to `EventStoreSvc`.

## Postgres migrations (Rust-based options)
- sqlx migrations (recommended v1): versioned SQL files, run via `sqlx migrate run`. Popular and straightforward.
- refinery: Rust API to run SQL/embedded migrations programmatically.
- Tables per plan: `events` (append-only) with unique constraints on `(stream_id, stream_version)` and `event_id`, index on `global_position`; optional `snapshots`.

## Subscribe implementation v1
- Start with polling using `global_position > last_seen`:
  - Simple to implement, robust, and easy to reason about idempotency.
  - Tunable poll interval; backfill guarantees ordering via `global_position`.
- Consider LISTEN/NOTIFY in v2 for low latency (still combine with backfill by position).

## Filter semantics (Subscribe)
- Support `stream_prefix` as in the proto to follow naming like `Order-123`.
- Can add `stream_type` later if needed; `prefix` covers many cases without extra indices.

## Backpressure and windowing defaults
- `ReadStream.max_count` limits page size.
- Streaming `Subscribe`: rely on gRPC flow control; keep batch sizes moderate and checkpoint by `global_position`.
- Optional keepalive/heartbeat later; gRPC keepalive settings usually suffice initially.

## IDs and timestamps
- IDs: UUID v7 for `event_id`.
- Timestamps: assign server-side on append for consistency.
  - In-memory: `chrono::Utc::now()`.
  - Postgres: `NOW()`/`clock_timestamp()` in SQL gets authoritative DB time.

## TDD with Given-When-Then (G/W/T)
- Aggregate tests:
  - Given: historical events loaded via `ReadStream`.
  - When: command handled by aggregate to produce new domain events.
  - Then: expect emitted events and append with optimistic concurrency (Exact(current_version)).
- Service tests (Rust): use a tonic client to assert `Append`, `ReadStream`, `Subscribe` behaviors, including CAS failures and monotonic `global_position`.

## Transport and tenancy posture
- Dev: plaintext gRPC.
- Future: TLS for transport security.
- v1: propagate metadata fields (e.g., `tenant_id`, `correlation_id`, `causation_id`) without enforcing authZ; authorization in v2.

## Client SDKs: minimal v1 scope
- Rust SDK: thin wrapper over generated client with ergonomics for append/read/subscribe.
- TypeScript SDK: generated types plus a small helper to map application events to `EventData` and back.
- Python SDK: similar thin helper around generated stubs.

## Suggested monorepo structure (sub-crates)
- `eventstore-proto/` → contains `.proto` and build scripts for codegen
- `eventstore-core/` → `EventStore` trait, domain envelopes, shared types
- `eventstore-inmem/` → in-memory backend (dev/testing)
- `eventstore-pg/` → Postgres backend + migrations (sqlx)
- `eventstore-bin/` → gRPC server binary; DI to choose in-memory or Postgres
  - `sdks/sdk-rs/` → Rust client SDK
  - `sdks/sdk-ts/` → TS client SDK
  - `sdks/sdk-py/` → Python client SDK

## Notes against the plan
- API surface and semantics match `PROJECT-PLAN_RUST-EVENT-STORE_20250826.md` (Append CAS, ReadStream paging, Subscribe by `global_position`).
- V1 emphasizes simplicity (polling, server-side timestamps, sqlx migrations), with clear paths to v2 (LISTEN/NOTIFY, TLS, authZ).

## Getting Started

Below are minimal steps to generate stubs and run the service locally. Adjust paths to your repo layout.

1) Rust server (tonic) codegen

- Add deps in `Cargo.toml`: `tonic`, `prost`, `prost-types`, `tokio`, `uuid`, `anyhow`, `thiserror`.
- In your proto crate, add a `build.rs` to run `tonic-build` over `eventstore/v1/eventstore.proto`.
- In your Rust crate, include generated code:

```rust
pub mod pb { tonic::include_proto!("eventstore.v1"); }
```

- Build to trigger codegen: `cargo build`.

2) TypeScript client stubs (ts-proto)

- Install: `npm i -D ts-proto protoc` (ensure `protoc` is on PATH).
- Generate:

```bash
protoc \
  --plugin="protoc-gen-ts_proto=$(npm root)/.bin/protoc-gen-ts_proto" \
  --ts_proto_out=./sdks/sdk-ts/src/gen \
  --ts_proto_opt=esModuleInterop=true,useOptionals=messages \
  -I ./eventstore-proto \
  eventstore/v1/eventstore.proto
```

- Use with `@grpc/grpc-js` or `nice-grpc`.

3) Python client stubs (grpcio-tools)

- Install: `python -m pip install grpcio grpcio-tools`
- Generate:

```bash
python -m grpc_tools.protoc \
  -I ./eventstore-proto \
  --python_out=./sdks/sdk-py/gen \
  --grpc_python_out=./sdks/sdk-py/gen \
  eventstore/v1/eventstore.proto
```

4) Run the server (in-memory backend)

- Start the tonic server binary (example listens on `:50051`).
- Quick check with grpcurl:

```bash
grpcurl -plaintext localhost:50051 list
grpcurl -plaintext -d '{"stream_id":"Order-1","from_version":1,"max_count":10,"forward":true}' \
  localhost:50051 eventstore.v1.EventStore/ReadStream
```

5) Run with Postgres backend (when ready)

- Start Postgres, run migrations (e.g., `sqlx migrate run`).
- Configure the server to construct `PostgresEventStore` and inject via `Arc<dyn EventStore>`.

## Pluggable backends (Memory, Postgres, Kurrent future)

The gRPC surface is stable; storage is pluggable behind the `EventStore` trait. Today we support in-memory and (planned) Postgres; future-proof for Kurrent DB.

- Trait-first boundary: the gRPC service depends only on `EventStore` (append, read_stream, subscribe).
- Adapters:
  - `InMemoryStore`: dev/test, already outlined in the plan.
  - `PostgresEventStore`: append-only table with unique constraints and `global_position` index; polling subscribe for v1.
  - `KurrentEventStore` (future): implement the same trait; map append/read/subscribe to Kurrent’s primitives.
- Dependency Injection (DI): at startup, select the backend by config/env and pass `Arc<dyn EventStore>` into `EventStoreSvc`.
- Semantic invariants across backends:
  - Optimistic concurrency via expected version (CAS) remains consistent.
  - Monotonic offsets via `global_position` (or backend equivalent) for catch-up and checkpointing.
  - Metadata fields (`tenant_id`, `correlation_id`, etc.) preserved end-to-end.

This modular design allows evolving from Memory → Postgres → Kurrent without API changes to clients.
