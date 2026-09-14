# syntax=docker/dockerfile:1.7
# =============================================================================
# AllSource Control Plane - Production-Optimized Multi-Stage Dockerfile
# =============================================================================
# Uses distroless for minimal attack surface (~2MB base image)
# No shell, no package manager, no unnecessary utilities
# =============================================================================

ARG GO_VERSION=1.26.6

# =============================================================================
# Stage 1: Build
# =============================================================================
FROM golang:${GO_VERSION}-alpine AS builder

ARG VERSION=dev
ARG REVISION=unknown
ARG TARGETOS=linux
ARG TARGETARCH=amd64

# Install build dependencies in single layer
RUN apk add --no-cache ca-certificates tzdata

WORKDIR /app

# Copy go mod files first for better caching
COPY go.mod go.sum ./
RUN go mod download && go mod verify

# Copy source code (filtered by .dockerignore)
COPY . .

# Build statically linked binary with optimizations
# -buildvcs=false: Skip VCS stamping (avoids .git dependency)
# -trimpath: Remove file system paths from binary
# -ldflags="-s -w": Strip debug info and symbol table
RUN CGO_ENABLED=0 GOOS=${TARGETOS} GOARCH=${TARGETARCH} \
    go build \
    -buildvcs=false \
    -ldflags="-s -w -X main.version=${VERSION} -X main.revision=${REVISION}" \
    -trimpath \
    -o /app/control-plane .

# =============================================================================
# Stage 2: Runtime (Distroless)
# =============================================================================
# distroless/static-debian12 includes:
# - CA certificates (for HTTPS)
# - /etc/passwd with nonroot user (UID 65534)
# - Timezone data
# - No shell, no package manager (minimal attack surface)
# =============================================================================
FROM gcr.io/distroless/static-debian12:nonroot

ARG VERSION=dev
ARG REVISION=unknown
ARG BUILDTIME=unknown

# OCI Image Labels
LABEL org.opencontainers.image.title="AllSource Control Plane" \
      org.opencontainers.image.description="Enterprise orchestration and management for AllSource" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.revision="${REVISION}" \
      org.opencontainers.image.created="${BUILDTIME}" \
      org.opencontainers.image.vendor="AllSource Team" \
      org.opencontainers.image.licenses="MIT" \
      org.opencontainers.image.source="https://github.com/all-source-os/allsource-monorepo" \
      org.opencontainers.image.base.name="gcr.io/distroless/static-debian12:nonroot"

WORKDIR /app

# Copy binary from builder
# distroless:nonroot runs as UID 65534 by default
COPY --from=builder /app/control-plane /app/control-plane

# Default x402 pricing config. Enabled flag is false in the file; flip
# X402_ENABLED=true at runtime to activate. Override the whole config by
# pointing X402_PRICING_CONFIG at a different path.
COPY --from=builder /app/config/x402-pricing.json /app/config/x402-pricing.json

# Expose metrics and API port
EXPOSE 3901

# Environment
ENV GIN_MODE=release
ENV PORT=3901

# Note: HEALTHCHECK removed - distroless has no shell
# Health checks should be configured in Kubernetes/orchestrator:
#   livenessProbe:
#     httpGet:
#       path: /health
#       port: 3901
#     initialDelaySeconds: 5
#     periodSeconds: 30
#   readinessProbe:
#     httpGet:
#       path: /health
#       port: 3901
#     initialDelaySeconds: 5
#     periodSeconds: 10

# Note: No tini needed - distroless handles signals correctly
# Go applications handle SIGTERM properly when running as PID 1
# The runtime uses a minimal init system that forwards signals

# Run as nonroot user (UID 65534, already set by distroless:nonroot tag)
USER nonroot:nonroot

ENTRYPOINT ["/app/control-plane"]
