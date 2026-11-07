# Changelog

All notable changes to the Go Control Plane will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **Legacy key sunset signal**: while `ALLSOURCE_LEGACY_KEY_SUNSET` is in the future, a response served with an unregistered managed onboarding key carries `Deprecation: @1794009600` (RFC 9745, 2026-11-07) and `Sunset: <HTTP-date of the sunset>` (RFC 8594). Refusals, registered client keys and dashboard `ask_` keys carry neither.

### Documentation
- README lists `ALLSOURCE_LEGACY_KEY_SUNSET`, `ALLSOURCE_ASSERTION_AUDIENCE` and the operator-only client-key routes under `/api/v1/admin/tenants/:id/client-keys`.

## [0.10.7] - 2026-02-26

### Fixed
- **OAuth proxy**: stopped forwarding Host header to prevent Fly.io misrouting
- **Service JWT auth**: Core requests now authenticated with service JWT instead of anonymous
- **OAuth callback URL**: uses `FRONTEND_URL` for callback base URL
- **External OAuth calls**: uses plain HTTP client (no service auth headers) for provider API calls
- **Go lint**: fixed staticcheck SA5011 nil pointer in cache_test.go, errcheck for godotenv.Load

### Added
- Demo account provisioning endpoint (`POST /api/v1/demo/start`)

## [0.10.6] - 2026-02-19

### Fixed
- **Go lint**: extracted `providerGitHub`/`providerGoogle` constants (goconst), added `stringFromMap`/`boolFromMap` helpers (errcheck), `//nolint:gosec` for OAuth URL constants

### Added
- OAuth login flow for GitHub and Google providers with CSRF state cookie protection
- `isSecureContext()` derives HTTPS from `FRONTEND_URL` scheme for correct cookie flags behind reverse proxies

---

## [0.2.0] - 2026-02-03

### Added

#### Performance Optimizations (US-026, US-027, US-028)

**Connection Pooling** (US-026)
- HTTP client connection pooling for Core communication
- Configurable pool size and idle timeout
- Connection reuse for reduced latency
- Health checking for pooled connections

**Response Caching** (US-027)
- `ResponseCache` with TTL-based expiration
- Cluster status caching (30s TTL)
- Metrics caching (10s TTL)
- Prometheus metrics for cache hits/misses
- Thread-safe `sync.Map` implementation

**Async Audit Logging** (US-028)
- Buffered channel (10,000 capacity) for async processing
- Background goroutine for log consumption
- Graceful backpressure handling with timeout
- Non-blocking audit writes in request path
- Clean shutdown with event draining

### Changed
- Updated Go to 1.24
- Improved error handling in API endpoints
- Enhanced metrics collection

### Performance
- API latency: <10ms p99
- Connection pool hit rate: >95%
- Throughput: 5K+ req/sec

---

## [0.1.0] - 2025-10-21

### Added

#### Foundation - Complete

**Authentication & Authorization**
- JWT authentication client
- Role-based access control (RBAC)
- Policy engine with 5 default policies

**Observability**
- Comprehensive audit logging
- Prometheus metrics integration
- OpenTelemetry tracing (Jaeger)
- Health checks and cluster status

**API**
- RESTful management API (12 endpoints)
- Clean Architecture implementation
- Domain layer with entities and repository interfaces
- Application layer with use cases and ports
- Infrastructure layer with concrete implementations

**Quality**
- Dependency injection with Google Wire
- 95%+ test coverage
- SOLID principles compliance

### Performance
- API latency: <15ms p99
- Throughput: 3K+ req/sec

---

## Version History

| Version | Date | Status | Highlights |
|---------|------|--------|------------|
| [0.2.0] | 2026-02-03 | Current | Connection Pooling, Performance |
| [0.1.0] | 2025-10-21 | Stable | Foundation, Auth, Observability |
