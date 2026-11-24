---
title: "Tenant Management Architecture"
status: CURRENT
last_updated: 2026-11-24
version: "2.0"
related:
  - "../archive/2026-11-23/current/ARCHITECTURE_OPTIMIZATION.md"
  - "../archive/2026-11-23/current/CLEAN_ARCHITECTURE.md"
  - "../adr/005-postgresql-removal-event-sourced-metadata.md"
---

# Tenant Management Architecture

> **Updated 2026-03-01:** Rewritten to reflect v0.10.0+ architecture where Core uses event-sourced system streams for tenant persistence and QS is fully stateless. Previous version (v1.0, 2026-02-02) showed Core tenants as plain DashMap without WAL backing. See ADR-005.

## TL;DR

- **Core** is the single source of truth for tenants via event-sourced system streams (WAL-durable)
- **Control Plane** provisions tenants in Core during demo/onboarding, proxies tenant CRUD
- **Query Service** is stateless — fetches tenant data from Core with ETS cache (5-min TTL)
- **No PostgreSQL** is used for tenant management

---

## Architecture Overview

```
┌─────────────────────────────────────────────────┐
│ Rust Core (Port 3900)                           │
│ ┌─────────────────────────────────────────────┐ │
│ │ SystemMetadataStore (event-sourced)         │ │
│ │ • Tenants stored as system stream events    │ │
│ │ • WAL-durable (survives restart)            │ │
│ │ • DashMap for fast reads (11.9 μs)          │ │
│ │ • SystemBootstrap loads on startup          │ │
│ └─────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────┘
           │
           │ HTTP API (/api/v1/tenants)
           ▼
┌─────────────────────────────────────────────────┐
│ Go Control-Plane (Port 3901)                    │
│ ┌─────────────────────────────────────────────┐ │
│ │ Provisions tenants during:                  │ │
│ │ • POST /api/v1/demo/start                   │ │
│ │ • POST /api/v1/onboard                      │ │
│ │ Proxies CRUD to Core, adds RBAC             │ │
│ └─────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────┘
           │
           │ JWT with tenant_id claim
           ▼
┌─────────────────────────────────────────────────┐
│ Elixir Query Service (Port 3902)                │
│ ┌─────────────────────────────────────────────┐ │
│ │ Stateless — NO database                     │ │
│ │ • RustCoreClient.get_tenant(id) → Core API  │ │
│ │ • ETS cache with 5-min TTL                  │ │
│ │ • Validates tenant from JWT claims           │ │
│ └─────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────┘
```

**Total PostgreSQL instances for tenants: 0**

---

## Current Implementation Details

### 1. Rust Core — Event-Sourced Tenants

**Storage**: System metadata stream events backed by WAL + DashMap cache.

Core stores tenants as events in system streams (`__system/` prefix). On startup, `SystemBootstrap` replays these events to rebuild the in-memory DashMap. Tenant data survives restarts via WAL.

**Key types**:
- `TenantManager` — DashMap-backed read cache for fast lookups (11.9 μs)
- `SystemMetadataStore` — event-sourced persistence layer
- `SystemBootstrap` — replays system streams on startup

**Tenant lifecycle**:
1. CP calls `POST /api/v1/auth/register` with `tenant_id`
2. Core creates tenant in system stream + DashMap
3. WAL ensures durability
4. On restart, `SystemBootstrap` rebuilds DashMap from WAL

### 2. Go Control-Plane — Tenant Provisioning

**File**: `apps/control-plane/internal/infrastructure/persistence/memory_tenant_repository.go`

CP maintains a local in-memory cache for its own routing, but Core is the source of truth. During demo/onboarding:

1. `POST /api/v1/demo/start` → creates user + tenant in Core
2. `POST /api/v1/onboard` → provisions tenant with explicit `tenant_id`
3. Issues JWT with `tenant_id`, `email`, `name` claims

### 3. Elixir Query Service — Stateless Tenant Cache

QS has **no database**. It fetches tenant data from Core on demand:

```elixir
# RustCoreClient.get_tenant(tenant_id) → Core HTTP API
# Result cached in ETS with 5-minute TTL
# TenantContext plug extracts tenant_id from JWT claims
```

### Durability

- **Event data**: Fully durable (WAL + Parquet). Survives restarts.
- **Tenant metadata**: Durable via system stream WAL. Survives restarts.
- **CP local cache**: In-memory only. Rebuilt from Core on demand. Not a concern — CP proxies to Core.

---

## Scaling Path

For multi-region or high-availability tenant management, the scaling strategy is Core leader-follower replication. Writes go to the Core leader, reads distributed across followers. No PostgreSQL needed.

---

## Summary

| Component | Tenant Storage | Persistence |
|-----------|---------------|-------------|
| Core | Event-sourced system streams + DashMap cache | WAL-durable |
| Control Plane | In-memory cache, delegates to Core | Not needed (Core is source of truth) |
| Query Service | ETS cache (5-min TTL), fetches from Core | Not needed (stateless) |
| PostgreSQL | Not used | N/A |

