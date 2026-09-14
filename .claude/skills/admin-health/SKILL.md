---
name: admin-health
description: >-
  Verify the AllSource admin console (admin.all-source.xyz) actually meets its goals — every page healthy and showing REAL data, proven against production, never claimed from a green deploy. Runs the `task admin-health` gate which mints an admin token, probes every admin page's backing endpoint, and classifies each WORKING / EMPTY / BROKEN. Use when the user asks "is the admin working", "are the admin goals met", "check admin health", "run the admin gates", before claiming any admin/control-plane/core/query-service change is done, and as a pre-deploy gate. Triggers: "admin health", "admin gates", "verify admin", "is the admin usable".
---

# Admin Health Gates

**Why this exists:** the admin power tool was once reported "shipped + deployed" while every page actually showed empty/zero data (counts 0 everywhere). "Deployed" ≠ "working." This gate proves, with real production data, that each admin page does its job — or names exactly which one doesn't and at which layer.

## Run the gate

```
task admin-health GATE=true
```

- Mints a 1h admin JWT from the fleet `JWT_SECRET` (read-only from the `allsource-query` container via `fly ssh`) — the same operator-approved pattern as `reap-demo`/`backfill-usage`. Requires `fly` (authed), `jq`, `openssl`, `curl`.
- Probes every admin page's CP/QS endpoint against PROD (`api.all-source.xyz`) and prints a per-page table with the real values it read (counts, metric numbers, list lengths).
- **Exit 0 = gates pass. Exit 1 = a goal-critical page is not WORKING (admin goals NOT met) or a page is BROKEN.**

Modes:
- `task admin-health` — base check: fails only on BROKEN (non-2xx / wrong shape / a list field that isn't a list).
- `task admin-health GATE=true` — **the goal gate**: the goal-critical pages MUST be WORKING (real data), else fail. Data-dependent pages may be EMPTY. **This is the one to run to answer "are the admin goals met?"**
- `task admin-health STRICT_EMPTY=true` — strictest: every page must have data (only for a fully-seeded env).
- `TENANT=<id>` — assert that tenant's per-tenant 360 shows real `event_count` end-to-end (Core `/stats` → CP → admin). GATE mode auto-checks a known-active tenant.

## The goals (per page) and the gate rubric

| Page | Goal (what "met" means) | Endpoint | Gated? |
|---|---|---|---|
| **/tenants** | shows REAL per-tenant event/member counts (≥1 tenant non-zero) | `/api/v1/admin/tenants` | **GATE** |
| **/tenants/:id** | per-tenant 360 shows real counts + health + billing | `…/tenants/:id` (+ `/usage`, `/fleet/health/:id`) | **GATE** (a known-active tenant) |
| **/fleet** | computes health across all tenants | `…/fleet/health` | **GATE** |
| **/monitoring** | live platform metrics (events, latency) + cluster | `…/admin/metrics/summary`, `…/cluster/members` | **GATE** |
| **/billing** (catalog) | canonical tiers `indie/studio/scale` | `/api/v1/billing/catalog` | **GATE** |
| **/security** (policies) | RBAC policies present | `/api/v1/policies` | **GATE** |
| /monitoring/alerts, /slos | alert rules / SLOs | `…/admin/alerts`, `…/slos` | data-dependent (EMPTY ok) |
| /billing (revenue/invoices/dunning) | revenue + invoices + dunning | `…/billing/{revenue,invoices,dunning}` | data-dependent |
| /security (ip-rules, token-audit, suspicious) | security telemetry | `…/security/*` | data-dependent |
| /outreach (notices) | proactive comms | `…/admin/notices` | data-dependent |
| /inbox | inbox connections | `…/admin/inbox/connections` | data-dependent (503 = no Nylas creds = EMPTY, not broken) |

**Classification (the honest part):**
- **WORKING** = 200 + real non-empty data → goal met.
- **EMPTY** = 200 but no rows/zeros. **Pass only for data-dependent pages** (genuinely no data configured yet — NOT a bug). For a goal-critical page, EMPTY **fails the gate**.
- **BROKEN** = non-2xx, error body, or a list field that isn't a list (would crash the page's `.map`). Always a fail.

Never mark a page healthy by faking data. Empty-because-no-data-exists is a documented pass; data-exists-but-not-surfaced is BROKEN — fix it.

## When to run (gate moments)

- **Before claiming any admin change is done** — the rule that this whole gate enforces. "It builds / it deployed" is not "it works."
- **After touching** `apps/admin`, `apps/control-plane`, `apps/core` (metering/stats), or `apps/query-service` (metrics) — these are the layers admin pages depend on.
- **After deploying** Core / QS / CP / the admin Vercel project — re-run to confirm prod still meets goals.
- **Periodically** as a health probe (it's idempotent + read-only except the token mint).

## On FAIL — where to look (by layer)

A goal-critical page that's BROKEN/EMPTY traces to one layer:
- **counts 0 / tenants / 360** → Core metering. Counts come from `metadata.quotas.events_used` (the mirror), surfaced by Core `build_tenant_stats` (`apps/core/src/infrastructure/web/tenant_api.rs`). The canonical number is `events_used`, reconciled to Core's real uncapped `total_count` every 5 min by `sync_events_usage.go` (`cae9ee8`). For an immediate one-shot, `task backfill-usage TENANT=<id> DRY=false` (but its count is capped at 1,000,000).
- **`event_count == 1000000` exactly (BROKEN: cap artifact)** → a round 1M is the backfill page-cap, NOT a real count. Let the reconciler resolve it; never trust an exact 1M.
- **created_at all the same date / "Created" shows today for everyone (BROKEN: data-integrity)** → CP dropped Core's real timestamp. `core_client.go` `TenantResponse` must decode `created_at`/`updated_at`; `core_tenant_repository.go coreTenantToEntity` must use them, falling back to `time.Now()` only when zero — never unconditionally. (Fixed 2026-06-26.)
- **monitoring zeros/broken** → QS metric mapping (`apps/query-service/.../admin_metrics_controller.ex`) or the CP passthrough (`apps/control-plane/.../metrics_handler.go`).
- **a list page crashes / "x.map is not a function"** → the admin client must return arrays (`asList` in `apps/admin/src/lib/*-api.ts`) + the page guards fields; see the resilience standards in `docs/proposals/ADMIN_TENANT_POWER_TOOL.md` §6.
- **401 on an admin endpoint** → wrong auth group / BFF not attaching the Bearer; the CP is Bearer-only (`docs/runbooks/CONTROL_PLANE_CORS.md`).

## References
- Per-page evidence + root causes: `docs/runbooks/ADMIN_HEALTH.md`
- Design + goals + resilience standards: `docs/proposals/ADMIN_TENANT_POWER_TOOL.md`
- The gate itself: `Taskfile.yml` → `admin-health` task
