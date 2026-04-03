# =============================================================================
# Multi-stage build for Event Store (with cargo-chef dependency caching)
# =============================================================================
#
# Stages:
#   1. planner  — generate cargo-chef recipe (dependency fingerprint)
#   2. cook     — pre-compile dependencies (cached when Cargo.toml/lock unchanged)
#   3. builder  — compile workspace crates only
#   4. runtime  — minimal image with just the binary
# =============================================================================

# ---------------------------------------------------------------------------
# Stage 1: Plan — extract dependency recipe
# ---------------------------------------------------------------------------
FROM rust:1.93-slim AS planner

RUN cargo install cargo-chef --locked

WORKDIR /app/event-store
COPY event-store/ /app/event-store/

RUN cargo chef prepare --recipe-path recipe.json

# ---------------------------------------------------------------------------
# Stage 2: Cook — compile dependencies (layer cached when deps unchanged)
# ---------------------------------------------------------------------------
FROM rust:1.93-slim AS cook

# Build dependencies (no libssl-dev — we use rustls, not OpenSSL)
RUN apt-get update && apt-get install -y \
    pkg-config \
    protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

RUN cargo install cargo-chef --locked

WORKDIR /app/event-store
COPY --from=planner /app/event-store/recipe.json recipe.json

# Cook dependencies with BuildKit cache mounts for cargo registry
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    cargo chef cook --release --recipe-path recipe.json

# ---------------------------------------------------------------------------
# Stage 3: Build — compile workspace crates (deps already compiled)
# ---------------------------------------------------------------------------
FROM cook AS builder

COPY event-store/ /app/event-store/

RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    cargo build --release --bin eventstore-bin && \
    test -f /app/event-store/target/release/eventstore-bin

# ---------------------------------------------------------------------------
# Stage 4: Runtime — minimal image
# ---------------------------------------------------------------------------
FROM debian:bookworm-slim

# Runtime dependencies (no libssl3 — we use rustls, not OpenSSL)
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -u 1000 eventstore

# Copy binary from builder
COPY --from=builder /app/event-store/target/release/eventstore-bin /usr/local/bin/eventstore

# Set ownership
RUN chown eventstore:eventstore /usr/local/bin/eventstore

# Switch to non-root user
USER eventstore

# Expose gRPC port
EXPOSE 50051

# Health check using bash /dev/tcp (no nc needed)
HEALTHCHECK --interval=30s --timeout=10s --start-period=40s --retries=3 \
  CMD [ "bash", "-c", "</dev/tcp/localhost/50051 || exit 1" ]

# Run the binary
ENTRYPOINT ["/usr/local/bin/eventstore"]
