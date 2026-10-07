# ADR-028: Append Idempotency Semantics

**Status:** Accepted
**Date:** 2026-10-06
**Related:** [ADR-021](ADR-021-expected-version-and-set-based-validation.md), issues #355, #362, #363

## Context

`AppendRequest.idempotency_key` makes a whole batch safe to retry when its
outcome is unknown (timeout, lost acknowledgment). The backends disagreed on
what a retry returns:

- **Order of checks.** Postgres checked the key first, then the
  `expected_aggregate_nonce` precondition. Memory checked the precondition
  first. A retry after a lost ack, sent after another writer advanced the
  stream, got the original ack on Postgres and `ABORTED` on memory.
- **Fingerprint.** Memory compared batches by their protobuf encoding.
  `headers` is a map and prost encodes it in hash-map iteration order, which
  differs between decodes, so an identical retry with two or more headers
  could be judged a different payload (#362). Postgres was fixed in #365.
- **In-flight retries (Postgres).** Two identical keyed requests in flight
  at once (a client retry overtaking its original): the loser got `ABORTED`,
  or `INTERNAL` from the nonce trigger when the stream was new, instead of
  the original ack.

The Rust repository's reconciliation (read the stream, compare event IDs)
hid all of this from Rust SDK users, but not from other clients.

## Decision

One contract, implemented by every backend and enforced by a shared
conformance suite.

### Scope and identity

A key is scoped to `(tenant_id, aggregate_id, idempotency_key)`. The same key
on another aggregate is a different request.

The request identity is `eventstore_core::fingerprint::batch_fingerprint`:
SHA-256 over each normalized event's canonical metadata (server-assigned
`recorded_time_unix_ms` and `global_nonce` zeroed, `headers` in key order)
followed by its payload. Every backend uses this one function.
`expected_aggregate_nonce` is not part of the fingerprint; the events'
`aggregate_nonce` values already pin the batch's position.

### Precedence

For an append with a non-empty key:

1. **Key committed, same fingerprint**: return the original
   `AppendResponse` (same `last_aggregate_nonce` and `last_global_nonce`).
   Nothing is written. This holds even if the stream has advanced since and
   the request's `expected_aggregate_nonce` is now stale.
2. **Key committed, different fingerprint**: `ALREADY_EXISTS`. Nothing is
   written. This also wins over a stale expected revision.
3. **Key unused**: the normal optimistic concurrency check. A stale
   `expected_aggregate_nonce` is `ABORTED` with `ConcurrencyErrorDetail`, and
   the key stays unused (a failed append claims nothing).

With an empty key, only step 3 applies.

### Concurrent identical requests

If identical keyed requests are in flight at once, exactly one commits and
every one of them returns its ack. Unkeyed writers racing for the same
revision: one wins, the others get `ABORTED`.

- **Memory**: appends are serialized by the streams write lock; the key is
  checked and recorded under it, before it is released.
- **Postgres**: an append re-checks after taking the per-tenant
  append-order advisory lock (which every writing append holds until commit):
  the key first, then the stream head. It also re-checks the key when the
  precondition fails, because it may have waited on the stream row lock for
  a twin that has since committed. Re-checks are plain reads and never wait.

### Compatibility

`batch_fingerprint` is byte-identical to what Postgres has stored for every
request with zero or one header per event (and for every request since
#365), so existing idempotency rows stay valid. Requests stored before #365
with two or more headers may not match on retry; such a retry gets
`ALREADY_EXISTS`, as before #365 (the Rust repository reconciles it). Memory
keeps no state across restarts, so its fingerprint change has no
compatibility impact.

### Conformance suite

`eventstore_core::conformance` (feature `conformance`) holds the cases; a
backend runs them all with `eventstore_core::append_conformance_tests!(factory)`
from an integration test. Memory and Postgres run it in CI (Postgres via
`TEST_DATABASE_URL` or a testcontainer). A new backend must run it too.

## Consequences

- A client can retry a keyed append with the same request until it gets an
  answer, and the answer does not depend on the backend or on what other
  writers did meanwhile.
- Retries must resend the exact batch (event IDs, metadata, headers,
  payload). Changing anything and reusing the key is `ALREADY_EXISTS`.
- Postgres appends do one or two extra indexed reads under the advisory lock.
- Reconciliation in the Rust repository stays as a fallback (pre-#365 rows,
  batches that grew between attempts) but is no longer needed for a plain
  lost-ack retry.
