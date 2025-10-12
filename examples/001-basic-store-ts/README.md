# Example 001 — Basic Event Store (TypeScript)

Minimal append/read/exists loop against the platform event store. By default the script targets the local dev-tools stack (gRPC + Postgres). Pass `--memory` to run entirely in-memory.

## Prerequisites

```bash
./dev-tools/dev init   # first time only
./dev-tools/dev start
```

## Running

```bash
cd examples/001-basic-store-ts
pnpm run build
pnpm run start
```

To fall back to the in-memory client:

```bash
pnpm run start -- --memory
```

## Behaviour

1. Append `UserRegistered` and `UserEmailChanged` events (optimistic concurrency).
2. Read the stream back and log the payloads.
3. Check stream existence.
4. Confirm a non-existent stream returns no events.

Environment variables `EVENT_STORE_ADDR` and `EVENT_STORE_TENANT` can be used to point at a different gRPC endpoint if needed.
