# Postgres Event Store: Throughput and Tail-Latency Baseline

Issue: [#354](https://github.com/syntropic137/event-sourcing-platform/issues/354). Related: #343.

This is the measured baseline for the Postgres backend **before** any
optimization or regression threshold is set. It measures the store as it ships:
the real `eventstore-bin` gRPC server, its production pool settings, and a
real Postgres with default (durable) commit settings.

Appends take a per-tenant transaction advisory lock so that commit order equals
`global_nonce` order (see `store_postgres.rs`, `it_commit_order.rs`). That lock
is a correctness guarantee. These numbers show what it costs. They are **not**
a reason to remove it: any alternative must keep the commit-order tests and the
paged-replay guarantees.

## TL;DR (full profile, durable, Apple M3 Max, Docker Desktop)

| Path | Sustained | Tail |
|---|---|---|
| Append, 1 tenant, batch 1 | ~560 ev/s (1 writer), ~900 ev/s ceiling (16 to 32 writers) | p99 4 ms (1 writer), 37 ms (16 writers, closed loop) |
| Append, 1 tenant, batch 1, **open loop** | stable at 226 ev/s (25% of ceiling) | p99 32 ms at 25%; 0.8 s at 50%; 2.5 s at 75%; 7.4 s at 90% |
| Append, many tenants, batch 1 | ~1,290 ev/s (16 to 32 tenants) | p99 27 ms (16 writers) |
| Append, 1 tenant, batch 10 / 100 | ~2,800 / ~3,400 ev/s | p99 125 ms / 853 ms per request |
| Replay, paged `ReadAll` (1000/page) | 70k to 145k ev/s | page p99 11 to 68 ms |
| Rehydrate a 100-event aggregate (`ReadStream`) | 1,000 to 2,000 aggregates/s (8 readers) | p99 11 ms (100k history), 48 ms (1M) |
| Subscription catch-up from 0 | ~230k ev/s | **1M events: 3.6 s to first event, ~1.9 GB server RSS** |
| End-to-end delivery, 250 ev/s, 1 / 8 / 32 subscribers | 250 ev/s each | delivery p99 13 / 59 / 297 ms |

Every scenario passed completeness and ordering verification (0 missing, 0
unexpected, 0 order violations, every subscriber's sequence identical to the
store's committed order).

## How to run

Requires Docker and a Rust toolchain. From `event-store/`:

```bash
make bench-pg                          # quick profile, ~2 min
make bench-pg-full                     # full profile, ~16 min (1M-event preload dominates)
make bench-pg DURABILITY=relaxed       # synchronous_commit=off, comparison only
make bench-pg PG_CPUS=2 PG_MEM=2g      # different container limits
```

Each run:

1. Builds `eventstore-bin` and `eventstore-bench` in release mode.
2. Starts a fresh container from a pinned image (below) with fixed CPU/memory
   limits; removes it on exit.
3. Spawns the real `eventstore-bin` (`BACKEND=postgres`, `RUST_LOG` unset, so
   only errors are logged), tagging its DB connections with an
   `application_name` so the sampler can find them.
4. Runs the scenario matrix, verifies every scenario, writes
   `bench-results/<timestamp>-<profile>-<durability>/{results.json,summary.md,server.log}`,
   prints the summary, and exits non-zero if any verification failed.

There is also a manual GitHub Actions workflow, `Bench • Postgres baseline`
(`.github/workflows/bench-postgres.yml`, `workflow_dispatch` only). Shared
runners are noisy, so use it to smoke-test the harness or spot gross
regressions, not to publish baselines. The bench is deliberately **not** part
of `make qa` or CI.

Direct invocation against an existing database (tenants are namespaced per
run, so reuse is safe, but table size then differs from a fresh run):

```bash
cargo build --release -p eventstore-bin -p eventstore-bench
./target/release/eventstore-bench --database-url postgres://u:p@host:5432/db \
  --server-bin ./target/release/eventstore-bin \
  [--pg-container NAME] [--profile quick|full] [--durability LABEL] [--out-dir DIR]
```

## Pinned versions and configuration

| Item | Value |
|---|---|
| Postgres image | `postgres:17.10@sha256:de1e13ca94377fa5a27aafd0e9fc200df9692b15152f0090fdf074074ea5e397` (17.10, Debian) |
| Container limits | `--cpus 4 --memory 4g --shm-size 1g` |
| Postgres flags | `shared_buffers=1GB max_connections=200 max_wal_size=4GB`; everything else default |
| Durability (baseline) | `fsync=on`, `synchronous_commit=on`, `full_page_writes=on`, `wal_sync_method=fdatasync` |
| Durability (`relaxed`) | as above plus `synchronous_commit=off` |
| Server | `eventstore-bin` from this repo, release build; `PostgresStore::connect` pool: `max_connections=5`, `acquire_timeout=30s` |
| Client | `eventstore-bench`, tonic 0.14 over loopback TCP, one HTTP/2 connection per worker |
| Toolchain | rustc 1.97.1 |
| Baseline host | Apple M3 Max, 16 logical CPUs, 128 GB, macOS (Darwin 25.6.0 arm64); Docker Desktop 29.8.0 VM with 16 CPUs / 15.6 GB |
| Code | store and server at `d6034f0f83ba` (origin/main; this PR does not change them); harness as of commit `bffecf6` on this PR's branch (the run's JSON records the base SHA because the harness was not yet committed). Reproduce with this PR's head: later harness commits only change how overloaded open-loop runs are accounted (see the footnote under the results) and add run metadata. |

Client, server and the Docker VM share one machine. Postgres I/O goes through
the Docker Desktop VM disk, so commit (fsync) latency is **not** representative
of a cloud volume. Treat absolute numbers as this-machine numbers; the ratios
(same vs different tenant, batch scaling, subscribers) transfer better.

## Methodology

### Workload

- **Events**: one aggregate type, binary payload (first 8 bytes carry the
  intended send time), no idempotency key. Each writer owns its aggregates and
  appends with the correct expected nonce, rolling to a new aggregate every 100
  events, so there are no OCC conflicts by design.
- **Append, closed loop**: each writer sends its next append when the previous
  returns. This finds the throughput ceiling. Its latency is service time at
  saturation; it hides queueing (coordinated omission), so it is not the tail a
  caller would see at a given arrival rate.
- **Append, open loop**: a dispatcher emits requests on a fixed schedule
  (25/50/75/90% of the measured single-tenant ceiling) regardless of
  completions; 64 lanes (32 in quick) pick them up. Latency is measured from the
  **intended** send time, so backlog shows up in the percentiles
  (coordinated-omission corrected). `service` latency (send to response) is
  also kept in `results.json`. Timer granularity is about 1 ms, which bounds the
  dispatcher's own error.
- **History**: a single tenant is preloaded to 10k, 100k and 1M events
  (16 writers, batch 100, 256 B). At each size: three paged `ReadAll` scans,
  closed-loop `ReadStream` of whole 100-event aggregates by 8 readers, a
  subscription catch-up from global nonce 0, and the reference append
  scenario again (table size grows with history).
- **End-to-end**: subscribers attach to an empty tenant and wait for the
  caught-up marker; 32 open-loop lanes then append at a fixed rate (250 and 500
  ev/s). Delivery latency = subscriber receive time minus the event's intended
  send time (same process, monotonic clock). A slow-subscriber variant runs 2
  normal subscribers next to one that sleeps 10 ms per event (100 ev/s
  capacity vs 250 ev/s offered).
- **Warmup and windows**: each loaded scenario has a warmup (1 s quick, 3 s
  full) whose samples are discarded, then a measurement window (5 s quick, 15 s
  full). Throughput = events acknowledged for requests started in the window /
  window length.
- **Percentiles**: HDR histograms (microsecond resolution, 3 significant
  digits) over every sample; per-worker histograms are merged, never averaged.

### What is sampled

- **Pool and lock waits**: every 20 ms, `pg_stat_activity` filtered to the
  server's `application_name`: busy connections (not `idle`), `idle in
  transaction`, backends waiting on `Lock/advisory` (the ordering lock), other
  lock waits, and `IO/Wal*` waits; plus ungranted advisory locks from
  `pg_locks`. sqlx does not expose acquire waits, so **"pool busy %" (all 5
  pool connections checked out)** is the pool-wait proxy: a request arriving
  then has to wait for a connection.
- **CPU and memory**: every 500 ms, `ps` for the server and bench processes,
  and cgroup v2 `cpu.stat` / `memory.current` inside the Postgres container
  (memory includes page cache). CPU% is averaged over the window (100 = one
  core). **RSS columns are high-water marks**: the allocator keeps memory, so a
  scenario after a memory-heavy one (for example after verifying the 64 KiB
  payload run, or after the 1M catch-up) inherits its RSS.

### Correctness under load

After every loaded scenario the bench reads each touched tenant back through
`ReadAll` and checks:

- every acknowledged event is present (`missing_acked = 0`);
- nothing is present that no writer sent (`unexpected = 0`; events of appends
  that returned an error are tolerated and counted separately);
- `global_nonce` strictly increases in tenant read order;
- every aggregate's nonces are exactly `1..n` in order.

For catch-up and end-to-end runs, every subscriber's delivered `global_nonce`
sequence must be **identical** to the store's committed sequence for the
tenant (no gaps, duplicates or reordering). `ReadStream` responses must contain
exactly 100 contiguous events. Any failure fails the run.

## Run: profile `full`, durability `durable`, 931s, all checks: **pass**

- Host: Apple M3 Max (16 logical CPUs, 128.0 GB), Darwin 25.6.0 arm64
- Docker 29.8.0 VM: 16 CPUs, 15.6 GB; Postgres container `postgres:17.10@sha256:de1e13ca94377fa5a27aafd0e9fc200df9692b15152f0090fdf074074ea5e397`: 4 CPUs, 4.0 GB
- Postgres 17.10 (Debian 17.10-1.pgdg13+1): fsync=on synchronous_commit=on full_page_writes=on wal_sync_method=fdatasync shared_buffers=1GB max_connections=200
- Server pool max 5 connections; git d6034f0f83ba; rustc 1.97.1 (8bab26f4f 2026-07-14)

### Append

Closed loop: latency is service time at saturation. Open loop: latency from intended send (coordinated-omission corrected). CPU% 100 = one core. Pool busy = % of samples with all 5 pool connections checked out. Adv wait = backends waiting on the per-tenant ordering lock (mean / max).

| scenario | writers | tenants | batch | payload B | mode | events/s | req/s | p50 ms | p95 ms | p99 ms | p99.9 ms | max ms | err | pool busy % | adv wait | WAL wait | srv CPU / pg CPU / srv RSS MB | DB events before | verified |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| append-w1-same-b1-p256 | 1 | 1 | 1 | 256 | closed | 558 | 558 | 1.63 | 2.70 | 3.88 | 7.89 | 26.9 | 0 | 0 | 0.00 / 0 | 0.18 | 13 / 21 / 10 | 0 | pass |
| append-w4-same-b1-p256 | 4 | 1 | 1 | 256 | closed | 675 | 675 | 5.03 | 11.34 | 16.53 | 39.13 | 221.4 | 0 | 0 | 2.06 / 3 | 0.07 | 21 / 33 / 14 | 9710 | pass |
| append-w4-diff-b1-p256 | 4 | 4 | 1 | 256 | closed | 1055 | 1055 | 3.25 | 6.54 | 9.78 | 27.36 | 42.5 | 0 | 0 | 0.00 / 0 | 0.35 | 39 / 60 / 15 | 22126 | pass |
| append-w16-same-b1-p256 | 16 | 1 | 1 | 256 | closed | 905 | 905 | 15.95 | 28.00 | 37.28 | 93.76 | 104.9 | 0 | 3 | 3.00 / 4 | 0.01 | 22 / 34 / 15 | 40882 | pass |
| append-w16-diff-b1-p256 | 16 | 16 | 1 | 256 | closed | 1290 | 1290 | 10.82 | 20.32 | 27.41 | 39.71 | 42.8 | 0 | 8 | 0.00 / 0 | 0.37 | 43 / 66 / 16 | 56726 | pass |
| append-w32-same-b1-p256 | 32 | 1 | 1 | 256 | closed | 876 | 876 | 33.05 | 54.14 | 88.51 | 116.54 | 123.7 | 0 | 4 | 3.00 / 4 | 0.02 | 22 / 34 / 17 | 80812 | pass |
| append-w32-diff-b1-p256 | 32 | 32 | 1 | 256 | closed | 1291 | 1291 | 21.73 | 40.06 | 50.37 | 85.06 | 92.6 | 0 | 7 | 0.00 / 0 | 0.32 | 44 / 66 / 17 | 96000 | pass |
| append-w16-same-b10-p256 | 16 | 1 | 10 | 256 | closed | 2796 | 280 | 49.57 | 107.58 | 125.18 | 147.20 | 149.6 | 0 | 61 | 3.60 / 4 | 0.09 | 14 / 32 / 18 | 119576 | pass |
| append-w16-same-b100-p256 | 16 | 1 | 100 | 256 | closed | 3420 | 34 | 429.82 | 713.73 | 852.99 | 870.91 | 870.9 | 0 | 92 | 3.90 / 4 | 0.02 | 9 / 27 / 19 | 171116 | pass |
| append-w16-same-b1-p4096 | 16 | 1 | 1 | 4096 | closed | 721 | 721 | 18.14 | 46.94 | 58.14 | 68.22 | 72.5 | 0 | 9 | 3.03 / 4 | 0.04 | 21 / 34 / 19 | 232416 | pass |
| append-w16-same-b1-p65536 | 16 | 1 | 1 | 65536 | closed | 515 | 515 | 28.27 | 54.08 | 67.90 | 112.58 | 114.5 | 0 | 27 | 3.11 / 4 | 0.09 | 28 / 39 / 87 | 245876 | pass |
| append-open-25pct-same-b1-p256 | 64 | 1 | 1 | 256 | open 226 rps | 226 | 226 | 3.76 | 9.68 | 32.29 | 42.27 | 44.3 | 0 | 1 | 0.10 / 3 | 0.04 | 8 / 13 / 884 | 255484 | pass |
| append-open-50pct-same-b1-p256 | 64 | 1 | 1 | 256 | open 452 rps | 452 | 452 | 3.75 | 711.17 | 842.75 | 865.28 | 865.8 | 0 | 8 | 1.04 / 4 | 0.14 | 15 / 25 / 885 | 259556 | pass |
| append-open-75pct-same-b1-p256 | 64 | 1 | 1 | 256 | open 679 rps | 571 [1] | 571 [1] | 723.97 | 1453.06 | 2527.23 | 2813.95 | 2840.6 | 0 | 12 | 2.90 / 4 | 0.07 | 21 / 35 / 885 | 267699 | pass |
| append-open-90pct-same-b1-p256 | 64 | 1 | 1 | 256 | open 814 rps | 546 [1] | 546 [1] | 2541.57 | 7348.22 | 7421.95 | 7430.14 | 7430.1 | 0 | 12 | 3.01 / 4 | 0.07 | 19 / 31 / 770 | 279913 | pass |
| append-w16-same-b1-p256-at-hist10000 | 16 | 1 | 1 | 256 | closed | 656 | 656 | 19.79 | 51.10 | 68.54 | 82.17 | 84.2 | 0 | 11 | 3.02 / 4 | 0.06 | 21 / 36 / 243 | 304570 | pass |
| append-w16-same-b1-p256-at-hist100000 | 16 | 1 | 1 | 256 | closed | 752 | 752 | 18.30 | 37.18 | 58.69 | 66.50 | 68.1 | 0 | 6 | 3.01 / 4 | 0.03 | 21 / 35 / 385 | 406974 | pass |
| append-w16-same-b1-p256-at-hist1000000 | 16 | 1 | 1 | 256 | closed | 408 | 408 | 21.49 | 121.53 | 431.87 | 660.48 | 704.0 | 0 | 20 | 3.01 / 4 | 0.09 | 15 / 29 / 1869 | 1319624 | pass |

[1] Corrected after review. This run divided events scheduled in the 15 s
window by 15 s, which reports the offered rate when a backlog drains after the
window (2.8 s at 75%, 7.3 s at 90%). The values shown are achieved throughput,
`events / (15 s + drain)`, which is what the harness now reports. Their
CPU/pool columns still include the drain period; the current harness stops
sampling at the window end. All other rows drained within a few ms and are
unaffected.

### History preload

| tenant total | added | writers | batch | payload B | secs | events/s | errors |
|---|---|---|---|---|---|---|---|
| 10000 | 10000 | 16 | 100 | 256 | 3.8 | 2628 | 0 |
| 100000 | 90000 | 16 | 100 | 256 | 28.9 | 3115 | 0 |
| 1000000 | 900000 | 16 | 100 | 256 | 294.3 | 3058 | 0 |

### Replay: paged ReadAll (page 1000)

| history | reps | events/s (median scan) | page p50 ms | page p95 ms | page p99 ms | page max ms | srv CPU / pg CPU / srv RSS MB | verified |
|---|---|---|---|---|---|---|---|---|
| 10000 | 3 | 119021 | 7.97 | 9.89 | 11.12 | 11.1 | 31 / 41 / 230 | pass |
| 100000 | 3 | 70826 | 13.51 | 21.57 | 67.78 | 105.0 | 15 / 76 / 243 | pass |
| 1000000 | 3 | 145747 | 5.54 | 18.30 | 42.53 | 118.7 | 33 / 28 / 386 | pass |

### Replay: aggregate rehydration (ReadStream, 100 events)

| history | readers | rehydrations/s | events/s | p50 ms | p95 ms | p99 ms | max ms | pool busy % | srv CPU / pg CPU / srv RSS MB | invalid |
|---|---|---|---|---|---|---|---|---|---|---|
| 10000 | 8 | 1872 | 187200 | 3.71 | 7.36 | 13.08 | 41.9 | 0 | 82 / 59 / 231 | 0 |
| 100000 | 8 | 1974 | 197380 | 3.58 | 6.73 | 11.09 | 64.2 | 0 | 83 / 61 / 243 | 0 |
| 1000000 | 8 | 1026 | 102573 | 5.14 | 20.80 | 48.29 | 247.6 | 0 | 45 / 44 / 386 | 0 |

### Subscription catch-up from 0

| history | first event ms | caught up ms | events/s | srv CPU / pg CPU / srv RSS MB | exact order |
|---|---|---|---|---|---|
| 10000 | 31 | 41 | 245922 | - / - / 231 | pass |
| 100000 | 371 | 443 | 225513 | 54 / 36 / 385 | pass |
| 1000000 | 3592 | 4318 | 231612 | 56 / 27 / 1938 | pass |

### End-to-end subscription delivery

Delivery latency = receive minus intended append send. Slow subscribers sleep per event.

| scenario | subs (fast+slow) | target ev/s | appended ev/s | append p99 ms | deliver p50 ms | p95 ms | p99 ms | max ms | per-sub ev/s | max lag fast / slow | drain ms fast / slow | slow p99 ms | pool busy % | srv CPU / pg CPU / srv RSS MB | exact order |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| e2e-250eps-1subs | 1+0 | 250 | 250 | 11.95 | 4.28 | 6.91 | 13.15 | 34.2 | 250 | 1 / - | 1 / - | - | 0 | 10 / 17 / 1869 | pass |
| e2e-250eps-8subs | 8+0 | 250 | 250 | 44.29 | 4.53 | 29.76 | 59.36 | 90.8 | 250 | 7 / - | 2 / - | - | 0 | 28 / 39 / 1869 | pass |
| e2e-250eps-32subs | 32+0 | 250 | 250 | 234.75 | 23.97 | 205.95 | 297.21 | 356.9 | 250 | 29 / - | 7 / - | - | 2 | 45 / 61 / 1511 | pass |
| e2e-500eps-1subs | 1+0 | 500 | 500 | 468.22 | 4.92 | 379.90 | 497.92 | 542.2 | 500 | 30 / - | 1 / - | - | 5 | 20 / 33 / 1511 | pass |
| e2e-500eps-8subs | 8+0 | 500 | 500 | 8237.06 | 189.69 | 8228.86 | 8273.92 | 8310.8 | 500 | 31 / - | 2 / - | - | 15 | 22 / 47 / 1511 | pass |
| e2e-500eps-32subs | 32+0 | 500 | 500 | 1070.08 | 271.62 | 990.21 | 1097.73 | 1147.9 | 500 | 33 / - | 7 / - | - | 5 | 36 / 86 / 1512 | pass |
| e2e-250eps-slow | 2+1 | 250 | 250 | 33.79 | 4.06 | 20.69 | 43.94 | 94.0 | 250 | 4 / 2887 | 2 / 35382 | 35127.3 | 1 | 16 / 26 / 1512 | pass |

The JSON for this run, with every field (service latency, sampler detail,
per-subscriber sequence checks), is in
[`baselines/2026-10-06-m3max-full-durable.json`](baselines/2026-10-06-m3max-full-durable.json).

## Repeatability and durability

Quick-profile runs on the same machine, same day. A, B and the first relaxed
run were taken around the full baseline. C and D were taken later while other
work (dozens of test Postgres containers and Rust builds from parallel
sessions) was loading the host; host load average was 17 to 80.

| scenario (quick) | A durable | B durable | relaxed | C durable, loaded | D durable, loaded |
|---|---|---|---|---|---|
| w1 same tenant, ev/s / p99 | 364 / 9.9 ms | 580 / 3.4 ms | 466 / 4.3 ms | 479 / 3.9 ms | 322 / 9.9 ms |
| w8 same tenant, ev/s / p99 | 872 / 21 ms | 894 / 19 ms | 834 / 16 ms | 428 / 50 ms | 491 / 86 ms |
| w8 different tenants, ev/s / p99 | 1,301 / 11 ms | 1,503 / 10 ms | 1,286 / 12 ms | 717 / 24 ms | 722 / 51 ms |
| w8 batch 100, ev/s | 3,800 | 3,840 | 3,000 | 3,296 | 2,766 |
| open loop 50%, p99 | 17 ms | 45 ms | 2,726 ms | 39 ms | 30 ms |
| e2e 250 ev/s, 8 subs, delivery p99 | n/a (ran at 500 ev/s) | 19 ms | 10 ms | 454 ms | 228 ms |

A second full-profile run under that load was **discarded**: single-writer
appends fell to 95 ev/s, the 1M preload took 41 minutes instead of 5, and 4
preload appends errored, which (correctly) failed the run. Completeness and
ordering still passed in every scenario of every run.

Takeaways:

- **Run-to-run spread is large** on Docker Desktop even on a quiet host: up to
  ~1.6x on single-writer throughput and an order of magnitude on open-loop
  tails near 50% load. A loaded host halves multi-writer throughput.
- **The same/different-tenant ratio is stable** (same-tenant throughput is 0.59 to 0.70 of different-tenant across all runs), as
  is batch scaling. Those are the better regression signals.
- **Relaxing commit durability does not raise the ceiling.** WAL flush is not
  the bottleneck here (WAL-wait samples are ~0.1 backends on average). The
  limit is the serialized critical section plus the 5-connection pool.
- Each run now records `uptime` at start (`host_load_at_start`), so loaded runs
  can be spotted and discarded.

## Findings

1. **The ordering lock costs about 30% of multi-writer append throughput.**
   Same tenant tops out at ~900 ev/s; spreading the same writers over separate
   tenants gives ~1,290 ev/s. With one tenant, a mean of ~3 backends wait on
   the advisory lock at all times, i.e. every pool connection except the
   holder. Adding writers past ~16 only adds latency (p99 37 ms at 16, 89 ms at
   32) without throughput.
2. **The critical section is round-trip bound, not fsync bound.** The lock is
   held from before the first INSERT to COMMIT, and each event is a separate
   INSERT round trip, followed by the aggregate upsert and `pg_notify`. Batching
   amortizes the commit but not the per-event INSERTs: batch 10 gives ~3x and
   batch 100 ~3.8x throughput, at 125 ms and 853 ms request p99. `idle in
   transaction` is ~0.5 to 0.9 backends on average, i.e. the lock holder is
   often waiting on the server, not on Postgres.
3. **The pool (5 connections) caps concurrency below the lock.** Other tenants'
   appends, subscriber polls and reads share the same 5 connections, so a busy
   tenant's lock queue also delays unrelated tenants and subscribers. Pool busy
   reaches 61% at batch 10 and 92% at batch 100.
4. **Open-loop tails collapse well before the closed-loop ceiling.** At 25% of
   the single-tenant ceiling p99 is 32 ms; at 50% it is 0.8 s in the full run
   (17 to 45 ms in two quick runs, 2.7 s in the relaxed run); at 75% and 90%
   backlog builds and drains seconds after load stops. A short stall (Docker
   disk, checkpoint) at that utilization queues enough work behind the lock
   and pool that recovery takes seconds. This is the main input to the budget.
5. **Subscription catch-up buffers the whole history.** `subscribe()` loads
   every matching row before yielding the first event. At 1M events: 3.6 s to
   first event and ~1.9 GB server RSS (from ~390 MB), which stays allocated.
   Throughput once streaming is fine (~230k ev/s). (`subscribe()` is being
   reworked separately; rerun this bench on that change.)
6. **Live delivery cost grows with subscribers.** Each subscriber runs its own
   query per wake-up on the shared pool. At 250 ev/s, delivery p99 goes from
   13 ms (1 subscriber) to 59 ms (8) to 297 ms (32), and append p99 rises with
   it (12, 44, 235 ms). At 500 ev/s the appends themselves back up and
   delivery p99 is dominated by append queueing (0.5 to 8 s), while subscriber
   lag stays at ~30 events.
7. **A slow subscriber is isolated, and delivery stays complete.** A subscriber
   consuming at 100 ev/s behind 250 ev/s fell 2,887 events behind and needed
   35 s after the writers stopped to drain, receiving every event in order. The
   fast subscribers next to it kept p99 44 ms. Server memory did not grow with
   its lag (HTTP/2 flow control backpressures the server stream).
8. **History size mostly affects tails.** Append at 10k / 100k / 1M history:
   656 / 752 / 408 ev/s, p99 69 / 59 / 432 ms (the 1M run follows a 300 s
   preload, so checkpoint and autovacuum activity overlap it). `ReadStream` p99
   goes from 11 to 48 ms between 100k and 1M.
9. **Completeness and ordering held everywhere**, including the overloaded
   open-loop and 32-subscriber runs.

## Capacity budget (DreamShip-relevant)

DreamShip consumes the store through its ESP adapter (per-tenant appends,
paged `ReadAll` replay, live subscriptions; see #343). Until DreamShip's own
production rates are known, budget per **server instance** (pool of 5) on
hardware comparable to the baseline. Plan on the left column. The right column
is where tails stop being predictable.

| Resource | Plan for (sustained) | Do not exceed | Basis |
|---|---|---|---|
| Single-tenant appends, 1 event each | **≤ 200 ev/s** (p99 < ~35 ms) | ~450 ev/s (50% of ceiling) | findings 1, 4 |
| Single-tenant appends, batched (10/request) | ≤ 1,000 ev/s | ~2,800 ev/s | finding 2 |
| All tenants combined, 1 event each | ≤ 600 ev/s | ~1,300 ev/s | findings 1, 3 |
| Live subscribers per tenant at ≤ 250 ev/s | ≤ 8 (delivery p99 < ~60 ms) | 32 (p99 ~300 ms, slows appends) | finding 6 |
| Subscriber catch-up from 0 | ≤ 100k events per catch-up (< 0.5 s, < 400 MB RSS) | 1M (3.6 s stall, ~1.9 GB RSS per concurrent catch-up) | finding 5 |
| Projection rebuild via paged `ReadAll` | ~70k ev/s, i.e. ~15 s per 1M events | n/a | replay table |
| Aggregate rehydration (100 events) | ≤ 1,000/s | ~2,000/s | `ReadStream` table |
| Slow consumers | lag is safe; drain time = backlog / consume rate | n/a | finding 7 |

If a DreamShip tenant needs more than ~200 single-event appends per second,
the levers, in order, are: batch events per command; spread load over tenants;
raise the server pool size (a config change, needs its own measurement); and
only then reduce the time the lock is held (for example, one multi-row INSERT
per batch), keeping `it_commit_order.rs` green. Removing the lock is out of
scope.

## Before setting regression thresholds

Do not derive thresholds from this single machine yet:

- Run the full profile at least 3 times on a dedicated Linux host (no Docker
  Desktop VM disk) and take the median of each metric; thresholds should sit
  outside the observed spread.
- Gate on ratios that are stable here (same/different tenant throughput,
  batch scaling, delivery p99 vs subscriber count) and on verification
  (must always pass), rather than on absolute ev/s.
- Re-baseline whenever the pool size, the append transaction or
  `subscribe()` changes.

## Limitations

- One machine; client, server and Postgres share CPUs. Bench CPU is reported
  (`bench_cpu_pct` in JSON) and stayed well below one core.
- Only one event type and payload shape; no idempotency keys; no OCC conflicts.
- Resource sampling is 2 Hz, so very short scenarios (10k catch-up) report no
  CPU figure.
- Pool waits are inferred from connection occupancy, not measured inside sqlx.
