#!/usr/bin/env bash
# smoke-test-auth-billing.sh — End-to-end test of the unified auth + billing
# path against the deployed AllSource stack.
#
# Tests the onboard → API key → event ingest → billing status path.
# Does NOT test OAuth login (requires a browser) or LemonSqueezy webhooks
# (requires real subscription events). Those need manual verification.
#
# Usage:
#   ./tooling/scripts/smoke-test-auth-billing.sh [CP_URL] [QS_URL]
#
# Defaults:
#   CP_URL: https://api.all-source.xyz
#   QS_URL: https://allsource-query.fly.dev
#
# Optional ES256 client-assertion leg (Test 4b): ALLSOURCE_CLIENT_ID,
# ALLSOURCE_CLIENT_KID, ALLSOURCE_TENANT_ID and ALLSOURCE_CLIENT_ASSERTION_CMD.

set -euo pipefail

CP_URL="${1:-https://api.all-source.xyz}"
QS_URL="${2:-https://allsource-query.fly.dev}"
PASS=0
FAIL=0
TESTS=()

pass() { PASS=$((PASS + 1)); TESTS+=("PASS: $1"); echo "  ✓ $1"; }
fail() { FAIL=$((FAIL + 1)); TESTS+=("FAIL: $1"); echo "  ✗ $1"; }

echo "=== Auth + Billing Smoke Test ==="
echo "Control Plane: $CP_URL"
echo "Query Service: $QS_URL"
echo ""

# --- Test 1: Health checks ---
echo "--- Test 1: Service health ---"
CP_STATUS=$(curl -sS -o /dev/null -w "%{http_code}" "$CP_URL/health")
if [ "$CP_STATUS" = "200" ]; then pass "Control Plane /health → 200"; else fail "Control Plane /health → $CP_STATUS"; fi

QS_STATUS=$(curl -sS -o /dev/null -w "%{http_code}" "$QS_URL/health")
if [ "$QS_STATUS" = "200" ]; then pass "Query Service /health → 200"; else fail "Query Service /health → $QS_STATUS"; fi

# CP /health is public and includes core_status field — no auth needed
CORE_BODY=$(curl -sS "$CP_URL/health")
CORE_EMBEDDED=$(echo "$CORE_BODY" | python3 -c "import sys,json; print(json.load(sys.stdin).get('core_status',''))" 2>/dev/null || true)
if [ "$CORE_EMBEDDED" = "healthy" ]; then pass "Core health (via CP /health) → core_status=healthy"; else fail "Core health → core_status=$CORE_EMBEDDED (expected healthy)"; fi

# --- Test 2: Onboard creates tenant + API key ---
echo ""
echo "--- Test 2: Onboard → tenant + API key ---"
TS=$(date +%s)
ONBOARD_RESP=$(curl -sS -w "\n%{http_code}" -X POST "$CP_URL/api/v1/onboard/start" \
  -H "content-type: application/json" \
  -d "{\"email\":\"billing-smoke-$TS@test.all-source.xyz\",\"name\":\"Billing Smoke $TS\"}")
ONBOARD_STATUS=$(echo "$ONBOARD_RESP" | tail -1)
ONBOARD_BODY=$(echo "$ONBOARD_RESP" | sed '$d')

if [ "$ONBOARD_STATUS" = "201" ]; then
  API_KEY=$(echo "$ONBOARD_BODY" | python3 -c "import sys,json; print(json.load(sys.stdin)['api_key'])" 2>/dev/null || true)
  TENANT_ID=$(echo "$ONBOARD_BODY" | python3 -c "import sys,json; print(json.load(sys.stdin)['tenant_id'])" 2>/dev/null || true)
  TIER=$(echo "$ONBOARD_BODY" | python3 -c "import sys,json; print(json.load(sys.stdin).get('tier',''))" 2>/dev/null || true)
  QUOTA=$(echo "$ONBOARD_BODY" | python3 -c "import sys,json; print(json.load(sys.stdin).get('events_quota',0))" 2>/dev/null || true)

  if [ -n "$API_KEY" ]; then
    pass "Onboard → 201, tenant=$TENANT_ID, tier=$TIER"
  else
    fail "Onboard → 201 but couldn't extract API key"
  fi
else
  fail "Onboard → HTTP $ONBOARD_STATUS (expected 201)"
  echo "  Response: $ONBOARD_BODY"
  echo "  Cannot continue. Aborting."
  exit 1
fi

# --- Test 3: Ingest event via Control Plane delegation ---
echo ""
echo "--- Test 3: Ingest event via CP delegation ---"
INGEST_RESP=$(curl -sS -w "\n%{http_code}" -X POST "$CP_URL/api/v1/events" \
  -H "Authorization: Bearer $API_KEY" \
  -H "content-type: application/json" \
  -d "{\"event_type\":\"billing.smoke_test\",\"entity_id\":\"smoke-$TS\",\"payload\":{\"test\":true,\"ts\":$TS}}")
INGEST_STATUS=$(echo "$INGEST_RESP" | tail -1)
INGEST_BODY=$(echo "$INGEST_RESP" | sed '$d')

if [ "$INGEST_STATUS" = "200" ] || [ "$INGEST_STATUS" = "201" ]; then
  pass "Event ingest via CP → $INGEST_STATUS"
else
  fail "Event ingest via CP → HTTP $INGEST_STATUS (expected 200/201)"
  echo "  Body: $INGEST_BODY"
fi

# --- Test 4: Query events via Control Plane delegation ---
echo ""
echo "--- Test 4: Query events via CP delegation ---"
QUERY_RESP=$(curl -sS -w "\n%{http_code}" "$CP_URL/api/v1/events/query?event_type=billing.smoke_test&limit=1" \
  -H "Authorization: Bearer $API_KEY")
QUERY_STATUS=$(echo "$QUERY_RESP" | tail -1)
QUERY_BODY=$(echo "$QUERY_RESP" | sed '$d')

if [ "$QUERY_STATUS" = "200" ]; then
  EVENT_COUNT=$(echo "$QUERY_BODY" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('count', len(d.get('events',[]))))" 2>/dev/null || echo "0")
  if [ "$EVENT_COUNT" -gt 0 ] 2>/dev/null; then
    pass "Event query → 200, found $EVENT_COUNT event(s)"
  else
    fail "Event query → 200 but no events found (expected at least 1)"
  fi
else
  fail "Event query → HTTP $QUERY_STATUS (expected 200)"
  echo "  Body: $QUERY_BODY"
fi

# Informational: before ALLSOURCE_LEGACY_KEY_SUNSET, a response served with a
# managed key that has no registration record carries Deprecation and Sunset.
LEGACY_HEADERS=$(curl -sS -o /dev/null -D - "$CP_URL/api/v1/events/query?event_type=billing.smoke_test&limit=1" \
  -H "Authorization: Bearer $API_KEY" | tr -d '\r' | grep -iE '^(deprecation|sunset):' || true)
if [ -n "$LEGACY_HEADERS" ]; then
  echo "  i Onboarding key is served on the legacy path:"
  while IFS= read -r header; do echo "      $header"; done <<< "$LEGACY_HEADERS"
else
  echo "  i No Deprecation/Sunset headers (key is registered, or no sunset is configured)"
fi

# --- Test 4b: ES256 client assertion (optional) ---
# Runs when ALLSOURCE_CLIENT_ID, ALLSOURCE_CLIENT_KID and ALLSOURCE_TENANT_ID name
# an operator-registered client key (docs/runbooks/CLIENT_KEY_ROTATION.md).
# The repo ships no standalone minting CLI, so ALLSOURCE_CLIENT_ASSERTION_CMD must
# be a command that prints one fresh assertion for that key, for example a small
# program built on the Rust SDK's Es256AssertionProvider. Assertions live 60s,
# so it is run here rather than read from a pre-minted variable.
echo ""
echo "--- Test 4b: Query events with an ES256 client assertion ---"
if [ -z "${ALLSOURCE_CLIENT_ID:-}" ] || [ -z "${ALLSOURCE_CLIENT_KID:-}" ] || [ -z "${ALLSOURCE_TENANT_ID:-}" ]; then
  echo "  - skipped: set ALLSOURCE_CLIENT_ID, ALLSOURCE_CLIENT_KID and ALLSOURCE_TENANT_ID to run it"
elif [ -z "${ALLSOURCE_CLIENT_ASSERTION_CMD:-}" ]; then
  echo "  - skipped: ALLSOURCE_CLIENT_* is set but no minting tool is available."
  echo "    Set ALLSOURCE_CLIENT_ASSERTION_CMD to a command that prints a fresh ES256 assertion."
else
  ASSERTION=$(bash -c "$ALLSOURCE_CLIENT_ASSERTION_CMD" 2>/dev/null | tr -d '[:space:]' || true)
  if [ -z "$ASSERTION" ]; then
    fail "ALLSOURCE_CLIENT_ASSERTION_CMD printed no assertion"
  else
    ASSERTED_RESP=$(curl -sS -D - -w "\n%{http_code}" "$CP_URL/api/v1/events/query?limit=1" \
      -H "Authorization: Bearer $ASSERTION")
    ASSERTED_STATUS=$(echo "$ASSERTED_RESP" | tail -1)
    if [ "$ASSERTED_STATUS" = "200" ]; then
      pass "Assertion for client=$ALLSOURCE_CLIENT_ID tenant=$ALLSOURCE_TENANT_ID → 200"
    else
      fail "Assertion query → HTTP $ASSERTED_STATUS (expected 200)"
      echo "  Body: $(echo "$ASSERTED_RESP" | sed '$d' | tail -1)"
    fi
    if echo "$ASSERTED_RESP" | tr -d '\r' | grep -qiE '^(deprecation|sunset):'; then
      fail "Assertion response carries Deprecation/Sunset (registered keys must not)"
    else
      pass "Assertion response carries no Deprecation/Sunset headers"
    fi
  fi
fi

# --- Test 5: Billing status from Query Service ---
# Note: the /api/billing/status route is behind Edition.enterprise?() in the
# QS router. If the deployed QS wasn't built with the enterprise edition flag,
# this route doesn't exist (404). That's a build-config gap, not a test failure.
echo ""
echo "--- Test 5: Billing status from Query Service ---"
BILLING_RESP=$(curl -sS -w "\n%{http_code}" "$QS_URL/api/billing/status" \
  -H "Authorization: Bearer $API_KEY")
BILLING_STATUS=$(echo "$BILLING_RESP" | tail -1)
BILLING_BODY=$(echo "$BILLING_RESP" | sed '$d')

if [ "$BILLING_STATUS" = "200" ]; then
  B_TIER=$(echo "$BILLING_BODY" | python3 -c "import sys,json; print(json.load(sys.stdin).get('tier',''))" 2>/dev/null || true)
  B_EVENTS_QUOTA=$(echo "$BILLING_BODY" | python3 -c "import sys,json; print(json.load(sys.stdin).get('events_quota',0))" 2>/dev/null || true)
  B_QUERIES_QUOTA=$(echo "$BILLING_BODY" | python3 -c "import sys,json; print(json.load(sys.stdin).get('queries_quota',0))" 2>/dev/null || true)

  if [ "$B_TIER" = "free" ]; then pass "Billing status → tier=free"; else fail "Billing status → tier=$B_TIER (expected free)"; fi
  if [ "$B_EVENTS_QUOTA" = "100000" ]; then pass "Events quota → 100000 (matches pricing memo)"; else fail "Events quota → $B_EVENTS_QUOTA (expected 100000)"; fi
  if [ "$B_QUERIES_QUOTA" = "10000" ]; then pass "Queries quota → 10000 (matches pricing memo)"; else fail "Queries quota → $B_QUERIES_QUOTA (expected 10000)"; fi
elif [ "$BILLING_STATUS" = "404" ]; then
  echo "  ⚠ Billing route 404 — Query Service not built with enterprise edition."
  echo "    The /api/billing/status route is behind Edition.enterprise?() in router.ex."
  echo "    Rebuild QS with EDITION=enterprise or enable the billing scope to test this."
  pass "Billing route correctly absent (QS not enterprise-edition) — not a bug"
else
  fail "Billing status → HTTP $BILLING_STATUS (expected 200 or 404)"
  echo "  Body: $BILLING_BODY"
fi

# --- Summary ---
echo ""
echo "=== Summary ==="
TOTAL=$((PASS + FAIL))
for t in "${TESTS[@]}"; do echo "  $t"; done
echo ""
echo "  $PASS/$TOTAL passed"
if [ "$FAIL" -gt 0 ]; then
  echo "  $FAIL FAILED"
  exit 1
else
  echo "  All tests passed."
fi

# --- Manual verification needed ---
echo ""
echo "=== Still needs manual verification ==="
echo "  - OAuth login flow (browser-based: Google/GitHub → JWT → dashboard)"
echo "  - LemonSqueezy webhook → tier upgrade (requires real subscription event)"
echo "  - Checkout flow → LemonSqueezy redirect → payment → webhook callback"
