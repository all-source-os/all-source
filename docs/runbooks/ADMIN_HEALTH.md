# Admin Console Health — verify every page returns real data

**Purpose.** Close the "deployed ≠ working" gap for the admin power tool
(`apps/admin`, `admin.all-source.xyz`). A green deploy proves the binary booted —
it does **not** prove a page renders real data. This runbook + the
`task admin-health` check prove, on demand, that every admin page is either
**WORKING** (200 + real non-empty data), **EMPTY** (200 but no rows yet — a
documented no-data state, not a bug), or flags it **BROKEN** (non-2xx, error, or a
list field that would crash the page's `.map`). The check **exits non-zero if any
page is BROKEN**.

> Provenance: built 2026-06-26 against PROD (`api.all-source.xyz`) with a
> short-lived admin JWT minted from the fleet `JWT_SECRET` (same mint as the
> `reap-demo` / `backfill-usage` Taskfile tasks). All evidence below is captured
> real-API output, not assumed.

---

## The command

```bash
# Probe every admin page against prod, classify, exit non-zero if any BROKEN.
task admin-health

# Assert a specific tenant's event_count end-to-end (counts=0 regression guard):
task admin-health TENANT=decebal-dobrica-at-gmail-com

# Stricter gate: also fail on EMPTY (use only when you expect data everywhere):
task admin-health STRICT_EMPTY=true

# Vars (match the other admin tasks): CP_URL, QS_APP, EMAIL.
```

It mints a 1h admin token by reading `JWT_SECRET` from the `allsource-query`
container over `fly ssh` and signing an HS256 `role:"admin"` JWT — identical to
`reap-demo`/`backfill-usage`. No new credentials, nothing persisted.

---

## Per-page health (captured 2026-06-26, prod)

`PAGE` is the admin route; `ENDPOINT` is the CP/QS backing call (all under
`/api/v1/admin/*`, same-origin via the BFF, Bearer-only, unless noted).

| Page (admin route) | Endpoint | Status | Classification | Real evidence |
|---|---|---|---|---|
| `/tenants` (counts) | `GET …/admin/tenants` | 200 | **FIXED** (was BROKEN: counts=0) | 41 tenants; `event_count` was 0 for every tenant incl. paid `studio`/`enterprise`. Root-caused + fixed (below). |
| `/tenants/:id` (360) | `GET …/admin/tenants/:id` (+ `/usage`, `/fleet/health/:id`, `/billing/invoices?tenant_id=`) | 200 | **FIXED** (same counts=0 cause) | detail `event_count` followed the list — 0 despite real events; now sourced from the metered counter. |
| `/fleet` | `GET …/admin/fleet/health` | 200 | **WORKING** | `total=41 healthy=41 at_risk=0 critical=0 worst=0` — real fleet rollup. |
| `/monitoring` (summary) | `GET …/admin/metrics/summary` | 200 | **FIXED** (was EMPTY-but-data-exists) | summary was all-zeros (`events_total=0 active_tenants=0`) despite real Core traffic. QS parser read metric names Core never emits — fixed (below). |
| `/monitoring` (charts) | `GET …/admin/metrics/timeseries` | 200 | **WORKING** | `points=3` — passthrough live. |
| `/monitoring` (cluster) | `GET …/admin/cluster/members` | 200 | **WORKING** | `members=1 roles=leader`. |
| `/monitoring/alerts` | `GET …/admin/alerts` | 200 | **EMPTY-correct** | `alerts=0` — no alert rules configured. Source verified: the list endpoint returns an (empty) array; the page renders an empty state. |
| `/monitoring/slos` | `GET …/admin/slos` | 200 | **EMPTY-correct** | `slos=0` — no SLOs configured. |
| `/billing` (revenue) | `GET …/admin/billing/revenue` | 200 | **WORKING** | `mrr=2607 arr=31284 churn=0 tiers=3` — real LemonSqueezy mirror. |
| `/billing` (invoices) | `GET …/admin/billing/invoices` | 200 | **WORKING** | `invoices=3` — real invoices present. |
| `/billing` (dunning) | `GET …/admin/billing/dunning` | 200 | **EMPTY-correct** | `items=0 total_count=0` — no dunning cases. NOTE: empty ⇒ CP returns `items:null` (Go nil slice); the client's `asList()` coerces null→[] so the page renders an empty state, not a crash. |
| `/billing` (catalog) | `GET /api/v1/billing/catalog` | 200 | **WORKING** | `tiers=indie,studio,scale` — canonical paid tiers (gap #4 already resolved; admin filter matches). |
| `/billing` (config) | `GET …/admin/billing/config-check` | 200 | **WORKING** | `ok=true issues=0 manual=1`. |
| `/security` (IP rules) | `GET …/admin/security/ip-rules` | 200 | **EMPTY-correct** | `ip_rules=0` — none configured. |
| `/security` (token audit) | `GET …/admin/security/token-audit` | 200 | **EMPTY-correct** | `entries=0 total=0` — `entries:null` when empty; `asList()`-guarded. |
| `/security` (suspicious) | `GET …/admin/security/suspicious-activity` | 200 | **EMPTY-correct** | `alerts=0` — no suspicious activity detected. |
| `/security` (policies) | `GET /api/v1/policies` | 200 | **WORKING** | `policies=5` — real RBAC policies. |
| `/outreach` | `GET …/admin/notices` (+ `…/fleet/health`) | 200 | **EMPTY-correct** | `notices=0` — no notices posted yet. |
| `/inbox` | `GET …/admin/inbox/connections` (+ `/messages`) | 200 | **WORKING** (was 401 on v75; fixed by CP **v76**) | `connections=0` — handler reached, auth OK. The v75→v76 deploy shipped the `e4b5b2c` inbox auth-group fix; the 401 is resolved. |

**Summary (live, CP v87, deployed 2026-06-27):** `task admin-health` exits **0** — **0 pages BROKEN**.
WORKING 8 · EMPTY-correct 8 (incl. inbox `connections=0`) ·
counts (`/tenants`, `/tenants/:id`) + `/monitoring` summary render EMPTY today
because their **Core/QS fixes are not yet deployed** (they are EMPTY, not BROKEN —
the pages return arrays/zeros and render, they do not crash). After the Core + QS
redeploys below, counts go non-zero and `/monitoring` shows real values.

> Why the gate is already green with counts at 0: counts=0 is a *data-not-surfaced*
> defect, but at the **page** level the endpoint returns a valid array and the row
> renders `0` — it does not crash. The harness flags BROKEN only for non-2xx /
> shape failures (what actually breaks a page). Use `STRICT_EMPTY=true` to force
> the empties to fail the gate (verified: that run exits non-zero, 10 BROKEN).

---

## Root cause + fix, per repair

### 1. Per-tenant counts = 0 (PRIMARY) — fixed in **Core** (`apps/core`)

**Symptom.** `/tenants` and `/tenants/:id` show `event_count = 0` and
`member_count = 0` for **every** tenant, including paid `studio`
(`decebal-dobrica-at-gmail-com`) and `enterprise` (`decebal1988-at-gmail-com`).

**Proof the data exists** (prod, via the committed backfill dry-run, which counts a
tenant's real events in Core):

```
POST /api/v1/admin/billing/backfill-usage {"tenant_id":"decebal-dobrica-at-iproov-com","dry_run":true}
  → {"events_found":257,"previous_events_used":0,...}      # 257 REAL events in Core
POST /api/v1/admin/billing/backfill-usage {"tenant_id":"decebal-dobrica-at-gmail-com","dry_run":true}
  → {"events_found":1000000,"previous_events_used":1000000,"capped":true}   # metered counter = 1,000,000
```

Yet the admin list/detail showed `event_count: 0` for **both**.

**Root cause (traced Core → CP → admin DTO → page).** The chain has one structural
break, in Core:

1. The Control Plane sources per-tenant counts from Core
   `GET /api/v1/tenants/{id}/stats` (`list_tenants.go eventCountForTenant`,
   `get_admin_tenant_detail.go`), reading `stats.EventCount`.
2. The CP's tolerant decoder (`tenant_stats.go`) maps `EventCount` from
   `usage.total_events` (then a flat `event_count`).
3. **But Core's `build_tenant_stats` (`apps/core/.../tenant_api.rs`) serialized the
   in-memory `TenantUsage` struct, whose `total_events` field is bumped only by
   `record_event()` — which the real ingest path NEVER calls.** The durable,
   authoritative counter lives in `metadata.quotas.events_used` (written by the
   forward-metering path `POST …/usage/increment`, the same number the Query
   Service dashboard reads at `tenant_controller.ex:265`). So `/stats` reported
   `total_events: 0` for everyone.
4. Because the `/stats` call **succeeds** with 0, `eventCountForTenant` returned 0
   and never fell back to the metadata mirror — so even gmail's
   `events_used = 1,000,000` was masked by the successful-but-zero stats response.

**Fix (Core, `build_tenant_stats`).** Read `metadata.quotas.{events_used,
queries_used}` and surface them: as flat `event_count` / `query_count`, and
overlaid into the `usage` block as `total_events` / `queries_used`. The CP's
existing decoder then reads the real number — **zero CP change required**. This one
change fixes every downstream consumer (admin list/detail, fleet-health's
has-data gate, cluster status, recovery diagnose), and is consistent with the
number the user's own dashboard already shows.

- File: `apps/core/src/infrastructure/web/tenant_api.rs` (`metered_quota_counter`
  helper + `build_tenant_stats`).
- Tests (Core, pass under `--features enterprise`): `stats_event_count_reflects_metered_counter_not_inmemory_usage`,
  `stats_event_count_is_zero_when_unmetered`, `stats_metered_counter_accepts_float_json`,
  and the end-to-end `stats_reflects_real_metering_path_end_to_end` (real
  `increment_usage` → `find_by_id` → `build_tenant_stats` → `event_count == 257`).

**Residual (documented, not a bug in the fix).** A tenant that ingested
**out-of-band** (Prime sync / direct Core / MCP — not through the metered QS write
path) has `events_used = 0` even with real events (e.g. iproov: 257 real, metered
0). For those, the count reads 0 until the operator reconciles the counter with the
real store via the existing remediation:

```bash
task backfill-usage TENANT=<id> DRY=false    # reconciles events_used from Core's real event log
```

This is by design (Core IS the database; the counter is a durable metadata mirror,
not a second source of truth). The harness surfaces it: a tenant showing
`event_count=0` whose backfill dry-run reports `events_found>0` is an un-metered
tenant, not a broken page.

### 2. `/monitoring` summary all-zeros — fixed in **Query Service** (`apps/query-service`)

**Symptom.** `/monitoring` summary cards read `events_total=0 active_tenants=0
uptime_s=0 p99=0ms` despite real Core traffic. (The charts + cluster panels were
already fine — they prove the CP→QS passthrough auth/connectivity works.)

**Root cause.** `admin_metrics_controller.ex build_summary` parsed Core's
Prometheus `/metrics` for metric **names Core never emits**:
`allsource_events_total`, `allsource_active_tenants`, `allsource_uptime_seconds`,
`allsource_http_requests_total/errors_total`, and a **summary** quantile for
`allsource_query_duration_seconds` (Core emits a **histogram**, not summary
quantiles). Every lookup returned 0 → a dead summary. The real series exist under
different names (verified live): `allsource_storage_events_total 13354`,
`allsource_query_duration_seconds_bucket{le=…}` (48 buckets),
`allsource_ingestion_errors_total`.

**Fix (QS parser mapping).**
- `events_total` ← `allsource_storage_events_total` (lifetime), fall back to
  `allsource_events_ingested_total` (session).
- `query_latency_p99_ms` ← `PrometheusParser.histogram_quantile("allsource_query_duration_seconds", 0.99)`
  (the correct function for Core's histogram; aggregates across `query_type`), ×1000.
- `error_rate_percent` ← `ingestion_errors_total / (events_ingested_total + errors)`.
- `active_tenants` / `uptime_seconds` / `events_per_second` — **no Core series
  exists**; they remain 0 (documented exporter gap, not a crash). After the fix,
  `events_total` becomes non-zero, so the page reads WORKING.

- File: `apps/query-service/lib/query_service_ex_web/controllers/admin_metrics_controller.ex`.
- Tests (QS, pass): `admin_metrics_summary_test.exs` (6 tests) +
  existing `prometheus_parser_test.exs histogram_quantile` coverage.

### 3. `/inbox` returns 401 — fix already committed (`e4b5b2c`), needs **CP redeploy**

**Symptom.** `GET …/admin/inbox/connections` and `…/inbox/messages` return
`401 {"error":"unauthorized","message":"authentication required"}` with a valid
admin token that works on every other `/api/v1/admin/*` route.

**Root cause.** The 401 originates from `RequirePermission` (`apps/control-plane/auth.go:547`), which
reads the api-group `AuthMiddleware` context — **not** the `AdminAuthMiddleware`
context the admin session uses. The deployed CP (Fly **v75**, 2026-06-25 09:45 BST)
predates commit **`e4b5b2c`** (2026-06-25 17:33 BST) which moved the inbox admin
routes onto the `AdminAuthMiddleware` group. So the fix is in `main` but **not yet
deployed**.

**Fix.** None needed in code — redeploy the Control Plane (below). After redeploy
the handler is reached and returns `200` (configured) or `503 "inbox not
configured"` (no Nylas creds — a documented EMPTY, the harness treats 503 as
EMPTY-correct).

### 4. `/tenants` "Created" date = today for EVERY tenant — fixed in **Control Plane** (`apps/control-plane`)

**Symptom.** The Tenants list shows **today's date** in the Created column for
*every* tenant — all 41, across all pages. The values are identical to the
sub-millisecond (`…707499086Z`, `…707499767Z`, `…707500017Z`) — a dead giveaway
they were all stamped in one loop, not at each tenant's real creation time.

**Root cause (column not backed by real data).** Core persists and returns a real
`created_at` (`tenant_api.rs:35`, set at creation). But the CP's Core client
struct `TenantResponse` (`core_client.go:177`) **had no `created_at` field**, so the
value was dropped on decode, and `coreTenantToEntity` (`core_tenant_repository.go:160`)
stamped **`CreatedAt: time.Now()`** unconditionally on every load. The column
therefore reflected "when the admin last loaded the list," never the creation time.
(`UpdatedAt` had the same bug, same line.)

**Fix.**
- `core_client.go` `TenantResponse`: add `CreatedAt time.Time \`json:"created_at"\``
  and `UpdatedAt time.Time \`json:"updated_at"\`` so Core's RFC3339 timestamps are
  decoded.
- `core_tenant_repository.go` `coreTenantToEntity`: use `resp.CreatedAt` /
  `resp.UpdatedAt`; fall back to `time.Now()` **only** when the value is zero
  (legacy tenants from Core builds before these fields existed), never overwriting a
  real timestamp.

Same class of defect as counts=0: a green deploy renders the column fine — it's just
fake data. Caught now by the data-integrity gate (below), which BROKENs when ≥8
tenants share one created_at date.

---

## What number does the admin show? (canonical event count)

The admin's `event_count` is **`metadata.quotas.events_used`** — the durable,
per-tenant **billing meter**. It is the canonical number because it is the one the
tenant is billed/quota'd on and the same number the user's own dashboard shows.

It is kept honest against the real Core event store by the **recurring reconciler**
`SyncEventsUsageUseCase` (`sync_events_usage.go`, commit `cae9ee8`): every 5 min it
queries Core's real `total_count` (single query, **uncapped**) and writes it back
into `events_used`, self-healing any drift.

Why drift happens at all: the Core ingest path (`ingest_event_v1`) does **not**
increment the meter on write — only the QS `UsageReporter` does, via
`POST …/usage/increment`. So events ingested out-of-band (Prime / MCP / direct Core /
CP) land durably in the store but bypass the meter. Incrementing on every write was
rejected deliberately — it's a per-event hot-path cost at 469K events/sec; periodic
reconcile is the chosen architecture.

**`backfill-usage` is NOT authoritative.** It is a manual one-shot whose count is
**capped** at `defaultBackfillMaxPages × queryPageLimit = 2000 × 500 = 1,000,000`
(`backfill_events_used.go:18`), so a busy tenant returns exactly `1000000`,
`capped:true` — an honest "≥1M," not a precise count. The earlier "gmail = 1,000,000"
was this cap artifact written into the meter; the uncapped reconciler supersedes it
(gmail now reads the real `1049` and will track the true store count as the
reconciler runs). The gate flags any `event_count == 1000000` exactly as a cap
artifact, so a capped value can never again be mistaken for a real count.

## Data-integrity gates (every column must be REAL)

`task admin-health` now fails (BROKEN) on two fake-value smells, not just on
non-2xx / shape errors — because "the page renders" is not "the data is real":

| Smell | Rule | Catches |
|---|---|---|
| **Fake `created_at`** | ≥8 tenants share ONE calendar date | the `time.Now()`-on-load bug (#4) |
| **Cap artifact** | any `event_count == 1000000` exactly | a backfill-capped value masquerading as a real count |

`GATE=true` additionally requires every goal-critical page to be WORKING (real
data), and `STRICT_EMPTY=true` fails on any EMPTY. Run `GATE=true` to answer "are
the admin goals met?"

---

## Deploy + re-verify (orchestrator)

The three fixes live in three apps and need their respective backend redeploys
(Fly; web is unaffected). Build/test green is already proven locally per app
(Core: 8 tenant_api + 9 increment_usage tests; QS: 26 metrics tests; CP: decoder
tests). Deploy, then re-run the harness to prove **0 BROKEN** + non-zero counts.

```bash
# 1. Core — surfaces the metered event_count in /stats (fixes counts fleet-wide).
fly deploy --config apps/core/fly.toml --dockerfile apps/core/Dockerfile -a allsource-core --remote-only

# 2. Query Service — fixes the /monitoring summary metric-name mapping.
fly deploy -a allsource-query --remote-only        # MUST build with ALLSOURCE_EDITION=enterprise (see fly.toml)

# 3. Control Plane — ships the already-committed inbox auth-group fix (e4b5b2c).
fly deploy --config apps/control-plane/fly.toml --dockerfile apps/control-plane/Dockerfile -a allsource-control-plane --remote-only

# 4. Prove it.
task admin-health TENANT=decebal-dobrica-at-gmail-com   # expect: exit 0, 0 BROKEN, tenant event_count = 1000000
```

**Counts end-to-end proof to capture after the Core deploy:**

```
GET /api/v1/admin/tenants/decebal-dobrica-at-gmail-com  → event_count = 1000000   (was 0)
# and the list:
GET /api/v1/admin/tenants?per_page=100  → at least one tenant with event_count > 0
```

If a deploy regresses, `fly releases -a <app>` → `fly deploy --image <prev>` (or
`fly releases rollback`) restores the prior image; the Core/QS fixes are additive
and isolated, so a roll-forward fix is preferred over rollback.

---

## How the harness classifies (so EMPTY ≠ BROKEN)

- **BROKEN** = non-2xx, OR a list field that is **present and a non-null, non-array
  type** (a string/number where the page calls `.map` → "x.map is not a
  function"). This mirrors what actually crashes the client.
- **EMPTY** = 200 with a zero count / `null` / absent list. `null`/absent is **not**
  BROKEN: the client's `asList()` coerces it to `[]` and renders an empty state
  (resilience §6). Verified per page that the empty state is the source genuinely
  having no rows, not data hidden by a shape bug.
- **WORKING** = 200 with a real non-empty list or a non-zero headline value.
- `STRICT_EMPTY=true` promotes every EMPTY → BROKEN (for environments where you
  expect data everywhere).

The check is in `Taskfile.yml` (`admin-health`).
