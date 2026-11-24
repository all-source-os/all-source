---
title: "AllSource Lifecycle Email Trail"
status: DRAFT — not sent
last_updated: 2026-11-23
owner: founder (@decebal)
send_vehicle: control-plane proactive-comms (ADMIN_TENANT_POWER_TOOL.md §4 Pillar C — Phase 6)
grounding:
  - apps/web/src/lib/config.ts (siteConfig — canonical pricing + product numbers)
  - apps/control-plane/internal/domain/entities/subscription.go (TierQuotaMap — backend entitlements)
  - docs/runbooks/HOSTED_EXTRACTION_METERING.md (Hound extraction allowances)
  - docs/proposals/PRICING_EXPOSURE_PLAN.md (5-tier model)
  - docs/proposals/ADMIN_TENANT_POWER_TOOL.md (comms endpoints/events/guards)
  - CLAUDE.md (architecture facts)
---

# AllSource Lifecycle Email Trail — DRAFT (not sent)

> **DRAFT for human review. Nothing here has been sent, scheduled, or wired to
> the comms system.** This is a strategy + full-copy document a reviewer can
> either schedule through the control-plane proactive-comms surface or hand to an
> editor with no further research. Every product claim traces to a real
> `siteConfig` value, a backend tier entitlement, or `CLAUDE.md` — see the
> grounding table in §3 and the per-email fact tags. Voice is founder-to-builder
> (`@decebal`), first person, plain-text-friendly, one CTA per email.
>
> **Read this caveat first:** the only lifecycle email the system sends
> automatically *today* is the quota warning (`check_usage_warnings.go`), and its
> shipped copy is on **retired tiers** ("upgrade to Pro $29 / Growth $79 billed
> yearly" — `check_usage_warnings.go:239`). Everything else in this trail ships
> when **Phase 6 (proactive comms)** of `ADMIN_TENANT_POWER_TOOL.md` lands. The
> trail is designed to be correct on day one of that phase; §3 flags every
> trigger that needs a new signal first.

---

## 1. Strategy summary

These are emails to **paying (or about-to-pay) builders** on a developer
event-store. The job of the trail is not to "nurture" — it's to close the gap
between *what a tenant pays for* and *what they actually touch*. Most AllSource
tenants ingest a handful of events and never reach the parts they're billed for:
11.9µs p99 Core indexed reads, the 55 MCP tools, x402 pay-per-call, Hound code/doc graphs,
the multi-language SDKs. A linear blast can't fix that, because the right next
email depends entirely on behavior — a tenant who hasn't ingested a single event
needs a different push than one who is at 80% of a 5M-event Studio quota. So the
trail **branches on real, event-sourced signals** (the same Core events and
quota meters the product already emits) and **segments by tier** so a £18.99 Indie
and a £298.99 Scale subscriber never get the same "what you bought" email. The
honest-founder voice and the hard grounding are not stylistic: over-promising to
someone who already paid is the fastest way to churn them and generate support
load, so accuracy beats cleverness on every line.

---

## 2. Lifecycle map

Five stages, branched by entry path and tier. "Segment" names the cohort the
email targets; tier-specific rows are called out explicitly.

```
 SIGNUP ──▶ ACTIVATION ──▶ PAID (post-purchase) ──▶ EXPANSION
   │            │                  │                     │
   │            │                  │                     ├──(quota/feature ceiling)
   └────────────┴──────────────────┴─────────────────────┴──▶ DORMANCY / RE-ENGAGEMENT
                                                               (any stage can fall here)
```

| Stage | Email | Segment | Goal (one behavior) | Entry trigger | Exit / suppression | Cadence |
|---|---|---|---|---|---|---|
| **Signup** | S1 Hosted welcome | Hosted free/trial tenant | Ingest first event | Tenant created (hosted) | Exits on first event → Activation | T+0 (within 1h of signup) |
| | S2 Self-host welcome | Self-host (GitHub) | Star/keep, join Discussions | Self-host install | — (out-of-band, see §3) | T+0, one-shot |
| **Activation** | A1 First-event nudge | Free/trial + Indie, 0 events | Ingest first event | Tenant age ≥ 2d AND `events_used == 0` | Exits on first event; max 1 send | Once, T+2d |
| | A2 First recall ("the aha") | Any tenant ingesting, ~0 queries | Run first recall/query | Crossed ~1K events AND `queries_used == 0` | Exits on first query | Once, on milestone |
| | A3 MCP introduction | Activated tenant | Connect MCP to Claude | Active ≥ 7d AND no MCP read seen | Exits on first MCP read | Once, T+7d |
| **Paid** | P-Indie welcome | **Indie** | Wire agent recall via MCP read | Subscription → `indie` | One-shot per purchase | T+0 (at checkout) |
| | P-Studio welcome | **Studio** | Enable MCP **write**-back | Subscription → `studio` | One-shot per purchase | T+0 |
| | P-Scale welcome | **Scale** | Provision **dedicated** MCP | Subscription → `scale` | One-shot per purchase | T+0 |
| | P-Ent welcome | **Enterprise** | Hand to SE (no auto-CTA) | Subscription → `enterprise` | SE-owned; comms suppressed | T+0, SE-routed |
| **Expansion** | E1 80% events | Indie/Studio/Scale (tier copy) | Upgrade one tier | `billing.usage_warning.sent` @ 80% | One per billing period per threshold | On threshold |
| | E2 100% events | Indie/Studio/Scale (tier copy) | Upgrade now / enable overage | `billing.usage_warning.sent` @ 100% | One per billing period per threshold | On threshold |
| | E3 x402 allowance | Paid w/ x402 usage | Keep overage on / size up | `x402_used ≥ 80%` of allowance | One per billing period | On threshold |
| | E4 Hound allowance | Paid using hosted extraction | Upgrade tokens or BYO LLM | `extraction_tokens_used ≥ 80%` | One per billing period | On threshold |
| | E5 Unused depth | Paid, ingesting, feature gap | Try the one feature they skip | Feature-gap cohort (see §3) | ≤1/wk; skip if feature used | Monthly sweep |
| **Dormancy** | D1 Stream went quiet | Any active-then-silent | Reconnect / send one event | `last_event_age` → At-Risk | Exits on any new event | Once on At-Risk |
| | D2 Still here? | Silent ~30d | Reply / book 15 min | `last_event_age` → Critical | Exits on reply or new event | Once on Critical |
| | D3 Paid-but-dormant | Paid + dormant + renews soon | Get value or downgrade honestly | Dormant AND renewal ≤ 14d | One-shot pre-renewal | Pre-renewal |

**Branch precedence (no double-send — see §6):** Operational (E2 100%, D3
pre-renewal, dunning) **>** Post-purchase (P-*) **>** Expansion (E1/E3/E4/E5)
**>** Activation (A*) **>** Dormancy (D1/D2). One lifecycle/marketing email per
tenant per 7-day window wins; operational sends are deduped per threshold but
exempt from the weekly cap.

---

## 3. Trigger → real-signal mapping

Legend: **REAL** = the signal exists and fires today · **REAL (scheduled)** =
data exists; a scheduled cohort/threshold job emits it (the usage-warning
pattern) · **NEEDS NEW SIGNAL** = no event/threshold fires this today; what's
needed is named.

| Email | Trigger | Backing signal | State | Notes / what's missing |
|---|---|---|---|---|
| S1 | tenant created (hosted) | Onboarding (`onboard.go` `CreateTenantUC`) | **NEEDS NEW SIGNAL** | No `tenant.created` Core event is emitted today (grep of `onboard.go` finds no `IngestEvent`/audit emit). Wire a `tenant.created` event or fire S1 from the onboarding handler. |
| S2 | self-host install | GitHub clone / `cargo install` | **NEEDS NEW SIGNAL (out-of-band)** | Self-host tenants have no CP record or email — **unreachable by proactive-comms by design**. S2 ships via GitHub README / Discussions / release notes, not this system. Listed for completeness. |
| A1 | ≥2d old, 0 events | `events_used` (`GET /tenants/{id}/stats`) + `created_at`; `last_event_age` = "never ingested" (`signals.go:51`) | **REAL (scheduled)** | Data is live (stats + the `last_event_age` health signal already classify "never ingested"). Needs the Phase-6 `onboarding_nudge` scheduled cohort job to actually send. |
| A2 | ~1K events, 0 queries | `events_used` + `queries_used` meters (`tenant.rs:185`, `sync_events_usage.go`) | **NEEDS NEW SIGNAL** | Meters are real; no "first-recall / event-count milestone" event fires. Add a threshold check mirroring `check_usage_warnings.go`. |
| A3 | active ≥7d, no MCP read | MCP read activity | **NEEDS NEW SIGNAL** | No per-tenant "first MCP read" signal found. Needs an MCP-usage marker (or approximate with `queries_used > 0` from MCP path). |
| P-Indie/Studio/Scale | subscription → tier | LemonSqueezy webhook `subscription_created/updated` → `resolveTierByName` (`webhook_lemonsqueezy.go:279`) | **REAL (entry) / NEEDS EMIT** | The webhook is the real entry point and re-derives quotas via `reconcile-subscription`. No discrete `subscription.activated`/tier-change Core event exists to subscribe to — emit one from the webhook handler, or call the comms send inline. |
| P-Ent | subscription → enterprise | same webhook | **REAL (entry)** | Auto-CTA suppressed; routed to SE. |
| E1 | events @ 80% | `billing.usage_warning.sent` @ `WarningThreshold80` (`check_usage_warnings.go:15,113`) | **REAL** | Fires today on a schedule and already sends an email — **but on retired-tier copy** (`:239`). E1 copy below replaces it. Dedup state lives in `metadata.usage_warnings.LastWarningPct`. |
| E2 | events @ 100% | `billing.usage_warning.sent` @ `WarningThreshold100` (`:16`) | **REAL** | Same path/threshold; operational severity. |
| E3 | x402 @ 80% | `x402_used` meter (`sync_x402_usage.go`, `subscription.go:113`) | **REAL (scheduled)** | Meter is live (summed from settled x402 payment events, 5-min reconciler). No 80% threshold event yet — add the threshold check like usage-warnings on `X402Allowance`. |
| E4 | extraction @ 80% | `prime.extraction.usage` Core event (`doc_extract.rs:443`) → `extraction_tokens_used` (`sync_extraction_usage.go`) | **REAL event / scheduled threshold** + **BLOCKED** | The per-extraction event is real and metered. Threshold check is net-new. **Also: hosted extraction is not live in prod** (two Fly secrets unset, `HOSTED_EXTRACTION_METERING.md`) and the per-tier token amounts are placeholders — E4 must not ship until that's turned on and the amounts are confirmed. |
| E5 | feature-gap cohort | derived from stats: `events_used>0` AND (`queries_used≈0` OR `extraction_tokens_used==0` OR `x402_used==0`) | **NEEDS NEW SIGNAL** | All inputs are real meters; no feature-adoption cohort query exists. Needs a Phase-6 scheduled sweep. |
| D1 | inactivity → At-Risk | `last_event_age` signal → fleet tier `at_risk` (`signals.go:29`, `staleFromHealth` `analyze_tenants.go:826`) | **REAL** | Fleet-health already classifies stale tenants. The `at_risk_outreach` template (`ADMIN_TENANT_POWER_TOOL.md §4`) targets exactly this. |
| D2 | inactivity → Critical | `last_event_age` → fleet tier `critical` (`signals.go:30`) | **REAL** | Same source, deeper threshold. |
| D3 | dormant + renews soon | fleet tier (above) + subscription renewal date (detail `Subscription`) + dunning (`admin_dunning.go`) | **REAL** | Both inputs exist; the join (dormant ∧ renewal ≤14d) is a Phase-6 cohort query. |

**The send surface itself is the biggest gap.** Per
`ADMIN_TENANT_POWER_TOOL.md §4 Pillar C` and its gap table, the comms endpoints
(`POST /api/v1/admin/{notices,messages}`, tenant-facing `GET /api/v1/notices`),
the in-app notice events (`admin.notice.*`), the `comms_opt_out` flag, and the
per-category cooldown are **net-new (Phase 6, not shipped)**. The only shipped
outbound path is `smtpEmailClient.SendEmail`, used solely by the quota warning.
So: the *signals* for E1/E2/D1/D2 are real now; the *machine that sends on them*
ships in Phase 6.

---

## 4. Full email copy

Format per email: Segment · Trigger · Subject (A/B where high-impact) ·
Preheader · Body (plain-text, one CTA) · Judging metric · Fact tags. Body copy
is written to be loaded into a template; `{{first_name}}`, `{{tenant_name}}`,
`{{events_used}}`, etc. are the comms-system variables.

---

### Stage: Signup

#### S1 — Hosted welcome
- **Segment:** new hosted free/trial tenant · **Trigger:** tenant created (NEEDS `tenant.created` emit)
- **Subject (A):** You're in. Here's the one thing to do first.
- **Subject (B):** AllSource is recording — give it something to remember
- **Preheader:** One event in, and you'll see what the agents are for.

```
Hey {{first_name}},

Decebal here — I build AllSource.

You've got a workspace. Right now it's an empty event store: a durable log
that records everything your agents do, and hands it back in about 12
microseconds (11.9µs p99) when they need to remember.

The fastest way to get it is to write one event:

  curl -X POST {{api_url}}/api/v1/events \
    -H "Authorization: Bearer {{api_key}}" \
    -d '{"entity_id":"user-1","event_type":"signed_up","data":{}}'

That's the whole loop — append an event, query it back, time-travel the
history. From there it scales to 469K events/sec on the same engine, all on a
~129MB footprint.

Send one event. That's the only homework.

→ {{quickstart_url}}

— Decebal

You can self-host the whole thing for free (Apache-2.0 core) if you'd rather run it
yourself: github.com/all-source-os/all-source
```
- **Metric:** activation event — first `events_used > 0` within 72h.
- **Facts:** 11.9µs p99 / 469K/sec / 129MB (`siteConfig.stats`); Apache-2.0 core self-host (`siteConfig.pricing[self-host]`).

#### S2 — Self-host welcome *(out-of-band — not sent by this system)*
- **Segment:** self-host installer · **Trigger:** install (no CP record — see §3)
- **Subject:** You're self-hosting AllSource — here's how to not get stuck
- **Preheader:** Apache-2.0 core, yours forever. Two links so you don't hit the wall alone.

```
You cloned AllSource and you're running it on your own hardware — unlimited
events, forever retention, full MCP, zero dollars. That's the deal, and it
stays the deal.

Two things that save people time:
- The 55 MCP tools drop straight into Claude Desktop. Wiring guide: {{mcp_docs_url}}
- Stuck? GitHub Discussions is where I actually answer: {{discussions_url}}

If you outgrow a single box and want us to run it, hosted starts at £18.99/mo
(Indie). No pressure — self-host is a first-class tier, not a trial.

— Decebal
```
- **Metric:** GitHub star / Discussions join (tracked on GitHub, not comms).
- **Facts:** unlimited/forever/full-MCP self-host + £18.99 Indie (`siteConfig.pricing`); 55 MCP tools (`siteConfig.stats[2]`).
- **Note:** self-host tenants are unreachable by the CP comms system (no tenant email). Ships via README/release notes/Discussions.

---

### Stage: Activation

#### A1 — First-event nudge
- **Segment:** free/trial + Indie, `events_used == 0`, ≥2d old · **Trigger:** REAL (scheduled) — `onboarding_nudge` cohort
- **Subject (A):** Your event store is still empty
- **Subject (B):** Two minutes to your first event (here's the curl)
- **Preheader:** Nothing's wrong — you just haven't written event #1 yet.

```
Hey {{first_name}},

Your AllSource workspace has been up for a couple of days and hasn't seen a
single event yet. That's the one step between "I signed up" and "oh, I get it."

Paste this:

  curl -X POST {{api_url}}/api/v1/events \
    -H "Authorization: Bearer {{api_key}}" \
    -d '{"entity_id":"demo","event_type":"hello","data":{}}'

Then query it back from /api/v1/events/query. That round-trip — append,
recall — is the entire product. Everything else (Prime memory, MCP, x402) is
built on it.

If something blocked you, just reply to this email. It comes to me.

→ {{quickstart_url}}

— Decebal
```
- **Metric:** activation — first `events_used > 0` within 48h of send.
- **Facts:** event append/query API (`CLAUDE.md` Core API); features named, not over-claimed.

#### A2 — First recall ("the aha") — HIGH-IMPACT (A/B)
- **Segment:** ingesting (~1K events) but `queries_used == 0` · **Trigger:** NEEDS NEW SIGNAL (event-count milestone)
- **Subject (A):** You've logged {{events_used}} events — now ask them a question
- **Subject (B):** Your agent's memory is sitting there unread
- **Subject (C):** Recall is the part you came for. You haven't run it yet.
- **Preheader:** 11.9µs p99 indexed read to answer "what happened?" — try one query.

```
Hey {{first_name}},

You've written {{events_used}} events into AllSource. That's the hard part
done — you have a real, durable history now (write-ahead log + Parquet, so
nothing's lost on restart).

Here's the part most people don't reach: asking it questions.

  GET {{api_url}}/api/v1/events/query?entity_id=<id>&limit=1&order=desc

That returns the latest state of any entity in about 12 microseconds
(11.9µs p99). Not milliseconds — microseconds. Fast enough that your agent
can check memory on *every* turn instead of rationing it.

That's the difference between "memory is a tool the agent calls" and "memory
is just there." Run one query and you'll feel it.

→ {{recall_docs_url}}

— Decebal
```
- **Metric:** activation — first `queries_used > 0` within 7d.
- **Facts:** 11.9µs p99 (`siteConfig.stats[1]`); WAL+Parquet durability (`CLAUDE.md`); query API + `order=desc` latest-state (`CLAUDE.md` Core API).

#### A3 — MCP introduction
- **Segment:** activated, no MCP read · **Trigger:** NEEDS NEW SIGNAL (first MCP read)
- **Subject (A):** Let Claude read and write this memory directly
- **Subject (B):** 55 MCP tools, 30-second install
- **Preheader:** Stop hand-rolling API calls — wire it into Claude Desktop.

```
Hey {{first_name}},

You're ingesting and recalling over the API. The next jump is letting Claude
do it for you.

AllSource ships 55 MCP tools. Drop them into Claude Desktop and the agent
reads and writes its own memory — no glue code, no embedding API to stand up.
Prime (our memory layer) installs in about 30 seconds.

  cargo install allsource-prime
  allsource-prime --data-dir ~/.prime/memory --mode http --port 3905

Then point Claude Desktop at it. After that, "remember this" and "what did we
decide last week?" just work.

→ {{mcp_docs_url}}

— Decebal
```
- **Metric:** activation — first MCP read within 14d.
- **Facts:** 55 MCP tools (`siteConfig.stats[2]`); Prime install-in-30s / no embedding API (`siteConfig.header` Prime entry); `allsource-prime` install (repurpose kit / blog).

---

### Stage: Paid (post-purchase) — tier-specific "what you bought"

#### P-Indie — Welcome to Indie — HIGH-IMPACT (A/B)
- **Segment:** **Indie** (£18.99) · **Trigger:** REAL entry (webhook → `indie`); NEEDS emit
- **Subject (A):** Welcome to Indie — here's exactly what you unlocked
- **Subject (B):** Your Indie plan: 500K events, 50K agent payments, 1M Hound tokens
- **Preheader:** The real ceilings, in plain numbers, so nothing surprises you.

```
Hey {{first_name}},

Thanks for going Indie — that's real money and I don't take it lightly. Here's
precisely what £18.99/mo buys, no asterisks:

- 500K events / month, 50K queries
- 50K x402 calls included — your agent can pay per call; overage is
  $0.0001/call after that
- 1M Hound extraction tokens — turn a repo or a pile of docs into a queryable
  graph
- Retention not limited by plan, 3 streams
- Hosted MCP (read) so Claude can read your memory directly
- Email support, 48h

The one move that makes Indie pay for itself: wire your agent's recall through
hosted MCP read. That's the 11.9µs p99 indexed read "what happened?" lookup, running inside Claude
instead of in your code.

→ {{mcp_read_setup_url}}

Reply any time — it reaches me.

— Decebal

(Annual is £15.17/mo if you'd rather pay yearly.)
```
- **Metric:** click → MCP-read setup; activation = first MCP read within 14d.
- **Facts:** £18.99/£15.17 · 500K events / 50K queries · 50K x402 + $0.0001/call · 1M extraction tokens · retention not limited by plan · 3 streams · MCP read · 48h support — all `siteConfig.pricing[indie]` + `subscription.go TierIndie` + `HOSTED_EXTRACTION_METERING` (indie 1M).

#### P-Studio — Welcome to Studio — HIGH-IMPACT (A/B)
- **Segment:** **Studio** (£78.99, "Popular") · **Trigger:** REAL entry (webhook → `studio`); NEEDS emit
- **Subject (A):** Welcome to Studio — write access is the upgrade
- **Subject (B):** Studio: 5M events, MCP write-back, 10M Hound tokens
- **Preheader:** The jump from Indie isn't just volume — it's MCP write.

```
Hey {{first_name}},

Studio is the tier most teams settle on, and the reason isn't the 10x volume —
it's the write verb. Here's the full set:

- 5M events / month, 500K queries (10x Indie)
- Hosted MCP: read AND write — the agent doesn't just recall, it commits new
  memory back through MCP, no custom endpoint
- 500K x402 calls included, $0.0001/call after
- 10M Hound extraction tokens
- Retention not limited by plan, unlimited streams
- Email support 24h + Discord

Do this first: turn on MCP write and let one agent record a decision back into
its own memory. Once the loop is read *and* write, the agent stops forgetting
between sessions on its own.

→ {{mcp_write_setup_url}}

— Decebal

(Annual is £63.17/mo.)
```
- **Metric:** click → MCP-write setup; activation = first MCP write within 14d.
- **Facts:** £78.99/£63.17 · 5M events / 500K queries · MCP read+write · 500K x402 · 10M extraction tokens · retention not limited by plan · unlimited streams · 24h+Discord — `siteConfig.pricing[studio]` + `subscription.go TierStudio` + `HOSTED_EXTRACTION_METERING` (studio 10M).

#### P-Scale — Welcome to Scale — HIGH-IMPACT (A/B)
- **Segment:** **Scale** (£298.99) · **Trigger:** REAL entry (webhook → `scale`); NEEDS emit
- **Subject (A):** Welcome to Scale — dedicated MCP and 50M events
- **Subject (B):** You're running a fleet. Here's the dedicated lane.
- **Preheader:** 50M events, 5M agent payments, your own MCP capacity.

```
Hey {{first_name}},

Scale is for when you've got 50+ agents leaning on the same memory and you
can't have them fighting for it. What £298.99/mo gives you:

- 50M events / month, 5M queries
- Hosted MCP: read + write + DEDICATED — your own MCP capacity, not a shared
  pool, so a busy fleet doesn't get noisy-neighbored
- 5M x402 calls included, $0.0001/call after
- 100M Hound extraction tokens
- Retention not limited by plan, unlimited streams
- Priority support + Slack

First thing worth doing: provision the dedicated MCP and point your busiest
agents at it. If you're past where Scale comfortably sits, reply and we'll talk
Enterprise — negotiated volume, SLA, dedicated SE.

→ {{dedicated_mcp_url}}

— Decebal

(Annual is £239.17/mo.)
```
- **Metric:** click → dedicated-MCP provisioning; reply rate for Enterprise interest.
- **Facts:** £298.99/£239.17 · 50M events / 5M queries · MCP read+write+dedicated · 5M x402 · 100M extraction tokens · retention not limited by plan · priority+Slack — `siteConfig.pricing[scale]` + `subscription.go TierScale` + `HOSTED_EXTRACTION_METERING` (scale 100M).

#### P-Ent — Enterprise welcome *(SE-routed, no auto-CTA)*
- **Segment:** **Enterprise** · **Trigger:** REAL entry (webhook → `enterprise`)
- **Subject:** Your AllSource Enterprise workspace is live
- **Preheader:** Your SE is {{se_name}} — they own the next step.

```
{{first_name}},

Your Enterprise workspace is provisioned: negotiated event volume, unlimited
retention, a dedicated MCP cluster, and 24/7 support with a named SE.

{{se_name}} ({{se_email}}) is your point of contact and will reach out to plan
onboarding. Nothing here is automated — for Enterprise, a person owns it.

— Decebal
```
- **Metric:** SE handoff completed (tracked in CRM, not comms opens).
- **Facts:** negotiated volume / unlimited retention / dedicated MCP cluster / 24-7 + dedicated SE — `siteConfig.pricing[enterprise]`.
- **Note:** marketing/lifecycle comms suppressed for Enterprise — high-touch only.

---

### Stage: Expansion

#### E1 — Events at 80% — HIGH-IMPACT (A/B), tier-specific ladder
- **Segment:** Indie/Studio/Scale (copy varies by tier) · **Trigger:** REAL — `billing.usage_warning.sent` @ 80%
- **Subject (A):** You're at 80% of your {{tier}} events this month
- **Subject (B):** {{events_used}} of {{events_quota}} events used — heads up
- **Preheader:** Not a wall yet. Here's the next ceiling and what it costs.

```
Hey {{first_name}},

Quick heads-up: you've used {{events_used}} of your {{events_quota}} events
this billing period — about 80%. Nothing's blocked, and I'd rather tell you
early than surprise you at the wall.

{{tier_upgrade_block}}

If this month was a spike, ignore me. If it's the new normal, the upgrade is
one click and prorated.

→ {{billing_url}}

— Decebal
```
- **`{{tier_upgrade_block}}` (tier-specific):**
  - **Indie →** `Studio (£78.99/mo) takes you from 500K to 5M events — 10x — plus MCP write.`
  - **Studio →** `Scale (£298.99/mo) takes you from 5M to 50M events and adds a dedicated MCP lane.`
  - **Scale →** `You can keep going on overage, or talk to us about Enterprise for negotiated volume and an SLA. Reply and we'll size it.`
- **Metric:** click → billing; upgrade conversion within 7d.
- **Facts:** quotas 500K/5M/50M and prices £78.99/£298.99 (`siteConfig.pricing`, `subscription.go`). **Replaces** the shipped retired-tier copy at `check_usage_warnings.go:239`.

#### E2 — Events at 100% (operational)
- **Segment:** Indie/Studio/Scale · **Trigger:** REAL — `billing.usage_warning.sent` @ 100%
- **Subject:** You've hit your {{tier}} event quota for this period
- **Preheader:** What happens next, and your two options.

```
Hey {{first_name}},

You've reached 100% of your {{events_quota}} events for this billing period.
I'm not going to dress this up — here's exactly what that means and your two
options:

1. Upgrade now (prorated) — {{tier_upgrade_block}} New capacity is live
   immediately.
2. Sit tight — your quota resets at {{period_reset_date}}. Your data is safe;
   retention is not limited by plan.

→ {{billing_url}}

If a single job blew through the quota and that's unexpected, reply — happy to
look.

— Decebal
```
- **Metric:** upgrade conversion within 48h; reply rate (surprise-overage signal).
- **Facts:** retention not limited by tier (`subscription.go` `TierQuotaMap`, `RetentionDays: -1` on every tier); same upgrade ladder as E1.
- **Note:** operational severity — exempt from the weekly marketing cap, still deduped per period.

#### E3 — x402 allowance at 80%
- **Segment:** paid tenants using x402 · **Trigger:** REAL (scheduled) — `x402_used ≥ 80%` of `X402Allowance`
- **Subject:** Your agents have used 80% of this month's x402 calls
- **Preheader:** This is the pay-per-call lever working. Here's the overage math.

```
Hey {{first_name}},

Your agents have spent about 80% of your included x402 calls
({{x402_used}} of {{x402_allowance}}). That's the pay-per-call model doing its
job — your agents are actually reading.

Two things to know:
- Overage is $0.0001/call. No cliff — calls keep working, you just pay per
  call past the allowance.
- If overage is becoming a line item, the next tier ships a bigger included
  allowance (Studio 500K, Scale 5M x402 calls) and usually works out cheaper.

Nothing to do unless you want to. Wanted you to see the number.

→ {{billing_url}}

— Decebal
```
- **Metric:** overage opt-in retained / upgrade within 14d.
- **Facts:** x402 allowances 50K/500K/5M + $0.0001/call overage (`siteConfig.pricing[*].x402`, `subscription.go X402Allowance`).

#### E4 — Hound extraction allowance at 80% *(gated on hosted extraction going live)*
- **Segment:** paid tenants using hosted Hound extraction · **Trigger:** REAL event (`prime.extraction.usage`) + scheduled threshold
- **Subject:** You're at 80% of your Hound extraction tokens
- **Preheader:** Two honest options: more tokens, or bring your own model.

```
Hey {{first_name}},

You've used about 80% of your hosted Hound extraction tokens this period
({{extraction_used}} of {{extraction_quota}}) — that's the LLM turning your
docs and code into graph nodes and edges.

When you hit 100%, hosted extraction is hard-gated (we return a 402) rather
than silently charging you. Two ways through:

1. Upgrade the tier for a bigger allowance — Studio includes 10M tokens, Scale
   100M.
2. Bring your own model — point PRIME_LLM_ENDPOINT at your own
   OpenAI-compatible endpoint and the cap never applies. You pay your provider
   directly; we never touch those tokens.

→ {{hound_docs_url}}

— Decebal
```
- **Metric:** upgrade OR BYO-LLM configuration within 14d; 402-block rate (should drop).
- **Facts:** per-tier tokens 1M/10M/100M + 402 hard-gate + BYO via `PRIME_LLM_ENDPOINT` (`HOSTED_EXTRACTION_METERING.md`).
- **BLOCKER:** do not ship until hosted extraction is live (`EXTRACTION_LLM_URL`/`_API_KEY` set) and the placeholder token amounts are confirmed (`HOSTED_EXTRACTION_METERING.md` "What's left").

#### E5 — Unused depth (feature-gap)
- **Segment:** paid, ingesting, ≥1 priced feature untouched · **Trigger:** NEEDS NEW SIGNAL (feature-gap cohort)
- **Subject (A):** You're paying for {{unused_feature}} and not using it
- **Subject (B):** One feature on your plan you haven't touched yet
- **Preheader:** No upsell — you already bought it. Here's how to use it.

```
Hey {{first_name}},

You're on {{tier}} and ingesting steadily — but {{unused_feature}} is sitting
unused, and you're already paying for it. This isn't an upsell; it's "use the
thing you bought."

{{unused_feature_block}}

If it's not useful for what you're building, tell me and I'll stop nudging —
genuinely.

→ {{feature_docs_url}}

— Decebal
```
- **`{{unused_feature_block}}` variants (pick the one matching the gap, one CTA each):**
  - **Recall unused** (`queries_used≈0`): `You've logged events but never queried them. The whole point is the 11.9µs p99 indexed read — here's a 2-line query to try.`
  - **Hound unused** (`extraction_tokens_used==0`): `Your plan includes {{extraction_quota}} Hound extraction tokens. Point Hound at one repo and get a queryable code-graph back.`
  - **x402 unused** (`x402_used==0`): `Your agents have never made an x402 call. If they pay per read, you only pay for memory they actually use.`
  - **SDK gap** (raw HTTP only): `You're hitting the API by hand — there are first-class SDKs for Rust, Go, Python, and TypeScript that do the auth and retries for you.`
- **Metric:** first-use of the named feature within 14d.
- **Facts:** 11.9µs p99 indexed read (`siteConfig.stats[1]`); extraction tokens by tier (`HOSTED_EXTRACTION_METERING`); 4 SDKs (`sdks/`: go, python-client, rust, typescript); x402 pay-per-read (`siteConfig`).

---

### Stage: Dormancy / re-engagement

#### D1 — Stream went quiet
- **Segment:** any active-then-silent · **Trigger:** REAL — `last_event_age` → fleet `at_risk`
- **Subject (A):** Your AllSource stream went quiet
- **Subject (B):** No events from {{tenant_name}} in {{days_silent}} days
- **Preheader:** Not a dunning email. Just checking the pipe didn't break.

```
Hey {{first_name}},

{{tenant_name}} hasn't sent an event in about {{days_silent}} days. Usually
that means one of two things: you finished what you were testing, or something
in the pipeline broke and you haven't noticed.

If it broke, the fastest check is to re-run that first append and confirm it
lands. If you wound it down on purpose, no worries at all.

→ {{dashboard_url}}

— Decebal
```
- **Metric:** re-activation — any new event within 7d.
- **Facts:** `last_event_age` / fleet At-Risk (`signals.go`); retention not limited by tier (`subscription.go`).

#### D2 — Still here?
- **Segment:** silent ~30d · **Trigger:** REAL — `last_event_age` → fleet `critical`
- **Subject:** Did AllSource not click for you?
- **Preheader:** Straight question — I read every reply.

```
Hey {{first_name}},

It's been about a month since {{tenant_name}} sent an event, so I'll just ask:
did AllSource not click, or did life get in the way?

I'm not going to send you a sequence pretending I didn't notice. If something
was confusing or missing, reply and tell me — it genuinely shapes what I build
next. If you want to step back to self-hosting (free, Apache-2.0 core, yours forever),
that's a fine outcome too and I'll point you at the export.

If you'd rather just talk it through: {{calendar_url}} — 15 minutes, me, no
sales pitch.

— Decebal
```
- **Metric:** reply rate / call booked; re-activation within 14d.
- **Facts:** self-host free (Apache-2.0 core) off-ramp (`siteConfig.pricing[self-host]`). No fake scarcity — honest by design.

#### D3 — Paid but dormant (pre-renewal, operational)
- **Segment:** paid + dormant + renewal ≤14d · **Trigger:** REAL — fleet tier + renewal date (+ dunning)
- **Subject:** You're about to renew {{tier}} — and you're not using it
- **Preheader:** I'd rather you get value or pause than auto-renew on nothing.

```
Hey {{first_name}},

Your {{tier}} plan ({{price}}/mo) renews on {{renewal_date}}, and I can see you
haven't sent an event in {{days_silent}} days. I don't want you paying for
capacity you're not touching — that's how people end up resenting a tool.

So, plainly:
- If you want back in, here's the 2-minute path to your first event again, and
  I'm on the other end of a reply if something's blocking you.
- If now's not the time, you can downgrade or move to self-host (free, Apache-2.0 core)
  before the renewal — no hard feelings, no retention dark-patterns.

→ {{billing_url}}

— Decebal
```
- **Metric:** re-activation OR explicit downgrade (both are wins over silent churn).
- **Facts:** tier price (`siteConfig.pricing`); self-host off-ramp; renewal date (subscription detail). Operational severity.

---

## 5. How this ships (control-plane proactive-comms mapping)

Every send maps onto the **Phase 6** comms surface in
`ADMIN_TENANT_POWER_TOOL.md §4 Pillar C` — built on the existing SMTP client
(`email_client.go smtpEmailClient.SendEmail`) + Core event-sourcing, **not** an
external ESP. (Reminder: this surface is designed, not shipped — only the quota
warning sends today.)

**Channels & endpoints**

| Use | Endpoint | Audit event | Notes |
|---|---|---|---|
| 1:1 lifecycle email (S1, A1–A3, P-*, E1–E5, D1–D3) | `POST /api/v1/admin/messages` `{tenant_id, template, dry_run?}` | `admin.message.sent` | Recipient = tenant metadata `email`; renders template from tenant + health/usage context. |
| In-app banner companion (P-*, E1/E2, D1) | `POST /api/v1/admin/notices` (cohort: `dry_run` → `{would:{recipient_tenant_ids,count}, confirm_token}`) | `admin.notice.created` | Tenant reads via `GET /api/v1/notices`; dismiss → `POST /api/v1/notices/:id/dismiss` (`admin.notice.dismissed`). |
| Internal note (e.g. D2 reply received) | `POST /api/v1/admin/tenants/:id/notes` | `admin.note.created` | Never sent to tenant; shows on the 360 timeline. |
| Admin review of sends | `GET /api/v1/admin/notices?tenant_id=` | — | "Who got what, when" is an event query. |

**Templates** (extend the four designed templates — `at_risk_outreach`,
`quota_warning`, `onboarding_nudge`, `dunning_reminder` — with these keys):

| Email | Template key | Reuse / new |
|---|---|---|
| S1 | `signup_welcome` | new |
| A1 | `onboarding_nudge` | **reuse (designed)** |
| A2 / A3 | `activation_recall` / `mcp_intro` | new |
| P-Indie/Studio/Scale | `paid_welcome_{indie,studio,scale}` | new (tier-keyed) |
| E1 / E2 | `quota_warning` (replaces shipped retired-tier copy) | **reuse (designed)** |
| E3 / E4 | `x402_allowance` / `hound_allowance` | new (mirror quota_warning) |
| E5 | `depth_nudge` | new |
| D1 / D2 | `at_risk_outreach` (severity by fleet tier) | **reuse (designed)** |
| D3 | `dunning_reminder` (pre-renewal variant) | **reuse (designed)** |

**Variables** the comms system fills from real reads: `{{first_name}}`,
`{{tenant_name}}`, `{{tier}}`, `{{price}}`, `{{events_used}}`,
`{{events_quota}}`, `{{x402_used}}`, `{{x402_allowance}}`,
`{{extraction_used}}`, `{{extraction_quota}}`,
`{{days_silent}}`, `{{renewal_date}}`, `{{period_reset_date}}`, `{{api_key}}` —
all sourced from `GET /api/v1/tenants/{id}/stats`, the tenant detail/subscription,
and `GET /api/v1/admin/fleet/health/:id`.

**Triggers → send.** Per-tenant emails (E1/E2 today; the rest in Phase 6) fire
from the scheduled jobs that already meter usage (`check_usage_warnings.go` and
the 5-min reconcilers) plus the new scheduled cohort/threshold jobs named in §3.
Cohort/in-app notices (E5 sweeps, D1 at-risk batches) go through
`POST /api/v1/admin/notices` with the **recovery blast-radius guard**:
`dry_run` previews recipients, an echoed `confirm_token`
(`recoveryGuard.mintConfirmToken`/`validateConfirmToken`) is required to apply,
and `max_recipients` is capped by `BatchDefaultMaxTenants = 25` /
`BatchAbsoluteCeiling = 100` (`recovery.go:33`).

**Guards applied at send** (server-enforced, §6 has the full rules): the CP
checks `comms_opt_out` (per-category) before sending and records
`skipped_opt_out` in the audit; per-tenant per-category cooldown is enforced by
reading the last `admin.message.sent` for that tenant+template; cohort sends are
blast-radius-capped as above.

---

## 6. Global suppression / guardrail rules

1. **Opt-out is law (per-category).** Honor `comms_opt_out`. Categories:
   **operational** (E2 100% quota, D3 pre-renewal, dunning, security) vs
   **lifecycle/marketing** (everything else). A marketing opt-out suppresses the
   lifecycle/marketing category; a hard global opt-out suppresses all. The CP
   checks the flag pre-send and writes `skipped_opt_out` to the audit — no silent
   drops.
2. **Weekly volume cap.** ≤ **1 lifecycle/marketing email per tenant per
   7 days**, enforced by reading `admin.message.sent` history. Operational sends
   don't count against the cap but are themselves deduped per threshold/period.
3. **Suppress on open ticket / recent dunning / recent operator touch.** If a
   tenant has an open support thread, an active dunning state
   (`GET /api/v1/admin/billing/dunning`), or a human operator message in the last
   72h, hold automated lifecycle sends. (Open-ticket suppression **needs a signal**
   — `email.received` exists but there's no first-class open-ticket flag; see §7.)
4. **No double-send across branches.** Dedup by `(tenant, template,
   billing_period)` — the same pattern `check_usage_warnings.go` already uses via
   `metadata.usage_warnings.LastWarningPct`. Branch precedence resolves overlaps:
   **Operational > Post-purchase > Expansion > Activation > Dormancy** — one
   lifecycle email wins per 7-day window. Dormancy and Expansion are mutually
   exclusive by construction (a tenant hitting quota is not dormant).
5. **Tier-correctness gate.** Every send re-reads the tenant's current tier from
   the subscription before rendering, so a retired-tier tenant (`pro`/`growth`)
   resolves through `MapRetiredTier` and never sees a wrong-tier "what you bought"
   email. This also forces E1/E2 off the shipped retired-tier copy.
6. **Self-host exclusion.** Self-host tenants have no CP email and are never
   targeted by this system (S2 is GitHub-side only).
7. **DRAFT discipline.** Nothing in this document is sent. A human reviews copy
   and schedules each template before any first send.

---

## 7. Open questions / assumptions

- **Free/trial entitlement.** I grounded the free/trial state on the backend
  `TierFree` quota (100K events/mo, 10K queries, retention -1 (no plan expiry), 1 stream, 0
  x402, 0 hosted extraction = BYO — `subscription.go:261`). `PRICING_EXPOSURE_PLAN.md`
  proposes a "14-day trial, 50K events" instead. **Confirm which is live** before
  S1/A1 quote any free-tier number — I deliberately avoided stating a free-tier
  quota in the copy to dodge this.
- **The whole comms send surface is Phase 6 (design, not shipped).** Only the
  quota warning sends today, and on retired-tier copy. The trail assumes Phase 6
  (`/api/v1/admin/{notices,messages}`, in-app notices, `comms_opt_out`,
  per-category cooldown) is built. Until then, E1/E2 are the only sendable emails
  and they need the `buildWarningEmail` copy swapped to the canonical tiers.
- **No `tenant.created` / tier-change / milestone events exist.** S1, A2, A3,
  P-*, E3, E4, E5 all need either a new Core event (emit `tenant.created` and
  `subscription.activated` from onboarding/the LemonSqueezy webhook) or a
  scheduled cohort/threshold job (the usage-warning pattern). All the underlying
  *data* is real (stats meters, `last_event_age`, the webhook) — only the
  trigger emit is missing. Flagged per-row in §3.
- **Hound extraction is not live in prod.** E4 is blocked until
  `EXTRACTION_LLM_URL`/`EXTRACTION_LLM_API_KEY` are set and the per-tier token
  amounts (1M/10M/100M) are confirmed — they're explicit placeholders
  (`HOSTED_EXTRACTION_METERING.md`). I used them in copy but gated E4 on go-live.
- **x402 unit = "calls".** `siteConfig` says "x402 calls" and `subscription.go`
  comments "x402 calls included per period"; I used "calls" throughout. Confirm
  that's the customer-facing unit (vs "payments" / "credits").
- **Open-ticket suppression (guardrail #3) needs a signal.** The `email.*`
  contract exists but there's no first-class "open support ticket" flag to
  suppress on. Either add one or approximate from recent inbound `email.received`.
- **A2's "~1K events" milestone is a chosen threshold,** not a product constant —
  set it wherever "enough history to be worth querying" lands for your data.
- **Annual prices** (£15.17 / £63.17 / £239.17 per month — `siteConfig.*.yearlyPrice`) appear in the
  paid-welcome copy; drop them if checkout already states billing cadence.
```
