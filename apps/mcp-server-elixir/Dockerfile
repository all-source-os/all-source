# syntax=docker/dockerfile:1.7
# =============================================================================
# AllSource MCP Server - Production-Optimized Multi-Stage Dockerfile
# =============================================================================

ARG ELIXIR_VERSION=1.18
ARG OTP_VERSION=27
ARG ALPINE_VERSION=3.23

# =============================================================================
# Stage 1: Build
# =============================================================================
FROM elixir:${ELIXIR_VERSION}-otp-${OTP_VERSION}-alpine AS builder

ARG VERSION=dev
ARG REVISION=unknown

# Install build dependencies
RUN apk add --no-cache git build-base cargo rust

# Set build environment
ENV MIX_ENV=prod

WORKDIR /app

# Install hex + rebar
RUN mix local.hex --force && \
    mix local.rebar --force

# Copy config and mix files
COPY mix.exs mix.lock ./
COPY config config

# Install dependencies
RUN mix deps.get --only $MIX_ENV && \
    mix deps.compile

# Copy source code
COPY lib lib
COPY tooling/customer-connection tooling/customer-connection

# Compile and create release
RUN mix compile && \
    mix release

# =============================================================================
# Stage 2: Runtime
# =============================================================================
FROM alpine:${ALPINE_VERSION}

ARG VERSION=dev
ARG REVISION=unknown
ARG BUILDTIME=unknown

# OCI Image Labels
LABEL org.opencontainers.image.title="AllSource MCP Server" \
      org.opencontainers.image.description="Model Context Protocol server for AI-native AllSource integration" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.revision="${REVISION}" \
      org.opencontainers.image.created="${BUILDTIME}" \
      org.opencontainers.image.vendor="AllSource Team" \
      org.opencontainers.image.licenses="MIT" \
      org.opencontainers.image.source="https://github.com/all-source-os/allsource-monorepo"

# Install runtime dependencies
RUN apk add --no-cache \
    libstdc++ \
    libgcc \
    ncurses-libs \
    openssl \
    tini && \
    rm -rf /var/cache/apk/* /tmp/* && \
    addgroup -g 1000 allsource && \
    adduser -u 1000 -G allsource -s /sbin/nologin -D allsource

WORKDIR /app

# Copy release from builder
COPY --from=builder --chown=allsource:allsource /app/_build/prod/rel/mcp_server_elixir ./

# Use non-root user
USER allsource:allsource

# Expose port
EXPOSE 4001

# Environment
ENV PHX_SERVER=true \
    MIX_ENV=prod \
    PORT=4001

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
    CMD bin/mcp_server_elixir rpc "System.otp_release()" || exit 1

# Use tini for signal handling
ENTRYPOINT ["/sbin/tini", "-g", "--"]

CMD ["bin/mcp_server_elixir", "start"]
