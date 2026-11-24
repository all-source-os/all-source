# Agent Authentication & Registration Design

## Status: Proposal
## Date: 2026-03-27

## Problem

AI agents need to programmatically create accounts and authenticate with AllSource. The current flow requires 4 steps across 3 services — too much friction for autonomous agents.

## What Exists Today

| Method | How | Good for Agents? |
|--------|-----|-------------------|
| **API Keys** | `POST /api/api-keys` → `X-API-Key` header | Yes, but requires an authenticated user first |
| **Demo flow** | `POST /api/auth/demo/start` → temp credentials | Quick start, but ephemeral |
| **Email signup** | Auth Service `POST /api/auth/sign-up/email` | Possible, but designed for humans |
| **Dev token** | `GET /api/auth/dev-token` (AUTH_DISABLED=true) | Dev only, not production |
| **Core bootstrap key** | `ALLSOURCE_BOOTSTRAP_API_KEY` env var | Operator-only, not self-service |

## The Gap

An AI agent today would need to:
1. Call Auth Service to create an account (email/password signup)
2. Get a JWT back
3. Have a tenant created via Control Plane
4. Use that JWT to create an API key
5. Use the API key going forward

That's 4 steps across 3 services.

## Options

### Option A: Agent Registration Endpoint (Recommended)

A single endpoint that does everything in one call:

```
POST /api/v1/agents/register
{
  "agent_name": "my-claude-code-agent",
  "agent_type": "mcp" | "sdk" | "cli",
  "callback_url": "https://..." (optional, for webhooks)
}

→ 201 Created
{
  "tenant_id": "tnt_...",
  "api_key": "ask_...",
  "tier": "free",
  "quotas": { "events": 10000, "queries": 1000 },
  "core_url": "https://core.all-source.xyz",
  "query_url": "https://query.all-source.xyz"
}
```

Lives in the **Control Plane** (tenant/account authority). Internally:
1. Creates a tenant with `agent_type` metadata
2. Creates an API key scoped to that tenant
3. Returns everything the agent needs to start working

No email, no password, no OAuth — just a name and you're in.

### Option B: Enhanced Demo Flow

Extend `POST /api/auth/demo/start` to return a **persistent** API key instead of ephemeral credentials. Less work but muddies the demo concept.

### Option C: MCP-Native Auth (OAuth 2.1 Device Flow)

RFC 8628 Device Authorization Grant:
1. Agent requests device code from Auth Service
2. User approves in browser (one-time)
3. Agent polls for token
4. Gets long-lived API key

How GitHub CLI, Docker CLI, etc. handle it. Best UX for agents with a human operator.

### Option D: Pre-provisioned Keys (Simplest)

Users create API keys in the dashboard, paste them into agent config. Already implemented.

## Recommendation

**Start with D (already done) + A (agent registration endpoint).** Add C (device flow) later when MCP auth spec stabilizes.

## Payments via x402 Protocol

### Background

The x402 protocol (by Coinbase) brings HTTP 402 "Payment Required" to life for machine-to-machine payments. We already have extensive x402 planning docs in `docs/x402/` from the hackathon project (Solana-focused). This section extends that work to cover **agent-to-API payments** for AllSource itself.

### How x402 Works

```
Agent                          AllSource API                    Facilitator
  │                                │                                │
  │─── GET /api/events ──────────►│                                │
  │                                │                                │
  │◄── 402 Payment Required ──────│                                │
  │    PAYMENT-REQUIRED header:    │                                │
  │    {amount, token, network,    │                                │
  │     recipient, facilitator}    │                                │
  │                                │                                │
  │    [Agent signs EIP-3009       │                                │
  │     authorization with its     │                                │
  │     private key — no gas]      │                                │
  │                                │                                │
  │─── GET /api/events ──────────►│                                │
  │    PAYMENT-SIGNATURE header    │───── /verify ────────────────►│
  │                                │◄──── {valid: true} ───────────│
  │                                │                                │
  │◄── 200 OK ────────────────────│───── /settle ────────────────►│
  │    PAYMENT-RESPONSE header     │◄──── {tx_hash} ──────────────│
  │    (contains tx receipt)       │                                │
  │                                │  [Event logged to Core:        │
  │                                │   x402.payment.verified]       │
```

### Key Properties

| Property | Value |
|----------|-------|
| Payment rail | EVM stablecoins (USDC on Base) or Solana SPL tokens |
| Gas fees | Paid by facilitator, not the agent |
| Agent requirement | Crypto wallet (private key + USDC balance) |
| Settlement | On-chain (seconds on Base L2) |
| Price volatility | None (stablecoins) |
| Protocol version | v2 (transport-agnostic: HTTP, MCP, A2A) |

### Pricing Model for AllSource API

```
Endpoint                          Price (USDC)
─────────────────────────────────────────────
POST /api/events                  $0.0001 per event
POST /api/events/batch            $0.00005 per event
GET  /api/events/query            $0.001 per query
POST /api/query                   $0.005 per complex query
GET  /api/analytics/*             $0.01 per analytics query
POST /api/projections             $0.01 per projection create
```

Free tier agents get 10K events/1K queries before x402 kicks in. Paid tier agents (via LemonSqueezy subscription) bypass x402 entirely — their quota is covered by subscription.

### Integration Architecture

x402 payment enforcement lives in the **Query Service** as an Elixir Plug, sitting between auth and the controller:

```
Request
  │
  ├─ CorrelationId plug
  ├─ AuthPipeline plug (API key or JWT)
  ├─ TenantContext plug
  ├─ UsageEnforcement plug ◄── checks quota
  │     │
  │     ├─ quota remaining? → pass through (no payment needed)
  │     └─ quota exceeded? → X402Payment plug
  │           │
  │           ├─ PAYMENT-SIGNATURE header present?
  │           │   ├─ yes → verify with facilitator → settle → pass through
  │           │   └─ no  → return 402 with PAYMENT-REQUIRED header
  │           │
  │           └─ Log x402.payment.* event to Core
  │
  └─ Controller
```

This means x402 is a **fallback** when quota is exhausted — not a replacement for subscriptions. Agents can:
1. Use free tier (10K events, no payment)
2. Subscribe via LemonSqueezy (unlimited within tier)
3. Pay-per-use via x402 (no subscription, pay as you go)

### Implementation Plan

#### Phase 1: Elixir x402 Plug (MVP)

No Elixir x402 SDK exists. Build a minimal plug:

```elixir
defmodule QueryServiceExWeb.Plugs.X402Payment do
  @moduledoc "x402 payment gate — returns 402 or verifies payment signature"

  import Plug.Conn

  def call(conn, opts) do
    case get_req_header(conn, "payment-signature") do
      [signature] ->
        # Decode base64 JSON, call facilitator /verify, then /settle
        verify_and_settle(conn, signature, opts)

      [] ->
        # Return 402 with payment requirements
        payment_required = build_payment_required(conn, opts)
        conn
        |> put_resp_header("payment-required", Base.encode64(Jason.encode!(payment_required)))
        |> send_resp(402, Jason.encode!(%{error: "payment_required", ...}))
        |> halt()
    end
  end
end
```

#### Phase 2: Agent Wallet Provisioning

For agents that don't have a wallet, offer automatic provisioning:

```
POST /api/v1/agents/register
{
  "agent_name": "my-agent",
  "provision_wallet": true    ◄── NEW
}

→ 201 Created
{
  "tenant_id": "tnt_...",
  "api_key": "ask_...",
  "wallet": {                 ◄── NEW (only if provision_wallet: true)
    "address": "0x...",
    "network": "eip155:8453",
    "funding_url": "https://..."
  }
}
```

Options for wallet provisioning:
- **Coinbase MPC Wallets** (recommended) — no private key exposure, programmatic signing
- **Locally generated keypair** — agent holds private key, signs locally
- **Custodial wallet** — AllSource holds keys, agent authorizes via API key

#### Phase 3: MCP Transport

x402 v2 supports MCP natively. Payment flows through `_meta["x402/payment"]` in JSON-RPC:

```json
{
  "jsonrpc": "2.0",
  "method": "tools/call",
  "params": {
    "name": "query_events",
    "arguments": {"stream": "orders"},
    "_meta": {
      "x402/payment": {
        "signature": "...",
        "payload": "..."
      }
    }
  }
}
```

This means AI agents using the AllSource MCP server can pay per tool call.

### Existing x402 Work

We have hackathon planning docs in `docs/x402/` covering:
- **SDK design** (`@allsource/x402-solana-sdk`) — Solana-focused, needs EVM extension
- **Demo app** — pay-per-call AI API
- **Analytics dashboard** — time-travel debugging for payments
- **SaaS strategy** — AllSource Paywall product

The hackathon focused on **Solana** payments. For agent payments, **Base (EVM) + USDC** is the better fit because:
1. Coinbase's facilitator service supports it natively
2. Lower fees than Solana mainnet for small transactions
3. USDC is the most liquid stablecoin on Base
4. Go and TypeScript SDKs already exist for EVM

### Alternatives Considered

| Protocol | Payment Rail | Pros | Cons |
|----------|-------------|------|------|
| **x402** (Coinbase) | EVM/Solana stablecoins | Gasless, stablecoins, AI-native | No Rust/Elixir SDK yet |
| **L402/LSAT** (Lightning Labs) | Bitcoin Lightning | Millisecond settlement, very low fees | BTC volatility, smaller ecosystem |
| **Stripe** | Traditional rails | Mature, well-known | High minimums, KYC friction, not agent-friendly |
| **Superfluid** | Streaming payments | Continuous billing | Complex, overkill for per-call |

### Event Sourcing Advantage

Every x402 payment becomes an event in Core:

```
x402.payment.requested  — agent hit quota, got 402
x402.payment.submitted  — agent sent signed authorization
x402.payment.verified   — facilitator confirmed signature
x402.payment.settled    — on-chain tx confirmed
x402.payment.failed     — verification or settlement failed
```

This enables:
- **Time-travel billing** — reconstruct exact payment state at any timestamp
- **Dispute resolution** — prove what was delivered and when
- **Usage analytics** — per-agent, per-endpoint payment breakdowns
- **Fraud detection** — pattern analysis on payment events

### Open Questions

1. **Facilitator**: Self-host or use Coinbase's hosted facilitator at `x402.org`?
2. **Minimum payment**: What's the floor before x402 overhead makes it uneconomical?
3. **Wallet UX**: How do we make wallet funding seamless for agents?
4. **Subscription vs x402**: Should paid-tier agents also have x402 as overflow?
5. **MCP auth spec**: Will the MCP protocol standardize payment/auth? Wait or lead?
