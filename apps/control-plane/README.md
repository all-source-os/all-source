---
title: "Go Control Plane"
status: CURRENT
last_updated: 2026-11-24
category: service
port: 3901
technology: Go
---

# AllSource Control Plane v0.30.0

> Auth, RBAC, policy, billing and operations service in front of the AllSource Core event store.

[![Go](https://img.shields.io/badge/go-1.26.11-blue.svg)](https://golang.org/)
[![Status](https://img.shields.io/badge/status-v0.30.0-green.svg)]()
[![Framework](https://img.shields.io/badge/framework-gin-orange.svg)](https://gin-gonic.com/)
[![Auth](https://img.shields.io/badge/auth-JWT-red.svg)]()
[![Tracing](https://img.shields.io/badge/tracing-OpenTelemetry-purple.svg)]()

## Overview

The control plane sits between operators and Core. It issues and validates JWTs, enforces RBAC and policies, writes an audit log, exports OpenTelemetry traces, and proxies authenticated requests to Core.

- **JWT authentication** - HS256 tokens signed with `JWT_SECRET`, which must match Core and the Query Service
- **Role-based access control** - 4 roles, 7 permissions (see [Permission System](#permission-system))
- **Audit logging** - JSON lines appended to `AUDIT_LOG_PATH`
- **OpenTelemetry tracing** - OTLP HTTP export, enabled by `OTEL_ENDPOINT`
- **Policy enforcement** - 5 default policies plus policies created through `/api/v1/policies`
- **Permission-based routes** - each route declares the permission it requires
- **Authenticated proxying** - forwards events, queries and read models to Core

---

## 🏗️ Architecture

```
┌─────────────────────────────────────────────────────┐
│            Operators / Dashboards                   │
│         (Authenticated via JWT/API Key)             │
└─────────────────────────────────────────────────────┘
                         │ HTTPS
                         ▼
┌─────────────────────────────────────────────────────┐
│       Go Control Plane v0.30.0 (Port 3901)          │
│  ┌───────────────────────────────────────────────┐  │
│  │  🔐 JWT Authentication Middleware             │  │
│  │  🛡️  RBAC & Permission Enforcement            │  │
│  │  📋 Audit Logging (All Operations)            │  │
│  │  📊 OpenTelemetry Distributed Tracing         │  │
│  │  📜 Policy Engine (5 Default Policies)        │  │
│  │  🎯 Authenticated Proxy to Core               │  │
│  └───────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────┘
                         │ HTTP + Auth Token
                         ▼
┌─────────────────────────────────────────────────────┐
│      Rust Event Store Core v0.30.0 (Port 3900)     │
│  • Event Ingestion (469K events/sec)                │
│  • Multi-Tenancy with Quotas                        │
│  • Rate Limiting (Token Bucket)                     │
│  • Authentication & RBAC                            │
│  • Backup & Restore                                 │
└─────────────────────────────────────────────────────┘
```

---

## ✨ Features

### 🔐 Authentication & Authorization
- **JWT Token Validation** - Validates tokens issued by Rust core
- **API Key Support** - Accepts API keys for service accounts
- **Auth Middleware** - Automatic token extraction and validation
- **Permission Checking** - Fine-grained permission enforcement
- **Role-Based Access** - 4 roles with distinct capabilities

### 📋 Audit Logging
- **Complete Audit Trail** - Logs all API requests and operations
- **Structured Logging** - JSON format for easy parsing
- **Event Types** - API requests, auth events, tenant events, operations
- **Rich Metadata** - User ID, tenant ID, IP address, user agent, duration
- **File-Based Storage** - Appends to the file at `AUDIT_LOG_PATH` (default `audit.log`)

### 📊 Distributed Tracing
- **OpenTelemetry SDK** - Industry-standard tracing
- **OTLP Exporter** - Export traces via OTLP HTTP (works with Jaeger, Tempo, etc.)
- **Span Propagation** - Distributed context across services
- **Rich Attributes** - HTTP method, route, status, user info
- **Error Tracking** - Automatic error recording in spans
- **Custom Events** - Add custom events for important actions

### 📜 Policy Enforcement
- **Policy Engine** - Evaluate policies against requests
- **Condition-Based Rules** - Support for eq, ne, gt, lt, contains, in
- **Priority System** - Higher priority policies evaluated first
- **Actions** - Allow, Deny, Warn
- **Default Policies** - 5 security policies out-of-the-box
- **Custom Policies** - Add your own policies dynamically

### Permission System

#### Roles
- **Admin** - Full access to all features
- **Developer** - Read, write, manage schemas & pipelines
- **ReadOnly** - Read-only access
- **ServiceAccount** - Read and write (no admin)

#### Permissions
- **Read** - View resources
- **Write** - Create and update resources
- **Admin** - Administrative operations
- **Metrics** - View metrics and monitoring data
- **ManageSchemas** - Manage event schemas
- **ManagePipelines** - Manage data pipelines
- **ManageTenants** - Manage tenant configuration

---

## 🔧 API Endpoints

Routes are listed from `main.go`. Every `/api/v1` route not listed under "Public" requires a Bearer token, and the permission column is the one the route enforces.

### Public (no auth)
```
GET  /livez                          Liveness (used by the Fly machine check)
GET  /health                         Health check
GET  /metrics                        Prometheus metrics (public listener)
GET  /docs, GET /openapi             API documentation and OpenAPI spec
GET  /readyz/consumers               Consumer access probe
GET  /api/v1/status/services         Service status
GET  /api/v1/billing/catalog         Billing catalog
GET  /api/v1/cluster/health          Cluster health
GET  /api/v1/teams/invite/:token     Invite lookup
```

Public handlers that accept a POST without a session: `/api/v1/auth/login`, `/api/v1/auth/register`, `/api/v1/auth/oauth/:provider` (GET), `/api/v1/onboard/start`, `/api/v1/design-partners/applications`, `/api/v1/changelog/subscriptions`, `/api/v1/changelog/unsubscribe`, `/api/v1/agents/anonymous-trial`, `/api/v1/demo/start`, `/x402/*`, `/api/v1/webhooks/*` (verified by provider signature), and `/api/v1/admin/*` (see the admin section; admin routes apply their own operator-session check).

### Authentication
```
POST /api/v1/auth/register           Create account (name, email, password)
POST /api/v1/auth/login              Log in (email or username, password)
GET  /api/v1/auth/session            Current user and tenant (requires Bearer)
GET  /api/v1/auth/oauth/:provider    Start OAuth login (github, google)
GET  /api/v1/auth/oauth/:provider/callback
```

### Events, reads and projections (proxied to Core)
```
POST /api/v1/events                  Ingest one event (permission: Write)
POST /api/v1/events/batch            Ingest a batch (Write)
GET  /api/v1/events/query            Query events (Read)
GET  /api/v1/event-feed              Tenant event feed (Read)
GET  /api/v1/read-models/:name       Read a read model (Read)
PUT  /api/v1/read-models/:name       Write a read model (Write)
GET  /api/v1/stats                   Stats (Read)
GET  /api/v1/read-costs              Read cost attribution (Read)
GET  /api/v1/streams                 Streams (Read)
GET  /api/v1/entities/:entity_id/state     Entity state (Read)
GET  /api/v1/entities/:entity_id/snapshot  Entity snapshot (Read)
ANY  /api/v1/prime/*path             Prime proxy
POST /api/v1/extraction/chat/completions   Extraction proxy (Write)
```

### Cluster and metrics
```
GET  /api/v1/cluster/status          Cluster status (Read)
GET  /api/v1/metrics/json            Aggregated metrics (authenticated)
GET  /api/v1/health/core             Core health (authenticated)
```

### Operations
```
POST /api/v1/operations/snapshots          Create snapshot (Admin)
GET  /api/v1/operations/snapshots          List snapshots (Read)
POST /api/v1/operations/compaction         Trigger compaction (Admin)
GET  /api/v1/operations/compaction/stats   Compaction stats (Read)
POST /api/v1/operations/replay             Start replay (Admin)
GET  /api/v1/operations/replay/:id         Replay progress (Read)
POST /api/v1/operations/replay/:id/cancel  Cancel replay (Admin)
GET  /api/v1/operations/history            Operation history (Read)
POST /api/v1/operations/backup             Create backup (Admin)
```

### Tenants
```
GET    /api/v1/tenants                 List tenants (Read)
GET    /api/v1/tenants/me/limits       Limits for the caller's tenant (Read)
GET    /api/v1/tenants/:id             Tenant details (Read)
POST   /api/v1/tenants                 Create tenant (ManageTenants; policy requires Admin)
PUT    /api/v1/tenants/:id             Update tenant (ManageTenants)
POST   /api/v1/tenants/:id/suspend     Suspend (Admin)
POST   /api/v1/tenants/:id/activate    Activate (Admin)
DELETE /api/v1/tenants/:id             Delete (Admin)
GET    /api/v1/tenants/:id/stats       Tenant stats (Read)
```

### Teams
```
GET    /api/v1/teams/members           List members (Read)
POST   /api/v1/teams/invite            Invite a member
POST   /api/v1/teams/join              Join with an invite
PUT    /api/v1/teams/members/:id       Change a member's role
DELETE /api/v1/teams/members/:id       Remove a member
POST   /api/v1/teams/agent-keys        Create agent key (Write)
GET    /api/v1/teams/agent-keys        List agent keys (Read)
DELETE /api/v1/teams/agent-keys/:name  Revoke agent key (Write)
```

### Users
```
GET    /api/v1/users                   List users (proxied to Core `/api/v1/auth/users`)
DELETE /api/v1/users/:id               Delete user (self-deletion is refused by policy 3)
```

### Policies
```
GET    /api/v1/policies                List (Read)
GET    /api/v1/policies/:id            Get (Read)
POST   /api/v1/policies/evaluate       Evaluate a request against policies (Read)
POST   /api/v1/policies                Create (Admin)
PUT    /api/v1/policies/:id            Update (Admin)
DELETE /api/v1/policies/:id            Delete (Admin)
POST   /api/v1/policies/:id/enable     Enable (Admin)
POST   /api/v1/policies/:id/disable    Disable (Admin)
```

### Schemas, config, notices, audit
```
GET  /api/v1/schemas                   List (Read)
POST /api/v1/schemas                   Register (ManageSchemas)
POST /api/v1/schemas/validate          Validate (Read)
GET  /api/v1/config                    List config (Read)
GET  /api/v1/config/:key               Get config (Read)
POST /api/v1/config, PUT/DELETE /api/v1/config/:key   Admin
GET  /api/v1/notices                   Tenant notices (Read)
POST /api/v1/notices/:id/dismiss       Dismiss notice (Read)
GET  /api/v1/audit                     Audit query (Read)
```

### Billing
```
GET  /api/v1/billing/egress                Egress usage (Read)
POST /api/v1/billing/egress/checkout       Egress checkout (Write)
POST /api/v1/billing/checkout              Checkout (Write)
POST /api/v1/billing/change-plan           Change plan (Write)
GET  /api/v1/billing/portal                Customer portal link (Write)
GET  /api/v1/billing/overage               Overage status (Read)
POST /api/v1/billing/overage/enable        Enable overage (ManageTenants)
POST /api/v1/billing/overage/disable       Disable overage (ManageTenants)
GET  /api/v1/billing/projected-charges     Projected charges (Read)
POST /api/v1/webhooks/lemonsqueezy         LemonSqueezy webhook (signature-verified)
```

### Agents and x402
```
POST /api/v1/agents/register           Register agent
POST /api/v1/agents/anonymous-trial    Anonymous trial key
GET  /api/v1/agents/me/payments        Payment history (Read)
POST /api/v1/agents/claim              Claim an agent (Read)
POST /api/v1/agent-echo                Echo (Read)
POST /x402/verify, POST /x402/settle, GET /x402/routes
```

### Admin (`/api/v1/admin`)
Operator routes, gated by the admin middleware. They are registered in `main.go` (approximately lines 798-957) and `client_keys_admin.go`. Groups: tenant administration (`/tenants`, `/tenants/:id/usage`, `/tenants/:id/quotas`, suspend and unsuspend), fleet health, inbox, design partners, changelog subscribers, partnerships, recovery (`/recovery/:id/...`), notices and messages, metrics and client health, alerts, SLOs, IP rules, token audit, billing administration, and view-as impersonation.

Client keys (operator session with the Admin role only):

```
POST   /api/v1/admin/tenants/:id/client-keys        Register a public P-256 JWK
GET    /api/v1/admin/tenants/:id/client-keys        List a tenant's client keys
DELETE /api/v1/admin/tenants/:id/client-keys/:kid   Revoke a client key
```

The admin middleware refuses every API credential, and tenants cannot register their own keys. The POST body is `{client_id, jwk, scopes}`: `jwk` is the public half only (a JWK with `d` is refused), and the response returns the `kid`, the RFC 7638 thumbprint the client puts in its token header. A registered client then authenticates with `Authorization: Bearer <ES256 JWT>` (`iss` = client id, `sub` = tenant, `aud`, `iat`, `exp` at most 60s after `iat`, `jti`); role and scopes come from the registration, not the token. Procedure: [`docs/runbooks/CLIENT_KEY_ROTATION.md`](../../docs/runbooks/CLIENT_KEY_ROTATION.md).

### Client Keys (Platform Operators Only)
```
POST   /api/v1/admin/tenants/:id/client-keys        Register a public P-256 JWK
GET    /api/v1/admin/tenants/:id/client-keys        List a tenant's client keys
DELETE /api/v1/admin/tenants/:id/client-keys/:kid   Revoke a client key
```

These routes sit behind the admin middleware, which refuses every API credential, and require an operator session with the Admin role. Tenants cannot register their own keys. The POST body is `{client_id, jwk, scopes}`: `jwk` is the public half only (a JWK with `d` is refused), and the response returns the `kid`, the RFC 7638 thumbprint the client puts in its token header. A registered client then authenticates with `Authorization: Bearer <ES256 JWT>` (`iss` = client id, `sub` = tenant, `aud`, `iat`, `exp` at most 60s after `iat`, `jti`); role and scopes come from the registration, not the token. Procedure: [`docs/runbooks/CLIENT_KEY_ROTATION.md`](../../docs/runbooks/CLIENT_KEY_ROTATION.md).

---

## 🚀 Quick Start

### Prerequisites
- Go 1.26.11 (matches `apps/control-plane/Dockerfile`)
- Core reachable at `CORE_SERVICE_URL` (default `http://localhost:3900`)
- (Optional) OTLP HTTP endpoint such as Jaeger or Tempo, set with `OTEL_ENDPOINT`

### Installation

```bash
# From the repository root
cd apps/control-plane

# Download modules
go mod download

# Set environment variables
export JWT_SECRET="your-jwt-secret-key"  # Must match Core and the Query Service
export AUDIT_LOG_PATH="audit.log"
export OTEL_ENDPOINT="localhost:4318"    # Optional, OTLP HTTP endpoint
export GIN_MODE="release"                # Production
```

### Running

#### Development Mode
```bash
go run main.go
```

#### Production Mode
```bash
# Build optimized binary
CGO_ENABLED=0 go build -ldflags="-s -w" -o control-plane .

# Run
./control-plane
```

#### Docker
```bash
# Build context is apps/control-plane; the image is distroless (apps/control-plane/Dockerfile)
docker build -t allsource-control-plane apps/control-plane
```

---

## ⚙️ Configuration

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `PORT` | `3901` | HTTP listen port |
| `JWT_SECRET` | `default-secret-change-in-production` | HS256 signing secret. Logs a warning when unset. Must match Core and the Query Service |
| `CORE_SERVICE_URL` | `http://localhost:3900` | Core base URL |
| `CORE_API_KEY` | `` | API key the typed Core client sends |
| `QUERY_SERVICE_URL` | `http://allsource-query.internal:3902` | Query Service base URL (metrics passthrough and delegation) |
| `QUERY_HEALTH_URL` | `<QUERY_SERVICE_URL>/health` | Query Service health URL used by fleet recovery |
| `AUDIT_LOG_PATH` | `audit.log` | Audit log file, appended as JSON lines |
| `OTEL_ENDPOINT` | `` | OTLP HTTP endpoint (for example `localhost:4318`). Empty disables tracing |
| `ENVIRONMENT` | `development` | Value of the tracing `environment` attribute |
| `GIN_MODE` | `debug` | `release` switches Gin to release mode. The Dockerfile sets `release` |
| `METRICS_ADDR` | `:9091` | Private listener for tenant-labelled Prometheus metrics. Do not expose it publicly |
| `ADMIN_EMAILS` | `` | Comma-separated emails granted the Admin role at login. All other users get Developer |
| `ALLSOURCE_ADMISSION_MODE` | `shadow` | Per-tenant admission: `off`, `shadow` (count only) or `enforce` |
| `ALLSOURCE_ADMISSION_OVERRIDES` | `` | JSON object of per-tenant limit overrides. Required when enforcing (`{}` for none) |
| `ALLSOURCE_KEY_AUTHORITY_MODE` | `shadow` | Managed API-key authority: `off`, `shadow` (count and log would-be refusals in `control_plane_auth_denials_total`, serve the request) or `enforce` (refuse; 503 when Core cannot answer). Enforce once the shadow count is zero or every denied tenant has a registered key |
| `ALLSOURCE_LEGACY_KEY_SUNSET` | `` | RFC 3339 instant. Before it, a managed onboarding key with no registration record is still served, with `Deprecation` (RFC 9745) and `Sunset` (RFC 8594) headers. From it on, that key is refused with 401 `reason: legacy_key_sunset`. Keys with a registration record are unaffected. Unset changes nothing. No effect while `ALLSOURCE_KEY_AUTHORITY_MODE=off` |
| `ALLSOURCE_ASSERTION_AUDIENCE` | `https://api.all-source.xyz` | The `aud` an ES256 client assertion must carry |
| `ALLSOURCE_METRICS_PROM_URL` | `https://api.fly.io/prometheus/allsource` | Prometheus API the admin client-health view reads |
| `ALLSOURCE_METRICS_PROM_TOKEN` | `` | Read-only token for that API. Unset shows the client-health view as not configured |
| `ALLSOURCE_CORE_BINARY` | `` | Test only. Points the `egressintegration` and `corerelease` tests at a Core binary |

Optional integrations. Each is enabled only when its variable is set:

| Variable | Effect when set |
|----------|-----------------|
| `LEMON_SQUEEZY_API_KEY`, `LEMON_SQUEEZY_STORE_ID`, `LEMON_SQUEEZY_WEBHOOK_SECRET`, `LEMON_SQUEEZY_VARIANT_MAP` | Enables the LemonSqueezy client, checkout and webhook verification |
| `SMTP_HOST`, `SMTP_PORT`, `SMTP_USERNAME`, `SMTP_PASSWORD`, `SMTP_FROM` | Enables the email client (usage warnings) |
| `AUTH_SERVICE_URL` | Routes login and register to the external auth service instead of Core |
| `GITHUB_CLIENT_ID`, `GITHUB_CLIENT_SECRET` | GitHub OAuth provider |
| `GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET` | Google OAuth provider |
| `FRONTEND_URL`, `ALLOWED_FRONTEND_URLS`, `OAUTH_COOKIE_DOMAIN` | OAuth redirect targets and cookie domain |
| `TURNSTILE_SECRET_KEY` | Cloudflare Turnstile check on browser sign-up (no-op when unset) |
| `COINBASE_CDP_KEY_NAME`, `COINBASE_CDP_PRIVATE_KEY`, `COINBASE_CDP_NETWORK`, `BASE_RPC_URL` | Coinbase CDP client for x402 auto-pay |
| `X402_ENABLED`, `X402_PRICING_CONFIG`, `X402_RECIPIENT_ADDRESS`, `X402_FACILITATOR_URL` | x402 payment routes. The pricing file defaults to `config/x402-pricing.json`, which ships with `enabled` false |
| `PRIME_SERVICE_URL`, `PRIME_API_KEY` | Prime proxy |
| `EXTRACTION_LLM_URL`, `EXTRACTION_LLM_API_KEY` | Extraction proxy upstream |
| `ANTHROPIC_API_KEY`, `ANTHROPIC_BASE_URL`, `ANTHROPIC_MODEL` | Anthropic LLM client (`internal/infrastructure/clients/llm`) |
| `RESEND_API_KEY`, `RESEND_WEBHOOK_SECRET` | Resend email provider and its webhook |
| `CONNECTOR_SECRET_KEY` | Key material read by `internal/infrastructure/secrets` |
| `EGRESS_LEASE` | Enables the egress lease path (value parsed by `egressLeaseEnabled`) |
| `CONTROL_PLANE_MULTI_INSTANCE` | Logs a warning that the x402 included-allowance counter is per instance |
| `DEMO_ENABLED` | Switch read by the onboarding flow (`onboard.go`) |
| `GEO_TENANT_ID` | Tenant for geo self-report |

### Example Configuration

```bash
# Production configuration
export PORT=3901
export JWT_SECRET="$(openssl rand -base64 32)"  # Generate secure secret
export AUDIT_LOG_PATH="/var/log/allsource/audit.log"
export OTEL_ENDPOINT="jaeger:4318"  # OTLP HTTP endpoint
export ENVIRONMENT="production"
export GIN_MODE="release"
```

---

## 📜 Default Policies

The control plane ships with 5 default policies (`policy.go`, `addDefaultPolicies`). Higher priority is evaluated first:

### 1. Prevent Default Tenant Deletion
- **Resource**: `tenant`
- **Action**: `deny`
- **Conditions**: `tenant_id == "default" AND operation == "delete"`
- **Priority**: 100

### 2. Require Admin for Tenant Creation
- **Resource**: `tenant`
- **Action**: `deny`
- **Conditions**: `operation == "create" AND role != "admin"`
- **Priority**: 90

### 3. Prevent Self-Deletion
- **Resource**: `user`
- **Action**: `deny`
- **Conditions**: `operation == "delete" AND target_user_id == user_id`
- **Priority**: 95

### 4. Warn on Large Operations
- **Resource**: `operation`
- **Action**: `warn`
- **Conditions**: `record_count > 10000`
- **Priority**: 50

### 5. Rate Limit Expensive Operations
- **Resource**: `operation`
- **Action**: `deny`
- **Conditions**: `operation_type IN ["snapshot", "backup", "restore"] AND recent_operations > 5`
- **Priority**: 80

---

## 📋 Usage Examples

### 1. Register and Login

Register takes `name`, `email` and `password`. It returns 201 with a token. The request body has no `role` field: the role comes from `ADMIN_EMAILS` at login (Admin when the email is listed, Developer otherwise).

```bash
# Register a new user
curl -X POST http://localhost:3901/api/v1/auth/register \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Ada",
    "email": "admin@example.com",
    "password": "secure_password_123"
  }'

# Log in; email or username is accepted in the "email" or "username" field
TOKEN=$(curl -s -X POST http://localhost:3901/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{
    "email": "admin@example.com",
    "password": "secure_password_123"
  }' | jq -r '.token')

echo "Token: $TOKEN"
```

Login and register verify credentials against Core (`/api/v1/auth/login` and `/api/v1/auth/register` on Core) unless `AUTH_SERVICE_URL` is set.

### 2. Access Protected Endpoints

```bash
# Current user and tenant
curl -X GET http://localhost:3901/api/v1/auth/session \
  -H "Authorization: Bearer $TOKEN"

# Get cluster status
curl -X GET http://localhost:3901/api/v1/cluster/status \
  -H "Authorization: Bearer $TOKEN"
```

### 3. Manage Tenants (Admin Only)

```bash
# Create a new tenant (id and name are required)
curl -X POST http://localhost:3901/api/v1/tenants \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "id": "acme",
    "name": "Acme Corporation",
    "description": "Pilot tenant"
  }'

# List all tenants
curl -X GET http://localhost:3901/api/v1/tenants \
  -H "Authorization: Bearer $TOKEN"

# Get tenant details
curl -X GET http://localhost:3901/api/v1/tenants/acme \
  -H "Authorization: Bearer $TOKEN"
```

### 4. Trigger Operations (Admin Only)

```bash
# Create snapshot
curl -X POST http://localhost:3901/api/v1/operations/snapshots \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{}'

# Start a replay for one entity (tenant_id and entity_id are required)
curl -X POST http://localhost:3901/api/v1/operations/replay \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "tenant_id": "acme",
    "entity_id": "user-123",
    "as_of": "2025-01-15T10:00:00Z"
  }'
```

### 5. View Audit Log

```bash
# Tail audit log
tail -f audit.log

# View recent audit events
tail -20 audit.log | jq .

# Filter auth events
jq 'select(.event_type == "api_request" and .path | contains("auth"))' audit.log
```

---

## 🧪 Testing

### Unit Tests

```bash
cd apps/control-plane
go test -v -race -covermode=atomic ./...   # same command as make quality-go
```

### Tests That Need a Core Binary

Tests with the `egressintegration` build tag run `TestEgressRealCore*` against a real Core binary. They skip unless `ALLSOURCE_CORE_BINARY` is set:

```bash
cd apps/control-plane
ALLSOURCE_CORE_BINARY=/path/to/allsource-core \
  go test -tags egressintegration -run '^TestEgressRealCore' -count=1 .
```

The `corerelease` tag adds a read-performance comparison between a baseline and a candidate Core binary (`ALLSOURCE_CORE_BASELINE_BINARY`, `ALLSOURCE_CORE_BINARY`). The docker-build workflow runs it as `verify-core-release`.

---

## 📊 Monitoring & Observability

### Prometheus Metrics

The control-plane exposes Prometheus metrics at `/metrics`:

```
# HTTP request metrics
control_plane_http_requests_total
control_plane_http_request_duration_seconds
control_plane_http_requests_in_flight

# Core health check metrics
control_plane_core_health_check_total
control_plane_core_health_check_duration_seconds

# Operation metrics
control_plane_snapshot_operations_total
control_plane_replay_operations_total

# Auth and cache metrics
control_plane_auth_denials_total
control_plane_cache_hits_total
control_plane_cache_misses_total
```

Series labelled with a tenant are served only by the private listener on
`METRICS_ADDR` (Fly scrapes it over the private network through `[metrics]` in
`fly.toml`). It serves everything above plus:

```
control_plane_tenant_response_bytes_total{tenant,route,method,status_class}
control_plane_tenant_request_bytes_total{tenant,route,method}
control_plane_tenant_requests_total{tenant,route,method,status_class}
control_plane_tenant_event_query_body_bytes_total{tenant,namespace,...}
control_plane_tenant_event_queries_total{tenant,namespace,...}
control_plane_tenant_event_query_conditional_total{tenant,namespace}
admission_would_refuse_total{lane,scope,tenant}
admission_refused_total{lane,scope,tenant}
```

`tenant` is capped at 256 distinct values (later tenants report as `other`;
unauthenticated requests as `anonymous`). `route` is the matched route
template, or `unmatched`.

### Distributed Tracing (OTLP)

View distributed traces in Jaeger UI (or any OTLP-compatible backend like Grafana Tempo):

```bash
# Start Jaeger with OTLP support (Docker)
docker run -d --name jaeger \
  -p 16686:16686 \
  -p 4317:4317 \
  -p 4318:4318 \
  jaegertracing/all-in-one:latest

# Access Jaeger UI
open http://localhost:16686

# Configure control-plane with OTLP HTTP endpoint
export OTLP_ENDPOINT="localhost:4318"
```

### Audit Logs

```bash
# View all audit events
cat audit.log | jq .

# Filter by user
cat audit.log | jq 'select(.user_id == "user-123")'

# Filter by event type
cat audit.log | jq 'select(.event_type == "policy_denial")'

# Count requests by path
cat audit.log | jq -r '.path' | sort | uniq -c
```

---

## 🛡️ Security Best Practices

### Production Deployment

1. **Change JWT Secret**
   ```bash
   export JWT_SECRET="$(openssl rand -base64 32)"
   ```

2. **Enable HTTPS** (via reverse proxy like Nginx/Traefik)

3. **Rotate Secrets Regularly**

4. **Monitor Audit Logs**
   - Set up log aggregation (ELK, Splunk, etc.)
   - Alert on suspicious patterns

5. **Enable Rate Limiting** in Rust core

6. **Use strong passwords.** Password hashing happens in Core, not in the control plane.

7. **Restrict Network Access**
   - Firewall rules
   - VPC/security groups

---

## 🏗️ Development

### Project Structure

```
control-plane/
├── main.go              # Entry point
├── auth.go              # JWT authentication client
├── audit.go             # Audit logging system
├── tracing.go           # OpenTelemetry tracing
├── policy.go            # Policy enforcement engine
├── metrics.go           # Prometheus metrics
├── middleware.go        # Middleware
├── internal/            # Clean-architecture packages (domain, application, infrastructure, interfaces)
├── config/              # x402 pricing config (x402-pricing.json)
├── docs/                # openapi.yaml and X402_SETUP.md
├── go.mod               # Dependencies
├── go.sum               # Checksums
├── README.md            # This file
└── package.json         # Metadata
```

### Code Style

```bash
# Format code
go fmt ./...

# Lint (requires golangci-lint)
golangci-lint run

# Vet code
go vet ./...
```

---

## 📖 Resources

- [Gin Web Framework](https://gin-gonic.com/)
- [JWT Authentication](https://jwt.io/)
- [OpenTelemetry Go](https://opentelemetry.io/docs/instrumentation/go/)
- [Jaeger Tracing](https://www.jaegertracing.io/)
- [Prometheus Monitoring](https://prometheus.io/)
- [AllSource Core](../core/README.md)

---

## 🤝 Contributing

Contributions welcome! Areas of interest:
- Unit tests for Go code
- Additional default policies
- Enhanced tracing context
- Grafana dashboards
- Kubernetes operator

---

## 📄 License

The control plane is part of the enterprise edition, licensed under the [Business Source License 1.1](../../LICENSE-BSL). See the edition table in the [root README](../../README.md).

---

## 📊 Changelog

[apps/control-plane/CHANGELOG.md](CHANGELOG.md)

---

<div align="center">

**AllSource Control Plane v0.30.0** - *Enterprise orchestration*

Go service | v0.30.0

Integrates with AllSource Core (Rust)

[Documentation](./README.md) | [Core Service](../core/)

</div>
