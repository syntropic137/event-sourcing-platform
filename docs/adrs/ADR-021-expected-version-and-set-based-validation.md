# ADR-021: ExpectedVersion Semantics & Stream-Per-Unique-Value Pattern

| Field   | Value |
|---------|-------|
| Status  | Accepted |
| Date    | 2026-04-06 |
| Drivers | syntropic137/syntropic137#594 — repo registration dedup |

## Context

Event sourcing systems need set-based validation — enforcing uniqueness constraints (e.g., "only one repo with this `full_name` per org") when the source of truth is an append-only event stream rather than a relational table with unique indexes.

Our Rust event store already supports the key primitive: when `expected_aggregate_nonce = 0`, the store rejects the append if the stream already contains events. This is the **NoStream** semantics familiar from EventStoreDB and Axon Framework. However, the Python SDK did not expose this capability explicitly, and a bug in the gRPC error code mapping (`FAILED_PRECONDITION` instead of `ABORTED`) prevented concurrency errors from being properly caught.

## Decision

### 1. Expose `ExpectedVersion` in the Python SDK

A lightweight class with semantic constants:

- `ExpectedVersion.NO_STREAM` (= `0`) — stream must not exist
- `ExpectedVersion.ANY` (= `None`) — skip version check
- `ExpectedVersion.exact(n)` — stream must be at version `n`

These map directly to `expected_aggregate_nonce` in the gRPC protocol.

### 2. Add `StreamAlreadyExistsError`

A subclass of `ConcurrencyConflictError` raised specifically when `expected_version == 0` (NoStream) and the stream already exists. This lets callers distinguish "duplicate creation" from "stale read" conflicts:

```python
try:
    await repository.save_new(claim_aggregate)
except StreamAlreadyExistsError:
    # Uniqueness violation — duplicate detected
    pass
```

### 3. Add `save_new()` to `EventStoreRepository`

A convenience method that uses `ExpectedVersion.NO_STREAM` explicitly, making the intent clear for the stream-per-unique-value pattern.

### 4. Fix gRPC error code mapping

The Rust event store sends `Code::Aborted` for concurrency conflicts. The Python SDK was catching `FAILED_PRECONDITION` — this is corrected to `ABORTED`, and the `ConcurrencyErrorDetail` protobuf is now decoded to extract the actual stream version.

### 5. Stream-Per-Unique-Value Pattern

For set-based validation, the recommended pattern is:

1. Derive a deterministic aggregate ID from the unique value (e.g., `hash(org_id, repo_full_name)`)
2. Create a lightweight "claim" aggregate whose stream ID encodes the uniqueness constraint
3. Use `save_new()` / `ExpectedVersion.NO_STREAM` — the event store enforces uniqueness atomically
4. Catch `StreamAlreadyExistsError` for duplicate detection

This avoids:
- Projection-based uniqueness checks (eventually consistent, race-prone)
- Database-level unique constraints (couples the event store to a specific backend)
- Distributed locks (complex, fragile)

## Consequences

### Positive

- **Zero new infrastructure** — leverages existing event store OCC semantics
- **Atomic uniqueness** — no race conditions between check and write
- **Backend-agnostic** — works with both Postgres and in-memory backends
- **Clear API** — `ExpectedVersion` documents intent; `StreamAlreadyExistsError` enables precise error handling

### Negative

- Uniqueness is per-stream — cross-stream uniqueness still requires a projection or external check
- The "claim" aggregate adds a stream per unique value (acceptable overhead for event stores)

### Risks

- `ExpectedVersion.ANY` currently maps to `None`, which the gRPC client defaults to `expected_aggregate_nonce = 0`. This means `ANY` has NoStream behavior on the wire. A future protocol change may be needed to support true "any version" semantics (e.g., a sentinel value like `-1`).

## References

- Greg Young, "Set-Based Consistency Validation" (2010)
- Oskar Dudycz, "How to ensure uniqueness in Event Sourcing" (2021)
- AxonIQ Reference Guide: "Set Based Consistency Validation"
- EventStoreDB: `ExpectedRevision.NoStream`
- Barry O'Sullivan, "Set-Based Consistency with Event Sourcing" (2019)
- Derek Comartin, "Unique Constraints in Event Sourcing" (2022)
- Martin Dilger, *Understanding Event Sourcing* — Ch. 37: Processor To-Do List pattern

## First Consumer

Syntropic137 repo registration dedup (issue #594, G4): `RepoClaimAggregate` with deterministic ID derived from `(org_id, full_name)`.
