# Changelog

All notable changes to AllSource Chronos are documented here.

## [Unreleased]

### Added

- **Registered ES256 client keys** — a tenant client keeps a P-256 private key,
  an operator registers the public half
  (`POST/GET /api/v1/admin/tenants/:id/client-keys`,
  `DELETE /api/v1/admin/tenants/:id/client-keys/:kid`), and each request carries a
  60-second ES256 assertion (`kid` = RFC 7638 thumbprint, `iss` = `client_id`,
  `sub` = tenant, `aud` = `ALLSOURCE_ASSERTION_AUDIENCE`). Role and scopes come from
  the registration. The gateway verifies assertions without a Core round-trip per
  request.
- **Client signers in every SDK** — Rust (`Es256AssertionProvider`, feature
  `es256-assertion`), TypeScript, Python and Go providers mint the assertion and
  are asked for a bearer on every request attempt. On that path only
  `Authorization` is sent. `allsource-mcp --remote` reads `ALLSOURCE_CLIENT_*`.
- **Legacy key sunset** — `ALLSOURCE_LEGACY_KEY_SUNSET` (RFC 3339) on the gateway
  and the Query Service. Managed onboarding keys with no registration record are
  served with `Deprecation` and `Sunset` headers until the instant and refused
  afterwards with `401` reason `legacy_key_sunset`. Dashboard keys are not
  affected.

Guide: [Registered client keys](guides/REGISTERED_CLIENT_KEYS.md). Operator
procedure: [CLIENT_KEY_ROTATION.md](runbooks/CLIENT_KEY_ROTATION.md). Decision:
[2026-11-07](decisions/2026-11-07-tenant-clients-registered-es256-keys.md).

## [0.20.1] - 2026-04-30

### Fixed

- **Parquet checkpoint write is now atomic** (#166) — `ParquetStorage` writes new
  checkpoints as `<file>.tmp`, fsyncs the file, renames into place, then fsyncs
  the parent directory. Eliminates the failure mode where a 0-byte `*.parquet`
  file left by a crashed/killed write would brick subsequent loads.
- **Defensive checkpoint load** — `load_all_events` and `load_events_for_tenant`
  log and skip per-file errors instead of aborting the whole boot. A single
  corrupt checkpoint can no longer take the store offline.
- **Boot-time quarantine** — `cleanup_partial_writes` now also detects 0-byte
  `*.parquet` files and renames them to `<orig>.parquet.corrupt-<unix-ts>` so
  operators can recover/inspect them without blocking startup.
- 4 new regression tests cover the atomic write, partial-rename behaviour, and
  the corrupt-quarantine path.

## [0.20.0] - 2026-04-28

### Added

#### Per-tenant data strategy (Core)

- **Tenant-partitioned on-disk layout** — events are written to
  `data/<tenant>/...` instead of a single flat tree. New `TenantPath` encoding
  threads tenant identity through every read/write.
- **One-shot flat → tenant migration tool** plus a Fly entrypoint hook that
  applies the migration on first boot of a v0.20.x container.
- **Lazy tenant loading** — boot is now O(1). `TenantLoader` +
  `ensure_tenant_loaded` singleflight pulls a tenant's checkpoints into the
  in-memory cache on first query rather than at startup.
- **LRU + memory-budget cache eviction** — per-tenant memory tracking, evict
  on budget pressure, explicit `evict_tenant` API. Multi-tenant stress test
  added.
- **Per-tenant snapshot compaction** with per-tenant retention TTL applied
  during compaction.
- **Bounded WAL replay** — runtime checkpoint loop bounds replay work after a
  crash.
- **Cold-tier archive trait** with a local-filesystem backend, ready for S3/GCS
  backends to be plugged in.

#### Regional independence

- `home_region` tenant attribute + tenant-pinned routing in Control Plane
  (Steps 1+2 of the active-active proposal). Design doc:
  `docs/proposals/regional-independence-tenant-pinned-active-active.md`.

#### Auth

- **`better-auth-allsource` 0.14.5 published to crates.io** — switches the
  client from `X-API-Key` to `Authorization: Bearer`, unblocking customers
  whose Control Plane traffic was 401-ing.

### Changed

- Web: blog card padding between cover image and meta row.
- Web: `react` / `@types/react` versions pinned to fix the admin Vercel build.

### Internal

- 22 Rust clippy fixes across persistence (cast_lossless, manual_let_else,
  doc_list_indent, redundant closures, etc.); 2 Go gosec/errcheck nolint
  annotations for known-safe paths.

## [0.19.2] - 2026-04-17

### Added

- **Event-sourced status page** — Control Plane heartbeat monitoring replaces
  Vigil. Web reads the event-sourced feed at `/status`.
- **Custom domain mapping** — `api.all-source.xyz` documented in the runbook.
- Control Plane data-plane delegation layer for chronis + SDK traffic, with
  per-request JWT minting and a cache for locally-minted `ask_` keys.
- Catch-all Prime delegation route on Control Plane.

### Changed

- **Core is internal-only** — gateway-first tenant resolution, narrower auth
  surface. Public auth always terminates at Control Plane.
- chronis default remote URL now points at `api.all-source.xyz`.

### Fixed

- chronis: URL-encode `since=` timestamp in sync HTTP client.
- Core/Prime: Fly internal-network compatibility.
- Web: dashboard API-key UX cleanup; status link added.
- Build: `set-version` regex handles 3-segment versions; Docker stubs
  `sdks/rust/examples/asset_projection.rs` to keep the Core image lean.

## [0.19.1] - 2026-04-17

### Added

- **Rust SDK `ProjectionWorker`** (#155) — full projection lifecycle in the
  client: `EventStreamClient` over Core's WebSocket, `ProjectionWorker` builder
  + event loop, `ProjectionHandle` (start/stop/reconnect), state push-back to
  Core KV, integration tests against a live Core, and an `asset_projection`
  example. New `ws` and `projection-worker` feature flags.
- Custom-projections use-case guide (`docs/use-cases/custom-projections.md`).
- Reference x402 endpoint at `/api/v1/agent-echo`; Pro-tier gate applied to
  `AgentAutoPayMiddleware`.

### Changed

- **Pro tier wired across Control Plane, Core, Query Service.** Query Service
  `subscription_tier` enum aligned with `tenant.ex`.
- Web is on Vercel, not Fly — `apps/web/fly.toml` removed; CLAUDE.md and skills
  updated to reflect the actual deployment topology.
- Codified SDK-only release tag convention: `sdk-<lang>-v<version>` rather than
  whole-monorepo `v<version>`.

### Fixed

- Control Plane: whitelist `/x402/*` paths in `AuthMiddleware`; set short
  `tenant_id` context key alongside the long form.

> **Note:** v0.19.0 is reserved for `sdk-rust-v0.19.0`, the SDK-only release
> that introduced `ProjectionWorker`. The monorepo went v0.18.2 → v0.19.1.

## [0.18.2] - 2026-04-14

### Changed

- Drop `x86_64-apple-darwin` from the Prime binary build matrix.
- Proposal #130: PRD for read-only Core against a foreign data directory.

## [0.18.1] - 2026-04-14

### Fixed

- Prime binary build: x86_64-apple-darwin target install step.

## [0.18.0] - 2026-04-14

### Added

- **Live reload for chronis TUI and web** — WebSocket-based, with in-process
  and WAL-tail backends.
- `ALLSOURCE_AUTH_DISABLED` alias for local auth-free mode (Core).
- Control Plane `ADMIN_EMAILS` allowlist grants `RoleAdmin` in signed JWTs.

### Fixed

- Core: validate paths at trust boundaries to prevent traversal.
- MCP: clear error when `CORE_MODE=embedded` runs on a remote build.
- MCP embedded transport: add Content-Length framing; harden WAL replay
  (#144).
- Blog: sanitize slug to prevent path traversal; upgrade undici to 8.x (#143).
- Auth: returning-user login no longer fails with `tenant-already-exists`;
  preserve auth header on redirects; fix web proxy path prefix.
- Control Plane: derive tenant ID from email so Core accepts it.
- UX: lead with Prime / agent memory throughout the marketing site.

## [0.17.3] - 2026-03-30

### Added

#### x402 + agent payments

- **Self-hosted x402 facilitator** with verify/settle and nonce replay
  protection (US-004).
- **CDP auto-pay** + remote facilitator that delegates to Coinbase by default.
- x402 payment middleware for gin routes (US-005); MCP x402 transport, quota
  gate, agent payments API, `QueryEvents` (US-005, 008, 009, 010).
- Quota-gated x402 flow — free tier first, pay-per-use on overflow (US-008).
- Agent self-registration endpoint + clean-architecture refactor on Control
  Plane.

#### Tenant isolation hardening

- Enforce `tenant_id` filtering in the event store query engine (security
  fix); verified end-to-end on signup + sync pagination (#128 prerequisite).
- `OptionalAuth` extractor used for tenant enforcement; `State` extractor moved
  to last position.

#### Self-service

- `allsource-onboard` skill for account + sync setup.
- v1.0 roadmap with rock/sand/water estimates; self-service onboarding gap
  analysis covering tenant isolation, auth, billing, and pagination.

## [0.17.2] - 2026-03-26

### Added

- **Event-sourced billing via Core**, with billing architecture docs.
- **better-auth service** with AllSource adapter — full auth migration (Rust +
  AllSource, no Postgres needed).
- Embedded Prime integration guide.
- `fly-status` skill for Fly.io deployment health reports.

### Changed

- Query Service: remove LemonSqueezy billing code (now owned by Control
  Plane).
- Admin: match web login styling — DotPattern + BlurFade.
- TS 6.0 compat: remove deprecated `baseUrl` from web/admin/ui tsconfigs.
- Bump Elixir 1.17 → 1.18 across CI, Dockerfiles, and the security workflow.

### Fixed

- SEO: `robots.txt`, sitemap, canonical URLs, metadata; refresh stale stats.
- Web: OG edge function size limit; turbo env-var warning.
- Security: bump grpc and undici for CVEs (#126, #124).
- E2E: API Keys dialog button unreachable — testid + scrollable card.
- chronis: v0.6.2 — sync no longer un-archives beads (#125).

## [0.17.1] - 2026-03-24

### Fixed

- MCP auth fix (#109).
- Go SDK Go 1.26 compatibility (#116).
- CSP headers (#123).
- Web: demo login broken — proxy `/api/v1/demo/start` to Control Plane;
  Recharts v3 Tooltip formatter type; TS2532 in onboarding test.
- CI: Prime added to Docker Build and Release workflows; ignore transitive
  dep advisories in security scan.

### Added

- Recall-bench improvements + benchmark results.
- Media production and SEO checklist for marketing readiness.

## [0.17.0] - 2026-03-23

### Added

#### AllSource Prime — unified agent memory engine

- **Prime**: graph store + vector index + temporal recall, served as MCP
  tools and over HTTP. New `apps/prime-mcp` workspace.
- **Prime ecosystem integration**: CI, Docker, Core HTTP proxy.
- **Web Demo Zone**: Prime Interactive Playground.
- Full Prime documentation, blog posts, marketing pages, and deployment
  config.

### Changed

- chronis v0.6.0: HTTP sync, remote mode, `CoreBackend` abstraction.
- chronis v0.6.1: UTF-8 truncation panic fix; API key auth for sync.
- chronis v0.5.3: adapt to allsource-core 0.16.0 API change.
- Workspace: `unsafe_code = "deny"` lint, dep cleanup.

### Fixed

- Query Service: Elixir tests fail in community mode.
- Rust: clippy lint fixes for `core_nif`, SDK client, and SDK tests.
- Prime Dockerfile: copies workspace root for dependency resolution; uses
  `rust:slim` runtime base to match glibc version.

## [0.16.0] - 2026-03-14

### Added

- **Open-core licensing** with feature gating and dual Docker builds (community
  vs. enterprise).
- `allsource-mcp` prepared for crates.io publishing (closes #108).

### Fixed

- Shorten keyword to satisfy crates.io's 20-char limit.

## [0.15.0] - 2026-03-08

### Added

#### Admin Dashboard (`apps/admin/`) — NEW
- **Full admin dashboard app** (Next.js, port 3001) for platform operations
- **Tenant management**: list with search/filter/pagination, detail view with quotas, members, audit log, suspend/unsuspend
- **Monitoring dashboard**: stat cards (uptime, events/sec, latency p99, error rate, active tenants), throughput/error-rate charts with configurable time ranges, cluster health with member roles and replication lag, 30s auto-refresh
- **Billing management**: MRR/ARR/growth/churn metrics, revenue trend charts, invoice list with status filtering, refund processing with confirmation dialog, dunning (failed payments) tracking
- **Alert rules**: CRUD for alert rules (metric, operator, threshold, duration, notification channel)
- **SLO management**: create/delete SLOs with compliance gauges, error budget tracking, sparkline trends
- **Security**: IP allowlist/blocklist CRUD, API token audit log with tenant filtering, suspicious activity detection (excessive auth failures, rate limit violations, token abuse, geo anomalies, query anomalies)
- **Auth**: JWT cookie (`admin_token`) with `role: "admin"` claim, OAuth callback (Google, GitHub, email), middleware-enforced on all protected routes
- **E2E tests (mock)**: 53 Playwright tests across 8 spec files using `page.route()` mocks for fast CI
- **E2E tests (real stack)**: 25 Playwright tests in `e2e/real/` against a real Docker stack — isolated `docker-compose.e2e.yml` (Core:3090, Control Plane:3091, Admin:3011), HS256 JWT generator for auth seeding, global setup with health checks and data seeding
- **Docker**: multi-stage Dockerfile (Bun builder → Node.js runtime, port 3001)

#### Control Plane (`apps/control-plane/`)
- **Admin route group** (`/api/v1/admin/*`) with `AdminAuthMiddleware` (JWT-based, requires `role: "admin"`)
- **Tenant admin endpoints**: `GET /admin/tenants` (list with search/filter/pagination), `GET /admin/tenants/:id` (detail with quotas, members, subscription, audit log), `GET /admin/tenants/:id/usage` (daily breakdown), `PUT /admin/tenants/:id/quotas`, `POST /admin/tenants/:id/suspend|unsuspend`, `POST /admin/tenants/bulk`
- **Admin billing endpoints**: `GET /admin/billing/revenue` (MRR, ARR, growth rate, churn rate, trend), `GET /admin/billing/invoices` (list with status filter), `POST /admin/billing/refund`, `GET /admin/billing/dunning`
- **Alert rules**: CRUD at `/admin/alerts` with in-memory repository
- **SLOs**: CRUD at `/admin/slos` with compliance calculation and in-memory repository
- **Security endpoints**: CRUD `/admin/security/ip-rules`, `GET /admin/security/token-audit` (query + summary), `GET /admin/security/suspicious-activity` (anomaly detection)
- **Domain entities**: `AlertRule`, `IPRule`, `SLO` with validation and unit tests
- **LemonSqueezy client enhancements** for billing integration
- Comprehensive test coverage: admin middleware, tenant handler, billing handler, IP rules, suspicious activity, token audit, alert rules, SLOs, quota updates

#### Query Service (`apps/query-service/`)
- **Admin metrics endpoints**: `GET /api/admin/metrics/summary` (aggregated cluster metrics), `GET /api/admin/metrics/timeseries` (chart data), `GET /api/cluster/members` (health and roles)
- **MetricsCache** GenServer: 30s TTL cache for Core `/metrics` Prometheus scraping
- **PrometheusParser**: Extracts counters/gauges/histograms from Core's Prometheus text format
- Tests for cache TTL behavior and Prometheus text parsing

#### Web Dashboard (`apps/web/`)
- **Onboarding wizard rewrite**: All SDK snippets verified against actual SDK source code — Rust (`allsource` crate, `CoreClient`/`QueryClient`/`IngestEventInput`/`QueryEventsParams`), Go (`allsource-go`, `New`/`Ingest`/`Query`/`QueryOptions`), TypeScript (`@allsource/client`, `ingestEvent`/`queryEvents` with snake_case), Python (`allsource-client`, `AllSourceClient`/`ingest`/`query`)
- **Vitest test suite**: 30 tests covering install commands, send/query snippet correctness, step navigation, Run It/Try It API calls
- **Billing plan cards**: tier ranking system (free/growth/enterprise) for context-aware upgrade/downgrade/current buttons; free plan shows "Cancel via Manage Subscription" when user is on paid plan

#### UI Library (`packages/ui/`)
- New components: `Dialog`, `Select`, `Table`, `Tabs` (Radix-based)

#### Chronis (`apps/chronis/`)
- Best practices documentation (gitignore rules, sync exchange files)

## [0.14.1] - 2026-03-05

### Added
- **WAL-backed consumer cursors**: `ConsumerRegistry` now persists cursor positions as system events (`_system.consumer.registered`, `_system.consumer.ack_updated`, `_system.consumer.deleted`) via `SystemMetadataStore`. On Core restart, consumer state is rebuilt from WAL during Stage 2 bootstrap. Previously, all cursor positions were lost on restart.
- **`Consumer` system domain**: New `SystemDomain::Consumer` variant with `consumer_events` module in `system_stream.rs`
- **Dual-mode `ConsumerRegistry`**: `new()` for in-memory (tests/backward compat), `new_durable(system_store)` for WAL-backed persistence
- **`ConsumerRegistry::count()`**: Returns number of registered consumers, used in bootstrap logging
- **`EventStore::set_consumer_registry()`**: Allows replacing the default in-memory registry with a durable one during startup
- 4 new durability tests: `test_durable_register_persists`, `test_durable_ack_persists`, `test_durable_recovery_multiple_acks`, `test_in_memory_mode_still_works`

### Changed
- `SystemBootstrap` Stage 2 now creates and recovers `ConsumerRegistry` alongside tenants, audit, and config repositories
- `SystemRepositories` struct includes `consumer_registry: Arc<ConsumerRegistry>`
- `ServiceContainer` and `ContainerBuilder` thread `consumer_registry` through DI layer
- System metadata bootstrap moved before `Arc::new(store)` in `main.rs` to allow `&mut` access for `set_consumer_registry`
- `SystemDomain::all()` now returns 6 domains (was 5)

## [0.14.0] - 2026-03-05

### Added
- **Optimistic concurrency control**: Per-entity version tracking in EventStore, `expected_version` field on event ingestion with 409 Conflict on mismatch, `entity_version` in query responses
- **Durable subscriptions**: Consumer registration (`POST /api/v1/consumers`), pull-based event polling with cursor tracking, acknowledgment with durable position persistence (survives restart)
- **WebSocket durable subscription**: `WS /ws?consumer_id=X` auto-replays missed events since last ack before switching to real-time delivery
- **Server-side event filtering**: WebSocket `subscribe` message with prefix filters (`scheduler.*`, `trade.*`), RESP3 `SUBSCRIBE scheduler.* index.*` with server-side prefix matching
- **JWT `is_demo` claim**: Control plane includes `is_demo` in JWT claims based on Core tenant data; Query Service reads from JWT instead of brittle tenant ID prefix matching

### Changed
- **Unified tenant management**: Retired duplicated `TenantManager` (application layer); all tenant operations now use `EventSourcedTenantRepository` via DI container as single source of truth
- Updated admin CLI and integration tests to use domain `Tenant` entity and `InMemoryTenantRepository`

## [0.13.1] - 2026-03-03

### Fixed
- **WAL recovery checkpoint bug** (Issue #84): `EventStore::with_config` recovery path now buffers WAL events into Parquet's `current_batch` before flushing. Previously `flush_storage()` was a silent no-op (empty batch), then the WAL was truncated — leaving events only in volatile memory, lost on next restart.

### Added
- **`EmbeddedCore::durability_status()`**: New API that compares in-memory, WAL, and Parquet layers, returning per-layer counts and warnings when data exists only in volatile memory.
- **Real durability data in MCP tools**: `wal_status`, `storage_stats`, and `health_deep` tools now return actual WAL/Parquet metrics (entries, bytes, file count, pending batch) instead of stub `total_events` from in-memory store. `health_deep` reports `"degraded"` with warnings when data is not durable.
- **`DurabilityStatus` struct**: Exported from `allsource_core::embedded` with `memory_events`, `wal_enabled/entries/bytes/sequence`, `parquet_enabled/files/bytes/pending_batch`, `durable` flag, and `warnings` vec.
- **Regression test**: `test_wal_recovery_checkpoints_to_parquet` — 3-session test (ingest → crash → reopen → verify → reopen → verify) proving events survive restart via Parquet checkpoint.
- **NIF `nif_durability_status`**: Full durability status exposed through Rustler NIF to Elixir MCP server.

## [0.12.0] - 2026-03-01

### Added
- **Network sync transport**: `SyncClient` for HTTP pull/push bidirectional sync with `POST /api/v1/sync/pull` and `/api/v1/sync/push` endpoints; `EmbeddedCore::version_vector()`, `events_for_sync()`, `receive_sync_push()`
- **Configurable conflict resolution**: `MergeStrategy` enum (AppendOnly, LastWriteWins, FirstWriteWins) with per-entity-type prefix matching via `Config::merge_strategy()`
- **MCP tool event emission**: `McpToolTracker` with `emit_tool_result()`, `emit_tool_error()`, `track()` with auto-timing feeding `ToolCallAuditProjection`
- **WebSocket backpressure**: `WebSocketConfig` with batching, `max_batch_size`, lagged notification (backward compatible — no config = original behavior)
- 20 new tests (1489 lib tests total)

### Fixed
- E2E test health check gate and shared `demoLogin` fixture extraction

## [0.11.0] - 2026-03-01

### Added
- **Embedded Core library API** (Issue #73): 8-phase implementation with 83 tests covering `EmbeddedCore` facade, bidirectional sync, CRDT conflict resolution, replicant worker protocol, AI projection templates, and TOON format
- **Dashboard E2E test suite** with tenant auto-provisioning
- **Projection backfill**: `register_projection_with_backfill()` replays existing events through newly registered projections
- **SDK quickstart example**: `apps/core/examples/embedded_quickstart.rs`
- **ADRs**: Architecture Decision Records for embedded core, crash-safe compaction, batch ingestion, projection backfill
- **Concurrency tests**: 10-task concurrent writer test (1000 events) and concurrent reader+writer test
- **Crash recovery test**: Write → shutdown → reopen → verify events survive via WAL

### Fixed
- **14 principal engineer review findings**: crash-safe compaction (WAL append inside write lock), true batch ingestion (single lock acquisition), NaN panics, floating-point drift, TOCTOU races, projection guards
- **Cross-tenant compaction guard**: Token compaction validates all events share the same `tenant_id`
- **TOON error handling**: `query_toon` propagates decode errors instead of silently returning empty results
- **Deterministic sync tests**: Replaced sleep-based timing with HLC node_id tie-breaking
- **fastembed** `embed()` API change (now requires `&mut self`)
- **jsonwebtoken v10** CryptoProvider requirement
- **criterion::black_box** deprecation in benchmarks

### Changed
- All Rust dependencies upgraded to latest (arrow 57, datafusion 52, rand 0.10, reqwest 0.13, tantivy 0.25, fastembed 5, jsonwebtoken 10, criterion 0.8)
- Removed unused dependencies: `axum-extra`, `jsonschema`, `testcontainers`, `testcontainers-modules`
- Comprehensive documentation audit and cleanup for v0.10.0+ architecture

## [0.10.7] - 2026-02-26

### Added
- **Query ergonomics**: `event_type_prefix` query parameter for prefix-based event type filtering (e.g., "index." matches "index.created", "index.updated")
- **Payload filtering**: `payload_filter` query parameter — JSON key-value matching against event payloads (e.g., `{"user_id":"abc-123"}`)
- **Duplicate entity detection**: `GET /api/v1/entities/duplicates` endpoint — groups events by payload fields and identifies entities with duplicate values
- **Consumer patterns guide**: `docs/current/QUERY_PATTERNS.md` — best practices for pagination, saga orchestration, uniqueness checks, and MCP consumers
- **Demo zone**: interactive demo portal with one-click account provisioning and guided feature showcases
- `Default` derive for `QueryEventsRequest` for ergonomic struct construction

### Fixed
- **Control plane OAuth proxy**: stopped forwarding Host header to prevent Fly.io misrouting
- **Service JWT auth**: Core requests from Control Plane now authenticated with service JWT
- **OAuth callback URL**: uses `FRONTEND_URL` for callback base URL instead of request host
- **External OAuth calls**: uses plain HTTP client (no service auth headers) for provider API calls

## [0.10.6] - 2026-02-19

### Fixed
- **Query DSL crash** (GitHub #65): `POST /api/query` returned HTTP 500 when `%Query{}` struct fell through to Access-based `is_map` clause in `maybe_add_next_link/3` — added explicit `%Query{}` catch-all clause
- **AUTH_DISABLED bypass**: dev mode now short-circuits before header inspection, so malformed `Authorization` headers no longer cause 401 when auth is disabled
- **Domain references**: replaced all `allsource.co` references with `all-source.xyz` across SDKs, registry, CI, and docs
- **Go lint issues**: extracted OAuth provider constants, added `stringFromMap`/`boolFromMap` helpers to satisfy errcheck, gosec nolint for URL constants
- **SIMD filter test threshold**: lowered flaky throughput assertion from 1M to 100K events/sec for CI stability

## [0.10.5] - 2026-02-17

### Added
- **Server-side projections**: Query Service projection engine with behaviour, registry, fold pipeline, and 5 projection modules (IndexState, TradeState, PortfolioState, SagaState)
- **Fold-on-read endpoint**: `POST /api/query/projected` — returns projected entity state instead of raw events, with snapshot-aware folding, server-side filtering, and pagination
- **Continuous projections**: ProjectionServer (PubSub subscription) + DynamicSupervisor for real-time materialized read models via ETS with fold-on-read fallback
- **Rust SDK** (`allsource` crate): typed client with circuit breaker, retry logic, and client-side fold helpers — published to crates.io
- Monorepo structure best practices document (`docs/MONOREPO_STRUCTURE.md`)
- Design proposal and 8 use cases for server-side projections

### Fixed
- **Wire format standardization**: all QS list endpoints now return consistent `{data, count, total}` — fixed snapshot controller mapping `total` → `count`, standardized webhook and replay controllers
- **MCP client resilience**: disabled Hackney connection pooling in CoreClient and ControlPlaneClient to prevent stale connection errors after Core restarts; increased retry delays (500ms base, 5s max)
- **Core persistence wiring** (from v0.10.4): `main.rs` now reads env vars and constructs `EventStoreConfig::production(...)` — all prior Docker images ran in-memory only

### Changed
- Consolidated all SDKs under `sdks/` — moved TypeScript client from `packages/client` to `sdks/typescript`, Rust SDK from `packages/rust-client` to `sdks/rust`
- Removed legacy duplicate SDKs from `packages/` (go-client, python-client)
- `packages/` now contains only shared internal packages (ui component library)
- Updated workspace Cargo.toml and bun workspace to reflect new SDK paths

## [0.10.4] - 2026-02-17

### Fixed
- Core persistence wiring: `main.rs` reads `ALLSOURCE_DATA_DIR` / `ALLSOURCE_WAL_DIR` / `ALLSOURCE_STORAGE_DIR` env vars and constructs `EventStoreConfig::production(...)` when persistence dirs are available
- Added durability test script (`tooling/durability-test/test-durability.sh`)

## [0.10.3] - 2026-02-16

### Changed
- Migrate all domains to all-source.xyz (web, API, docs, emails)
- Archive incorrect PostgreSQL docs, rewrite C4 architecture diagrams

### Fixed
- Rust: clippy, fmt, sort, doc links
- Go: golangci-lint (goconst, gocritic)
- Elixir: credo --strict, format, dialyzer, unused deps, test fixes
- Fix Makefile set-version target

## [0.10.1] - 2026-02-15

### Changed
- Remove PostgreSQL dependency from Query Service (now a stateless API gateway)
- Add JWT/API key auth, tenant cache, and usage reporter to Query Service

### Added
- Billing, webhook, and HAL support in Control Plane
- LemonSqueezy webhook integration for subscription management
- OpenAPI spec for Control Plane (920-line spec)

## [0.10.0] - 2026-02-14

### Changed
- Remove all PostgreSQL repositories from Control Plane — Core is the single source of truth
- Replace pg_tenant, pg_audit, pg_config, pg_operation, pg_policy, pg_user with Core-backed repositories

### Added
- Core: POST/GET /api/v1/audit/events — audit event logging and querying
- Core: GET/POST/PUT/DELETE /api/v1/config — dynamic config management
- CoreClient with connection pooling, retries, and OpenTelemetry tracing
- Leader-follower replication via WAL shipping

### Fixed
- 68 golangci-lint issues across Control Plane
- Rust clippy and cargo fmt in new API files

## [0.9.1] - 2026-02-12

### Added
- Core: GET /api/v1/streams — list all streams (entity IDs) with event counts
- Core: GET /api/v1/event-types — list all event types with usage statistics
- Core: POST /api/v1/events/batch — batch event ingestion
- Query Service: list_streams and list_event_types integration
- Web: useStreams() and useEventTypes() React hooks
- Time travel context and picker components
- Core replication design proposal

## [0.9.0] - 2026-02-12

### Added
- MCP Server: 8 event management tools (delete, archive, restore, export, import, clone, merge, split entities) with dry-run preview and audit trails — 27 tools total, 309 tests
- Web: redesigned login/signup pages with WCAG 2.1 accessibility
- Docker Compose override for isolated local development (ports 3900-3908)
- Version management commands in Makefile (set-version, bump-version)

### Changed
- Query Service: added cmake for NIF compilation, APM/analytics, cluster support, WebSocket channels, OpenAPI spec, Prometheus metrics

### Fixed
- Web Dockerfile for monorepo standalone builds
- Consistent versioning at 0.9.0 across all services
