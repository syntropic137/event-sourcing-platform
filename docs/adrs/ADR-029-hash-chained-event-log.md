# ADR-029: Hash-Chained Event Log

**Status:** Proposed
**Date:** 2026-10-07 (revised 2026-10-08)
**Deciders:** NeuralEmpowerment
**Related:** [ADR-026](ADR-026-subscription-failure-semantics.md), [ADR-027](ADR-027-cross-language-event-envelope.md), [ADR-028](ADR-028-append-idempotency-semantics.md), [ADR-030](ADR-030-extension-model-capabilities-not-plugins.md), issues #308, #337, #366, #403

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
service would pick its own coverage, encoding and bugs, and none could cover
server-assigned positions.

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
- **Migrations run at startup.** `connect_with_config` applies every
  pending sqlx migration when a server starts.
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
  The Rust event-sourcing SDK (`event-sourcing/rust`) converts to
  `RecordedEvent`, which drops `content_schema` and `payload_sha256`,
  reports `event_version` 0 as 1, and may be upcast. Backward reads are
  broken today: a backward page's continuation cursor is taken from the
  newest event of the page, so pages overlap (#403, both backends), and
  `ReadAll` backward from `u64::MAX` reads nothing (the value wraps to -1
  in SQL).
- **Capabilities (#366).** `GetServerInfo` reports version, API, backend and
  named guarantees; each backend opts in explicitly. It reports no store
  identity.
- **No deletion exists.** There is no tenant deletion, compaction or
  redaction feature today.

## Decision

### 0. A built-in, opt-in, irreversible store capability

Hash chaining is **built-in server code behind a store-wide flag**, not a
plugin. There is no plugin framework, hook trait, registry or dynamic
loading; ADR-030 records that rule for every future extension.

- **Off by default.** A store that never activates the chain behaves
  exactly as today, apart from inert schema (section 4).
- **Store-wide.** One flag for the whole store, all tenants. Per-tenant
  activation is out of scope.
- **Activated once, offline, irreversibly.** An operator runs
  `eventstore-admin chain-activate` during a write pause (section 5).
  After that there is no off switch; disabling the chain means migrating
  to a new store.
- **Config must match the store.** The server reads `HASH_CHAIN` (`on` or
  `off`, default `off`) at startup and compares it with the store's
  activation record. It refuses to start when they disagree: `off` on an
  activated store (which would silently write unchained events) and `on`
  on a store that is not activated (run `chain-activate` first, or a
  pre-activation backup was restored; section 5).
- **Rust first.** Encoding, backends, admin tool, verifier and anchoring
  are Rust. TypeScript and Python get regenerated proto stubs only (new
  fields appear, no behavior); a verifier in those languages waits until
  an application needs one. The golden vectors keep that possible.

### 1. Chain scope: one hash per event, linked into both its stream and its tenant

Options:

| Option | Detects (against a trusted head) | Verify one aggregate | Anchoring | Write cost |
|---|---|---|---|---|
| A. Per stream | Edits, reorders, excision inside that stream | Read the stream, given a trusted stream head | One head per stream (millions) | Stream head is already read under its row lock |
| B. Per tenant, `global_nonce` order | Everything in the tenant log, including a whole stream deleted or added, reorder across streams | Read the tenant range from the stream's first event to a trusted tenant head | One head per tenant | Read the tenant's last event under the tenant lock |
| C. Both, in one hash | A or B, depending on which head is trusted | As A, or as B | As A or B | A plus B, 32 more bytes per event |

**Decision: C.** Each event's hash commits to two predecessors: the
previous event of its stream and the previous event of its tenant. One hash,
two links. Adding stream links later would need a new format, so they are
in v1.

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
  (Merkle inclusion proofs would shorten that; out of scope).
- **Fallback: tenant-only (B)** if C misses the performance budget
  (section 9). The fallback is decided before the format ships, never
  after.
- Contention topology is unchanged: the tenant lock already serializes one
  tenant's appends. The serialized window does get longer (hashing, one
  head read, wider rows); section 9 bounds it. A chain over all tenants
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
  payload bytes and keep the chain verifiable (section 8). That digest is
  computed by the server; the client-supplied `payload_sha256` is just
  another covered metadata field and is never trusted as a substitute.
- Genesis (no predecessor) is 32 zero bytes.
- Precondition: the Postgres database encoding is `UTF8` (checked at
  startup), so text round-trips byte for byte.
- **Payloads that reference external content.** The chain covers the event
  as stored. If an event carries a reference and content hash of a blob
  kept elsewhere, the chain covers that reference and hash, not the blob;
  checking the blob against its hash is the application's job. That
  storage pattern is documented separately, not by this ADR.

**Algorithm: SHA-256.** Already a Rust dependency (`sha2`, used by the
fingerprint), in the Node and Python standard libraries, and in
Postgres (`sha256()`, so operators can audit in SQL), FIPS-approved, and
hardware-accelerated on current x86 and ARM. Events are small and an
append's cost is the commit, not the hash (section 9). Length extension is
irrelevant: this is not a MAC.

**Versioning and domain separation.** The tag `esp/event-chain/v1` is part of
the hashed bytes, and every stored link records its format number. A new
field in `EventMetadata`, a new algorithm or a new layout is format v2 with
a new tag; v1 links stay verifiable forever. A verifier that does not know a
format reports the event as unverifiable, never as valid. A test fails if
`EventMetadata` gains a field the v1 encoder does not list as covered or
explicitly excluded.

### 3. The server computes the link, inside the append transaction

**Decision: server**, at append, under the tenant lock. A client cannot: it
does not know `global_nonce`, `recorded_time_unix_ms` or the tenant's
previous event (other writers). A client-computed stream-only chain would
also be forgeable by the client and unenforceable by the store.

Postgres append on an activated store:

1. Before the transaction: compute `SHA-256(payload)` per event (the only
   per-byte work), so payload size does not lengthen the locked window.
2. Unchanged: stream row lock, tenant lock, #363 re-checks. Lock order
   (stream row, then tenant) is preserved.
3. The stream-head re-check query also reads the hash of the stream's last
   event (primary key `(tenant_id, aggregate_id, last_nonce)`), the
   tenant's last event (`global_nonce`, `event_hash`) via a new index
   `(tenant_id, global_nonce)`, and draws the batch's nonces (`SELECT
   nextval(...) FROM generate_series(1, n)`, sorted), in the same
   statement: no added round trip, which matters because the locked window
   is round trip bound (`POSTGRES-BASELINE.md`, finding 2). Nonces are
   still drawn after the lock is taken, so #337 commit ordering is
   unchanged. Heads are read from the event rows themselves, not from a
   cache table that could drift from them.
4. **Predecessor rules.** If the stream's or tenant's last event is at or
   above the epoch, it must carry a recognized link; otherwise the append
   is refused (`FAILED_PRECONDITION`, logged with the position). If the
   tenant's last event is below the epoch (the tenant's first chained
   event), `prev_tenant_hash` is that tenant's **seal head** from
   `chain_seals` (section 5), or zero if the tenant has no seal; a tenant
   with events below the epoch but no seal row is refused. If the
   stream's last event is below the epoch, `prev_stream_hash` is zero.
   This is a structural check, not verification: the server does not
   re-verify history on append. If chained rows were deleted, the server
   chains onto the surviving predecessor; that fork is visible only to an
   anchor (section 9).
5. Rust computes each event's hash in batch order, chaining within the
   batch, with the one `recorded_ms` it already uses.
6. The single write statement (#370) inserts the events with explicit
   `global_nonce` and link columns, and stores the batch's last
   `event_hash` in the idempotency row. The existing defensive check (one
   row per event, increasing nonces) stays.

A rolled-back append leaves no trace (heads are rows of the same
transaction). Drawn-but-unused nonces leave gaps, as rollbacks already do.
A store that is not activated runs today's append path unchanged.

Memory computes the same links under its existing write lock. Memory is
not persistent, so with `HASH_CHAIN=on` it is activated at startup with
epoch 1, no legacy events and no seals. Memory is for development and
tests: it gets a fresh `store_id` per process, publishes no inventory, and
is outside the anchoring and restore procedures.

**Idempotent retries.** A keyed retry of a committed batch must return the
original hash. The `idempotency` row gains `last_event_hash`, written in the
same statement as the events, so the replay path returns it like
`last_global_nonce`. Rows whose `last_global_nonce` is below the epoch were
never chained and return an empty hash; a row at or above the epoch
without a hash (impossible unless tampered with) fails the replay with
`DATA_LOSS`.

A retry receipt comes from the `idempotency` table, which is outside the
chain, so it is a claim, not evidence. An SDK never replaces a newer anchor
with it, and verifies it (read the event at that position, check it
connects to a trusted head) before using it as an anchor.

The link lives in `EventData`, not `EventMetadata` (section 4), so the
ADR-028 fingerprint is unchanged. A client that sends a link on append has
it ignored and replaced, like `global_nonce` and `recorded_time_unix_ms`
today.

### 4. Storage and API

**Postgres schema (startup migrations, inert until activation):**

- `events`: `chain_format SMALLINT`, `event_hash BYTEA`,
  `prev_stream_hash BYTEA`, `prev_tenant_hash BYTEA`, all nullable.
  Catalog-only change.
- `idempotency.last_event_hash BYTEA`, nullable.
- Index `(tenant_id, global_nonce)`, built `CONCURRENTLY` in its own
  non-transactional migration (sqlx `-- no-transaction`) so a large table
  is not write-locked while it builds. A failed concurrent build leaves an
  `INVALID` index that `IF NOT EXISTS` would skip; the migration checks
  `pg_index.indisvalid` and drops and rebuilds it. It also serves the
  tenant-filtered `global_nonce` scans of `ReadAll` and `Subscribe`.
- Tables `chain_activation` (at most one row: `store_id UUID`, `epoch
  BIGINT`, `chain_format`, `activated_at`, `inventory_digest`) and
  `chain_seals (tenant_id PRIMARY KEY, legacy_count, first_nonce,
  last_nonce, seal_head)`, both empty.

None of this changes behavior: no constraint, no epoch, off-mode appends
unchanged. **Activation** (section 5) fills the two tables and installs
enforcement.

Cost once active: 98 bytes of link data per event plus varlena headers, one
more index entry per event (the index exists either way), and the matching
WAL; measured in section 9.

**Proto (`eventstore.v1`, additive):**

```proto
message EventChainLink {
  uint32 format           = 1;  // 1 = ADR-029 v1
  bytes  event_hash       = 2;  // 32 bytes
  bytes  prev_stream_hash = 3;  // 32 bytes, zero at a stream's chain start
  bytes  prev_tenant_hash = 4;  // 32 bytes, seal head or zero at a tenant's chain start
}
message EventData {
  EventMetadata  meta    = 1;
  bytes          payload = 2;
  EventChainLink chain   = 3;  // set on every read path for chained events; ignored on append
}
message AppendResponse {
  ...
  bytes last_event_hash  = 3;  // hash of the batch's last event (head of the
                               // stream, and of the tenant, at commit)
}
message GetServerInfoResponse {
  ...
  string store_id    = 5;  // set once activated
  uint64 chain_epoch = 6;  // first chained global_nonce; 0 = not activated
  uint32 chain_format = 7;
}
```

These server-reported values are informational. A verifier takes
`store_id` and the epoch from the anchored activation inventory, never from
the server it is checking.

**No new RPC in v1.** A head is the last event of a backward read with an
explicit upper bound, which the SDK helper hides; the same PR clamps
`ReadAll`'s `from_global_nonce` to `i64::MAX` so `u64::MAX` means "from the
end". Backward reads depend on #403 (overlapping backward pages); until it
lands, verifiers use forward walks only and the head helper reads a single
event backward. A writer gets heads from `AppendResponse.last_event_hash`.
A server-side "verify" RPC is rejected: the threat model distrusts the
server, so verification belongs to the reader.

**Capability `hash_chained_log`:** advertised only by an activated store.
Every event at or above the epoch carries a v1 link, links are present on
all read paths, keyed retries return the original hash, and the server
refuses an append whose stream or tenant predecessor (at or above the
epoch) has no recognized link. It does not promise that the server checked
history before appending. A client that requires it calls
`require_capabilities`.

### 5. Activation, legacy seal, parity, restore

**Upgrade and activation procedure.**

1. Deploy the new version with `HASH_CHAIN=off`. Startup migrations add
   the inert schema; writes continue. The index build is concurrent.
2. **Write pause.** Stop every server instance (old and new versions).
3. Run `eventstore-admin chain-activate --anchor <sink>` against the
   database directly. It refuses unless the schema is migrated and
   `chain_activation` is empty, then runs **one transaction**:
   1. `LOCK TABLE events IN EXCLUSIVE MODE` (reads allowed, writes
      blocked; a stray writer waits instead of slipping in).
   2. Check the `global_nonce` sequence has increment 1 and `CACHE 1`
      (the `BIGSERIAL` default; a larger cache would let a stray session
      insert a pre-cached nonce below the epoch), then `epoch =
      nextval(...)` and assert `epoch > max(global_nonce)`. Every committed
      event is below it; every later insert draws a larger value.
   3. Seal every tenant with events below the epoch (below) and insert
      the `chain_seals` rows.
   4. Insert `chain_activation` (fresh random `store_id`, epoch, format 1,
      time, inventory digest).
   5. `ALTER TABLE events ADD CONSTRAINT events_chained CHECK
      (global_nonce < <epoch> OR (chain_format = 1 AND
      octet_length(event_hash) = 32 AND octet_length(prev_stream_hash) = 32
      AND octet_length(prev_tenant_hash) = 32)) NOT VALID`. Existing rows
      are all below the epoch. This enforces structure, not cryptographic
      validity; a v2 format widens it in the same migration that adds v2.
   6. Commit.

   A crash before commit leaves only a nonce gap; rerun. After commit the tool
   builds the activation inventory from the committed tables, publishes it
   through the anchor sink (section 6) and reads it back to confirm. If
   publishing fails, `eventstore-admin chain-export-inventory` retries
   from the same tables; the result is byte-identical.
4. Confirm the inventory is published to a medium outside the database's
   trust domain, and record its digest out of band (section 6). Do not resume writes before that: until it is anchored,
   the seal is only as trustworthy as the database.
5. Start every instance with `HASH_CHAIN=on`. Writes resume, chained.

Fencing comes from the pause plus the constraint, not from startup checks.
An old binary started by mistake, or an instance still running with
`HASH_CHAIN=off` from before activation, draws a nonce above the epoch and
writes no link, so its append fails the constraint (fail closed). A new
instance with `HASH_CHAIN=off` refuses to start. A Postgres server never
activates itself (memory, which has no history, does at startup).

**Legacy seal (the backfill).** History below the epoch is not rewritten:
rewriting would mean disabling the append-only trigger and updating the
largest table. Instead the activation transaction computes, per tenant, a
seal over its legacy events in `global_nonce` order:

```
s_0 = 32 zero bytes
s_i = event_hash v1 of legacy event i with tag "esp/legacy-seal/v1",
      prev_stream_hash = zero, prev_tenant_hash = s_(i-1)
seal_head = s_n
```

stored as `(tenant_id, legacy_count = n, first_nonce, last_nonce,
seal_head)`. The tenant's first chained event takes `prev_tenant_hash =
seal_head`, so legacy and live history form one chain per tenant. The seal
runs through each tenant's final legacy event inside the pause, so there is
no gap between seal and chain. A legacy stream's first chained event keeps
`prev_stream_hash = zero`; its legacy prefix is covered by the tenant seal
only (no per-stream seals). Cost: one full ordered scan of `events` during
the pause; the pause lasts as long as that scan (benchmarked in the
activation PR).

The verifier never infers "sealed" from a non-zero predecessor. For each
tenant it looks up the tenant's entry in the anchored inventory, recomputes
the seal from the legacy rows it reads, compares it with the anchored
`seal_head`, `legacy_count` and bounds, then walks the chained events from
the one whose `prev_tenant_hash` equals that seal head. A tenant with no
entry must have no events below the epoch, and its first chained event
links to zero.

What the seal **proves**, given the inventory is anchored outside the
database's trust domain:

- Any later modification, insertion, deletion or reorder of a sealed
  legacy event is detected when its tenant is verified.
- Deletion of a tenant listed in the inventory, or of its whole legacy
  history, is detected.
- Every anchored post-activation head of a tenant covers its legacy prefix
  through the seal link.

What it does **not** prove:

- **Anything about history before activation.** The seal certifies what
  the store contained at the pause, trusted to the operator who ran it.
  Tampering before activation is sealed in, not caught.
- Anything if the inventory is kept only in the database: an attacker who
  can write the database can recompute the seal and the chain.
- Changes made between commit and publication of the inventory (step 3 to
  4). The procedure keeps writes stopped; the operator is trusted there.
- Equivocation: the seal is checked against the history served to the
  verifier. A compromised server can show other readers other histories.
- Physical storage form: `NULL` vs empty and `JSONB` layout changes are
  invisible by design (section 2).
- Tenants created after activation: they are not in the inventory, start
  at genesis zero, and are covered only by their own later head anchors.

**Memory backend parity.** Same links, same encoding, same conformance
cases. Memory has no legacy events, so its epoch is 1 and it has no seals.
`recorded_time_unix_ms` differs per event in memory and per batch in
Postgres; both are hashed as stored, so both verify.

**Backup and restore.** A whole-database `pg_dump` (the runbook's
requirement) carries the link columns, `idempotency.last_event_hash`,
`chain_activation`, `chain_seals` and the constraint. Before writes resume
after any restore, the runbook runs `eventstore-admin chain-verify`, which
fetches the pinned activation inventory and every held head record from
the anchor sink, verifies every tenant's seal and chain to its restored head, and
checks each anchored head for **reachability** (an ancestor of, or equal
to, the restored head). Missing evidence is reported as unknown, never as
verified.

- **Pre-activation backup restored:** `chain_activation` is empty while an
  activation inventory exists in the anchor sink. A server with
  `HASH_CHAIN=on` refuses to start; `chain-verify` reports the mismatch.
- **Older post-activation backup restored:** the log rewinds. The
  restored chain verifies on its own; any head anchored after the backup
  point is unreachable, which is the rollback signal. A fork is only
  visible if some anchor was taken on the discarded branch after the
  divergence.
- **Clones** (a restored copy that also takes writes) share the
  `store_id`; two live writers on one `store_id` are an operator error that
  anchors expose as diverging heads.

Consumers keep the runbook's **applied high-water mark** rule
(BACKUP-RESTORE.md). The chain makes it checkable: a consumer can store the
`event_hash` of its applied high-water event and, on resume, re-read that
position; an absent event or different hash means the log under it
changed. Persisting that hash in `CheckpointStore` (which stores only a
`u64` today) is out of scope for v1.

### 6. Anchoring behind an adapter trait

Anchors are what make the chain and the seal evidence. The **server never
anchors**; the admin tool, or an application using the Rust SDK, publishes
records through a small trait, and verification fetches them back.

**Record format** (`esp/anchor/v1`, JSON carrier, binary digest):

```json
{
  "format": "esp/anchor/v1",
  "kind": "inventory",
  "store_id": "8d0e...-uuid",
  "chain_format": 1,
  "epoch": "1048577",
  "created_at_unix_ms": "1791500000000",
  "tenants": [
    { "tenant_id": "acme", "legacy_count": "5012",
      "first_nonce": "3", "last_nonce": "1048570", "seal_head": "<64 hex>" }
  ],
  "digest": "<64 hex>"
}
```

A `"kind": "heads"` record has the same envelope with `"heads": [{
"tenant_id", "global_nonce", "event_hash" }]` instead of `tenants`.

- 64-bit integers are decimal strings (JSON numbers lose precision above
  2^53); hashes are lowercase hex.
- `digest` is SHA-256 over a length-prefixed binary encoding of every
  other field (the `u64`/`str`/`h32` primitives of section 2, tag
  `esp/anchor/v1`, entries sorted by `tenant_id` bytes, duplicates
  rejected, entry count included). JSON layout, key order and whitespace
  are therefore irrelevant, and a reader recomputes the digest from the
  parsed fields before trusting a record.
- The inventory digest is also stored in `chain_activation.inventory_digest`
  for cross-checking (not as evidence).

**Trait** (crate `eventstore-anchor`, Rust):

```rust
pub struct AnchorRecord { /* parsed esp/anchor/v1 record */ }
pub struct AnchorRef { pub digest: [u8; 32], pub location: String }
pub enum AnchorKind { Inventory, Heads }
pub struct AnchorPage { pub records: Vec<AnchorRecord>, pub next: Option<String> }

#[async_trait]
pub trait AnchorSink: Send + Sync {
    /// Store the record durably. Create-only and idempotent by digest:
    /// republishing the same record succeeds; never overwrites.
    async fn publish(&self, record: &AnchorRecord) -> Result<AnchorRef, AnchorError>;
    /// Read one record back (publish confirmation, pinned inventory).
    async fn get(&self, at: &AnchorRef) -> Result<Option<AnchorRecord>, AnchorError>;
    /// Records for `store_id` of `kind`, in position order, paginated.
    async fn list(&self, store_id: Uuid, kind: AnchorKind, after: Option<&str>)
        -> Result<AnchorPage, AnchorError>;
}
```

- **Sinks store, verification decides.** Digest recomputation,
  reachability and fork checks live in shared code above the trait. Every
  retained head record is checked, not only the newest: each must be an
  ancestor of, or equal to, the current head, and two records with
  different hashes at the same `(tenant_id, global_nonce)` are a fork.
- **What the sink is trusted for.** Digests catch altered records, not
  omitted ones: a sink, or anyone holding its write or delete credentials,
  that hides records weakens the evidence, and nothing in the records
  reveals that. So: (a) the operator records the inventory digest out of
  band at activation (runbook, ticket, second medium), and `chain-verify`
  requires it (`--inventory <digest>`) and fetches that record by digest;
  (b) the medium must make records undeletable by whoever can write the
  database (protected git branch, object lock); (c) anchors cover what was
  published, nothing more. Signed or transparency-log adapters would add a
  receipt to `AnchorRef`; out of scope for v1.
- **Tenant coverage.** gRPC reads are per tenant and there is no tenant
  enumeration RPC. `chain-anchor` and `chain-verify` cover the tenants in
  the inventory, in earlier head records, and any passed explicitly;
  `chain-anchor` with database access can also discover tenants from the
  `(tenant_id, global_nonce)` index. A tenant created after activation and
  never anchored is not covered, and `chain-verify` prints the covered set.
- **Filesystem adapter (default, first PR):**
  `<dir>/<store_id>/<kind>/<zero-padded position>-<digest>.json`, written
  to a temp file, fsynced, then hard-linked into place (fails if the name
  exists) and the directory fsynced. On its own it is **not** an
  independent anchor: a local directory sits in the operator's trust
  domain. Pushing it to a protected git remote is an ops step the runbook
  describes.
- **S3 Object Lock adapter (later PR, cargo feature).** Other media are new
  adapters, not new formats.

`eventstore-admin` uses the trait for `chain-activate`, `chain-anchor`
(reads current tenant heads over gRPC and publishes a `heads` record) and
`chain-verify`. Scheduling `chain-anchor` is the operator's job; anchor
freshness bounds the undetectable rewrite window (section 9).

### 7. Rust SDK verification

- `eventstore_core::chain`: the one Rust implementation of the encoding,
  `event_hash`, the legacy seal, and a streaming verifier, used by both
  backends, the admin tool and the Rust SDK (as `fingerprint` is today).
  Published golden vectors (inputs, canonical bytes, hashes, including
  headers, empty fields, unicode, genesis, seal links and multi-event
  batches) are the contract any later port must meet.
- Verification runs on raw `EventData` as received, before conversion,
  version normalization, upcasting or decoding: in the low-level client,
  ahead of building `RecordedEvent`, which then carries the verified link.
  Re-verifying from a `RecordedEvent` is not supported.
- Two result levels, never conflated:
  - **consistent**: every event recomputes to its hash and links to the
    previous one, from a lower bound to the last event read;
  - **authenticated**: consistent, and the walk reaches a trusted head at
    or after the last event of interest.
- Completeness of a range comes from the links, not the nonces. A range is
  complete only if its lower end reaches genesis, the tenant's anchored
  seal head, or a hash the caller already trusts. A suffix that merely
  ends at the trusted head, with earlier events withheld, is incomplete.
- A stream's links restart at zero after activation, so a trusted stream
  head authenticates only the stream's chained suffix. `verify_stream`
  reports that boundary ("chained from aggregate nonce k"); the legacy
  prefix is covered only by verifying the tenant's seal.
- API: `verify_stream(aggregate_id, trusted_stream_head)` and
  `verify_tenant_range(from, trusted_lower, trusted_head)`, forward walks,
  each returning the level reached, the verified head, or the first failing
  position and reason (bad hash, broken link, non-monotonic position,
  unexpected genesis, unknown format, unchained event above the epoch, seal
  mismatch, not connected to the trusted head, lower bound not proven).
- Live consumers reach only **consistent** until a later anchor covers
  what they consumed. Tenant continuity needs every tenant event, so it is
  checked only on an unfiltered feed. The Rust projection runner reads the
  whole tenant log by default (`ProjectionRunner::new`, empty feed), so it
  can check tenant continuity; a runner narrowed with `with_feed_prefix`
  skips tenant predecessors and needs a separate unfiltered verification
  feed or a periodic `verify_tenant_range`.
- TypeScript and Python: regenerated stubs only. No verifier until an
  application needs one.

### 8. Legitimate deletion, and not idempotency

**Legitimate deletion and compaction.** None exists today. Defined answers
so a future feature does not have to break the chain:

| Operation | Verifier result |
|---|---|
| Projection rebuild | Not affected: projections are not chained. |
| Payload redaction (erasure, crypto-shredding) | Future, own ADR. Hashing the payload as a digest keeps this possible: keep the row and link, drop the bytes, keep a server-written digest (a column v1 does not add), plus an attested redaction record. |
| Stream or prefix compaction | Must not delete rows: a deleted event is a hole in its tenant chain. Compact by redacting to stubs. |
| Tenant deletion | Detected as "tenant absent" by any anchor or inventory entry for it. The chain cannot tell a legitimate deletion from an attack; deletion must leave an attested record outside the tenant. |

**Not idempotency.** An idempotency key identifies **a command attempt**
and must be reproducible before the outcome is known (ADR-028). An event
hash identifies **a position in history** and depends on server-assigned
values, so it can never be an idempotency key, and the fingerprint never
includes it (the link is outside `EventMetadata`). DreamShip's
payload-derived key, which swallows a legitimate X -> Y -> X revision, is a
downstream codec defect fixed by minting a command id at the trigger; ESP
already rejects reuse of a key for a different batch (`ALREADY_EXISTS`).

### 9. Threat model and performance

Verification always means: the range read is **connected by links to a
trusted head** (anchored at or after the range's end) and its lower end is
proven (genesis, the anchored seal, or an already trusted hash). A trusted
start alone proves nothing: an attacker can keep it and recompute
everything after it.

**Detected**, for such a range:

- modification of any covered field or payload byte of a chained event,
  and of any sealed legacy event (via the anchored inventory);
- reorder, insertion or excision of chained events, within a stream or
  across streams of a tenant (tenant head), or within one stream (stream
  head);
- splicing another tenant's events in (tenant id is covered);
- truncation or rollback of the tail, including restoring an older backup,
  when the trusted head is newer than the truncation point;
- deletion of a chain's prefix or of an inventoried tenant.

**Not detected:**

- **Unanchored suffix.** Everything after the newest trusted head. The hash
  is unkeyed and public by design; anchor freshness is the defense.
- **Equivocation.** A server can show different, internally valid
  histories to different readers. Only readers that compare heads against
  a shared anchor notice.
- **Withholding / staleness.** A consistent stale prefix is noticed only
  by a reader holding a newer anchor.
- **Restore forks** are detected only by an anchor taken on the discarded
  branch after the divergence.
- **A compromised server at append time** chains whatever it chooses.
  Server-signed heads would help against a database-only attacker; a
  possible later addition.
- **Pre-activation history** beyond what the seal certifies (section 5).
- Anything outside `events`: idempotency records, checkpoints, projections,
  externally stored blobs (only their reference and hash are covered).
- **Confidentiality.** The chain hides nothing; an event whose metadata
  and payload are guessable can be confirmed by hashing guesses. Anchor
  heads where their audience may see the log anyway.

**Performance budget.** Run `make bench-pg` (and the full profile on a
quiet host) interleaved, main / branch / main / branch, activated store vs
main, and compare neighbouring runs, as the baseline document prescribes.
Include batch 1 and batch 100, many tenants, replay, and keyed appends.

| Path (post-#370 quick reference) | Reference | Budget vs main |
|---|---|---|
| Append, 1 tenant, batch 1, same-tenant ceiling | ~970 ev/s | at most 5% lower |
| Append, 1 tenant, batch 1, 1 writer, p99 | ~8 ms | at most +1 ms |
| Append, 1 tenant, batch 100 | ~19k ev/s | at most 10% lower |
| Append, 8 tenants, batch 1 | ~1,800 ev/s | at most 5% lower |
| `ReadAll` replay | 70k to 145k ev/s (pre-#370) | at most 10% lower |

A store that is not activated must show no regression. Why the activated
path should fit: the locked window is round-trip bound, and the design adds
no round trip; payload digests are computed before the lock; hashing inside
it is linear in metadata bytes (headers are unbounded today). If the budget
is missed, the fallbacks are tenant-only links, then a cap on header bytes.

## Alternatives considered

- **A plugin framework** (hook trait, registry, WASM, sidecar). Rejected;
  see ADR-030.
- **Per-consumer chains in the envelope.** Each consumer picks coverage and
  encoding; none can cover server-assigned positions. Rejected (#308).
- **Chain on by default for every store.** Costs every user, and a
  migration that silently changes append behavior for running fleets.
  Rejected for opt-in activation.
- **Server-side activation at startup.** Cannot fence other running
  instances and makes a config change irreversible by accident. Rejected
  for an explicit admin command during a pause.
- **In-place link backfill of legacy rows.** Requires disabling the
  append-only trigger and rewriting the largest table. Rejected for the
  offline seal.
- **Offline anchored checkpoint manifests only** (no append-path change).
  Retrospective range commitments: no per-event evidence on the wire, no
  append receipts, no live continuity checks. A different product from
  #308; rejected as the primitive.
- **Server-side anchoring.** The server would hold the credentials of the
  medium meant to be outside its trust domain. Rejected; anchoring is a
  client-side tool behind `AnchorSink`.
- **Merkle tree / transparency log (RFC 6962 style).** Logarithmic proofs,
  much more machinery. Merkle checkpoints over v1 heads can be added later
  (a new anchor kind) without changing v1.
- **Database-level controls (pgaudit, WAL archiving, ledger databases).**
  Give a reader nothing to check. Useful as defense in depth.
- **Protobuf bytes as the canonical form.** Not canonical across
  implementations; rejected (section 2).

## Rollout (small PRs, Rust first)

0. Prerequisite, independent: #403, backward pagination overlap (both
   backends, conformance tests for gaps, multipage and genesis).
1. This ADR and ADR-030.
2. `eventstore_core::chain`: encoding, hash, seal, forward verifier, golden
   vectors, `EventMetadata` coverage guard. No behavior change.
3. `eventstore-anchor`: record format and digest, `AnchorSink`, filesystem
   adapter, golden record vectors.
4. Proto: `EventChainLink`, `EventData.chain`,
   `AppendResponse.last_event_hash`, ServerInfo `store_id` /
   `chain_epoch` / `chain_format`, capability constant (not advertised),
   `ReadAll` clamp. Regenerate TS and Python stubs; no behavior there.
5. Memory backend: `HASH_CHAIN`, links, conformance in both modes (on,
   off, retry returns original hash, client link ignored, rollback leaves
   heads untouched, refuse unlinked predecessor). Advertise on memory when
   on.
6. Postgres inert migrations (index first, concurrent, `indisvalid`
   check).
7. Postgres chained append path, startup config/store check,
   `eventstore-admin chain-activate` (seal, constraint, inventory publish),
   forced-race and crash tests, bench against the budget (and the
   tenant-only fallback decision). Tamper tests: `UPDATE` with the trigger
   disabled, `DELETE`, `TRUNCATE`, older-dump and pre-activation restore.
8. Rust SDK verification (`verify_stream`, `verify_tenant_range`,
   `RecordedEvent.chain`), `eventstore-admin chain-anchor` and
   `chain-verify`. Runbook: activation, anchoring, restore drill.
9. Later, on demand: S3 Object Lock adapter; TS/Python verifiers;
   checkpoint-hash persistence; signed heads.

## Consequences

- An operator who activates the chain gets history a reader can check
  instead of trust, given anchors held outside the database. Without
  anchors it catches accidents and partial tampering, not a determined
  database administrator.
- Stores that never activate pay only inert columns, an index and two
  empty tables.
- Activation is one-way and needs a write pause as long as one ordered
  scan of `events`.
- Two append paths (chained and not) live in both backends, contained by
  conformance tests in both modes.
- `EventMetadata` changes now require a chain-format decision; the guard
  test enforces it.
- Future deletion features must redact to stubs rather than delete rows,
  or accept that they end verifiability.
