# Proactive-Comms Efficiency Engine (prompt 050)

**Status:** shipped (control-plane + apps/admin). Read with `CLAUDE.md` (isolation)
and `docs/marketing/lifecycle-email-trail.md` (the first campaign measured).

## What it is

Instrumentation + attribution that answers *"did the messages we send drive real
downstream behavior — not vanity opens, but real product events?"* and surfaces it
to operators.

**It dogfoods Core.** Efficiency is a **temporal join over the tenant's own Core
event stream** — email engagement events ⋈ goal events — computed by a control-plane
reconciler. This subsystem uses no parallel analytics store: the product's event
store + temporal queries are its authority. PostHog separately measures public
acquisition and product UX; it never replaces comms outcomes in Core.

It is **generalized**: it measures any proactive-comms send, with the lifecycle
email trail as the first consumer.

## Why control-plane, not Core or QS

This is an **operator-level, cross-tenant analytic** ("is campaign X working across
the fleet?"), so it lives in the **control-plane** — the same place the billing
reconcilers aggregate across tenants. Per `CLAUDE.md`, per-*tenant* user-facing
read-model compute belongs in the Query Service, and per-tenant projection compute
must not go in Core's engine. This engine is neither: it is a cross-tenant operator
read-model, computed in CP, reading Core events.

## Event schema (durable Core events)

All comms-efficiency events live under the **`admin-comms` operator tenant**
(`CommsAuditTenant`), co-located with the existing `admin.message.sent` send event,
each carrying the **customer `tenant_id` as a payload tag**. Customer streams stay
uncluttered by marketing telemetry; goal events (below) live in the customer streams.

| Event | Source | Meaning |
|---|---|---|
| `admin.message.sent` | comms send path | a send (extended with the tags below) |
| `comms.holdout` | holdout splitter | a deterministically suppressed would-send |
| `email.delivered` | Resend webhook | ESP confirmed delivery |
| `email.opened` | Resend webhook | open (UNRELIABLE — Apple MPP inflates) |
| `email.clicked` | Resend webhook | tracked-link click (LEAD signal) |
| `email.bounced` | Resend webhook | bounce |
| `email.complained` | Resend webhook | spam complaint |
| `email.unsubscribed` | list-unsub | unsubscribe (ESP-agnostic slot) |

**Shared correlation tags** on every send + holdout + engagement event (without them
there is no join): `tenant_id`, `campaign_id`, `message_id` (ESP id), `trail_stage`,
`variant`, `cohort`, `tier`, `send_ts`, `holdout`(bool). Defined in
`apps/control-plane/internal/application/usecases/comms_efficiency_schema.go`.

### Engagement ingress (idempotent)

`POST /api/v1/webhooks/resend/events` (Svix-verified) → `CommsUseCase.RecordEngagement`:
maps the Resend type → an engagement event, resolves `message_id` → the send's tags
(via a Core-config correlation record written at send time), and ingests under
entity `comms:engage:<message_id>:<type>` with `expected_version: 0`. A replayed
webhook hits a version conflict and is dropped — **exactly one event per
(message_id, type)**, and repeat opens/clicks collapse to the per-recipient funnel
binary. Extends the in-flight inbound handler additively (new method + optional
dependency; the inbound `email.received` path and constructor are untouched).

## Holdout splitter (causal lift)

At send time, a configurable `holdout_pct` of each campaign's cohort is suppressed
and recorded as `comms.holdout` (same tags) so `conversion(sent)` vs
`conversion(holdout)` — true lift — is measurable. Rules (enforced in
`CommsUseCase.SendMessage`):

- **Deterministic**: `sha256(tenant_id|campaign_id) % 100 < pct` — the same
  (tenant, campaign) always lands the same side, reproducibly across reconciler runs.
- **Marketing only**: operational/critical templates (quota, dunning, security) are
  **never** held out — you must never suppress a service-critical message for a test.
- **Opt-out wins**: an opted-out tenant is suppressed as `skipped_opt_out`, not held
  out.

## Attribution reconciler

`CommsEfficiencyUseCase` (`comms_efficiency.go`) mirrors the billing `sync_*`
reconcilers: scheduled (`comms_efficiency`, 15 min, in `scheduler.go`), wired in
`container.go`, paged Core queries, write-back of an operator-side projection
(Core config `comms:efficiency:projection`). The API reads that projection (with a
live-compute fallback so the panel is never empty).

Per **campaign / stage / variant / tier** over the goal's window N it computes:
delivered, open-rate, click-rate, **conversion = goal-in-window / delivered**,
time-to-goal (median), unsub-rate, complaint-rate, and
**lift = conversion(sent) − conversion(holdout)**.

### Attribution window N

Per-stage, seeded from the trail's trigger table (`lifecycle-email-trail.md §3`):
signup 3d, activation-first-event 2d, activation-recall 7d, mcp-intro 14d,
**paid-welcome 14d**, expansion-quota 7d, expansion-x402 14d, depth 14d, dormancy
7d, dormancy-paid 14d. Default 7d for an unmapped stage. Encoded in `DefaultGoalMap`.

### Edge cases (deliberate, documented)

The reconciler resolves each send/holdout via **last-touch within the window**, per
`(tenant, goal_event)`. For each touch (ascending by `send_ts`) the credit region is
`[send_ts, min(next_touch_ts, send_ts+N))` (the final touch keeps the full window);
the earliest goal in that region credits the touch.

- **Goal before the send** → not credited (region starts at `send_ts`).
- **Multiple emails before one goal** → **last-touch**: the most recent send whose
  window covers the goal gets the credit; one goal credits at most one send.
- **Goal outside the window** → miss (region capped at `send_ts+N`).
- **Churned/deleted tenant** → counted in the delivery funnel but **excluded from
  conversion/lift** (its goal stream is unreadable, so conversion is a floor); the
  reconcile degrades, never errors, and surfaces a `churned` count.

### Lift denominators (why two conversion numbers)

- `conversion_rate = converted / delivered` — the delivery-funnel headline.
- `lift = convert_sent − convert_holdout`, both on the **intent-to-treat** basis
  (`goal / cohort size`), because the holdout is the counterfactual for the **send
  decision**, not delivery. Lift is the only **causal** number on the panel.

## Goal-event map — real today vs needs-new-signal

Seeded from the trail's trigger→signal table. Surfaced honestly in the panel's goal
legend.

| Stage | Goal Core event | State |
|---|---|---|
| signup, activation-first-event, dormancy, dormancy-paid | any product event (reactivation/activation) | **REAL** |
| **paid_welcome** | **`subscription.activated`** (trial→paid — the HERO) | **REAL** |
| expansion_quota | `subscription.upgraded` | **REAL** |
| expansion_x402 | `x402.payment.settled` | **REAL** |
| activation_recall (A2) | first recall/query | NEEDS SIGNAL (queries are reads, not events) |
| mcp_intro (A3) | first MCP read | NEEDS SIGNAL |
| depth (E5) | first feature-use | NEEDS SIGNAL |

**`subscription.activated` / `subscription.upgraded` are now emitted today** by the
subscription apply path (`update_subscription_metadata.go applyLocked`) on a tier
transition into / up within paid — making the trial→paid hero metric real. This only
**records** a state change that already happened; it changes no entitlement and moves
no money. Idempotent webhooks/renewals at the same tier emit nothing.

## The panel (`apps/admin` → Efficiency)

`GET /api/v1/admin/comms/efficiency`. The page leads with **trial→paid conversion as
the hero** and the causal lift vs holdout; the funnel table leads with **clicks +
conversion + lift**; **opens are visually subordinated (muted) with a one-line MPP
caveat** so an operator never optimizes on a corrupted signal. **There is no free
segment** — the free tier is retired; tiers are trial / Indie / Studio / Scale /
Enterprise, read from the send tags.

## Files

- Schema/config/holdout: `internal/application/usecases/comms_efficiency_schema.go`
- Send-path tags + holdout + correlation: `comms.go`, `comms_audit.go`
- Engagement ingress: `comms_engagement.go` + `interfaces/http/resend_webhook_handler.go` (`Events`)
- Reconciler: `comms_efficiency.go`; scheduler `scheduler.go`; wiring `container.go`
- Hero signal emit: `update_subscription_metadata.go`; `entities/subscription.go` (`TierRank`)
- Endpoint: `interfaces/http/comms_efficiency_handler.go`; route in `main.go`
- Admin UI: `apps/admin/src/lib/comms-efficiency-api.ts`,
  `apps/admin/src/app/(authenticated)/comms-efficiency/page.tsx`, sidebar nav
- Tests: `comms_efficiency_test.go`, `comms_holdout_test.go`,
  `resend_webhook_handler_test.go` (engagement)
