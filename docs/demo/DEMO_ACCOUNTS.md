# Demo Accounts — Implementation & Operations Guide

How demo tenants work end-to-end, how to enable them in production, and how the daily reset cycle keeps data fresh.

---

## Architecture Overview

Demo accounts go through the **same registration and login flow** as real users. There is no special auth path — the only difference is that the Control Plane generates the credentials automatically and creates the tenant with `is_demo: true`.

```
  Login Page
  "Try Demo"
       │
       ▼
  POST /api/v1/demo/start (Control Plane)
       │
       ├── 1. Register credentials in Core (/api/v1/auth/register)
       │      (same path as normal RegisterHandler)
       │
       ├── 2. Create tenant in Core with is_demo=true, enterprise quotas
       │
       ├── 3. Seed sample data (7 business events + 1000 rich events)
       │
       └── 4. Return { email, password }
       │
       ▼
  Web App receives credentials
       │
       ▼
  POST /api/v1/auth/login (Control Plane)
       │   (normal email+password login — same as any user)
       │
       └── Returns { token }
       │
       ▼
  /api/auth/callback?token=...
       │
       ▼
  httpOnly cookie set ──> Redirect to /dashboard
```

Demo tenants differ from regular tenants in three ways:

1. **`is_demo: true`** flag on the Core tenant entity — propagated through all API responses
2. **Enterprise quotas** (`events_quota: -1`, `queries_quota: -1`) — unlimited usage
3. **Quota bypass** in Query Service — the `UsageEnforcement` plug skips quota checks entirely for demo tenants

---

## Service-by-Service Breakdown

### Core (Rust)

The `is_demo` field is a first-class boolean on the `Tenant` domain entity:

```
apps/core/src/domain/entities/tenant.rs    — bool field, defaults to false
apps/core/src/application/dto/tenant_dto.rs — included in all DTOs
apps/core/src/infrastructure/web/tenant_api.rs — accepted on create/update, returned in responses
```

Core also owns the rich seed endpoint:

```
POST /api/v1/demo/seed
```

Seeds 1000 events across 5 event types (log.info, log.warning, log.error, metric.cpu, metric.memory) with 384-dimension synthetic embeddings. **Idempotent** — checks for a `demo.seed_marker` event before creating anything.

### Control Plane (Go)

**`POST /api/v1/demo/start`** — the main entry point. No authentication required.

```
apps/control-plane/onboard.go — DemoStartHandler function
```

What it does:
1. Generates unique demo credentials: `demo-{uuid8}@demo.allsource.dev` + random password
2. Registers credentials in Core via `POST /api/v1/auth/register` (same as normal registration)
3. Creates tenant in Core with `is_demo: true`, enterprise quotas
4. Seeds 7 inline business events + calls Core's `/api/v1/demo/seed` for 1000 rich events
5. Returns `{ email, password, is_demo: true }` — **no token**

The client is responsible for logging in with the returned credentials through the normal `POST /api/v1/auth/login` flow.

The endpoint is excluded from auth middleware in `auth.go` (path prefix `/api/v1/demo/`).

### Query Service (Elixir/Phoenix)

**Quota bypass** — `UsageEnforcement` plug checks `demo_tenant?/1` before enforcing:

```
apps/query-service/lib/query_service_ex_web/plugs/usage_enforcement.ex
```

Demo tenants are identified by `"is_demo" => true` in the tenant map (string or atom key). When matched, the plug increments usage counters (for analytics) but does **not** halt the connection or return 402.

**Daily reset** — `DemoReset` GenServer:

```
apps/query-service/lib/query_service_ex/demo_reset.ex
```

Runs on a 15-minute check interval. Once per day at the configured hour:
1. Lists all tenants from Core, filters by `is_demo == true`
2. Deletes each demo tenant via `DELETE /api/v1/tenants/{id}`
3. Calls `POST /api/v1/demo/seed` to re-seed the shared demo dataset

### Web App (Next.js)

**Login page** — "Try Demo" button at bottom of the login card:

```
apps/web/src/app/(auth)/login/page.tsx
```

Flow:
1. Calls `POST /api/v1/demo/start` on the web app — gets back `{ email, password }`. The route proxies to `{AUTH_SERVICE_URL}/api/auth/demo/start` when `AUTH_SERVICE_URL` is set; otherwise it proxies to `{CONTROL_PLANE_INTERNAL_URL}/api/v1/demo/start`
2. Fills the email login form with the returned credentials
3. Calls `POST {CONTROL_PLANE_URL}/api/v1/auth/login` with those credentials (same as manual login)
4. Redirects through `/api/auth/callback?token=...` (same as any login)

This means the demo exercises the **exact same code paths** as a real user signing up and logging in.

**Demo banner** — persistent amber banner on all dashboard pages:

```
apps/web/src/components/dashboard/demo-banner.tsx
```

Only renders when `tenant.is_demo === true`. Not dismissible (unlike the early access banner). Includes a "Create a real account" link to `/signup`.

---

## Environment Variables

| Variable | Service | Default | Description |
|----------|---------|---------|-------------|
| `DEMO_RESET_ENABLED` | Query Service | `false` | Set to `true` to enable the daily reset GenServer |
| `DEMO_RESET_HOUR` | Query Service | `4` | UTC hour (0-23) when the daily reset runs |

`DEMO_ENABLED=true` is required on Control Plane. Core needs no demo env var.

---

## Production Deployment Checklist

### 1. Enable demo on Control Plane

Set `DEMO_ENABLED=true`. Without it, `POST /api/v1/demo/start` returns 403 `demo_disabled`. When enabled, the endpoint is unauthenticated. The auth middleware already skips `/api/v1/demo/` paths.

### 2. Enable daily reset on Query Service

Add to your Query Service environment:

```bash
DEMO_RESET_ENABLED=true
DEMO_RESET_HOUR=4          # 4 AM UTC, adjust as needed
```

Without `DEMO_RESET_ENABLED=true`, the `DemoReset` GenServer returns `:ignore` on init — it won't start or consume resources.

### 3. Verify Core seed endpoint

Core's `/api/v1/demo/seed` is always available. Verify it works:

```bash
curl -X POST http://core:3900/api/v1/demo/seed
# Expected: {"seeded": true, "event_count": 1000, ...}
```

### 4. Web app — no config needed

The "Try Demo" button uses `NEXT_PUBLIC_CONTROL_PLANE_URL` which should already be set for OAuth.

### 5. Docker Compose example

```yaml
allsource-query-service:
  environment:
    DEMO_RESET_ENABLED: "true"
    DEMO_RESET_HOUR: "4"
    # ... other existing env vars
```

---

## Security Considerations

### Authentication

Demo accounts are real accounts in Core's auth system. They go through the same password hashing, credential storage, and JWT issuance as any other user. The only difference is that credentials are auto-generated rather than user-chosen.

### Token lifetime

Demo accounts get the same 7-day JWT as regular email/password users (set by the LoginHandler). The tenant itself has `is_demo: true` which is what controls the demo behavior, not the token.

### Resource limits

Demo tenants have unlimited quotas (`-1`) so they aren't blocked by the `UsageEnforcement` plug. However, they are still subject to:

- **Rate limiting** — the `RateLimiting` plug applies per-tenant rate limits regardless of demo status
- **Normal API validation** — invalid payloads are rejected the same as for regular tenants

### Tenant isolation

Each demo click creates a **new tenant** with unique credentials. Demo tenants are fully isolated from each other and from real tenants via Core's standard tenant scoping.

### Daily cleanup

The `DemoReset` GenServer deletes stale demo tenants. Without it enabled, demo tenants accumulate indefinitely. In production, always set `DEMO_RESET_ENABLED=true`.

---

## Testing

### Unit tests

```bash
# Usage enforcement demo bypass (19 tests including 3 demo-specific)
cd apps/query-service
mix test test/query_service_ex_web/plugs/usage_enforcement_test.exs
```

### E2E tests against production

The primary tests drive the actual browser UI — click "Try Demo", watch the full register → login → dashboard flow happen. No internal service URLs needed.

```bash
cd tooling/e2e

# Against production (only needs the web app URL)
BASE_URL=https://app.all-source.xyz \
  bunx playwright test tests/smoke/auth-staging.spec.ts

# With direct CP access for API-level tests too
BASE_URL=https://app.all-source.xyz \
CONTROL_PLANE_URL=https://cp.all-source.xyz \
  bunx playwright test tests/smoke/auth-staging.spec.ts

# Demo zone tests (includes @demo tagged authenticated tests)
BASE_URL=https://app.all-source.xyz \
CONTROL_PLANE_URL=https://cp.all-source.xyz \
  bunx playwright test tests/smoke/demo-zone.spec.ts

# Run only @demo tagged tests
BASE_URL=https://app.all-source.xyz \
  bunx playwright test --grep @demo
```

When `BASE_URL` is set, the local Next.js dev server is **not** started. When omitted, playwright auto-starts it from `apps/web`.

### E2E tests locally

```bash
cd tooling/e2e

# Starts Next.js dev server automatically, uses localhost defaults
bunx playwright test tests/smoke/auth-staging.spec.ts
```

### Manual verification

```bash
# 1. Create demo credentials
curl -s -X POST http://localhost:3901/api/v1/demo/start | jq .
# Returns: { "email": "demo-abc12345@demo.allsource.dev", "password": "demo-abc12345-def67890", ... }

# 2. Log in with those credentials (normal login flow)
curl -s -X POST http://localhost:3901/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email":"demo-abc12345@demo.allsource.dev","password":"demo-abc12345-def67890"}' | jq .
# Returns: { "token": "eyJ...", "user": { ... } }

# 3. Use the token to query events
TOKEN="eyJ..."
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:3902/api/v1/events | jq .count

# 4. Force a demo reset (if GenServer is running)
# From an iex session:
QueryServiceEx.DemoReset.reset_now()
```

---

## Monitoring

Watch for these in production logs:

```
# GenServer started successfully
[DemoReset] Started. Reset hour: 4 UTC, check interval: 15m

# Daily reset executing
[DemoReset] Starting daily demo data reset
[DemoReset] Found 12 demo tenant(s) to reset
[DemoReset] Deleted tenant demo-a1b2c3d4
[DemoReset] Demo data re-seeded successfully

# If Core is down during reset
[DemoReset] Failed to list demo tenants: ...
```

The reset is best-effort — if Core is temporarily unavailable, it retries on the next 15-minute check. As long as the current UTC hour still matches the reset hour, it will keep trying.

---

## Flow Diagram: User Journey

```
User visits /login
       │
       ▼
  [Try Demo] button
       │
       ▼
  POST /api/v1/demo/start ──> { email, password }
       │
       │  (Control Plane registers real credentials in Core,
       │   creates demo tenant, seeds data)
       │
       ▼
  POST /api/v1/auth/login ──> { token }
       │
       │  (Normal login — same code path as any user)
       │
       ▼
  /api/auth/callback?token=...
       │
       ▼
  httpOnly cookie set ──> /dashboard
       │
       ▼
  Dashboard loads:
       ├── DemoBanner: "Demo Account — data resets daily"
       ├── Pre-seeded events visible in event browser
       ├── Analytics dashboards populated
       └── Full API access (rate-limited, no quota limits)
       │
       ▼
  (Next day at DEMO_RESET_HOUR)
       │
       ▼
  DemoReset GenServer:
       ├── DELETE all is_demo tenants from Core
       └── Re-seed shared demo data
```
