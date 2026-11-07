#!/usr/bin/env bash
# Production smoke test for the stateless, tenant-isolated Prime (epic t-10f876).
#
# Run this AFTER deploying 6c (see the runbook at the bottom). It verifies, in
# prod, that:
#   1. the new build is live (REST auth gate: no key → 401, not 201)
#   2. a tenant can write + read its own Prime node through the gateway
#   3. cross-tenant isolation: tenant B cannot see tenant A's node
#   4. the app holds no durable store (checked via `fly volumes`)
#
# Needs two real tenant API keys (mint at https://www.all-source.xyz/connect):
#   TENANT_A_KEY=...  TENANT_B_KEY=...  ./tooling/verify-prime-prod.sh
#
# Once the gateway's ALLSOURCE_LEGACY_KEY_SUNSET has passed, only these keys
# still authenticate: dashboard ask_ keys, managed keys with a registration
# record, and fresh ES256 client assertions for an operator-registered key.
# A managed key minted by the old onboarding flow with no record gets 401
# reason legacy_key_sunset, which this script reports as a failed create.
#
# Optional overrides:
#   API_BASE      (default https://api.all-source.xyz)   — the Control Plane edge
#   PRIME_DIRECT  (default https://allsource-prime.fly.dev) — the app's public URL
#   FLY_APP       (default allsource-prime)
#
# NOTE: this writes a marker node into tenant A's (and B's) memory. Harmless
# test data; delete via the dashboard if you care.

set -uo pipefail

API_BASE="${API_BASE:-https://api.all-source.xyz}"
PRIME_DIRECT="${PRIME_DIRECT:-https://allsource-prime.fly.dev}"
FLY_APP="${FLY_APP:-allsource-prime}"

pass=0
fail=0
ok()  { echo "  ✅ $1"; pass=$((pass+1)); }
no()  { echo "  ❌ $1"; fail=$((fail+1)); }

need() { [ -n "${!1:-}" ] || { echo "FATAL: set $1 (a tenant API key)"; exit 2; }; }
need TENANT_A_KEY
need TENANT_B_KEY

echo "== 1. New build live? app REST must REQUIRE the key =="
code=$(curl -sS -o /dev/null -w '%{http_code}' --max-time 20 \
  -X POST "$PRIME_DIRECT/api/v1/prime/nodes" \
  -H 'content-type: application/json' -d '{"type":"probe","properties":{}}')
# Old build returns 201 (unauthenticated write). New build: 401 (PRIME_API_KEY gate)
# or 400 (X-Tenant-Id required) — anything but 201 means the gate is live.
if [ "$code" = "201" ]; then no "POST /nodes with no key → 201 (OLD build still serving — 6c not deployed)"; \
  else ok "POST /nodes with no key → $code (auth gate live)"; fi

echo "== 2. tenant A writes + reads its own node (via gateway) =="
created=$(curl -sS --max-time 20 -X POST "$API_BASE/api/v1/prime/nodes" \
  -H "Authorization: Bearer $TENANT_A_KEY" -H 'content-type: application/json' \
  -d '{"type":"contact","properties":{"name":"VerifyA-'"$$"'"}}')
echo "    create resp: $created"
# entity_id like node:contact:<uuid>; pull it from the JSON.
eid=$(printf '%s' "$created" | grep -oE 'node:[a-z0-9_]+:[a-f0-9-]+' | head -1)
if [ -n "$eid" ]; then ok "tenant A created $eid"; else no "tenant A create failed (no entity_id in response)"; fi

if [ -n "$eid" ]; then
  a_read=$(curl -sS -o /dev/null -w '%{http_code}' --max-time 20 \
    -H "Authorization: Bearer $TENANT_A_KEY" "$API_BASE/api/v1/prime/nodes/$eid")
  [ "$a_read" = "200" ] && ok "tenant A reads its own node → 200" || no "tenant A read → $a_read (expected 200)"

  echo "== 3. ISOLATION: tenant B must NOT see tenant A's node =="
  b_read=$(curl -sS -o /dev/null -w '%{http_code}' --max-time 20 \
    -H "Authorization: Bearer $TENANT_B_KEY" "$API_BASE/api/v1/prime/nodes/$eid")
  # 404 = isolated. 200 = CROSS-TENANT LEAK — fail loudly.
  if [ "$b_read" = "200" ]; then no "tenant B sees tenant A's node → 200  *** CROSS-TENANT LEAK ***"; \
    else ok "tenant B cannot see tenant A's node → $b_read (isolated)"; fi
fi

echo "== 4. statelessness: the app must own NO volume =="
if command -v fly >/dev/null 2>&1; then
  vols=$(fly volumes list -a "$FLY_APP" 2>/dev/null | grep -c "prime_data" || true)
  [ "$vols" = "0" ] && ok "no prime_data volume on $FLY_APP (stateless)" || no "$FLY_APP still has a prime_data volume ($vols) — not stateless"
else
  echo "  (skip — fly CLI not found; run: fly volumes list -a $FLY_APP → expect none)"
fi

echo
echo "== RESULT: $pass passed, $fail failed =="
[ "$fail" -eq 0 ] && echo "PROD VERIFIED ✅" || { echo "PROD CHECK FAILED ❌"; exit 1; }

# =============================================================================
# DEPLOY RUNBOOK (6c) — run BEFORE this script
# =============================================================================
#   KEY=$(openssl rand -hex 32)
#   # Same shared secret on BOTH apps so CP can authenticate to the prime app:
#   fly secrets set PRIME_API_KEY="$KEY" -a allsource-control-plane
#   fly secrets set PRIME_API_KEY="$KEY" -a allsource-prime
#   # Point the prime app at Core + size the per-tenant warm cache:
#   fly secrets set CORE_URL=http://allsource-core.internal:3900 \
#                   PRIME_TENANT_CACHE_CAP=64 PRIME_TENANT_CACHE_TTL_SECS=300 -a allsource-prime
#   # (Optional) preflight the embedder model so first recall isn't slow:
#   #   the app supports `--mode warm` as a CI canary; or just let it warm on first use.
#   # Deploy the stateless app, then the CP (to pick up the new ProxyPrime):
#   fly deploy --config apps/prime-mcp/fly.toml --dockerfile apps/prime-mcp/Dockerfile -a allsource-prime
#   fly deploy -a allsource-control-plane
#   # Remove the now-unused volume (the app no longer mounts /data):
#   #   fly volumes list -a allsource-prime   # find the id
#   #   fly volumes destroy <id> -a allsource-prime
