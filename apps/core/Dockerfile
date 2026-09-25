# syntax=docker/dockerfile:1.7
# =============================================================================
# AllSource Core - Production-Optimized Multi-Stage Dockerfile
# Optimized for: Fast cold starts, minimal image size, security
#
# Build targets (community edition — open source, Apache 2.0):
#   docker build --target runtime-community .         # Distroless, community features
#   docker build --target runtime-community-alpine .  # Alpine + HEALTHCHECK
#
# Build targets (enterprise edition — BSL 1.1):
#   docker build --target runtime .                   # Distroless, all features (default)
#   docker build --target runtime-alpine .            # Alpine + HEALTHCHECK
# =============================================================================

# Above the crate's MSRV (1.92): the image builds `analytics`, and datafusion 55 needs 1.94.
ARG RUST_VERSION=1.94
ARG ALPINE_VERSION=3.23
ARG EDITION=enterprise

# =============================================================================
# Stage 1: Chef - Prepare cargo-chef for dependency caching
# =============================================================================
FROM rust:${RUST_VERSION}-alpine AS chef

RUN apk add --no-cache musl-dev && \
    cargo install cargo-chef --locked

WORKDIR /app

# =============================================================================
# Stage 2: Planner - Analyze dependencies
# =============================================================================
FROM chef AS planner

# Copy workspace root manifests first
COPY Cargo.toml Cargo.lock ./

# Copy the core app source
COPY apps/core/Cargo.toml apps/core/Cargo.lock* apps/core/
COPY apps/core/src apps/core/src

# Create stubs for workspace members and optional targets
# Only workspace.members need stubs — excluded crates (registry, better-auth-allsource)
# have their own Cargo.toml and are invisible to the workspace resolver.
COPY tooling/performance/Cargo.toml tooling/performance/Cargo.toml
COPY sdks/rust/Cargo.toml sdks/rust/Cargo.toml
# core_nif is a cdylib (Rustler NIF) which musl cannot produce, so we stub it
# as a plain lib to satisfy workspace resolution without triggering cdylib builds
RUN mkdir -p tooling/performance/src sdks/rust/src sdks/rust/examples \
            apps/mcp-server-elixir/native/core_nif/src \
            apps/core/benches apps/core/tests/stress_tests && \
    echo 'fn main() {}' > tooling/performance/src/main.rs && \
    echo '' > sdks/rust/src/lib.rs && \
    echo 'fn main() {}' > sdks/rust/examples/asset_projection.rs && \
    printf '[package]\nname = "core_nif"\nversion = "0.1.0"\nedition = "2021"\n\n[lib]\nname = "core_nif"\npath = "src/lib.rs"\n\n[dependencies]\n' \
        > apps/mcp-server-elixir/native/core_nif/Cargo.toml && \
    echo '' > apps/mcp-server-elixir/native/core_nif/src/lib.rs && \
    echo 'fn main() {}' > apps/core/benches/performance_benchmarks.rs && \
    echo 'fn main() {}' > apps/core/benches/iai.rs && \
    echo 'fn main() {}' > apps/core/tests/stress_tests/seven_day_stress.rs && \
    mkdir -p crates/allsource-prime-models/src && \
    printf '[package]\nname = "allsource-prime-models"\nversion = "0.1.0"\nedition = "2021"\n\n[lib]\npath = "src/lib.rs"\n\n[dependencies]\n' \
        > crates/allsource-prime-models/Cargo.toml && \
    echo '' > crates/allsource-prime-models/src/lib.rs && \
    cargo chef prepare --recipe-path recipe.json

# =============================================================================
# Stage 3: Builder base - Shared dependency cooking
# =============================================================================
FROM chef AS builder-base

ARG VERSION=dev
ARG REVISION=unknown
ARG BUILDTIME=unknown

# Stub the optional allsource-prime-models path dep BEFORE cook: cargo-chef
# reconstructs the workspace skeleton from recipe.json and needs this directory
# to exist (the dep is feature-gated off in the analytics build, so a stub
# suffices). Without it, cook panics "No such file or directory".
RUN mkdir -p crates/allsource-prime-models/src && \
    printf '[package]\nname = "allsource-prime-models"\nversion = "0.1.0"\nedition = "2021"\n\n[lib]\npath = "src/lib.rs"\n\n[dependencies]\n' \
        > crates/allsource-prime-models/Cargo.toml && \
    echo '' > crates/allsource-prime-models/src/lib.rs

# Cook dependencies from recipe (cached layer — recipe is version-agnostic)
# cargo-chef replaces version fields with placeholders, so version bumps in
# Cargo.toml/Cargo.lock don't invalidate this expensive layer.
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json -p allsource-core --features analytics

# Now copy real manifests (version-sensitive) and set up workspace structure
COPY Cargo.toml Cargo.lock ./
COPY apps/core/Cargo.toml apps/core/
COPY tooling/performance/Cargo.toml tooling/performance/
COPY sdks/rust/Cargo.toml sdks/rust/
RUN mkdir -p apps/core/src tooling/performance/src sdks/rust/src sdks/rust/examples \
            apps/mcp-server-elixir/native/core_nif/src && \
    echo 'fn main() {}' > apps/core/src/main.rs && \
    echo 'fn main() {}' > tooling/performance/src/main.rs && \
    echo '' > sdks/rust/src/lib.rs && \
    echo 'fn main() {}' > sdks/rust/examples/asset_projection.rs && \
    printf '[package]\nname = "core_nif"\nversion = "0.1.0"\nedition = "2021"\n\n[lib]\nname = "core_nif"\npath = "src/lib.rs"\n\n[dependencies]\n' \
        > apps/mcp-server-elixir/native/core_nif/Cargo.toml && \
    echo '' > apps/mcp-server-elixir/native/core_nif/src/lib.rs && \
    mkdir -p crates/allsource-prime-models/src && \
    printf '[package]\nname = "allsource-prime-models"\nversion = "0.1.0"\nedition = "2021"\n\n[lib]\npath = "src/lib.rs"\n\n[dependencies]\n' \
        > crates/allsource-prime-models/Cargo.toml && \
    echo '' > crates/allsource-prime-models/src/lib.rs

# Copy actual source files for binary build
COPY apps/core/src apps/core/src
RUN mkdir -p apps/core/benches apps/core/tests/stress_tests && \
    echo 'fn main() {}' > apps/core/benches/performance_benchmarks.rs && \
    echo 'fn main() {}' > apps/core/benches/iai.rs && \
    echo 'fn main() {}' > apps/core/tests/stress_tests/seven_day_stress.rs

# =============================================================================
# Stage 3a: Builder Community - Community edition (Apache 2.0)
# =============================================================================
FROM builder-base AS builder-community
RUN CARGO_BUILD_JOBS=4 cargo build --release -p allsource-core \
        --bin allsource-core --bin allsource-migrate-storage \
        --features community,analytics && \
    strip /app/target/release/allsource-core /app/target/release/allsource-migrate-storage

# =============================================================================
# Stage 3b: Builder Enterprise - Enterprise edition (BSL 1.1)
# =============================================================================
FROM builder-base AS builder-enterprise
RUN CARGO_BUILD_JOBS=4 cargo build --release -p allsource-core \
        --bin allsource-core --bin allsource-migrate-storage \
        --features enterprise,analytics && \
    strip /app/target/release/allsource-core /app/target/release/allsource-migrate-storage

# =============================================================================
# Stage 4a: Runtime Community - Distroless (Apache 2.0, open source)
# =============================================================================
FROM gcr.io/distroless/static-debian12:nonroot AS runtime-community

ARG VERSION=dev
ARG REVISION=unknown
ARG BUILDTIME=unknown

LABEL org.opencontainers.image.title="AllSource Core (Community)" \
      org.opencontainers.image.description="High-performance event store built in Rust — community edition" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.revision="${REVISION}" \
      org.opencontainers.image.created="${BUILDTIME}" \
      org.opencontainers.image.vendor="AllSource Team" \
      org.opencontainers.image.licenses="Apache-2.0" \
      org.opencontainers.image.source="https://github.com/all-source-os/allsource-monorepo" \
      org.opencontainers.image.base.name="gcr.io/distroless/static-debian12:nonroot"

WORKDIR /app
COPY --from=builder-community /app/target/release/allsource-core ./allsource-core
COPY --from=builder-community /app/target/release/allsource-migrate-storage ./allsource-migrate-storage
EXPOSE 3900

ENV RUST_LOG=allsource_core=info,tower_http=info \
    ALLSOURCE_HOST=0.0.0.0 \
    ALLSOURCE_PORT=3900 \
    ALLSOURCE_DATA_DIR=/app/data \
    PORT=3900 \
    MALLOC_ARENA_MAX=2

ENTRYPOINT ["./allsource-core"]

# =============================================================================
# Stage 4b: Runtime Enterprise (DEFAULT) - Distroless (BSL 1.1)
# =============================================================================
FROM gcr.io/distroless/static-debian12:nonroot AS runtime

ARG VERSION=dev
ARG REVISION=unknown
ARG BUILDTIME=unknown

LABEL org.opencontainers.image.title="AllSource Core (Enterprise)" \
      org.opencontainers.image.description="High-performance event store built in Rust — enterprise edition" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.revision="${REVISION}" \
      org.opencontainers.image.created="${BUILDTIME}" \
      org.opencontainers.image.vendor="AllSource Team" \
      org.opencontainers.image.licenses="BSL-1.1" \
      org.opencontainers.image.source="https://github.com/all-source-os/allsource-monorepo" \
      org.opencontainers.image.base.name="gcr.io/distroless/static-debian12:nonroot"

WORKDIR /app
COPY --from=builder-enterprise /app/target/release/allsource-core ./allsource-core
COPY --from=builder-enterprise /app/target/release/allsource-migrate-storage ./allsource-migrate-storage
EXPOSE 3900

ENV RUST_LOG=allsource_core=info,tower_http=info \
    ALLSOURCE_HOST=0.0.0.0 \
    ALLSOURCE_PORT=3900 \
    ALLSOURCE_DATA_DIR=/app/data \
    PORT=3900 \
    MALLOC_ARENA_MAX=2

ENTRYPOINT ["./allsource-core"]

# =============================================================================
# Stage 4c: Runtime Community Alpine - With Docker HEALTHCHECK
# =============================================================================
FROM alpine:${ALPINE_VERSION} AS runtime-community-alpine

ARG VERSION=dev
ARG REVISION=unknown
ARG BUILDTIME=unknown

LABEL org.opencontainers.image.title="AllSource Core (Community)" \
      org.opencontainers.image.description="High-performance event store built in Rust — community edition" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.revision="${REVISION}" \
      org.opencontainers.image.created="${BUILDTIME}" \
      org.opencontainers.image.vendor="AllSource Team" \
      org.opencontainers.image.licenses="Apache-2.0" \
      org.opencontainers.image.source="https://github.com/all-source-os/allsource-monorepo" \
      org.opencontainers.image.base.name="alpine:${ALPINE_VERSION}"

RUN apk add --no-cache curl ca-certificates && \
    addgroup -g 1000 -S allsource && \
    adduser -u 1000 -S allsource -G allsource -h /app -s /sbin/nologin && \
    mkdir -p /app/data && \
    chown -R allsource:allsource /app

WORKDIR /app
COPY --from=builder-community --chown=allsource:allsource /app/target/release/allsource-core ./allsource-core
COPY --from=builder-community --chown=allsource:allsource /app/target/release/allsource-migrate-storage ./allsource-migrate-storage
COPY --chown=allsource:allsource apps/core/docker-entrypoint.sh /app/docker-entrypoint.sh
RUN chmod +x /app/docker-entrypoint.sh
USER allsource
EXPOSE 3900

ENV RUST_LOG=allsource_core=info,tower_http=info \
    ALLSOURCE_HOST=0.0.0.0 \
    ALLSOURCE_PORT=3900 \
    ALLSOURCE_DATA_DIR=/app/data \
    PORT=3900 \
    MALLOC_ARENA_MAX=2

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:3900/health || exit 1

# Wrapper script supports the optional ALLSOURCE_MIGRATE_FLAT_TO_TENANT
# one-shot migration. When unset, the script execs allsource-core directly.
ENTRYPOINT ["/app/docker-entrypoint.sh"]

# =============================================================================
# Stage 4d: Runtime Enterprise Alpine - With Docker HEALTHCHECK
# =============================================================================
FROM alpine:${ALPINE_VERSION} AS runtime-alpine

ARG VERSION=dev
ARG REVISION=unknown
ARG BUILDTIME=unknown

LABEL org.opencontainers.image.title="AllSource Core (Enterprise)" \
      org.opencontainers.image.description="High-performance event store built in Rust — enterprise edition" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.revision="${REVISION}" \
      org.opencontainers.image.created="${BUILDTIME}" \
      org.opencontainers.image.vendor="AllSource Team" \
      org.opencontainers.image.licenses="BSL-1.1" \
      org.opencontainers.image.source="https://github.com/all-source-os/allsource-monorepo" \
      org.opencontainers.image.base.name="alpine:${ALPINE_VERSION}"

RUN apk add --no-cache curl ca-certificates && \
    addgroup -g 1000 -S allsource && \
    adduser -u 1000 -S allsource -G allsource -h /app -s /sbin/nologin && \
    mkdir -p /app/data && \
    chown -R allsource:allsource /app

WORKDIR /app
COPY --from=builder-enterprise --chown=allsource:allsource /app/target/release/allsource-core ./allsource-core
COPY --from=builder-enterprise --chown=allsource:allsource /app/target/release/allsource-migrate-storage ./allsource-migrate-storage
COPY --chown=allsource:allsource apps/core/docker-entrypoint.sh /app/docker-entrypoint.sh
RUN chmod +x /app/docker-entrypoint.sh
USER allsource
EXPOSE 3900

ENV RUST_LOG=allsource_core=info,tower_http=info \
    ALLSOURCE_HOST=0.0.0.0 \
    ALLSOURCE_PORT=3900 \
    ALLSOURCE_DATA_DIR=/app/data \
    PORT=3900 \
    MALLOC_ARENA_MAX=2

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:3900/health || exit 1

# Wrapper script supports the optional ALLSOURCE_MIGRATE_FLAT_TO_TENANT
# one-shot migration. When unset, the script execs allsource-core directly.
ENTRYPOINT ["/app/docker-entrypoint.sh"]
