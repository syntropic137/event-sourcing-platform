---
sidebar_position: 1
---

# Quickstart

1. Install dependencies with `pnpm install`.
2. Start the dev-tools stack (gRPC + Postgres):
   ```bash
   ./dev-tools/dev init   # first time only
   ./dev-tools/dev start
   ```
3. Run `make examples-run` to execute all TypeScript examples against the live event store.
4. To run a single example in memory mode, append `-- --memory` to its `pnpm run start` command.

> CI tip: set `FORCE_TESTCONTAINERS=1` (or rely on the `CI` env var) to skip dev-tools shortcuts and fall back to pure Testcontainers.
