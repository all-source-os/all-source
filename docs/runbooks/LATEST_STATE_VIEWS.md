# Latest-state views

Operating declarative views: who folds them, the flags that turn them on, how
to define one, and what to watch. Decision: [ADR-021](../adr/021-views-folded-by-query-service.md).
Wire contract: [`docs/contracts/latest-state/README.md`](../contracts/latest-state/README.md).

## What runs where

| Piece | Owner | Role |
|---|---|---|
| Definitions | Core, read models `views_registry` (per tenant) and `views_index` (reserved tenant `_views`) | Validated and stored by Core on `PUT /api/v1/views/{view}` |
| Folder | Query Service | Reads `views_index`, pulls each tenant's `/api/v1/event-feed`, applies the reducer, commits to `view_<name>` with revision CAS |
| Rows | Core, read model `view_<name>` | Served by `/api/v1/views/{view}/entities[/{key}]` and `/changes` |
| Edge | Gateway (`apps/control-plane/views.go`) | Forces the tenant, strips client `tenant_id`, clamps `wait` to 20 s, holds `changes` long-polls in their own lane |

Core never applies a reducer. If the Query Service is down, every read keeps
answering from the last committed rows; nothing errors, the rows just stop
moving. `GET /api/v1/views/{view}` shows `checkpoint` and `ready`, which is the
only freshness signal a reader gets.

## Prerequisites

### Core: `ALLSOURCE_VIEWS`

Off by default. Defining a view writes to the store, so every
`/api/v1/views*` route answers `404 {"error":"views_disabled"}` unless Core
runs with `ALLSOURCE_VIEWS=1` (or `true`).

```bash
fly secrets set ALLSOURCE_VIEWS=1 -a allsource-core
```

Setting a secret restarts the machines. Unsetting it turns the view routes off
again; stored definitions and rows stay in their read models.

### Query Service: a `system`-tenant admin Core key in `CORE_API_KEY`

The folder reads every tenant's feed and writes every tenant's view rows with
one credential. Core honours `tenant_id` on `/api/v1/event-feed` and
`/api/v1/read-models/{name}` only for the service principal: role `admin` and
tenant `system`, the same identity as the control plane's service token. Only
that principal may read `views_index` or write `view_*`, `views_registry` and
`views_index`. A tenant's own admin is not the service principal, because the
gateway mints per-request tokens with the caller's role. Any other key gets
`403` on the first act-as call, and no view ever becomes `ready`. Core refuses
to create a tenant named `system` or starting with `_`.

```bash
fly secrets list -a allsource-query          # CORE_API_KEY must be present
fly secrets set CORE_API_KEY=<system-tenant admin key> -a allsource-query
```

The key can act for any tenant on those routes. Keep it only in the Query
Service's secrets; never put it in a gateway, SDK or client config.

### Core: `ALLSOURCE_FEED_DURABLE_LOG` (optional)

Off by default. Without it the feed epoch is random per Core process, so every
Core restart answers the folder's next feed read with `409 version_conflict`
and the folder rebuilds each view from position 0 with `reset: true`. Readers
are not affected: the `changes` cursor is the read-model revision, which
survives restarts on its own. The log only removes the rebuild.

| Value | Effect |
|---|---|
| unset, empty, `0`, `false`, `off` | Off |
| `1`, `true`, `on`, `*`, `all` | Every tenant |
| `tenant-a,tenant-b` | Only the listed tenants |

The log lives in `<wal dir>/feed-log/`, one file per tenant, and needs a
writable store with a WAL. When it cannot open, Core logs
`ALLSOURCE_FEED_DURABLE_LOG needs a writable store with a WAL; feed positions
stay per process` or `Durable feed log unavailable at …` and carries on without
it.

Enable per environment, smallest scope first:

1. Local: `ALLSOURCE_FEED_DURABLE_LOG=all` in the Core process environment.
2. Production, one tenant: `fly secrets set ALLSOURCE_FEED_DURABLE_LOG=<tenant id> -a allsource-core`.
   After the restart, read the feed twice across a second restart and confirm
   `cursor.epoch` is unchanged.
3. Widen the list, then `all`.

To roll back, unset the secret. Feed positions go back to per process and the
folder rebuilds once.

## Defining the ops-status view

Through the gateway, with a key that carries write permission:

```bash
curl -X PUT https://api.all-source.xyz/api/v1/views/ops_status \
  -H "Authorization: Bearer $ALLSOURCE_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "source": { "prefixes": ["ops.probe."] },
    "key": "{payload.probe}:{day:timestamp}",
    "ops": [
      { "on": "ops.probe.observed", "set": { "probe": "payload.probe", "day": "day:timestamp" } },
      { "on": "ops.probe.observed", "when": { "payload.ok": true }, "increment": { "ok": 1 } },
      { "on": "ops.probe.observed", "when": { "payload.ok": false }, "increment": { "failed": 1 } },
      { "on": "ops.probe.observed", "max": { "last_seen_at": "timestamp" } },
      { "on": "ops.probe.observed", "set": { "status": "payload.status" } }
    ]
  }'
```

A `400 invalid_view` names the field Core rejected. Then check the fold:

```bash
curl -H "Authorization: Bearer $ALLSOURCE_KEY" \
  https://api.all-source.xyz/api/v1/views/ops_status
# generation, revision, checkpoint, ready

curl -H "Authorization: Bearer $ALLSOURCE_KEY" \
  "https://api.all-source.xyz/api/v1/views/ops_status/entities?prefix=probe-a:&limit=10"
```

`ready: false` with a `checkpoint` that does not move means the folder is not
running for this tenant: check the Query Service logs and `CORE_API_KEY`
first. Redefining the view bumps `generation`; readers on `changes` get
`409 view_reset` once the rebuild commits and re-read the entity pages.

## What to watch

### Query Service folder

| Counter | Means | Act when |
|---|---|---|
| `skipped` | Events whose key template references a missing payload field | It grows for a view that should match every event: the definition's `key` does not fit the events |
| `rejected_transition` | `status` changes not listed in `transitions` | Any sustained rate: either the producer emits out-of-order states or the transition table is wrong |
| rebuilds | Views refolded from position 0 (new generation, or `409 version_conflict` on the feed) | More than one per Core restart, or rebuilds with no restart: the feed epoch is changing under the folder. Consider `ALLSOURCE_FEED_DURABLE_LOG` for that tenant |

### Gateway `changes` lane

`changes` requests with `wait > 0` are held in their own lane, not in the 16
global response slots: 64 slots in total, at most 8 per tenant, each held for
`wait` plus 5 s. A request past either cap gets `429` with reason `pool_full`
and counts in `admission_refused_total{scope="global"}`.

A tenant can read its own lane state:

```bash
curl -H "Authorization: Bearer $ALLSOURCE_KEY" \
  https://api.all-source.xyz/api/v1/tenants/me/limits
# view_changes: { slots, occupied, held, max_per_tenant, max_wait }
```

`occupied` near `slots` for long stretches means long-pollers across tenants
are filling the lane; `held` at `max_per_tenant` for one tenant means that
tenant runs more than 8 concurrent `sync` loops and should share one.
