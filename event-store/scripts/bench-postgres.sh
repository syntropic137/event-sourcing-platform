#!/usr/bin/env bash
# Live-Postgres throughput / tail-latency baseline (issue #354).
#
# Starts a fresh, pinned Postgres container with explicit durability and
# resource limits, builds the release server + bench, runs the bench against
# the real eventstore-bin gRPC server, and removes the container afterwards.
#
#   PROFILE=quick|full        scenario matrix (default quick, ~3 min; full ~20 min)
#   DURABILITY=durable|relaxed
#       durable: Postgres defaults (fsync=on, synchronous_commit=on,
#                full_page_writes=on). This is the baseline.
#       relaxed: synchronous_commit=off (fsync stays on), for comparison only.
#   PG_CPUS / PG_MEM          container limits (default 4 / 4g)
#   BENCH_PG_PORT             host port (default 55432)
#   OUT_DIR                   results directory (default bench-results/<ts>)
#
# Run from event-store/: `make bench-pg` or `make bench-pg-full`.
set -euo pipefail

cd "$(dirname "$0")/.."

# Pinned: postgres 17.10 (Debian), multi-arch index digest.
PG_IMAGE="${PG_IMAGE:-postgres:17.10@sha256:de1e13ca94377fa5a27aafd0e9fc200df9692b15152f0090fdf074074ea5e397}"
PROFILE="${PROFILE:-quick}"
DURABILITY="${DURABILITY:-durable}"
PG_CPUS="${PG_CPUS:-4}"
PG_MEM="${PG_MEM:-4g}"
PORT="${BENCH_PG_PORT:-55432}"
OUT_DIR="${OUT_DIR:-bench-results/$(date -u +%Y%m%dT%H%M%SZ)-${PROFILE}-${DURABILITY}}"
NAME="esp-bench-pg-$$"

case "$DURABILITY" in
  durable) DURABILITY_ARGS=() ;;
  relaxed) DURABILITY_ARGS=(-c synchronous_commit=off) ;;
  *) echo "DURABILITY must be durable or relaxed" >&2; exit 2 ;;
esac

command -v docker >/dev/null || { echo "docker is required" >&2; exit 2; }

echo "Building release server and bench ..."
cargo build --release -p eventstore-bin -p eventstore-bench

cleanup() {
  if docker inspect "$NAME" >/dev/null 2>&1 && ! docker rm -f "$NAME" >/dev/null; then
    echo "WARNING: failed to remove container $NAME; remove it with: docker rm -f $NAME" >&2
  fi
}
trap cleanup EXIT

echo "Starting $PG_IMAGE as $NAME (cpus=$PG_CPUS mem=$PG_MEM durability=$DURABILITY) ..."
docker run -d --name "$NAME" \
  --cpus "$PG_CPUS" --memory "$PG_MEM" --shm-size 1g \
  -e POSTGRES_USER=bench -e POSTGRES_PASSWORD=bench -e POSTGRES_DB=bench \
  -p "127.0.0.1:${PORT}:5432" \
  "$PG_IMAGE" \
  postgres -c shared_buffers=1GB -c max_connections=200 -c max_wal_size=4GB \
  ${DURABILITY_ARGS[@]+"${DURABILITY_ARGS[@]}"} >/dev/null

# The image's init server listens on the unix socket only, so a TCP probe
# succeeds only once the final server is up.
for _ in $(seq 1 120); do
  if docker exec "$NAME" pg_isready -h 127.0.0.1 -U bench -d bench >/dev/null 2>&1; then
    break
  fi
  sleep 0.5
done
docker exec "$NAME" pg_isready -h 127.0.0.1 -U bench -d bench >/dev/null

mkdir -p "$OUT_DIR"
./target/release/eventstore-bench \
  --database-url "postgres://bench:bench@127.0.0.1:${PORT}/bench" \
  --server-bin ./target/release/eventstore-bin \
  --pg-container "$NAME" \
  --profile "$PROFILE" \
  --durability "$DURABILITY" \
  --out-dir "$OUT_DIR"
