---
title: "AllSource Event Store - Monorepo"
status: CURRENT
last_updated: 2026-11-24
version: "0.30.1"
---

<div align="center">

# AllSource Event Store

An event store in Rust with WAL and Parquet durability, time-travel queries, an MCP interface, and an embedded agent-memory engine (Prime). No PostgreSQL in the event path. Throughput and latency baselines are in [Benchmarks](#benchmarks).

[![CI](https://github.com/all-source-os/all-source/actions/workflows/ci.yml/badge.svg)](https://github.com/all-source-os/all-source/actions/workflows/ci.yml)
[![Container CI](https://github.com/all-source-os/all-source/actions/workflows/container-ci.yml/badge.svg)](https://github.com/all-source-os/all-source/actions/workflows/container-ci.yml)
[![Docker Build](https://github.com/all-source-os/all-source/actions/workflows/docker-build.yml/badge.svg)](https://github.com/all-source-os/all-source/actions/workflows/docker-build.yml)
[![Security Scanning](https://github.com/all-source-os/all-source/actions/workflows/security.yml/badge.svg?branch=main)](https://github.com/all-source-os/all-source/actions/workflows/security.yml)
[![CodeQL](https://github.com/all-source-os/all-source/actions/workflows/security.yml/badge.svg?branch=main&event=push)](https://github.com/all-source-os/all-source/security/code-scanning)
[![Release](https://img.shields.io/github/v/release/all-source-os/all-source?label=release&color=blue)](https://github.com/all-source-os/all-source/releases/latest)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)
[![License: BSL 1.1](https://img.shields.io/badge/Enterprise-BSL_1.1-orange.svg)](LICENSE-BSL)

[![crates.io](https://img.shields.io/crates/v/allsource-core?logo=rust&logoColor=white&label=allsource-core)](https://crates.io/crates/allsource-core)
[![crates.io downloads](https://img.shields.io/crates/d/allsource-core?label=downloads&color=2ea44f)](https://crates.io/crates/allsource-core)
[![npm](https://img.shields.io/npm/v/%40allsourcedev%2Fclient?logo=npm&label=%40allsourcedev%2Fclient)](https://www.npmjs.com/package/@allsourcedev/client)
[![MCP Server](https://img.shields.io/badge/MCP-server-8A2BE2?logo=anthropic)](apps/mcp-server-elixir/)
[![Polyglot](https://img.shields.io/badge/stack-Rust_·_Go_·_Elixir_·_TS-informational)](#architecture)

[![Core (Community)](https://img.shields.io/badge/ghcr.io-allsource--core--community-2ea44f?logo=docker&logoColor=white)](https://ghcr.io/all-source-os/allsource-core-community)
[![Query Service (Community)](https://img.shields.io/badge/ghcr.io-allsource--query--service--community-2ea44f?logo=docker&logoColor=white)](https://ghcr.io/all-source-os/allsource-query-service-community)
[![Enterprise images](https://img.shields.io/badge/ghcr.io-enterprise_(BSL_1.1,_auth_required)-orange?logo=docker&logoColor=white)](#docker-images)

> **Pulling for OSS / CI?** Use the `*-community` images above (Apache 2.0, public, no auth). The unsuffixed `allsource-core` / `allsource-control-plane` / `allsource-query-service` images are BSL 1.1 enterprise builds and require GHCR authentication. See [Docker Images](#docker-images).

</div>

---

## Try it

```bash
# Embed the event store engine (Rust)
cargo add allsource-core

# …or talk to a gateway with a client SDK
cargo add allsource              # Rust
bun add @allsourcedev/client     # TypeScript

# Run the ingestion benchmark harness
cargo run --release -p allsource-performance
```

**[Self-Hosting Guide](docs/self-hosting/README.md)** · [Quick Start](docs/QUICK_START.md) · [Benchmarks](#benchmarks) · [crates.io](https://crates.io/crates/allsource-core) · [npm](https://www.npmjs.com/package/@allsourcedev/client)

---

## Quick Links

| | |
|---|---|
| **Get Started** | [Self-Hosting Guide](docs/self-hosting/README.md) · [Quick Start](docs/QUICK_START.md) · [Docker Guide](docs/deployment/DOCKER.md) · [Troubleshooting](docs/guides/TROUBLESHOOTING.md) |
| **Architecture** | [Monorepo Structure](docs/MONOREPO_STRUCTURE.md) · [Data Flow (C4)](docs/current/PROD_DATA_FLOW_C4.md) · [Tenant Model](docs/current/TENANT_ARCHITECTURE.md) |
| **API & Specs** | [API Reference](docs/current/API_REFERENCE.md) · [Performance](docs/current/PERFORMANCE.md) · [Event Store Features](docs/current/EVENT_STORE_FEATURES.md) |
| **Operations** | [Release Guide](docs/guides/RELEASE.md) · [Quality Gates](docs/current/QUALITY_GATES.md) · [WebSocket Config](docs/guides/WEBSOCKET_CONFIGURATION.md) |
| **Services** | [Core](apps/core/) · [Control Plane](apps/control-plane/) · [Query Service](apps/query-service/) · [MCP Server](apps/mcp-server-elixir/) · [Prime MCP](apps/prime-mcp/) · [allsource-mcp](docs/guides/ALLSOURCE_MCP.md) · [Web](apps/web/) |
| **Agent Memory** | [Prime Guide](docs/guides/PRIME_AGENT_PROMPT.md) · [Examples](apps/core/examples/) · [Comparison: zer0dex](apps/web/content/zer0dex-vs-allsource-recall.mdx) |
| **Deploy** | [Helm Chart](deploy/helm/allsource/) · [Kubernetes](deploy/k8s/) · [Fly.io apps](#flyio-apps) |

---

## Architecture

```
Clients --> Query Service (Elixir, :3902) --> Core (Rust, :3900)
                |                                  |
           Control Plane (Go, :3901)         WAL + Parquet + DashMap
           (auth, billing, policies)         (events, projections,
                                              snapshots, schemas)
```

**Core is the database.** Events are stored in WAL (CRC32, fsync) and Parquet (Snappy compression), and served from DashMap (concurrent in-memory reads). No PostgreSQL dependency. Persistence is enabled by setting `ALLSOURCE_DATA_DIR`; without it Core runs in memory only. See [Data Flow (C4)](docs/current/PROD_DATA_FLOW_C4.md).

### Ports

| Service | Default port | Where it is set |
|---------|--------------|-----------------|
| Core | 3900 | `ALLSOURCE_PORT` or `PORT` (`apps/core/src/infrastructure/config/config.rs`) |
| Control Plane | 3901 | `PORT` (`apps/control-plane/main.go`). `docker-compose.yml` sets 8080 |
| Control Plane metrics | 9091 | `METRICS_ADDR` (private listener) |
| Query Service | 3902 | `PORT` (`apps/query-service/config/runtime.exs`) |
| Prime MCP (HTTP mode) | 3905 | `PRIME_PORT` (`apps/prime-mcp/src/main.rs`) |
| MCP Server (Elixir) | 4001 | image `EXPOSE` (`apps/mcp-server-elixir/Dockerfile`) |
| Web | 3000 | `docker-compose.yml` |

### Monorepo Structure

```
apps/
  core/               # Rust event store (Prime graph/vector/recall modules)
  prime-mcp/          # Prime MCP server (stdio and HTTP)
  control-plane/      # Go auth, RBAC, policy, billing, operations
  query-service/      # Elixir read plane and API gateway
  mcp-server-elixir/  # MCP server
  web/                # Next.js dashboard
  admin/              # Operator dashboard (admin.all-source.xyz)
  auth/               # Rust auth service (better-auth adapter)
  chronis/            # Event-sourced task CLI (cn)
  marketing-assets/   # Remotion render sources (screenshots, GIFs, video)

crates/               # Shared Rust library crates
sdks/                 # Client SDKs: rust, go, python-client, typescript
packages/             # Shared non-Rust packages (ui)
integrations/         # Framework adapters (langgraph-checkpoint-allsource)

deploy/
  helm/               # Helm charts
  k8s/                # Kubernetes manifests
  cloudrun/           # Cloud Run configs
  prometheus/         # Monitoring config
  grafana/            # Grafana provisioning

tooling/
  allsource-mcp/      # Local MCP server (cargo install allsource-mcp)
  performance/        # Ingestion benchmark harness (allsource-performance)
  mammoth-bench/      # Prime recall benchmark against keyword search
  recall-bench/       # Recall benchmark harness
  data-flow-test/     # E2E data flow test
  durability-test/    # WAL/Parquet durability test
```

---

## Benchmarks

Baseline recorded in [PERFORMANCE.md](docs/current/PERFORMANCE.md): **469,000 events/sec**
ingestion and **11.9 µs** p99 query latency.

To run the ingestion harness on your own hardware:

```bash
cargo run --release -p allsource-performance
```

The harness is [`tooling/performance/src/main.rs`](tooling/performance/src/main.rs).
Use `--release`. Debug builds are not comparable.
Detailed Criterion benchmarks: `cargo bench --bench performance_benchmarks` from `apps/core`.

---

## Agent Memory (Prime)

AllSource Prime is an agent memory engine: a knowledge graph, vector search and a compressed index in one binary.

```bash
cargo install allsource-prime
```

Add to Claude Desktop (`~/.claude/claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "prime": {
      "command": "allsource-prime",
      "args": ["--data-dir", "~/.prime/memory"]
    }
  }
}
```

The `prime_*` MCP tools cover knowledge management, semantic search, cross-domain reasoning, and temporal queries. See the [Prime Guide](docs/guides/PRIME_AGENT_PROMPT.md) for usage patterns.

Embedding Prime directly in your own Rust / Tauri app (no MCP server, no HTTP)? See the [Embedded Prime Integration Guide](docs/guides/EMBEDDED_PRIME_INTEGRATION.md) — covers feature flags, `Prime::open()`, `RecallEngine`, HNSW vector search, local `fastembed` embeddings, and a full Tauri example.

**Examples:**

```bash
cargo run --no-default-features --features prime --example prime_graph
cargo run --no-default-features --features prime-full --example prime_vectors
cargo run --no-default-features --features prime-recall --example prime_recall
```

For a comparison with manual markdown indexes on cross-domain recall, see [zer0dex vs AllSource recall](apps/web/content/zer0dex-vs-allsource-recall.mdx).

### mammoth — Prime as a one-command plugin

Prime and `chronis` packaged for coding agents, so an agent gets durable memory
without you wiring an MCP server by hand:

```
/plugin marketplace add all-source-os/all-source
/plugin install mammoth
```

Local-first and no account. Cross-machine sync is opt-in when you want it.

Recall is hybrid: in-process embeddings, plus the graph, plus recency. The
benchmark lives in [`tooling/mammoth-bench/`](tooling/mammoth-bench/) and scores
recall against a keyword search-and-grep baseline on reworded queries. The
60-memory, 60-query run in [`RESULTS-full.txt`](tooling/mammoth-bench/RESULTS-full.txt)
gives hit@5 0.90 for memory against 0.83 for the baseline, with p50 latency 3.0 ms.
The other recorded run, [`RESULTS.txt`](tooling/mammoth-bench/RESULTS.txt), gives hit@5 0.94 against 0.78.

**It composes with [caveman](https://github.com/JuliusBrussee/caveman) rather
than competing with it.** caveman compresses what your agent says; mammoth
persists what it learned. Running both gives you fewer tokens and a memory that
survives the session.

---

## Components

Roadmap: [Consolidated Roadmap](docs/roadmaps/2026-02-15_CONSOLIDATED_ROADMAP.md).
Release history: [CHANGELOG](docs/CHANGELOG.md).

### Rust Core (port 3900) — [docs](apps/core/) · [features](docs/current/EVENT_STORE_FEATURES.md) · [perf](docs/current/PERFORMANCE.md)

The database. Source of truth for all event data.

- Ingestion baseline 469,000 events/sec and 11.9 µs query latency, from [PERFORMANCE.md](docs/current/PERFORMANCE.md). Reproduce with `cargo run --release -p allsource-performance` ([benchmarks](#benchmarks))
- WAL (CRC32, fsync) + Parquet (Snappy) + DashMap. Persistence needs `ALLSOURCE_DATA_DIR`
- Leader-follower replication via WAL shipping (`replication` feature, enterprise edition)
- Multi-tenancy (`multi-tenant` feature, enterprise edition)
- Vector search (`vector-search` feature: fastembed + instant-distance) and keyword search (`keyword-search` feature: tantivy)
- Embedded API: use Core as an in-process library, with TOON output and network sync

### Go Control Plane (port 3901) — [docs](apps/control-plane/) *(enterprise)*

Authentication, authorization, billing, and operational management.

- JWT auth and RBAC (4 roles, 7 permissions), policy enforcement with audit logging
- Registered ES256 client keys: clients sign 60 s assertions with an operator-registered P-256 key ([guide](docs/guides/REGISTERED_CLIENT_KEYS.md), [decision](docs/decisions/2026-11-07-tenant-clients-registered-es256-keys.md))
- Dated sunset for unregistered onboarding keys (`ALLSOURCE_LEGACY_KEY_SUNSET`, `401 legacy_key_sunset`); dashboard keys unaffected
- OAuth login (GitHub, Google)
- LemonSqueezy billing integration, HAL hypermedia API, OpenAPI spec
- OpenTelemetry distributed tracing

### Elixir Query Service (port 3902) — [docs](apps/query-service/) · [API ref](docs/allsource-qs-api-reference.md)

Stateless read plane and API gateway. Routes to Core. The Control Plane issues JWTs and manages API keys.

- No PostgreSQL dependency
- Server-side projections with fold-on-read and continuous folding via PubSub
- `AUTH_DISABLED=true` (or `1`) for local dev; ignored in production builds
- Tesla HTTP client, Broadway event processing
- OpenAPI specification via `open_api_spex`

### MCP Server — [docs](apps/mcp-server-elixir/)

Interface for Claude Desktop or any MCP client. Responses use the TOON format.

### allsource-mcp (local debugging) — [guide](docs/guides/ALLSOURCE_MCP.md)

MCP server that reads WAL and Parquet files directly. No running Core server needed.

- `cargo install allsource-mcp`
- Read-only tools, including `list_stores`, `watch_events`, `fold_entity_lifecycle`, `get_latest_state`, `list_latest`, `list_changes`
- Built on the [Embedded Core API](docs/adr/001-embedded-core-library.md)

### Web Dashboard (port 3000) — [docs](apps/web/)

Next.js 16 application (`apps/web`). Port 3000 in `docker-compose.yml`.

### SDKs — [Rust](sdks/rust/) · [Go](sdks/go/) · [Python](sdks/python-client/) · [TypeScript](sdks/typescript/)

| SDK | Package | Source |
|-----|---------|--------|
| Rust | `allsource` | `sdks/rust/Cargo.toml` |
| Go | module `github.com/all-source-os/allsource-go` | `sdks/go/go.mod` |
| Python | `allsource-client` | `sdks/python-client/pyproject.toml` |
| TypeScript | `@allsourcedev/client` (npm) | `sdks/typescript/package.json` |

- LangGraph: experimental [`langgraph-checkpoint-allsource`](integrations/langgraph-checkpoint-allsource/) checkpoint adapter (alpha)

---

## Editions

AllSource follows an **open-core model**. The community edition is fully functional for single-node deployments. Enterprise features are available under a commercial license.

### Community Edition (Apache 2.0) — free, open source

Everything you need to run a production event store:

- **Core event store** — WAL + Parquet + DashMap, full durability, 469K events/sec
- **Full CRUD API** — events, projections, snapshots, schemas, webhooks, consumers
- **Analytics** — EventQL (DataFusion SQL engine), frequency/correlation/summary
- **WebSocket streaming** — real-time event subscriptions
- **Query Service** — API gateway with dev-mode auth
- **MCP Server** for Claude Desktop and other MCP clients
- **SDKs** — Rust, Go, Python, TypeScript

### Enterprise Edition (BSL 1.1) — commercial license

Adds multi-node and multi-tenant capabilities:

- **Leader-follower replication** — WAL shipping with semi-sync and sync modes; failover through `allsource-sentinel`
- **Multi-tenant management** — tenant CRUD, quota enforcement, usage tracking
- **Billing integration** — LemonSqueezy webhooks, subscription tiers
- **Rate limiting tiers** — professional and unlimited tiers (community includes free tier)
- **Control Plane** — Go service for auth, RBAC, policies, audit logging

> The BSL 1.1 license converts to Apache 2.0 on **2029-03-01**. See [LICENSE-BSL](LICENSE-BSL) for details.

---

## Docker Images

### Community (public — no auth required)

```bash
# Quick start — single command
docker compose -f docker-compose.community.yml up -d

# Or pull individually
docker pull ghcr.io/all-source-os/allsource-core-community:latest
docker pull ghcr.io/all-source-os/allsource-query-service-community:latest
```

| Image | License | Access | Base |
|-------|---------|--------|------|
| `ghcr.io/all-source-os/allsource-core-community` | Apache 2.0 | **Public** | Distroless |
| `ghcr.io/all-source-os/allsource-query-service-community` | Apache 2.0 | **Public** | Alpine |

Full self-hosting guide: [docs/self-hosting/README.md](docs/self-hosting/README.md)

### Enterprise (private — requires GHCR auth)

```bash
# Authenticate to GHCR
echo $GHCR_TOKEN | docker login ghcr.io -u USERNAME --password-stdin

# Pull enterprise images
docker pull ghcr.io/all-source-os/allsource-core:latest
docker pull ghcr.io/all-source-os/allsource-query-service:latest
```

| Image | License | Access | Base |
|-------|---------|--------|------|
| `ghcr.io/all-source-os/allsource-core` | BSL 1.1 | **Private** | Distroless (`nonroot`) |
| `ghcr.io/all-source-os/allsource-core-alpine` | BSL 1.1 | **Private** | Alpine (`USER allsource`) |
| `ghcr.io/all-source-os/allsource-query-service` | BSL 1.1 | **Private** | Alpine (`USER app`) |
| `ghcr.io/all-source-os/allsource-control-plane` | BSL 1.1 | **Private** | Distroless (`nonroot`) |
| `ghcr.io/all-source-os/allsource-mcp-server` | BSL 1.1 | **Private** | Alpine (`USER allsource`) |
| `ghcr.io/all-source-os/allsource-mcp-server-embedded` | BSL 1.1 | **Private** | Debian bookworm-slim (`USER allsource`) |
| `ghcr.io/all-source-os/allsource-prime` | BSL 1.1 | **Private** | `rust:<RUST_VERSION>-slim` (`USER prime`) |

> The `allsource-web` frontend is not published to GHCR. It deploys to Fly.io as `allsource-web` (see [Fly.io apps](#flyio-apps)).

### Building from Source

```bash
# Community edition
docker build --target runtime-community -t allsource-core:community apps/core
docker build --build-arg ALLSOURCE_EDITION=community -t allsource-query:community apps/query-service

# Enterprise edition
docker build --target runtime -t allsource-core:enterprise apps/core
docker build --build-arg ALLSOURCE_EDITION=enterprise -t allsource-query:enterprise apps/query-service
```

See [Docker Guide](docs/deployment/DOCKER.md) · [Release Guide](docs/guides/RELEASE.md) · [Self-Hosting](docs/self-hosting/README.md)

### Fly.io apps

The hosted deployment runs these Fly apps. The `fly.toml` in each directory is the source.

| Fly app | Directory | Role |
|---------|-----------|------|
| `allsource-core` | `apps/core` | Event store |
| `allsource-query` | `apps/query-service` | Query Service |
| `allsource-control-plane` | `apps/control-plane` | Control Plane (`api.all-source.xyz`) |
| `allsource-auth` | `apps/auth` | Auth service |
| `allsource-prime` | `apps/prime-mcp` | Prime MCP |
| `allsource-web` | `apps/web` | Dashboard (`all-source.xyz`, `www`) |
| `allsource-admin` | `apps/admin` | Operator dashboard (`admin.all-source.xyz`) |

---

## Development

### Prerequisites

- **Rust** nightly-2026-07-16 (pinned in `rust-toolchain.toml`) · **Go** 1.26.11 (control plane) · **Elixir** 1.17+ (OTP 27+) · **Bun** 1.3+

### Quick Start

```bash
git clone https://github.com/all-source-os/all-source.git
cd all-source

# Community edition (open source)
docker compose -f docker-compose.community.yml up -d

# Enterprise edition (all services)
docker compose up -d

# Or run individual services from source
cd apps/core && cargo run
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

See [Quality Gates](docs/current/QUALITY_GATES.md) · [Quality Gates Setup](docs/guides/QUALITY_GATES_SETUP.md)

### Version Locations

| Service | File |
|---------|------|
| Core | `apps/core/Cargo.toml` |
| Control Plane | `apps/control-plane/main.go` (`Version`, also `serviceVersion` in `tracing.go`) |
| Query Service | `apps/query-service/mix.exs` |
| MCP Server | `apps/mcp-server-elixir/mix.exs` |
| Prime MCP | `apps/prime-mcp/Cargo.toml` |
| K8s Manifests | `deploy/k8s/core.yaml`, `deploy/k8s/query-service.yaml` (image tags) |

`make check-versions` runs `scripts/check-versions.sh`, which compares these files and the SDK manifests.

---

## Security

[![Security Scanning](https://github.com/all-source-os/all-source/actions/workflows/security.yml/badge.svg?branch=main)](https://github.com/all-source-os/all-source/actions/workflows/security.yml)

Live status for every scanner runs off the badge above — it reflects the latest
run of [`security.yml`](.github/workflows/security.yml) on `main`.

| Scanner | Covers | Runs |
| --- | --- | --- |
| [CodeQL](https://github.com/all-source-os/all-source/security/code-scanning) | Go and JavaScript/TypeScript | push to `main`, weekly (Monday 00:00 UTC), on demand |
| `govulncheck` | Go dependencies and the standard library, with reachability analysis | push to `main`, weekly |
| `cargo audit` | Rust dependencies | push to `main`, weekly |
| `mix deps.audit` | Elixir dependencies | push to `main`, weekly |
| Trivy | Container images: core, core-alpine, control-plane, query-service | after a successful Docker Build on `main` |
| `go-licenses`, `cargo-deny` | License compliance | push to `main`, weekly |

Workflow: [`.github/workflows/security.yml`](.github/workflows/security.yml).
Findings land in the repository's
[code-scanning dashboard](https://github.com/all-source-os/all-source/security/code-scanning).

Two notes on reading them:

- **Reachability differs from version matching.** Dependency scanners flag by installed
  version. `govulncheck` reports only the vulnerable functions the code calls.
- **Generated output is excluded**, via
  [`.github/codeql/codeql-config.yml`](.github/codeql/codeql-config.yml). Build
  artefacts, bundled reports, and vendored assets are not first-party code.

To report a vulnerability privately, open a
[security advisory](https://github.com/all-source-os/all-source/security/advisories/new).

---

## License

- **Community edition**: [Apache License 2.0](LICENSE) — free for any use
- **Enterprise features** (replication, multi-tenant, billing): [Business Source License 1.1](LICENSE-BSL) — converts to Apache 2.0 on 2029-03-01

Enterprise-licensed files are marked with a BSL header comment. All other files are Apache 2.0.

---

<div align="center">

[Issues](https://github.com/all-source-os/all-source/issues) · [Releases](https://github.com/all-source-os/all-source/releases) · [Docs Hub](docs/README.md)

</div>
