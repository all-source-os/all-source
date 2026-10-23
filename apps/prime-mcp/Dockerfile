# syntax=docker/dockerfile:1.7
# =============================================================================
# AllSource Prime — Multi-Stage Dockerfile
#
# Build from repo root (required for workspace dependencies):
#   docker build -t allsource-prime -f apps/prime-mcp/Dockerfile .
#
# Run:
#   docker run -v prime-data:/data -p 3905:3905 allsource-prime
# =============================================================================

ARG RUST_VERSION=1.92

# =============================================================================
# Stage 1: Builder
# =============================================================================
FROM rust:${RUST_VERSION}-slim AS builder-source

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev cmake g++ \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Copy workspace root manifest and lockfile
COPY Cargo.toml Cargo.lock ./

# Copy allsource-core (the main dependency)
COPY apps/core/ apps/core/

# Copy shared crates allsource-core depends on (e.g. allsource-prime-models).
COPY crates/ crates/

# Copy prime-mcp itself
COPY apps/prime-mcp/ apps/prime-mcp/

# Stub workspace members that prime-mcp doesn't need but Cargo.toml references
RUN mkdir -p apps/mcp-server-elixir/native/core_nif/src \
    && echo '' > apps/mcp-server-elixir/native/core_nif/src/lib.rs \
    && printf '[package]\nname = "core_nif"\nversion = "0.1.0"\nedition = "2024"\n' \
       > apps/mcp-server-elixir/native/core_nif/Cargo.toml \
    && mkdir -p sdks/rust/src \
    && echo '' > sdks/rust/src/lib.rs \
    && printf '[package]\nname = "allsource"\nversion = "0.16.0"\nedition = "2024"\n' \
       > sdks/rust/Cargo.toml \
    && mkdir -p tooling/performance/src \
    && echo 'fn main() {}' > tooling/performance/src/main.rs \
    && printf '[package]\nname = "allsource-performance"\nversion = "0.1.0"\nedition = "2024"\n[[bin]]\nname = "allsource-performance"\npath = "src/main.rs"\n' \
       > tooling/performance/Cargo.toml

WORKDIR /build/apps/prime-mcp

FROM builder-source AS builder-image
RUN timeout -k 15 260 cargo build --release --locked -p image --lib

FROM builder-image AS builder-parquet
RUN timeout -k 15 260 cargo build --release --locked -p parquet --lib

FROM builder-parquet AS builder-core
RUN timeout -k 15 260 cargo build --release --locked -p allsource-core --lib

FROM builder-core AS builder
RUN timeout -k 15 260 cargo build --release --locked

# =============================================================================
# Stage 2: Runtime
# =============================================================================
FROM rust:${RUST_VERSION}-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

RUN groupadd -r prime && useradd -r -g prime -d /data prime

COPY --from=builder /build/apps/prime-mcp/target/release/allsource-prime /usr/local/bin/

RUN mkdir -p /data && chown prime:prime /data
VOLUME /data

USER prime

ENV PRIME_DATA_DIR=/data \
    PRIME_MODE=http \
    PRIME_PORT=3905 \
    PRIME_LOG_LEVEL=info

EXPOSE 3905

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:3905/health || exit 1

ENTRYPOINT ["allsource-prime"]
CMD ["--data-dir", "/data", "--mode", "http", "--port", "3905"]
