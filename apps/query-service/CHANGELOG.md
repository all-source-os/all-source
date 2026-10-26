# Changelog

All notable changes to the Query Service will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- View folding (latest-state contract, ADR-021): `Views.Reducer` applies declarative view definitions; `Views.ViewFolder` folds each tenant's view from Core's event feed into the `view_<name>` read model; `Views.ViewCatalog` polls `GET /api/v1/admin/views`. Off unless `VIEWS_FOLD_ENABLED=true`.

### Removed
- `GET /api/v1/snapshots` and `POST /api/query/projected`, with `ProjectionSync`, `EventPipeline`, `Projections.ProjectionServer`, `ProjectionSupervisor`, `FoldPipeline` and the Core snapshot and projection-state client calls. Their Core stores carry no tenant key.

## [0.10.7] - 2026-02-26

### Added
- Demo reset scheduler (`DemoReset`) for automatic cleanup of demo accounts
- Config controller for frontend configuration endpoint

## [0.10.6] - 2026-02-19

### Fixed
- **Query DSL crash** (GitHub #65): `POST /api/query` returned HTTP 500 when `%Query{}` struct fell through to the `is_map` clause in `maybe_add_next_link/3` that uses bracket syntax — structs don't implement the Access behaviour. Added explicit `%Query{}` catch-all clause before the generic map clause.
- **AUTH_DISABLED bypass**: `AuthPipeline` now checks `DevMode.auth_disabled?()` first, before inspecting headers. Previously, a malformed `Authorization: Bearer <garbage>` header would trigger JWT validation and reject the request even when auth was disabled.

### Added
- Server-side projections: behaviour, registry, fold pipeline, 5 projection modules (IndexState, TradeState, PortfolioState, SagaState, ProjectionBehaviour)
- Fold-on-read endpoint: `POST /api/query/projected` with snapshot-aware folding
- Continuous projections: ProjectionServer (PubSub subscription) + DynamicSupervisor
- Wire format standardization for all list endpoints

---

## [0.2.0] - 2026-02-03

### Added

#### Phase 2: Core Integration - Complete

**WebSocket Integration (US-017)**
- `CoreWebSocketClient` module using WebSockex
- Real-time event streaming from Core
- PubSub integration for event distribution
- Auto-reconnect with exponential backoff
- Connection state management
- 18+ tests passing

**Projection State Sync (US-019)**
- `ProjectionSync` GenServer for state synchronization
- ETS cache for local reads with sub-millisecond latency
- Automatic restore from Core on restart
- Configurable sync intervals
- Health monitoring and metrics
- 20+ tests passing

**Broadway Pipeline (US-020)**
- Production-ready `CoreProducer` for Broadway
- `EventPipeline` Broadway topology
- Cursor tracking and persistence
- Backpressure handling
- Batch processing with configurable sizes
- Performance benchmarks (10K+ events/sec)
- 56 tests passing

### Changed
- Updated `RustCoreClient` with projection state endpoints
- Enhanced error handling for WebSocket disconnections
- Improved telemetry for Broadway pipeline metrics

### Performance
- WebSocket message processing: <1ms latency
- Projection state reads: <100μs (ETS cache)
- Broadway throughput: 10K+ events/sec
- Total tests: 281 passing

---

## [0.1.0] - 2025-12-01

### Added

#### Phase 1: Foundation - Complete

**Query DSL (54 tests)**
- Fluent query building with Elixir pipes
- Predicates: eq, gt, lt, gte, lte, between, in, not_in
- Time helpers: days_ago, hours_ago, since, until
- Sorting, limiting, field projection

**Projections (61 tests)**
- GenServer-based state management
- OTP supervision for fault tolerance
- Event application to projections
- Snapshot support (in-memory)
- Current/historical state queries

**Event Pipelines (81 tests)**
- 6 operator types: Filter, Transform, Enrich, Validate, Route, Aggregate
- Batch processing
- Statistics tracking
- Error handling

**HTTP Client (34 tests)**
- Tesla-based client to Core
- Connection pooling
- Event CRUD, queries, snapshots
- Error handling

**Phoenix HTTP API (5 tests, 11 endpoints)**
- GET/POST /api/events
- POST /api/query
- GET/POST /api/projections
- GET /api/health, /api/metrics

**Production Readiness**
- Docker with multi-stage build
- Mix releases
- Health checks & metrics
- Environment-based configuration

### Performance
- Query latency: <10ms p95
- Projection updates: <5ms
- Total tests: 281 passing

---

## Version History

| Version | Date | Status | Highlights |
|---------|------|--------|------------|
| [0.2.0] | 2026-02-03 | Current | Phase 2 Core Integration, WebSocket, Broadway |
| [0.1.0] | 2025-12-01 | Stable | Phase 1 Foundation, Query DSL, Projections |
