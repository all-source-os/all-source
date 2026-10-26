# Latest-state read contract

Wire contract for reading current state without history: entity state without
history, conditional reads, views (declarative read models folded by the Query
Service and stored in Core), and the event-feed and read-model protocol the
SDKs package. Decision record: [ADR-021](../../adr/021-views-folded-by-query-service.md).
Research behind it: [latest-state reads](../../research/2026-10-27-latest-state-reads.md).

Every SDK sends the key as `Authorization: Bearer <key>`. The gateway reads no
other header and answers `401 "no authorization header"` without it. SDKs that
also send `X-API-Key` do so for callers that reach the Query Service directly.

Every SDK test reads the JSON fixtures in [`sdks/contract/fixtures/`](../../../sdks/contract/fixtures/),
so the four SDKs and Core agree on one set of bodies.

## 1. Conditional reads (all latest-state routes)

| Rule | Detail |
|---|---|
| `ETag` | Strong, `"sha256:<first 32 hex of sha256(body)>"`, on `200` responses of every route in this document and on `GET /api/v1/events/query`. Built from the response body, never from a store-wide counter: `entity_versions` is keyed by entity id across tenants (`store.rs:177`), so a counter-based tag would leak another tenant's activity. |
| `If-None-Match` | When it equals the tag Core would send, Core answers `304 Not Modified` with the same `ETag` and no body. A list of tags and `*` are honoured per RFC 9110. |
| Determinism | A body that carries a request-time field (for example a `queried_at`) must exclude it from the hash or drop it, otherwise a 304 can never happen. |
| Gateway | Forwards `If-None-Match`, copies `ETag`, meters a `304` as zero bytes for every route here. |

## 2. Entity state without history

`GET /api/v1/entities/{entity_id}/state?as_of&history`

- `history` defaults to `false`. `history=true` restores the `history[]` array
  exactly as before.
- Response (`history=false`):

```json
{
  "entity_id": "skill:abc",
  "current_state": { "name": "x", "status": "published" },
  "event_count": 12,
  "last_updated": "2026-10-27T10:00:00Z",
  "as_of": null,
  "version": 12
}
```

- `version` is the number of this tenant's events folded into the state, the
  same value as `event_count`. It is tenant-scoped because it is counted from
  the tenant's own fold.
- `GET /api/v1/entities/{entity_id}/snapshot` returns the same body.
- Conditional per section 1.

## 3. Event feed and read models (existing, now packaged by every SDK)

Unchanged Core routes, documented here because the SDKs implement them.

### `GET /api/v1/event-feed`

Query: `epoch` and `position` (both or neither), `limit` (default 100),
`prefixes` (comma separated, at most 8, each at most 128 bytes), `entity_id`.

```json
{
  "tenant_id": "t1",
  "cursor": { "epoch": 41, "position": 0 },
  "head": { "epoch": 41, "position": 3 },
  "events": [ { "id": "…", "event_type": "skill.published", "entity_id": "skill:abc", "payload": {}, "timestamp": "…", "version": 1 } ],
  "positions": [1],
  "caught_up": true
}
```

- Resume with the last element of `positions` as the next `position`.
- `409` with `{"error":"version_conflict","expected_version":<epoch sent>,"current_version":<current epoch>}`
  means the feed generation changed (Core restart or eviction). The consumer
  rebuilds from `position` 0 of the new epoch and commits with `reset: true`.

### `GET /api/v1/read-models/{name}`

Query: `prefix` (default empty), `after`, `limit` (1..500, default 100),
`revision`.

```json
{
  "revision": 7,
  "content_revision": 7,
  "checkpoint": { "epoch": 41, "position": 3 },
  "ready": true,
  "unchanged": false,
  "rows": { "skill:abc": { "status": "published" } },
  "next": null
}
```

`revision=<content_revision>` from a previous read returns `unchanged: true`
and no rows.

### `PUT /api/v1/read-models/{name}`

```json
{
  "expected_revision": 7,
  "checkpoint": { "epoch": 41, "position": 3 },
  "ready": true,
  "reset": false,
  "changes": { "skill:abc": { "status": "retired" }, "skill:old": null }
}
```

Returns `{"revision": 8}`. `null` deletes a row. A stale `expected_revision` is
`409 version_conflict`. Requires `projections:write` at the gateway.

### Service principal acting for a tenant

The Query Service holds one Core credential. Core honours a `tenant_id` query
parameter on `GET /api/v1/event-feed` and `GET|PUT /api/v1/read-models/{name}`
**only when the caller is the service principal**: role admin and tenant
`system`. A tenant's own admin does not qualify, because the gateway mints
per-request tokens with the caller's role. For any other caller a
`tenant_id` that differs from the authenticated tenant is `403`. The gateway
never forwards a client-supplied `tenant_id` on these routes.

## 4. Views

A view is a read model whose fold is a declarative reducer. The tenant defines
it, the Query Service folds it from the event feed, Core stores the definition
and the rows. Core validates definitions but never applies a reducer
(tenant-isolation-check, CLAUDE.md "Per-tenant read-model compute lives in the
Query Service").

Names: 1..48 characters of `[a-z0-9_-]`. Rows live in the read model
`view_<name>`; definitions in the read model `views_registry`, row key
`<name>`. Tenants cannot `PUT /api/v1/read-models/view_*` or
`/read-models/views_registry` directly (`403 reserved_read_model`); only the
admin principal can.

### Definition

`PUT /api/v1/views/{view}` (`projections:write`)

```json
{
  "source": { "prefixes": ["ops.probe."] },
  "key": "{payload.probe}:{day:timestamp}",
  "ops": [
    { "on": "ops.probe.observed", "set": { "probe": "payload.probe", "day": "day:timestamp" } },
    { "on": "ops.probe.observed", "when": { "payload.ok": true }, "increment": { "ok": 1 } },
    { "on": "ops.probe.observed", "when": { "payload.ok": false }, "increment": { "failed": 1 } },
    { "on": "ops.probe.observed", "max": { "last_seen_at": "timestamp" } },
    { "on": "ops.probe.observed", "set": { "status": "payload.status" } }
  ]
}
```

| Field | Rule |
|---|---|
| `source.prefixes` | 1..8 event-type prefixes, each at most 128 bytes. The fold reads only these from the feed. |
| `key` | Row-key template. Segments: literal text, `{entity_id}`, `{event_type}`, `{payload.<dotted.path>}`, `{day:timestamp}` (UTC `YYYY-MM-DD`). Rendered key at most 512 bytes. An event whose template references a missing payload field is skipped and counted. |
| `ops[]` | 1..64 operations, applied in order to the row selected by `key`. |
| `on` | Exact event type, or a prefix ending in `.` or `*`. |
| `when` | Optional map of `<path>` to a JSON scalar; all must be equal. |

Value expressions (right-hand side of `set`, `max`, `min`, `append`,
`remove`): `payload.<path>`, `entity_id`, `event_type`, `timestamp`,
`day:timestamp`, or `{"literal": <json>}`.

| Operation | Effect on the row |
|---|---|
| `set: {field: expr}` | Assign. Last writer by feed position wins. |
| `unset: [field]` | Remove fields. |
| `delete: true` | Delete the row (written as `null`, so `changes` reports it). |
| `increment: {field: n}` | Add an integer, starting from 0. |
| `max` / `min: {field: expr}` | Keep the larger / smaller value (numbers numerically, strings lexically, so RFC 3339 timestamps work). |
| `append` / `remove: {field: expr}` | Set semantics on an array field: append if absent, remove if present. |
| `status: {field, from: "suffix"\|expr, transitions?}` | Set `field` from the event type's last segment (`suffix`) or an expression. With `transitions` (`{"from": ["to", …]}`), a change not listed is ignored and counted as `rejected_transition`. An unset field accepts any first value. |

Validation errors are `400 {"error":"invalid_view","detail":"…"}`. A new
definition increments `generation`; the Query Service rebuilds the rows with
`reset: true` and the old rows keep serving until the rebuild commits its first
page.

`GET /api/v1/views/{view}` returns
`{"view","definition","generation","revision","checkpoint","ready"}`.
`DELETE /api/v1/views/{view}` removes the definition and the rows.
`GET /api/v1/views` lists the tenant's views.

### Latest state of one row

`GET /api/v1/views/{view}/entities/{key}` (`projections:read`)

```json
{ "view": "ops_status", "key": "probe-a:2026-10-27", "state": { "ok": 1430, "failed": 2 }, "version": 57, "revision": 61, "generation": 2, "ready": true }
```

`version` is the model revision that last wrote this key. `404
{"error":"not_found","ready":<bool>}` when the row does not exist. Conditional
per section 1.

### Latest state of many rows

`GET /api/v1/views/{view}/entities?prefix&after&limit` (`projections:read`)

```json
{ "view": "ops_status", "rows": [ { "key": "probe-a:2026-10-26", "state": {}, "version": 40 } ], "next": "probe-a:2026-10-26", "revision": 61, "content_revision": 61, "generation": 2, "ready": true }
```

`limit` 1..500, default 100. Page size at most 128 KiB. `revision` is the
watermark to start `changes` from after the last page.

### Changes after a revision

`GET /api/v1/views/{view}/changes?after=<revision>&limit&wait` (`projections:read`)

```json
{ "view": "ops_status", "changes": [ { "key": "probe-a:2026-10-27", "version": 62, "state": { "ok": 1431, "failed": 2 } }, { "key": "probe-z:2026-10-20", "version": 62, "state": null } ], "revision": 62, "caught_up": true, "generation": 2 }
```

- Rows whose last write is after `after`, ordered by `version` then key.
  `state: null` is a deletion. Revisions are journaled read-model revisions, so
  a cursor survives Core restarts.
- `revision` is the cursor for the next call: the highest `version` returned,
  or the model revision when caught up.
- `wait` (0..20 seconds, default 0): when nothing changed after `after`, Core
  holds the request until a commit lands or `wait` elapses, then answers with
  whatever exists (possibly empty, `caught_up: true`). A held request does not
  occupy a read-model slot.
- `409 {"error":"view_reset","generation":<n>}` when `after` is older than the
  last reset: the client re-reads the entity pages.

### SDK surface

Every SDK exposes, with idiomatic names:

| Call | Does |
|---|---|
| `entityState(id, {asOf, history, ifNoneMatch})` | Section 2, returns state or NotModified |
| `queryEvents(...)` | Response includes `entity_version` when Core sends it; supports `ifNoneMatch` |
| `feed({cursor, prefixes, entityId, limit})` | Section 3 feed page |
| `readModel(name).page({prefix, after, limit, revision})` / `.commit(commit)` | Section 3 |
| `defineView(name, definition)`, `getView`, `deleteView`, `listViews` | Section 4 |
| `latest(view, key, {ifNoneMatch})` | One row or NotModified |
| `latestMany(view, {prefix, limit})` | Iterates every page |
| `sync(view, {checkpoint, wait})` | Snapshot (all pages) then `changes` loop; on `view_reset` re-snapshots; honours `Retry-After` on 429/503; yields upserts and deletes plus a checkpoint the caller stores |

## 5. Retired routes

Removed from Core, Query Service and SDKs because their stores have no tenant
key: `POST|GET /api/v1/snapshots`, `GET /api/v1/snapshots/{entity_id}/latest`,
`/api/v1/projections/{name}/state`, `/api/v1/projections/{name}/{entity_id}/state`,
`/api/v1/projections/{name}/bulk`, `/api/v1/projections/{name}/bulk/save`,
Query Service `GET /api/v1/snapshots` and `POST /api/query/projected`.
Core's internal global projections (`entity_snapshots`, `event_counters`,
Prime) are unchanged.
