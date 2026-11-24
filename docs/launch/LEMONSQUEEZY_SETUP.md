# LemonSqueezy Setup Guide

## Control Plane Environment Variables

| Env Var | Description |
|---|---|
| `LEMON_SQUEEZY_API_KEY` | API key from LemonSqueezy dashboard |
| `LEMON_SQUEEZY_STORE_ID` | Store ID from LemonSqueezy dashboard |
| `LEMON_SQUEEZY_VARIANT_MAP` | JSON mapping `<tier>:<period>` keys to variant IDs, e.g. `{"indie:monthly":"<variant_id>","indie:annual":"<variant_id>"}` |
| `LEMON_SQUEEZY_WEBHOOK_SECRET` | HMAC signing secret from LemonSqueezy webhook config |

## LemonSqueezy Dashboard Setup

### 1. Product & Variants

Create one product (e.g. "AllSource Subscription") with the following variants. Prices and quotas come from `apps/web/src/lib/config.ts` (`siteConfig.pricing`) and `TierQuotaMap` in `apps/control-plane/internal/domain/entities/subscription.go`. Each paid tier needs a monthly and an annual variant. The Control Plane resolves checkout and webhook tiers through `LEMON_SQUEEZY_VARIANT_MAP`, and fails checkout for any tier whose variant is not mapped.

| Variant | Monthly | Yearly (per month) | Events quota | Notes |
|---|---|---|---|---|
| **Indie** | £18.99 | £15.17 billed yearly | 500,000/mo | 50K x402 calls included. 3 streams. |
| **Studio** | £78.99 | £63.17 billed yearly | 5,000,000/mo | 500K x402 calls included. Unlimited streams. |
| **Scale** | £298.99 | £239.17 billed yearly | 50,000,000/mo | 5M x402 calls included. Dedicated MCP. |
| **Enterprise** | Custom | Custom | Negotiated | Sales-led, no self-serve checkout; no LemonSqueezy variant required. |

- No "free" variant needed. Self-Host is not sold through LemonSqueezy.
- For each new variant in LemonSqueezy, note the numeric variant ID for the map in section 2.

### 2. Variant Map

Set `LEMON_SQUEEZY_VARIANT_MAP` as JSON on the Control Plane. Keys are `<tier>:<period>`, with period `monthly` or `annual`. The Control Plane strips the period suffix and maps each variant ID to its bare tier:

```json
{
  "indie:monthly": "<variant_id_indie_monthly>",
  "indie:annual": "<variant_id_indie_annual>",
  "studio:monthly": "<variant_id_studio_monthly>",
  "studio:annual": "<variant_id_studio_annual>",
  "scale:monthly": "<variant_id_scale_monthly>",
  "scale:annual": "<variant_id_scale_annual>"
}
```

Deploy with `fly secrets set LEMON_SQUEEZY_VARIANT_MAP='{...}' -a allsource-control-plane`.

### 3. Webhook

Configure a webhook in LemonSqueezy pointing to:

```
https://<control-plane-domain>/api/v1/webhooks/lemonsqueezy
```

Subscribe to these events:
- `subscription_created`
- `subscription_updated`
- `subscription_cancelled`
- `subscription_expired`
- `subscription_payment_failed`

Copy the HMAC signing secret into `LEMON_SQUEEZY_WEBHOOK_SECRET`.

## Data Flow

```
User clicks Upgrade → POST /api/v1/billing/checkout (tier, period)
  → Control Plane looks up variant ID from LEMON_SQUEEZY_VARIANT_MAP
  → Creates LemonSqueezy checkout session
  → Returns checkout_url → frontend redirects

User completes payment → LemonSqueezy fires webhook
  → POST /api/v1/webhooks/lemonsqueezy
  → Control Plane verifies HMAC signature
  → Extracts tenant_id from meta.custom_data (seeded during checkout)
  → Writes billing event to Core (event-sourced)

Query Service reads billing state from Core
  → GET /api/billing/status → derives current tier from latest billing events
```

See `docs/current/BILLING_ARCHITECTURE.md` for full C4 diagrams and sequence flows.

## Webhook Event Handling

| Event | Action |
|---|---|
| `subscription_created` | Writes `billing.subscription.created` to Core, sets tier |
| `subscription_updated` | Writes `billing.subscription.updated` to Core |
| `subscription_cancelled` | Writes event + suspends tenant |
| `subscription_expired` | Writes event + suspends tenant |
| `subscription_payment_failed` | Writes `billing.payment.failed`, sets status to `past_due` |

## Important

- **Only the Control Plane** talks to LemonSqueezy. No other service needs LemonSqueezy secrets.
- **Query Service** reads billing state from Core events — no direct LemonSqueezy integration.
- Billing events are durable in Core (WAL + Parquet) and provide a full audit trail.
