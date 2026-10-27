---
title: "Go Control Plane"
status: CURRENT
last_updated: 2026-02-02
category: service
port: 3901
technology: Go
---

# AllSource Control Plane v1.0

> Enterprise-grade orchestration and management layer for AllSource event store

[![Go](https://img.shields.io/badge/go-1.22-blue.svg)](https://golang.org/)
[![Status](https://img.shields.io/badge/status-v1.0-green.svg)]()
[![Framework](https://img.shields.io/badge/framework-gin-orange.svg)](https://gin-gonic.com/)
[![Auth](https://img.shields.io/badge/auth-JWT-red.svg)]()
[![Tracing](https://img.shields.io/badge/tracing-OpenTelemetry-purple.svg)]()

## 🎯 What's New in v1.0

The AllSource Control Plane has been upgraded from **v0.1.0** to **v1.0** with enterprise features:

### 🆕 v1.0 Features
- ✅ **JWT Authentication** - Validates tokens from Rust core
- ✅ **Role-Based Access Control (RBAC)** - 4 roles, 7 permissions
- ✅ **Audit Logging** - Complete audit trail of all operations
- ✅ **OpenTelemetry Tracing** - Distributed tracing via OTLP (Jaeger, Tempo, etc.)
- ✅ **Policy Enforcement** - 5 default policies, custom policy engine
- ✅ **Permission-Based Routes** - Fine-grained access control
- ✅ **Authenticated Proxying** - Secure forwarding to Rust core

### 🔄 Upgraded from v0.1.0
- ⬆️ **From**: Basic health checks and metrics
- ⬆️ **To**: Full enterprise orchestration layer
- ⬆️ **Added**: 5 new modules, 2,000 lines of code
- ⬆️ **Security**: Now requires authentication for all endpoints

---

## 🏗️ Architecture

```
┌─────────────────────────────────────────────────────┐
│            Operators / Dashboards                   │
│         (Authenticated via JWT/API Key)             │
└─────────────────────────────────────────────────────┘
                         │ HTTPS
                         ▼
┌─────────────────────────────────────────────────────┐
│       Go Control Plane v1.0 (Port 3901)             │
│  ┌───────────────────────────────────────────────┐  │
│  │  🔐 JWT Authentication Middleware             │  │
│  │  🛡️  RBAC & Permission Enforcement            │  │
│  │  📋 Audit Logging (All Operations)            │  │
│  │  📊 OpenTelemetry Distributed Tracing         │  │
│  │  📜 Policy Engine (5 Default Policies)        │  │
│  │  🎯 Authenticated Proxy to Core               │  │
│  └───────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────┘
                         │ HTTP + Auth Token
                         ▼
┌─────────────────────────────────────────────────────┐
│      Rust Event Store Core v1.0 (Port 8080)        │
│  • Event Ingestion (469K events/sec)                │
│  • Multi-Tenancy with Quotas                        │
│  • Rate Limiting (Token Bucket)                     │
│  • Authentication & RBAC                            │
│  • Backup & Restore                                 │
└─────────────────────────────────────────────────────┘
```

---

## ✨ Features

### 🔐 Authentication & Authorization
- **JWT Token Validation** - Validates tokens issued by Rust core
- **API Key Support** - Accepts API keys for service accounts
- **Auth Middleware** - Automatic token extraction and validation
- **Permission Checking** - Fine-grained permission enforcement
- **Role-Based Access** - 4 roles with distinct capabilities

### 📋 Audit Logging
- **Complete Audit Trail** - Logs all API requests and operations
- **Structured Logging** - JSON format for easy parsing
- **Event Types** - API requests, auth events, tenant events, operations
- **Rich Metadata** - User ID, tenant ID, IP address, user agent, duration
- **File-Based Storage** - Appends to `audit.log` file
- **Configurable** - Enable/disable via `AUDIT_LOG_PATH` env var

### 📊 Distributed Tracing
- **OpenTelemetry SDK** - Industry-standard tracing
- **OTLP Exporter** - Export traces via OTLP HTTP (works with Jaeger, Tempo, etc.)
- **Span Propagation** - Distributed context across services
- **Rich Attributes** - HTTP method, route, status, user info
- **Error Tracking** - Automatic error recording in spans
- **Custom Events** - Add custom events for important actions

### 📜 Policy Enforcement
- **Policy Engine** - Evaluate policies against requests
- **Condition-Based Rules** - Support for eq, ne, gt, lt, contains, in
- **Priority System** - Higher priority policies evaluated first
- **Actions** - Allow, Deny, Warn
- **Default Policies** - 5 security policies out-of-the-box
- **Custom Policies** - Add your own policies dynamically

### 🎯 Permission System

#### Roles
- **Admin** - Full access to all features
- **Developer** - Read, write, manage schemas & pipelines
- **ReadOnly** - Read-only access
- **ServiceAccount** - Read and write (no admin)

#### Permissions
- **Read** - View resources
- **Write** - Create and update resources
- **Admin** - Administrative operations
- **Metrics** - View metrics and monitoring data
- **ManageSchemas** - Manage event schemas
- **ManagePipelines** - Manage data pipelines
- **ManageTenants** - Manage tenant configuration

---

## 🔧 API Endpoints

### Public (No Auth)
```
GET  /health              Health check
GET  /metrics             Prometheus metrics
```

### Authentication
```
POST /api/v1/auth/register  Register new user (public)
POST /api/v1/auth/login     User login (public)
GET  /api/v1/auth/me        Current user info (requires auth)
```

### Cluster Management (Requires Auth)
```
GET  /api/v1/cluster/status       Cluster status
GET  /api/v1/metrics/json         Aggregated metrics
GET  /api/v1/health/core          Core service health
```

### Operations (Requires Admin Permission)
```
POST /api/v1/operations/snapshot  Create snapshot
POST /api/v1/operations/replay    Trigger event replay
POST /api/v1/operations/backup    Create backup
```

### Tenant Management (Admin Only)
```
GET    /api/v1/tenants         List all tenants
GET    /api/v1/tenants/:id     Get tenant details
POST   /api/v1/tenants         Create new tenant
PUT    /api/v1/tenants/:id     Update tenant
DELETE /api/v1/tenants/:id     Delete tenant
```

### User Management (Admin Only)
```
GET    /api/v1/users           List all users
DELETE /api/v1/users/:id       Delete user
```

---

## 🚀 Quick Start

### Prerequisites
- Go 1.22 or higher
- AllSource Rust Core v1.0 running on `localhost:3900`
- (Optional) OTLP-compatible tracing backend (Jaeger, Tempo, etc.)

### Installation

```bash
# Clone repository
cd services/control-plane

# Install dependencies (if Go is installed)
go mod download

# Set environment variables
export JWT_SECRET="your-jwt-secret-key"  # Must match Rust core
export AUDIT_LOG_PATH="audit.log"
export OTLP_ENDPOINT="localhost:4318"  # Optional, OTLP HTTP endpoint
export GIN_MODE="release"  # For production
```

### Running

#### Development Mode
```bash
go run main_v1.go
```

#### Production Mode
```bash
# Build optimized binary
CGO_ENABLED=0 go build -ldflags="-s -w" -o control-plane main_v1.go

# Run
./control-plane
```

#### Docker (Example Dockerfile)
```dockerfile
FROM golang:1.26-alpine AS builder
WORKDIR /app
COPY go.mod go.sum ./
RUN go mod download
COPY *.go ./
RUN CGO_ENABLED=0 go build -ldflags="-s -w" -o control-plane main_v1.go

FROM alpine:latest
RUN apk --no-cache add ca-certificates
WORKDIR /root/
COPY --from=builder /app/control-plane .
EXPOSE 8081
CMD ["./control-plane"]
```

---

## ⚙️ Configuration

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `PORT` | `8081` | Port to listen on |
| `JWT_SECRET` | `default-secret-change-in-production` | JWT secret (must match Rust core) |
| `AUDIT_LOG_PATH` | `audit.log` | Path to audit log file |
| `OTLP_ENDPOINT` | `` | OTLP HTTP endpoint (e.g., `localhost:4318` for Jaeger/Tempo) |
| `ENVIRONMENT` | `development` | Environment name (development, staging, production) |
| `GIN_MODE` | `debug` | Gin mode (`debug` or `release`) |
| `METRICS_ADDR` | `:9091` | Private listener for tenant-labelled Prometheus metrics; never expose it publicly |
| `ALLSOURCE_ADMISSION_MODE` | `shadow` | Per-tenant admission: `off`, `shadow` (count only, no response changes) or `enforce` |
| `ALLSOURCE_ADMISSION_OVERRIDES` | `` | JSON object of per-tenant limit overrides; required (`{}` for none) when enforcing |
| `ALLSOURCE_KEY_AUTHORITY_MODE` | `shadow` | Managed API-key authority: `off`, `shadow` (count and log would-be refusals in `control_plane_auth_denials_total{mode,reason,tenant}`, serve the request) or `enforce` (refuse; 503 when Core cannot answer). Enforce only once the shadow count is zero or every denied tenant has been reissued a registered key |
| `ALLSOURCE_METRICS_PROM_URL` | `https://api.fly.io/prometheus/allsource` | Prometheus API the admin client-health view reads |
| `ALLSOURCE_METRICS_PROM_TOKEN` | `` | Read-only Fly token for that API; unset shows the client-health view as not configured |

### Example Configuration

```bash
# Production configuration
export PORT=8081
export JWT_SECRET="$(openssl rand -base64 32)"  # Generate secure secret
export AUDIT_LOG_PATH="/var/log/allsource/audit.log"
export OTLP_ENDPOINT="jaeger:4318"  # OTLP HTTP endpoint
export ENVIRONMENT="production"
export GIN_MODE="release"
```

---

## 📜 Default Policies

The control-plane ships with 5 default security policies:

### 1. Prevent Default Tenant Deletion
- **Resource**: `tenant`
- **Action**: `deny`
- **Conditions**: `tenant_id == "default" AND operation == "delete"`
- **Priority**: 100

### 2. Require Admin for Tenant Creation
- **Resource**: `tenant`
- **Action**: `deny`
- **Conditions**: `operation == "create" AND role != "Admin"`
- **Priority**: 90

### 3. Prevent Self-Deletion
- **Resource**: `user`
- **Action**: `deny`
- **Conditions**: `operation == "delete" AND target_user_id == user_id`
- **Priority**: 95

### 4. Warn on Large Operations
- **Resource**: `operation`
- **Action**: `warn`
- **Conditions**: `record_count > 10000`
- **Priority**: 50

### 5. Rate Limit Expensive Operations
- **Resource**: `operation`
- **Action**: `deny`
- **Conditions**: `operation_type IN ["snapshot", "backup", "restore"] AND recent_operations > 5`
- **Priority**: 80

---

## 📋 Usage Examples

### 1. Register and Login

```bash
# Register a new admin user
curl -X POST http://localhost:3901/api/v1/auth/register \
  -H "Content-Type: application/json" \
  -d '{
    "username": "admin",
    "email": "admin@example.com",
    "password": "secure_password_123",
    "role": "Admin"
  }'

# Login to get JWT token
TOKEN=$(curl -s -X POST http://localhost:3901/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{
    "username": "admin",
    "password": "secure_password_123"
  }' | jq -r '.token')

echo "Token: $TOKEN"
```

### 2. Access Protected Endpoints

```bash
# Get current user info
curl -X GET http://localhost:3901/api/v1/auth/me \
  -H "Authorization: Bearer $TOKEN"

# Get cluster status
curl -X GET http://localhost:3901/api/v1/cluster/status \
  -H "Authorization: Bearer $TOKEN"
```

### 3. Manage Tenants (Admin Only)

```bash
# Create a new tenant
curl -X POST http://localhost:3901/api/v1/tenants \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "id": "acme",
    "name": "Acme Corporation",
    "tier": "professional"
  }'

# List all tenants
curl -X GET http://localhost:3901/api/v1/tenants \
  -H "Authorization: Bearer $TOKEN"

# Get tenant details
curl -X GET http://localhost:3901/api/v1/tenants/acme \
  -H "Authorization: Bearer $TOKEN"
```

### 4. Trigger Operations (Admin Only)

```bash
# Create snapshot
curl -X POST http://localhost:3901/api/v1/operations/snapshot \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{}'

# Trigger event replay
curl -X POST http://localhost:3901/api/v1/operations/replay \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "entity_id": "user-123",
    "as_of": "2025-01-15T10:00:00Z"
  }'
```

### 5. View Audit Log

```bash
# Tail audit log
tail -f audit.log

# View recent audit events
tail -20 audit.log | jq .

# Filter auth events
jq 'select(.event_type == "api_request" and .path | contains("auth"))' audit.log
```

---

## 🧪 Testing

### Integration Tests

Run the comprehensive integration test suite:

```bash
# Make sure both services are running:
# Terminal 1: cd services/core && cargo run
# Terminal 2: cd services/control-plane && go run main_v1.go

# Terminal 3: Run integration tests
cd services
chmod +x integration_test.sh
./integration_test.sh
```

The test suite includes:
- ✅ Pre-flight checks (services running)
- ✅ Authentication flow tests
- ✅ Multi-tenancy tests
- ✅ RBAC & permission tests
- ✅ Core service integration
- ✅ Audit & observability tests
- ✅ Policy enforcement tests
- ✅ Operation tests

---

## 📊 Monitoring & Observability

### Prometheus Metrics

The control-plane exposes Prometheus metrics at `/metrics`:

```
# HTTP request metrics
control_plane_http_requests_total
control_plane_http_request_duration_seconds
control_plane_http_requests_in_flight

# Core health check metrics
control_plane_core_health_checks_total
control_plane_core_health_check_duration_seconds

# Operation metrics
control_plane_snapshot_operations_total
control_plane_replay_operations_total

# System metrics
control_plane_uptime_seconds
```

Series labelled with a tenant are served only by the private listener on
`METRICS_ADDR` (Fly scrapes it over the private network through `[metrics]` in
`fly.toml`). It serves everything above plus:

```
control_plane_tenant_response_bytes_total{tenant,route,method,status_class}
control_plane_tenant_request_bytes_total{tenant,route,method}
control_plane_tenant_requests_total{tenant,route,method,status_class}
control_plane_tenant_event_query_body_bytes_total{tenant,namespace,selector,...}
control_plane_tenant_event_queries_total{tenant,namespace,selector,...}
control_plane_tenant_event_query_conditional_total{tenant,namespace}
admission_would_refuse_total{lane,scope,tenant}
admission_refused_total{lane,scope,tenant}
```

`tenant` is capped at 256 distinct values (later tenants report as `other`;
unauthenticated requests as `anonymous`). `route` is the matched route
template, or `unmatched`.

### Distributed Tracing (OTLP)

View distributed traces in Jaeger UI (or any OTLP-compatible backend like Grafana Tempo):

```bash
# Start Jaeger with OTLP support (Docker)
docker run -d --name jaeger \
  -p 16686:16686 \
  -p 4317:4317 \
  -p 4318:4318 \
  jaegertracing/all-in-one:latest

# Access Jaeger UI
open http://localhost:16686

# Configure control-plane with OTLP HTTP endpoint
export OTLP_ENDPOINT="localhost:4318"
```

### Audit Logs

```bash
# View all audit events
cat audit.log | jq .

# Filter by user
cat audit.log | jq 'select(.user_id == "user-123")'

# Filter by event type
cat audit.log | jq 'select(.event_type == "policy_denial")'

# Count requests by path
cat audit.log | jq -r '.path' | sort | uniq -c
```

---

## 🛡️ Security Best Practices

### Production Deployment

1. **Change JWT Secret**
   ```bash
   export JWT_SECRET="$(openssl rand -base64 32)"
   ```

2. **Enable HTTPS** (via reverse proxy like Nginx/Traefik)

3. **Rotate Secrets Regularly**

4. **Monitor Audit Logs**
   - Set up log aggregation (ELK, Splunk, etc.)
   - Alert on suspicious patterns

5. **Enable Rate Limiting** in Rust core

6. **Use Strong Passwords**
   - Minimum 12 characters
   - Argon2 hashing in Rust core

7. **Restrict Network Access**
   - Firewall rules
   - VPC/security groups

---

## 🏗️ Development

### Project Structure

```
control-plane/
├── main.go              # Original v0.1.0 (deprecated)
├── main_v1.go           # New v1.0 application entry
├── auth.go              # JWT authentication client
├── audit.go             # Audit logging system
├── tracing.go           # OpenTelemetry tracing
├── policy.go            # Policy enforcement engine
├── metrics.go           # Prometheus metrics (v0.1.0)
├── middleware.go        # Middleware (v0.1.0)
├── go.mod               # Dependencies
├── go.sum               # Checksums
├── README.md            # Original README (v0.1.0)
├── README_V1.md         # This file (v1.0)
└── package.json         # Metadata
```

### Code Style

```bash
# Format code
go fmt ./...

# Lint (requires golangci-lint)
golangci-lint run

# Vet code
go vet ./...
```

---

## 🔄 Migration from v0.1.0

### Breaking Changes
1. **Authentication Required** - All endpoints (except `/health` and `/metrics`) now require authentication
2. **New Main File** - Use `main_v1.go` instead of `main.go`
3. **Environment Variables** - New env vars for JWT secret, audit log, OTLP tracing

### Migration Steps

1. **Update Dependencies**
   ```bash
   go mod tidy
   ```

2. **Set Environment Variables**
   ```bash
   export JWT_SECRET="your-secret"
   export AUDIT_LOG_PATH="audit.log"
   ```

3. **Use New Main File**
   ```bash
   go run main_v1.go  # Instead of main.go
   ```

4. **Update Clients**
   - Add `Authorization: Bearer <token>` header
   - Obtain token via `/api/v1/auth/login`

5. **Test Integration**
   ```bash
   ./integration_test.sh
   ```

---

## 📖 Resources

- [Gin Web Framework](https://gin-gonic.com/)
- [JWT Authentication](https://jwt.io/)
- [OpenTelemetry Go](https://opentelemetry.io/docs/instrumentation/go/)
- [Jaeger Tracing](https://www.jaegertracing.io/)
- [Prometheus Monitoring](https://prometheus.io/)
- [AllSource Core v1.0](../core/V1_COMPLETE.md)

---

## 🤝 Contributing

Contributions welcome! Areas of interest:
- Unit tests for Go code (currently 0% coverage)
- Additional default policies
- Enhanced tracing context
- Grafana dashboards
- Kubernetes operator

---

## 📄 License

MIT License - see LICENSE file for details

---

## 📊 Version History

### v1.0.0 (2025-10-21)
- ✅ JWT authentication client
- ✅ RBAC enforcement
- ✅ Audit logging
- ✅ OpenTelemetry tracing
- ✅ Policy enforcement
- ✅ Permission-based routes
- ✅ 12 API endpoints
- ✅ Integration test suite

### v0.1.0 (2025-10-20)
- ✅ Basic health checks
- ✅ Prometheus metrics
- ✅ Cluster status endpoint
- ✅ Core health proxy
- ✅ Snapshot/replay operations (demo mode)

---

<div align="center">

**AllSource Control Plane v1.0** - *Enterprise orchestration*

Built with 🐹 Go | v1.0.0

Integrates with AllSource Core v1.0 (Rust)

[Documentation](./README_V1.md) | [Integration Tests](../integration_test.sh) | [Core Service](../core/)

</div>
