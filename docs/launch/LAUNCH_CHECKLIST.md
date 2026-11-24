# AllSource Launch Checklist

**Last reviewed:** 2026-11-24
**Current versions:** Core 0.30.0 · latest release tag v0.30.0
**Active branches:** `main` — x402 + agent registration are already merged. The `feat/agent-auth-x402` branch is historical.

Single source of truth for what's left to launch. Supersedes the 6 archived docs under `docs/launch/archived/`. Pair with `docs/launch/CHRONIS_CLOUD_LAUNCH_PLAN.md` for the chronis cloud sync launch specifically.

Status legend: `[ ]` open · `[~]` in progress · `[x]` done · `[-]` dropped/deferred

---

## Phase A — Infrastructure & Credentials

- [ ] Fly apps exist and running in region `iad`, all `started` with health checks passing: `allsource-core`, `allsource-query`, `allsource-control-plane`, `allsource-prime`, `allsource-auth`. Verify with `fly status -a <app>`.
  - Query Service app is named `allsource-query`, not `allsource-query-service`.
  - Web runs on Fly as `allsource-web` (`apps/web/fly.toml`), serving `all-source.xyz` and `www`.
- [x] Google OAuth app registered; `GOOGLE_CLIENT_ID` / `GOOGLE_CLIENT_SECRET` saved
- [x] GitHub OAuth app registered; `GITHUB_CLIENT_ID` / `GITHUB_CLIENT_SECRET` saved
- [ ] LemonSqueezy: API key, store ID, webhook secret (needed for paid-tier upgrade flow)
- [ ] Coinbase CDP server wallet on Base (mainnet for launch, Sepolia for staging) — for x402 payouts
- [x] `SECRET_KEY_BASE` deployed on `allsource-query`, `ALLSOURCE_JWT_SECRET` deployed on `allsource-core` (verified via `fly secrets list` 2026-04-16)
- [x] Custom domains live: `api.all-source.xyz` → `allsource-control-plane` (Fly, Let's Encrypt cert). DNS via Unstoppable Domains. See `docs/runbooks/SLO_SLA.md` for record values.
- [x] Status page lives at `https://www.all-source.xyz/status` (Fly, `allsource-web`). Polls Control Plane's `/api/v1/status/services` JSON feed; the feed is backed by an in-process probe cache so the page stays up during a Core outage. The previous standalone status hostname (Vigil on Fly) was retired 2026-04-25 — no separate Fly app, no separate cert.

## Phase B — Deploy core stack

Backend and frontend on Fly. Each backend app runs the v0.30.0 build:

- [ ] `allsource-core` — v0.30.0, healthy, persistent volume `allsource_data`
- [ ] `allsource-query` — v0.30.0, healthy 2/2
- [ ] `allsource-control-plane` — v0.30.0, serves `GET /x402/routes`
- [ ] `allsource-prime` — v0.30.0
- [ ] `allsource-auth` — v0.30.0
- [x] Web frontend deployed on Fly app `allsource-web` at `https://all-source.xyz`
- [x] Core autoscale `min_machines_running = 1` (`apps/core/fly.toml:101`)
- [x] Fly alerts on Core/Query `/health` failures configured in Fly dashboard

### Required secrets (reference)

**`allsource-core`**
- `ALLSOURCE_JWT_SECRET`

**`allsource-query`**
- `SECRET_KEY_BASE`
- `PHX_HOST=allsource-query.fly.dev`
- `CORE_URL=http://allsource-core.internal:3900`
- `CORE_WS_URL=ws://allsource-core.internal:3900/api/v1/events/stream`
- `GOOGLE_CLIENT_ID` / `GOOGLE_CLIENT_SECRET`
- `GITHUB_CLIENT_ID` / `GITHUB_CLIENT_SECRET`
- `LEMON_SQUEEZY_API_KEY`, `LEMON_SQUEEZY_STORE_ID`, `LEMON_SQUEEZY_WEBHOOK_SECRET`

**`allsource-control-plane`**
- `JWT_SECRET`
- `CORE_URL=http://allsource-core.internal:3900`
- `QUERY_SERVICE_URL=http://allsource-query.internal:3902`
- `FRONTEND_URL=https://all-source.xyz`
- `X402_ENABLED=true`
- `X402_PRICING_CONFIG=/app/config/x402-pricing.json`
- `X402_RECIPIENT_ADDRESS`
- `X402_FACILITATOR_URL=https://x402.coinbase.com`
- `CDP_API_KEY_NAME`, `CDP_API_KEY_PRIVATE_KEY`

**Web (`allsource-web`)** — public values are build arguments in `apps/web/fly.toml`
- `NEXT_PUBLIC_API_URL=https://allsource-query.fly.dev`

## Phase C — x402 & agent auth

- [x] x402 + agent registration merged to main (`apps/control-plane/internal/infrastructure/x402/`)
- [x] Pricing config exists at `apps/control-plane/docs/x402-pricing.example.json` — copy to deploy path and point `X402_PRICING_CONFIG` at it
- [x] Agent registration handler `POST /api/v1/agents/register` (`main.go:330`, `agents.go:14-42`)
- [x] Agent payment history `GET /api/v1/agents/me/payments` (`main.go:335`)
- [x] `GET /x402/routes` discovery endpoint added (`handler.go` Routes handler, `main.go:341`)
- [ ] Verify 402 flow against deployed Control Plane: unauth request to priced route → `402 Payment Required`
- [ ] Staging auto-pay test: blow past free tier on Base Sepolia, confirm `/api/v1/agents/me/payments` shows a settled payment

## Phase D — Tenant bootstrap & chronis sync

- [x] Bootstrap admin key set — `ALLSOURCE_BOOTSTRAP_API_KEY` is Deployed as a Fly secret on `allsource-core` (verified via `fly secrets list`). Note: `ALLSOURCE_BOOTSTRAP_TENANT` is not listed as a separate secret — Core defaults to tenant `default` when unset
- [ ] Confirm bootstrap key is stored in 1Password; rotate after first real tenant exists
- [ ] Create team tenant + per-user API keys via `POST /api/v1/tenants` and `POST /api/v1/auth/api-keys`
- [ ] Each team member: populate `.chronis/config.toml` (`mode = "remote"`, `remote_url`, `api_key`) — see `CHRONIS_CLOUD_LAUNCH_PLAN.md`
- [ ] End-to-end: Alice `cn task create` → Bob `cn sync` → Bob `cn list` shows it

## Phase E — Backend data integration

- [x] **Unified auth shipped with a secret boundary**: OAuth JWTs contain human identity, tenant, role, and expiry only. Users explicitly create scoped credentials under `/dashboard/api-keys`; raw keys are shown once and are never returned by the session API or persisted in browser storage.
- [x] **LemonSqueezy webhook → tenant plan**: `webhook_lemonsqueezy.go:85-124` processes subscription events; `handleSubscriptionUpdated` (line 146) updates tenant tier + quotas via `updateSubUC.Execute()`
- [x] Dashboard stats wired (`apps/web/src/hooks/use-dashboard-stats.ts` — Promise.all over `getTenantUsage`/`listProjections`/`getMetrics`)
- [x] Live event feed uses Phoenix Channel WebSocket (`live-event-feed.tsx:31` → `usePhoenixChannel("events:all", ...)`)
- [x] Event explorer fetches real data via `useEvents()` — no demo-data fallback path
- [x] `live-metrics.tsx` magic-number fallbacks dropped — metric fields are now nullable, UI renders `—` when real metrics are unavailable
- [x] E2E smoke test: `tooling/scripts/smoke-test-auth-billing.sh` passes 7/7 (onboard → ingest → query → billing). QS billing route gated behind Edition.enterprise?() — documented gap.

## Phase F — Product polish (post-launch OK)

- [x] Settings page (`apps/web/src/app/dashboard/settings/page.tsx`: profile, security, notifications)
- [x] Projections/Pipelines page (`pipelines/page.tsx` — fetches via `apiClient.listProjections()`, status cards, pause/resume controls)
- [x] Privacy Policy page (`apps/web/src/app/(marketing)/privacy/page.tsx`)
- [x] Terms of Service page (`apps/web/src/app/(marketing)/terms/page.tsx`)

> **SLO/SLA work moved to `docs/runbooks/SLO_SLA.md`.** Targets and PagerDuty/Slack alerting are tracked as TODOs there, not in this checklist.

## Phase G — Positioning & messaging (web app copy)

- [x] Hero subtitle "Time-travel your data" with benchmarks (`hero.tsx:150-152`, `186-191`: 469K events/sec, 11.9μs)
- [x] Benchmarks component on landing (`HeroStats()` in `hero.tsx`, count-up animation)
- [x] MCP tools showcase: 73 tool definitions (`apps/mcp-server-elixir/lib/mcp_server_elixir/protocol/mcp_tools.ex`)
- [x] "database" → "event store" audit: only one user-facing fix needed (`dashboard/demo/page.tsx` "your database" → "your event store"); rest of the copy already uses contrastive framing correctly
- [x] Use cases page (`apps/web/src/app/(marketing)/use-cases/page.tsx` — audit trails, event replay, AI agent memory, financial history)
- [x] AllSource vs EventStoreDB comparison page (`apps/web/src/app/(marketing)/compare/eventstoredb/page.tsx`)
- [x] Pricing tiers in `apps/web/src/lib/config.ts` (`siteConfig.pricing`): Self-Host (free, self-run), Indie £18.99/mo, Studio £78.99/mo, Scale £298.99/mo, Enterprise (custom). Tier entitlements: `TierQuotaMap` in `apps/control-plane/internal/domain/entities/subscription.go`.
  - [ ] LemonSqueezy: confirm the indie / studio / scale variants exist in the dashboard and are mapped in `LEMON_SQUEEZY_VARIANT_MAP`. Setup steps: `docs/launch/LEMONSQUEEZY_SETUP.md`.

## Phase H — Launch marketing assets

- [x] 60s demo video: Remotion composition at `apps/marketing-assets/`, rendered to MP4 (656KB optimized). Integrated on use-cases page.
- [x] Hero screenshot 2540×1520 (retina 2x): `apps/web/public/assets/hero-screenshot.png` + WebP. Used in hero, solution, how-it-works sections.
- [x] 4 feature GIFs: event explorer, live stream, API key creation, onboarding — rendered as MP4 (150KB total), integrated in features section.
- [x] 15 blog header images: unique per article, 1200×630 WebP (164KB total).
- [x] Draft ProductHunt listing: `docs/marketing/archive/pre-relaunch/PRODUCTHUNT_LISTING.md` — tagline, description, topics, maker comment.
- [x] Draft X.com launch thread: `docs/marketing/archive/pre-relaunch/TWITTER_LAUNCH_THREAD.md` — 8 tweets, each <280 chars.
- [ ] Draft Show HN post (after 2–3 testimonials collected)
- [ ] Line up 5–10 upvoters for ProductHunt launch day

## Phase I — Launch execution

- [~] Day 1: Deploy + smoke test + OAuth verification + "Early Access" banner (banner shipped, smoke tests pass, OAuth needs manual verification)
- [ ] Day 2: X.com thread, monitor signups
- [ ] Day 3–5: Billing checkout tested end-to-end against LemonSqueezy
- [ ] Day 7–10: ProductHunt launch (Tue–Thu, schedule 12:01 AM PT, first 2h engagement)
- [ ] Day 14+: Collect testimonials, post Show HN
- [ ] Success metrics: 5 signups day 2 · 50 signups day 10 · demo video 100+ views

## Phase J — Deferred / nice-to-have

- [-] SALES-001 Export C4 diagrams to PNG/SVG (P3)
- [-] SALES-002 Generate performance charts via Python/plotly (P3)
- [-] SALES-003 Convert pitch deck to presentation format (P3, blocked by SALES-002)
- [-] Interactive time-travel query playground (P1 Turso plan, month 2)
- [-] Early adopter case studies (month 2+)

## Phase K — SDKs

SDKs already exist in `sdks/` (go, python-client, rust, typescript). Per memory: **only Rust crates publish to crates.io**. JS/Python/Go SDKs distribute via GitHub registry.

- [x] SDK READMEs reference `https://api.all-source.xyz` (TS fixed from `allsource-query.fly.dev`, Go and Python already correct)
- [x] TypeScript SDK README — Authentication section + quickstart + API reference + error handling
- [x] Python SDK README — Authentication section + sync/async quickstart + API reference
- [x] Go SDK README — Authentication section + quickstart + error handling + query options
- [ ] Sample apps (JS/Python/Go) — deferred, docs cover the essentials

---

## Known gaps at launch

| Gap | Impact | Workaround |
|---|---|---|
| Pricing tiers differ from Turso plan recommendation | Marketing/positioning question | Decide tier strategy before launch day |

---

## Doc map

- `LAUNCH_CHECKLIST.md` — this file, single checklist
- `CHRONIS_CLOUD_LAUNCH_PLAN.md` — detailed runbook for chronis → cloud sync with x402
- `archived/` — superseded planning docs (marketing copy drafts in `MARKETING_MATERIALS.md` still usable)
