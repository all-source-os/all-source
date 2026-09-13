# syntax=docker/dockerfile:1.7
# =============================================================================
# AllSource Web - Production-Optimized Multi-Stage Dockerfile (Bun)
# 
# Build from monorepo root: docker build -f apps/web/Dockerfile -t allsource-web .
# =============================================================================

ARG BUN_VERSION=1.3
ARG ALPINE_VERSION=3.23

# =============================================================================
# Stage 1: Build with Bun
# =============================================================================
FROM oven/bun:${BUN_VERSION}-alpine AS builder

ARG VERSION=dev
ARG REVISION=unknown
ARG NEXT_PUBLIC_APP_URL=https://www.all-source.xyz
ARG NEXT_PUBLIC_API_URL=https://api.all-source.xyz
ARG NEXT_PUBLIC_CONTROL_PLANE_URL=https://api.all-source.xyz
ARG NEXT_PUBLIC_WS_URL=wss://allsource-query.fly.dev
ARG NEXT_PUBLIC_GA_MEASUREMENT_ID=G-3347JLG51K
ARG NEXT_PUBLIC_TURNSTILE_SITE_KEY=

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
COPY apps/web/package.json apps/web/

# Install dependencies (cached layer if package.jsons unchanged)
RUN bun install

# Copy tooling configs (shared across packages)
COPY tooling/typescript tooling/typescript

# Copy packages/ui source and config
COPY packages/ui/src packages/ui/src
COPY packages/ui/tsconfig.json packages/ui/
COPY packages/ui/bunfig.toml packages/ui/
COPY packages/ui/postcss.config.mjs packages/ui/

# Copy web app source and configs
COPY apps/web/src apps/web/src
COPY apps/web/public apps/web/public
COPY apps/web/next.config.mjs apps/web/
COPY apps/web/postcss.config.mjs apps/web/
COPY apps/web/tsconfig.json apps/web/
COPY apps/web/components.json apps/web/
COPY apps/web/content apps/web/content

# Docs needed for changelog page (reads ../../docs/CHANGELOG.md at build time)
COPY docs/CHANGELOG.md docs/

# Environment for build
ENV NEXT_TELEMETRY_DISABLED=1 \
    NODE_ENV=production \
    NEXT_PUBLIC_VERSION=${VERSION} \
    NEXT_PUBLIC_APP_URL=${NEXT_PUBLIC_APP_URL} \
    NEXT_PUBLIC_API_URL=${NEXT_PUBLIC_API_URL} \
    NEXT_PUBLIC_CONTROL_PLANE_URL=${NEXT_PUBLIC_CONTROL_PLANE_URL} \
    NEXT_PUBLIC_WS_URL=${NEXT_PUBLIC_WS_URL} \
    NEXT_PUBLIC_GA_MEASUREMENT_ID=${NEXT_PUBLIC_GA_MEASUREMENT_ID} \
    NEXT_PUBLIC_TURNSTILE_SITE_KEY=${NEXT_PUBLIC_TURNSTILE_SITE_KEY}

# Build the web application (run next build directly, not turbo)
WORKDIR /app/apps/web
RUN bun next build

# =============================================================================
# Stage 2: Runtime
# =============================================================================
FROM node:26-alpine AS runner

ARG VERSION=dev
ARG REVISION=unknown
ARG BUILDTIME=unknown

# OCI Image Labels
LABEL org.opencontainers.image.title="AllSource Web" \
      org.opencontainers.image.description="Interactive web dashboard for AllSource event store" \
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
    PORT=3000 \
    HOSTNAME="0.0.0.0"

# Install tini and create non-root user
RUN apk add --no-cache tini wget && \
    rm -rf /var/cache/apk/* && \
    addgroup --system --gid 1001 nodejs && \
    adduser --system --uid 1001 nextjs

# Copy built application with correct permissions
# Standalone preserves monorepo structure, so paths must match
COPY --from=builder --chown=nextjs:nodejs /app/apps/web/.next/standalone ./
COPY --from=builder /app/apps/web/public ./apps/web/public
COPY --from=builder --chown=nextjs:nodejs /app/apps/web/.next/static ./apps/web/.next/static

USER nextjs

EXPOSE 3000

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
    CMD wget --no-verbose --tries=1 --spider http://localhost:3000/api/healthz || exit 1

# Use tini for signal handling
ENTRYPOINT ["/sbin/tini", "-g", "--"]

CMD ["node", "apps/web/server.js"]
