# Read cost

Finding which caller and which stream class cost the most reads, turning the
recording off and on, and reading the threshold and overflow logs. Wire
contract, row shapes and series: [`docs/contracts/read-cost.md`](../contracts/read-cost.md).

## What runs where

| Piece | Where | Role |
|---|---|---|
| Headers | Client (Rust SDK 0.30) | `x-allsource-caller` on every request, `x-allsource-read-id` on every page of a walk |
| Gateway (`allsource-control-plane`) | `delegation.go`, `query_egress.go` | Validates both headers, forwards them, counts wire bytes and queries per tenant and caller; proxies `/api/v1/read-costs` and `/api/v1/streams` with the tenant forced |
| Core (`allsource-core`) | `infrastructure/web/read_cost.rs` | Records every `events/query` page: walks, per-tenant class and caller tables, five Prometheus series, threshold logs |
| MCP (`tooling/allsource-mcp`) | `stream_sizes.rs` | `stream_sizes`, `read_costs`, `cap_check` |

All of Core's tables are in memory. A Core restart or deploy clears them, so
read them before a deploy if the numbers matter.

## Which caller costs the most

Through the gateway, with the MCP bound to a remote store (see
[`tooling/allsource-mcp/README.md`](../../tooling/allsource-mcp/README.md)):

```text
read_costs  { "store": "remote", "order": "bytes", "limit": 20 }
read_costs  { "store": "remote", "order": "truncated" }
```

The gateway answers for the tenant of the key the MCP holds and no other.
`order: "pages"` finds walk-heavy callers; `whole` finds callers that read a
class from the start with no `since`. A row with `approximate: true` took over
a full table's smallest row and carries that row's counts as an error bound.

Wire bytes by caller over time are the gateway series
`control_plane_tenant_event_query_caller_bytes_total` on the egress
attribution dashboard ([`BUSINESS_DASHBOARD.md`](BUSINESS_DASHBOARD.md)).
Core's `body_bytes` is the uncompressed body, so the two differ by the gzip
ratio.

Large `unattributed`, `other` or `invalid` rows mean the client does not send
a caller, has more than 64 distinct callers on one gateway process, or sends
one that fails `^[a-z][a-z0-9_.]{0,63}$`.

## Which streams are largest

```text
stream_sizes  { "store": "remote", "sort": "bytes", "limit": 20 }
cap_check     { "store": "remote", "caps": [{ "stream_class": "skill_run", "cap_events": 20000 }] }
```

A `with_bytes` scan walks every event of the tenant on Core and holds the
events lock per chunk of 4,096 events. Run it on demand, not on a schedule.
Concurrent requests for one tenant share one scan.

Against a local data directory the same tools scan the embedded store and
need no gateway; `read_costs` answers `UNSUPPORTED` there, because only a
serving Core records reads.

## Reaching Core directly

Core is private. From an operator machine:

```bash
fly proxy 3900 -a allsource-core
```

Then, in another shell:

```bash
curl -s localhost:3900/metrics | grep '^allsource_read_'
curl -s "localhost:3900/api/v1/read-costs?tenant_id=<tenant>&order=pages&limit=20"
```

`/api/v1/read-costs` skips Core's auth like `/api/v1/stats`, so Core answers
for any `tenant_id` it is given. Tenant isolation holds only through the
gateway, which forces the authenticated tenant; anyone who reaches Core
directly can read every tenant's rows. Never expose it on a public service.
Core does not rate-limit `/api/v1/streams?with_bytes=true` either; it runs at
most two whole-tenant scans at once and queues the rest. Local stacks that
publish Core's port on the host (the wallet project's
`docker-compose.allsource.yml` maps 3280 to 3282) give the same access to
anything on that host.

## Turning recording off and on

At runtime, with a Core admin key, over the proxy:

```bash
curl -s -X PUT localhost:3900/api/v1/admin/read-cost \
  -H "Authorization: Bearer $CORE_ADMIN_KEY" \
  -H 'content-type: application/json' \
  -d '{"enabled": false}'
```

401 means no key was sent, 403 that the key lacks admin permission. The answer
echoes `{"enabled": false}`. The switch lives in memory: a restart returns to
the boot value.

At boot:

```bash
fly secrets set ALLSOURCE_READ_COST=off -a allsource-core
```

Setting a secret restarts the machine; see [`CORE_DEPLOY.md`](CORE_DEPLOY.md)
for what clients see during the window. `fly secrets unset ALLSOURCE_READ_COST
-a allsource-core` turns it back on. Off, queries still carry their facts and
the layer drops them; the routes keep answering from what was recorded
before.

## Logs

```bash
fly logs -a allsource-core | grep -E 'read_cost_threshold|read_label_overflow|read_header_invalid'
fly logs -a allsource-control-plane | grep read_header_invalid
```

| Line | Means | Look next |
|---|---|---|
| `read_cost_threshold reason=walk_pages` | One walk passed 20 pages | `read_costs` ordered by `pages` for that caller and class |
| `read_cost_threshold reason=matched_pages` | One page matched more than 50,000 events | The caller's filter: a whole-class read where a `since` or entity id would do |
| `read_cost_threshold reason=walk_abandoned` | A walk stopped with pages left for more than 30 s | The caller's budget or timeout cut the walk |
| `read_cost_threshold reason=single_truncated` | A one-page read hit its limit with more behind it | The caller treats a first page as the whole answer |
| `read_label_overflow` | The process has 256 stream-class labels; later classes count as `other` | A client sending ids where classes belong |
| `read_header_invalid` | A header failed its pattern and was rewritten to `invalid` | The client's caller name or read id generator |

Threshold lines carry the tenant only as `tenant_fingerprint`, the first six
bytes of its SHA-256 in hex. To match a tenant, hash its id the same way; the
id itself is never logged.

## Limits

- A timer sweeps every 10 s, so an abandoned walk is counted 30 to 40 s after
  its last page.
- Each tenant holds at most 256 walks in flight; a 257th evicts that
  tenant's own least recently seen walk as `walk_evicted`. The table holds
  4,096 walks in all.
- Single truncated also counts the first page of a walk from a client that
  sends no read id.
- Core keeps 128 tenants, 128 classes and 256 caller rows per tenant; the
  least recently active tenant is dropped first. `read_costs` says which
  limit cut the rows: `rows_approximate`, `classes_overflowed` or
  `tenant_reentered`.
- The tables live in one Core process. With more than one Core machine, each
  answers for the pages it served, and a restart empties them; `read_costs`
  shows the answering instance as `recording.instanceId` beside
  `recording.since`. Rows from two replies add up only when the ids match.
- `stream_sizes` on a remote store reads at most 10,000 streams; past that,
  completeness says `stream_limit_reached` and class totals are partial.
  `scan_interrupted` means Core saw events removed during its scan and its
  retry; run it again. On an embedded store, `refresh_failed` means the
  store could not catch up with its writer and the sizes may be stale.
