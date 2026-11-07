# Authentication Chain — Core, Query Service, and Clients

## Overview

```
Client (X-API-Key: ask_xxx)
        |
Query Service (Elixir, port 3902)
        |  validates key via Core /api/v1/auth/me
        |  caches result for 120s (ETS, SHA-256 key)
        |
Core (Rust, port 3900)
        |  AuthManager validates key hash in DashMap
        |  returns {tenant_id, role} from stored key
```

## How it works

### 1. Client sends request to Query Service

Clients authenticate with `X-API-Key` header:

```bash
curl -H "X-API-Key: ask_xxx..." https://query-service/api/v1/events?stream=orders
```

### 2. Query Service validates the key

The `AuthPipeline` plug (`auth_pipeline.ex`) tries methods in order:

1. `X-API-Key` header -> calls `ApiKeyCache.fetch/2`
2. `Authorization: Bearer <jwt>` -> validates JWT locally
3. Dev mode bypass (`AUTH_DISABLED=true`)

For API keys, `ApiKeyCache` checks its ETS cache (120s TTL, keyed by SHA-256
hash of the raw key). On cache miss, it calls `RustCoreClient.verify_api_key/1`.

### 3. RustCoreClient calls Core's `/api/v1/auth/me`

```elixir
# rust_core_client.ex — simplified
Tesla.get(client, "/api/v1/auth/me",
  headers: [{"authorization", raw_key}])
```

Core's auth middleware:
- Extracts the token from the `Authorization` header
- Detects it's an API key (starts with `ask_`)
- Calls `AuthManager.validate_api_key(&token)` which hash-matches against its
  in-memory DashMap (loaded from durable system WAL on startup)
- Returns `Claims {tenant_id, role}` -> synthesized into `UserInfo` by `/me`

### 4. Query Service resolves tenant

After `/me` returns `{tenant_id, role}`, Query Service:
- Fetches the full tenant record from Core (`GET /api/v1/tenants/:id`)
- Verifies subscription status (active/trialing)
- Caches the validated result in ETS
- Sets `conn.assigns` with `tenant_id`, `current_tenant`, `auth_method: :api_key`

## Hosted gateway (Control Plane)

Clients of the hosted service call the Go gateway at `https://api.all-source.xyz`,
which reads only the `Authorization: Bearer` header (`X-API-Key` is ignored).
`auth.go` routes the bearer by its shape:

1. **ES256 client assertion** (JWT header `alg: ES256` with a `kid`): verified in
   `asserted_keys.go` against the tenant's registered public keys, read from the
   `clientkey:<tenant>` entity in Core and cached for 30 s per gateway machine. No
   Core round-trip per request. Claims: `iss` = registered `client_id`, `sub` =
   tenant, `aud` = `ALLSOURCE_ASSERTION_AUDIENCE`, `exp - iat` at most 60 s, 30 s
   skew, `jti` required. Role (`serviceaccount`) and scopes come from the
   registration. Keys are registered by operators only
   (`/api/v1/admin/tenants/:id/client-keys`).
2. **`ask_` API key**: validated by Core `/api/v1/auth/me`, as above. The legacy
   sunset does not apply.
3. **Any other JWT**: validated locally. A managed-kind key is checked against the
   key authority (`apikey:<tenant>`); one with no `apikey.created` record (keys from
   the old onboarding `SignAPIKey`) is served until `ALLSOURCE_LEGACY_KEY_SUNSET`
   with `Deprecation` and `Sunset` response headers, and refused afterwards with
   `401` reason `legacy_key_sunset`.

Refusals answer `{"error", "reason", "message"}`. Reason table and procedure:
[CLIENT_KEY_ROTATION.md](../runbooks/CLIENT_KEY_ROTATION.md). Decision:
[2026-11-07 registered ES256 keys](../decisions/2026-11-07-tenant-clients-registered-es256-keys.md).
Client setup: [Registered client keys](../guides/REGISTERED_CLIENT_KEYS.md).

The Query Service does not verify assertions; asserted clients go through the
gateway.

## Environment variables

### Control Plane gateway

| Variable | Purpose |
|----------|---------|
| `ALLSOURCE_ASSERTION_AUDIENCE` | Required `aud` of client assertions (default `https://api.all-source.xyz`) |
| `ALLSOURCE_KEY_AUTHORITY_MODE` | `off`, `shadow` or `enforce` for managed-key checks |
| `ALLSOURCE_LEGACY_KEY_SUNSET` | RFC 3339 instant; unregistered managed keys are served before it and refused with `legacy_key_sunset` from it. Unset keeps prior behaviour |

### Core

| Variable | Purpose |
|----------|---------|
| `ALLSOURCE_BOOTSTRAP_API_KEY` | Bootstrap a persistent API key on first boot (idempotent) |
| `ALLSOURCE_BOOTSTRAP_TENANT_ID` | Tenant ID for the bootstrap key |
| `ALLSOURCE_DEV_MODE` | `true`/`1` to bypass auth (never in production) |
| `ALLSOURCE_DATA_DIR` | Enables system WAL for durable key storage |
| `JWT_SECRET` | Shared secret for JWT validation |

### Query Service

| Variable | Purpose |
|----------|---------|
| `CORE_URL` | Core HTTP URL (e.g. `http://core:3900`) |
| `CORE_WS_URL` | Core WebSocket URL for streaming |
| `CORE_API_KEY` | API key for QS -> Core internal calls |
| `JWT_SECRET` | Same shared secret as Core |
| `AUTH_DISABLED` | `true` to bypass auth in dev (QS-side) |
| `ALLSOURCE_LEGACY_KEY_SUNSET` | Same RFC 3339 instant as the gateway; must parse or boot fails. Before it, served legacy keys emit `[:query_service_ex, :auth, :legacy_key_served]`; after it they get `legacy_key_sunset` (nested under `error`) |

## Production setup (recommended)

```bash
# Core
ALLSOURCE_BOOTSTRAP_API_KEY=ask_your-production-key
ALLSOURCE_BOOTSTRAP_TENANT_ID=your-tenant
ALLSOURCE_DATA_DIR=/data/events
ALLSOURCE_DEV_MODE=false
JWT_SECRET=your-shared-secret

# Query Service
CORE_URL=http://core:3900
CORE_API_KEY=ask_your-production-key
JWT_SECRET=your-shared-secret
```

On first boot, Core persists the bootstrap key as a `_system.auth.key_provisioned`
event in the system WAL. On subsequent restarts, the key is replayed from storage
automatically — the env var is only needed for the initial creation (but is safe
to leave set; it's idempotent).

## Dev mode behavior

When `ALLSOURCE_DEV_MODE=true`:

- **Auth on provisioning endpoints is skipped** — you can create keys without
  an existing key
- **If a valid API key is provided**, it is validated normally and `/me` returns
  the key's real `tenant_id` and `role`
- **If no token is provided**, a synthetic admin context is used
  (`tenant_id: "dev-tenant"`, `role: admin`)
- **Rate limiting is disabled**

This means dev mode is safe to use for initial key provisioning without breaking
the QS auth chain — as long as the provisioned keys are persisted (requires
`ALLSOURCE_DATA_DIR` to be set).

## Key formats

- **API keys**: `ask_` prefix, validated by hash lookup in Core's AuthManager
- **JWTs**: Bearer tokens, validated by shared `JWT_SECRET`
- **Header precedence**: `Authorization` header checked first, `X-API-Key` as
  fallback (legacy)
