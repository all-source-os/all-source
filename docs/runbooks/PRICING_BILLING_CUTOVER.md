# Pricing & Billing Cutover Runbook (011)

Cutover from the legacy 4-tier model (`free` / `pro` / `growth` / `enterprise`,
LemonSqueezy) to the 5-tier 011 model from
[`docs/proposals/PRICING_EXPOSURE_PLAN.md`](../proposals/PRICING_EXPOSURE_PLAN.md) §2:

| Public (web) | Backend tier | Price | Events/mo | x402 incl. | Retention | Streams | MCP |
|---|---|---|---|---|---|---|---|
| Self-Host | `free` | $0 | 100K* | 0 | 7d* | 1* | none (self-host) |
| Indie | `indie` | $19 | 500K | 50K | 14d | 3 | read |
| Studio | `studio` | $79 | 5M | 500K | 90d | ∞ | read+write |
| Scale | `scale` | $299 | 50M | 5M | 365d | ∞ | read+write+dedicated |
| Enterprise | `enterprise` | Custom | ∞ | ∞ | ∞ | ∞ | dedicated |

\* `free` here is the hosted/grandfather fallback; true Self-Host runs on the
user's own hardware with no hosted quota.

Source of truth: `apps/control-plane/internal/domain/entities/subscription.go`
(`TierQuotaMap`, `QuotasForTier`, `MapRetiredTier`). Public tier copy lives in
`apps/web/src/lib/config.ts` (`siteConfig.pricing`, prompt 010).

## Live price catalog resilience (2026-09-14)

The table above records the original 011 plan, **not today's checkout price**.
Paid list prices come only from live Lemon Squeezy variants. `/api/v1/billing/catalog`
on the control plane refreshes them every five minutes. A complete six-price
snapshot (Indie/Studio/Scale × monthly/annual) persists in Core's existing
event-sourced config store under `billing:lemon_squeezy:catalog:v1`. No new Fly
volume or static paid-price fallback is used.

- A canceled web request cannot cancel the provider refresh or cache an empty
  result. Incomplete/failed refreshes leave the last complete snapshot intact.
- Provider-confirmed prices remain available for at most **14 days** after the
  last successful check, across control-plane restarts and deploys. A changed
  store/variant map invalidates that snapshot immediately. Price changes persist
  immediately; unchanged prices are re-saved at most daily.
- After one day without a successful check, pricing UI labels the verification
  date. Checkout always uses Lemon Squeezy's current price and shows the final
  amount before payment; the display cache never authorizes or calculates a charge.
- On a first deployment with no snapshot, public UI retries its empty catalog
  every five seconds while the detached provider fetch warms. It never guesses
  a paid price from `siteConfig`.

Check both public endpoints after deploy: `https://api.all-source.xyz/api/v1/billing/catalog`
and `https://www.all-source.xyz/api/billing/catalog`. Both should show six
positive provider prices, `fetched_at`, and `currency`. If the control plane is
empty, check `BillingConfigCheck` and Lemon Squeezy lookup errors in Fly logs;
if only web is empty, check the web-to-control-plane proxy and its two-second
deadline. Do not treat an HTTP 200 with `tiers:[]` as a healthy catalog.

> ⚠️ **Storage model:** subscription + entitlement state is persisted as **Core
> tenant-metadata JSON**, not Postgres columns. The new entitlement fields
> (`x402_allowance`, `retention_days`, `max_streams`, `mcp_scope`,
> `grandfather_until`) are additive JSON — **no SQL migration is required.**

---

## Existing-customer quota — RESOLVED (no-downgrade floor)

`MapRetiredTier` remaps legacy tiers to 011 successors for **price + x402 scope**:

```
developer/starter → indie    pro → indie    growth/team → studio
```

A naive remap would **lower** the events quota of existing paid customers
(`pro` 1M→500K, `growth` 10M→5M). To prevent that silent downgrade,
`QuotasForTier` applies a **no-downgrade floor** (`legacyEventsFloor` in
`subscription.go`):

- A retired PAID tier keeps its **pre-011 events/queries quota** — `pro` stays
  at 1,000,000 / 100,000; `growth`/`team` stay at 10,000,000 / 1,000,000.
- The **new 011 dimensions** (x402 allowance, retention, streams, MCP scope) are
  taken from the successor tier. These didn't exist pre-011, so applying them is
  additive, not a reduction (e.g. a `pro` tenant keeps 1M events and *gains*
  indie's 50K x402 allowance + `read` MCP scope).
- New signups can never land on a retired tier, so the floor only ever protects
  existing subscriptions.

So existing customers are never downgraded on the dimension they paid for.
`GrandfatherUntil` / `IsGrandfathered()` remain available for the §6 90-day free
cohort. The §6 launch-discount email is still the recommended nudge to migrate
pro/growth customers onto a canonical tier, but it is no longer required to avoid
harm.

## x402 allowance overshoot — MITIGATED

The allowance counter (`x402_used`) is reconciled from Core events by the
scheduler. Two mitigations bound how far a tenant can exceed its included
allowance between reconciliations:

- Reconciliation interval shortened **15 min → 1 min** (`scheduler.go`).
- `CoreQuotaChecker` keeps a **per-instance in-process counter** of allowance
  calls served free since the last Core read, so the gate denies further free
  calls the moment the allowance is locally spent — it does not wait for the
  reconciler. Residual multi-instance overshoot is bounded by
  (instances × calls-since-last-tick), not a full window of traffic. Core
  remains the source of truth (rebaselines whenever its counter changes).

---

## What 011 implemented (in code, uncommitted at time of writing)

- Canonical `indie`/`studio`/`scale` tiers + retired aliases with `MapRetiredTier`.
- Full entitlement set per tier (events, queries, x402 allowance, retention,
  streams, MCP scope) in `TierQuotaMap`; `QuotasForTier` auto-resolves retired ids.
- `GrandfatherUntil` + `IsGrandfathered()` on `SubscriptionMetadata`.
- x402 allowance metering, **event-sourced through Core**: `SyncX402UsageUseCase`
  reconciles `x402_used` from `x402.allowance.consumed` + `x402.payment.settled`
  events; `x402.AllowanceChecker` / `quota_gate.go` enforce the allowance bucket
  before falling through to the pay-per-call x402 middleware.
- LemonSqueezy is the sole payment provider; `BillingPeriod` recorded.
- Tests updated to the new contract: `go test ./internal/...` green.

> **Provider: LemonSqueezy (sole).** This deployment bills on LemonSqueezy
> (`payment_provider` is `lemonsqueezy`; `comp` for vouchers). The Stripe path
> was removed (t-12cb36) — LemonSqueezy is the only provider. Checkout/webhook
> canonicalize retired tiers (`MapRetiredTier`) and resolve a variant per billing
> period. (Git history has the Stripe code if it's ever needed again.)

## DONE — LemonSqueezy provisioning (2026-06-06, allsource-control-plane, TEST mode)

- [x] **Variants created** in store `282851` (All Source) — Indie/Studio/Scale ×
      monthly/annual. Published variant IDs:
      `indie:monthly 1755406`, `indie:annual 1755405`,
      `studio:monthly 1755391`, `studio:annual 1755367`,
      `scale:monthly 1755412`, `scale:annual 1755411`.
      (Each product also has a `Default` variant in `pending` status — unused cruft.)
- [x] **Fly secrets set + deployed** on `allsource-control-plane`:
      `LEMON_SQUEEZY_STORE_ID`, `LEMON_SQUEEZY_VARIANT_MAP` (`<tier>:<period>` keys),
      `LEMON_SQUEEZY_API_KEY` (test), `LEMON_SQUEEZY_WEBHOOK_SECRET`.
- [x] **Webhook registered** (id `108007`) →
      `POST https://allsource-control-plane.fly.dev/api/v1/webhooks/lemonsqueezy`,
      events: subscription_created/updated/cancelled/expired/payment_failed.
- [x] **Checkout proven** — LS API checkout for variant `1755406` returned a live
      URL accepting our `custom_data` (tenant_id/tier/billing_period).

## Still TODO — manual / human-gated steps

1. **End-to-end purchase verification** — **TEST MODE DONE (2026-06-28)**: a test
   checkout (`4242 4242 4242 4242`) via `/pricing` fired the webhook, signature
   verified, tenant metadata got the right tier + `billing_period`. **Still
   pending: re-run once on LIVE creds** after the TEST→LIVE swap (#3).
2. ~~**Set up the `sales@all-source.xyz` inbox.**~~ **RESOLVED 2026-09-16 by
   moving the address, not by adding a mailbox.** The Enterprise CTA now points
   at `sales@wolventech.com` in `apps/web/src/lib/config.ts` and
   `apps/web/src/app/dashboard/billing/page.tsx`.

   The 2026-06-28 "DONE" on this item was wrong, and the reason is worth keeping
   because it is invisible from inside the product. `all-source.xyz` has no MX
   record at all:

   ```
   $ dig ANY all-source.xyz @ns1.unstoppabledomains.com +noall +answer
   all-source.xyz.  3600  IN  A  66.241.125.155

   $ dig +short MX wolventech.com
   10 inbound-smtp.eu-west-1.amazonaws.com.
   ```

   One A record and nothing else. With no MX a sender falls back to the A record
   under RFC 5321, which is the Fly proxy and does not speak SMTP on port 25, so
   mail to any `@all-source.xyz` address bounced at the sending server. Every
   Enterprise lead between the pricing relaunch and 2026-09-16 was dropped.

   `siteConfig.links.email` — the contact rendered on `/privacy` and `/terms`
   and emitted as the Organization `email` in JSON-LD — was
   `hello@all-source.xyz` and is now `sales@wolventech.com` too.

   **Only `sales@` is known to exist on `wolventech.com`.** An MX record proves
   the domain accepts mail; it says nothing about which local parts are
   configured. Do not assume `hello@`, `support@` or `info@` resolve there
   without checking, and do not introduce one on that basis.

   Whoever puts an address on `all-source.xyz` in future has to add MX first. A
   `mailto:` on a domain with no mail records fails silently and only on the
   sender's side, so nothing in this repo or in Fly will ever report it.
3. **Swap TEST → LIVE LemonSqueezy.** **SUBSTANTIALLY DONE (2026-06-28):**
   - [x] Live `LEMON_SQUEEZY_API_KEY` set on `allsource-control-plane`.
   - [x] **Variant map UPDATED to live ids (2026-07-10).** ⚠️ Correction: LS test
         and live variants are **separate objects with different ids** here — the
         test ids (`1755xxx`) 404 under the live key. `config-check` passing was a
         false positive: it validates map *format/tier/period*, NOT that each id
         resolves at LS, so the stale map shipped and `/api/v1/billing/catalog`
         returned `tiers:[]` → every paid price rendered `—` and Indie checkout
         404'd. Live ids (from `task ls-variants`): indie m/a `1890953`/`1890952`,
         studio `1890950`/`1890949`, scale `1890948`/`1890945`. LEMON_SQUEEZY_VARIANT_MAP
         reset to these. **Lesson: always `task ls-variants` under the LIVE key and
         confirm `/billing/catalog` is non-empty before calling the swap done.**
   - [x] Live webhook registered + `LEMON_SQUEEZY_WEBHOOK_SECRET` set to match (both
         written from one value by `task ls-webhook-register`; `webhook_secret_len:32`,
         HMAC self-test green).
   - [x] **Store Test Mode OFF** — checkouts now charge real cards.
   - [x] **Test API key revoked** (2026-06-28) — the chat-shared key is dead; only
         the live key remains.
   - Cutover tooling: `Taskfile.yml` `ls-variant` / `ls-variants` /
     `ls-webhook-register` / `ls-config-check` (read `LEMON_SQUEEZY_API_KEY` from
     `.env`).
4. ~~**Add the `scale` backend binding.**~~ **DONE** — `config.ts` binds the Scale
   row to `tier: "scale"` (`apps/web/src/lib/config.ts:232`); no longer null.
5. **Retired-tier backfill.** One-shot over existing tenants applying
   `MapRetiredTier` so no live subscription points at a removed variant.
6. **Grandfather the free cohort** (§6): set `GrandfatherUntil = cutover + 90d` on
   current free/hosted tenants; send the $9-Indie launch-discount email (ops).

> ### 🛠 Resolve with — Control Plane fleet recovery (catalog: [`FLEET_HEALTH_RECOVERY.md`](./FLEET_HEALTH_RECOVERY.md))
> The **retired-tier backfill (item #5)** and any **dunning drift** that follows the
> cutover now have named recovery surfaces in the admin Control Plane (commits
> a02667e / e233bee / b6e3c88 — **DEPLOYED** on `allsource-control-plane` v87,
> 2026-06-27; these complement, don't replace, the steps above). All write audit
> events into Core; none touches Postgres.
>
> - **Retired-tier backfill across the cohort → `recovery_batch`.** `POST
>   /api/v1/admin/recovery/batch` (MCP: `recovery_batch`) with `action:
>   "reconcile-subscription"` and a `filter` scoped to the retired-tier cohort
>   (e.g. `pro` / `growth` / `team`) re-applies `QuotasForTier` — picking up the
>   §"no-downgrade floor" + the successor tier's new 011 dimensions — across many
>   tenants in one bounded pass. **Guard (server-enforced + rendered in the UI):
>   dry-run is mandatory and on by default; it returns the full affected-tenant
>   list + per-tenant preview + a `confirm_token`; to apply you must echo back the
>   token AND type the exact affected count.** Hard `max_tenants` cap (default 25,
>   ceiling 100). Destructive sub-actions (reprovision/restore/rotate) are
>   forbidden in batch.
> - **One tenant's quota/tier drift → `reconcile_subscription`.** `POST
>   /api/v1/admin/recovery/:id/reconcile-subscription` (MCP:
>   `recovery_reconcile_subscription`) — the single-tenant form of the same fix
>   (Guarded; dry-run shows the computed entitlements before apply). Use this when
>   one live subscription still points at a removed variant after the bulk pass.
> - **Past-due / dunning drift → `resolve_dunning`.** `POST
>   /api/v1/admin/recovery/:id/resolve-dunning` (MCP: `recovery_resolve_dunning`)
>   re-issues checkout / marks for manual review / extends grace for a
>   `past_due`/`unpaid`/`expired` tenant in the dunning list (Guarded; preview the
>   action taken before apply). LemonSqueezy remains the source of truth for
>   retry/lockout timing; this only nudges the tenant out of drift.
>
> **Always dry-run first and confirm the count** before any batch apply — an
> unbounded "reconcile everything" is the single most dangerous surface, which is
> exactly why the count-echo + `max_tenants` cap exist.

## Hosted extraction is a fixed add-on, never metered into money (#292)

Hound extraction LLM tokens are metered per tenant
(`quotas.extraction_tokens_used`, reconciled from `prime.extraction.usage` events
every 5 min) and the meter does two jobs only: it **gates** — the hard quota check
refuses extraction past `extraction_tokens_quota` — and it **displays**, on
`/dashboard/billing` beside events and queries.

**Extraction usage is never reported to LemonSqueezy.** A tenant that wants more
extraction buys a flat add-on that raises `extraction_tokens_quota`; the price is
the add-on, not the tokens. A metered reporter was built and reverted once the
model was settled, so if you find per-token reporting in the history, that is why
it is not in the tree.

Consequences worth knowing before changing it:

- Event overage reports unit counts to ONE LemonSqueezy subscription item, priced
  per unit there. Any future extraction charge on that item would be priced as
  events — extraction would need its own usage-based variant first.
- Raising a tenant's allowance is a metadata change (`extraction_tokens_quota`),
  not a billing change. The gate picks it up on the next request.

## Verification (test mode)

- [ ] Checkout for indie/studio/scale (monthly + annual) completes; webhook fires;
      tenant metadata shows correct tier + full entitlement set.
- [ ] Indie tenant capped at 500K events + 50K x402; overage path bills per call.
- [ ] A grandfathered free tenant still serves requests inside the window.
- [ ] No billing/x402 secret in any `NEXT_PUBLIC_` value or the web bundle.

## Rollback

- Entitlement/tier changes are JSON metadata — revert by re-applying the prior
  tenant metadata (or `git revert` the 011 commit and re-sync).
- No schema migration ⇒ no DDL to reverse.

## Scaling note — x402 allowance counter is per-instance (t-d7aabc)

The included-allowance gate (`CoreQuotaChecker`, `apps/control-plane/internal/infrastructure/x402/quota_gate.go`)
keeps an **in-process** counter of allowance calls served free between the
1-minute Core reconciler ticks. Core is the durable source of truth (every free
serve writes an `x402.allowance.consumed` event), but the fast-path tightening
counter is not shared.

- **Single instance (today):** EXACT at the allowance boundary. The Control Plane
  runs one Fly machine, so there is no overshoot.
- **Multiple instances:** the counter is per-instance, so residual overshoot at
  the boundary is bounded by `(instances - 1) × calls-served-since-the-last-tick`
  (≤ ~1 minute of the other instances' allowance traffic). Still far tighter than
  a full reconciliation window, but non-zero.

**If you scale the Control Plane out:**
1. Set `CONTROL_PLANE_MULTI_INSTANCE=true` on every instance. Boot then logs a
   loud `WARNING` so the bound is visible (it is otherwise silent).
2. To make the gate exact again, move the counter to a **shared store**. Prefer a
   **Core-side atomic counter** (a Core endpoint that atomically increments and
   returns `x402_used`) over Redis — reconciliation already flows through Core,
   so this avoids adding a second stateful dependency. Redis is the fallback if a
   Core change isn't feasible.

Until CP scales out, the accepted bound is **zero** and no action is required.
