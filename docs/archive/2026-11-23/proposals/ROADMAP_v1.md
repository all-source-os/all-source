# Roadmap to v1.0.0

**Status:** Planning
**Date:** 2026-03-27
**Current version:** 0.17.2

A 1.0 means **semver stability** — the public API won't break without a 2.0. This document uses **Rock / Sand / Water** estimates.

## Rocks (must do — blocking 1.0)

| # | Task | Severity | Estimate | Notes |
|---|------|----------|----------|-------|
| R1 | **Fix #128: Path traversal vulnerability** | Critical | Rock (2-3h) | File path construction uses untrusted data in config.rs, storage_integrity.rs |
| R2 | **Fix #127: Embedded Prime/Recall docs** | Medium | Sand (1h) | Integration guide for embedded use |
| R3 | **Remove deprecated functions** | API hygiene | Sand (30m) | `node_entity_id()`, `edge_entity_id()` → `EntityId::node()`, `EntityId::edge()` |
| R4 | **Resolve TODO in recall/api.rs** | Incomplete | Sand (1h) | Vector search integration in L2 context |
| R5 | **Stabilize Prime API surface** | Design | Rock (4h) | 52 methods on facade — review, document, freeze |

## API Commitments (frozen at 1.0)

These cannot change without a 2.0:

1. **Event schema** — `prime.node.created`, `prime.edge.created`, etc. Stored in WAL.
2. **Feature flag names** — `prime`, `prime-vectors`, `prime-full`, `prime-recall`, `embedded`, `server`
3. **EntityId wire format** — `node:{type}:{id}`, `edge:{id}`, `vec:{id}`
4. **Core HTTP API** — `/api/v1/events`, `/api/v1/prime/*`
5. **EmbeddedCore API** — `ingest()`, `query()`, `projection()`, `shutdown()`
6. **MCP tool names** — `prime_add_node`, `prime_recall`, etc.

## Sand (should do — quality bar for 1.0)

| Item | Current | 1.0 Target | Estimate |
|------|---------|------------|----------|
| Test coverage | 1712 tests | Add Prime HTTP proxy integration tests | Sand (2h) |
| Docs | ADRs + README | rustdoc published, integration guides | Sand (4h) |
| Security | 1 open CVE (#128) | Zero critical/high | Rock (3h) |
| Deprecations | 2 deprecated fns | Removed | Sand (30m) |
| Facade size | 52 methods, 1 file | Split per ADR-014 | Rock (4h) |
| SDK parity | Rust full, TS/Go/Python HTTP-only | Rust + TypeScript with Prime | Rock (1d) |
| Error handling | PrimeError with anyhow fallback | Typed errors, no anyhow in public API | Sand (2h) |
| Changelog | Git log only | CHANGELOG.md with semver sections | Sand (1h) |

## Water (nice to have — not blocking)

- Benchmarks published (recall-bench results in docs)
- Fly.io deployments on latest version
- OpenAPI spec for Prime HTTP endpoints
- Migration guide from 0.x to 1.0

## Timeline

| Phase | Work | Estimate |
|-------|------|----------|
| 1. Security | Fix #128 path traversal | Rock (1 day) |
| 2. API cleanup | Remove deprecations, resolve TODOs, facade split | Rock (2 days) |
| 3. Docs | API reference, integration guide, CHANGELOG.md | Sand (1 day) |
| 4. SDK | TypeScript SDK with Prime support | Rock (1 day) |
| 5. Release | Full CI, tag v1.0.0, publish crates, Docker images | Sand (1 day) |

**Total: ~1 week of focused work.**

Critical path: Security (#128) → API stabilization → Release.
Everything else is parallelizable.

## Decision Log

- 2026-03-27: Assessment created. Priority shifted to team sync setup first (deploy Core v0.17.2, enable `cn sync` for team).
