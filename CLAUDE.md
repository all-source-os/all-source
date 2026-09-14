# CLAUDE.md — AllSource Chronos Monorepo

## Critical: Architecture Facts

**AllSource Core IS the database.** It is a purpose-built Rust event store with full durability:
- **WAL** (Write-Ahead Log): CRC32 checksums, configurable fsync (default 100ms), crash recovery
- **Parquet**: Columnar persistence with Snappy compression, periodic flush
- **DashMap**: In-memory concurrent map for 11.9μs queries and 469K events/sec throughput

**DO NOT** describe Core as "in-memory only", "dumb", "not production-ready", or claim "data is lost on restart." Event data and operational metadata are durable. Users, tenants, API keys, configuration, subscriptions, quotas, and billing metadata are event-sourced through Core system streams; in-memory maps and service caches are rebuilt from Core.

**DO NOT** suggest PostgreSQL as a required or primary AllSource store. Current services have zero PostgreSQL dependency: Core is the source of truth for events and operational metadata. The correct way to improve availability is through Core replication (leader-follower, WAL shipping) — not by adding another database.

## Repository Structure

**See `docs/MONOREPO_STRUCTURE.md` for full rules.** Key directories:

```
apps/              — Deployable services ONLY
  core/            — Rust event store (AllSource Core) with Prime graph/vector/recall modules
  query-service/   — Elixir/Phoenix API gateway (auth, billing, routing to Core)
  web/             — Next.js frontend dashboard
  mcp-server-elixir/ — Elixir MCP server (separate from Rust MCP Docker binary)
  prime-mcp/       — Rust MCP server for Prime (stdio + HTTP, standalone workspace)
  auth/            — Rust auth service (better-auth + AllSource adapter, standalone workspace)
  chronis/         — Event-sourced task CLI (standalone workspace, excluded from root)
crates/            — Shared Rust library crates (NOT binaries, NOT SDKs)
sdks/              — Client SDKs — ALL languages, NO EXCEPTIONS
  rust/            — Rust SDK
  go/              — Go SDK
  python-client/   — Python SDK
  typescript/      — TypeScript SDK (@allsource/client)
packages/          — Shared non-Rust packages (NOT SDKs, NOT crates)
  ui/              — Shared UI component library
docs/
  proposals/       — Design proposals (e.g., CORE_REPLICATION_DESIGN.md)
  use-cases/       — Use case documents
  current/         — Current architecture docs
deploy/            — K8s manifests, deployment configs
tooling/           — Developer tools (durability tests, etc.)
```

**DO NOT** put SDKs in `packages/`, `apps/`, or anywhere else. SDKs go in `sdks/`.
**DO NOT** put shared Rust crates in `packages/`, `apps/`, or `sdks/`. Shared Rust libraries go in `crates/`.

## Isolation (CRITICAL)

**Apps, SDKs, and crates are fully isolated.** See `docs/MONOREPO_STRUCTURE.md` Rules 3 and 6 for details.

The dependency graph is strictly one-directional:

```
apps/ → crates/ → (external)
sdks/ → (external only — SDKs are standalone HTTP clients)
```

- No app may COPY, import, or depend on another app's source code
- No SDK may import from `apps/`, `crates/`, or another SDK
- Each app's Dockerfile references only its own source, plus `crates/` stubs if needed
- Apps that would create cross-contamination in Dockerfiles must be **excluded** from the root Cargo workspace (`workspace.exclude`)
- Apps communicate over the network, not via direct function calls

**DO NOT** add `COPY apps/<other-app>/` to any Dockerfile. **DO NOT** add new apps to `workspace.members` if it forces other Dockerfiles to stub them out. **DO NOT** create dependencies from `sdks/` → `apps/` or `crates/` → `apps/`.

## Service Architecture

```
Clients → Control Plane / Query Service → Core (Rust, port 3900)
                                           |
                              WAL + Parquet + DashMap
                              (events, system metadata,
                               projections, snapshots, schemas)
```

- **Core** = the database. Source of truth for events, users, tenants, API keys, configuration, subscriptions, quotas, and billing metadata.
- **Query Service** = stateless tenant-facing read plane. It separates HTTP queries, Phoenix Channel realtime delivery, analytics endpoints, and rebuildable per-tenant projections over Core events. Caches and read models are disposable; Core remains durable source of truth.
- **Control Plane** = public auth, onboarding, billing, and administration layer that persists through Core.
- **PostgreSQL** = not required or deployed by current AllSource services.

### Per-tenant read-model compute lives in the Query Service, NOT Core

Any **per-tenant, user-facing read-model that computes over a tenant's event
stream** — projections, materialized views, "your projections" dashboards — is a
**Query Service** concern. QS already folds Core's stream into read-models
(`ProjectionSync` → ETS) and terminates the tenant. **Do NOT** put per-tenant
projection compute/state in Core's projection engine (it's on the ingest hot
path and is a global, database-internal mechanism). Core's only role here is to
**store the enabled set as opaque tenant metadata** (`metadata.projections.enabled`)
and to serve the tenant's (already tenant-scoped, fail-closed) event stream.

Core's existing global engine projections (`entity_snapshots`, `event_counters`,
Prime's 9, the embedded demo set) are fine — they are internal, global, not
per-tenant. The line: Core may *store* the enabled set; it may not *compute or
serve* per-tenant projection state.

This is enforced by the `tenant-isolation-check` gate
(`tooling/tenant-isolation-check/`), which fails if `apps/core/src` gains
per-tenant projection compute (overridable only via an inline
`// CORE_PROJECTION_OK: <reason>` comment). Rationale + the boundary live in
`docs/proposals/PER_TENANT_PROJECTIONS.md` ("Why not Core").

## Production Deployment Topology

**Everything runs on Fly.io, in the `allsource` organization.** Frontends moved
off Vercel on 2026-09-02 (`fc601012`); `docs/runbooks/FLY_FRONTENDS.md` is the
authority for the frontend half.

| Fly app | Surface | Deploy |
|---|---|---|
| `allsource-core` | event store | `fly deploy --config apps/core/fly.toml` from the repo **root** |
| `allsource-query` | Query Service | `fly deploy --config apps/query-service/fly.toml` |
| `allsource-control-plane` | `api.all-source.xyz` | `fly deploy` from `apps/control-plane` |
| `allsource-auth` | Rust auth service | `fly deploy --config apps/auth/fly.toml` |
| `allsource-prime` | Prime MCP | `fly deploy --config apps/prime-mcp/fly.toml --dockerfile apps/prime-mcp/Dockerfile` from the root |
| `allsource-web` | `all-source.xyz` + `www` | `fly deploy . --config apps/web/fly.toml --remote-only --ha=false` |
| `allsource-admin` | `admin.all-source.xyz` | `fly deploy . --config apps/admin/fly.toml --remote-only --ha=false` |

Build context differs per app and is not guessable — a wrong context fails the
Docker build in a way that reads as a missing file. Follow the table.

Public values are build arguments in each `fly.toml`; server-only URLs and
tokens are Fly secrets or runtime env. Unstoppable Domains DNS points the apex,
`www`, `admin` and `api` records straight at Fly.

## Core API

All Core endpoints use the `/api/v1/` prefix. Key endpoints:
- `POST /api/v1/events` — ingest event (returns 200, not 201)
- `GET /api/v1/events/query` — query events (returns `{"events": [...], "count": N}`). Results are ordered by `(timestamp, version)`. Default order is ascending (oldest first); pass `order=desc` for newest first — e.g. `?entity_id=<id>&limit=1&order=desc` fetches the latest event for an entity.
- `GET /api/v1/projections` — list projections (returns `{"projections": [...], "total": N}`)
- `GET /api/v1/snapshots` — list snapshots
- `GET /api/v1/schemas` — list schemas
- `GET /health` — health check (note: root path, not /api/v1/health)
- `GET /metrics` — Prometheus metrics

Core wraps responses in maps (`{"events": [...]}`, `{"projections": [...]}`). The Query Service's RustCoreClient unwraps these before passing to controllers.

## Query Service Config

- `CORE_URL` — Core connection URL (not RUST_CORE_URL — clean env var names, no implementation details)
- `CORE_WS_URL` — Core WebSocket URL for real-time streaming
- Config key in Elixir: `:core_url` (not `:rust_core_url`)

## Docker Stack

Defined in the wallet project: `/Users/decebaldobrica/Projects/alphaSigmaPro/wallet/docker/docker-compose.allsource.yml`

Services on `supabase_network_alpha-sigma-pro`:
- `allsource-core-leader` (port 3280 → 3900, replication on 3910)
- `allsource-core-follower-1` (port 3281 → 3900)
- `allsource-core-follower-2` (port 3282 → 3900)
- `allsource-query-service` (port 3283 → 3902)
- `allsource-mcp` (port 3904)

Building Docker on Apple Silicon: native arm64 only — QEMU cross-compilation to linux/amd64 fails on Erlang NIF.

## Scaling Strategy

See `docs/proposals/CORE_REPLICATION_DESIGN.md`:
- Leader-follower replication via WAL shipping
- Query Service routes writes to leader, reads round-robin across followers
- No Raft, no PostgreSQL in the event path, no multi-leader

## Git Workflow

**Default: commit direct to `main`. Do NOT create feature branches or open pull requests for routine work.**

This repo is maintained by a single owner who reviews changes at commit time, not PR time. Going through feature-branch-and-PR for docs, fixes, and features adds rounds without adding review value. Branching/PR flows also surface a recurring local `HEAD` branch-slip bug that has cost time debugging git mechanics instead of shipping.

### Rules

- **Default workflow:** `git status` → `git add <paths>` → `git commit` → `git push origin main`. No branches. No `gh pr create`.
- **Always verify `git branch --show-current` before committing.** The branch-slip bug can flip HEAD between tool calls. If you aren't on `main`, fix it before staging.
- **Always run `git status` before committing.** Include related dirty files in the same commit or flag them to the user. Never leave the tree messy after a push.
- **NEVER force-push `main`.** Force-pushing a feature branch to update a PR was fine under the old flow; force-pushing `main` is not. If you need to rewrite history, stop and ask.
- **NEVER push `--tags` or run `git tag` without the user's explicit go-ahead.** Releases are the one place where "user manually pushes" still applies — see `.claude/skills/chronos-release/SKILL.md` and the release discipline note in `MEMORY.md`.
- **NEVER bypass hooks or signing.** No `--no-verify`, no `-c commit.gpgsign=false`. If a hook fails, fix the underlying issue.

### Exceptions — when a branch or PR IS still the right call

Use judgment, and ask if unsure:

- **User explicitly asks for a PR** — honor the request.
- **External contributions** from anyone other than the repo owner.
- **Experimental / WIP work** the user wants reviewable without it landing on `main`.
- **Releases** — the release flow still runs on `main` directly (see the release skill), but tag push requires explicit user confirmation. Tag naming is scoped by release scope: full-monorepo releases use `v<VERSION>`; SDK-only releases use `sdk-<lang>-v<VERSION>` (e.g. `sdk-rust-v0.19.0`) so per-SDK versions don't collide with Core/QS versions.

### What this replaces

Any advice from prior sessions or auto-memory entries about "push to a feature branch first" or "open a PR before merging" no longer applies for routine work on this repo. The main-branch-first workflow is the authoritative default.
