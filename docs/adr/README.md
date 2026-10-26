# Architecture Decision Records

This directory contains Architecture Decision Records (ADRs) documenting significant technical decisions in the AllSource Chronos project.

## Index

| ADR | Title | Status | Date | Release |
|-----|-------|--------|------|---------|
| [001](001-embedded-core-library.md) | Embedded Core Library API | Accepted | 2026-02-28 | unreleased |
| [002](002-crash-safe-compaction.md) | Crash-Safe Token Compaction | Accepted | 2026-03-01 | unreleased |
| [003](003-batch-ingestion-single-lock.md) | Batch Ingestion with Single Lock | Accepted | 2026-03-01 | unreleased |
| [004](004-projection-backfill.md) | Projection Backfill on Registration | Accepted | 2026-03-01 | unreleased |
| [005](005-postgresql-removal-event-sourced-metadata.md) | PostgreSQL Removal, Event-Sourced Metadata | Accepted | 2026-02-14 | v0.10.0 |
| [006](006-server-side-projections-fold-on-read.md) | Server-Side Projections (Fold-on-Read) | Accepted | 2026-02-17 | v0.10.5 |
| [007](007-domain-value-objects.md) | Domain Value Objects for Type Safety | Accepted | 2026-02-02 | v0.9.1 |
| [008](008-vector-search-fastembed.md) | Vector Search with fastembed | Accepted | 2026-02-03 | v0.10.0 |
| [009](009-simd-json-zero-copy-deserialization.md) | simd-json Zero-Copy Deserialization | Accepted | 2026-02-03 | v0.10.0 |
| [010](010-native-arm64-ci.md) | Native ARM64 CI Runners | Accepted | 2026-02-03 | v0.10.0 |
| [011](011-prime-entity-id-enum.md) | Typed EntityId Enum for Prime | Accepted | 2026-03-20 | unreleased |
| [012](012-prime-directed-adjacency.md) | Unified DirectedAdjacencyProjection | Accepted | 2026-03-20 | unreleased |
| [013](013-prime-atomic-deletion.md) | Atomic Node Deletion via Batch Ingestion | Accepted | 2026-03-20 | unreleased |
| [014](014-prime-facade-decomposition.md) | Facade Decomposition into Sub-API Modules | Accepted | 2026-03-20 | unreleased |
| [015](015-prime-vector-index-generation.md) | Generation Counter for VectorIndexProjection | Accepted | 2026-03-20 | unreleased |
| [016](016-prime-eventual-consistency.md) | Eventual Consistency Model for Prime | Accepted | 2026-03-20 | unreleased |
| [017](017-chronis-prime-integration.md) | Chronis Prime Integration | Accepted | 2026-03-26 | unreleased |
| [018](018-embedded-eager-parquet-hydration.md) | Eager Parquet Hydration on Embedded Boot | Accepted | 2026-05-30 | chronis v0.7.2 |
| [021](021-views-folded-by-query-service.md) | Views Folded by the Query Service, Stored in Core | Accepted | 2026-10-27 | unreleased |

## Format

Each ADR follows the structure:
- **Context**: What problem are we solving?
- **Decision**: What did we decide and why?
- **Consequences**: What are the trade-offs?

ADRs are immutable once accepted. If a decision is reversed, a new ADR supersedes the old one (with a `Superseded by` link).
