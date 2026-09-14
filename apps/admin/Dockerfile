# syntax=docker/dockerfile:1.7
# =============================================================================
# AllSource Admin - Production-Optimized Multi-Stage Dockerfile (Bun)
#
# Build from monorepo root: docker build -f apps/admin/Dockerfile -t allsource-admin .
# =============================================================================

ARG BUN_VERSION=1.3
ARG ALPINE_VERSION=3.23

# =============================================================================
# Stage 1: Build with Bun
# =============================================================================
FROM oven/bun:${BUN_VERSION}-alpine AS builder

ARG VERSION=dev
ARG REVISION=unknown
ARG NEXT_PUBLIC_APP_URL=https://admin.all-source.xyz
ARG NEXT_PUBLIC_API_URL=https://api.all-source.xyz

RUN apk add --no-cache libc6-compat

WORKDIR /app

# Copy root package files first for better layer caching
COPY package.json bun.lock ./

# Copy ALL workspace package.jsons to satisfy lockfile (monorepo pattern)
# Tooling packages
COPY tooling/typescript/package.json tooling/typescript/

# Package packages
COPY packages/ui/package.json packages/ui/

# App packages
COPY apps/admin/package.json apps/admin/

# Install dependencies (cached layer if package.jsons unchanged)
RUN bun install

# Copy tooling configs (shared across packages)
COPY tooling/typescript tooling/typescript

# Copy packages/ui source and config
COPY packages/ui/src packages/ui/src
COPY packages/ui/tsconfig.json packages/ui/
COPY packages/ui/bunfig.toml packages/ui/
COPY packages/ui/postcss.config.mjs packages/ui/

# Copy admin app source and configs
COPY apps/admin/src apps/admin/src
COPY apps/admin/public apps/admin/public
COPY apps/admin/next.config.mjs apps/admin/
COPY apps/admin/postcss.config.mjs apps/admin/
COPY apps/admin/tsconfig.json apps/admin/
COPY apps/admin/next-env.d.ts apps/admin/

# Environment for build
ENV NEXT_TELEMETRY_DISABLED=1 \
    NODE_ENV=production \
    NEXT_PUBLIC_VERSION=${VERSION} \
    NEXT_PUBLIC_APP_URL=${NEXT_PUBLIC_APP_URL} \
    NEXT_PUBLIC_API_URL=${NEXT_PUBLIC_API_URL}

# Build the admin application
WORKDIR /app/apps/admin
RUN bun next build

# =============================================================================
# Stage 2: Runtime
# =============================================================================
FROM node:25-alpine AS runner

ARG VERSION=dev
ARG REVISION=unknown
ARG BUILDTIME=unknown

# OCI Image Labels
LABEL org.opencontainers.image.title="AllSource Admin" \
      org.opencontainers.image.description="Administration dashboard for AllSource platform" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.revision="${REVISION}" \
      org.opencontainers.image.created="${BUILDTIME}" \
      org.opencontainers.image.vendor="AllSource Team" \
      org.opencontainers.image.licenses="MIT" \
      org.opencontainers.image.source="https://github.com/all-source-os/allsource-monorepo"

WORKDIR /app

# Environment
ENV NODE_ENV=production \
    NEXT_TELEMETRY_DISABLED=1 \
    PORT=3001 \
    HOSTNAME="0.0.0.0"

# Install tini and create non-root user
RUN apk add --no-cache tini wget && \
    rm -rf /var/cache/apk/* && \
    addgroup --system --gid 1001 nodejs && \
    adduser --system --uid 1001 nextjs

# Copy built application with correct permissions
# Standalone preserves monorepo structure, so paths must match
COPY --from=builder --chown=nextjs:nodejs /app/apps/admin/.next/standalone ./
COPY --from=builder /app/apps/admin/public ./apps/admin/public
COPY --from=builder --chown=nextjs:nodejs /app/apps/admin/.next/static ./apps/admin/.next/static

USER nextjs

EXPOSE 3001

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
    CMD wget --no-verbose --tries=1 --spider http://localhost:3001/ || exit 1

# Use tini for signal handling
ENTRYPOINT ["/sbin/tini", "-g", "--"]

CMD ["node", "apps/admin/server.js"]
