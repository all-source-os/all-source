# Use Case: AI Agents Syncing Tasks to an AllSource Team Tenant

This document describes how AI agents (e.g., `ralph-tui`, custom LLM workers) authenticate
against AllSource Core and sync `chronis` task events into a shared team workspace.

---

## Overview

AllSource accepts three kinds of credentials:

| Credential | Format | Issued by | Used by |
|---|---|---|---|
| **CP JWT** | `eyJ...` (signed JWT) | Control Plane at login | Dashboard browser sessions |
| **Scoped API key** | signed bearer token | Dashboard API Keys | `chronis` CLI, AI agents, automation scripts |
| **Registered client key** | P-256 keypair; the client signs a 60 s ES256 assertion per request | the client generates it; a platform operator registers the public key via `POST /api/v1/admin/tenants/:id/client-keys` | long-running services and agents, `allsource-mcp --remote` |

A registered client key's role is always `serviceaccount` and its scopes come from
the registration. Setup: [Registered client keys](../guides/REGISTERED_CLIENT_KEYS.md).

Human team members receive a CP JWT on OAuth login. It contains identity and tenant claims only;
it never embeds a long-lived API key. The dashboard proxies authenticated requests with the
httpOnly session cookie.

AI agents never go through OAuth. They use **dedicated `ask_...` keys** provisioned per agent
by a team Admin or Owner, scoped to the team's Core tenant.

---

## End-to-End Flow

### 1. Human login

```
Browser → OAuth provider → Control Plane callback
  → create/find the user's tenant
  → sign a human JWT with identity, tenant, role, and expiry
  → return JWT to browser (auth_token cookie)
```

`GET /api/auth/session` returns user and tenant data only. Browser storage persists that same
non-secret profile state. Dashboard API requests stay behind same-origin proxies.

### 2. API key provisioning (manual, one-time per client)

A user creates a named, minimally scoped key under **Dashboard → API Keys**. The Chronis setup
action preselects `events:read` and `events:write`:

```
Dashboard → POST /api/api-keys
  { "name": "Chronis sync", "scopes": ["events:read", "events:write"] }
  → Query Service signs a tenant-scoped API credential
  → store key metadata and prefix
  → return the raw key once
```

The raw key is returned **exactly once** and masked by default. Copy it immediately.

### 3. Agent configuration

Add the key to the agent's `chronis` config:

```toml
# .chronis/config.toml
mode = "remote"

[sync]
remote_url = "https://api.all-source.xyz"
api_key    = "<your-key-here>"
```

Or via the CLI:

```bash
cn init --remote https://api.all-source.xyz --api-key <your-key-here>
```

From this point the agent writes and reads task events directly against Core with no
dashboard involvement.

### 4. Agent runtime

```
ralph-tui / custom agent
  → reads api_key from .chronis/config.toml
  → POST https://api.all-source.xyz/api/v1/events
      Authorization: Bearer ask_<key>
  → Core validates key: ServiceAccount role, resolves tenant from key record
  → event written to tenant's event stream
```

All agents sharing the same team tenant write to the same event stream. Human team members
can query that stream from the dashboard in real time.

---

## Permissions Matrix

| Operation | CP JWT (human) | ask_... (agent) |
|---|---|---|
| Ingest events (`POST /api/v1/events`) | via proxy | direct |
| Query events (`GET /api/v1/events/query`) | via proxy | direct |
| Manage schemas | via proxy | direct |
| List/manage team members | CP only | blocked |
| Create/revoke agent keys | CP only | blocked |
| Access Core config store | Admin JWT only | blocked |

Agent keys (`ask_...`) have `ServiceAccount` role: Read + Write on event data only. They
cannot create tenants, manage users, or access the config store.

---

## Key Lifecycle

### Listing keys

```
GET /api/team/agent-keys
→ returns [ { name, key_id, created_at }, ... ]
```

Key IDs are shown truncated in the dashboard. The raw `ask_...` value is never retrievable
after initial creation.

### Revoking a key

```
DELETE /api/team/agent-keys/:name
→ Control Plane RevokeAgentKeyHandler
    → DELETE /api/v1/auth/api-keys/:key_id on Core
    → removes AgentKeyMeta from Core config
```

After revocation, any agent using that key will receive `401 Unauthorized`. Update the
agent's `.chronis/config.toml` with a new key provisioned from the dashboard.

### Rotation workflow

1. In the dashboard, create a new key with a new name (e.g., `ralph-tui-prod-2`)
2. Update `.chronis/config.toml` on the agent with the new key
3. Verify the agent syncs successfully
4. Revoke the old key from the dashboard

A registered client key rotates differently: the operator registers the new public
key, the agent switches, and the operator revokes the old kid. See
[CLIENT_KEY_ROTATION.md](../runbooks/CLIENT_KEY_ROTATION.md).

### Legacy onboarding keys

Keys minted by the old onboarding flow that have no registration record stop working
at the gateway's `ALLSOURCE_LEGACY_KEY_SUNSET` and get `401` with reason
`legacy_key_sunset`. Dashboard keys created as above are not affected. An agent
holding an old onboarding key should move to a dashboard key or a registered client
key before that date.

---

## Multiple Agents, One Tenant

All agent keys provisioned under a team tenant write to the **same** Core event stream.
This is intentional: the team tenant is the shared workspace.

```
ralph-tui (agent key A) ──┐
custom-llm (agent key B) ─┤──→ team tenant event stream ──→ dashboard
ci-bot (agent key C) ──────┘
```

If you need per-agent isolation, create a separate team tenant for that agent.

---

## Security Notes

- **Keys are never stored server-side in plaintext.** Only metadata and a safe prefix are
  persisted. The raw bearer value exists only during creation or rotation.
- **One key per agent.** Name keys descriptively (e.g., `ralph-tui-staging`,
  `deploy-bot-prod`) so you know which key to revoke if an agent is compromised.
- **Revoke immediately** if a key is leaked. Rotation is safe and fast (see above).
- **Keys are scoped to the team tenant.** A key cannot be used to access another team's
  data even if the `tenant_id` is guessed.

---

## Dashboard UI

**Dashboard → API Keys**

- **Create sync key**: pre-fills a Chronis name and the read/write event scopes
- Key value shown once, masked by default, with reveal and copy controls
- Key table: name, safe prefix, scopes, dates, rotate, and revoke controls
- Empty state guides admins to create their first key
