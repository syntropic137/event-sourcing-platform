# 006-event-bus-ts — Work in Progress

This example will expand on the TypeScript SDK. It targets the local gRPC event store by default; use `--memory` for a fast in-memory run. Track progress in `EXAMPLES_PLAN.md`.

## Run

```bash
./dev-tools/dev start
pnpm --filter ./examples/006-event-bus-ts run start
```

Add `-- --memory` to run without the gRPC backend.
