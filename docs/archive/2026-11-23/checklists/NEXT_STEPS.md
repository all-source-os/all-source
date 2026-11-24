# Next Steps Checklist

Status as of 2026-03-01. Updated to reflect current architecture (v0.10.7+).

> **Note (2026-03-01):** Section 2 was rewritten. The previous version proposed moving metadata to PostgreSQL, which contradicts the actual architecture where Core is the sole data store and QS is stateless. See ADR-005 for the PostgreSQL removal decision.

---

## 1. OAuth Login (GitHub & Google)

- [ ] Verify OAuth login works end-to-end with GitHub
- [ ] Verify OAuth login works end-to-end with Google
- [ ] Confirm JWT cookie is set after callback redirect
- [ ] Confirm `/api/auth/me` returns user after login
- [ ] Decide: implement email/password auth or remove the UI for it
  - Frontend has login/signup forms for email auth
  - Backend has **zero** email auth endpoints (register, login, verify-email, forgot-password, reset-password, resend-verification)
  - Either implement all 6 endpoints or remove the forms and go OAuth-only

**What's configured:**
- `.env`: `GITHUB_CLIENT_ID`, `GITHUB_CLIENT_SECRET`, `GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET`, `JWT_SECRET`, `FRONTEND_URL`
- `dev.exs`: OAuth config loaded from env vars
- `oauth_controller.ex`: Full flow implemented (redirect → provider → callback → JWT)

---

## 2. Architecture Cleanup (Query Service vs Control Plane)

### Current State (v0.10.0+)

Core is the sole data store. QS is stateless (no PostgreSQL). CP delegates to Core.

| Concern | Core | Control Plane | Query Service |
|---|---|---|---|
| Events | Source of truth (WAL+Parquet) | — | API gateway (proxies to Core) |
| Projections | Stores projection state | — | Server-side fold + API gateway |
| Schemas | Source of truth | — | Proxies to Core |
| Auth (login/register) | Auth manager + JWT | Proxies to Core + adds OAuth | Validates JWT (shared secret) |
| Tenant CRUD | Source of truth (event-sourced) | Proxies to Core + provisions on demo/start | Caches via ETS (5-min TTL) |
| Billing | — | LemonSqueezy integration | Usage enforcement plug |
| Audit logging | System streams | Proxies to Core | — |
| Config | System metadata | Proxies to Core | — |

### Remaining Cleanup Tasks

- [ ] Define clear API contract: which service handles which routes (document)
- [ ] Remove duplicate auth endpoints — Core should handle auth, CP should proxy
- [ ] Fix inconsistent endpoint naming (`/api/v1/users` vs `/api/v1/auth/users`)
- [ ] Add ETS cache invalidation webhook (CP → QS `/internal/tenant-updated`)
- [ ] Consolidate audit logging API across CP and Core

### Architecture

```
Clients → Query Service (data plane)     → Core (events, projections, schemas)
       → Control Plane (management plane) → Core (tenants, users, auth, billing, config, audit)
                                             ↓
                                        WAL + Parquet + DashMap (all data durable)
```

---

## 3. Vercel Deployment for Web Panel

### Pre-deployment

- [ ] Add `NEXT_PUBLIC_API_URL` to `apps/web/.env.example`
- [ ] Add web quality gates to CI (type-check, lint, build)
- [ ] Decide production Query Service URL for `NEXT_PUBLIC_API_URL`
- [ ] Configure CORS in Query Service for Vercel domain

### Deploy

- [ ] Connect GitHub repo to Vercel
- [ ] Set environment variables in Vercel dashboard:
  - `NEXT_PUBLIC_API_URL` — production Query Service URL
  - `NEXT_PUBLIC_APP_URL` — Vercel deployment URL
- [ ] Verify build succeeds (Next.js 16.1, Bun, standalone output)
- [ ] Verify OAuth callback works with Vercel URL (update provider redirect URIs)
- [ ] Set up preview deployments for PRs

### Optional

- [ ] Create `vercel.json` for explicit build config
- [ ] Configure custom domain
- [ ] Set up Vercel Analytics

**Current state:** Next.js 16.1 with `output: "standalone"`, Dockerfile exists, API client ready. No Vercel config yet.

---

## 4. Admin Panel & Subscription Management

### What Exists (~20%)

- [x] Billing page — plan selection, usage bars, upgrade flow
- [x] Team management — invite, roles, seat limits
- [x] Audit log — 25-day retention, 8 action types
- [x] API key management — create, rotate, revoke
- [x] Status page — service health checks (but uptime is hardcoded)
- [x] Control Plane billing endpoints — Stripe/LemonSqueezy checkout, portal, overage

### Tenant Administration (missing)

- [ ] Admin panel: list all tenants with search/filter
- [ ] View tenant details (plan, usage, quotas, members)
- [ ] Edit tenant quotas manually
- [ ] Force plan downgrade / suspend tenant
- [ ] Bulk tenant operations (disable, archive, export data)
- [ ] Tenant-specific usage breakdown (admin perspective)

### Real Monitoring (missing — status page fakes numbers)

- [ ] Replace hardcoded uptime % with actual metrics
- [ ] Event ingestion rate dashboard (real-time)
- [ ] Error rate tracking per endpoint
- [ ] Request volume / throughput graphs
- [ ] SLO dashboards
- [ ] Alert rules and thresholds
- [ ] Incident management interface

### Billing Administration (missing)

- [ ] Invoice history and download
- [ ] Payment reconciliation UI (provider vs local DB)
- [ ] Revenue reports (MRR/ARR tracking)
- [ ] Dispute / refund manual processing
- [ ] Dunning / collection workflow
- [ ] Churn analysis and retention metrics

### Security Administration (missing)

- [ ] IP allowlist / blocklist
- [ ] API token audit (which tenants used which keys, when)
- [ ] Suspicious activity detection
- [ ] Fine-grained RBAC policy editor
- [ ] SSO configuration UI

---

## Priority Order

1. **Test OAuth login** — code is ready, quick verification
2. **Deploy web to Vercel** — low effort, high visibility
3. **Architecture cleanup** — define CP vs QS roles, eliminate proxy overlap
4. **Admin panel** — tenant management + real monitoring + billing admin
