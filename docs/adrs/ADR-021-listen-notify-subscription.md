# ADR-021: Hybrid LISTEN/NOTIFY for PostgreSQL Subscription Streaming

## Status

Accepted

## Date

2026-04-06

## Context

The PostgreSQL event store backend uses `stream::unfold()` with a two-phase state machine for subscriptions:

1. **Replay phase** — fetches historical events from the database.
2. **Live phase** — polls for new events.

The Live phase previously polled every 200ms using `tokio::time::interval(Duration::from_millis(200))`. This introduced up to 200ms latency per poll cycle before a subscriber saw new events. In production, this compounded with gRPC streaming overhead, sequential projection dispatch, and checkpoint persistence to produce ~2 seconds of end-to-end staleness.

The in-memory backend already uses `tokio::sync::broadcast` for instant event delivery (~0ms). We needed to bring the PostgreSQL backend to comparable latency.

## Decision

Replace the 200ms polling loop with a **hybrid LISTEN/NOTIFY + polling fallback** approach:

### 1. pg_notify() on append

The `append()` method fires `SELECT pg_notify('eventstore_events', ...)` inside the transaction, just before commit. PostgreSQL guarantees the notification is only delivered after commit. The payload format is `"{tenant_id}:{last_global_nonce}"` — compact, parseable, well under PG's 8KB NOTIFY limit.

Additionally, the store broadcasts the notification in-process via `tokio::sync::broadcast` for subscribers on the same instance.

### 2. Background PgListener task

A dedicated `PgListener` connection (outside the connection pool) runs `LISTEN eventstore_events` in a background `tokio::spawn` task. Received notifications are parsed and forwarded to the broadcast channel. The task uses exponential backoff (1s → 30s cap) for reconnection on failure.

### 3. Hybrid wake in subscribe() Live phase

The Live phase uses `tokio::select!` to wake on whichever comes first:

- **Broadcast notification** — immediate wake when a relevant event is appended.
- **Fallback polling interval** (5 seconds) — safety net for missed notifications.

After waking, the subscription always queries the database — the notification is a hint, not a source of truth. This preserves the correctness guarantees of ADR-013 (cursor-after-yield semantics).

## Design Choices

### Single channel vs per-tenant

We use a single channel `eventstore_events` with tenant filtering in the broadcast receiver. Per-tenant channels would require per-subscriber dedicated connections or dynamic LISTEN/UNLISTEN management. The filtering cost (one string comparison per notification) is negligible.

### pg_notify in Rust vs DB trigger

The `append()` method is the only write path for events (UPDATE/DELETE are blocked by immutability triggers). Keeping the notification in Rust code is testable, visible in code review, and fires once per batch (vs per-row for a trigger).

### Broadcast buffer capacity

256 messages. If a subscriber falls behind (receives `RecvError::Lagged`), it queries the database immediately. The broadcast is best-effort; the database is always authoritative.

### Fallback poll interval

5 seconds. Short enough that missed notifications are caught quickly, long enough to be negligible load. This is a safety net, not the primary delivery path.

## Consequences

### Positive

- Event delivery latency reduced from ~200ms to <50ms (typical <10ms).
- Existing subscription tests pass without modification (and run faster).
- ADR-013 cursor semantics are fully preserved.
- Graceful degradation: if PgListener drops, subscribers fall back to 5s polling.
- No SQL migrations required — `pg_notify()` and `LISTEN` are built-in PostgreSQL features.
- Compatible with TimescaleDB (LISTEN/NOTIFY works identically on hypertables).

### Negative

- One additional dedicated PostgreSQL connection for the listener (outside the pool).
- `PostgresStore::new()` now requires `database_url` parameter (minor API change).
- `tokio::spawn` in constructor ties the listener lifecycle to the `Arc<PostgresStore>`.

### Neutral

- No changes to the `EventStore` trait, protobuf schema, or gRPC API.
- No changes to SQL migrations.

## References

- [ADR-013: Subscribe Cursor After Yield](ADR-013-subscribe-cursor-after-yield.md) — Cursor semantics preserved by this change.
- [PostgreSQL LISTEN/NOTIFY documentation](https://www.postgresql.org/docs/current/sql-notify.html)
- [sqlx PgListener API](https://docs.rs/sqlx/latest/sqlx/postgres/struct.PgListener.html)
