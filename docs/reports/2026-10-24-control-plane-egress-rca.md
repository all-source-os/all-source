---
title: "Control-plane egress alert: root cause"
status: CURRENT
last_updated: 2026-10-24
---

# Control-plane egress alert: root cause

`ControlPlaneDailyBandwidthCritical` has fired since 2026-10-21T17:20Z on
`allsource-bandwidth-alerts` and re-sends every six hours. On 2026-10-24 at
17:20Z its value was 66.0 GB of `allsource-control-plane` edge output in 24
hours, against a 50 GB threshold.

The bytes are event-query responses to one caller: the Longhand control plane,
authenticated as tenant `onboard-control-plane-at-technical-leaders-com`,
calling `https://api.all-source.xyz` from Fly `iad`. It re-reads the same
unbounded histories every few seconds without caching them. Since 15:40Z the
egress guard has been refusing a share of those reads, so the alert value is now
falling because Longhand is being throttled, not because the reads stopped.

Everything below was read from production on 2026-10-24 between 17:00Z and
17:45Z unless a date says otherwise. Sources are the alert evaluator, Fly's
Prometheus for the `allsource` org, the control plane's own `/metrics`, and
both services' logs and code.

## What the bytes are

| Measure | Value | Source |
|---|---|---|
| Edge output, 24h, region `iad` | 65,932,026,596 B | `sum by (region)(increase(fly_edge_data_out{app="allsource-control-plane"}[24h]))` |
| Edge output, 24h, all other regions | 10.2 MB (`lhr` 5.5 MB, `ams` 4.6 MB) | same |
| Event-query response bodies, 15:40:35Z to 17:26:26Z | 2,759,325,514 B | `control_plane_event_query_response_bytes_total` |
| Edge output, `iad`, same window | 2,772,412,584 B | `increase(fly_edge_data_out[6351s])` |
| Share of edge bytes that are query bodies | 99.5% | ratio of the two rows above |
| Edge responses, 24h, `iad` | 3,317,281 (3,311,013 with status 200) | `fly_edge_http_responses_count` |

No other route carries meaningful bytes: since the 15:40Z restart the control
plane served 60,216 successful event queries, 265 `POST /api/v1/events` and 127
`/auth/session` requests, and no stream, Prime, stats or entity routes. Bodies
are sent uncompressed: process transmit bytes (2.790e9) are roughly the same as
body bytes.

The `event_query_large_response` log line is a sampler, not an accounting log.
It logs only bodies of at least 16 KiB and at most one line per 10 seconds per
process (`apps/control-plane/query_egress.go`). Its lines show which query
shapes exist, not how many bytes each one moves.

## Who the caller is

- Every sampled line carries `tenant_fingerprint=2732b1a7938f`. The fingerprint
  is the first 6 bytes of `sha256(tenant_id)`, and the control plane overwrites
  `tenant_id` with the authenticated tenant before sampling
  (`apps/control-plane/delegation.go`, `ProxyEventsQuery`).
  `sha256("onboard-control-plane-at-technical-leaders-com")` starts
  `2732b1a7938f`.
- The sampled event fingerprints decode to event families that exist only in
  the Longhand control plane: `auth.member.`, `auth.org.`,
  `auth.member.updated`, `queue_binding.`, `entitlement.team_member.`,
  `team_queue.work_item.`, `skill.`, and `auth-session:` entity reads.
- Longhand `control-plane/api/fly.toml:23` sets `primary_region = "iad"`, and
  `:160-161` set `ALLSOURCE_CORE_URL` and `ALLSOURCE_QUERY_URL` to the public
  `https://api.all-source.xyz`, not a `.internal` address. A request from an
  `iad` machine to that hostname enters Fly's edge in `iad` and its response is
  billed as `allsource-control-plane` edge output.
- Ruled out: `allsource-web` received 32 MB in the same 24 hours,
  `allsource-query` logs show only health checks, CI points at a local Core,
  and human traffic from London enters through `lhr` (5.5 MB).

Not established: no client IP or user agent was observed, and the Longhand Fly
app is outside this token's organisation, so its machine count and regions were
not read directly. The attribution rests on the authenticated tenant, the event
families and the Longhand configuration.

## Why it re-reads so much

Request count rose while bytes fell. Edge 200 responses were 1.18 M per day on
10-14 and 3.72 M on 10-23. The traffic is a polling read pattern whose
responses are sometimes large.

**1. Per-request session reads fetch the whole session history.** Every
authenticated Longhand web or admin request resolves its session through
`better-auth-allsource` `get_latest(auth-session:<token>)`, which queries the
entity with `ENTITY_HISTORY_LIMIT = 1000`, newest first, with no time bound
(`better-auth-allsource-0.14.12/src/client.rs:106,204`). Longhand caches the
result for 15 seconds per machine (`control-plane/api/src/infrastructure/auth_cache.rs:119`).
A session appends `auth.session.expiry_updated` once per hour of use with a
24-hour sliding lifetime, so a session that is polled continuously never
expires and its history keeps growing. Sampled reads were 57–86 KB each.

**2. Per-request RBAC reads are uncached by design.** `list_workspaces_for_user`
runs on every authorised request unless `LONGHAND_SHARED_QUEUE_PROJECTION=true`.
It reads three `auth.member` arms, two of them without a payload filter and each
with a limit of 5000, plus a cursor walk of `auth.org.`
(`members_skills.rs:82-88,137-144`, `member_stream.rs:64-86`). The module states
"Zero authorization inputs are cached" (`auth_cache.rs:30`). The variable is not
set in `control-plane/api/fly.toml [env]`; whether it is set as a secret was not
read.

**3. The schedule tick re-reads every queue binding once per tenant.** Every 60
seconds on every machine, `schedule_tick` lists tenants and, for each one, calls
the unscoped `list_all_queue_bindings_strict()` and filters the result locally
(`control-plane/api/src/application/schedule_tick.rs:124-166`). The whole
binding set (29 KB per sample) crosses the edge once per tenant, per machine,
per minute. `queue_lifeline` and `hosted_scheduler` each list tenants every 300
seconds as well.

**4. Bounded team-queue and skill reads re-ship whole windows.** Since 15:40Z,
55% of bytes (1.517 GB) come from `event_type_prefix` reads that are both
time-bounded and payload-filtered: 6.4% of requests at about 361 KB each. The
Longhand team queue builds exactly this shape (`team_queue.rs:476-478`), and so
does skill catalog paging (`skill_catalog_paging.rs:35`). Each poll fetches the
full window again instead of reading forward from a stable position. The
control plane's metrics carry no namespace label, so the split between
team-queue and skill bytes is unmeasured.

Shares since 15:40Z, by the control plane's shape counters:

| Shape | Bytes share | Requests | Mean body |
|---|---|---|---|
| prefix, time-bounded, payload-filtered | 55% | 4,198 | ~361 KB |
| prefix, unbounded, payload-filtered | 21% | 16,258 | ~36 KB |
| prefix, unbounded, unfiltered | 16% | 20,171 | ~22 KB |
| entity id, unbounded | 5.3% | 13,751 | ~11 KB |
| event type | 1.8% | — | — |

## History

Daily control-plane edge output, each figure the 24 hours ending at 00:00Z on
the date shown, about 100% from `iad`:

| 10-15 | 10-16 | 10-17 | 10-18 | 10-19 | 10-20 | 10-21 | 10-22 | 10-23 | 10-24 |
|---|---|---|---|---|---|---|---|---|---|
| 318 GB | 338 GB | 216 GB | 86 GB | 304 GB | 426 GB | 935 GB | 558 GB | 62 GB | 77 GB |

- The alert app was deployed at 10-21 17:19Z, so 17:20Z is when the alert began
  to exist, not when output crossed 50 GB. The 300–935 GB days came before any
  alerting.
- 10-21 23:32–23:34Z: output fell from 6.0 MB/s to 1.2 MB/s and bytes per
  response from about 360 KB to 11–15 KB, while requests per second rose from
  33 to 55–60. This follows the Longhand deploy of `acd4952` ("validate
  candidate replays by entity version", run 39691587796, 23:32:30Z), which reuses
  replayed pages instead of fetching them again. 66 GB a day is what remains
  after that change.
- 10-22 21:05Z to 10-23 02:55Z: a burst at 2.0–2.3 MB/s and 60–72 requests per
  second. No AllSource or Longhand deploy aligns with it; its cause is not
  identified.
- 10-24 15:40:36Z: control plane v120 (`sha-bd0e8bf`) started serving with the
  egress guard. Successful requests from `iad` fell from about 21,000 to 5,600
  per 10 minutes. 429 responses appeared at 190–880 per 10 minutes, and mean
  response time rose from about 4 ms to about 11 ms. No Longhand deploy happened
  then; the last successful one was at 11:38Z.

## The 15:40Z drop is throttling

`apps/control-plane/egress_guard.go` holds funded responses in one process-wide
pool of 16 slots (`make(chan struct{}, 16)`) and answers 429 with
`Retry-After: 30` when the pool is full. Each admitted response also makes two
ledger round trips to Core (reserve and complete). A single tenant polling at
35 requests per second fills that pool, and the guard is not per tenant, so
every tenant shares the same 16 slots. Version 118 carried the same guard on
10-23 and produced 1,505 429s in ten minutes before it was replaced eleven
minutes later.

Successful requests fell by about 75%, while 429s account for only a few
percent of requests. The client is therefore backing off, not just being
refused. Whether Longhand's AllSource SDK honours `Retry-After` was not read,
nor its circuit-breaker thresholds. On the Longhand side this most likely shows
up as slower or stale board, queue and skill views. Longhand's logs were not
visible to this investigation.

The alert will resolve as the pre-15:40Z hours leave its 24-hour window. That
recovery comes from refused and abandoned reads, not from a smaller read
pattern, and it will undo itself if the guard is widened.

## Secondary findings in the same window

**Usage sync for `decebal-dobrica-at-gmail-com` fails every five minutes.** Core
returns `503 archive_hydration_refused` with `needed_bytes=2943581248`
(`apps/core/src/error.rs:200-218`, raised from `store.rs:1818-1852`). The
requirement is the tenant archive (116,864,964 B in 138 files) times 16, plus a
1 GiB reserve. `available_bytes` is kernel memory headroom from
`/proc/meminfo` `MemAvailable` (about 2.4 GB), not `ALLSOURCE_CACHE_BYTES`. Core
RSS was 5.0 GB about 18 minutes after its 17:07Z launch, against a tenant-cache
estimate of 633 MB. Two control-plane jobs need a whole-tenant count or an
event-type-and-since read (`sync_events_usage.go:69-76`,
`sync_extraction_usage.go:104-111`), so neither can use the bounded retained
reads from #340 or #346. The control plane's client retries each 503 three times
and ignores `Retry-After` (`core_client.go:420-457`). Consequence: that tenant's
`events_used` and `extraction_tokens_used` stop reconciling. Separately,
`x402_usage_sync` falls back to two full scans of the 138-file archive every
minute and matches nothing.

**The `__system` store is never checkpointed.** `SystemMetadataStore`
(`apps/core/src/infrastructure/persistence/system_store.rs`) is WAL-only. It has
no flush, seal or truncate path, and keeps every event in an unbounded in-memory
`Vec`. The only `seal`, `remove_sealed` and `truncate` callers are the data
store's (`store.rs:557,1551,1554`). On disk it is 41 segments (about 200 MB),
growing at about 18 events per second (about 0.8 GB a day). Each boot replays
it in full: 151,329 events at the 13:58Z boot, and the health check reports
252,539 system events now. The "checkpoints are not keeping up" warning is
therefore wrong for this store: nothing checkpoints it. The writer behind 18
events per second was not identified. This store is a plausible part of the gap
between the 5.0 GB RSS and the 633 MB estimate, and so of the hydration refusals
above; that link is unmeasured.

**The control plane's byte metrics are not scraped.** `apps/control-plane/fly.toml`
has no `[metrics]` section, and the org Prometheus has no `control_plane_*`
series. Shape history before each restart is lost, and the counters carry no
tenant or namespace label.

## Fix sites

The traffic should be fixed where it originates, in the Longhand control plane's
read pattern. AllSource-side changes should make the cost visible and the guard
fair, not hide the reads.

| # | Change | Repo | Effect |
|---|---|---|---|
| 1 | Resolve sessions from the latest event only, or a bounded read, instead of 1000 events of history; stop growing session entities by one event per hour of polling | `better-auth-allsource`, Longhand `control-plane/api` | removes the 57–86 KB per-request session reads |
| 2 | Cache RBAC membership per user with event-driven invalidation, or enable the shared projection behind `LONGHAND_SHARED_QUEUE_PROJECTION` | Longhand `control-plane/api` | removes the per-request `auth.member` and `auth.org.` reads |
| 3 | Read queue bindings once per tick, not once per tenant; run the tick on one machine | Longhand `schedule_tick.rs` | divides the binding reads by tenants times machines |
| 4 | Read team-queue and skill windows forward from a stable position (#345 exposes positions in bounded feeds) | Longhand `team_queue.rs`, `skill_catalog_paging.rs` | removes the largest byte share |
| 5 | Honour `Retry-After` on 429 and classify 429 as a refusal | Longhand AllSource client usage | stops retries from adding requests during throttling |
| 6 | Give the egress guard per-tenant slots instead of one global pool of 16 | AllSource `apps/control-plane/egress_guard.go` | one heavy tenant no longer starves the rest |
| 7 | Compress `/api/v1/events/query` JSON responses | AllSource `apps/control-plane` | fewer bytes for the same reads |
| 8 | Scrape control-plane metrics and add a namespace label to the byte-by-shape counters | AllSource `apps/control-plane/fly.toml`, `query_egress.go` | per-caller byte history survives restarts |
| 9 | Count tenant events from a Core statistic, and honour `Retry-After` on 503 | AllSource `apps/control-plane` scheduler and `core_client.go` | usage sync stops forcing whole-tenant hydration |
| 10 | Snapshot and truncate the `__system` WAL, and find its 18-per-second writer | AllSource `apps/core` `system_store.rs` | bounded boot replay, memory and disk |

## Not checked

- Client IP and user agent for the query requests, and the Longhand app's
  machine count and regions.
- Longhand's logs, and whether its SDK honours `Retry-After`, so the user-facing
  effect of the 429s is inferred, not observed.
- The production value of `LONGHAND_SHARED_QUEUE_PROJECTION`.
- The split of the time-bounded byte share between team-queue and skill reads.
- The cause of the 10-22 21:05Z burst.
- Which caller issues the unbounded `auth.member.` prefix read sampled about
  every 60 seconds (`list_all_members`, `list_revoked_member_user_ids` or
  `teams_invitations.rs:350`).
- The `__system` writer and the breakdown of Core's 5.0 GB RSS.
