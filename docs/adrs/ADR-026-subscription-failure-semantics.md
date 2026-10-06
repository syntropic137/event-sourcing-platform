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
