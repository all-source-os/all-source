# Self-Hosting AllSource Chronos (Community Edition)

AllSource Chronos is a high-performance event store with full durability (WAL + Parquet + in-memory DashMap). The community edition is licensed under Apache 2.0 and includes everything you need to run a single-node event store with an API gateway.

## Quick Start

```bash
# Clone the repository
git clone https://github.com/all-source-os/all-source.git
cd all-source

# Start Core + Query Service
docker compose -f docker-compose.community.yml up -d

# Verify health
curl http://localhost:3900/health
# → {"status":"healthy","service":"allsource-core","version":"..."}

# Ingest your first event
curl -X POST http://localhost:3900/api/v1/events \
  -H "Content-Type: application/json" \
  -d '{"entity_id":"user-123","event_type":"UserCreated","payload":{"name":"Alice"}}'

# Query it back
curl "http://localhost:3900/api/v1/events/query?entity_id=user-123"
```

## Architecture

```
Clients → Query Service (port 3902) → Core (port 3900)
                                         |
                                    WAL + Parquet + DashMap
                                    (durable event storage)
```

- **Core** is the database. All event data is stored durably via WAL (write-ahead log with CRC32 checksums) and Parquet (columnar storage with Snappy compression). Events survive restarts.
- **Query Service** is the API gateway. In community mode, it provides basic routing to Core with dev-mode authentication.

## Environment Variables

### Core

| Variable | Default | Description |
|----------|---------|-------------|
| `ALLSOURCE_HOST` | `0.0.0.0` | Bind address |
| `ALLSOURCE_PORT` | `3900` | HTTP port |
| `ALLSOURCE_DATA_DIR` | `/app/data` | Data directory for WAL + Parquet |
| `ALLSOURCE_JWT_SECRET` | (random) | JWT signing secret |
| `ALLSOURCE_BOOTSTRAP_API_KEY` | (none) | Pre-configured API key |
| `ALLSOURCE_BOOTSTRAP_TENANT_ID` | `default` | Tenant for bootstrap API key |
| `RUST_LOG` | `allsource_core=info` | Log level |

### Query Service

| Variable | Default | Description |
|----------|---------|-------------|
| `CORE_URL` | `http://localhost:3900` | Core backend URL |
| `CORE_WS_URL` | `ws://localhost:3900/api/v1/events/stream` | Core WebSocket URL |
| `ALLSOURCE_EDITION` | `community` | Edition (`community` or `enterprise`) |
| `PORT` | `3902` | HTTP port |
| `SECRET_KEY_BASE` | (required) | Phoenix secret key |

## Data Persistence

Mount a volume to `/app/data` in the Core container. This directory contains:
- `wal/` — Write-ahead log files (crash recovery)
- `parquet/` — Columnar storage files (long-term persistence)
- `__system/` — System metadata (tenants, auth, config)

Data survives container restarts. Back up this directory for disaster recovery.

## Community vs Enterprise

| Feature | Community | Enterprise |
|---------|-----------|------------|
| Event store (WAL + Parquet) | Yes | Yes |
| Full CRUD API | Yes | Yes |
| Projections, schemas | Yes | Yes |
| Analytics (EventQL) | Yes | Yes |
| WebSocket streaming | Yes | Yes |
| Webhooks | Yes | Yes |
| Leader-follower replication | - | Yes |
| Multi-tenant management | - | Yes |
| Quota enforcement | - | Yes |
| Billing integration | - | Yes |
| Rate limiting tiers | Free tier | All tiers |

For enterprise features, contact the AllSource team or see [LICENSE-BSL](../../LICENSE-BSL) for commercial licensing options.

## Building from Source

```bash
# Build community Core image
docker build --target runtime-community-alpine -t allsource-core:community apps/core

# Build community Query Service image
docker build --build-arg ALLSOURCE_EDITION=community -t allsource-query:community apps/query-service
```

## API Reference

Core API endpoints (all under `/api/v1/`):

- `POST /api/v1/events` — Ingest an event
- `POST /api/v1/events/batch` — Batch ingest
- `GET /api/v1/events/query` — Query events (filter by entity_id, event_type, tenant_id, as_of/since/until, limit)
- `GET /api/v1/events/{id}` — Get event by ID
- `GET /api/v1/projections` — List projections
- `GET /api/v1/schemas` — List schemas
- `GET /api/v1/stats` — Storage statistics
- `GET /health` — Health check
- `GET /metrics` — Prometheus metrics
