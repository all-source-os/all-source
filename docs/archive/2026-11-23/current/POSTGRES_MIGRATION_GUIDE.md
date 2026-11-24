# Migration Guide: PostgreSQL to Event-Sourced Metadata

## Overview

As of v0.10.0, AllSource Core stores its own operational metadata (tenants, audit events, configuration) using its built-in event store rather than PostgreSQL. The `postgres` feature flag and its associated repository implementations are deprecated and will be removed in a future release.

This guide covers migrating existing PostgreSQL metadata into Core's system streams.

## Why This Change?

- **Eliminates an external dependency.** Core no longer requires a PostgreSQL instance for operational metadata.
- **Dogfooding.** Core uses its own WAL + DashMap persistence for metadata, the same storage engine that handles user events.
- **Simpler deployments.** One binary, one data directory. No database migrations, connection pools, or pg backups to manage.

## Architecture: Before and After

### Before (PostgreSQL)

```
Core --[sqlx]--> PostgreSQL
  tables: tenants, audit_events, config_entries
```

### After (Event-Sourced)

```
Core --> SystemMetadataStore (WAL at $ALLSOURCE_DATA_DIR/__system/)
  streams: _system:tenant:*, _system:audit:*, _system:config:*
  caches:  DashMap (tenants, config), RwLock<Vec> (audit)
```

## Migration Steps

### Step 1: Export PostgreSQL Data

Connect to your existing PostgreSQL instance and export the metadata as JSON.

**Tenants:**
```sql
SELECT json_agg(t) FROM (
  SELECT id, name, description, status, metadata,
         max_events_per_second, max_streams, max_storage_bytes,
         event_count, stream_count, storage_used_bytes,
         created_at, updated_at
  FROM tenants
  ORDER BY created_at
) t;
```

**Config entries:**
```sql
SELECT json_agg(c) FROM (
  SELECT key, value, description, category, created_at, updated_at
  FROM config_entries
  ORDER BY key
) c;
```

Audit events do not need migration - they are historical records. New audit events will be written to the system stream going forward.

### Step 2: Prepare System Stream Events

Convert each exported record into a system stream event. The event types and payload formats are:

**Tenant creation event:**
```json
{
  "event_type": "_system.tenant.created",
  "stream_name": "_system:tenant:{tenant_id}",
  "data": {
    "name": "my-tenant",
    "quotas": {
      "max_events_per_second": 10000,
      "max_streams": 1000,
      "max_storage_bytes": 10737418240
    },
    "metadata": {}
  }
}
```

**Config set event:**
```json
{
  "event_type": "_system.config.set",
  "stream_name": "_system:config:entries",
  "data": {
    "key": "retention.max_age_days",
    "value": "90",
    "description": "Max event retention in days",
    "category": "retention"
  }
}
```

### Step 3: Ingest via SystemMetadataStore

The simplest approach: set the `ALLSOURCE_SYSTEM_DATA_DIR` and `ALLSOURCE_BOOTSTRAP_TENANT` environment variables, start Core, and let it create a fresh system store. Then use Core's tenant management API to recreate your tenants:

```bash
# Set env vars
export ALLSOURCE_SYSTEM_DATA_DIR=/data/__system
export ALLSOURCE_BOOTSTRAP_TENANT=default

# Start Core - it will bootstrap with a default tenant
./allsource-core
```

For each tenant from your PostgreSQL export, use the tenant creation API or the event-sourced repository directly if running embedded.

### Step 4: Disable the PostgreSQL Feature

Once you've verified that all tenants and config are present in the system streams:

1. Remove `--features postgres` from your build command
2. Remove the `DATABASE_URL` environment variable
3. Decommission the PostgreSQL instance (or keep it for the Control Plane if applicable)

### Step 5: Verify

```bash
# Check health endpoint includes system stream info
curl http://localhost:3900/health | jq '.system_streams'

# Verify tenants are loaded
curl http://localhost:3900/api/v1/tenants
```

## Environment Variables

| Variable | Purpose | Default |
|----------|---------|---------|
| `ALLSOURCE_SYSTEM_DATA_DIR` | Directory for system metadata WAL | `$ALLSOURCE_DATA_DIR/__system/` |
| `ALLSOURCE_BOOTSTRAP_TENANT` | Default tenant name on first boot | `default` |

## Fallback Behavior

If `ALLSOURCE_SYSTEM_DATA_DIR` is not set and `ALLSOURCE_DATA_DIR` is not set, Core falls back to in-memory metadata (no persistence for tenants/config). This is the same behavior as running without the `postgres` feature previously.

If the system WAL is corrupted on startup, Core logs a warning and falls back to in-memory metadata. The data plane (user events) continues unaffected.

## Deprecated Components

The following are deprecated and will be removed in a future release:

| Component | Replacement |
|-----------|-------------|
| `PostgresTenantRepository` | `EventSourcedTenantRepository` |
| `PostgresAuditRepository` | `EventSourcedAuditRepository` |
| `PostgresEventStreamRepository` | Core's native WAL + Parquet storage |
| `postgres` feature flag in Cargo.toml | No feature flag needed (built-in) |

## Notes

- **PostgreSQL is still used by the Control Plane** (Go service) for its own operational metadata (users, policies, operations). This migration only affects Core (Rust).
- **PostgreSQL is still used by the Query Service** (Elixir) for user accounts, API keys, and billing. This migration does not affect the Query Service.
- The `rocksdb-storage` feature flag is unrelated and remains available.
