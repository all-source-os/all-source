---
title: "AllSource Event Store - Monorepo"
status: CURRENT
last_updated: 2026-11-24
version: "0.30.1"
---

<div align="center">

# AllSource Event Store

**Event store with WAL and Parquet durability, time-travel queries, an MCP interface and embedded agent memory (Prime).**

[![CI](https://github.com/all-source-os/all-source/actions/workflows/ci.yml/badge.svg)](https://github.com/all-source-os/all-source/actions/workflows/ci.yml)
[![Container CI](https://github.com/all-source-os/all-source/actions/workflows/container-ci.yml/badge.svg)](https://github.com/all-source-os/all-source/actions/workflows/container-ci.yml)
[![Docker Build](https://github.com/all-source-os/all-source/actions/workflows/docker-build.yml/badge.svg)](https://github.com/all-source-os/all-source/actions/workflows/docker-build.yml)
[![Release](https://img.shields.io/github/v/release/all-source-os/all-source?label=release&color=blue)](https://github.com/all-source-os/all-source/releases/latest)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](../../LICENSE)

[![Core](https://img.shields.io/badge/Core-v0.30.1-orange?logo=rust&logoColor=white)](../../apps/core/)
[![Control Plane](https://img.shields.io/badge/Control_Plane-v0.30.1-00ADD8?logo=go&logoColor=white)](../../apps/control-plane/)
[![Query Service](https://img.shields.io/badge/Query_Service-v0.30.1-4B275F?logo=elixir&logoColor=white)](../../apps/query-service/)
[![Web](https://img.shields.io/badge/Web-Next.js_16-000000?logo=next.js&logoColor=white)](../../apps/web/)
[![MCP Server](https://img.shields.io/badge/MCP_Server-73_tools-8A2BE2)](../../apps/mcp-server-elixir/)

[![Core Image](https://img.shields.io/badge/ghcr.io-allsource--core-blue?logo=docker&logoColor=white)](https://ghcr.io/all-source-os/allsource-core)
[![Control Plane Image](https://img.shields.io/badge/ghcr.io-allsource--control--plane-blue?logo=docker&logoColor=white)](https://ghcr.io/all-source-os/allsource-control-plane)
[![Query Service Image](https://img.shields.io/badge/ghcr.io-allsource--query--service-blue?logo=docker&logoColor=white)](https://ghcr.io/all-source-os/allsource-query-service)

</div>

---

## Quick Links

| | |
|---|---|
| **Get Started** | [Quick Start](../../docs/QUICK_START.md) · [Docker Guide](../../docs/deployment/DOCKER.md) · [Troubleshooting](../../docs/guides/TROUBLESHOOTING.md) |
| **Architecture** | [Monorepo Structure](../../docs/MONOREPO_STRUCTURE.md) · [Data Flow (C4)](../../docs/current/PROD_DATA_FLOW_C4.md) · [Tenant Model](../../docs/current/TENANT_ARCHITECTURE.md) |
| **API & Specs** | [API Reference](../../docs/current/API_REFERENCE.md) · [Performance](../../docs/current/PERFORMANCE.md) · [Event Store Features](../../docs/current/EVENT_STORE_FEATURES.md) |
| **Operations** | [Release Guide](../../docs/guides/RELEASE.md) · [Quality Gates](../../docs/current/QUALITY_GATES.md) · [WebSocket Config](../../docs/guides/WEBSOCKET_CONFIGURATION.md) |
| **Services** | [Core](../../apps/core/) · [Control Plane](../../apps/control-plane/) · [Query Service](../../apps/query-service/) · [MCP Server](../../apps/mcp-server-elixir/) · [Web](../../apps/web/) |
| **Deploy** | [Helm Chart](../../deploy/helm/allsource/) · [Kubernetes](../../deploy/k8s/) · [Fly.io](../../apps/core/fly.toml) |

---

## Architecture

```
Clients --> Query Service (Elixir, :3902) --> Core (Rust, :3900)
                |                                  |
           Control Plane (Go, :3901)         WAL + Parquet + DashMap
           (auth, billing, policies)         (events, projections,
                                              snapshots, schemas)
```

**Core is the database.** Events are stored in WAL (CRC32, fsync) and Parquet (Snappy compression), and served from DashMap (concurrent in-memory reads). No PostgreSQL dependency. Persistence is enabled by `ALLSOURCE_DATA_DIR`; without it Core runs in memory only. See [Data Flow (C4)](../../docs/current/PROD_DATA_FLOW_C4.md).

### Monorepo Structure

```
apps/
  core/               # Rust event store (the database)
  control-plane/      # Go auth, RBAC, policy, billing, operations
  query-service/      # Elixir read plane and API gateway
  mcp-server-elixir/  # MCP server
  prime-mcp/          # Prime MCP server (agent memory)
  web/                # Next.js dashboard
  auth/               # Rust auth service
  chronis/            # Event-sourced task CLI (cn)
  admin/              # Operator dashboard
  marketing-assets/   # Remotion render sources

deploy/
  helm/               # Helm charts
  k8s/                # Kubernetes manifests
  cloudrun/           # Cloud Run configs
  prometheus/         # Monitoring config
  grafana/            # Grafana provisioning

tooling/
  data-flow-test/     # E2E data flow test
  durability-test/    # WAL/Parquet durability test
```

The full layout and the dependency rules are in [MONOREPO_STRUCTURE.md](../../docs/MONOREPO_STRUCTURE.md).

---

## Project Status & Roadmap

> Changelog: [CHANGELOG](../../docs/CHANGELOG.md) · Roadmap: [Consolidated Roadmap](../../docs/roadmaps/2026-02-15_CONSOLIDATED_ROADMAP.md)

### Rust Core (port 3900) — [docs](../../apps/core/) · [features](../../docs/current/EVENT_STORE_FEATURES.md) · [perf](../../docs/current/PERFORMANCE.md)

The database. Source of truth for all event data.

- 469K events/sec ingestion, 11.9µs query latency — reproduce with `cargo run --release -p allsource-performance` from the repo root
- WAL (CRC32, fsync) + Parquet (Snappy) + DashMap for durability and speed
- Leader-follower replication via WAL shipping
- Schema registry, stream processing pipelines, multi-tenancy with RBAC
- Vector search (`vector-search` feature: fastembed + instant-distance) and keyword search (`keyword-search` feature: tantivy)
- **Embedded API**: use Core as an in-process library, with TOON output for LLMs. Powers [`allsource-mcp`](../../docs/guides/ALLSOURCE_MCP.md) (`cargo install allsource-mcp`) for local MCP debugging without a running server.

### Go Control Plane (port 3901) — [docs](../../apps/control-plane/)

Authentication, authorization, billing, and operational management.

- JWT auth and RBAC, policy enforcement with audit logging
- OAuth login (`oauth.go`)
- LemonSqueezy billing integration, OpenAPI spec (`apps/control-plane/docs/openapi.yaml`)
- OpenTelemetry distributed tracing

### Elixir Query Service (port 3902) — [docs](../../apps/query-service/) · [API ref](../../docs/allsource-qs-api-reference.md)

Stateless API gateway. Routes to Core, delegates auth to Control Plane.

- No PostgreSQL dependency
- Server-side projections with fold-on-read and continuous folding via PubSub
- `AUTH_DISABLED=true` (or `1`) for local dev; ignored when running in production
- Broadway event processing, Tesla HTTP client
- OpenAPI specification via `open_api_spex`

### MCP Server — [docs](../../apps/mcp-server-elixir/)

Interface for Claude Desktop or any MCP client. Responses use the TOON format.

### Web Dashboard (port 3000) — [docs](../../apps/web/)

- Next.js 16.3.5, served on port 3000 (`apps/web/Dockerfile`)

### SDKs — [Rust](../../sdks/rust/) · [Go](../../sdks/go/) · [Python](../../sdks/python-client/) · [TypeScript](../../sdks/typescript/)

- TypeScript package: `@allsourcedev/client` (`sdks/typescript/package.json`)
- Python package: `allsource-client` (`sdks/python-client/pyproject.toml`)

---

## Docker Images

`.github/workflows/docker-build.yml` builds `linux/amd64` images and pushes them to `ghcr.io/all-source-os`. It runs on pushes to `main` and on `v*` tags. Each image gets these tags: `latest` (default branch), `sha-<short>`, the branch name, and for `v*` tags the full version plus `<major>.<minor>` and `<major>`. A `v*` tag such as `v0.29.0` produces the tags `0.29.0`, `0.29` and `0`.

Images with `-community` in the name are the community edition. The `verify-community-public` job warns when they are not public.

| Image | Dockerfile (target) | Base |
|-------|---------------------|------|
| `allsource-core` | `apps/core/Dockerfile` (`runtime`) | distroless `static-debian12:nonroot` |
| `allsource-core-alpine` | `apps/core/Dockerfile` (`runtime-alpine`) | Alpine |
| `allsource-core-community` | `apps/core/Dockerfile` (`runtime-community`) | distroless `static-debian12:nonroot` |
| `allsource-control-plane` | `apps/control-plane/Dockerfile` | distroless `static-debian12:nonroot` |
| `allsource-query-service` | `apps/query-service/Dockerfile` (`ALLSOURCE_EDITION=enterprise`) | Alpine |
| `allsource-query-service-community` | `apps/query-service/Dockerfile` (`ALLSOURCE_EDITION=community`) | Alpine |
| `allsource-mcp-server` | `apps/mcp-server-elixir/Dockerfile` | Alpine |
| `allsource-mcp-server-embedded` | `apps/mcp-server-elixir/Dockerfile.embedded` | `debian:bookworm-slim` |
| `allsource-prime` | `apps/prime-mcp/Dockerfile` (`runtime`) | `rust:<RUST_VERSION>-slim` |

`apps/web/Dockerfile` exists, but `docker-build.yml` does not build a web image.

```bash
# Start the stack defined in the repo root (ALLSOURCE_VERSION selects the tag, default latest)
docker compose up -d

# Pull a specific version
docker pull ghcr.io/all-source-os/allsource-core:0.29.0
```

See [Docker Guide](../../docs/deployment/DOCKER.md) · [Release Guide](../../docs/guides/RELEASE.md)

---

## Development

### Prerequisites

- **Rust** nightly-2026-07-16 (pinned in `rust-toolchain.toml`) · **Go** 1.26.11 (control plane) · **Elixir** 1.17+ (query service) · **Bun** (web)

### Quick Start

```bash
git clone https://github.com/all-source-os/all-source.git
cd all-source
docker compose up -d

# Or run individual services
cd apps/core && cargo run
cd apps/control-plane && go run .
cd apps/query-service && mix phx.server
```

### Testing

```bash
cd apps/core && cargo test --lib
cd apps/control-plane && go test ./...
cd apps/query-service && mix test
cd apps/mcp-server-elixir && mix test
```

### Quality Gates

```bash
make quality-rust       # fmt + clippy + test + doc
make quality-go         # vet + lint + test
make quality-elixir     # format + credo + test
make check-versions     # Verify version consistency
```

See [Quality Gates](../../docs/current/QUALITY_GATES.md) · [Quality Gates Setup](../../docs/guides/QUALITY_GATES_SETUP.md)

### Version Locations

| Service | File |
|---------|------|
| Core | `apps/core/Cargo.toml` |
| Control Plane | `apps/control-plane/main.go` |
| Query Service | `apps/query-service/mix.exs` |
| MCP Server | `apps/mcp-server-elixir/mix.exs` |
| K8s Manifests | `deploy/k8s/*.yaml` |

---

## License

- Default build (`community` feature) and most of the crate: [Apache License 2.0](../../LICENSE).
- Enterprise source files carry a Business Source License 1.1 header. List them with `grep -rl "Business Source License" apps/core/src`. Terms: [LICENSE-BSL](../../LICENSE-BSL). The BSL work converts to Apache 2.0 on 2029-03-01.

---

<div align="center">

[Issues](https://github.com/all-source-os/all-source/issues) · [Releases](https://github.com/all-source-os/all-source/releases) · [Docs Hub](../../docs/README.md)

</div>
