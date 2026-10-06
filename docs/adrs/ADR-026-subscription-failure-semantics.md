# ADR-026: Subscription Failure Semantics

**Status:** Accepted
**Date:** 2026-10-06
**Related:** [ADR-013](ADR-013-subscribe-cursor-after-yield.md), [ADR-021](ADR-021-listen-notify-subscription.md), issue #350

## Context

The Postgres `subscribe()` turned a failed replay or live query into an
empty result set (`.await.unwrap_or_default()`). A failed replay then looked
like a successful catch-up: the stream sent its caught-up marker
(`event: None`) and moved to live. A failed live poll looked like "no new
events". Outages and query failures were hidden behind normal subscription
behavior, and consumers had no signal to reconnect or alert.

## Decision

Subscriptions never hide a failure. Retry belongs to the consumer, which owns
the checkpoint.

1. **Typed error, then end of stream.** When a replay or live query fails,
   the stream yields one `StoreError::Unavailable` and ends. Over gRPC this is
   status `UNAVAILABLE`. The message names the phase (`replay` or `live`) and
   the next undelivered position (`resume from global_nonce N`).
2. **No false catch-up.** The caught-up marker is sent only after a replay
   query succeeded. A replay failure ends the stream before any marker.
3. **Cursor is never advanced on failure.** The server-side cursor is the last
   delivered event (ADR-013). A failed query leaves it unchanged; the reported
   resume position is that cursor plus one.
4. **No server-side retry loop.** A store-internal retry would need a degraded
   status channel the protocol does not have. Ending the stream with a
   retryable status is explicit and works with every client.

## Undecodable stored events (#351)

Before #351, a row that could not be decoded was logged and skipped; later
valid events moved the cursor past it, and an all-invalid batch advanced the
cursor explicitly. Consumers built incomplete projections with no error.

Now the same "stop, never skip" rule applies:

1. Rows are decoded in `global_nonce` order. Events **before** the first
   undecodable row are delivered; then the stream yields
   `StoreError::UndecodableEvent { global_nonce, reason }` (gRPC `DATA_LOSS`)
   and ends. No caught-up marker, no later position, no cursor advance.
2. The error names the position and the column only. Decoder messages are
   dropped because they can quote stored values; payloads are never logged.
3. `read_all` / `read_stream` return the same error instead of panicking.
4. `DATA_LOSS` is not fixed by retrying. A consumer that reconnects from its
   checkpoint hits the same error at the same position. Alert on it.
5. No server-side skip or quarantine policy exists. Any future one must be
   opt-in, observable (metric/log per skipped position) and auditable.

### Operator recovery

1. **Identify.** Take `global_nonce` from the error or the store log
   (`subscription stopped at an undecodable stored event`). Inspect metadata,
   not payload:
   `SELECT tenant_id, aggregate_id, aggregate_nonce, event_id, event_type, event_version, content_type, jsonb_typeof(headers) FROM events WHERE global_nonce = <N>;`
2. **Choose a fix.**
   - *Reader cannot read valid data* (version skew, schema drift): deploy an
     event store version that decodes the row. Consumers resume unchanged.
   - *Row is corrupt and repairable*: events are append-only (the
     `trg_events_immutable_update` trigger blocks `UPDATE`). Under change
     control, in one transaction: `ALTER TABLE events DISABLE TRIGGER
     trg_events_immutable_update`, correct only the broken column of that one
     row, re-enable the trigger, commit. Record who, why, the position and the
     before/after column values in your change log.
   - *Row is unrecoverable*: skip it explicitly **per consumer** by setting
     that consumer's checkpoint to `N` (it resumes at `N + 1`). This is the
     only skip path: deliberate, per consumer, and auditable through the
     checkpoint write. Record the skipped position; the consumer's read model
     lacks that event and may need a rebuild or a compensating event.
3. **Verify.** Reconnect; the subscription passes `N` (or resumes at `N + 1`).

## Consumer contract (at-least-once)

- `from_global_nonce` is **inclusive**. Events arrive in `global_nonce` order.
- Persist a checkpoint (the `global_nonce` of the last event you processed)
  after processing. On any stream error or end, reconnect with
  `from_global_nonce = checkpoint + 1`. With no checkpoint, use your original
  start position.
- Prefer your own checkpoint over the position in the error message: an event
  can be delivered but not yet processed when the stream fails.
- Reconnecting from an earlier position than strictly necessary re-delivers
  events. Duplicates are allowed under this contract; consumers must be
  idempotent (ADR-014).
- Retry `UNAVAILABLE` with backoff. Every committed event at or after the
  reconnect position is delivered, including events committed during the
  outage.

### Client behavior

| Client | On subscription error |
|--------|-----------------------|
| gRPC server (`eventstore-bin`) | Logs the error, maps it with `StoreError::to_status()`, ends the response stream with that status. |
| Rust SDK (`sdk-rs`) | `tonic::Streaming` yields `Err(Status)` with `Code::Unavailable`. |
| TypeScript SDK (`sdk-ts`) | The async iterator rejects with the gRPC error (`code` 14). |
| Python (`event_sourcing` `GrpcEventStoreClient.subscribe`) | Raises `EventStoreError`. `SubscriptionCoordinator` retries with exponential backoff and resumes each projection from its saved checkpoint. |

## Consequences

- Outages are visible to consumers and to logs (`subscription ... query failed`).
- Consumers that ignored stream errors now see them; they must reconnect.
- Fault-injection tests (`eventstore-backend-postgres/tests/it_subscribe_faults.rs`)
  cover replay and live failures, with and without a prefix filter, and
  reconnect from the saved checkpoint.
- An undecodable row halts every consumer that reaches it until an operator
  acts. That is intended: a visible stop beats a silently incomplete
  projection. The Python coordinator retries `DATA_LOSS` with backoff like any
  other error, so it stays stopped at the position and logs each attempt.
- `it_subscribe_undecodable.rs` stores a decoder-invalid row between valid rows
  (replay and live) and an all-invalid batch, and checks that nothing is
  delivered past it and reconnecting fails at the same position.
