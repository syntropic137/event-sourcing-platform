# 002 Simple Aggregate (TypeScript + gRPC)

This example demonstrates a basic event-sourced aggregate using the Event Sourcing TS SDK and the gRPC Event Store.

## Prerequisites

- Rust event store server running locally at `localhost:50051`:

```bash
cd ../../event-store
make run
```

## Run

```bash
cd examples/002-simple-aggregate-ts
pnpm install
pnpm run dev
```
