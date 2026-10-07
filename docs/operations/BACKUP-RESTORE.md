# Event Store Backup, Restore, and Recovery

Operational contract for the Postgres event store (`BACKEND=postgres`):
what an acknowledgment means, how to retry safely, how consumers recover,
and how to back up and restore. Every claim marked **(drilled)** is executed
by the recovery drill suite (issue #355).

```bash
make recovery-drill                 # from the repo root (or make -C event-store recovery-drill)
make -C event-store recovery-drill DRILL_ARGS="--nocapture"   # with drill output
make -C event-store recovery-drill-clean                      # remove containers of an interrupted run
```

The drills need Docker and take a few minutes. They are `#[ignore]`d, so
`cargo test`, `make qa` and `make qa-full` skip them; CI runs them nightly
and on demand (`.github/workflows/recovery-drill.yml`).

## Drill scope and safety

- Every drill creates its own Postgres containers (`postgres:15`, override
  with `DRILL_PG_IMAGE`), labelled `esp-recovery-drill=<DRILL_RUN_ID>`, and
  removes them (with volumes) on completion or panic. `make recovery-drill`
  also removes any container left with its run label. Set
  `DRILL_KEEP_CONTAINERS=1` to keep them for inspection.
- The drills never connect to dev infrastructure (`make dev-start`),
  `DATABASE_URL`, or `TEST_DATABASE_URL`. Kills, outages, and restores only
  ever target containers the drill created.
- The real `eventstore-bin` binary is started as a child process and killed
  with SIGKILL. Its logs go to `$TMPDIR/esp-recovery-drill-logs/`.

| Drill (`event-store/eventstore-bin/tests/`) | Proves |
|---|---|
| `drill_restart::acked_appends_survive_eventstore_and_postgres_crashes` | Acked appends are present, byte for byte, at their acked positions after SIGKILL of the event store and of Postgres. A running event store recovers its pool after a Postgres crash without a restart. |
| `drill_restart::kill_before_insert_leaves_nothing_and_retry_commits_once` | Kill while the append waits for the ordering lock: nothing persists; identical retry commits once. |
| `drill_restart::kill_after_insert_before_commit_rolls_back_and_retry_commits_once` | Kill after events and aggregate head were written in the transaction, before COMMIT: all rolled back; retry commits once. |
| `drill_restart::lost_ack_after_commit_identical_retry_is_idempotent` | Commit succeeded, process died before the client saw the ack: identical retry returns the original result; no key gives `ABORTED`; same key with other content gives `ALREADY_EXISTS`; one copy stored. |
| `drill_restart::kill_storm_during_writes_reconciles_exactly_once` | Six SIGKILL/restart cycles under a writer that retries every command with its stable key: each command stored exactly once, every ack names its commit, projection matches. |
| `drill_connectivity::db_outage_during_replay_surfaces_and_resume_from_checkpoint_completes` | Store-to-Postgres traffic held by a proxy once the replay starts (the request never reaches Postgres), then all connections cut: `UNAVAILABLE` naming `replay` and `resume from global_nonce <checkpoint+1>`, no caught-up marker, nothing delivered. Appends through the cut node fail visibly. Resume from checkpoint applies every event, including those another node wrote during the outage. |
| `drill_connectivity::db_outage_during_live_consumption_surfaces_and_resume_completes` | Same for the live phase. |
| `drill_connectivity::lagging_checkpoint_redelivery_and_eventstore_kill_yield_idempotent_projection` | Consumer checkpoints every 7 events: restarts redeliver already-applied events, which the projection skips while still advancing the checkpoint (also with no new writes); event store SIGKILLed under a live subscription ends the stream with an error. |
| `drill_backup::pg_dump_restore_to_fresh_instance_preserves_log_and_rebuilds_projection` | `pg_dump` / `pg_restore` into a separate fresh container; see below. |
| `drill_backup::historical_versions_replay_only_with_retained_upcasters` | Historical event versions replay only with the consumer's upcasters; a missing one stops replay explicitly at the first such event. |
| `drill_backup::external_projection_ahead_of_restored_log_is_detected_and_rebuilt` | A consumer outside the restored database whose checkpoint is behind the restored head but whose state is ahead of it: checkpoint-only resume gives wrong state; the applied high-water check plus rebuild gives the exact state. |

## Durability configuration

An acknowledged append (`Append` returned OK) is a committed Postgres
transaction. It survives an event-store or Postgres process crash **only if**
Postgres runs with:

| Setting | Required | Why |
|---|---|---|
| `fsync` | `on` | Otherwise a crash can corrupt or lose committed data. |
| `synchronous_commit` | `on` (or `remote_apply`/`remote_write` with replicas) | `off` acknowledges before the WAL is flushed: recent acked appends can vanish on crash. |
| `full_page_writes` | `on` | Protects against torn pages after an OS crash. |

These are the Postgres defaults. The drills pass them explicitly and assert
them with `SHOW` before and after each crash **(drilled)**.

## Appends: acknowledgments and retries

Outcomes of an append whose response the client did not receive (timeout,
connection reset, event-store crash) are unknown: it may or may not have
committed. The store makes the retry safe:

- **Use a stable idempotency key and an identical request** (same event ids,
  payloads, metadata, `expected_aggregate_nonce`). If the original committed,
  the retry returns the original `AppendResponse` (same `last_global_nonce`)
  and writes nothing **(drilled)**. If it did not, the retry commits once
  **(drilled)**.
- Same key, different content: `ALREADY_EXISTS`; nothing written **(drilled)**.
- No key: a retry after a committed original fails with `ABORTED` (optimistic
  concurrency on `expected_aggregate_nonce`), so it cannot duplicate, but the
  client cannot tell its own write from a competing one without reading the
  stream **(drilled)**. Event ids are also unique per tenant.
- A retry sent while the original is still in flight inside Postgres (the
  original's connection outlived a client timeout or a process kill) waits on
  the original's row locks. If the original then commits, the retry fails
  with `ABORTED`; re-send it with the same key and it resolves to the
  recorded result. Treat `ABORTED` on a keyed retry as "retry again with the
  same key"; it becomes a real conflict only if it persists.
- The idempotency record is per `(tenant_id, aggregate_id, idempotency_key)`
  and committed in the same transaction as the events.

**Idempotency fingerprint and headers.** The fingerprint covers event
metadata, including `headers`. Before #355 the header map was hashed in hash
map iteration order, which differs between decodes, so an identical retry
with two or more headers was often refused with `ALREADY_EXISTS`. Headers
are now hashed in key order. Fingerprints for requests with zero or one
header are unchanged. Records written by older versions for requests with
two or more headers may still refuse a retry; confirm by reading the stream.

## Consumers: subscription failures and checkpoints

- A subscription never hides a failed query: the stream yields `UNAVAILABLE`
  naming the phase and `resume from global_nonce N`, then ends; a failed
  replay never sends the caught-up marker (ADR-026) **(drilled with real
  connection loss)**. An undecodable stored row yields `DATA_LOSS` (#351).
- Delivery is at least once and, within one subscription, in strictly
  increasing `global_nonce` order (every drill consumer asserts this). The
  consumer owns its checkpoint and resubscribes with
  `from_global_nonce = checkpoint + 1`.
- To make results idempotent, apply each event and advance the checkpoint in
  one transaction, and/or dedupe on `event_id`. The drill projection does
  both; with a checkpoint saved only every 7 events, redelivered events are
  skipped and the final state is exact **(drilled)**.
- `projection_checkpoints` (from the store migrations) is the place for
  checkpoints of projections that live in the event-store database. Keeping
  projection state and checkpoint in that database means one backup captures
  them consistently with the log.

## Backups

### What a backup must contain

Back up the **whole event-store database**, not selected tables:

| Object | Why |
|---|---|
| `events` | The log: ids, payload bytes, metadata, `global_nonce`, `aggregate_nonce`. |
| `aggregates` | Stream heads used for optimistic concurrency. |
| `idempotency` | Lets clients retry pre-backup requests safely after a restore. |
| `events_global_nonce_seq` | Next global position. Without it new appends reuse positions. |
| `projection_checkpoints` and co-located projection tables | Consumer positions consistent with the log. |
| `_sqlx_migrations` | Schema version; the server skips applied migrations on start. |
| Functions and triggers (`forbid_events_mutation`, `validate_aggregate_nonce`) | Append-only and nonce-contiguity enforcement. |

`pg_dump` of the database includes all of these.

### Taking a backup

```bash
# Logical, consistent snapshot (MVCC); safe while the event store is running.
pg_dump --format=custom --file=eventstore-$(date +%Y%m%dT%H%M%S).dump \
  "postgres://USER:PASSWORD@HOST:5432/DBNAME"
```

`pg_dump` reads one snapshot: appends committed after it started are not in
the backup. Because appends of one tenant commit in `global_nonce` order, the
snapshot never contains an event of a tenant without every earlier committed
event of that tenant. Store dumps
encrypted; payloads are application data.

Physical backups (`pg_basebackup` + WAL archiving, point-in-time recovery)
are the right tool for large stores and low RPO. They are standard Postgres
operations and are **not** exercised by the drill.

### Restoring

Always restore into a **fresh, empty** database (new instance or new
database), never over a live one:

```bash
createdb -h NEWHOST -U USER eventstore
pg_restore --exit-on-error --single-transaction --no-owner \
  -h NEWHOST -U USER -d eventstore eventstore-YYYYMMDDTHHMMSS.dump
```

Then point the event store at it (`BACKEND=postgres DATABASE_URL=...`) and
verify before sending traffic:

```sql
SELECT tenant_id, count(*), max(global_nonce) FROM events GROUP BY tenant_id;
SELECT last_value, is_called FROM events_global_nonce_seq;
SELECT count(*) FROM idempotency;
SELECT projection_name, global_position FROM projection_checkpoints;
SELECT tgname FROM pg_trigger WHERE tgrelid = 'events'::regclass AND NOT tgisinternal;
```

Compare with the same queries on the source (or values recorded at backup
time).

### What the drill verifies after restore (drilled)

Into a separate fresh container, against the source:

- Every row of `events`, `aggregates`, `idempotency`,
  `projection_checkpoints`, `_sqlx_migrations` and the drill's projection
  tables is identical, all columns (`to_jsonb` row comparison).
- `events_global_nonce_seq` state and the `events` triggers are identical;
  `UPDATE events` is still rejected as append-only.
- Starting the event store applies no migrations.
- `ReadAll` returns the same events in the same strict order (ids, metadata,
  revisions, exact payload bytes, historical `event_version`s).
- Identical retries of pre-backup keyed requests return the original acks;
  a changed request under an old key gets `ALREADY_EXISTS`.
- New appends continue after the highest restored `global_nonce`.
- A restored consumer checkpoint resumes exactly after its position (no
  duplicates, no gaps), including events appended after the restore.
- A projection rebuilt from scratch on the restored log equals the expected
  state and the projection state carried by the backup.

### Consumers after restoring an older backup

Restoring rewinds the log to the backup point. Events committed after the
backup are gone, and new appends reuse their `global_nonce` values (and, for
retried or re-issued commands, possibly their event ids and aggregate
nonces) with different content.

Consumers whose state and checkpoint live **inside** the restored database
are rewound consistently with it. For a consumer whose state lives
**outside** it, the checkpoint alone is not enough: the checkpoint may lag
the state it describes. Example: applied through position 104, checkpoint
saved at 98, restored head 100. The checkpoint looks safe, yet the
projection holds effects of the lost events 101 to 104, and a dedupe ledger
would skip new events that reuse their ids.

For each such consumer, compute its **applied high-water mark**: the highest
position whose effect is in its state (for example `max(global_nonce)` of
its dedupe ledger, or a position stored with every state write). Then:

- high-water mark `<=` restored head of its tenant: resume from its
  checkpoint;
- otherwise: rebuild its projection from zero (or from a projection
  snapshot taken no later than the backup), or reconcile it against the
  restored log. Do not resume from the checkpoint.

A consumer that records only a checkpoint and no applied position cannot
make this decision safely: rebuild it. **(drilled:**
`drill_backup::external_projection_ahead_of_restored_log_is_detected_and_rebuilt`
shows that checkpoint-only resume leaves lost effects and drops a new event
that reuses a lost event id, and that the high-water check plus rebuild
yields the exact state.**)**

## Historical event versions and upcasters

The store keeps `event_type`, `event_version`, `content_type`, headers and
payload bytes exactly as written, forever, and the restore preserves them
**(drilled with v1/v2 fixtures in
`event-store/eventstore-bin/tests/fixtures/historical-events.json`)**. The
store never transforms payloads. Interpreting an old version is the
consumer's job:

- **Retain every upcaster/handler for every version that exists in the
  log**, for as long as the log can be replayed (projection rebuild, new
  consumer, restore). Deleting a "legacy" upcaster makes domain replay of a
  restored or existing log fail.
- A consumer must stop on an event version it cannot interpret, without
  advancing its checkpoint, rather than skip it. The drill's consumer with a
  missing v1 upcaster stops at the first v1 event with its checkpoint before
  it **(drilled)**.

## Excluded failure cases (not certified by the drill)

- **Power loss / OS crash.** A process SIGKILL (event store or Postgres)
  leaves the OS page cache intact. The drill does not prove that data reached
  stable storage, nor that disks and controllers honor flushes. Durability
  under power loss depends on the settings above plus storage that honors
  `fsync`.
- Storage corruption, torn writes, filesystem or volume loss, or a lost
  Docker volume.
- Replication, failover, split brain, and physical backup/PITR.
- **Hanging connections.** The drill cuts connections (the client sees a
  reset or EOF), so failures surface within seconds. A path that silently
  drops packets (paused VM or container, firewall black hole) is not
  detected promptly: the store sets no statement timeout or TCP keepalive
  itself, so unless the database role or connection options set
  `statement_timeout`, an append or subscription query can hang until OS TCP
  timeouts.
- Restoring a dump into an older Postgres major version than it was taken
  from, or a partially restored dump (use `--exit-on-error
  --single-transaction`).
- Undecodable stored rows (covered separately by #351 tests).
- Cross-tenant ordering: positions are ordered per tenant only.
