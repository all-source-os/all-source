#!/usr/bin/env bash
# State (secrets, data, logs) lives in $PR366_STATE, never in the repo.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../../.." && pwd)"
STATE="${PR366_STATE:-${TMPDIR:-/tmp}/pr366-proof}"
ENV="$STATE/stack.env"

case "${1:-}" in
  init)
    mkdir -p "$STATE"
    printf 'JWT=%s\nSYSKEY=ask_%s\n' "$(openssl rand -hex 32)" "$(openssl rand -hex 24)" >"$ENV"
    go build -C "$ROOT/apps/control-plane" -o "$STATE/control-plane" .
    echo "state: $STATE"
    ;;
  core)
    source "$ENV"
    mkdir -p "$STATE/core-data"
    export ALLSOURCE_HOST=127.0.0.1 ALLSOURCE_PORT=4900 ALLSOURCE_DATA_DIR="$STATE/core-data"
    export ALLSOURCE_JWT_SECRET="$JWT" ALLSOURCE_BOOTSTRAP_TENANT=system
    export ALLSOURCE_BOOTSTRAP_API_KEY="$SYSKEY" ALLSOURCE_BOOTSTRAP_TENANT_ID=system
    export ALLSOURCE_VIEWS=1 ALLSOURCE_FEED_DURABLE_LOG=all RUST_LOG=info
    exec "${CORE_BIN:-$ROOT/target/debug/allsource-core}" >>"$STATE/core.log" 2>&1
    ;;
  qs)
    source "$ENV"
    cd "$ROOT/apps/query-service"
    export MIX_ENV=dev ALLSOURCE_EDITION=enterprise
    export CORE_URL=http://127.0.0.1:4900 CORE_WS_URL=ws://127.0.0.1:4900/api/v1/events/stream
    export CORE_API_KEY="$SYSKEY" JWT_SECRET="$JWT" VIEWS_FOLD_ENABLED=true PORT=3902
    [ -f "$STATE/canaries.json" ] && export CONSUMER_CANARY_CREDENTIALS="$(cat "$STATE/canaries.json")"
    exec mix phx.server >>"$STATE/qs.log" 2>&1
    ;;
  cp)
    source "$ENV"
    export PORT=4901 CORE_SERVICE_URL=http://127.0.0.1:4900 QUERY_SERVICE_URL=http://127.0.0.1:3902
    export JWT_SECRET="$JWT" AUDIT_LOG_PATH="$STATE/cp-audit.log"
    [ -f "$STATE/canaries.json" ] && export CONSUMER_CANARY_CREDENTIALS="$(cat "$STATE/canaries.json")"
    exec "$STATE/control-plane" >>"$STATE/cp.log" 2>&1
    ;;
  *)
    echo "usage: $0 init|core|qs|cp" >&2
    exit 2
    ;;
esac
