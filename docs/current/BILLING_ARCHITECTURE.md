# Billing Architecture

> **Status:** Current architecture as of 2026-03-26. Includes planned event-sourced migration.

## Overview

Billing in AllSource follows the platform's event-sourcing philosophy: subscription lifecycle events are written to Core as durable events, and downstream services derive current state from projections over those events.

**Billing provider:** LemonSqueezy.

---

## C4 Level 2: Billing Container Diagram

Shows how billing data flows between containers.

```mermaid
C4Container
    title Billing Architecture — Container Diagram

    Person(user, "User", "Subscribes, upgrades, manages billing")

    System_Boundary(allsource, "AllSource Platform") {
        Container(web, "Web App", "Next.js", "Dashboard with billing UI, plan cards, usage charts")
        Container(cp, "Control Plane", "Go/Gin", "Webhook handler, checkout sessions, billing management")
        Container(qs, "Query Service", "Elixir/Phoenix", "Serves billing state to frontend from Core projections")
        Container(core, "AllSource Core", "Rust", "Durable event store — billing events persisted via WAL + Parquet")
    }

    System_Ext(ls, "LemonSqueezy", "Payment provider — checkout, subscriptions, invoices")

    Rel(user, web, "Clicks Upgrade, views billing", "HTTPS")
    Rel(web, cp, "POST /api/v1/billing/checkout", "HTTP (internal)")
    Rel(web, qs, "GET /api/billing/status", "HTTP (internal)")
    Rel(cp, ls, "Creates checkout session", "HTTPS")
    Rel(ls, cp, "Webhook: subscription events", "HTTPS")
    Rel(cp, core, "Writes billing.* events", "HTTP")
    Rel(qs, core, "Queries billing events by tenant", "HTTP")

    UpdateLayoutConfig($c4ShapeInRow="3", $c4BoundaryInRow="1")
```

---

## C4 Level 3: Billing Data Flow (Sequence)

### Checkout Flow

```mermaid
sequenceDiagram
    participant U as User
    participant W as Web App
    participant CP as Control Plane
    participant LS as LemonSqueezy
    participant C as Core

    U->>W: Click "Upgrade to Studio"
    W->>CP: POST /api/v1/billing/checkout<br/>{tier: "studio", period: "monthly"}
    CP->>CP: Resolve variant ID from<br/>LEMON_SQUEEZY_VARIANT_MAP
    CP->>LS: Create checkout session<br/>(variant_id, custom_data: {tenant_id})
    LS-->>CP: checkout_url
    CP-->>W: {checkout_url}
    W->>U: Redirect to LemonSqueezy checkout
    U->>LS: Complete payment
    LS->>CP: POST /api/v1/webhooks/lemonsqueezy<br/>subscription_created
    CP->>CP: Verify HMAC signature
    CP->>CP: Extract tenant_id from meta.custom_data
    CP->>C: POST /api/v1/events<br/>{event_type: "billing.subscription.created",<br/>entity_id: tenant_id, payload: {...}}
    C-->>CP: 200 OK (event persisted)
    Note over C: Event durable in WAL + Parquet

    U->>W: Reload dashboard
    W->>QS: GET /api/billing/status
    QS->>C: GET /api/v1/events/query<br/>{entity_id: tenant_id, event_type: "billing.*"}
    C-->>QS: Latest billing events
    QS-->>W: {tier: "studio", status: "active", ...}
    W->>U: Shows Studio plan badge
```

### Webhook Processing Flow

```mermaid
sequenceDiagram
    participant LS as LemonSqueezy
    participant CP as Control Plane
    participant C as Core

    LS->>CP: POST /api/v1/webhooks/lemonsqueezy<br/>X-Signature: hmac-sha256

    alt Invalid Signature
        CP-->>LS: 400 Bad Request
    end

    CP->>CP: Parse event (subscription_created,<br/>subscription_updated, subscription_cancelled,<br/>subscription_expired, payment_failed)
    CP->>CP: Resolve tier from variant name/ID
    CP->>C: POST /api/v1/events<br/>{event_type: "billing.subscription.{action}",<br/>entity_id: tenant_id,<br/>payload: {tier, status, provider: "lemonsqueezy",<br/>subscription_id, customer_id, variant_id}}
    C-->>CP: 200 OK

    alt subscription_cancelled or subscription_expired
        CP->>C: POST /api/v1/events<br/>{event_type: "billing.tenant.suspended",<br/>entity_id: tenant_id}
    end

    CP-->>LS: 200 {status: "processed"}
```

---

## Service Responsibilities

### Control Plane (Writer)

The Control Plane is the **only service that talks to LemonSqueezy**. It:

- Receives webhook events and verifies HMAC signatures
- Creates checkout sessions via LemonSqueezy API
- Resolves tier from variant name/ID (configurable via `LEMON_SQUEEZY_VARIANT_MAP`)
- Writes billing events to Core with full payload
- Suspends tenants on cancellation/expiry
- Manages customer portal URLs
- Tracks overage and projected charges

**Env vars:**
- `LEMON_SQUEEZY_API_KEY` — API key
- `LEMON_SQUEEZY_STORE_ID` — Store ID
- `LEMON_SQUEEZY_VARIANT_MAP` — JSON mapping tier names to variant IDs
- `LEMON_SQUEEZY_WEBHOOK_SECRET` — HMAC verification secret

### AllSource Core (Storage)

Core stores billing events like any other event — durable via WAL + Parquet:

- `billing.subscription.created` — New subscription
- `billing.subscription.updated` — Plan change, status change
- `billing.subscription.cancelled` — User cancelled
- `billing.subscription.expired` — Subscription expired
- `billing.payment.succeeded` — Payment confirmation
- `billing.payment.failed` — Payment failure
- `billing.tenant.suspended` — Tenant suspended due to billing

Entity ID = tenant ID. Full audit trail preserved.

### Query Service (Reader)

The QS reads billing state from Core and serves it to the frontend:

- Queries Core for latest `billing.*` events per tenant
- Derives current subscription state (tier, status, quotas, expiry)
- Serves `GET /api/billing/status` to the frontend
- **No LemonSqueezy secrets** — reads from Core only

### Web App (UI)

The frontend calls:
- QS `GET /api/billing/status` — current plan, usage, quotas
- CP `POST /api/v1/billing/checkout` — initiate upgrade (proxied via Next.js)
- CP `GET /api/v1/billing/portal` — customer self-service portal

---

## Tier Configuration

Tiers are resolved at write time by the Control Plane. The mapping is configurable via `LEMON_SQUEEZY_VARIANT_MAP`:

```json
{"studio": "<variant_id>", "enterprise": "<variant_id>"}
```

Each tier has associated quotas (defined in the Control Plane):

| Tier | Events/mo | Queries/mo |
|------|-----------|------------|
| Free | 100,000 | 10,000 |
| Indie | 500,000 | 50,000 |
| Studio | 5,000,000 | 500,000 |
| Scale | 50,000,000 | 5,000,000 |
| Enterprise | Custom | Custom |

---

## Event Schema

Billing events follow the standard AllSource event format:

```json
{
  "event_type": "billing.subscription.created",
  "entity_id": "tenant-abc-123",
  "payload": {
    "subscription_id": "sub_12345",
    "customer_id": "cus_67890",
    "tier": "studio",
    "status": "active",
    "payment_provider": "lemonsqueezy",
    "variant_id": 12345,
    "variant_name": "Studio Monthly",
    "billing_period": "monthly",
    "amount_cents": 9900,
    "currency": "USD"
  }
}
```
