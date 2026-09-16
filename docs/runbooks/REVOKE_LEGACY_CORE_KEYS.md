# Revoking legacy Core API keys

Procedure only. **No key material, key id, tenant id or hostname belongs in
this file, in a commit message, or in a GitHub issue — this repository is
public.** Keys go in a password manager or a Fly secret, nowhere else.

## What this is about

Core API keys minted before the current provisioning rules can carry a role or
a lifetime nobody would grant today, and copies of the plaintext value may still
sit in local configs from when they were issued. Core itself only ever stores
`key_hash`, and the struct marks it `#[serde(skip_serializing)]`, so the risk is
not in Core's store. It is in the copies that left it at mint time, and in keys
that outlived their reason to exist.

## Read this first: enumeration is currently blind

The operator surface cannot tell you which keys are legacy.
`clients.CoreAPIKeyInfo`, what `ListCoreAPIKeys` returns and what the recovery
dry-run prints, carries exactly four fields:

```go
type CoreAPIKeyInfo struct {
    ID       string
    Name     string
    TenantID string
    Active   bool
}
```

Core's own `ApiKey` has `role`, `created_at`, `expires_at` and `last_used`. All
four are dropped at the Control Plane boundary. Those are precisely the fields
that distinguish a legacy key from a current one, so today the only signal an
operator has is the key's *name*.

**Fix that before running a revocation sweep**, or the sweep is guesswork:
widen `CoreAPIKeyInfo` and the Core `GET /api/v1/auth/api-keys` response to
carry `role`, `created_at` and `last_used`, and show them in the rotate-keys
dry-run preview. Revoking on name-matching alone will either miss keys or kill
working ones.

## Identifying a legacy key, once the fields exist

| Signal | Why it matters |
|---|---|
| `role` is not `serviceaccount` | the canonical role; anything else predates the fix that made every other value 403 |
| `created_at` predates the current provisioning path | minted under rules that no longer apply |
| `last_used` is null or very old | nothing depends on it, so revoking is cheap |
| `expires_at` is null | a key with no end date is the one worth ending |

A key that is both never-used and off-role is the safe first cohort. A key in
active use needs its consumer rotated onto the replacement first.

## Revoking

Do **not** write a new revocation path. The guarded one already exists and is
audited:

```
POST /api/v1/admin/recovery/:tenant_id/rotate-keys
```

It mints a replacement with the canonical `serviceaccount` role **first**, then
revokes the old keys, so a failure cannot leave a tenant with no working key.

1. **Dry run.** `{"dry_run": true}`. The response names every key that will stop
   working and returns a `confirm_token`.
2. **Read the preview.** `keys_to_invalidate` and `key_names` are the blast
   radius. If a name you do not recognise appears, stop and find its consumer.
3. **Apply.** Echo the `confirm_token` from that dry run. A token from a
   different preview is refused.
4. **Hand the new key over** through a password manager or `fly secrets set`.
   Never paste it into chat, an issue, or a commit.
5. **Confirm the audit event.** Every apply writes `admin.recovery.*` into Core.

Full surface and auth model: [FLEET_HEALTH_RECOVERY.md](./FLEET_HEALTH_RECOVERY.md).
Admin access is the `ADMIN_EMAILS` allowlist, applied once at token mint.

## Purging plaintext copies

Revoking the key at Core is half the job. The copy that was handed out at mint
time still exists somewhere, and a revoked key in a config file is a support
ticket waiting to happen, not a vulnerability. Check, in this order:

| Where | How |
|---|---|
| MCP client configs | `claude_desktop_config.json`, `~/.cursor/mcp.json`, `.mcp.json` in any repo |
| Prime data dirs | `~/.prime/` and any `--data-dir` passed with `--api-key` |
| Fly secrets | `fly secrets list -a <app>` names them without printing values |
| CI secrets | repository and organisation secrets |
| Shell history | the one people forget |

Replace, do not just delete: a config pointing at a revoked key fails in a way
that reads like an outage.

## Definition of done

- [ ] `role`, `created_at` and `last_used` reach the operator surface
- [ ] Every tenant's keys enumerated and classified against the table above
- [ ] Legacy cohort rotated through the guarded flow, dry run read first
- [ ] Replacement keys delivered through a secret store
- [ ] Plaintext copies replaced everywhere in the table
- [ ] `admin.recovery.*` audit events present for every rotation

## Related

- Scopes on an API key are **display-only** today. An `events:read` key can
  write, because no scopes claim reaches the JWT and every key is
  `serviceaccount`. Revoking legacy keys does not change that; it is a separate
  open decision.
- The `serviceaccount` versus `service_account` role-string drift silently 403'd
  every key it touched. `rotate_keys` exists partly to repair that, which is why
  it forces the canonical role on the replacement.
