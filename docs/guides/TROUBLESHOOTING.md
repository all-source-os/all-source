---
title: "Troubleshooting Guide"
status: CURRENT
last_updated: 2026-02-02
category: guide
---

# Troubleshooting Guide

This guide covers common issues and solutions for the AllSource platform. Each section includes diagnostic commands, problem descriptions, and step-by-step solutions.

---

## Quick Diagnostics

Before diving into specific issues, run these commands to check overall system health:

```bash
# Check all service ports
lsof -i :3900 -i :3901 -i :3902 -i :3000 2>/dev/null | grep LISTEN

# Verify services are responding
curl -s http://localhost:3900/health && echo " - Core OK"
curl -s http://localhost:3901/health && echo " - Control Plane OK"
curl -s http://localhost:3902/health && echo " - Query Service OK"

# Check Docker containers (if using Docker)
docker ps --format "table {{.Names}}\t{{.Status}}\t{{.Ports}}"
```

---

## Rust Core (Port 3900)

### Service Won't Start

**Symptoms:**
- `cargo run` exits immediately
- "Address already in use" error
- Panic on startup

**Diagnostic Commands:**
```bash
# Check if port is in use
lsof -i :3900

# Check Rust version
rustc --version
# Expected: 1.70.0 or higher

# Verify dependencies
cd apps/core
cargo check
```

**Solutions:**

**Port already in use:**
```bash
# Find and kill the process using port 3900
kill -9 $(lsof -t -i :3900)

# Or change the port in config
ALLSOURCE_PORT=3910 cargo run --release
```

**Missing dependencies:**
```bash
cd apps/core
cargo build --release 2>&1 | head -50
# Look for missing crate errors
```

**MSRV (Minimum Supported Rust Version) error:**
```bash
# Update Rust to required version
rustup update stable
rustup default stable

# Verify version meets requirement (1.70.0+)
rustc --version
```

---

### Connection Refused Errors

**Symptoms:**
- `curl: (7) Failed to connect to localhost port 3900`
- Control Plane can't reach Core
- WebSocket connections fail

**Diagnostic Commands:**
```bash
# Test basic connectivity
curl -v http://localhost:3900/health

# Check network interfaces
netstat -an | grep 3900

# Verify Core is bound correctly (should show 0.0.0.0:3900)
lsof -i :3900 -P
```

**Solutions:**

**Core not listening on all interfaces:**
```bash
# Ensure Core binds to 0.0.0.0 (not just 127.0.0.1)
# In your run command or config:
ALLSOURCE_HOST=0.0.0.0 cargo run --release
```

**Firewall blocking connections:**
```bash
# macOS: Check firewall settings
sudo /usr/libexec/ApplicationFirewall/socketfilterfw --getglobalstate

# Linux: Check iptables
sudo iptables -L -n | grep 3900
```

---

### Performance Issues

**Symptoms:**
- Query latency > 100ms (expected: ~12 microseconds)
- Memory usage growing unbounded
- CPU at 100%

**Diagnostic Commands:**
```bash
# Check memory usage
ps aux | grep allsource

# Profile with perf (Linux)
perf top -p $(pgrep allsource)

# Check event count
curl http://localhost:3900/api/v1/stats | jq '.total_events'
```

**Solutions:**

**High memory usage:**
```bash
# Check for memory leaks - restart with logging
RUST_LOG=debug cargo run --release 2>&1 | tee core.log

# Consider enabling Parquet persistence to offload memory
# Edit config to enable parquet_storage
```

**Slow queries:**
```rust
// Ensure indexes exist for common query patterns
// Check if entity_id queries are using DashMap correctly

// Performance baseline expectations:
// - DashMap query: 11.9 microseconds
// - 469K events/sec throughput
```

**CPU bound operations:**
```bash
# Check for blocking operations in async code
# Enable tokio-console for async debugging
RUSTFLAGS="--cfg tokio_unstable" cargo run --release
```

---

### WAL/Storage Issues

**Symptoms:**
- "WAL corrupted" errors
- Events not persisting after restart
- CRC checksum failures

**Diagnostic Commands:**
```bash
# Check WAL files
ls -la data/wal/

# Verify file permissions
stat data/wal/*

# Check disk space
df -h .
```

**Solutions:**

**WAL corruption:**
```bash
# Backup current WAL
mv data/wal data/wal.backup.$(date +%Y%m%d)

# Recreate WAL directory
mkdir -p data/wal

# Restart Core (will start fresh)
cargo run --release
```

**Permission issues:**
```bash
# Fix ownership
chown -R $(whoami) data/

# Fix permissions
chmod -R 755 data/
```

**Disk full:**
```bash
# Check and clean up old files
du -sh data/*

# Archive old Parquet files
gzip data/parquet/*.parquet
```

---

## Go Control Plane (Port 3901)

### JWT Authentication Failures

**Symptoms:**
- "invalid token" or "token expired" errors
- 401 Unauthorized responses
- "signature is invalid" in logs

**Diagnostic Commands:**
```bash
# Test authentication endpoint
curl -X POST http://localhost:3901/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin"}' | jq

# Decode JWT (without validation)
echo "YOUR_JWT_TOKEN" | cut -d'.' -f2 | base64 -d 2>/dev/null | jq
```

**Solutions:**

**Token expired:**
```bash
# Request new token
TOKEN=$(curl -s -X POST http://localhost:3901/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin"}' | jq -r '.token')

# Use new token
curl -H "Authorization: Bearer $TOKEN" http://localhost:3901/api/v1/tenants
```

**JWT secret mismatch:**
```bash
# Ensure JWT_SECRET environment variable is consistent
export JWT_SECRET="your-secret-key"
go run main.go
```

**Clock skew issues:**
```bash
# Check system time
date

# Sync time (macOS)
sudo sntp -sS time.apple.com

# Sync time (Linux)
sudo ntpdate pool.ntp.org
```

---

### Client Assertion and Legacy Key Refusals

**Symptoms:**
- A client using a registered client key (ES256 assertion) gets 401, 403 or 503
- A client using an old onboarding key gets 401 after a fixed date

Every refusal has the body `{"error", "reason", "message"}`. Read `reason`; it
names the failed check. The gateway also counts each one in
`control_plane_auth_denials_total{mode, reason, tenant}`.

**Diagnostic Commands:**
```bash
# Show the refusal body
curl -sS -i https://api.all-source.xyz/api/v1/events?limit=1 \
  -H "Authorization: Bearer $TOKEN"

# Decode the assertion header and claims (no validation)
echo "$TOKEN" | cut -d'.' -f1 | tr '_-' '/+' | base64 -d 2>/dev/null | jq
echo "$TOKEN" | cut -d'.' -f2 | tr '_-' '/+' | base64 -d 2>/dev/null | jq
```

**`asserted_signature` (401):** the token or `kid` is malformed, `jti` is
missing, or the signature does not verify with the registered public key. The
client is usually signing with a different key than the one registered. Recompute
the kid from the client's PEM ([Registered client keys](REGISTERED_CLIENT_KEYS.md#2-build-the-public-jwk-and-compute-the-kid))
and compare it with the registration.

**`asserted_audience` (401):** `aud` is not the one string the gateway expects
(`ALLSOURCE_ASSERTION_AUDIENCE`, default `https://api.all-source.xyz`). An array
`aud` is refused. Set the client's audience to match.

**`asserted_skew` (401):** `iat` or `nbf` is more than 30 s in the future, or
`exp - iat` is over 60 s. Sync the client clock (see above) or shorten the token
lifetime.

**`asserted_expired` (401):** `exp` is missing or more than 30 s in the past. The
client reused an old assertion; mint a new one per request or reuse one only
until shortly before `exp`.

**`asserted_unregistered` (401):** no registration matches (tenant, kid), or
`iss` is not the registered `client_id`, or `sub` is not the tenant. Ask an
operator to register the key, or fix `client_id` and tenant. After a new
registration, wait 30 s for the gateway cache.

**`asserted_revoked` (401):** the kid was revoked. A revoked key cannot be
registered again; generate a new keypair and have it registered.

**`asserted_unavailable` (503, `Retry-After: 5`):** the gateway could not read the
tenant's registrations from Core and has no good read from the last 5 minutes.
Retry after 5 s; this is a Core outage, not a client problem.

**`history_invalid` / `incomplete_authority`:** the tenant's `clientkey:<tenant>`
stream failed the gateway's strict fold. Escalate to an operator.

**`scope` (403):** the key authenticated, but its registered scopes do not cover
the route. Register a key with the needed scopes.

**`legacy_key_sunset` (401):** the bearer is a managed onboarding key (an HS256
JWT minted by the old onboarding flow) with no registration record, and the
request came after `ALLSOURCE_LEGACY_KEY_SUNSET`. Before that date such responses
carry `Deprecation` and `Sunset` headers. Dashboard `ask_` keys are not affected.
Move the client to a registered client key or a dashboard key. The Query Service
nests `reason` and `message` under `error` for this refusal.

Operator-side detail: [CLIENT_KEY_ROTATION.md](../runbooks/CLIENT_KEY_ROTATION.md#reading-refusals).

---

### RBAC Permission Denied

**Symptoms:**
- "permission denied" for valid users
- Roles not being applied correctly
- Admin operations failing

**Diagnostic Commands:**
```bash
# Check user roles
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:3901/api/v1/users/me | jq '.roles'

# List available permissions
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:3901/api/v1/rbac/permissions | jq
```

**Solutions:**

**Missing role assignment:**
```bash
# Assign admin role to user
curl -X POST http://localhost:3901/api/v1/users/{user_id}/roles \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"role": "admin"}'
```

**RBAC policy cache stale:**
```bash
# Restart Control Plane to reload policies
# Or trigger policy refresh via API
curl -X POST http://localhost:3901/api/v1/rbac/refresh \
  -H "Authorization: Bearer $ADMIN_TOKEN"
```

---

### Proxy Errors to Core

**Symptoms:**
- "upstream connection refused" errors
- Timeouts when proxying to Core
- 502 Bad Gateway responses

**Diagnostic Commands:**
```bash
# Verify Core is reachable from Control Plane
curl http://localhost:3900/health

# Check Control Plane logs
go run main.go 2>&1 | grep -i "proxy\|core\|upstream"

# Test direct vs proxied endpoints
curl http://localhost:3900/api/v1/events  # Direct
curl http://localhost:3901/api/v1/events  # Proxied
```

**Solutions:**

**Core URL misconfigured:**
```bash
# Set correct Core URL
export ALLSOURCE_CORE_URL=http://localhost:3900
go run main.go
```

**Core not started:**
```bash
# Start Core first, then Control Plane
cd apps/core && cargo run --release &
sleep 5  # Wait for Core to initialize
cd apps/control-plane && go run main.go
```

**Network timeout:**
```go
// Increase timeout in proxy configuration
// In apps/control-plane/internal/proxy/proxy.go:
client := &http.Client{
    Timeout: 30 * time.Second,  // Increase from default
}
```

---

### Tracing Configuration

**Symptoms:**
- No traces appearing in Jaeger/Zipkin
- Span context not propagating
- Missing trace IDs in logs

**Diagnostic Commands:**
```bash
# Check if tracing endpoint is configured
env | grep OTEL

# Verify Jaeger is running (if using)
curl http://localhost:16686/api/services | jq

# Check Control Plane trace export
go run main.go 2>&1 | grep -i "trace\|otel\|jaeger"
```

**Solutions:**

**Enable tracing:**
```bash
# Set OpenTelemetry configuration
export OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317
export OTEL_SERVICE_NAME=allsource-control-plane
go run main.go
```

**Missing trace context propagation:**
```bash
# Ensure W3C trace context headers are forwarded
# Add to proxy configuration:
# - traceparent
# - tracestate
```

---

## Elixir Query Service (Port 3902)

### WebSocket Connection Issues

**Symptoms:**
- `⚠️ WebSocket config needed` warning on startup
- No real-time event updates
- `core_websocket: disconnected` in health check

**Diagnostic Commands:**
```bash
# Check health endpoint for WebSocket status
curl http://localhost:3902/api/health | jq '.components.core_websocket'

# Verify CORE_WS_URL is set
docker exec allsource-query-service printenv | grep CORE_WS

# Test WebSocket endpoint on Core
curl -i -N \
  -H "Connection: Upgrade" \
  -H "Upgrade: websocket" \
  -H "Sec-WebSocket-Version: 13" \
  -H "Sec-WebSocket-Key: test" \
  http://localhost:3900/api/v1/events/stream
```

**Solutions:**

**WebSocket URL not configured:**
```bash
# Set the environment variable (use ws:// not http://)
export CORE_WS_URL=ws://allsource-core:3900

# Docker Compose
environment:
  CORE_WS_URL: ws://core:3900
```

**Wrong protocol prefix:**
```bash
# Wrong - will fail
CORE_WS_URL=http://allsource-core:3900

# Correct
CORE_WS_URL=ws://allsource-core:3900

# Production with TLS
CORE_WS_URL=wss://allsource-core.example.com:443
```

**Wrong port in Kubernetes:**
```bash
# Docker Compose uses port 3900
CORE_WS_URL=ws://core:3900

# Kubernetes uses port 3901
CORE_WS_URL=ws://allsource-core:3901
```

**Core service unreachable:**
```bash
# Ensure Core is running first
curl http://allsource-core:3900/health

# Check network connectivity
docker exec allsource-query-service ping allsource-core
```

**Disable WebSocket (use HTTP polling):**
```bash
# If you don't need real-time, disable WebSocket
export CORE_WS_ENABLED=false
```

See the full [WebSocket Configuration Guide](./WEBSOCKET_CONFIGURATION.md) for detailed setup instructions.

---

### Dependency Compilation Errors

**Symptoms:**
- `mix deps.get` fails
- Native extension compilation errors
- "could not compile dependency" messages

**Diagnostic Commands:**
```bash
# Check Elixir/Erlang versions
elixir --version
erl -eval 'erlang:display(erlang:system_info(otp_release)), halt().'

# Check for lock file issues
cd apps/query-service
cat mix.lock | head -20
```

**Solutions:**

**Native dependency failures (macOS):**
```bash
# Install build tools
xcode-select --install

# For specific NIFs (e.g., Jason, :crypto)
brew install openssl
export LDFLAGS="-L/opt/homebrew/opt/openssl/lib"
export CPPFLAGS="-I/opt/homebrew/opt/openssl/include"

mix deps.clean --all
mix deps.get
mix deps.compile
```

**Native dependency failures (Linux):**
```bash
# Install build essentials
sudo apt-get install build-essential erlang-dev

mix deps.clean --all
mix deps.get
mix deps.compile
```

**Hex/rebar issues:**
```bash
# Update Hex and rebar
mix local.hex --force
mix local.rebar --force

# Clean and retry
rm -rf _build deps
mix deps.get
```

---

### GenServer Timeout Issues

**Symptoms:**
- `** (EXIT) time out` errors
- GenServer calls hanging
- Process mailbox overflow

**Diagnostic Commands:**
```bash
# Connect to running node
iex --sname debug --remsh query_service@localhost

# Check process info
Process.info(pid, [:message_queue_len, :memory, :status])

# List all processes with large mailboxes
:erlang.processes() |> Enum.filter(fn p ->
  {:message_queue_len, len} = Process.info(p, :message_queue_len)
  len > 1000
end)
```

**Solutions:**

**Increase timeout:**
```elixir
# For specific calls that need more time
GenServer.call(pid, :expensive_operation, 30_000)  # 30 seconds
```

**Handle slow operations asynchronously:**
```elixir
# Instead of blocking calls
def handle_call(:slow_op, from, state) do
  Task.start(fn ->
    result = do_slow_operation()
    GenServer.reply(from, result)
  end)
  {:noreply, state}
end
```

**Reduce mailbox pressure:**
```elixir
# Use handle_continue for initialization
def init(args) do
  {:ok, initial_state, {:continue, :load_data}}
end

def handle_continue(:load_data, state) do
  # Heavy initialization here
  {:noreply, loaded_state}
end
```

---

### Phoenix Connection Problems

**Symptoms:**
- "connection refused" to port 3902
- WebSocket upgrades failing
- CORS errors in browser

**Diagnostic Commands:**
```bash
# Check if Phoenix is listening
lsof -i :3902

# Test endpoint
curl -v http://localhost:3902/health

# Check WebSocket endpoint
websocat ws://localhost:3902/socket/websocket
```

**Solutions:**

**Phoenix not binding to correct interface:**
```elixir
# In config/dev.exs or runtime.exs
config :query_service_ex, QueryServiceExWeb.Endpoint,
  http: [ip: {0, 0, 0, 0}, port: 3902]
```

**CORS configuration:**
```elixir
# In endpoint.ex or config
plug Corsica,
  origins: ["http://localhost:3000", "http://localhost:3901"],
  allow_headers: ["content-type", "authorization"],
  allow_methods: ["GET", "POST", "PUT", "DELETE", "OPTIONS"]
```

**WebSocket path mismatch:**
```javascript
// Frontend should connect to correct path
const socket = new Phoenix.Socket("/socket", {
  params: {token: authToken}
});
```

---

### Hot Reload Issues

**Symptoms:**
- Code changes not taking effect
- "module not found" after changes
- Compilation succeeds but old code runs

**Diagnostic Commands:**
```bash
# Check if file watcher is running
ps aux | grep -i "file.*watch"

# Force recompilation
mix compile --force

# Check loaded modules
iex -S mix
> :code.which(MyModule)
```

**Solutions:**

**Clear build artifacts:**
```bash
rm -rf _build
mix compile
```

**Restart with clean state:**
```bash
# Stop any running instances
pkill -f "mix\|beam"

# Start fresh
iex -S mix phx.server
```

**Check file watcher configuration:**
```elixir
# In config/dev.exs
config :query_service_ex, QueryServiceExWeb.Endpoint,
  live_reload: [
    patterns: [
      ~r"priv/static/.*(js|css|png|jpeg|jpg|gif|svg)$",
      ~r"lib/query_service_ex_web/(live|views)/.*(ex)$",
      ~r"lib/query_service_ex_web/templates/.*(eex)$"
    ]
  ]
```

---

## MCP Server

### Claude Desktop Connection Failures

**Symptoms:**
- MCP server not appearing in Claude Desktop
- "Server disconnected" messages
- No tool icons showing

**Diagnostic Commands:**
```bash
# Check MCP server can start
cd apps/mcp-server-elixir
mix run --no-halt

# Check Claude Desktop logs (macOS)
cat ~/Library/Logs/Claude/mcp*.log | tail -50

# Verify config path exists
cat ~/Library/Application\ Support/Claude/claude_desktop_config.json | jq
```

**Solutions:**

**Fix Claude Desktop configuration:**
```json
{
  "mcpServers": {
    "allsource": {
      "command": "mix",
      "args": ["run", "--no-halt"],
      "cwd": "/absolute/path/to/apps/mcp-server-elixir",
      "env": {
        "ALLSOURCE_CORE_URL": "http://localhost:3900",
        "ALLSOURCE_CONTROL_URL": "http://localhost:3901"
      }
    }
  }
}
```

**Common mistakes:**
```bash
# Ensure path is absolute (not relative)
# Wrong: "cwd": "./apps/mcp-server-elixir"
# Right: "cwd": "/Users/username/Projects/allsource/allsource-monorepo/apps/mcp-server-elixir"

# Ensure dependencies are installed
cd apps/mcp-server-elixir
mix deps.get
mix compile
```

**Restart Claude Desktop completely:**
1. Quit Claude Desktop (Cmd+Q on macOS)
2. Wait 5 seconds
3. Reopen Claude Desktop
4. Look for the plug icon in bottom right

---

### Tool Execution Errors

**Symptoms:**
- "Tool failed to execute" messages
- Partial results returned
- Timeout errors on tool calls

**Diagnostic Commands:**
```bash
# Test tool manually
cd apps/mcp-server-elixir
iex -S mix

# In IEx:
McpServerElixir.Tools.QueryEvents.execute(%{"entity_id" => "test-123"})
```

**Solutions:**

**Backend services not running:**
```bash
# Ensure Core is running
curl http://localhost:3900/health

# Ensure Control Plane is running
curl http://localhost:3901/health

# Check MCP server can reach them
curl http://localhost:3900/api/v1/events?entity_id=test-123
```

**Invalid tool parameters:**
```elixir
# Check parameter validation in tool module
# Ensure required parameters are provided
# Example: entity_id is usually required for queries
```

**Increase tool timeout:**
```elixir
# In MCP tool handler
def execute(params) do
  Task.async(fn -> do_heavy_work(params) end)
  |> Task.await(60_000)  # 60 second timeout
end
```

---

### TOON Encoding Issues

**Symptoms:**
- Malformed TOON responses
- JSON fallback not working
- "Invalid TOON format" errors

**Diagnostic Commands:**
```bash
# Test TOON encoding
cd apps/mcp-server-elixir
iex -S mix

# In IEx:
McpServerElixir.ToonEncoder.encode(%{test: "value"})
```

**Solutions:**

**Force JSON format:**
```elixir
# In tool response, specify format
%{
  format: :json,  # Force JSON instead of TOON
  data: response_data
}
```

**Fix TOON encoder:**
```elixir
# Check encoder handles all types
# Common issue: nested structures, atoms, dates

def encode(data) when is_map(data) do
  # Ensure all keys are strings
  data
  |> Enum.map(fn {k, v} -> {to_string(k), encode(v)} end)
  |> Enum.into(%{})
end
```

**Debug encoding:**
```elixir
# Add logging to see what's being encoded
def encode(data) do
  IO.inspect(data, label: "TOON input")
  result = do_encode(data)
  IO.inspect(result, label: "TOON output")
  result
end
```

---

## General Issues

### Port Conflicts

**Symptoms:**
- "Address already in use" errors
- Services failing to start
- Multiple instances running

**Diagnostic Commands:**
```bash
# Find all AllSource processes
ps aux | grep -E "allsource|cargo|go run|mix|node"

# Check specific ports
for port in 3000 3900 3901 3902; do
  echo "Port $port:"
  lsof -i :$port 2>/dev/null || echo "  Not in use"
done
```

**Solutions:**

**Kill conflicting processes:**
```bash
# Kill all processes on a specific port
kill -9 $(lsof -t -i :3900)
kill -9 $(lsof -t -i :3901)
kill -9 $(lsof -t -i :3902)

# Or kill all related processes
pkill -f "allsource\|allsource"
```

**Use alternative ports:**
```bash
# Core
ALLSOURCE_PORT=3910 cargo run --release

# Control Plane
CONTROL_PLANE_PORT=3911 go run main.go

# Query Service
PORT=3912 mix phx.server
```

---

### Docker Networking Issues

**Symptoms:**
- Containers can't communicate
- "Connection refused" between services
- Host can't reach containers

**Diagnostic Commands:**
```bash
# List networks
docker network ls

# Inspect network
docker network inspect allsource-network

# Check container IPs
docker inspect -f '{{range.NetworkSettings.Networks}}{{.IPAddress}}{{end}}' container_name

# Test inter-container connectivity
docker exec core ping control-plane
```

**Solutions:**

**Create shared network:**
```bash
# Create network
docker network create allsource-network

# Run containers on same network
docker run --network allsource-network --name core ...
docker run --network allsource-network --name control-plane ...
```

**Use service names instead of localhost:**
```yaml
# docker-compose.yml
services:
  core:
    environment:
      - HOST=0.0.0.0
  control-plane:
    environment:
      # Use service name, not localhost
      - ALLSOURCE_CORE_URL=http://core:3900
```

**Fix host networking:**
```bash
# For development, use host network
docker run --network host ...

# Or map ports explicitly
docker run -p 3900:3900 -p 3901:3901 ...
```

---

### Version Mismatches

**Symptoms:**
- API compatibility errors
- "Unknown field" or "missing field" errors
- Serialization failures

**Diagnostic Commands:**
```bash
# Check all versions
cd apps/core && cargo --version && cat Cargo.toml | grep version
cd apps/control-plane && go version && cat go.mod | head -5
cd apps/query-service && elixir --version && cat mix.exs | grep version
```

**Solutions:**

**Update all services:**
```bash
# Update Rust
rustup update

# Update Go
go mod tidy
go get -u ./...

# Update Elixir deps
mix deps.update --all
```

**Pin compatible versions:**
```toml
# Cargo.toml - use exact versions
serde = "=1.0.195"

# go.mod - use specific versions
require github.com/example/pkg v1.2.3

# mix.exs - use specific versions
{:phoenix, "~> 1.7.10"}
```

---

### Build Failures

**Symptoms:**
- Compilation errors
- Missing dependencies
- Linker errors

**Diagnostic Commands:**
```bash
# Rust build with verbose output
cd apps/core
cargo build --release -v 2>&1 | tail -100

# Go build with verbose output
cd apps/control-plane
go build -v ./... 2>&1

# Elixir with detailed errors
cd apps/query-service
mix compile --verbose 2>&1
```

**Solutions:**

**Clean and rebuild (all services):**
```bash
# Root makefile (if exists)
make clean
make build

# Or manually:
# Rust
cd apps/core && cargo clean && cargo build --release

# Go
cd apps/control-plane && go clean && go build

# Elixir
cd apps/query-service && rm -rf _build deps && mix deps.get && mix compile
```

**Fix missing system dependencies:**
```bash
# macOS
brew install openssl protobuf cmake

# Ubuntu/Debian
sudo apt-get install libssl-dev protobuf-compiler cmake build-essential

# Fedora/RHEL
sudo dnf install openssl-devel protobuf-compiler cmake gcc-c++
```

**Fix Rust-specific issues:**
```bash
# Update toolchain
rustup update

# Install missing components
rustup component add rustfmt clippy

# Fix cargo cache issues
rm -rf ~/.cargo/registry/cache
cargo build --release
```

---

## Getting Help

### Log Collection

When reporting issues, collect these logs:

```bash
# Create diagnostics directory
mkdir -p /tmp/allsource-diag

# Collect service logs
cargo run --release 2>&1 | tee /tmp/allsource-diag/core.log &
go run main.go 2>&1 | tee /tmp/allsource-diag/control-plane.log &
mix phx.server 2>&1 | tee /tmp/allsource-diag/query-service.log &

# Collect system info
uname -a > /tmp/allsource-diag/system.txt
rustc --version >> /tmp/allsource-diag/system.txt
go version >> /tmp/allsource-diag/system.txt
elixir --version >> /tmp/allsource-diag/system.txt

# Package for sharing
tar -czf allsource-diag.tar.gz /tmp/allsource-diag
```

### Useful Resources

- [Quick Start Guide](./QUICK_START.md) - Initial setup
- [Quality Gates](../current/QUALITY_GATES.md) - CI/CD troubleshooting
- [Claude Desktop Setup](./mcp-server/CLAUDE_DESKTOP_SETUP.md) - MCP configuration
- [Architecture Overview](../current/CLEAN_ARCHITECTURE.md) - System design

### Reporting Issues

When creating an issue, include:

1. **Environment**: OS, language versions, Docker version (if applicable)
2. **Steps to reproduce**: Exact commands that trigger the issue
3. **Expected behavior**: What should happen
4. **Actual behavior**: What actually happens
5. **Logs**: Relevant error messages and stack traces
6. **Configuration**: Relevant config files (sanitize secrets!)

---

## Quick Reference

| Service | Port | Health Check | Logs |
|---------|------|--------------|------|
| Rust Core | 3900 | `curl localhost:3900/health` | `cargo run 2>&1` |
| Go Control Plane | 3901 | `curl localhost:3901/health` | `go run main.go 2>&1` |
| Elixir Query Service | 3902 | `curl localhost:3902/health` | `mix phx.server` |
| Web UI | 3000 | `curl localhost:3000` | `bun dev` |

| Issue Type | First Command | Second Command |
|------------|---------------|----------------|
| Port in use | `lsof -i :PORT` | `kill -9 PID` |
| Service down | `curl localhost:PORT/health` | Check logs |
| Build failure | `make clean` | `make build` |
| Dependency issue | Clean deps directory | Reinstall deps |
