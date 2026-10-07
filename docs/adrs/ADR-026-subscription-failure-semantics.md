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

## Paged replay (#369)

Postgres replay and live delivery read keyset pages (`global_nonce > cursor
ORDER BY global_nonce LIMIT page`, default 1000 rows), fetching the next page
only after the consumer has taken the previous one. The rules above apply per
page query:

1. A page query failure mid-replay yields `UNAVAILABLE` with the last
   delivered position plus one; events already fetched in the previous page
   are delivered first, and no caught-up marker follows.
2. The caught-up marker is sent once, after the first page shorter than the
   page size (an empty page when the history ends on a page boundary).
3. While live, a full page is followed by another query right away, not by a
   wait for NOTIFY or the fallback poll.
4. Each page reads its own snapshot. That cannot skip a nonce because a
   tenant's appends commit in `global_nonce` order (#337): every snapshot
   holds a gap-free prefix of the tenant's log, the guarantee live polling
   already relies on.

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
   The position is also sent as gRPC trailing metadata
   `esp-undecodable-global-nonce` so clients need not parse the message; the
   Python client raises `UndecodableEventError(global_nonce)`.
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
     With the Python `SubscriptionCoordinator`, move the checkpoint of every
     projection it runs that has not passed `N`: one failing track cancels
     the whole plan. This works even when `N` is the tenant head: the head
     probe maps `DATA_LOSS` on the head event to its position (via
     `UndecodableEventError.global_nonce`) instead of failing.
3. **Verify.** Reconnect; the subscription passes `N` (or resumes at `N + 1`).
   With the Python `SubscriptionCoordinator`: in the default mode call
   `start()` again; with `undecodable_recheck_interval` set it re-checks on
   its own. `halted` returns to `None` once it is past `N`.

### Python `SubscriptionCoordinator` on `DATA_LOSS` (#360)

`DATA_LOSS` is not retried with the transient-error backoff. On an
`UndecodableEventError` from any track the coordinator halts:

- Every track is cancelled. No checkpoint is moved past `N`: no projection
  was handed the event. ProcessManager drains are stopped and held (re-check
  attempts do not wake them), so no side effect runs while halted; they are
  woken when the halt clears.
- It logs one `ERROR` per position and sets `halted` to a
  `SubscriptionHaltedError` (`.global_nonce`, message points here);
  `is_healthy` is `False`. Alert on either.
- Default (`undecodable_recheck_interval=None`): `start()` raises the
  `SubscriptionHaltedError`. Fix the cause, then call `start()` again.
- `undecodable_recheck_interval=<seconds>`: `start()` stays running, halted,
  and re-plans at that fixed pace (logging at `DEBUG`), resuming on its own
  after the operator acts.
- `halted` clears only when a plan needs nothing at or below `N` (every
  projection was moved past it), or every track that started at or below `N`
  delivered an event at or past `N` (the row decodes again). A partial
  checkpoint move leaves it halted.

`UNAVAILABLE` and other errors are still retried with exponential backoff.

### Rust `ProjectionRunner::run_supervised` on `DATA_LOSS` (#372)

Same semantics as the Python coordinator, for one runner:

- The client maps `DATA_LOSS` with the `esp-undecodable-global-nonce`
  trailer to `Error::DataLoss { global_nonce }`. It is never retried with
  the transient backoff and never skipped.
- Catch-up applies every valid event before the position (a failing
  `read_all` page is re-read one event at a time up to it), so the
  checkpoint ends at the event just below `N` and an operator skip skips
  only `N`. The head probe maps `DATA_LOSS` on the head event to its
  position, as in Python.
- It logs one `ERROR` per position, sets `RunnerHealth::halted_at` and
  state `Halted`, and holds the `LiveProcessor`: an in-flight pass is
  cancelled, as in Python, and no pass runs while halted; it is woken when
  the halt clears.
- Default: `run_supervised` returns `Error::DataLoss`. With
  `with_undecodable_recheck(interval)` it stays halted and re-checks at
  that fixed pace. The halt clears once the loaded checkpoint is at or past
  `N`, or the store delivers an event at or past `N`.

Other transient failures (`UNAVAILABLE`, `RESOURCE_EXHAUSTED`, transport,
Postgres projection-store connection loss) reconnect from the checkpoint
with jittered exponential backoff; everything else stops with a typed
error.

## Projection handler failures (syntropic137#1696)

The same "stop, never skip" rule applies one level up, in the Python
`SubscriptionCoordinator`, when the store delivered an event fine but a
projection failed to apply it (`handle_event` returned `FAILURE` or raised).

Before, the coordinator logged it ("event will be retried") and the track
moved on. Nothing retried it: the projection's next successful event saved a
higher checkpoint, and nothing re-reads below a checkpoint. One transient
projection-store error lost the event for good while the read model reported
itself current. Production lost two `WorkflowExecutionStarted` events this way.

Now:

1. **The failing projection is held below the event.** Its checkpoint is not
   advanced past it by anything: not by its next event, not by a skip. Under
   `start()` it is taken off its track; under `dispatch_event()` the call
   raises `ProjectionHandlerFailedError` and later events are not handed to
   that projection until the failed one is delivered again and applied.
2. **Only that projection waits.** Every other projection, on the same track
   or another, keeps consuming. One poison event must not stall the whole
   read side, including ProcessManagers that drive work.
3. **It is retried alone, with backoff.** After 1s, doubling per consecutive
   failure of the same event up to 30s, a track of its own resumes it from its
   checkpoint, which delivers the event again. The exponent is capped, so the
   delay never overflows however long an event stays poison. A held
   ProcessManager runs no `process_pending()`, even when its retry track
   starts live: its to-do list is missing the event. A drain already inside
   `process_pending()` when the hold lands is cancelled. It is woken once it
   has applied the event. A retry still pending when the projection is
   rebuilt is dropped; the next plan replays it from 0.
4. **Visible.** `held_projections` names each held projection and the event,
   `is_healthy` is False while any is held, and each failure logs an `ERROR`
   plus a `WARNING` with the retry delay. A handler that keeps failing holds
   its read model at that event until it is fixed or rebuilt.

There is no skip or dead-letter policy. As with undecodable rows, any future
one must be opt-in, observable and auditable.

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
| Rust SDK (`sdk-rs`) | `tonic::Streaming` yields `Err(Status)` with `Code::Unavailable`, or `Code::DataLoss` for an undecodable event (position in trailing metadata `esp-undecodable-global-nonce`). |
| Rust event sourcing SDK (`event-sourcing-rust`) | `Error::EventStore(Status)` for `UNAVAILABLE`; `Error::DataLoss { global_nonce }` for `DATA_LOSS` with a position. `ProjectionRunner::run_supervised` retries the former with backoff and halts on the latter (see above). |
| TypeScript SDK (`sdk-ts`) | The async iterator rejects with the gRPC error (`code` 14 `UNAVAILABLE`, or 15 `DATA_LOSS`). Messages and a terminal error/end that arrive while the consumer is busy are buffered and delivered on later `next()` calls (`streamToAsyncIterator`), so a failure is never lost between reads. |
| Python (`event_sourcing` `GrpcEventStoreClient.subscribe`) | Raises `EventStoreError`; for `DATA_LOSS` with a position, the subclass `UndecodableEventError` (`.global_nonce`). `SubscriptionCoordinator` retries `UNAVAILABLE` with exponential backoff and resumes each projection from its saved checkpoint; on `UndecodableEventError` it halts with `SubscriptionHaltedError` instead (see above). A projection that fails an event is held below it and retried alone (see Projection handler failures). |

## Consequences

- Outages are visible to consumers and to logs (`subscription ... query failed`).
- Consumers that ignored stream errors now see them; they must reconnect.
- Fault-injection tests (`eventstore-backend-postgres/tests/it_subscribe_faults.rs`)
  cover replay and live failures, with and without a prefix filter, and
  reconnect from the saved checkpoint.
- An undecodable row halts every consumer that reaches it until an operator
  acts. That is intended: a visible stop beats a silently incomplete
  projection. The Python coordinator halts at the position with a typed
  `SubscriptionHaltedError` and one `ERROR` log, rather than retrying and
  logging every attempt (#360), until an operator repairs the row or moves
  checkpoints past it.
- `it_subscribe_undecodable.rs` stores a decoder-invalid row between valid rows
  (replay and live) and an all-invalid batch, and checks that nothing is
  delivered past it and reconnecting fails at the same position.
