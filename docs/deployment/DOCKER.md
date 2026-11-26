# Docker Images

Production-optimized container images for AllSource services.

## Images

| Image | Built from | Base | Runtime user |
|-------|------------|------|--------------|
| `ghcr.io/all-source-os/allsource-core` | `apps/core/Dockerfile`, no `--target` (builds the last stage, `runtime-alpine`) | `alpine:3.23` | uid 1000, `allsource` (`apps/core/Dockerfile:340`) |
| `allsource-core-alpine` | `apps/core/Dockerfile`, `--target runtime-alpine` (`.github/workflows/docker-build.yml:87`) | `alpine:3.23` | uid 1000, `allsource` |
| `allsource-core-community` | `apps/core/Dockerfile`, `--target runtime-community` (`docker-build.yml:97`) | `gcr.io/distroless/static-debian12:nonroot` | uid 65532, from the base image (no `USER` line in the stage, `Dockerfile:209`) |
| `allsource-control-plane` | `apps/control-plane/Dockerfile` | `gcr.io/distroless/static-debian12:nonroot` | `USER nonroot:nonroot` (uid 65532, `Dockerfile:53,108`) |
| `allsource-query-service` | `apps/query-service/Dockerfile` | `alpine:3.23` | uid 1000, `app` (`Dockerfile:94,101`) |
| `allsource-prime` | `apps/prime-mcp/Dockerfile` | `rust:<RUST_VERSION>-slim` | system user `prime` from `useradd -r`, uid not pinned (`Dockerfile:99,106`) |

The enterprise distroless target `runtime` (`apps/core/Dockerfile:242`) is not pushed by any workflow. Build it locally with `--target runtime`.

---

## Quick Start

### Pull Images

```bash
# Latest stable
docker pull ghcr.io/all-source-os/allsource-core:latest
docker pull ghcr.io/all-source-os/allsource-control-plane:latest
docker pull ghcr.io/all-source-os/allsource-query-service:latest

# Specific version (release tags are pushed without the "v" prefix, .github/workflows/release.yml:49)
docker pull ghcr.io/all-source-os/allsource-core:0.30.2
```

### Run Locally

```bash
# Core Event Store (Rust)
docker run -d \
  --name allsource-core \
  -p 3900:3900 \
  -v allsource-data:/app/data \
  ghcr.io/all-source-os/allsource-core:latest

# Control Plane (Go)
docker run -d \
  --name allsource-control-plane \
  -p 3901:3901 \
  -e CORE_SERVICE_URL=http://allsource-core:3900 \
  -e JWT_SECRET=$(openssl rand -hex 32) \
  ghcr.io/all-source-os/allsource-control-plane:latest

# Query Service (Elixir) — stateless, no database needed
docker run -d \
  --name allsource-query-service \
  -p 3902:3902 \
  -e SECRET_KEY_BASE=$(openssl rand -hex 64) \
  -e CORE_URL=http://allsource-core:3900 \
  -e CORE_WS_URL=ws://allsource-core:3900/api/v1/events/stream \
  ghcr.io/all-source-os/allsource-query-service:latest
```

---

## Docker Compose

The easiest way to run the complete stack locally.

```bash
# Start all services
docker compose up -d

# View logs
docker compose logs -f

# Stop
docker compose down
```

See [`docker-compose.yml`](../../docker-compose.yml) in the repository root.

---

## Environment Variables

### Core (Rust)

Defaults are the code defaults. Where the image sets a different value, the image value is given.

| Variable | Default | Description |
|----------|---------|-------------|
| `RUST_LOG` | `allsource_core=info,tower_http=info` (`apps/core/src/main.rs:65`; the image sets the same) | Log filter |
| `ALLSOURCE_HOST` | `0.0.0.0` (`main.rs:492`) | Bind address |
| `ALLSOURCE_PORT` | `3900`; read before `PORT` (`main.rs:487-491`) | HTTP port |
| `ALLSOURCE_DATA_DIR` | Image: `/app/data`. Unset in code: in-memory mode, no WAL or Parquet (`apps/core/src/store.rs:3863-3925`) | Data directory (WAL and Parquet under `storage/` and `wal/`) |
| `ALLSOURCE_WAL_ENABLED` | `true` (`store.rs:3880`) | Enable the write-ahead log |
| `ALLSOURCE_JWT_SECRET` | unset: a random per-process secret is used and tokens from other services fail (`main.rs:387-394`) | JWT validation secret |
| `ALLSOURCE_MIGRATE_FLAT_TO_TENANT` | unset: skipped. Alpine images only; the entrypoint runs the one-shot migration when set to `true` (`apps/core/docker-entrypoint.sh`) | One-shot storage migration |

### Control Plane (Go)

| Variable | Default | Description |
|----------|---------|-------------|
| `GIN_MODE` | `release` (set by the image, `apps/control-plane/Dockerfile:85`). Release mode applies only when this is `release` (`main.go:151`) | Gin framework mode |
| `PORT` | `3901` (`main.go:39,1255`) | HTTP port |
| `CORE_SERVICE_URL` | `http://localhost:3900` (`main.go:40-41,139-141`) | Core service URL |
| `CORE_API_KEY` | unset | Bearer for requests to Core (`apps/control-plane/internal/infrastructure/clients/core_client.go:450-452`) |
| `JWT_SECRET` | unset: falls back to a fixed default and logs an insecure-default warning (`main.go:165-169`). Set it in every deployment | JWT signing secret |
| `FRONTEND_URL` | unset | Frontend origin for OAuth callbacks; always an allowed origin (`apps/control-plane/oauth.go:73,160`) |
| `ALLOWED_FRONTEND_URLS` | unset | Additional comma-separated origins (`oauth.go:83`) |
| `PRIME_SERVICE_URL` | `http://allsource-prime.internal:3905` (`main.go:353-355`) | allsource-prime base URL |
| `PRIME_API_KEY` | unset | Shared bearer for allsource-prime. Forwarded with `X-Tenant-Id` when set (`delegation.go:507`) |

### Query Service (Elixir)

> **Note:** Query Service does not use a database. It fetches tenant and user data from Core.

| Variable | Default | Description |
|----------|---------|-------------|
| `SECRET_KEY_BASE` | unset: a random value is generated at each boot (`apps/query-service/config/runtime.exs:62-64`) | Phoenix secret key base. Set it so signed values stay valid across restarts |
| `ALLSOURCE_EDITION` | `community` (`runtime.exs:19`) | Edition at runtime. Enterprise routes exist only in an image built with `ALLSOURCE_EDITION=enterprise` (`apps/query-service/Dockerfile:34-44`) |
| `PORT` | `3902` (`runtime.exs:67`) | HTTP port |
| `CORE_URL` | `http://localhost:3900` (`runtime.exs:96`) | Core service URL |
| `CORE_WRITE_URL` | `CORE_URL` (`runtime.exs:98`) | Core URL for writes (leader) |
| `CORE_READ_URLS` | `CORE_URL` (`runtime.exs:100-105`) | Comma-separated Core URLs for reads |
| `CORE_API_KEY` | unset | Bearer sent to Core when set (`apps/query-service/lib/query_service_ex/infrastructure/adapters/rust_core_client.ex:128-130`) |
| `CORE_WS_URL` | `ws://localhost:3900/api/v1/events/stream` (`runtime.exs:117`) | Core WebSocket URL ([see guide](../guides/WEBSOCKET_CONFIGURATION.md)) |
| `CORE_WS_ENABLED` | `true` (`runtime.exs:118`) | Enable real-time WebSocket client |
| `PHX_HOST` | `localhost` (`runtime.exs:66`) | Hostname for URL generation |

#### OAuth (Control Plane)

Read in `apps/control-plane/oauth.go:54-62`.

| Variable | Description |
|----------|-------------|
| `GOOGLE_CLIENT_ID` | Google OAuth client ID |
| `GOOGLE_CLIENT_SECRET` | Google OAuth client secret |
| `GITHUB_CLIENT_ID` | GitHub OAuth client ID |
| `GITHUB_CLIENT_SECRET` | GitHub OAuth client secret |

#### Billing (Control Plane)

| Variable | Description |
|----------|-------------|
| `LEMON_SQUEEZY_API_KEY` | LemonSqueezy API key. Billing is enabled when set (`apps/control-plane/main.go:232`); the client requires it (`internal/infrastructure/clients/lemon_squeezy_client.go:193-195`) |
| `LEMON_SQUEEZY_STORE_ID` | LemonSqueezy store ID. Required by the client (`lemon_squeezy_client.go:198-200`) |
| `LEMON_SQUEEZY_WEBHOOK_SECRET` | Webhook signing secret (`internal/interfaces/http/webhook_handler.go:47`) |

---

## Health Checks

### Core and Control Plane

The Core alpine images define a `HEALTHCHECK` (`apps/core/Dockerfile:312-313,359-360`). The Control Plane image has none, and distroless images have no shell. Core serves `GET /health` on 3900 (`apps/core/src/infrastructure/web/api.rs:115`). The Control Plane serves `GET /health` on 3901 (`apps/control-plane/main.go:530`). Configure probes in the orchestrator, for example:

```yaml
# Kubernetes example
livenessProbe:
  httpGet:
    path: /health
    port: 3900
  initialDelaySeconds: 5
  periodSeconds: 30

readinessProbe:
  httpGet:
    path: /health
    port: 3900
  initialDelaySeconds: 5
  periodSeconds: 10
```

### Query Service (Docker)

Built-in `HEALTHCHECK` using Elixir release RPC:

```dockerfile
HEALTHCHECK --interval=30s --timeout=5s --start-period=15s --retries=3 \
    CMD bin/query_service_ex rpc "IO.puts(:ok)" | grep -q "ok" || exit 1
```

---

## Building Locally

The Core Dockerfile copies workspace crates from the repository root (`apps/core/Dockerfile:40-50`), so Core builds from the repository root. The Control Plane and Query Service build from their own directories.

```bash
# Core, from the repository root. With no --target, the last stage (runtime-alpine) is built.
docker build -f apps/core/Dockerfile -t allsource-core .

# Core, distroless enterprise image
docker build -f apps/core/Dockerfile --target runtime -t allsource-core:distroless .

# Core, distroless community image
docker build -f apps/core/Dockerfile --target runtime-community -t allsource-core:community .

# Control Plane
cd apps/control-plane
docker build -t allsource-control-plane .

# Query Service
cd apps/query-service
docker build -t allsource-query-service .
```

---

## Platform

CI builds all images for `linux/amd64` only (`.github/workflows/release.yml:116`, `.github/workflows/docker-build.yml:410`). No workflow publishes an arm64 image.

---

## Security

### Image Hardening

- **Non-root execution**: every image in the table above runs as a non-root user.
  - Core distroless: uid 65532 from the `nonroot` base image (no `USER` line)
  - Core alpine: uid 1000, `allsource`
  - Control Plane: `USER nonroot:nonroot`, uid 65532. The comments at `apps/control-plane/Dockerfile:49,73,107` say 65534; those comments do not match the `nonroot` base image.
  - Query Service: uid 1000, `app`
  - allsource-prime: system user `prime`, uid assigned at build time
- **No shell in distroless images**: `allsource-core-community`, `allsource-control-plane` and the enterprise `runtime` target have no shell or package manager. Alpine images include `/bin/sh`.

### Vulnerability Scanning

```bash
# Scan with Trivy
trivy image ghcr.io/all-source-os/allsource-core:latest

# Scan with Grype
grype ghcr.io/all-source-os/allsource-core:latest
```

---

## Performance Tuning

### Memory Optimization

Core uses mimalloc as its global allocator (`apps/core/src/main.rs:4`), so glibc malloc tunables such as `MALLOC_ARENA_MAX` do not apply to it. Its memory split is exported at `/metrics` (`apps/core/src/infrastructure/observability/metrics.rs:559,565,581`): `allsource_heap_live_bytes`, `allsource_allocator_retained_bytes`, `allsource_resident_structure_bytes` (and `allsource_resident_structure_entries`).

### Elixir BEAM Tuning

The Query Service image sets these Erlang VM flags (`apps/query-service/Dockerfile:123`):
```
ERL_FLAGS="+sbwt very_short +sbwtdcpu very_short +sbwtdio very_short"
```

---

## Troubleshooting

### Container won't start

```bash
# Check logs
docker logs allsource-core

# Run interactively (alpine variant only)
docker run -it --rm allsource-core:alpine /bin/sh
```

### Health check failing

```bash
# Test health endpoint manually
curl http://localhost:3900/health
curl http://localhost:3901/health
curl http://localhost:3902/api/health
```

### Query Service crypto errors

The runtime Alpine version must match the `elixir:…-alpine` build image for OpenSSL compatibility. Both use the `ALPINE_VERSION` build argument, default `3.23` (`apps/query-service/Dockerfile:16-17`).

---

## Next Steps

- [WebSocket Configuration](../guides/WEBSOCKET_CONFIGURATION.md) - Real-time event streaming setup
- [Kubernetes Deployment](../../deploy/k8s/) - Production Kubernetes manifests
- [Helm Chart](../../deploy/helm/allsource/) - Helm chart for easy deployment
