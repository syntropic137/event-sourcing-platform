# Example 002 — Simple Aggregate (TypeScript)

Event-sourced aggregate lifecycle (submit/cancel) using the TypeScript SDK. The example targets the dev-tools gRPC event store by default and supports an in-memory override for quick experiments.

## Prerequisites

```bash
./dev-tools/dev init   # first time only
./dev-tools/dev start
```

## Running

```bash
cd examples/002-simple-aggregate-ts
pnpm run build
pnpm run start
```

To run in-memory instead of gRPC:

```bash
pnpm run start -- --memory
```

## Behaviour

1. Submit an order and persist via the repository (version 1).
2. Load the aggregate and inspect its status.
3. Cancel the order, persist again (version 2).
4. Reload to confirm replayed state.

The script registers domain events with the serializer so that gRPC mode works transparently. Environment overrides (`EVENT_STORE_ADDR`, `EVENT_STORE_TENANT`) are supported when targeting a non-dev stack.
