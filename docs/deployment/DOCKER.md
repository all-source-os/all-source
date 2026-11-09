# Docker Images

Production-optimized container images for AllSource services.

## Image Gallery

| Service | Image | Size | Base |
|---------|-------|------|------|
| **Core** | `ghcr.io/all-source-os/chronos-core` | **15.7 MB** | Distroless |
| **Control Plane** | `ghcr.io/all-source-os/chronos-control-plane` | **27.9 MB** | Distroless |
| **Query Service** | `ghcr.io/all-source-os/chronos-query-service` | **35.1 MB** | Alpine 3.22 |

### Why So Small?

- **Distroless** base images (~2MB) with no shell or package manager
- **Static linking** for Rust and Go binaries
- **Multi-stage builds** that only include runtime artifacts
- **Stripped binaries** with debug symbols removed

---

## Quick Start

### Pull Images

```bash
# Latest stable
docker pull ghcr.io/all-source-os/chronos-core:latest
docker pull ghcr.io/all-source-os/chronos-control-plane:latest
docker pull ghcr.io/all-source-os/chronos-query-service:latest

# Specific version
docker pull ghcr.io/all-source-os/chronos-core:v0.10.7
```

### Run Locally

```bash
# Core Event Store (Rust)
docker run -d \
  --name allsource-core \
  -p 3900:3900 \
  -v allsource-data:/app/data \
  ghcr.io/all-source-os/chronos-core:latest

# Control Plane (Go)
docker run -d \
  --name allsource-control-plane \
  -p 3901:3901 \
  -e CORE_URL=http://allsource-core:3900 \
  -e JWT_SECRET=$(openssl rand -hex 32) \
  ghcr.io/all-source-os/chronos-control-plane:latest

# Query Service (Elixir) — stateless, no database needed
docker run -d \
  --name allsource-query-service \
  -p 3902:3902 \
  -e SECRET_KEY_BASE=$(openssl rand -hex 64) \
  -e CORE_URL=http://allsource-core:3900 \
  -e CORE_WS_URL=ws://allsource-core:3900 \
  ghcr.io/all-source-os/chronos-query-service:latest
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

| Variable | Default | Description |
|----------|---------|-------------|
| `RUST_LOG` | `allsource_core=info` | Log level |
| `ALLSOURCE_HOST` | `0.0.0.0` | Bind address |
| `ALLSOURCE_PORT` | `3900` | HTTP port |
| `ALLSOURCE_DATA_DIR` | `/app/data` | Data directory (WAL + Parquet) |
| `ALLSOURCE_WAL_ENABLED` | `true` | Enable Write-Ahead Log |
| `ALLSOURCE_PARQUET_ENABLED` | `true` | Enable Parquet persistence |
| `ALLSOURCE_JWT_SECRET` | - | JWT secret for auth (optional) |

### Control Plane (Go)

| Variable | Default | Description |
|----------|---------|-------------|
| `GIN_MODE` | `release` | Gin framework mode |
| `PORT` | `3901` | HTTP port |
| `CORE_URL` | `http://localhost:3900` | Core service URL |
| `JWT_SECRET` | - | JWT signing secret (required) |
| `FRONTEND_URL` | - | Frontend URL for OAuth callbacks |

### Query Service (Elixir)

> **Note:** Query Service is stateless — no database required. It fetches tenant/user data from Core.

| Variable | Default | Description |
|----------|---------|-------------|
| `SECRET_KEY_BASE` | - | Phoenix secret key (required) |
| `PORT` | `3902` | HTTP port |
| `CORE_URL` | `http://localhost:3900` | Core service URL |
| `CORE_WS_URL` | `ws://localhost:3900` | Core WebSocket URL ([see guide](../guides/WEBSOCKET_CONFIGURATION.md)) |
| `CORE_WS_ENABLED` | `true` | Enable real-time WebSocket client |
| `PHX_HOST` | `localhost` | Hostname for URL generation |

#### OAuth (Query Service)

| Variable | Description |
|----------|-------------|
| `GOOGLE_CLIENT_ID` | Google OAuth client ID |
| `GOOGLE_CLIENT_SECRET` | Google OAuth client secret |
| `GITHUB_CLIENT_ID` | GitHub OAuth client ID |
| `GITHUB_CLIENT_SECRET` | GitHub OAuth client secret |

#### Billing (Query Service)

| Variable | Description |
|----------|-------------|
| `LEMON_SQUEEZY_API_KEY` | LemonSqueezy API key |
| `LEMON_SQUEEZY_STORE_ID` | LemonSqueezy store ID |
| `LEMON_SQUEEZY_WEBHOOK_SECRET` | Webhook signing secret |

---

## Health Checks

### Core & Control Plane (Kubernetes)

Distroless images have no shell. Configure health checks in your orchestrator:

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

```bash
# Core (default: distroless)
cd apps/core
docker build -t allsource-core .

# Core (alpine variant with Docker HEALTHCHECK)
docker build --target runtime-alpine -t allsource-core:alpine .

# Control Plane
cd apps/control-plane
docker build -t allsource-control-plane .

# Query Service
cd apps/query-service
docker build -t allsource-query-service .
```

---

## Multi-Architecture Support

All images are built for:
- `linux/amd64` (x86_64)
- `linux/arm64` (Apple Silicon, AWS Graviton)

```bash
# Build multi-arch locally
docker buildx build --platform linux/amd64,linux/arm64 -t allsource-core .
```

---

## Security

### Image Hardening

- **Non-root execution**: All containers run as non-root users
  - Core/Control Plane: UID 65534 (distroless `nonroot`)
  - Query Service: UID 1000 (`app` user)
- **No shell access**: Distroless images have no shell or package manager
- **Read-only filesystem**: Compatible with `readOnlyRootFilesystem: true`
- **Minimal attack surface**: Only runtime dependencies included

### Vulnerability Scanning

```bash
# Scan with Trivy
trivy image ghcr.io/all-source-os/chronos-core:latest

# Scan with Grype
grype ghcr.io/all-source-os/chronos-core:latest
```

---

## Performance Tuning

### Memory Optimization

Core is a static musl binary using mimalloc, so glibc malloc settings such as `MALLOC_ARENA_MAX` have no effect on it. Its memory split is exported at `/metrics`: `allsource_heap_live_bytes`, `allsource_allocator_retained_bytes` and `allsource_resident_structure_bytes`.

### Elixir BEAM Tuning

Query Service includes optimized BEAM flags:
```
+sbwt very_short +sbwtdcpu very_short +sbwtdio very_short
```

These reduce scheduler busy-wait times for faster cold starts.

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

Ensure Alpine version matches the build image (3.22+) for OpenSSL compatibility.

---

## Next Steps

- [WebSocket Configuration](../guides/WEBSOCKET_CONFIGURATION.md) - Real-time event streaming setup
- [Kubernetes Deployment](../k8s/) - Production Kubernetes manifests
- [Helm Chart](../../deploy/helm/allsource/) - Helm chart for easy deployment
