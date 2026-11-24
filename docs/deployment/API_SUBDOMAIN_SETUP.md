# `api.all-source.xyz` — Subdomain Setup

Runbook for pointing the public `api.all-source.xyz` subdomain at the
Control Plane on Fly.io. Use this when the DNS + cert side of epic
`t-ce84` (Control Plane delegation layer) is ready to ship.

Until this runbook completes, clients (chronis, SDKs) can point at
`https://allsource-control-plane.fly.dev` directly — the delegation
routes work the same way, just with a less pleasant hostname.

## Target architecture

```
chronis / SDK  →  https://api.all-source.xyz        (public)
                  ─▶ allsource-control-plane.fly.dev (Fly cert + CNAME)
                      ─▶ delegation layer
                          ├─▶ allsource-core.internal:3900      (writes)
                          └─▶ allsource-query.internal:3902     (reads)
```

Core, Prime, and Query Service remain internal-only — only Control
Plane has public DNS.

## Prerequisites (verified 2026-04-17)

- `allsource-control-plane` exists on Fly, owner `allsource`
- Dedicated IPv6: `2a09:8280:1::d4:42b8:0`
- Shared IPv4: `66.241.125.106`
- `all-source.xyz` DNS is managed in Unstoppable Domains; its apex, `www`, `admin` and `api` records point at Fly
- No existing Fly certs for this subdomain

## Step 1 — allocate the cert on Fly

Safe / reversible — this just registers the intent to issue a cert and
prints the DNS records required.

```bash
fly certs add api.all-source.xyz -a allsource-control-plane
```

Expected output format:

```
CNAME    api.all-source.xyz               allsource-control-plane.fly.dev
AAAA     api.all-source.xyz               2a09:8280:1::d4:42b8:0
CNAME    _acme-challenge.api.all-source.xyz   <fly-printed-target>
```

Record the `_acme-challenge` target exactly as Fly prints it — it's
unique per cert.

## Step 2 — add records in Unstoppable Domains DNS

Unstoppable Domains DNS console → `all-source.xyz`. Add:

| Type  | Name                  | Value                                    |
|-------|-----------------------|------------------------------------------|
| CNAME | `api`                 | `allsource-control-plane.fly.dev`        |
| CNAME | `_acme-challenge.api` | *(target printed by Fly in Step 1)*      |

## Step 3 — wait and verify

Propagation depends on Unstoppable Domains DNS. Fly's LetsEncrypt issuance
fires automatically once the `_acme-challenge` CNAME resolves.

```bash
# DNS propagation
dig +short api.all-source.xyz
# Expect: allsource-control-plane.fly.dev CNAME chain resolving to the
# shared IPv4 / dedicated IPv6 above.

# Cert status
fly certs check api.all-source.xyz -a allsource-control-plane
# Expect: "Certificate for api.all-source.xyz: Issued"

# End-to-end
curl -I https://api.all-source.xyz/health
# Expect: HTTP/2 200 with x-fly-request-id
```

If `fly certs check` says "Awaiting DNS", re-check DNS propagation
and the `_acme-challenge` record matches what Fly printed.

## Step 4 — deploy and smoke-test delegation

```bash
fly deploy -a allsource-control-plane --config apps/control-plane/fly.toml
```

After deploy:

```bash
# 401 without auth (delegation routes require a tenant)
curl -i https://api.all-source.xyz/api/v1/events/query
# Expect: 401 Unauthorized

# Register a test account (Control Plane's public auth endpoint)
TS=$(date +%s)
RESP=$(curl -s -X POST https://api.all-source.xyz/api/v1/auth/register \
  -H "Content-Type: application/json" \
  -d "{\"name\":\"probe-$TS\",\"username\":\"probe-$TS\",\"email\":\"probe-$TS@test.local\",\"password\":\"$(openssl rand -hex 16)\"}")

# Keep the returned JWT as a human session token. It deliberately contains no
# long-lived API credential. Create a scoped test key from Dashboard → API Keys,
# then provide it to this shell without printing or committing it.
JWT=$(echo "$RESP" | python3 -c "import sys,json;print(json.load(sys.stdin)['token'])")
read -s "API_KEY?Scoped test API key: "
echo

# Write + read through the delegation layer
curl -s -X POST https://api.all-source.xyz/api/v1/events \
  -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"event_type":"smoke.test","entity_id":"smoke-1","payload":{"ok":true}}'

curl -s "https://api.all-source.xyz/api/v1/events/query?since=2020-01-01T00:00:00Z" \
  -H "Authorization: Bearer $API_KEY" | python3 -m json.tool
# Expect: {"events":[{event_type:"smoke.test",...}], "count":1, ...}
# Same tenant writes and reads — the original bug is fixed.
```

## Step 5 — cutover Core to internal-only

Only after the smoke test above passes end-to-end via `api.all-source.xyz`.
See epic `t-ce84`, beads `t-0ff8` (Core internal-only) and `t-a64e`
(remove Core's public auth middleware).

## Rollback

- `fly certs remove api.all-source.xyz -a allsource-control-plane`
- Delete the two DNS records in Unstoppable Domains DNS.
- Clients revert to using `allsource-control-plane.fly.dev` directly.
- No code changes required — chronis already accepts any `--remote` URL.
