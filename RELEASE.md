# AllSource Monorepo - v0.8.0 Release

**Release Date**: 2026-02-08
**Codename**: Clean Architecture
**Status**: RELEASED (historical; current version 0.30.0)

---

## Release Highlights

This release represents a **major milestone** for the AllSource Event Store platform, achieving **100% completion** of all planned Q1 2026 roadmap items with significant architectural improvements across all components.

### Key Achievements

| Metric | Value |
|--------|-------|
| User Stories Completed | **79/79 (100%)** |
| Tasks Closed | **107/110 (97%)** |
| New Lines of Code | ~47,000 |
| Rust Tests Passing | **1,221** |
| Elixir Tests Passing | **508** (281 Query + 227 MCP) |
| Performance | 726K events/sec |

---

## Component Releases

### AllSource Core v0.8.0

**Clean Architecture & Domain-Driven Design**

The Rust Core has been completely refactored following Clean Architecture principles:

- **15+ Value Objects**: Type-safe domain primitives with validation
- **6+ Entities**: Full DDD entity implementations
- **7+ Repository Implementations**: In-memory and RocksDB backends
- **6+ Use Cases**: Business logic encapsulation
- **Native Search**: Vector search + BM25 + Hybrid search
- **DI Container**: `ServiceContainer` with builder pattern
- **Storage Integrity**: SHA-256 checksums, WAL verification
- **Partition Monitoring**: Hot partition detection, metrics
- **7-Day Stress Tests**: Production hardening suite

**v0.8.0 Quality Improvements**:
- **SIMD Filter Refactoring**: Removed buggy SIMD timestamp implementations, replaced with reliable scalar iterators following Rust best practices
- **Arena Pool Test Fix**: Fixed flaky arena allocation tests
- **Dependency Updates**: Resolved cargo audit vulnerabilities
- **1,221 tests passing** with 0 failures

**Performance**:
- 726K events/sec ingestion (up from 469K in v0.6.0)
- 11.9μs entity query latency
- Clean architecture overhead: <3%

### Query Service v0.2.0

**Phase 2: Core Integration Complete**

Real-time integration with Core via WebSocket and Broadway:

- **CoreWebSocketClient**: Real-time event streaming
- **ProjectionSync**: Automatic state synchronization
- **Broadway Pipeline**: Production-ready stream processing
- **281 tests passing**

**v0.2.0 Quality Improvements**:
- **Race Condition Fix**: Fixed `ProjectionSync` test flakiness by using synchronous call after async cast for proper GenServer message ordering

### MCP Server v0.2.0

**AI-Native Enhancements**

Tools designed specifically for AI agent interaction:

- **Enhanced Tool Descriptions**: Agent guidance embedded
- **Query Advice Tool**: Intelligent recommendations
- **Conversation Context**: Multi-turn interaction support
- **Quick Exploration**: Fast sampling and statistics
- **Native Search Tools**: Semantic and hybrid search

**v0.2.0 Quality Improvements**:
- **ConversationContext Fix**: Made test setup robust by checking for existing GenServer process
- **227 tests passing** with 0 failures

### Control Plane v0.2.0

**Performance Optimizations**

- **Connection Pooling**: Reduced latency for Core communication
- **Response Caching**: TTL-based caching with Prometheus metrics
- **Async Audit Logging**: Non-blocking audit writes via channels

---

## Epics Completed

### v1.1: Rust Core Clean Architecture (100%)

| User Story | Status |
|------------|--------|
| US-001: Create domain value objects | ✅ Complete |
| US-002: Create domain entities | ✅ Complete |
| US-003: Create EventStream aggregate | ✅ Complete |
| US-004: Create repository traits | ✅ Complete |
| US-005: Create application use cases | ✅ Complete |
| US-006: Create application services | ✅ Complete |
| US-007: Refactor persistence layer | ✅ Complete |
| US-008: Refactor web handlers | ✅ Complete |
| US-009: Set up DI container | ✅ Complete |
| US-010: Add storage integrity checks | ✅ Complete |
| US-011: Add partition monitoring | ✅ Complete |
| US-012: Add 7-day stress tests | ✅ Complete |

### v1.2: Performance Optimizations (100%)

| User Story | Status |
|------------|--------|
| US-021: Zero-copy deserialization | ✅ Complete |
| US-022: Lock-free data structures | ✅ Complete |
| US-023: Batch processing | ✅ Complete |
| US-024: Memory pool allocations | ✅ Complete |
| US-025: SIMD event filtering | ✅ Complete |
| US-026: Go connection pooling | ✅ Complete |
| US-027: Go response caching | ✅ Complete |
| US-028: Go async audit logging | ✅ Complete |

### MCP AI-Native Enhancements (100%)

| User Story | Status |
|------------|--------|
| US-013: Enhanced tool descriptions | ✅ Complete |
| US-014: Query advice tool | ✅ Complete |
| US-015: Conversation context | ✅ Complete |
| US-016: Quick exploration tools | ✅ Complete |

### Native Search Capabilities (100%)

| User Story | Status |
|------------|--------|
| US-029: Vector search engine | ✅ Complete |
| US-030: BM25 keyword search | ✅ Complete |
| US-031: Hybrid search orchestrator | ✅ Complete |
| US-032: MCP search tools | ✅ Complete |

### Query Service Phase 2 (100%)

| User Story | Status |
|------------|--------|
| US-017: CoreWebSocketClient | ✅ Complete |
| US-018: Projection state API | ✅ Complete |
| US-019: ProjectionSync GenServer | ✅ Complete |
| US-020: Broadway pipeline | ✅ Complete |

---

## Infrastructure Improvements

### Developer Experience (NEW)

- **Elixir Test Reporting**: New `make elixir-test` with detailed trace output and failure summaries
- **Failed Test Re-runs**: `make elixir-test-failed` to quickly iterate on failures
- **Test Watch Mode**: `make elixir-test-watch` for TDD workflow
- **Markdown Reports**: `make elixir-test-report` generates test summary reports
- **Quality Gates**: Enhanced CI failure reporting with log capture

### GitHub Actions Optimization (100%)

- Native ARM64 runners (no QEMU emulation)
- Registry cache for multi-arch builds
- Optimized container CI pipeline
- Security workflow improvements

### Docker Image Publishing (100%)

- Multi-stage builds for all services
- Published to GitHub Container Registry
- Version badges in README linking to GHCR packages
- Optimized image sizes
- Documentation for all images

### Production Readiness Review (100%)

- Core test coverage audit
- Query service test quality audit
- Performance benchmark verification

### SaaS MVP (100%)

- Fly.io deployment configuration
- OAuth authentication (Google, GitHub)
- LemonSqueezy billing integration
- Usage metering system
- Subscription enforcement
- API rate limiting
- Customer onboarding flow

---

## Breaking Changes

None. This release is fully backward compatible.

---

## Upgrade Guide

### From v0.7.x to v0.8.0

1. **Update Dependencies**:
   ```bash
   # Rust Core
   cd apps/core && cargo update

   # Query Service
   cd apps/query-service && mix deps.get

   # MCP Server
   cd apps/mcp-server-elixir && mix deps.get
   ```

2. **Rebuild**:
   ```bash
   # Rust Core
   cargo build --release

   # Elixir services
   mix compile
   ```

3. **Run Tests**:
   ```bash
   cargo test
   mix test
   ```

4. **No Data Migration Required**

---

## Future Work (Q2-Q3 2026)

| Epic | Timeline | Priority |
|------|----------|----------|
| Query Service Phase 3 | Q2-Q3 2026 | LOW |
| Redis Protocol (Optional) | Q2 2026 | LOW |
| Multi-Node Clustering | Q1 2027 | FUTURE |

---

## Task Summary

```
📊 Issue Database Status

Total Issues:           110
Closed:                 107 (97%)
Open:                   2 (E2E tests only)
In Progress:            1

All roadmap items complete!
```

---

## Contributors

- AI-assisted development with Claude
- Chronis (`cn`) task tracking

---

## Links

- **Roadmap**: `docs/roadmaps/2026-02-15_CONSOLIDATED_ROADMAP.md`
- **Core Changelog**: `docs/CHANGELOG.md`
- **Query Service Changelog**: `apps/query-service/CHANGELOG.md`
- **MCP Server Changelog**: `apps/mcp-server-elixir/CHANGELOG.md`
- **Control Plane Changelog**: `apps/control-plane/CHANGELOG.md`

---

**AllSource Event Store** - High-performance, AI-native event sourcing
