# Examples Overview

All TypeScript examples default to the local dev-tools gRPC event store. Start the stack with `./dev-tools/dev start`. To run any example against the in-memory client, append `-- --memory` to the `pnpm run start` invocation.

## Quick smoke test

```bash
make examples-run
```

The target builds and runs each example sequentially using the gRPC backend.

## Catalogue

| Example                    | Status | Concept                                             |
| -------------------------- | ------ | --------------------------------------------------- |
| 001-basic-store-ts         | ✅     | Raw event store usage (append/read/exists)          |
| 002-simple-aggregate-ts    | ✅     | Aggregates with decorators & optimistic concurrency |
| 003-multiple-aggregates-ts | ✅     | Multiple aggregates working together                 |
| 004-cqrs-patterns-ts       | ✅     | Command/Query separation with read models           |
| 005-projections-ts         | ✅     | Event-driven projections and analytics              |
| 006-event-bus-ts           | ✅     | Cross-aggregate communication via events            |
| 007-inventory-complete-ts  | 🔄     | Complete inventory management system                 |
| 008-observability-ts       | 🔄     | System monitoring and health metrics                |
| 009-web-dashboard-ts       | 🔄     | Live HTML dashboard showing projections             |

## Possible Future Examples

Additional examples that could be implemented to demonstrate advanced patterns:

| Example                    | Concept                                             |
| -------------------------- | --------------------------------------------------- |
| 010-ecommerce-complete-ts  | Full e-commerce system with multiple contexts      |
| 011-banking-complete-ts    | Complete banking system with compliance            |
| 012-sagas-ts               | Long-running processes and saga patterns           |
| 013-multi-tenant-ts       | Multi-tenant event sourcing architecture           |
| 014-event-versioning-ts   | Event schema evolution and upcasting               |
| 015-performance-ts         | Performance optimization and benchmarking          |

Progress and detailed tasks are tracked in `EXAMPLES_PLAN.md`.
