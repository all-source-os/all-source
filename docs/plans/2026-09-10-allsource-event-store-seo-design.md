# AllSource event-store SEO design

Date: 2026-09-10
Status: approved for implementation

## Decision

Position AllSource first as a purpose-built event store database for event-sourced systems. Core is the durable database and source of truth. Query Service exposes tenant-facing read paths. Prime derives agent memory from Core history; it is an application of the event store, not the product category.

## Homepage hierarchy

1. Lead with the category: event store database built for event sourcing.
2. Name the primary jobs: immutable streams, replay, point-in-time state, projections, schema governance, and durable consumers.
3. Use benchmark numbers only with their measured scope.
4. Present Prime and MCP after database capabilities as proof that the same history supports higher-level applications.
5. Link the homepage and Platform navigation to a practical event-sourcing-pattern hub.

## Search architecture

Create one hub at `/event-sourcing/patterns` and ten bounded, static pattern pages:

- aggregate event streams
- optimistic concurrency
- projections and read models
- snapshots and checkpoints
- event replay
- idempotent consumers
- event schema evolution
- temporal queries
- multi-tenant event streams
- durable subscriptions

Each page must answer one technical query, explain tradeoffs, show an AllSource-specific implementation, list failure modes and production checks, cite authoritative sources where relevant, and link to three related patterns. Pages must be self-canonical, statically generated, independently useful, and included in the sitemap.

## Quality gates

- Exactly ten unique pattern slugs.
- Unique metadata title, description, direct answer, implementation guidance, failure modes, and checklist per page.
- At least 300 words of pattern-specific structured copy per page.
- Metadata titles remain concise and descriptions remain within search-snippet bounds.
- Every related-page reference resolves to another pattern.
- Hub, pages, homepage, navigation, schema, and sitemap use the same event-store category language.
- No logo, brand-color, pricing, or unrelated user-owned file changes.

## Evidence boundary

Use repository-backed capabilities only: append-only event streams, CRC32-checked WAL, Parquet persistence, optimistic concurrency through expected versions, projections, automatic and manual snapshots, schema registration and validation, point-in-time reconstruction, tenant scoping, and durable-consumer checkpoints. Avoid claims about unimplemented distributed consensus or unmeasured performance paths.
