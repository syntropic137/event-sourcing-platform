# 005 – Rust Event Store (PoC)

A minimal gRPC-based Event Store implemented in Rust with an in-memory backend. Includes end-to-end service tests and a simple Makefile-driven workflow.

- [005 – Rust Event Store (PoC)](#005--rust-event-store-poc)
  - [Packages](#packages)
    - [SDKs](#sdks)
  - [Planned Backends](#planned-backends)
  - [Quickstart](#quickstart)
    - [How-To: Run the server](#how-to-run-the-server)
    - [How-To: Smoke test](#how-to-smoke-test)
    - [SDK Quickstarts](#sdk-quickstarts)
  - [Documentation](#documentation)
    - [How-To: Run the documentation site](#how-to-run-the-documentation-site)
  - [QA Pipeline](#qa-pipeline)
    - [Coverage](#coverage)
  - [Service Endpoints](#service-endpoints)
  - [Domain Vocabulary](#domain-vocabulary)
  - [Dev Notes](#dev-notes)
  - [Troubleshooting](#troubleshooting)


## Packages

- `eventstore-core/` — core traits, shared types, and errors. Re-exports protobuf-generated types.
  - See: [`eventstore-core/README.md`](eventstore-core/README.md)
- `eventstore-proto/` — protobuf definitions and generated Rust types for the gRPC API.
  - See: [`eventstore-proto/README.md`](eventstore-proto/README.md)
- `eventstore-backend-memory/` — in-memory backend implementing core traits; ideal for dev/tests.
  - See: [`eventstore-backend-memory/README.md`](eventstore-backend-memory/README.md)
- `eventstore-backend-postgres/` — Postgres-backed durable implementation.
  - See: [`eventstore-backend-postgres/README.md`](eventstore-backend-postgres/README.md)
- `eventstore-bin/` — tonic-based gRPC server binary and integration tests; selects backend via `BACKEND` env.
  - See: [`eventstore-bin/README.md`](eventstore-bin/README.md)

### SDKs

- `sdks/sdk-ts/` — TypeScript SDK (runtime client + optional ts-proto stubs).
  - See: [`sdks/sdk-ts/README.md`](sdks/sdk-ts/README.md)
- `sdks/sdk-py/` — Python SDK using grpcio (experimental).
  - See: [`sdks/sdk-py/README.md`](sdks/sdk-py/README.md)
- `sdks/sdk-rs/` — Rust SDK (thin client built on `eventstore-proto`).
  - See: [`sdks/sdk-rs/README.md`](sdks/sdk-rs/README.md)

## Planned Backends

- `eventstore-backend-kurrent/` — planned future backend integrating with KurrentDB. Not yet implemented.

## Quickstart

- Build: `make build`
- Run server: `make run` (env `BACKEND=memory` by default)
- Smoke test (grpcurl): `make smoke`
- See all make commands: `make help`

### How-To: Run the server

- **Memory backend (default):**
  - `make run`
  - Optional: `BIND_ADDR=0.0.0.0:50051 make run`

- **Postgres backend:**
  - Requires a Postgres instance and `DATABASE_URL`.
  - Example:
    - `BACKEND=postgres DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5432/postgres make run`
  - The server reads:
    - `BACKEND` = `memory` | `postgres`
    - `DATABASE_URL` when `BACKEND=postgres`
    - `BIND_ADDR` (default `0.0.0.0:50051`)

### How-To: Smoke test

- With the server running on `localhost:50051`:
  - `make smoke`
  - This issues a basic gRPC call to verify connectivity.

### SDK Quickstarts

- **TypeScript (runtime client or typed stubs):**
  - Install deps: `npm --prefix experiments/005-rust-event-store/sdks/sdk-ts install`
  - Generate stubs (optional, for typed client): `make gen-ts`
  - Run runtime example (reliable ESM invocation):
    ```bash
    node --experimental-specifier-resolution=node \
      experiments/005-rust-event-store/sdks/sdk-ts/node_modules/ts-node/dist/bin.js \
      experiments/005-rust-event-store/sdks/sdk-ts/src/examples/basic.ts
    ```
  - Run typed example (after gen-ts):
    ```bash
    node --experimental-specifier-resolution=node \
      experiments/005-rust-event-store/sdks/sdk-ts/node_modules/ts-node/dist/bin.js \
      experiments/005-rust-event-store/sdks/sdk-ts/src/examples/typed-basic.ts
    ```
  - Note: Direct `ts-node src/examples/basic.ts` may fail under ESM; use the above `node ... ts-node` form or `--loader ts-node/esm`.

- **Python:**
  - Generate stubs: `make gen-py` (requires `protoc` and `grpcio-tools`)
  - Run example: `python experiments/005-rust-event-store/sdks/sdk-py/examples/basic.py`

- **Rust:**
  - The Rust SDK uses the `eventstore-proto` crate types. See `sdks/sdk-rs/README.md`.

## Documentation

All documentation is organized in the [`docs/`](./docs/) directory with a comprehensive index at [`docs/index.md`](./docs/index.md).

### How-To: Run the documentation site

A Docusaurus-powered documentation site is available for a richer reading experience:

- **Start the docs site:** `make docs-start`
  - This installs dependencies and starts the site on `http://localhost:4321`
  - The site automatically syncs content from the `docs/` directory

- **Build for production:** `make docs-build`

- **Serve built site:** `make docs-serve`

- **See all docs commands:** `make docs`

## QA Pipeline

- Run all checks and tests: `make qa`
- Lints only: `make fmt` and `make clippy`

### Coverage

Coverage is integrated via `cargo-llvm-cov`.

- Install once: `make cov-setup`
- Generate LCOV + HTML: `make coverage`
  - Outputs: `coverage/lcov.info`, `coverage/html/index.html`
- During QA (text summary + threshold): `make qa`
  - Uses `COVERAGE_MIN` (default `90`) to fail QA if line coverage is below threshold.
  - If you haven't installed the tool yet, run `make cov-setup` first.

## Service Endpoints

- `Append`
- `ReadStream` (forward and backward reads)
- `Subscribe` (replay + live; supports `stream_prefix` filtering)

## Domain Vocabulary

See the glossary in `docs/ubiquitous-language.md` for definitions of:
- Event, EventData, EventMetadata
- Stream, Stream Type, Stream Version, global nonce
- Expected Version (Any | NoStream | StreamExists | Exact)
- Append, ReadStream, Subscribe
- Aggregate, Projection, Snapshot (with communication warning)
- Concurrency (OCC), Idempotency, Content types, Schema evolution

## Dev Notes

- Tests start a real tonic server on ephemeral ports and use the generated client.
- The in-memory backend is designed for clarity; persistence/backpressure are out of scope for this PoC.

## Troubleshooting

- Missing coverage tool: run `make cov-setup` (installs `cargo-llvm-cov`).
- grpcurl not found: `brew install grpcurl` or `cargo install grpcurl`.
