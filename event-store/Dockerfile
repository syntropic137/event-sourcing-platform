# Multi-stage build for Event Store
# Stage 1: Build the Rust binary
FROM rust:1.82-slim as builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

# Install nightly toolchain for edition2024 support
RUN rustup toolchain install nightly && \
    rustup default nightly

# Create app directory
WORKDIR /app

# Copy event-sourcing SDK first (dependency)
COPY event-sourcing/ ./event-sourcing/

# Copy event-store
COPY event-store/ ./event-store/

# Build release binary
WORKDIR /app/event-store
RUN cargo build --release --bin eventstore-bin && \
    ls -la /app/event-store/target/release/ && \
    test -f /app/event-store/target/release/eventstore-bin

# Stage 2: Runtime image
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
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

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=40s --retries=3 \
  CMD [ "sh", "-c", "nc -z localhost 50051 || exit 1" ]

# Run the binary
ENTRYPOINT ["/usr/local/bin/eventstore"]
