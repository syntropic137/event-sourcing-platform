# TypeScript SDK (experimental)

Two ways to use the client:

- **Runtime client** (`EventStoreClientRT`): no codegen required. File: `src/runtime-client.ts`.
- **Typed client** (`EventStoreClientTS`): uses ts-proto generated stubs in `src/gen/`.

## Prereqs

- Node 18+ and npm
- Running Event Store server (default `localhost:50051`). Example:

```bash
# from repo root
make run -C experiments/005-rust-event-store
```

## Install deps

```bash
npm --prefix experiments/005-rust-event-store/sdks/sdk-ts install
```

## Generate typed stubs (optional but recommended)

```bash
# Requires protoc
brew install protobuf
make gen-ts -C experiments/005-rust-event-store
```

## Run examples

Runtime client example:

```bash
node --experimental-specifier-resolution=node \
  experiments/005-rust-event-store/sdks/sdk-ts/node_modules/ts-node/dist/bin.js \
  experiments/005-rust-event-store/sdks/sdk-ts/src/examples/basic.ts
```

Typed client example:

```bash
node --experimental-specifier-resolution=node \
  experiments/005-rust-event-store/sdks/sdk-ts/node_modules/ts-node/dist/bin.js \
  experiments/005-rust-event-store/sdks/sdk-ts/src/examples/typed-basic.ts
```

## Importing in your app

```ts
import { EventStoreClientTS } from "@eventstore/sdk-ts";
import { Expected, EventMetadata } from "@eventstore/sdk-ts";

const client = new EventStoreClientTS("localhost:50051");
await client.append({
  aggregateId: "Order-1",
  aggregateType: "Order",
  expectedAny: Expected.ANY,
  events: [{ meta: EventMetadata.create({ eventType: "OrderCreated" }), payload: Buffer.from("hello") }],
});
```
