# Postgres Backend: Connection Pool and Timeouts

Issues: [#368](https://github.com/syntropic137/event-sourcing-platform/issues/368)
(timeouts, keepalives), [#370](https://github.com/syntropic137/event-sourcing-platform/issues/370)
(pool size). Related: [ADR-026](../adrs/ADR-026-subscription-failure-semantics.md),
[BACKUP-RESTORE.md](BACKUP-RESTORE.md), [POSTGRES-BASELINE.md](../performance/POSTGRES-BASELINE.md).

`eventstore-bin` with `BACKEND=postgres` reads these environment variables at
startup, next to `DATABASE_URL` and `BIND_ADDR`. Unset means the default. An
invalid value (not a non-negative integer, out of range, min above max) stops
startup with an error naming the variable; it is never silently replaced by a
default. The effective settings are logged at `info` on startup.

| Variable | Default | Meaning |
|---|---|---|
| `PG_POOL_MAX_CONNECTIONS` | `10` | Most connections in the pool (at least 1). |
| `PG_POOL_MIN_CONNECTIONS` | `0` | Connections kept open while idle (at most the max). |
| `PG_ACQUIRE_TIMEOUT_MS` | `10000` | Longest wait for a pooled connection, including opening one. At least 1. |
| `PG_STATEMENT_TIMEOUT_MS` | `30000` | Server `statement_timeout`. Also sets the client deadline (below). `0` disables both. |
| `PG_LOCK_TIMEOUT_MS` | `10000` | Server `lock_timeout`: longest wait for a row lock or the append-order advisory lock. `0` disables. |
| `PG_IDLE_IN_TRANSACTION_TIMEOUT_MS` | `10000` | Server `idle_in_transaction_session_timeout`. `0` disables. |
| `PG_TCP_KEEPALIVE_SECS` | `30` | Server `tcp_keepalives_idle`; interval is a third of it, 3 probes. `0` keeps the server's defaults. |

The server settings are sent as session options in the connection startup
packet, so they apply to every pooled connection and need no server-side
configuration. Options given in `DATABASE_URL` (`?options=...`) are kept,
but these are appended after them and win for the same setting: use the
variables above, not the URL, for these timeouts. Migrations run on a separate connection
without these timeouts, so a slow migration is not killed at startup.

## What bounds what

Two layers, because neither alone covers a silent network failure.

**Server side** (Postgres enforces, the store sees an error):

- `statement_timeout` cancels a statement that runs too long (SQLSTATE
  `57014`).
- `lock_timeout` fails an append that waits too long for a row lock or the
  per-tenant append-order lock (`55P03`), for example behind a stalled
  holder on another node.
- `idle_in_transaction_session_timeout` terminates a session that sits in an
  open transaction without sending anything (`25P03`). This is what frees
  the **per-tenant append-order lock** when the event store's side of the
  connection vanishes mid-append: the transaction rolls back and other
  appenders of that tenant (on any node) continue. The store itself never
  pauses inside a transaction for more than one round trip, so the default
  is far above any legitimate gap.
- TCP keepalives let the server drop idle sessions whose client is gone.
  Ignored on Unix-socket connections.

**Client side** (the store stops waiting):

- **Client deadline** = `PG_STATEMENT_TIMEOUT_MS` + 5 s. Every append
  (the whole transaction), `ReadStream`, `ReadAll` page and subscription
  query must finish within it. When the network silently drops packets
  (paused VM, firewall black hole) the server cannot report its own timeout,
  so this deadline is what turns a hang into an error. The 5 s grace lets the
  server's timeout fire first when the server is reachable, giving a precise
  error. The deadline covers the whole operation, so an append whose lock
  waits and statements are each within their server limits but together
  exceed it still fails (`UNAVAILABLE`, retryable); size
  `PG_STATEMENT_TIMEOUT_MS` for the longest operation you accept, not the
  longest single statement.
- A connection whose operation did not finish (deadline passed, or the
  caller gave up first: gRPC deadline, client disconnect) is closed, never
  returned to the pool, so a stalled connection cannot hold a pool slot.
- `PG_ACQUIRE_TIMEOUT_MS` bounds waiting for a connection, including
  connecting to (or health-checking a connection to) an unreachable server.
- The dedicated LISTEN/NOTIFY connection (it has its own one-connection
  pool and does not use a pool slot) is probed with `SELECT 1` after 30 s
  without traffic and reconnected if the probe fails or takes over 10 s.
  Until then subscriptions still see new events through their 5 s fallback
  poll.

All of these surface to gRPC clients as `UNAVAILABLE` (ADR-026), as do
other lost-connection errors (`08xxx`, shutdown `57P0x`, too many connections
`53300`, pool timeouts, I/O errors). Before #368 several of these were
`INTERNAL`.

Worst-case time to an error on a black-holed path: about
`PG_ACQUIRE_TIMEOUT_MS` + client deadline for a request that arrives during
the stall (it first waits for a connection), and the client deadline for one
already running. A live subscription adds up to its 5 s fallback poll,
because an idle subscription issues no query. With the defaults: an
in-flight append fails within 35 s, a new one within 45 s.
**(drilled** with 2 s / 3 s settings: `drill_blackhole`, see
[BACKUP-RESTORE.md](BACKUP-RESTORE.md).**)**

## Appends that time out

A timed-out append has an unknown outcome, exactly like a lost
acknowledgment: if the deadline passed during `COMMIT`, it may have
committed. Retry the identical request with the same idempotency key (see
[Appends: acknowledgments and retries](BACKUP-RESTORE.md#appends-acknowledgments-and-retries)).
If it did not commit, its transaction is rolled back by the server (at the
latest after `PG_IDLE_IN_TRANSACTION_TIMEOUT_MS` once the path is dead), so
nothing partial is ever visible and commit order is unaffected.

## Subscriptions

A subscription is not one long query. Replay and live delivery read keyset
pages of at most 1000 rows
([#369](https://github.com/syntropic137/event-sourcing-platform/issues/369));
the live phase queries only per wake-up (NOTIFY or 5 s fallback poll). Every
page query goes through the same pool, session timeouts and client deadline,
and each reads one page, so neither `statement_timeout` nor the deadline
grows with history, and idle live subscriptions run no query that could
time out. A catch-up from far behind is many short queries, never one long
one.

## Sizing the pool

- Appends to **one tenant** are serialized by the per-tenant append-order
  advisory lock (commit order = `global_nonce` order; never remove it). More
  connections do not raise one tenant's ceiling: extra appenders wait on the
  lock while holding a connection. What raises it is a shorter lock hold
  (fewer round trips while it is held; see POSTGRES-BASELINE.md).
- Connections are what let **different tenants**, reads (`ReadStream`,
  `ReadAll`) and subscription queries proceed alongside appends. With the old
  fixed 5, a handful of same-tenant appenders waiting on the lock occupied
  the whole pool and starved reads and subscription polls of every tenant.
- Budget `PG_POOL_MAX_CONNECTIONS` x instances + 1 LISTEN connection per
  instance + other clients below the server's `max_connections` (Postgres
  default 100), leaving headroom for maintenance sessions.
- `PG_POOL_MIN_CONNECTIONS` only avoids connect latency after idle periods.

## Known limits

- **Migrations at startup** are not bounded once connected (connecting is,
  by `PG_ACQUIRE_TIMEOUT_MS`): a legitimate migration may take long, so no
  timeout fits. A path that stalls mid-migration hangs startup; the process
  never becomes ready, which readiness probes see.
- **LISTEN reconnects**: sqlx cleans up a dropped listener connection in a
  background task without a timeout. On a dead path each such task (at most
  one per reconnect, 40 s or more apart) lives until the OS abandons the
  socket. It never holds a slot of the main pool.
