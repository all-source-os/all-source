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

# Dependency layers depend only on manifests, lockfiles and the shared crates,
# so a source change in Core or Prime does not recompile the dependency graph.
COPY Cargo.toml Cargo.lock ./
COPY apps/core/Cargo.toml apps/core/
COPY crates/ crates/
COPY sdks/rust/ sdks/rust/
COPY apps/prime-mcp/Cargo.toml apps/prime-mcp/Cargo.lock apps/prime-mcp/

# Cargo refuses a manifest whose declared targets are missing, so each target
# gets an empty stand-in until the real sources are copied. Only default
# targets a crate really has are stubbed: an extra one would become a target.
RUN for spec in "apps/core src/lib.rs src/main.rs" "apps/prime-mcp src/main.rs"; do \
        set -- $spec; dir=$1; shift; manifest="$dir/Cargo.toml"; \
        for target in "$@" \
            $(grep -oE '^path = "[^"]+\.rs"' "$manifest" | cut -d'"' -f2) \
            $(grep -A1 -E '^\[\[bench\]\]' "$manifest" | grep -oE '^name = "[^"]+"' | cut -d'"' -f2 | sed 's|.*|benches/&.rs|'); do \
            mkdir -p "$dir/$(dirname "$target")"; \
            echo 'fn main() {}' > "$dir/$target"; \
        done; \
    done

# Stub workspace members that prime-mcp doesn't need but Cargo.toml references
RUN mkdir -p apps/mcp-server-elixir/native/core_nif/src \
    && echo '' > apps/mcp-server-elixir/native/core_nif/src/lib.rs \
    && printf '[package]\nname = "core_nif"\nversion = "0.1.0"\nedition = "2024"\n' \
       > apps/mcp-server-elixir/native/core_nif/Cargo.toml \
    && mkdir -p tooling/performance/src \
    && echo 'fn main() {}' > tooling/performance/src/main.rs \
    && printf '[package]\nname = "allsource-performance"\nversion = "0.1.0"\nedition = "2024"\n[[bin]]\nname = "allsource-performance"\npath = "src/main.rs"\n' \
       > tooling/performance/Cargo.toml

WORKDIR /build/apps/prime-mcp

# Each phase fits one 5-minute CI step. Exit 124 keeps the finished units for
# the next phase to resume; any other failure stops the build.
FROM builder-source AS builder-deps-warm
RUN timeout -k 15 230 cargo build --release --locked; \
    status=$?; \
    if [ "$status" -eq 124 ]; then echo "dependency build paused at its deadline"; exit 0; fi; \
    exit "$status"

FROM builder-deps-warm AS builder-deps-resume
RUN timeout -k 15 230 cargo build --release --locked; \
    status=$?; \
    if [ "$status" -eq 124 ]; then echo "dependency build paused at its deadline"; exit 0; fi; \
    exit "$status"

FROM builder-deps-resume AS builder-deps
RUN timeout -k 15 240 cargo build --release --locked; \
    status=$?; \
    if [ "$status" -eq 124 ]; then echo "dependency build paused at its deadline"; exit 0; fi; \
    exit "$status"

FROM builder-deps AS builder-core
COPY apps/core/ /build/apps/core/
COPY apps/prime-mcp/ /build/apps/prime-mcp/
# The stand-ins were written after the checkout, so the real files look older
# than the compiled stubs; without the touch cargo would keep the stubs.
RUN find /build/apps/core /build/apps/prime-mcp -name '*.rs' -exec touch {} + \
    && timeout -k 15 260 cargo build --release --locked -p allsource-core --lib

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
