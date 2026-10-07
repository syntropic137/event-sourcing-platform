# ADR-029: Hash-Chained Event Log

**Status:** Proposed
**Date:** 2026-10-07
**Deciders:** NeuralEmpowerment
**Related:** [ADR-026](ADR-026-subscription-failure-semantics.md), [ADR-027](ADR-027-cross-language-event-envelope.md), [ADR-028](ADR-028-append-idempotency-semantics.md), issues #308, #337, #366

## Context

The store is append-only by interface: no RPC mutates or deletes an event,
and Postgres row triggers (`forbid_events_mutation`) reject `UPDATE` and
`DELETE` on `events`. Nobody can check that property after the fact. A
database administrator, a restored backup, `TRUNCATE` (not covered by row
triggers), or a disabled trigger can change history, and a reader has no way
to notice.

DreamShip (#308) asks for a hash chain as a platform primitive: each event
commits to its predecessor, so given a trusted head any edit, reorder or
excision is detectable without trusting storage. Done per consumer, every
service would pick its own coverage, encoding and bugs.

Facts about the current store that constrain the design:

- **Ordering.** `events.global_nonce` is one `BIGSERIAL` shared by all
  tenants. A tenant's nonces are increasing but not contiguous. Postgres
  appends take `pg_advisory_xact_lock(APPEND_ORDER_LOCK_NAMESPACE,
  hashtext(tenant_id))` before drawing a nonce and hold it to commit (#337),
  so a tenant's events commit in `global_nonce` order and only one append
  per tenant is inside its write window at a time. Memory serializes all
  appends under its `streams` write lock.
- **Append path.** Under the lock, Postgres re-checks the idempotency key
  and stream head (#363), then writes events, the stream head
  (`aggregates`), the idempotency row and the NOTIFY in one statement
  (#370). `global_nonce` is drawn by the column default inside that
  statement. `recorded_time_unix_ms` is set once per batch in Rust.
- **What the server changes.** `normalize_event` fills empty `aggregate_id`,
  `aggregate_type`, `tenant_id` and `content_type` before anything is
  stored or fingerprinted. After that nothing is rewritten. Empty
  `content_schema`, `correlation_id`, `causation_id`, `actor_id` and
  `payload_sha256` are stored as `NULL` and read back as empty, so "absent"
  and "empty" are the same value. `headers` is stored as `JSONB` and read
  back as a map (key order not preserved). `payload_sha256` is a
  client-supplied field the server never checks.
- **Idempotency (ADR-028).** A keyed retry of a committed batch returns the
  original `AppendResponse`, built from the `idempotency` row; nothing is
  written. The fingerprint covers `EventData.meta` (server fields zeroed,
  headers sorted) and `payload`, nothing else.
- **Reads.** `ReadStream`, `ReadAll` and `Subscribe` return `EventData`.
  TypeScript and Python decode eagerly; the Rust SDK's `RecordedEvent`
  keeps most metadata but drops `content_schema` and `payload_sha256`.
- **Capabilities (#366).** `GetServerInfo` reports named guarantees; each
  backend opts in explicitly.
- **No deletion exists.** There is no tenant deletion, compaction or
  redaction feature today.

## Decision

### 1. Chain scope: one hash per event, linked into both its stream and its tenant

Options:

| Option | Detects | Verify one stream | Anchoring | Write cost |
|---|---|---|---|---|
| A. Per stream | Edits, reorders, excision inside a stream | Read the stream | One head per stream (millions) | Read stream head (already locked `FOR UPDATE`) |
| B. Per tenant, `global_nonce` order | Everything in the tenant log, including deleting or adding a whole stream, reorder across streams | Read the tenant range covering the stream | One head per tenant | Read tenant head under the tenant lock |
| C. Both, in one hash | Union of A and B | Read the stream | One head per tenant (stream heads are committed by it) | A plus B |

**Recommendation: C.** Each event's hash commits to two predecessors: the
previous event of its stream and the previous event of its tenant. One hash,
two links.

- The tenant chain is the integrity primitive: one head per tenant to
  anchor, and it catches whole-stream deletion and cross-stream reorder,
  which a per-stream chain cannot.
- The stream link makes the common check (load one aggregate, verify it)
  cost one stream read instead of a tenant range scan.
- No new contention. The tenant lock already serializes one tenant's
  appends; reading and advancing the tenant head inside it adds no wait.
  Per-tenant chaining needs that lock; a chain over all tenants would need
  a global lock (rejected: it serializes every tenant, see the
  many-tenant numbers in `POSTGRES-BASELINE.md`).
- `hashtext` collisions make two tenants share a lock. That only adds
  serialization; each tenant still has its own head.

### 2. What the hash covers and how it is encoded

The hash covers **every field of the stored event**, after normalization,
exactly as a reader receives it, plus both links. Nothing the server
rewrites is covered in its pre-rewrite form (normalization happens before
hashing, and nothing is rewritten afterwards).

| Field | Assigned by |
|---|---|
| `tenant_id`, `aggregate_id`, `aggregate_type` | client (server fills when empty) |
| `aggregate_nonce` | client (server checks) |
| `event_id`, `event_type`, `event_version`, `content_type` (server default when empty), `content_schema`, `correlation_id`, `causation_id`, `actor_id`, `timestamp_unix_ms`, `payload_sha256`, `headers` | client |
| `global_nonce`, `recorded_time_unix_ms` | server |
| SHA-256 of `payload` | derived |
| `prev_stream_hash`, `prev_tenant_hash` | server |

`global_nonce` is covered so positions are evidence too: renumbering events
(even order-preserving) breaks the chain, and checkpoints stay meaningful.
Covering it means the server must know the nonces before hashing; see
section 3.

**Canonical encoding, format v1.** Not protobuf: protobuf serialization is
not canonical across implementations (map order was #355/#362), and a new
proto field would silently change what is covered. An explicit field list
in a fixed order:

```
u64(n)   = 8 bytes, big-endian
str(s)   = u64(len(utf8 bytes)) || utf8 bytes      (no Unicode normalization)
bytes(b) = u64(len(b)) || b
h32(x)   = exactly 32 bytes

event_hash = SHA-256(
    bytes("esp/event-chain/v1")
    str(tenant_id)   str(aggregate_id)   str(aggregate_type)
    u64(aggregate_nonce)   u64(global_nonce)
    str(event_id)   str(event_type)   u64(event_version)
    str(content_type)   str(content_schema)
    str(correlation_id)   str(causation_id)   str(actor_id)
    u64(timestamp_unix_ms)   u64(recorded_time_unix_ms)
    bytes(payload_sha256)
    u64(header_count)   then per header, sorted by key bytes: str(key) str(value)
    h32(SHA-256(payload))
    h32(prev_stream_hash)   h32(prev_tenant_hash)
)
```

- Every variable-length value is length-prefixed and the field order is
  fixed, so the encoding is injective (the property fingerprint v2 added in
  ADR-028; same `u64` framing).
- Absent and empty strings encode identically (zero length), matching what
  Postgres reads back. Integers are encoded as the proto value (`u32`
  widened to `u64`); `event_version` is hashed as stored, `0` stays `0`
  (ADR-027's "0 means 1" is a reader rule, applied after verification).
- The payload enters as its digest, so a future redaction can drop the
  payload bytes and keep the chain verifiable (section 5).
- Genesis (no predecessor) is 32 zero bytes. It is valid only where
  section 5 says a chain may start.
- Precondition: the Postgres database encoding is `UTF8` (checked at
  startup), so text round-trips byte for byte.

**Algorithm: SHA-256.** Already a dependency (`sha2`, used by the
fingerprint), available in every target language's standard library and in
Postgres (`sha256()`, so operators can audit in SQL), FIPS-approved, and
hardware-accelerated on current x86 and ARM. BLAKE3 is faster on large
inputs, but events are small and an append's cost is the commit, not the
hash (section 7). Length extension is irrelevant: this is not a MAC.

**Versioning and domain separation.** The tag `esp/event-chain/v1` is part of
the hashed bytes, and every stored link records its format number. A new
field in `EventMetadata`, a new algorithm or a new layout is format v2 with
a new tag; v1 links stay verifiable forever. A verifier that does not know a
format reports the event as unverifiable, never as valid. Adding a field to
`EventMetadata` without a new chain format leaves that field uncovered; the
conformance suite fails if `EventMetadata` gains a field the v1 encoder does
not list as covered or explicitly excluded.

### 3. The server computes the link, inside the append transaction

Options: client computes, server computes.

**Recommendation: server**, at append, under the tenant lock. A client
cannot: it does not know `global_nonce`, `recorded_time_unix_ms` or the
tenant's previous event (other writers). A client-computed stream-only chain
would also be forgeable by the client and unenforceable by the store.

Postgres append, changed:

1. Unchanged up to and including taking the tenant lock and the #363
   re-checks.
2. The re-check that reads the stream head also reads the stream's head
   hash (`aggregates.head_hash`), the tenant head (`tenant_chain_heads`,
   `FOR UPDATE`, belt and braces with the advisory lock), and draws the
   batch's nonces (`SELECT nextval(...) FROM generate_series(1, n)`,
   sorted). Still one round trip; nonces are still drawn after the lock is
   taken, so #337 commit ordering is unchanged.
3. Rust computes each event's hash in batch order, chaining within the
   batch, with the one `recorded_ms` it already uses.
4. The single write statement (#370) inserts the events with explicit
   `global_nonce` and their link columns, and also advances
   `aggregates.head_hash` and upserts `tenant_chain_heads`. The existing
   defensive check (one row per event, increasing nonces) stays.

A rolled-back append leaves no trace in either head (same transaction).
Drawn-but-unused nonces leave gaps, as rollbacks already do.

Memory computes the same links under its existing write lock, keeping a
per-tenant and per-stream head hash in memory.

**Idempotent retries.** A keyed retry of a committed batch must return the
original hashes. The replay path already returns `last_aggregate_nonce`
from the `idempotency` row; it additionally reads `event_hash` of the event
at `(tenant_id, aggregate_id, last_committed_nonce)` (primary-key lookup,
only on replay). Events are immutable, so this is always the original
hash. For a batch committed before the chain existed it is empty.

The link lives in `EventData`, not `EventMetadata` (section 4), so the
ADR-028 fingerprint is unchanged: identical retries still match, and
fingerprints stored before this change stay valid. A client that sends a
link on append (for example a copy tool echoing read events) has it ignored
and replaced, like `global_nonce` and `recorded_time_unix_ms` today.

### 4. Storage and API

**Postgres (new migration):**

- `events`: `chain_format SMALLINT`, `event_hash BYTEA`,
  `prev_stream_hash BYTEA`, `prev_tenant_hash BYTEA`, all nullable (NULL for
  events written before the migration). Adding nullable columns without a
  default is a catalog-only change.
- `events`: `CHECK (event_hash IS NOT NULL AND chain_format IS NOT NULL ...)
  NOT VALID`. Enforced for new rows, existing rows not scanned. An older
  server binary still running after the migration therefore fails its
  appends loudly instead of writing an unchained event in the middle of a
  chain (see section 5, rollout).
- `aggregates.head_hash BYTEA` (stream head hash).
- `tenant_chain_heads (tenant_id PK, chain_format, last_global_nonce,
  head_hash, genesis_global_nonce)`. Needed because no index serves "latest
  event of a tenant" cheaply; it is a cache of the tenant's last event, so
  tampering with it produces a detectable break at the next append and it
  can be rebuilt from `events`.
- `chain_epoch (started_at_global_nonce)`: one row, the sequence value read
  by the migration while it holds `ACCESS EXCLUSIVE` on `events`. Every
  legacy row is at or below it; every row above it is chained.

Cost: 3 x 32 bytes plus 2 bytes per event, one small row update per append.

**Proto (`eventstore.v1`, additive):**

```proto
message EventChainLink {
  uint32 format           = 1;  // 1 = ADR-029 v1
  bytes  event_hash       = 2;  // 32 bytes
  bytes  prev_stream_hash = 3;  // 32 bytes, zero at a stream's chain start
  bytes  prev_tenant_hash = 4;  // 32 bytes, zero at a tenant's chain start
}
message EventData {
  EventMetadata  meta    = 1;
  bytes          payload = 2;
  EventChainLink chain   = 3;  // set on every read path; ignored on append
}
message AppendResponse {
  ...
  bytes last_event_hash  = 3;  // hash of the batch's last event = tenant head at commit
}
```

Present on `ReadStream`, `ReadAll` and `Subscribe`. Unchained (legacy)
events have no `chain`.

**No new RPC in v1.** The tenant head is the last event of `ReadAll`
backward with `max_count = 1` (or `AppendResponse.last_event_hash` for a
writer); the stream head is the last event of `ReadStream` backward. A
server-side "verify" RPC is rejected: the threat model distrusts the server,
so verification belongs to the reader. An operator scan for corruption is a
client-side CLI built on the SDK.

**Capability `hash_chained_log`:** every event appended at or after the
chain epoch carries a v1 link, links are present on all read paths, keyed
retries return the original hash, and the server refuses (rather than
skips) an append it cannot chain. Advertised by a backend only when all of
this holds; a client that requires it calls `require_capabilities`.

**SDKs (Rust first):**

- `eventstore_core::chain`: the one Rust implementation of the encoding,
  `event_hash`, and a streaming verifier, used by both backends and the
  Rust SDKs (as `fingerprint` is today). Published golden vectors (JSON:
  inputs, canonical bytes, hashes, including headers, empty fields, unicode,
  genesis and multi-event batches) are the cross-language contract, as the
  ADR-027 fixtures are.
- Verification always runs on raw `EventData`, before upcasting or decoding
  (the store never transforms payloads; upcasting is client-side).
  `RecordedEvent` gains `chain`, `content_schema` and `payload_sha256` so a
  projection can verify what it consumes.
- API: `verify_stream(aggregate_id, trusted_head)`,
  `verify_tenant_range(from, to, trusted_start, trusted_head)` returning the
  verified head or the first failing position and reason (bad hash, broken
  link, non-monotonic position, unexpected genesis, unknown format,
  unchained event inside the chained range).
- Continuity for consumers: a projection may store the `event_hash` of its
  checkpoint event next to the checkpoint. On resume it re-reads that event
  and compares. A mismatch means the log under it changed (most commonly a
  restore of an older backup, see section 5) and the consumer must rebuild
  instead of resuming. A full-tenant subscriber can also check every
  `prev_tenant_hash` against the previous event as it goes.
- Anchoring: the SDK exposes a head as `(tenant_id, global_nonce,
  event_hash)`. Storing it outside the store's trust domain (another
  database, object storage with retention lock, a transparency log, a
  signed record) is the application's job; the ADR does not pick a medium.
- TypeScript and Python ports follow, tested against the golden vectors.

### 5. Migration, parity, backup/restore, legitimate deletion

**Existing events: genesis at the upgrade point, no backfill.**

- Backfill would `UPDATE` every historical row: the append-only trigger
  must be disabled, the largest table rewritten, and the chain would start
  by asserting that the operator who disabled the guard did not touch
  anything. It also takes the lock-ordered append path out of the picture
  for history. Rejected.
- Instead, each tenant's chain starts at its first event above the chain
  epoch, with `prev_tenant_hash` zero; each stream's chain starts at its
  first chained event, with `prev_stream_hash` zero. Legacy events are
  readable, unchained, and reported as such.
- A verifier given the epoch (anchor it once at upgrade) enforces: a zero
  `prev_tenant_hash` only at the tenant's first event above the epoch; a
  zero `prev_stream_hash` only at `aggregate_nonce = 1` or where the
  stream's previous event is at or below the epoch; no unchained event above
  the epoch.
- Optional, later: an admin command computes a **legacy seal** per tenant
  (a v1-style chain over the legacy events, computed offline, not stored in
  the rows) for the operator to anchor. It makes later edits to legacy
  history detectable without rewriting it. Not needed for v1.

**Rolling upgrade.** The migration's `NOT VALID` check makes appends by
older binaries fail after the migration. Upgrade with a brief write drain
(stop old binaries, migrate, start new), as ADR-028 already asks to finish a
rollout before relying on cross-version behavior. Silent gaps are worse
than a visible pause.

**Memory backend parity.** Same links, same encoding, same conformance
cases; no legacy events (memory starts empty), so its epoch is 0.
`recorded_time_unix_ms` differs per event in memory and per batch in
Postgres; both are hashed as stored, so both verify.

**Backup and restore.** A whole-database `pg_dump` (the runbook's
requirement) carries the link columns, `tenant_chain_heads` and
`chain_epoch`; a dump missing `tenant_chain_heads` makes the next append
chain to the wrong predecessor, which verification then reports. Runbook
and drill additions: verify each tenant's chain after restore and compare
its head with the last anchored head. Restoring an **older** backup rewinds
the log: the restored chain verifies on its own, an anchored head newer
than the restore point is no longer reachable (reported as rollback), and
new appends fork from the restored head. That is correct evidence, and it
gives the "consumers after restoring an older backup" procedure a direct
signal: a consumer whose stored checkpoint hash does not match the event at
that position must rebuild.

**Legitimate deletion and compaction.** None exists today. Defined answers
so a future feature does not have to break the chain:

| Operation | Verifier result |
|---|---|
| Projection rebuild | Not affected: projections are not chained. |
| Payload redaction (erasure, crypto-shredding) | Keep the row and its link, drop the payload bytes, store `SHA-256(payload)`. The chain verifies; the verifier reports "verified, payload redacted" for those events. Needs its own ADR. |
| Stream or prefix compaction | Must not delete rows: a deleted event is a hole in its tenant chain. Compact by redacting to stubs (metadata needed for hashing plus links stay). |
| Tenant deletion | The whole chain is gone; there is nothing left to verify. A verifier holding an anchor reports "tenant absent". The chain cannot tell a legitimate deletion from an attack; deletion must leave an attested record outside the tenant (operator audit log). |

### 6. Not idempotency

The chain and idempotency want opposite things and stay separate:

- An idempotency key identifies **a command attempt**, so a retry must be
  able to reproduce it before knowing the outcome. ESP's key is scoped to
  `(tenant_id, aggregate_id, key)` and paired with a content fingerprint
  (ADR-028). The intended key is a command id minted once where the command
  is triggered; the Rust repository's key is derived from the batch's
  expected revision, size and its event ids, which are minted once per
  recorded batch.
- An event hash identifies **a position in history** and depends on
  server-assigned values. A client cannot know it before the append, so it
  can never be an idempotency key, and the fingerprint must never include
  it (it does not: the link is outside `EventMetadata`).
- DreamShip's key is a pure function of the payload, and its own
  `stream_head` treats a key match anywhere in a stream as committed, so a
  legitimate X -> Y -> X revision is swallowed. That is a downstream codec
  defect, fixed by minting a command id at the trigger. Within ESP, reusing a
  key for a new batch is not silently accepted: the fingerprint covers each
  event's `aggregate_nonce` and `event_id`, so the third write of X under the
  first X's key is `ALREADY_EXISTS`. ESP changes nothing here.

### 7. Threat model and performance

**Detected**, for a range verified against a trusted head (anchored at or
after the end of the range) or a trusted start:

- modification of any covered field or payload byte of a chained event;
- reorder, insertion or excision of chained events, within a stream or
  across streams of a tenant;
- splicing another tenant's events in (tenant id is covered);
- truncation or rollback of the tail, including restoring an older backup,
  when the trusted head is newer than the truncation point;
- deletion of a chain's prefix (the new first event's predecessor link is
  not zero and the epoch rules reject it).

**Not detected:**

- An attacker who can write the database and recompute the whole suffix and
  head. The hash is unkeyed and the algorithm is public by design, so
  without an external anchor taken **before** the tampering this is
  undetectable. Anchoring is the defense; signed heads (a server key) would
  add protection against a database-only attacker but not against a
  compromised server, and are a possible later addition.
- A compromised server at append time: it chains whatever it chooses.
  Writers can read back and compare their own events.
- Withholding: a server can serve a consistent but stale prefix. Only a
  reader holding a newer anchor notices.
- Anything outside `events`: stream heads, idempotency records, checkpoints,
  projections. Legacy (pre-epoch) events, unless sealed.
- Confidentiality. The chain hides nothing and encrypts nothing. Publishing
  a head discloses one digest over inputs that include a random `event_id`,
  so it does not enable guessing low-entropy payloads. (The separate hazard
  noted in #308, deriving identifiers from short user text, is unrelated to
  hashing for integrity.)

**Performance budget** (against `docs/performance/POSTGRES-BASELINE.md`,
measured with `make bench-pg-full` before and after):

| Path | Baseline | Budget |
|---|---|---|
| Append, 1 tenant, batch 1, closed-loop ceiling | ~900 ev/s | at most 5% lower |
| Append, 1 tenant, batch 1, p99 (1 writer) | 4 ms | at most +1 ms |
| Append, batch 100 | ~3,400 ev/s | at most 5% lower |
| Append, many tenants | ~1,290 ev/s | at most 5% lower |
| `ReadAll` replay | 70k to 145k ev/s | at most 10% lower (rows grow by ~100 bytes) |

Why it fits: SHA-256 over a few hundred bytes is about a microsecond, far
below the commit cost that bounds the single-tenant ceiling. No round trip
is added under the lock (nonce draw and head reads join the existing
re-check). The added writes are 98 bytes per event and one small row per
append. Client-side verification runs at hundreds of thousands of events
per second per core, faster than replay delivers them.

## Alternatives considered

- **Per-consumer chains in the envelope (status quo downstream).** Each
  consumer picks coverage and encoding; none can cover server-assigned
  positions. Rejected (#308).
- **Merkle tree / transparency log (RFC 6962 style).** Logarithmic
  inclusion and consistency proofs, much more machinery. A chain is the
  base it would sit on; Merkle checkpoints over chain heads can be added
  later without changing v1.
- **Periodic checkpoint hashes only (no per-event link).** Cheaper per
  append, but detection is per range and verification still reads the whole
  range. Per-event links cost about the same and localize the failure.
- **Database-level controls (pgaudit, WAL archiving, ledger databases).**
  Operational, Postgres-only, and give a reader nothing to check. Rejected
  as the primitive; still useful as defense in depth.
- **Protobuf bytes as the canonical form.** Not canonical across
  implementations; rejected (section 2).

## Rollout (small PRs, Rust first)

1. This ADR.
2. `eventstore_core::chain`: encoding, hash, verifier, golden vectors, the
   `EventMetadata` coverage guard test. No behavior change.
3. Proto: `EventChainLink`, `EventData.chain`,
   `AppendResponse.last_event_hash`, capability constant (not advertised).
   Regenerate TS and Python stubs; no SDK behavior change.
4. Memory backend links, replay returns the original hash, conformance
   cases in `eventstore_core::conformance` (stream and tenant continuity,
   tenants independent, keyed retry returns the original hash, client link
   ignored, rollback leaves heads untouched). Advertise
   `hash_chained_log` on memory.
5. Postgres: migration (columns, `NOT VALID` check, heads, epoch), append
   path, replay hash lookup, conformance plus forced-race tests across the
   lock, bench before/after against the budget. Advertise
   `hash_chained_log` on Postgres.
6. Rust SDK: `RecordedEvent` fields, `verify_stream`,
   `verify_tenant_range`, checkpoint-hash continuity check in the
   projection runner (opt-in), docs.
7. Operations: `BACKUP-RESTORE.md` and drill (chain verifies after restore;
   older-restore rollback and consumer fork detected), upgrade procedure.
8. TypeScript and Python verifiers against the golden vectors.

## Consequences

- A reader can check history instead of trusting storage, given an anchor
  it trusts. Without anchoring, the chain catches accidents and partial
  tampering but not a determined database administrator.
- `EventMetadata` changes now require a chain-format decision; the guard
  test enforces it.
- Upgrading a Postgres store needs a short write drain.
- Every event costs about 100 bytes more; appends do slightly more work
  under the tenant lock, within the stated budget.
- Future deletion features must redact to stubs rather than delete rows, or
  accept that they end verifiability.
