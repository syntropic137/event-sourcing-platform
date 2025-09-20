---
sidebar_position: 1
---

# Event Sourcing TypeScript SDK

Start here to install the TypeScript SDK, choose a client, and run the examples. The CLI targets the dev-tools gRPC stack by default; append `--memory` to run without external services.

## Install

```bash
pnpm install @event-sourcing-platform/typescript
```

## Run the examples

```bash
./dev-tools/dev start
make examples-run
```

This builds and runs every example against the live event store. Individual examples can be run with `pnpm --filter ./examples/<name> run start` and the `--memory` flag if desired.
