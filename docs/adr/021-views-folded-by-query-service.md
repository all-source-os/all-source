# ADR-021: Views Folded by the Query Service, Stored in Core

**Status:** Accepted
**Date:** 2026-10-27
**Contract:** [`docs/contracts/latest-state/README.md`](../contracts/latest-state/README.md) · **Research:** [`docs/research/2026-10-27-latest-state-reads.md`](../research/2026-10-27-latest-state-reads.md) · **Runbook:** [`docs/runbooks/LATEST_STATE_VIEWS.md`](../runbooks/LATEST_STATE_VIEWS.md)

## Context

A consumer that wants the current state of its entities has no usable route
for it. The research note measured the result: every production read is
`GET /api/v1/events/query` followed by a client-side fold, and 99.5% of the
gateway's 66 GB of daily edge output on 2026-10-24 was event-query bodies.

The latest-state primitives Core already had each fail for a different reason:

- `GET /api/v1/entities/{id}/state` returned the merged state plus a
  `history[]` of every event, so the body was larger than the raw stream.
- The `SnapshotManager` and the projection state cache are keyed by entity id
  with no tenant, held in memory and lost on restart. They cannot be offered
  to a tenant.
- The event feed and read models are tenant-scoped and durable, but the fold
  stays in the consumer, no SDK packages them, and the feed epoch was random
  per process, so every restart invalidated every cursor.

The research note left one boundary open: whether Core folds views on ingest,
or the Query Service folds them and Core stores the rows. The CLAUDE.md rule
"Per-tenant read-model compute lives in the Query Service, NOT Core" and the
`tooling/tenant-isolation-check` gate already forbid per-tenant projection
compute in `apps/core/src`.

## Decision

**A view is a read model whose fold is a declarative reducer. The Query
Service folds it from the event feed and commits the rows to Core. Core
validates and stores the definitions and the rows, and serves them. Core never
applies a reducer.**

1. **Definitions.** `PUT /api/v1/views/{view}` stores a definition: source
   prefixes, a row-key template and up to 64 operations from a fixed set
   (`set`, `unset`, `delete`, `increment`, `max`, `min`, `append`, `remove`,
   `status`). Core validates the shape and rejects anything else with
   `400 invalid_view`. Definitions live in the read model `views_registry`;
   a copy of every tenant's definitions lives in `views_index` under the
   reserved tenant `_views`, so the folder reads one model instead of
   scanning every tenant. A new definition bumps `generation`.
2. **Folding.** The Query Service reads `views_index`, pulls each tenant's
   event feed for the view's prefixes, applies the reducer
   (`apps/query-service/lib/query_service_ex/views/reducer.ex`) and commits
   rows to the read model `view_<name>` with the existing revision CAS. A new
   generation is rebuilt with `reset: true`; the old rows keep serving until
   the rebuild commits its first page.
3. **Serving.** Core answers `GET /api/v1/views/{view}/entities/{key}`
   (latest), `GET /api/v1/views/{view}/entities` (latest many, paged) and
   `GET /api/v1/views/{view}/changes` (rows changed after a revision, with an
   optional `wait` of up to 20 s). Every `200` carries a body-derived `ETag`,
   and a matching `If-None-Match` gets `304`.
4. **Changes cursor.** The `changes` cursor is the read-model revision, which
   is journaled with each commit. It survives a Core restart without the
   durable feed log. The feed log (`ALLSOURCE_FEED_DURABLE_LOG`) only spares
   the folder a rebuild when Core restarts.
5. **Acting for a tenant.** The folder holds one admin Core key of the
   `system` tenant. Core honours a `tenant_id` query parameter on the feed and
   read-model routes only for that service principal (role admin and tenant
   `system`); for anyone else, a tenant's own admin included, a different
   `tenant_id` is `403`.
   Tenants cannot write `view_*`, `views_registry` or `views_index` through
   the read-model API (`403 reserved_read_model`). The gateway strips any
   client `tenant_id` on these routes.
6. **Entity state.** `GET /api/v1/entities/{id}/state` gains a `history`
   parameter, `false` by default, and a `version` field equal to the number
   of the tenant's events folded.
7. **Retirement.** The untenanted routes are removed from Core, the Query
   Service and the SDKs: `/api/v1/snapshots*`, `/api/v1/projections/{name}/state`,
   `/api/v1/projections/{name}/{entity_id}/state`, `/api/v1/projections/{name}/bulk*`,
   and the Query Service's `GET /api/v1/snapshots` and
   `POST /api/query/projected`. Core's internal global projections
   (`entity_snapshots`, `event_counters`, Prime) are unchanged.
8. **Flags.** Defining a view writes to the store, so the view routes answer
   `404 views_disabled` unless Core runs with `ALLSOURCE_VIEWS=1`. The durable feed log is off unless
   `ALLSOURCE_FEED_DURABLE_LOG` names the tenants (or `all`).

## Consequences

**Positive**

- A latest read returns one state and no history. `history=true` is the only
  way to get history from the entity state route.
- `changes` cursors survive Core restarts and deploys, because they are
  revisions of a journaled read model rather than feed positions.
- The tenant-isolation rule and its gate stand unchanged. Core stores and
  serves rows; it computes no per-tenant state.
- The read-model protocol is unchanged, so a consumer whose fold the reducer
  set cannot express keeps folding the feed and committing rows itself.

**Negative**

- The Query Service becomes a second failure domain for freshness. While it
  is down, latest reads keep serving the last committed rows, so a reader sees
  stale state rather than an error. `ready` and `checkpoint` on the view say
  how far the fold got; nothing else does.
- Reducers are limited to the declarative set. Guarded state machines beyond
  `status` transitions, joins and imports with admission rules stay in the
  consumer.
- The folder needs an admin Core key in `CORE_API_KEY`. That key can act for
  any tenant on the feed and read-model routes, so it is held only by the
  Query Service.
- Every change takes one extra hop (Core feed, Query Service, Core commit)
  before a reader sees it.

## Alternatives considered

- **Core folds on ingest.** One hop fewer, but it puts per-tenant reducer
  compute on the ingest hot path, which the CLAUDE.md rule and the
  tenant-isolation gate exist to prevent. Rejected.
- **Re-key the snapshot store and projection state cache by tenant.** Keeps
  two in-memory stores with no durability and no versioned schema, and still
  leaves the fold undefined for a tenant. Rejected in favour of retiring them.
