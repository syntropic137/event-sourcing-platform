---
sidebar_position: 2
---

# Running with Postgres & gRPC

To spin up Postgres and the event store locally, run:

```bash
docker-compose -f docker-compose.examples.yml up --build
```

This exposes the gRPC endpoint on `localhost:50051` and a Postgres instance on port `5435`.

Set your environment variables when testing SDKs or examples:

```bash
export EVENT_STORE_MODE=grpc
export EVENT_STORE_ADDR=127.0.0.1:50051
export EVENT_STORE_TENANT=example-tenant
```

Example 001 will now talk to the live stack (`pnpm run start`).

To tear down the services:

```bash
docker-compose -f docker-compose.examples.yml down
```
