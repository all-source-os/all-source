---
title: "Elixir Query Service"
status: CURRENT
last_updated: 2026-11-24
category: service
port: 3902
technology: Elixir
---

# AllSource Query Service (Elixir)

Query Service is the stateless, tenant-facing read plane over Core. It
separates four consumption paths without creating another database:

- **HTTP reads:** event queries, stream discovery, projection state, schemas,
  and replay jobs;
- **Realtime reads:** tenant-scoped event and projection updates through
  Phoenix Channels at `/ws`;
- **Analytics reads:** Core-backed and Query Service-computed aggregations with
  quotas and ETS caching;
- **Read models:** rebuildable per-tenant projections folded from durable Core
  events.

Core remains source of truth. Query Service caches and projection state can be
rebuilt from Core after restart.

Projections run as OTP processes under supervision. Event streams use Broadway,
GenStage and Phoenix Channels.

## Quick Start

```elixir
# Query DSL (lib/query_service_ex/application/dsl/query_dsl.ex)
import QueryServiceEx.Application.DSL.QueryDSL

from_events()
|> where(event_type: "order.placed")
|> since(days_ago(7))
|> limit(100)

# Projection process (lib/query_service_ex/application/use_cases/projection_server.ex)
{:ok, pid} = QueryServiceEx.Application.UseCases.ProjectionServer.start_link(
  projection: projection_definition,  # a ProjectionDefinition struct
  entity_id: "user-123"
)
```

## 🏗️ Architecture

```
domain/           # Pure business logic
application/      # Use cases & DSL
infrastructure/   # External adapters
cluster/
integrations/
prime/
projections/
views/
```

## Features

- **Query DSL** (`QueryServiceEx.Application.DSL.QueryDSL`): pipe-style query building
- **Projection processes** with OTP supervision (`ProjectionServer`)
- **Broadway** and **GenStage** pipelines (Kafka and RabbitMQ producers are dependencies; see `/api/integrations`)
- **Tesla** HTTP client to Core, with a Core WebSocket stream (`CORE_WS_URL`)
- **Phoenix Channels** at `/ws`
- **Telemetry** and Prometheus metrics (`/api/metrics/prometheus`)
- **OpenAPI** spec (`open_api_spex`) at `/api/openapi`, Swagger UI at `/api/docs`

### Tests

Run `mix test` from `apps/query-service`.

## API Endpoints

The service listens on port 3902 (`PORT`). Routes are defined in
`lib/query_service_ex_web/router.ex`. Pipelines are `:api` (no auth),
`:authenticated` (JWT or API key), and `:tenant_scoped` (auth, tenant context, consistency routing, egress budget).

### Public (no auth)
```
GET  /health                         Health (also at /api/health and /api/v1/health)
GET  /api/health/live                Liveness
GET  /api/health/ready               Readiness
GET  /api/health/replay              Replay health
GET  /api/health/consumers           Consumer health
GET  /api/metrics                    JSON metrics
GET  /api/metrics/prometheus         Prometheus text format
GET  /api/openapi                    OpenAPI 3.0 JSON
GET  /api/docs                       Swagger UI
GET  /api/cluster                    Cluster health
GET  /api/cluster/members            Cluster members
POST /api/v1/demo/seed               Demo seed
POST /api/v1/feedback                Feedback
GET  /api/v1/config/benchmarks       Benchmark config
POST /internal/update-leader         Leader URL update (called by allsource-sentinel)
POST /internal/tenant-updated        Tenant update (requires INTERNAL_API_KEY)
```

`GET /api/auth/dev-token` exists only in non-prod builds, and it returns a token only when `AUTH_DISABLED=true`.

### Authenticated (JWT or API key)
```
GET    /api/auth/me                  Current user (also /api/v1/auth/me)
POST   /api/auth/logout              Log out (also /api/v1/auth/logout)
GET    /api/cluster/registry, /supervisor, /hash-ring
GET    /api/integrations/kafka/config, /rabbitmq/config
POST   /api/integrations/kafka/publish, /rabbitmq/publish
GET    /api/admin/metrics/summary, /timeseries
GET    /api/api-keys                 List API keys
POST   /api/api-keys                 Create API key
DELETE /api/api-keys/:id             Revoke API key
GET    /api/billing/status           Billing status (enterprise build)
```

### Tenant-scoped (auth, tenant, active subscription)
Reads have rate limiting. Writes also enforce the event quota, and query endpoints enforce the query quota.
```
POST   /api/events                   Create event (quota)
POST   /api/events/batch             Batch create (quota)
GET    /api/events                   List events
GET    /api/events/query             Core-compatible query
GET    /api/events/:id               Event by id
GET    /api/events/entity/:entity_id
GET    /api/events/type/:event_type
GET    /api/streams
GET    /api/event-types
POST   /api/query                    Execute query (query quota)
GET    /api/projections              List projections
GET    /api/projection-templates
POST   /api/projections              Create projection
DELETE /api/projections/:name
GET    /api/projections/:name/state
GET    /api/webhooks                 List subscriptions
POST   /api/webhooks
GET    /api/webhooks/:id
PUT    /api/webhooks/:id
DELETE /api/webhooks/:id
GET    /api/webhooks/:id/deliveries
GET    /api/schemas
GET    /api/schemas/:event_type
POST   /api/schemas
GET    /api/replay                   Replay jobs
POST   /api/replay/preview
POST   /api/replay                   Start replay
GET    /api/replay/:id
POST   /api/replay/:id/cancel
DELETE /api/replay/:id
GET    /api/analytics/frequency, /summary, /correlation, /percentiles, /stddev,
       /sliding-window, /session-window, /cache/stats
POST   /api/analytics/cache/invalidate
POST   /api/customer-agent/...       Customer agent surface (own live boundary; not the JWT pipeline)
```

The same tenant-scoped routes are also served under `/api/v1` (events, projections, Prime).
Prime routes under `/api/v1/prime`: `GET /graph`, `GET|POST /projections`, `POST /nodes/:id/project`, `GET /nodes/:id/fields/:field/provenance`.

### Enterprise-only routes
Compiled only when `ALLSOURCE_EDITION=enterprise` (the Dockerfile build argument also controls it):
```
GET|PUT /api/tenant, GET /api/tenant/usage, GET|PUT /api/tenant/schema-enforcement
GET    /api/team/members
POST   /api/team/invite
DELETE /api/team/members/:user_id
PUT    /api/team/members/:user_id/role
GET /api/tenant/audit-logs
GET /api/tenants/me/analytics
GET /api/billing/status
```

Billing routes that moved to the Control Plane (`/api/billing/portal`, `checkout`,
`overage`, `overage/enable`, `overage/disable`, `projected-charges`) return
301 with a `location` header that points at `MGMT_PLANE_URL`.

## Authentication and the legacy key sunset

Hosted tenant clients reach this service through the gateway at
`https://api.all-source.xyz`. The Control Plane verifies ES256 client assertions
(`Authorization: Bearer <ES256 JWT>` signed with an operator-registered P-256
key). The Query Service does not verify those assertions itself
(`lib/query_service_ex/infrastructure/adapters/consumer_access_probe.ex`). Keys are
registered by platform operators only:
[`docs/runbooks/CLIENT_KEY_ROTATION.md`](../../docs/runbooks/CLIENT_KEY_ROTATION.md).

`ALLSOURCE_LEGACY_KEY_SUNSET` (RFC 3339, read in `config/runtime.exs`) sets the
instant after which a managed onboarding key with no registration record is
refused with 401 and `"reason": "legacy_key_sunset"`. Before that instant such a
key is still served, and the response carries `Deprecation` (RFC 9745) and
`Sunset` (RFC 8594) headers. Keys with a registration record are unaffected.
Unset changes nothing, and the variable has no effect while
`ALLSOURCE_KEY_AUTHORITY_MODE=off`. Set the same value on the Control Plane.

## Development

```bash
# Install dependencies
mix deps.get

# Run tests
mix test

# Start server (port 3902)
mix phx.server

# Interactive shell
iex -S mix
```

## Configuration

| Variable | Default | Read in | Purpose |
|----------|---------|---------|---------|
| `PORT` | `3902` | `runtime.exs` | HTTP port |
| `PHX_HOST` | `localhost` | `runtime.exs` | Host used in generated URLs |
| `SECRET_KEY_BASE` | temporary value when unset | `runtime.exs` | Phoenix secret. Set it in production |
| `ALLOWED_ORIGINS` | unset | `runtime.exs` | CORS origins |
| `ALLSOURCE_EDITION` | `community` | `config.exs`, `runtime.exs` | `enterprise` compiles the enterprise routes |
| `CORE_URL` | `http://localhost:3900` | `config.exs`, `runtime.exs` | Core base URL |
| `CORE_WRITE_URL` | `CORE_URL` | `runtime.exs` | Core target for writes |
| `CORE_READ_URLS` | unset | `runtime.exs` | Read replicas (comma-separated) |
| `CORE_MAX_REPLICATION_LAG_MS` | `5000` | `runtime.exs` | Replica lag limit |
| `CORE_HEALTH_CHECK_INTERVAL_MS` | `5000` | `runtime.exs` | Core health probe interval |
| `CORE_WS_URL` | `ws://localhost:3900/api/v1/events/stream` | `config.exs`, `runtime.exs` | Core event stream |
| `CORE_WS_ENABLED` | `true` | `runtime.exs` | Set `false` to disable the stream |
| `CORE_API_KEY` | unset | `config.exs`, `runtime.exs` | API key for Core requests |
| `JWT_SECRET` | unset | `jwt_auth.ex`, `auth_pipeline.ex` | HS256 secret for JWTs. Must match Core and the Control Plane |
| `INTERNAL_API_KEY` | unset | `internal_api_key.ex` | Key for `/internal/tenant-updated` |
| `MGMT_PLANE_URL` | `` | `hal.ex`, `billing_controller.ex` | Control Plane base for redirects and HAL links |
| `AUTH_DISABLED` | unset | `dev_mode.ex` | `true` or `1` bypasses auth outside prod. Ignored in prod |
| `ALLSOURCE_KEY_AUTHORITY_MODE` | `shadow` | `runtime.exs` | `off`, `shadow` or `enforce` |
| `ALLSOURCE_LEGACY_KEY_SUNSET` | unset | `runtime.exs` | See the sunset section above |
| `CUSTOMER_REVIEW_ENABLED`, `CUSTOMER_EVIDENCE_ENABLED`, `CUSTOMER_REPLAY_ENABLED`, `CUSTOMER_CONNECTIONS_ENABLED`, `CUSTOMER_REMOTE_ENABLED`, `VIEWS_FOLD_ENABLED` | `false` | `runtime.exs` | Feature flags (`"true"` enables) |

## Production Deployment

```bash
# Build Docker image (community edition is the default build argument)
docker build -t allsource-query-service:latest .

# Enterprise build
docker build --build-arg ALLSOURCE_EDITION=enterprise -t allsource-query-service:enterprise .

# Run with Docker
docker run -p 3902:3902 \
  -e CORE_URL=http://rust-core:3900 \
  -e SECRET_KEY_BASE=$(mix phx.gen.secret) \
  allsource-query-service:latest
```

The image runs as the non-root user `app` (UID 1000) on Alpine. The health check
calls `bin/query_service_ex rpc`.

