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
  hashtext(tenant_id))` after the stream row lock (`aggregates ... FOR
  UPDATE`), before drawing a nonce, and hold it to commit (#337). A tenant's
  events therefore commit in `global_nonce` order and only one append per
  tenant is inside its write window at a time. Memory serializes all
  appends under its `streams` write lock.
- **Append path.** Under the lock, Postgres re-checks the idempotency key
  and stream head (#363), then writes events, the stream head
  (`aggregates`), the idempotency row and the NOTIFY in one statement
  (#370). `global_nonce` is drawn by the column default inside that
  statement. `recorded_time_unix_ms` is set once per batch in Rust.
- **Indexes.** `events` has `(global_nonce)`, `(tenant_id, aggregate_id)`
  and `(tenant_id, recorded_time_unix_ms)`, plus the primary key
  `(tenant_id, aggregate_id, aggregate_nonce)`. Nothing serves "latest
  event of a tenant", nor the tenant-filtered `global_nonce` scans of
  `ReadAll` and `Subscribe`.
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
- **Reads.** `ReadStream`, `ReadAll` and `Subscribe` return raw `EventData`.
  TypeScript and Python decode eagerly. The Rust event-sourcing SDK
  (`event-sourcing/rust`) converts to `RecordedEvent`, which drops
  `content_schema` and `payload_sha256`, reports `event_version` 0 as 1,
  and may be upcast. Backward reads need an explicit upper bound:
  `ReadStream` backward from 0 reads position 1, `ReadAll` backward from
  `u64::MAX` reads nothing (the value wraps to -1 in SQL).
- **Capabilities (#366).** `GetServerInfo` reports named guarantees; each
  backend opts in explicitly.
- **No deletion exists.** There is no tenant deletion, compaction or
  redaction feature today.

## Decision

### 1. Chain scope: one hash per event, linked into both its stream and its tenant

Options:

| Option | Detects (against a trusted head) | Verify one aggregate | Anchoring | Write cost |
|---|---|---|---|---|
| A. Per stream | Edits, reorders, excision inside that stream | Read the stream, given a trusted stream head | One head per stream (millions) | Stream head is already read under its row lock |
| B. Per tenant, `global_nonce` order | Everything in the tenant log, including a whole stream deleted or added, reorder across streams | Read the tenant range from the stream's first event to a trusted tenant head | One head per tenant | Read the tenant's last event under the tenant lock |
| C. Both, in one hash | A or B, depending on which head is trusted | As A, or as B | As A or B | A plus B, 32 more bytes per event |

**Recommendation: C.** Each event's hash commits to two predecessors: the
previous event of its stream and the previous event of its tenant. One hash,
two links.

- The tenant chain is the integrity primitive: one head per tenant to
  anchor, and it catches whole-stream deletion and cross-stream reorder,
  which a per-stream chain cannot.
- The stream link is a cheaper check when the reader already holds a
  trusted **stream** head: a writer holds one for every stream it appended
  to (`AppendResponse.last_event_hash` is the hash of the stream's newest
  event), and a loader holds one from its previous verified load. Reading
  only the stream does **not** authenticate it against a tenant anchor: the
  `prev_tenant_hash` values on a stream's events are opaque without the
  events between them. Proving a stream against a tenant anchor means
  walking the tenant chain from the stream's first event to the anchor
  (Merkle inclusion proofs would shorten that; out of scope, see
  Alternatives).
- Tenant-only (B) is the simpler fallback if stream heads turn out not to
  be held by applications in practice; C costs 32 bytes per event and one
  extra read on a row the append already locks.
- Contention topology is unchanged: the tenant lock already serializes one
  tenant's appends. The serialized window does get longer (hashing, one
  head read, wider rows); section 7 bounds it. A chain over all tenants
  would need a global lock (rejected: it serializes every tenant).
- `hashtext` collisions make two tenants share a lock. That only adds
  serialization; each tenant still has its own chain.

### 2. What the hash covers and how it is encoded

The hash covers **every field of the event as a reader decodes it**, plus
both links. It is computed after normalization and after the server assigns
`global_nonce` and `recorded_time_unix_ms`; nothing changes after hashing.
It authenticates these logical values, not their physical storage form
(`NULL` vs empty, `JSONB` layout).

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
  fixed, so the byte string is an unambiguous encoding of the metadata, the
  payload digest and the links (the framing fingerprint v2 introduced in
  ADR-028). Because the payload enters as a digest, binding to the payload
  rests on SHA-256 collision resistance, not on injectivity.
- Absent and empty strings encode identically (zero length), matching what
  Postgres reads back. Integers are encoded as the proto value (`u32`
  widened to `u64`); `event_version` is hashed as stored, `0` stays `0`
  (ADR-027's "0 means 1" is a reader rule, applied after verification).
- The payload enters as its digest so a future redaction can drop the
  payload bytes and keep the chain verifiable (section 5). That digest is
  computed by the server; the client-supplied `payload_sha256` is just
  another covered metadata field and is never trusted as a substitute.
- Genesis (no predecessor) is 32 zero bytes (section 5).
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
`EventMetadata` without a new chain format leaves that field uncovered; a
test fails if `EventMetadata` gains a field the v1 encoder does not list as
covered or explicitly excluded.

### 3. The server computes the link, inside the append transaction

Options: client computes, server computes.

**Recommendation: server**, at append, under the tenant lock. A client
cannot: it does not know `global_nonce`, `recorded_time_unix_ms` or the
tenant's previous event (other writers). A client-computed stream-only chain
would also be forgeable by the client and unenforceable by the store.

Postgres append, changed:

1. Before the transaction: compute `SHA-256(payload)` per event (the only
   per-byte work), so payload size does not lengthen the locked window.
2. Unchanged: stream row lock, tenant lock, #363 re-checks. Lock order
   (stream row, then tenant) is preserved.
3. The stream-head re-check query (today one of the one or two plain
   reads after the lock; the key re-check stays separate) also reads the
   hash of the stream's last event
   (primary key `(tenant_id, aggregate_id, last_nonce)`), the tenant's last
   event (`global_nonce`, `event_hash`) via a new index
   `(tenant_id, global_nonce)`, and draws the batch's nonces (`SELECT
   nextval(...) FROM generate_series(1, n)`, sorted), in the same statement:
   no added round trip, which matters because the locked window is round
   trip bound (`POSTGRES-BASELINE.md`, finding 2). Nonces are still drawn
   after the lock is taken, so #337 commit ordering is unchanged. Heads are
   read from the event rows themselves, not from a cache table that could
   drift from them.
4. **Fail closed, narrowly.** If the tenant's or stream's last event is at
   or above the chain epoch (section 5) and has no link, or a link of an
   unknown format, the append is refused (`FAILED_PRECONDITION`, logged
   with the position). This is a structural check, not verification: the
   server does not re-verify history on append. If rows were deleted, the
   server chains onto the surviving predecessor, and if a tenant's chained
   events were all deleted, it starts a new genesis. Both are forks that
   only an anchor detects (section 7), and the restore procedure (section
   5) verifies against anchors before writes resume.
5. Rust computes each event's hash in batch order, chaining within the
   batch, with the one `recorded_ms` it already uses.
6. The single write statement (#370) inserts the events with explicit
   `global_nonce` and link columns, and stores the batch's last
   `event_hash` in the idempotency row. The existing defensive check (one
   row per event, increasing nonces) stays.

A rolled-back append leaves no trace (heads are rows of the same
transaction). Drawn-but-unused nonces leave gaps, as rollbacks already do.

Memory computes the same links under its existing write lock from the last
event of the stream and of the tenant.

**Idempotent retries.** A keyed retry of a committed batch must return the
original hash. The `idempotency` row gains `last_event_hash`, written in the
same statement as the events, so the replay path returns it like
`last_global_nonce`. Rows written before the migration have none: if their
`last_global_nonce` is below the epoch the batch was never chained and the
hash is empty; otherwise (impossible unless the row was tampered with) the
replay fails with `DATA_LOSS` instead of returning an empty hash.

A retry receipt comes from the `idempotency` table, which is outside the
chain and returned before any head check, so it is a claim, not evidence.
It describes a historical head; an SDK never replaces a newer anchor with
it, and verifies it (read the event at that position, check it connects
to a trusted head) before using it as an anchor. After restoring an older
backup, lost idempotency rows still let earlier commands run again (an
existing runbook limit); the chain exposes the divergence, it does not
restore exactly-once execution.

The link lives in `EventData`, not `EventMetadata` (section 4), so the
ADR-028 fingerprint is unchanged: identical retries still match, and
fingerprints stored before this change stay valid. A client that sends a
link on append (for example a copy tool echoing read events) has it ignored
and replaced, like `global_nonce` and `recorded_time_unix_ms` today.

### 4. Storage and API

**Postgres (new migrations):**

- `events`: `chain_format SMALLINT`, `event_hash BYTEA`,
  `prev_stream_hash BYTEA`, `prev_tenant_hash BYTEA`, all nullable (NULL
  for events written before the migration). Catalog-only change.
- `events`: `CHECK (chain_format IS NOT NULL AND event_hash IS NOT NULL
  AND ...) NOT VALID`. Enforced for new rows; existing rows not scanned. An
  older server binary still running after the migration fails its appends
  loudly instead of writing an unchained event into a chain (section 5).
- `idempotency.last_event_hash BYTEA`.
- Index `(tenant_id, global_nonce)`, built `CONCURRENTLY` in its own
  non-transactional migration (sqlx `-- no-transaction`) so a large table
  is not write-locked while it builds. It also serves the tenant-filtered
  `global_nonce` scans of `ReadAll` and `Subscribe`.
- `chain_epoch (first_chained_global_nonce)`: one row, written by the
  migration while it holds `ACCESS EXCLUSIVE` on `events`, from the sequence
  state: `last_value + 1` if `is_called`, else `last_value` (an unused
  sequence's first value is `last_value` itself). Every row below it is
  legacy, every row at or above it is chained. On an empty database the
  epoch is 1.

Cost: 98 bytes of link data per event plus varlena headers, one more index
entry per event, and the matching WAL; measured in section 7.

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
  bytes last_event_hash  = 3;  // hash of the batch's last event (head of the
                               // stream, and of the tenant, at commit)
}
```

Present on `ReadStream`, `ReadAll` and `Subscribe`. Unchained (legacy)
events have no `chain`.

**No new RPC in v1.** A head is the last event of a backward read with an
explicit upper bound (`ReadAll` from `i64::MAX`, `ReadStream` from the
stream's `last_aggregate_nonce`), which the SDK helper hides; the same PR
clamps `ReadAll`'s `from_global_nonce` to `i64::MAX` so `u64::MAX` means
"from the end". A writer gets heads from `AppendResponse.last_event_hash`.
A server-side "verify" RPC is rejected: the threat model distrusts the
server, so verification belongs to the reader. An operator scan for
corruption is a client-side CLI built on the SDK.

**Capability `hash_chained_log`:** every event at or above the chain epoch
carries a v1 link, links are present on all read paths, keyed retries
return the original hash, and the server refuses an append whose stream or
tenant predecessor (at or above the epoch) has no recognized link. It does
not promise that the server checked history before appending. Advertised by a backend only when all of this
holds; a client that requires it calls `require_capabilities`.

**SDKs (Rust first):**

- `eventstore_core::chain`: the one Rust implementation of the encoding,
  `event_hash`, and a streaming verifier, used by both backends and the
  Rust SDKs (as `fingerprint` is today). Published golden vectors (JSON:
  inputs, canonical bytes, hashes, including headers, empty fields,
  unicode, genesis and multi-event batches) are the cross-language
  contract, as the ADR-027 fixtures are.
- Verification runs on raw `EventData` as received, before any conversion,
  version normalization, upcasting or decoding. In the Rust SDK that is in
  the low-level client and the projection runner, ahead of building
  `RecordedEvent`; `RecordedEvent` then carries the verified `chain` link.
  Re-verifying from a `RecordedEvent` is not supported (it is not the
  stored form).
- Two result levels, never conflated:
  - **consistent**: every event recomputes to its hash and links to the
    previous one, from a lower bound to the last event read;
  - **authenticated**: consistent, and the walk reaches a trusted head at
    or after the last event of interest.
- Completeness of a range comes from the links, not the nonces (a tenant's
  nonces have gaps). A range is complete only if the walk's lower end
  reaches either the genesis (zero predecessor) or a hash the caller
  already trusts (a previously authenticated head) at or before the
  requested start. A suffix that merely ends at the trusted head, with
  earlier events withheld, is reported as incomplete.
- API: `verify_stream(aggregate_id, trusted_stream_head)` and
  `verify_tenant_range(from, trusted_lower, trusted_head)`, each returning
  the level reached, the verified head, or the first failing position and
  reason (bad hash, broken link, non-monotonic position, unexpected
  genesis, unknown format, unchained event inside the chained range, not
  connected to the trusted head, lower bound not proven).
- Live consumers: a subscriber can only reach **consistent** until a later
  anchor covers what it consumed; authentication of live events is
  deferred to the next anchor (the SDK records the last consistent head and
  re-checks when an anchor arrives). Tenant continuity needs every tenant
  event, so it is checked only on an unfiltered subscription;
  `aggregate_id_prefix` subscriptions (the Rust projection runner's
  default) skip tenant predecessors and need a separate unfiltered
  verification feed or periodic `verify_tenant_range`.
- Consumers and restores: the runbook's **applied high-water mark** rule
  (BACKUP-RESTORE.md) stays the decision rule. The chain makes it
  checkable: a consumer stores the `event_hash` of its applied high-water
  event atomically with its state. On resume it re-reads that position; if
  the event is absent or its hash differs, the log under it changed (for
  example an older backup was restored) and it rebuilds. This is a
  rollback check only: a matching stored hash says nothing about edits to
  earlier events, which need `verify_tenant_range` against the saved head.
- Anchoring: the SDK exposes a head as `(tenant_id, global_nonce,
  event_hash)`. Storing it outside the store's trust domain (another
  database, object storage with retention lock, a transparency log, a
  signed record) is the application's job; the ADR does not pick a medium.
- TypeScript and Python ports follow, tested against the golden vectors.

### 5. Migration, parity, backup/restore, legitimate deletion

**Existing events: genesis at the upgrade point, no backfill.**

- Backfill would `UPDATE` every historical row: the append-only trigger
  must be disabled and the largest table rewritten, and the chain would
  start by trusting the operator who disabled the guard. Rejected.
- Instead, each tenant's chain starts at its first event at or above the
  epoch, with `prev_tenant_hash` zero; each stream's chain starts at its
  first chained event, with `prev_stream_hash` zero (at `aggregate_nonce =
  1`, or after a legacy prefix).
- A verifier walking back from a trusted head is bound all the way to the
  genesis by the hashes; it needs no epoch for that. It reports the
  unchained events before the genesis as legacy and uncovered. The epoch
  only matters for those: whether an unchained row is legacy or was
  inserted later cannot be decided from the chain. Operators anchor the
  epoch at upgrade.
- Optional, later: an admin command computes a **legacy seal** per tenant
  (a v1-style chain over the legacy events, computed offline, not stored in
  the rows) for the operator to anchor. Later edits to legacy history then
  become detectable without rewriting it. Not needed for v1.

**Upgrade contract.** Migrations run automatically when a server starts
(`connect_with_config`), so the first new instance migrates while old ones
may still be writing; the `NOT VALID` check then fails their appends.
Procedure:

1. Build the `(tenant_id, global_nonce)` index first (its own migration,
   `CONCURRENTLY`, writes continue). A failed concurrent build leaves an
   `INVALID` index that `IF NOT EXISTS` would skip; the migration checks
   `pg_index.indisvalid` and drops and rebuilds it.
2. Drain writes: stop every old instance.
3. Start the new version; it applies the chain migration and records the
   epoch. The operator records the epoch and each tenant's first head out
   of band (that is the epoch's only authentication).
4. Resume writes.

Silent gaps are worse than a short pause; ADR-028 already asks to finish a
rollout before relying on cross-version behavior.

**Legacy coverage.** Streams with pre-epoch events are only partly covered;
replaying such an aggregate uses unverified history, and the verifier says
so ("chained from position k"). Applications that need full coverage seal
legacy history (above) or rebuild into new streams.

**Memory backend parity.** Same links, same encoding, same conformance
cases; memory starts empty, so its epoch is 1 and it has no legacy events.
`recorded_time_unix_ms` differs per event in memory and per batch in
Postgres; both are hashed as stored, so both verify.

**Backup and restore.** A whole-database `pg_dump` (the runbook's
requirement) carries the link columns, `idempotency.last_event_hash` and
`chain_epoch`. Heads are the event rows, so a restore cannot leave a stale
head behind; a restore that lost link columns makes the server refuse
appends (fail closed) rather than chain onto them. Runbook and drill
additions, before writes resume: verify each tenant's chain to its restored
head, and check every anchor the operator holds for **reachability** (is
the anchored hash an ancestor of, or equal to, the restored head?), not
equality with the head. Restoring an **older** backup rewinds the log: the
restored chain verifies on its own; an anchor taken after the backup point
is unreachable, which is the rollback signal; new appends then fork from
the restored head. An anchor taken before the backup point stays reachable
on both branches, so a fork is only visible if some anchor was taken on the
discarded branch after the divergence. Restoring a pre-upgrade backup with
a new binary re-runs the chain migration and sets a new epoch at the
restored point; post-upgrade anchors are then unreachable, as above.
Consumers apply the applied-high-water check described in section 4.

**Legitimate deletion and compaction.** None exists today. Defined answers
so a future feature does not have to break the chain:

| Operation | Verifier result |
|---|---|
| Projection rebuild | Not affected: projections are not chained. |
| Payload redaction (erasure, crypto-shredding) | Keep the row and its link, drop the payload bytes, keep a server-written payload digest. The chain verifies; the verifier reports "verified, payload redacted". The chain cannot tell an authorized redaction from a deletion that kept the digest, so each redaction needs its own attested record (who, why, when). Needs its own ADR. |
| Stream or prefix compaction | Must not delete rows: a deleted event is a hole in its tenant chain. Compact by redacting to stubs (the fields needed for hashing plus links stay). |
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
  defect, fixed by minting a command id at the trigger. Within ESP, reusing
  a key for a new batch is not silently accepted: the fingerprint covers
  each event's `aggregate_nonce` and `event_id`, so the third write of X
  under the first X's key is `ALREADY_EXISTS`. ESP changes nothing here.

### 7. Threat model and performance

Verification always means: the range read is **connected by links to a
trusted head** (anchored at or after the range's end) and its lower end is
proven (genesis or an already trusted hash; section 4). A trusted start
alone proves nothing: an attacker can keep it and recompute everything
after it.

**Detected**, for such a range:

- modification of any covered field or payload byte of a chained event;
- reorder, insertion or excision of chained events, within a stream or
  across streams of a tenant (tenant head), or within one stream (stream
  head);
- splicing another tenant's events in (tenant id is covered);
- truncation or rollback of the tail, including restoring an older backup,
  when the trusted head is newer than the truncation point;
- deletion of a chain's prefix (the new first event's predecessor link is
  not zero).

**Not detected:**

- **Unanchored suffix.** Everything after the newest trusted head. An
  attacker who can write the database can recompute that suffix; the hash
  is unkeyed and the algorithm public by design. Anchor freshness is the
  defense: the window of undetectable rewrite is the time since the last
  anchor.
- **Rewrite before any anchor.** With no anchor taken before the tampering,
  the whole chain can be recomputed.
- **Equivocation.** A server can show different, internally valid histories
  to different readers. Only readers that compare heads (or check them
  against a shared anchor or transparency log) notice.
- **Withholding / staleness.** A server can serve a consistent stale
  prefix. Only a reader holding a newer anchor notices.
- **Restore forks** are detected only by an anchor taken on the discarded
  branch after the divergence (it becomes unreachable). Anchors from before
  the divergence are reachable on both branches. Both branches verify on
  their own; which is legitimate is an operational question.
- **Deleted tails or a deleted tenant followed by new appends.** The server
  chains onto what is left (or starts a new genesis); only an anchor newer
  than the deletion point detects it.
- **A compromised server at append time** chains whatever it chooses.
  Writers can read back and compare their own events. Server-signed heads
  would add protection against a database-only attacker (not against a
  compromised server); a possible later addition.
- Anything outside `events`: idempotency records, checkpoints, projections.
  Legacy (pre-epoch) events, unless sealed.
- **Confidentiality.** The chain hides nothing. Do not assume a published
  hash hides its inputs: `event_id` is not required to be random or
  secret, and an event whose metadata and payload are guessable can be
  confirmed by hashing guesses. Anchor heads where their audience may see
  the log anyway. (The separate hazard in #308, identifiers derived from
  short user text, is unrelated to integrity hashing.)

**Performance budget.** The idle-host tables in
`docs/performance/POSTGRES-BASELINE.md` predate #370; the post-#370
re-baseline was taken on a loaded host and is ratio-only. So the budget is
relative: run `make bench-pg` (and the full profile on a quiet host)
interleaved, main / branch / main / branch, and compare neighbouring runs,
as the baseline document prescribes. Include batch 1 and batch 100, many
tenants, replay, and keyed appends (the idempotency re-check path).

| Path (post-#370 quick reference) | Reference | Budget vs main |
|---|---|---|
| Append, 1 tenant, batch 1, same-tenant ceiling | ~970 ev/s | at most 5% lower |
| Append, 1 tenant, batch 1, 1 writer, p99 | ~8 ms | at most +1 ms |
| Append, 1 tenant, batch 100 | ~19k ev/s | at most 10% lower |
| Append, 8 tenants, batch 1 | ~1,800 ev/s | at most 5% lower |
| `ReadAll` replay | 70k to 145k ev/s (pre-#370) | at most 10% lower |

Why it should fit: the locked window is round-trip bound (baseline finding
2), and the design adds no round trip. Payload digests are computed before
the lock. Inside it, hashing is per event over metadata whose size the
client controls (headers are unbounded today), so the cost is linear in
metadata bytes: about a microsecond for typical events, more for large
header maps; batch 100 shows it first. The added writes are the link
columns, one index entry per event and their WAL. If the budget is missed,
the first fallbacks are tenant-only links and a cap on header bytes.
Client-side verification is per event, local and cheap relative to
receiving the event.

## Alternatives considered

- **Per-consumer chains in the envelope (status quo downstream).** Each
  consumer picks coverage and encoding; none can cover server-assigned
  positions. Rejected (#308).
- **Tenant-only or stream-only chain.** Discussed in section 1; tenant-only
  is the fallback, stream-only cannot detect a whole stream deleted or
  added and gives no single head to anchor.
- **Offline anchored checkpoint manifests** (a job periodically hashes each
  tenant's log range and anchors the digest; no append-path change). Same
  anchoring delay as periodically anchored chain heads, and a stream can be
  authenticated by scanning the covered tenant range. Differences: no
  per-event evidence on the wire (a reader cannot check a live
  subscription or localize a failure without rescanning), positions after
  the last manifest are unprotected, and every consumer that wants to check
  must reimplement the range hash. A reasonable stopgap if hashing on the
  append path proves too costly; not the recommended primitive.
- **Merkle tree / transparency log (RFC 6962 style).** Logarithmic
  inclusion and consistency proofs (would let a stream be proven against a
  tenant anchor without a tenant walk), much more machinery. Merkle
  checkpoints over v1 chain heads can be added later without changing v1.
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
   `AppendResponse.last_event_hash`, capability constant (not advertised),
   `ReadAll` clamp of `from_global_nonce`. Regenerate TS and Python stubs;
   no SDK behavior change.
4. Memory backend links, retry returns the original hash, conformance cases
   in `eventstore_core::conformance` (stream and tenant continuity, tenants
   independent, keyed retry returns the original hash, client link ignored,
   rollback leaves heads untouched, refuse to chain onto an unlinked
   predecessor). Advertise `hash_chained_log` on memory.
5. Postgres: index migration (concurrent), chain migration (columns,
   `NOT VALID` check, epoch, idempotency column), append path, conformance
   plus forced-race tests (first append of a new tenant racing, large
   batches, deadline expiry inside the lock), bench before/after against
   the budget. Advertise `hash_chained_log` on Postgres.
6. Rust SDK: verification in the low-level client and projection runner,
   `RecordedEvent.chain`, `verify_stream`, `verify_tenant_range`, opt-in
   applied-high-water hash check, docs.
7. Operations: `BACKUP-RESTORE.md` and drill (chain verifies after restore;
   older-restore rollback and consumer divergence detected), upgrade
   procedure.
8. TypeScript and Python verifiers against the golden vectors.

## Consequences

- A reader can check history instead of trusting storage, given an anchor
  it trusts. Without anchoring, the chain catches accidents and partial
  tampering but not a determined database administrator.
- `EventMetadata` changes now require a chain-format decision; the guard
  test enforces it.
- Upgrading a Postgres store needs a short write drain and a concurrent
  index build.
- Every event costs about 100 bytes plus an index entry more; appends do
  slightly more work under the tenant lock, within the stated budget.
- Future deletion features must redact to stubs rather than delete rows, or
  accept that they end verifiability.
