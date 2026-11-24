# Pricing Plan Details

Source of record for per-tier entitlements. **Two layers must always agree:**
- **Displayed** — `apps/web/src/lib/config.ts` (`siteConfig.pricing` features) + live prices from LemonSqueezy via `/api/v1/billing/catalog`.
- **Enforced** — `apps/control-plane/internal/domain/entities/subscription.go` (`TierQuotaMap`: events/queries quota, x402 allowance, retention, max streams, MCP scope).

Change both together or they drift. Prices themselves are sourced live from LemonSqueezy (see `docs/runbooks/PRICING_BILLING_CUTOVER.md`).

## Current tiers

| | Self-Host | Indie £18.99 | Studio £78.99 | Scale £298.99 | Enterprise |
|---|---|---|---|---|---|
| **Events/mo** | unlimited (own hw) | 500K | 5M | 50M | negotiated (∞) |
| **Queries/mo** | — | 50K | 500K | 5M | ∞ |
| **x402 included** | — | 50K | 500K | 5M | ∞ |
| **x402 overage** | — | $0.0001/call | $0.0001/call | $0.0001/call | negotiated |
| **Retention** | forever | forever | forever | forever | forever |
| **Outbound data/mo** | — | 25 GB | 150 GB | 750 GB | custom finite |
| **Streams** | ∞ | 3 | ∞ | ∞ | ∞ |
| **MCP** | full (self-host) | read | read + write | read + write + dedicated | dedicated cluster |
| **Support** | GitHub community | email 48h | email 24h + Discord | priority + Slack | 24/7 + dedicated SE |

Backend also has a legacy `free` tier (100K events, forever retention, 1 stream, no hosted MCP). Per product direction **no hosted tenant should sit on `free`** — see the early-adopter migration below.

## Open questions (to revisit)

Retention is not a tier lever. `TierQuotaMap` sets `RetentionDays: -1` (forever) for every tier (`apps/control-plane/internal/domain/entities/subscription.go:265-281`), and the pricing FAQ states that data retention is not limited by plan tier (`apps/web/src/lib/config.ts:324`). No retention window per tier is pending.

Decisions still pending:
- **Headline differentiator** — events/mo, outbound data, or MCP scope: which leads the pitch?
- **x402** — keep flat `$0.0001/call` overage across tiers, or scale it per tier?
- **Streams** — is Indie's 3-stream cap right, or unlimited everywhere?

When these land, update `siteConfig.pricing` **and** `TierQuotaMap` in the same change, and (if a paid tier's numbers change) confirm the LemonSqueezy variant + this doc.

## Early-adopter migration (decided 2026-06-07)

Existing hosted tenants currently on `free`/Self-Host are migrated off it:
- **All hosted-free tenants → Studio**, with an **early-adopter voucher = 1 year free** (`SubscriptionMetadata.GrandfatherUntil = now + 365d`). After the year they convert to paid Studio.
- **Owner account → Enterprise** (comped, no expiry).
- Run mechanism: env-guarded one-shot on control-plane boot (`RUN_EARLY_ADOPTER_MIGRATION`), idempotent (skips tenants already on a paid tier).
