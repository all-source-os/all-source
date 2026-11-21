# Read-cost contract

What a client sends so its event reads can be attributed, what Core and the
gateway count from it, and the routes and MCP tools that report it. Operating
it: [`docs/runbooks/READ_COST.md`](../runbooks/READ_COST.md).

Everything here is in memory. A Core restart clears the tables; the
Prometheus series reset with the process like every other counter.

## Request headers

Both are optional and travel as headers, never as query parameters.

| Header | Pattern | Meaning |
|---|---|---|
| `x-allsource-caller` | `^[a-z][a-z0-9_.]{0,63}$` | The code area that issued the read, such as `lh.skill_catalog` |
| `x-allsource-read-id` | `^[0-9a-f]{16}$` | One walk: every page of one logical read carries the same value |

- The gateway and Core each check both patterns. A value that fails is
  replaced with `invalid` before it is forwarded or counted. The
  `read_header_invalid` log line carries the header name and the value's
  length only, at most once per minute per header.
- An invalid read id counts as absent. An invalid caller is counted under the
  caller `invalid`. The reserved names `unattributed`, `other` and `invalid`
  are rejected as callers like any malformed value.
- Core takes a value the gateway already replaced with `invalid` as that
  replacement, without logging or counting it again: a read id of `invalid`
  is absent and a caller of `invalid` counts under `invalid`.
- A read with no caller is counted as `unattributed`.
- Rust SDK 0.30: `ClientConfig::with_caller` sets the caller on every
  request, `QueryEventsParams::caller` overrides it for one query, and
  `EventPaginator` creates one `ReadId` per walk and sends it on every page.

## Stream class

The tenant-free name of what a read targets. Core labels its counters with it
and Longhand classifies its own reads with the same rule
(`apps/core/src/domain/stream_class.rs`, whose test vectors are the shared
contract):

1. Entity id: split on `:`, take the first segment that starts with a letter
   and holds no digit, with `-` read as `_`. No such segment gives `entity`.
2. Otherwise the event-type prefix, with a trailing `.` dropped.
3. Otherwise the event type with its last `.`-segment dropped; a single
   segment is kept.
4. Otherwise `unscoped`.

The result is lowercased and cut to 64 characters; anything that then fails
`^[a-z][a-z0-9_.]{0,63}$` is `unclassified`.

| Input | Class |
|---|---|
| entity `{tenant}:{user}:skill_run:{id}` | `skill_run` |
| entity `auth-member:abc123` | `auth_member` |
| prefix `auth.member.` | `auth.member` |
| event type `auth.member.updated` | `auth.member` |
| entity `longhand:{user}:skill_run:{id}` | `longhand` |

Core classifies a read with its tenant: an entity id whose first segment is
exactly the requesting tenant's id skips that segment, so
`longhand:{user}:skill_run:{id}` read by tenant `longhand` is `skill_run`.
Only the first segment is compared, exactly as sent. An entity id that starts
with another tenant's all-letter id still yields that id as its class, as
does a read classified without a tenant, which the row above shows.

## Facts Core records per query page

`GET /api/v1/events/query` attaches the facts below to its response, and a
layer inside the 16 MiB response bound records them, so an oversized body is
recorded as `too_large` before the bound replaces it. Other routes are not
counted. Embedded Core never runs the layer.

| Term | Definition |
|---|---|
| page | One `events/query` response |
| logical read | A page with no read id, or the first page of a walk |
| whole reads (count) | Counted once per logical read: a walk's later pages add pages, not whole reads |
| walk | Pages sharing one read id; a page with `has_more` opens or extends it, a page without ends it |
| whole read | A page with no `since` and no entity id that is not a `limit=1` descending read and not a `limit=0` count probe |
| single truncated | No read id, no `since`, no entity id, offset 0, served limit at least 2, a full page and `has_more` |
| walk abandoned | A walk whose last page had `has_more` and got no next page for more than 30 s |
| walk evicted | A walk dropped because its tenant had 256 walks in flight (its own oldest goes) or 4,096 were in flight in all (the oldest goes); never counted as abandoned |
| body bytes | Length of the uncompressed JSON body; 0 on a 304. A body whose length is not known exactly counts its lower bound and marks the caller row `approximate` |

The first page of a walk whose client sends no read id also matches single
truncated, so the count is exact only for clients that send read ids.

A timer sweeps idle walks every 10 s, so a walk is counted abandoned more than
30 and at most 40 s after its last page, whether or not another query arrives.
A page exactly 30 s after the previous one continues the walk.

Each tenant holds at most 256 walks in flight. A tenant opening a 257th walk
evicts its own least recently seen walk, never another tenant's; the table
holds 4,096 walks in all.

## Routes

### `GET /api/v1/read-costs`

| Parameter | Values | Default |
|---|---|---|
| `tenant_id` | required at Core; the gateway always overwrites it with the authenticated tenant | — |
| `order` | `bytes`, `pages`, `matched`, `whole`, `truncated` | `bytes` |
| `limit` | at most 256 | 50 |

Core skips its own auth on this route, like `/api/v1/stats` and
`/api/v1/streams`, and answers for whatever `tenant_id` it is given. Tenant
isolation holds only through the gateway, which overwrites `tenant_id` with
the authenticated tenant. Core must stay private: anyone who reaches it
directly can read any tenant's rows.

Missing or empty `tenant_id` answers 400. Caller rows carry no matched count,
so `matched` orders them by pages; class rows carry no bytes, so `bytes`
orders them by pages. Both lists are cut to `limit`.

Core keeps, per tenant, at most 128 classes (later ones count under `other`)
and 256 caller rows. When the caller table is full, the row with the fewest
body bytes is replaced: the new key inherits its counts and is marked
`approximate`. At most 128 tenants are kept, the least recently active
evicted first.

Empty rows alone do not mean "no reads". The response says what the rows
cover:

- `enabled`: recording is on now. When `false`, pages are not being counted.
- `since`: when the tables last started empty: process start, or the switch
  turning recording back on, which clears every table so no row spans a gap.
- `tenant_tracked`: the tenant has rows. `false` both for a tenant with no
  reads since `since` and for one pushed out of the 128-tenant table; Core
  cannot tell the two apart.
- `tenant_since`: when the tenant's rows started. Equal to `since` while no
  tenant has been pushed out of the table since `since`: a tenant first seen
  later had no earlier pages. Once any tenant has been pushed out, a tenant
  entering the table gets its entry time, since it may be coming back. Null
  when untracked.
- `instance_id`: 32 hex characters, random per Core process. The tables live
  in one process and are never shared or persisted, so the rows cover only
  the pages this Core instance served since `since`. With several Core
  machines, each answers for its own pages, and a restart starts a new id and
  empty tables. Compare `instance_id` across replies before adding or
  diffing rows.

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "ReadCostsResponse",
  "type": "object",
  "required": ["callers", "classes", "enabled", "since", "instance_id", "tenant_tracked", "tenant_since"],
  "properties": {
    "enabled": { "type": "boolean" },
    "since": { "type": "string", "format": "date-time" },
    "instance_id": { "type": "string", "pattern": "^[0-9a-f]{32}$", "description": "Random per Core process; rows cover only this instance" },
    "tenant_tracked": { "type": "boolean" },
    "tenant_since": { "type": ["string", "null"], "format": "date-time" },
    "callers": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["caller", "stream_class", "logical_reads", "pages", "whole_reads", "body_bytes", "single_truncated", "walk_abandoned", "approximate", "first_seen", "last_seen"],
        "properties": {
          "caller": { "type": "string", "description": "A caller name, invalid, other or unattributed" },
          "stream_class": { "type": "string" },
          "logical_reads": { "type": "integer", "minimum": 0 },
          "pages": { "type": "integer", "minimum": 0 },
          "whole_reads": { "type": "integer", "minimum": 0 },
          "body_bytes": { "type": "integer", "minimum": 0 },
          "single_truncated": { "type": "integer", "minimum": 0 },
          "walk_abandoned": { "type": "integer", "minimum": 0 },
          "approximate": { "type": "boolean" },
          "first_seen": { "type": ["string", "null"], "format": "date-time", "description": "First page counted for this row; a replaced row keeps its predecessor's. Null for a row created only by an abandoned walk" },
          "last_seen": { "type": ["string", "null"], "format": "date-time", "description": "Latest page counted for this row" }
        }
      }
    },
    "classes": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["stream_class", "logical_reads", "pages", "whole_reads", "single_truncated", "walk_abandoned", "walk_evicted", "matched_sum", "matched_max", "max_pages_per_walk"],
        "properties": {
          "stream_class": { "type": "string" },
          "logical_reads": { "type": "integer", "minimum": 0 },
          "pages": { "type": "integer", "minimum": 0 },
          "whole_reads": { "type": "integer", "minimum": 0 },
          "single_truncated": { "type": "integer", "minimum": 0 },
          "walk_abandoned": { "type": "integer", "minimum": 0 },
          "walk_evicted": { "type": "integer", "minimum": 0 },
          "matched_sum": { "type": "integer", "minimum": 0 },
          "matched_max": { "type": "integer", "minimum": 0 },
          "max_pages_per_walk": { "type": "integer", "minimum": 0, "description": "Most pages seen in one walk" }
        }
      }
    }
  }
}
```

### `GET /api/v1/streams?with_bytes=true`

The existing stream listing, with sizes when `with_bytes=true`.

| Parameter | Values | Default |
|---|---|---|
| `tenant_id` | the gateway overwrites it with the authenticated tenant; no tenant answers an empty list with `complete: false`, `reason: "no_tenant"` | — |
| `with_bytes` | `true` adds `estimated_bytes` and `events_last_7d` | `false` |
| `sort` | `events`, `bytes`, largest first, ties by `stream_id` | `events` |
| `offset`, `limit` | paging over the sorted list | 0, 100 with `with_bytes` |

`estimated_bytes` is the heap Core holds for the stream's events: the fixed
event header plus payload and metadata. Core sizes events in place, holding
the events lock for one chunk of 4,096 at a time. Removing events between
chunks shifts their positions, so Core restarts the scan once; if events are
removed again, it answers what it counted with `complete: false`, and an
event may then be skipped or counted twice. Concurrent `with_bytes` requests
for one tenant share one scan, and a complete scan answers later `with_bytes`
requests for that tenant for 30 s after it started, so sizes can trail writes
by that long; an incomplete scan is not reused. A scan that fails answers 503
`{"error": "store_unavailable"}` and is not shared with waiting requests,
which run their own. `total` counts the tenant's streams before paging.

Each pass hydrates the tenant first. A tenant that cannot be hydrated answers
503 `store_unavailable`, with or without `with_bytes`, never zero streams. A
pass is complete only if the tenant was still resident, and not evicted, when
it ended; the retry hydrates again, so a tenant evicted during the first pass
is counted in full by the second, and one evicted during both answers
`complete: false`. Core reads the tenant's eviction generation before it
hydrates and compares it once the scan ends, so an eviction anywhere in
between, even one followed by a reload, makes the pass incomplete. Without
`with_bytes`, a tenant evicted between hydration and the end of the listing
answers `complete: false`, `reason: "tenant_evicted"`.

`GET /api/v1/stats?tenant_id=` and `GET /api/v1/event-types?tenant_id=`
answer 503 `store_unavailable` when the tenant cannot be hydrated, never zero
counts. They read the tenant the same way and retry once when it was evicted
during the read; a second eviction is 503 too.

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "ListStreamsResponse",
  "type": "object",
  "required": ["streams", "total", "complete"],
  "properties": {
    "total": { "type": "integer", "minimum": 0 },
    "complete": { "type": "boolean", "description": "False when a with_bytes scan saw events removed or the tenant evicted during it and its retry, when no tenant was named, or when the tenant was evicted under a listing without with_bytes" },
    "reason": { "type": "string", "enum": ["no_tenant", "tenant_evicted"], "description": "Present only with those two causes of complete: false" },
    "streams": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["stream_id", "event_count", "last_event_at"],
        "properties": {
          "stream_id": { "type": "string", "description": "The entity id" },
          "event_count": { "type": "integer", "minimum": 0 },
          "last_event_at": { "type": ["string", "null"], "format": "date-time" },
          "estimated_bytes": { "type": "integer", "minimum": 0, "description": "Only with with_bytes=true" },
          "events_last_7d": { "type": "integer", "minimum": 0, "description": "Only with with_bytes=true" }
        }
      }
    }
  }
}
```

### `PUT /api/v1/admin/read-cost`

`{"enabled": bool}` turns recording on or off without a restart. Admin
permission: 401 without auth, 403 without the permission. Core boots enabled
unless `ALLSOURCE_READ_COST=off`; the switch is not persisted, so a restart
returns to the boot value. Turning recording off drops every walk in flight
without counting it, and the sweep does nothing while off. Turning it back on
clears every table and restarts `since`.

### Gateway

`GET /api/v1/read-costs` and `GET /api/v1/streams` are proxied beside
`/api/v1/stats`. Both force the authenticated tenant into `tenant_id`, need
the read permission, and accept a managed key with the `events:read` scope.

## Metrics

### Core, no tenant or caller label

The stream class is capped at 256 values per process; later classes count as
`other`, increment `allsource_read_label_overflow_total{label="stream_class"}`
and log `read_label_overflow` once per process. Event-type and prefix classes
keep their digit segments, so they count toward the same 256-label cap before
falling into `other`.

| Series | Labels |
|---|---|
| `allsource_read_pages_total` | `stream_class`, `outcome` ∈ `complete`, `has_more`, `not_modified`, `too_large` |
| `allsource_read_events_matched_total` | `stream_class` |
| `allsource_read_truncated_total` | `stream_class`, `kind` ∈ `single`, `walk_abandoned`, `walk_evicted` |
| `allsource_read_whole_reads_total` | `stream_class` |
| `allsource_read_label_overflow_total` | `label` |
| `allsource_read_header_invalid_total` | `header` ∈ `caller`, `read_id` |

### Gateway, events-query path

| Series | Labels | Unit |
|---|---|---|
| `control_plane_tenant_event_query_caller_bytes_total` | `tenant`, `caller` | wire bytes |
| `control_plane_tenant_event_query_caller_queries_total` | `tenant`, `caller` | queries |
| `control_plane_read_header_invalid_total` | `header` | rejected values |

Per tenant, the first 64 distinct callers per gateway process become labels
and later ones count as `other`; `unattributed` and `invalid` are always
available. The 64 slots belong to the tenant id, not its `tenant` label, so
tenants counted under `other` each keep their own; past 512 tenant ids, later
tenants share the slots of their label.

## Logs

| Event | Fields | Rate |
|---|---|---|
| `read_cost_threshold` | `tenant_fingerprint` (6 bytes of SHA-256), `caller` (`none` when absent), `stream_class`, `reason`, `pages`, `matched` | once per minute per fingerprint, caller and class |
| `read_header_invalid` | `header`, `length` | once per minute per header |
| `read_label_overflow` | `label` | once per process |

`reason` is one of `walk_pages` (a walk past 20 pages), `matched_pages` (a
page matching more than 50,000 events), `walk_abandoned`, or
`single_truncated` (only when the read carries a caller). The tenant id never
appears in a log line.

## MCP tools (`tooling/allsource-mcp`)

All three are read-only and take `store` and `diagnostic` like every other
tool.

| Tool | Arguments | Embedded store | Remote store |
|---|---|---|---|
| `stream_sizes` | `sort` (`events`, `bytes`), `limit` (≤ 500, default 50) | scans the store | `/api/v1/streams?with_bytes=true&limit=10000` and `/api/v1/stats` |
| `read_costs` | `order`, `limit` (≤ 256, default 50) | `UNSUPPORTED`: `unsupported: read_costs reads from the gateway; …` | `/api/v1/read-costs` |
| `cap_check` | `caps: [{stream_class, cap_events, measure?}]` (≤ 256), `threshold_pct` (1–100, default 80) | over `stream_sizes` | over `stream_sizes` |

- `stream_sizes` returns stream rows `{stream_id, stream_class, events, bytes,
  events_last_7d, last_event_at}` and class rows `{stream_class, streams,
  events, bytes, events_last_7d, max_stream_events, max_stream_bytes}`. Class
  totals cover every stream read, not only the `limit` rows returned. On a
  remote store with more than 10,000 streams, completeness is `false` with
  reason `stream_limit_reached`, `omitted` is Core's `total` less the rows
  read, and `page.totalCount` is Core's `total`. An embedded store whose last
  refresh failed serves what it held before: completeness is `false` with
  reason `refresh_failed`, and `context.store` carries the refresh state
  (`lastRefreshError`, `refreshedAt`). When Core's scan answers `complete: false`
  (or an embedded scan does), completeness is `false` with reason
  `scan_interrupted`, or `no_tenant` when Core named that reason.
- A remote reply with no `complete`, or with a stream lacking
  `estimated_bytes` or `events_last_7d` (a Core older than 0.30 ignores
  `with_bytes`), has completeness `false` with reason `bytes_unavailable`.
  Class `bytes`, `max_stream_bytes` and `events_last_7d` are then `null`, never
  summed from absent values, and `sort: "bytes"` is refused as `unsupported:`.
  A row without `stream_id` or `event_count`, or a reply without `total`, is an
  error.
- An embedded tenant that cannot be hydrated is an error
  (`store unavailable: …`), never zero streams. The embedded scan runs on a
  blocking thread.
- `stream_sizes` classes come from stream ids, so they are entity classes. A
  prefix or event-type class such as `auth.member` never appears in them.
- `cap_check` compares each cap with the class's largest stream
  (`measure: "largest_stream"`, the default) or the sum of its streams
  (`measure: "class_total"`). A class at or above `threshold_pct` of its cap
  is in `over`, others in `under`, and a class with no stream in the store in
  `unmeasured`, never in `under`.
- `read_costs` adds `truncated {single, walk_abandoned, walk_evicted}` and
  `maxPagesPerWalk`, summed and maximised over the class rows returned, and
  `totalsCover`: `all_class_rows` when fewer class rows than `limit` came
  back, so the totals are the tenant's, else `returned_class_rows`.
- `read_costs` adds `recording {enabled, instanceId, since, tenantTracked,
  tenantSince}` from Core; the rows cover only the Core instance named by
  `instanceId`, since `since`. Completeness is `false`, with the first reason
  that applies: `recording_off` when `enabled` is `false`,
  `tenant_not_tracked` when the tenant has no rows, `recording_state_unknown`
  when Core sent neither field or a tracked tenant without readable `since`
  and `tenant_since`, `rows_approximate` when a caller row is `approximate`,
  `classes_overflowed` when a class row is `other`, `tenant_reentered` when
  `tenant_since` is later than `since`, and `limit_reached` when a list came
  back at `limit`. A reply without `callers` or `classes`, a caller row
  without `approximate`, or a class row missing a summed field, is an error.
