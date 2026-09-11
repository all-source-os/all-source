---
title: "AllSource Event Store - Monorepo"
status: CURRENT
last_updated: 2026-03-30
version: "0.23.0"
---

<div align="center">

# AllSource Event Store

**The AI-native event store** — durable event sourcing in Rust at **469K events/sec** ([reproducible](#benchmarks)), 11.9 µs reads, time-travel queries, a native MCP interface, and a built-in agent-memory engine. No Postgres in the event path.

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
[![MCP Server](https://img.shields.io/badge/MCP-61_tools-8A2BE2?logo=anthropic)](apps/mcp-server-elixir/)
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

# Prove the 469K events/sec for yourself
cargo run --release -p allsource-performance
```

**[Self-Hosting Guide](docs/self-hosting/README.md)** · [Quick Start](docs/QUICK_START.md) · [Benchmarks](#benchmarks) · [crates.io](https://crates.io/crates/allsource-core) · [npm](https://www.npmjs.com/package/@allsourcedev/client)

---

## Quick Links

| | |
|---|---|
| **Get Started** | [Self-Hosting Guide](docs/self-hosting/README.md) · [Quick Start](docs/QUICK_START.md) · [Docker Guide](docs/deployment/DOCKER.md) · [Troubleshooting](docs/guides/TROUBLESHOOTING.md) |
| **Architecture** | [Clean Architecture](docs/current/CLEAN_ARCHITECTURE.md) · [Tenant Model](docs/current/TENANT_ARCHITECTURE.md) · [Replication Design](docs/proposals/CORE_REPLICATION_DESIGN.md) |
| **API & Specs** | [API Reference](docs/current/API_REFERENCE.md) · [Performance](docs/current/PERFORMANCE.md) · [Event Store Features](docs/current/EVENT_STORE_FEATURES.md) |
| **Operations** | [Release Guide](docs/guides/RELEASE.md) · [Quality Gates](docs/current/QUALITY_GATES.md) · [WebSocket Config](docs/guides/WEBSOCKET_CONFIGURATION.md) |
| **Services** | [Core](apps/core/) · [Control Plane](apps/control-plane/) · [Query Service](apps/query-service/) · [MCP Server](apps/mcp-server-elixir/) · [Prime MCP](apps/prime-mcp/) · [allsource-mcp](docs/guides/ALLSOURCE_MCP.md) · [Web](apps/web/) |
| **Agent Memory** | [Prime Guide](docs/guides/PRIME_AGENT_PROMPT.md) · [Examples](apps/core/examples/) · [Comparison: zer0dex](docs/articles/zer0dex-comparison.md) |
| **Deploy** | [Helm Chart](deploy/helm/allsource/) · [Kubernetes](deploy/k8s/) · [Fly.io](apps/core/fly.toml) |

---

## Architecture

```
Clients --> Query Service (Elixir, :3902) --> Core (Rust, :3900)
                |                                  |
           Control Plane (Go, :3901)         WAL + Parquet + DashMap
           (auth, billing, policies)         (events, projections,
                                              snapshots, schemas)
```

**Core IS the database.** No PostgreSQL for events — just WAL (CRC32, fsync), Parquet (Snappy compression), and DashMap (concurrent in-memory reads). Zero external dependencies. [Full architecture docs →](docs/current/CLEAN_ARCHITECTURE.md)

### Monorepo Structure

```
apps/
  core/               # Rust event store          — the database (+ Prime graph/vector/recall)
  prime-mcp/          # Prime MCP server          — agent memory (stdio + HTTP)
  control-plane/      # Go auth/billing/ops       — the gatekeeper
  query-service/      # Elixir API gateway        — the router
  mcp-server-elixir/  # MCP server (73 tools)     — the AI interface
  web/                # Next.js dashboard          — the UI

deploy/
  helm/               # Helm charts
  k8s/                # Kubernetes manifests
  cloudrun/           # Cloud Run configs
  prometheus/         # Monitoring config
  grafana/            # Grafana provisioning

tooling/
  allsource-mcp/      # Local MCP server (cargo install allsource-mcp)
  recall-bench/       # Recall benchmark harness (LoCoMo, LongMemEval, cross-ref)
  data-flow-test/     # E2E data flow test
  durability-test/    # WAL/Parquet durability test
```

---

## Benchmarks

The performance numbers are reproducible — don't take the figure on faith, run the harness:

```bash
cargo run --release -p allsource-performance
```

It drives the hot paths of Core's ingestion pipeline (SIMD JSON parsing, lock-free
queues, the sharded batch processor, arena allocation, SIMD filtering) plus a
concurrent end-to-end pipeline, and asserts minimum throughput targets so
regressions fail loudly. Representative output on an **Apple M2 Max (12 cores),
`--release`**:

| Stage | Throughput |
|---|---|
| **Event ingestion** (batch processor) | **494K events/sec** |
| End-to-end pipeline (4 threads, concurrent) | 948K events/sec |
| Sustained ingestion (2 s wall) | 381K events/sec |
| SIMD JSON parsing | 1.09M docs/sec · 149 MB/s |
| Lock-free queue (push) | 72M events/sec |
| SIMD event filtering | 9.1M events/sec |
| Arena allocation | 28.8M allocs/sec |

The headline **469K events/sec** is the batch-processor ingestion path; this run
measured **494K** on M2 Max. Numbers are hardware-dependent and `--release` is
mandatory — debug builds run 10–20× slower. Harness:
[`tooling/performance/src/main.rs`](tooling/performance/src/main.rs) · step-by-step
walkthrough: [How to reproduce the benchmark](https://www.all-source.xyz/blog/reproduce-the-469k-events-benchmark).
Query latency (11.9 µs reads via DashMap) and methodology:
[PERFORMANCE.md](docs/current/PERFORMANCE.md).

---

## Agent Memory (Prime)

AllSource Prime is a unified agent memory engine — knowledge graph + vector search + compressed index in one binary.

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

19 MCP tools for knowledge management, semantic search, cross-domain reasoning, and temporal queries. See the [Prime Guide](docs/guides/PRIME_AGENT_PROMPT.md) for usage patterns.

Embedding Prime directly in your own Rust / Tauri app (no MCP server, no HTTP)? See the [Embedded Prime Integration Guide](docs/guides/EMBEDDED_PRIME_INTEGRATION.md) — covers feature flags, `Prime::open()`, `RecallEngine`, HNSW vector search, local `fastembed` embeddings, and a full Tauri example.

**Examples:**

```bash
cargo run --no-default-features --features prime --example prime_graph
cargo run --no-default-features --features prime-full --example prime_vectors
cargo run --no-default-features --features prime-recall --example prime_recall
```

```
$ cargo run --features prime --example prime_graph
Graph: 3 nodes, 3 edges
Project team: Alice (person), Bob (person)
Bob mentors: Alice
Path Alice → Bob: 2 hops
Alice's 2-hop subgraph: 3 nodes, 3 edges
```

See [zer0dex comparison](docs/articles/zer0dex-comparison.md) for how Prime's auto-generated compressed index beats manual markdown indexes on cross-domain recall.

---

## Project Status & Roadmap (v0.17.3)

### What's New in v0.17.3

- **WAL-backed consumer cursors**: Consumer cursor positions now persist through Core restarts via system events in the WAL. `ConsumerRegistry` supports dual-mode operation (in-memory for tests, durable for production). On startup, consumer state is rebuilt from `_system.consumer.*` events during Stage 2 bootstrap.
- **New `Consumer` system domain**: `_system.consumer.registered`, `_system.consumer.ack_updated`, `_system.consumer.deleted` event types for full consumer lifecycle tracking.

### Previous releases

- **v0.14.0**: Optimistic concurrency control, durable subscriptions, server-side event filtering, unified tenant management, JWT `is_demo` claim
- **v0.13.1**: WebSocket Mint migration, service JWT auth, Chronis CLI, Fly.io production deploy
- **v0.12.0**: Network sync transport, configurable conflict resolution, MCP tool event emission, WebSocket backpressure
- **v0.11.0**: Embedded Core library (8 phases), full dependency upgrade (arrow 57, datafusion 52, rand 0.10, reqwest 0.13, tantivy 0.25, fastembed 5)

> Full roadmap: [Consolidated Roadmap](docs/roadmaps/2026-02-15_CONSOLIDATED_ROADMAP.md) · Known gaps: [Roadmap P0](docs/roadmaps/2026-02-15_CONSOLIDATED_ROADMAP.md#p0-fix-existing-gaps)

### Rust Core (port 3900) — [docs](apps/core/) · [features](docs/current/EVENT_STORE_FEATURES.md) · [perf](docs/current/PERFORMANCE.md)

The database. Source of truth for all event data.

- 469K events/sec ingestion, 11.9µs query latency — [reproduce →](#benchmarks)
- WAL (CRC32, fsync) + Parquet (Snappy) + DashMap for durability and speed
- **v0.10.4+**: persistence wiring fix — env vars now correctly configure WAL+Parquet on startup
- Leader-follower replication via WAL shipping *(enterprise)* ([design](docs/proposals/CORE_REPLICATION_DESIGN.md))
- Schema registry, stream processing pipelines, multi-tenancy with RBAC *(enterprise)*
- Vector search (fastembed + HNSW) and BM25 keyword search (tantivy)
- **Embedded API**: use Core as an in-process library (1489 tests, 8 phases complete) with TOON output, network sync, and conflict strategies

### Go Control Plane (port 3901) — [docs](apps/control-plane/) *(enterprise)*

Authentication, authorization, billing, and operational management.

- JWT auth & RBAC (4 roles, 7 permissions), policy enforcement with audit logging
- OAuth login (GitHub, Google) with CSRF-protected state cookies
- LemonSqueezy billing integration, HAL hypermedia API, OpenAPI spec
- OpenTelemetry distributed tracing

### Elixir Query Service (port 3902) — [docs](apps/query-service/) · [API ref](docs/allsource-qs-api-reference.md)

Stateless API gateway. Routes to Core, delegates auth to Control Plane.

- Fully stateless, no PostgreSQL dependency
- Server-side projections with fold-on-read and continuous folding via PubSub
- `POST /api/query/projected` — snapshot-aware fold endpoint
- `AUTH_DISABLED` mode for local dev (fully bypasses all auth)
- Tesla HTTP client with connection pooling, Broadway event processing
- OpenAPI specification via `open_api_spex`

### MCP Server (73 tools) — [docs](apps/mcp-server-elixir/)

AI-native interface for Claude Desktop or any MCP client.

- 73 tools across 11 categories (discover, search, drill-down, context, mutate, ops, tenants, schema, analytics, dev)
- TOON format responses (~50% fewer tokens than JSON)

### allsource-mcp (local debugging) — [guide](docs/guides/ALLSOURCE_MCP.md)

Lightweight MCP server that reads WAL + Parquet files directly — no running Core server needed.

- `cargo install allsource-mcp` — single binary, zero dependencies
- 8 read-only tools: query, sample, stats, snapshot, timeline, explain, reconstruct, analyze
- Built on the [Embedded Core API](docs/adr/001-embedded-core-library.md) — same durability, no HTTP overhead

### Web Dashboard (port 3000) — [docs](apps/web/)

- Next.js 16 + TypeScript + React + Tailwind + shadcn/ui
- Events, API Keys, Billing, Pipelines, Settings pages

### SDKs — [Rust](sdks/rust/) · [Go](sdks/go/) · [Python](sdks/python-client/) · [TypeScript](sdks/typescript/)

- Rust SDK published to crates.io; TypeScript SDK to npm as `@allsourcedev/client`; Go and Python SDKs distributed via the GitHub registry
- LangGraph: experimental [`langgraph-checkpoint-allsource`](integrations/langgraph-checkpoint-allsource/) adapter passes all 58 current base conformance tests

### What's Next

| Priority | Focus | Details |
|----------|-------|---------|
| **P0** | Fix existing gaps | 5 QS endpoints return 501, Core fork commit stubbed, MCP analytics are basic aggregations |
| **P1** | SaaS launch | Fly.io deploy, LemonSqueezy products, onboarding wizard, landing page |
| **P2** | QS Phase 3 | Phoenix Channels WebSocket, Broadway Kafka/RabbitMQ, distributed mode |
| **P3** | Future | Multi-node Raft, geo-replication (CRDT), GraphQL |

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
- **MCP Server** — 73 AI-native tools for Claude Desktop
- **SDKs** — Rust, Go, Python, TypeScript

### Enterprise Edition (BSL 1.1) — commercial license

Adds multi-node and multi-tenant capabilities:

- **Leader-follower replication** — WAL shipping, semi-sync/sync modes, automatic failover
- **Multi-tenant management** — tenant CRUD, quota enforcement, usage tracking
- **Billing integration** — LemonSqueezy/Stripe webhooks, subscription tiers
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
| `ghcr.io/all-source-os/allsource-core` | BSL 1.1 | **Private** | Distroless |
| `ghcr.io/all-source-os/allsource-query-service` | BSL 1.1 | **Private** | Alpine |
| `ghcr.io/all-source-os/allsource-control-plane` | BSL 1.1 | **Private** | Distroless |
| `ghcr.io/all-source-os/allsource-mcp-server` | BSL 1.1 | **Private** | Alpine |
| `ghcr.io/all-source-os/allsource-prime` | BSL 1.1 | **Private** | Distroless |

> The `allsource-web` frontend is not published to GHCR — it deploys to Vercel at https://www.all-source.xyz.

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

---

## Development

### Prerequisites

- **Rust** 1.92+ · **Go** 1.24+ · **Elixir** 1.17+ (OTP 27+) · **Bun** 1.3+

### Quick Start

```bash
git clone https://github.com/all-source-os/all-source.git
cd allsource-monorepo

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
cd apps/core && cargo test --lib          # 1489 tests
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
| Control Plane | `apps/control-plane/main.go` |
| Query Service | `apps/query-service/mix.exs` |
| MCP Server | `apps/mcp-server-elixir/mix.exs` |
| K8s Manifests | `deploy/k8s/*.yaml` |

---

## Security

[![Security Scanning](https://github.com/all-source-os/all-source/actions/workflows/security.yml/badge.svg?branch=main)](https://github.com/all-source-os/all-source/actions/workflows/security.yml)

Live status for every scanner runs off the badge above — it reflects the latest
run of [`security.yml`](.github/workflows/security.yml) on `main`.

| Scanner | Covers | Runs |
| --- | --- | --- |
| [CodeQL](https://github.com/all-source-os/all-source/security/code-scanning) | Go + JavaScript/TypeScript, `security-and-quality` queries | every push to `main`, weekly, on demand |
| `govulncheck` | Go dependencies **and** the standard library, with reachability analysis | every push to `main`, weekly |
| `cargo audit` | Rust dependencies (`apps/core`) | every push to `main`, weekly |
| `mix deps.audit` | Elixir dependencies (`apps/query-service`) | every push to `main`, weekly |
| Trivy | Container images and filesystem | every push to `main`, weekly |

Findings land in the repository's
[code-scanning dashboard](https://github.com/all-source-os/all-source/security/code-scanning).

Two notes on reading them honestly:

- **Reachability beats version-matching.** Dependency scanners flag by installed
  version; `govulncheck` reports only what the code actually calls. A recent
  sweep flagged 35 CVEs by version, of which 5 were genuinely reachable — those
  5 were fixed, and the rest were noise from unused code paths.
- **Generated output is excluded**, via
  [`.github/codeql/codeql-config.yml`](.github/codeql/codeql-config.yml). Build
  artefacts, bundled reports, and vendored assets are not first-party code, and
  scanning them buries real findings under style warnings about minified files.

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
