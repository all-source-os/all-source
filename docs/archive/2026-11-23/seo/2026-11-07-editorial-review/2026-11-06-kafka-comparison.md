# Kafka comparison: review draft

**Status:** Product correction for existing owner review; publication requires separate authorization. Evidence collected 2026-11-06 around 10:05 UTC. Scope: developer-facing event-sourcing content; locale unspecified.

## Evidence and diagnosis

**Observed:** `all-source/apps/web/content/why-event-sourcing-2026.mdx:55–57` calls Kafka a transport and asserts that a specific entity/time query cannot achieve sub-millisecond latency. The local `cqrs-vs-event-sourcing.mdx:77` instead distinguishes Kafka's role from the business source of truth.

**Observed:** Search for `Kafka event sourcing event store vs broker partition stream query entity` returned [Microsoft's event-sourcing guidance](https://learn.microsoft.com/en-us/azure/architecture/patterns/event-sourcing) and [Apache Kafka interactive-query documentation](https://kafka.apache.org/documentation/streams/developer-guide/interactive-queries.html). [Apache's introduction](https://kafka.apache.org/intro/) describes Kafka as an event-streaming platform that stores and processes streams, with retention configured per topic. Kafka Streams exposes queries over application state stores; that does not establish native per-entity event-stream queries or any latency guarantee.

**Inferred:** The absolute transport-only description and unmeasured latency claim can mislead readers comparing event stores and brokers. Replacing them improves technical accuracy. Search results suggest comparison intent; query volume, ranking and traffic impact are unknown. Severity: moderate editorial accuracy; confidence: high on the quoted local copy, medium on production relevance.

## Proposed replacement

> Kafka is an event-streaming platform that stores and distributes records. An event-sourcing design also needs an explicit system of record, a retention and recovery policy, and a way to handle concurrent changes to an entity. Kafka Streams can maintain queryable application state, but the query model and operational responsibilities depend on how that application is built. Evaluate per-entity history, optimistic concurrency and replay requirements against the capabilities of the chosen architecture.

Optional contextual link, after verifying the production target: `[CQRS and event sourcing](/blog/cqrs-vs-event-sourcing)`. The local article covers the distinction; current production availability and existing links were not verified.

## Publication and verification

**Observed:** Web tool could not access either presumed production article URL on this run. This is a tool access failure, not proof of site outage or non-indexation. The installed Rust audit release binary was absent during the 2026-11-05 assessment; no competing build was started. No current Search Console export or page-level analytics were inspected.

**Setup required:** Existing product/release owner should review the correction. After an authorized release, fetch the page and check rendered text and link; use existing Search Console reporting for indexing and dated search performance.

**Boundary:** `all-source/docs/IMPACT_PLAN.md:24–25` states paid growth remains HOLD and the BET gate is unchanged. Preserve that gate, free/paid model and unknown revenue. No owner contact or Chronis query was made.
