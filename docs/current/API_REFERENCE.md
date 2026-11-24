---
title: "API Reference"
status: CURRENT
last_updated: 2026-11-23
category: reference
related:
  - EVENT_STORE_FEATURES.md
  - QUICK_START.md
---

# API Reference

## Overview

AllSource is a distributed event store platform consisting of three core services, each running on its own port:

| Service | Port | Technology | Purpose |
|---------|------|------------|---------|
| **Rust Core** | 3900 | Rust/Axum | Event store operations, storage, projections, schemas |
| **Go Control Plane** | 3901 | Go/Gin | Authentication, orchestration, cluster management |
| **Elixir Query Service** | 3902 | Elixir/Phoenix | Real-time queries, projections, WebSocket streaming |

All services expose RESTful APIs with JSON payloads and support WebSocket connections for real-time streaming.

---

## Rust Core API (Port 3900)

The Rust Core service is the primary event store engine, handling event ingestion, storage, querying, and projections.

### Health & Status

#### GET /health
Check service health status.

```bash
curl http://localhost:3900/health
```

**Response:**
```json
{
  "status": "healthy",
  "version": "1.0.0",
  "timestamp": "2026-02-02T12:00:00Z"
}
```

#### GET /metrics
Prometheus-format metrics endpoint.

```bash
curl http://localhost:3900/metrics
```

#### GET /api/v1/stats
Aggregated statistics.

```bash
curl http://localhost:3900/api/v1/stats
```

**Response:**
```json
{
  "total_events": 1000000,
  "total_entities": 50000,
  "total_projections": 25,
  "total_schemas": 10,
  "uptime_seconds": 86400,
  "memory_used_bytes": 536870912,
  "storage_used_bytes": 10737418240,
  "ingest_rate_per_sec": 469000.0,
  "query_rate_per_sec": 10000.0
}
```

---

### Events

#### POST /api/v1/events
Ingest a single event.

```bash
curl -X POST http://localhost:3900/api/v1/events \
  -H "Content-Type: application/json" \
  -H "X-Tenant-ID: tenant-123" \
  -d '{
    "entity_id": "user-456",
    "event_type": "user.created",
    "payload": {
      "name": "John Doe",
      "email": "john@example.com"
    },
    "metadata": {
      "source": "api",
      "correlation_id": "req-789"
    }
  }'
```

**Response:**
```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "entity_id": "user-456",
  "event_type": "user.created",
  "timestamp": "2026-02-02T12:00:00Z",
  "version": 1
}
```

#### GET /api/v1/events/query
Query events with filters.

```bash
# Query by entity ID
curl "http://localhost:3900/api/v1/events/query?entity_id=user-456"

# Query by event type
curl "http://localhost:3900/api/v1/events/query?event_type=user.created"

# Query with time range
curl "http://localhost:3900/api/v1/events/query?since=2026-01-01T00:00:00Z&until=2026-02-01T00:00:00Z"

# Query with limit
curl "http://localhost:3900/api/v1/events/query?entity_id=user-456&limit=100"
```

**Query Parameters:**
| Parameter | Type | Description |
|-----------|------|-------------|
| `entity_id` | string | Filter by entity ID |
| `event_type` | string | Filter by event type |
| `since` | datetime | Events after this timestamp |
| `until` | datetime | Events before this timestamp |
| `limit` | integer | Maximum events to return |

#### GET /api/v1/events/stream
WebSocket endpoint for real-time event streaming.

```bash
# Connect via WebSocket
wscat -c ws://localhost:3900/api/v1/events/stream
```

---

### Entity State

#### GET /api/v1/entities/{entity_id}/state
Reconstruct current entity state by replaying events.

```bash
curl http://localhost:3900/api/v1/entities/user-456/state

# Time-travel query (state as of specific time)
curl "http://localhost:3900/api/v1/entities/user-456/state?as_of=2026-01-15T00:00:00Z"
```

#### GET /api/v1/entities/{entity_id}/snapshot
Get cached entity snapshot.

```bash
curl http://localhost:3900/api/v1/entities/user-456/snapshot
```

---

### Projections

#### GET /api/v1/projections
List Core's internal projections.

```bash
curl http://localhost:3900/api/v1/projections
```

#### GET /api/v1/projections/{name}
Get a projection.

#### DELETE /api/v1/projections/{name}
Delete a projection.

#### POST /api/v1/projections/{name}/reset
Reset a projection.

#### POST /api/v1/projections/{name}/pause
Pause a projection.

#### POST /api/v1/projections/{name}/start
Start a paused projection.

---

### Schema Registry

#### POST /api/v1/schemas
Register a new schema.

```bash
curl -X POST http://localhost:3900/api/v1/schemas \
  -H "Content-Type: application/json" \
  -d '{
    "subject": "user.created",
    "schema": {
      "type": "object",
      "properties": {
        "name": {"type": "string"},
        "email": {"type": "string", "format": "email"}
      },
      "required": ["name", "email"]
    },
    "description": "Schema for user creation events"
  }'
```

#### GET /api/v1/schemas
List all schema subjects.

```bash
curl http://localhost:3900/api/v1/schemas
```

#### GET /api/v1/schemas/{subject}
Get the latest schema for a subject.

```bash
curl http://localhost:3900/api/v1/schemas/user.created
```

#### GET /api/v1/schemas/{subject}/versions
List all versions of a schema.

```bash
curl http://localhost:3900/api/v1/schemas/user.created/versions
```

#### POST /api/v1/schemas/validate
Validate an event payload against a schema.

```bash
curl -X POST http://localhost:3900/api/v1/schemas/validate \
  -H "Content-Type: application/json" \
  -d '{
    "subject": "user.created",
    "payload": {
      "name": "John Doe",
      "email": "john@example.com"
    }
  }'
```

#### PUT /api/v1/schemas/{subject}/compatibility
Set schema compatibility mode.

```bash
curl -X PUT http://localhost:3900/api/v1/schemas/user.created/compatibility \
  -H "Content-Type: application/json" \
  -d '{
    "mode": "backward"
  }'
```

**Compatibility Modes:** `none`, `backward`, `forward`, `full`

---

### Replay

#### POST /api/v1/replay
Start a replay operation.

```bash
curl -X POST http://localhost:3900/api/v1/replay \
  -H "Content-Type: application/json" \
  -d '{
    "projection_name": "user_stats",
    "from_timestamp": "2026-01-01T00:00:00Z",
    "batch_size": 1000
  }'
```

#### GET /api/v1/replay
List active replays.

```bash
curl http://localhost:3900/api/v1/replay
```

#### GET /api/v1/replay/{replay_id}
Get replay progress.

```bash
curl http://localhost:3900/api/v1/replay/replay-123
```

#### POST /api/v1/replay/{replay_id}/cancel
Cancel a replay.

```bash
curl -X POST http://localhost:3900/api/v1/replay/replay-123/cancel
```

#### DELETE /api/v1/replay/{replay_id}
Delete a replay.

```bash
curl -X DELETE http://localhost:3900/api/v1/replay/replay-123
```

---

### Pipelines

#### POST /api/v1/pipelines
Register a stream processing pipeline.

```bash
curl -X POST http://localhost:3900/api/v1/pipelines \
  -H "Content-Type: application/json" \
  -d '{
    "name": "user_activity",
    "operators": [
      {
        "type": "filter",
        "config": {
          "field": "event_type",
          "operator": "eq",
          "value": "user.activity"
        }
      },
      {
        "type": "window",
        "config": {
          "type": "tumbling",
          "size": 3600
        }
      }
    ]
  }'
```

#### GET /api/v1/pipelines
List all pipelines.

```bash
curl http://localhost:3900/api/v1/pipelines
```

#### GET /api/v1/pipelines/{pipeline_id}
Get pipeline details.

```bash
curl http://localhost:3900/api/v1/pipelines/pipeline-123
```

#### DELETE /api/v1/pipelines/{pipeline_id}
Delete a pipeline.

```bash
curl -X DELETE http://localhost:3900/api/v1/pipelines/pipeline-123
```

#### GET /api/v1/pipelines/{pipeline_id}/stats
Get pipeline statistics.

```bash
curl http://localhost:3900/api/v1/pipelines/pipeline-123/stats
```

#### PUT /api/v1/pipelines/{pipeline_id}/reset
Reset pipeline state.

```bash
curl -X PUT http://localhost:3900/api/v1/pipelines/pipeline-123/reset
```

#### GET /api/v1/pipelines/stats
Get statistics for all pipelines.

```bash
curl http://localhost:3900/api/v1/pipelines/stats
```

---

### Analytics

#### GET /api/v1/analytics/frequency
Get event frequency over time.

```bash
curl "http://localhost:3900/api/v1/analytics/frequency?window=hour&event_type=user.created"
```

**Query Parameters:**
| Parameter | Type | Description |
|-----------|------|-------------|
| `window` | string | Time window: `minute`, `hour`, `day`, `week`, `month` |
| `event_type` | string | Filter by event type |
| `entity_id` | string | Filter by entity ID |
| `since` | datetime | Start time |
| `until` | datetime | End time |

#### GET /api/v1/analytics/summary
Get statistical summary.

```bash
curl http://localhost:3900/api/v1/analytics/summary
```

#### GET /api/v1/analytics/correlation
Get event correlation analysis.

```bash
curl http://localhost:3900/api/v1/analytics/correlation
```

---

### Compaction

#### POST /api/v1/compaction/trigger
Trigger storage compaction.

```bash
curl -X POST http://localhost:3900/api/v1/compaction/trigger
```

#### GET /api/v1/compaction/stats
Get compaction statistics.

```bash
curl http://localhost:3900/api/v1/compaction/stats
```

---

## Go Control Plane API (Port 3901)

The Go Control Plane provides authentication, authorization, tenant management, and orchestration capabilities.

### Health & Status

#### GET /health
Check control plane health.

```bash
curl http://localhost:3901/health
```

#### GET /health/core
Check core service health via control plane.

```bash
curl http://localhost:3901/health/core
```

#### GET /metrics
Prometheus-format metrics.

```bash
curl http://localhost:3901/metrics
```

---

### Authentication

#### POST /api/v1/auth/login
Authenticate and receive JWT token.

```bash
curl -X POST http://localhost:3901/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{
    "username": "admin@example.com",
    "password": "secure-password"
  }'
```

**Response:**
```json
{
  "access_token": "eyJhbGciOiJIUzI1NiIs...",
  "refresh_token": "eyJhbGciOiJIUzI1NiIs...",
  "token_type": "Bearer",
  "expires_in": 3600
}
```

---

### Tenants

#### POST /api/v1/tenants
Create a new tenant.

```bash
curl -X POST http://localhost:3901/api/v1/tenants \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Acme Corp",
    "tier": "standard",
    "metadata": {
      "industry": "technology"
    }
  }'
```

#### GET /api/v1/tenants
List all tenants.

```bash
curl http://localhost:3901/api/v1/tenants \
  -H "Authorization: Bearer $TOKEN"
```

#### GET /api/v1/tenants/{tenant_id}
Get tenant details.

```bash
curl http://localhost:3901/api/v1/tenants/tenant-123 \
  -H "Authorization: Bearer $TOKEN"
```

#### PUT /api/v1/tenants/{tenant_id}
Update a tenant.

```bash
curl -X PUT http://localhost:3901/api/v1/tenants/tenant-123 \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "tier": "professional",
    "quotas": {
      "max_events_per_day": 1000000
    }
  }'
```

#### DELETE /api/v1/tenants/{tenant_id}
Delete a tenant.

```bash
curl -X DELETE http://localhost:3901/api/v1/tenants/tenant-123 \
  -H "Authorization: Bearer $TOKEN"
```

---

### Users

#### GET /api/v1/users
List users.

```bash
curl http://localhost:3901/api/v1/users \
  -H "Authorization: Bearer $TOKEN"
```

#### DELETE /api/v1/users/{user_id}
Delete a user.

```bash
curl -X DELETE http://localhost:3901/api/v1/users/user-456 \
  -H "Authorization: Bearer $TOKEN"
```

---

### Policies

#### POST /api/v1/policies
Create an access policy.

```bash
curl -X POST http://localhost:3901/api/v1/policies \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "read-only-events",
    "effect": "allow",
    "actions": ["read"],
    "resources": ["events/*"]
  }'
```

#### GET /api/v1/policies
List all policies.

```bash
curl http://localhost:3901/api/v1/policies \
  -H "Authorization: Bearer $TOKEN"
```

#### GET /api/v1/policies/{policy_id}
Get policy details.

```bash
curl http://localhost:3901/api/v1/policies/policy-123 \
  -H "Authorization: Bearer $TOKEN"
```

#### PUT /api/v1/policies/{policy_id}
Update a policy.

```bash
curl -X PUT http://localhost:3901/api/v1/policies/policy-123 \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "actions": ["read", "write"]
  }'
```

#### DELETE /api/v1/policies/{policy_id}
Delete a policy.

```bash
curl -X DELETE http://localhost:3901/api/v1/policies/policy-123 \
  -H "Authorization: Bearer $TOKEN"
```

---

### Operations

#### POST /api/v1/operations/snapshots
Coordinate snapshot creation across cluster.

```bash
curl -X POST http://localhost:3901/api/v1/operations/snapshots \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "entity_id": "user-456"
  }'
```

#### POST /api/v1/operations/replay
Coordinate replay operations across cluster.

```bash
curl -X POST http://localhost:3901/api/v1/operations/replay \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "projection_name": "user_stats"
  }'
```

#### GET /api/v1/cluster/status
Get cluster topology and status.

```bash
curl http://localhost:3901/api/v1/cluster/status \
  -H "Authorization: Bearer $TOKEN"
```

---

### Proxy to Core

All `/api/v1/*` requests (except control plane specific endpoints) are proxied to the Rust Core service with authentication validation.

```bash
# This request is proxied to Core with auth validation
curl http://localhost:3901/api/v1/events \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "entity_id": "user-456",
    "event_type": "user.created",
    "payload": {}
  }'
```

---

## Elixir Query Service API (Port 3902)

The Elixir Query Service provides real-time query capabilities, projection management, and WebSocket streaming with high concurrency.

### Health & Status

#### GET /health
Check service health.

```bash
curl http://localhost:3902/health
```

**Response:**
```json
{
  "status": "healthy",
  "service": "query-service",
  "version": "1.0.0"
}
```

---

### Events

#### GET /api/events
Query events via the Query Service.

```bash
curl "http://localhost:3902/api/events?entity_id=user-456"

# With filters
curl "http://localhost:3902/api/events?event_type=user.created&limit=100"
```

**Query Parameters:**
| Parameter | Type | Description |
|-----------|------|-------------|
| `entity_id` | string | Filter by entity ID |
| `event_type` | string | Filter by event type |
| `since` | datetime | Events after this timestamp |
| `until` | datetime | Events before this timestamp |
| `limit` | integer | Maximum events to return |

---

### Queries

#### POST /api/query
Execute a custom query.

```bash
curl -X POST http://localhost:3902/api/query \
  -H "Content-Type: application/json" \
  -d '{
    "type": "aggregate",
    "entity_id": "user-456",
    "aggregation": "count"
  }'
```

---

### Projections

#### GET /api/projections
List registered projections.

```bash
curl http://localhost:3902/api/projections
```

#### GET /api/projections/{name}
Get projection details and current state.

```bash
curl http://localhost:3902/api/projections/user_stats
```

#### GET /api/projections/{name}/state
Get projection state for all entities.

```bash
curl http://localhost:3902/api/projections/user_stats/state
```

---

### WebSocket Streaming

The Query Service exposes a WebSocket endpoint at `/ws` for real-time event streaming via Phoenix Channels.

#### Connection

Connect via WebSocket with JWT authentication:

```javascript
import {Socket} from "phoenix"

const socket = new Socket("ws://localhost:3902/ws", {
  params: {token: "your-jwt-token"}
});

socket.connect();
```

```bash
# Connect via wscat (requires token query param)
wscat -c "ws://localhost:3902/ws?token=your-jwt-token"
```

#### Event Channels

Subscribe to real-time events via channels:

**All Events** (`events:all`):
```javascript
const channel = socket.channel("events:all", {});
channel.join()
  .receive("ok", resp => console.log("Joined successfully", resp))
  .receive("error", resp => console.log("Join failed", resp));

channel.on("new_event", event => {
  console.log("Received event:", event);
  // {
  //   id: "550e8400-e29b-41d4-a716-446655440000",
  //   entity_id: "user-456",
  //   event_type: "user.created",
  //   payload: {...},
  //   timestamp: "2026-02-02T12:00:00Z"
  // }
});
```

**Entity-Specific Events** (`events:{entity_id}`):
```javascript
const channel = socket.channel("events:user-456", {});
channel.join();

// Only receives events for entity_id "user-456"
channel.on("new_event", event => {
  console.log("User event:", event);
});
```

**Event Type Subscription** (`events:type:{event_type}`):
```javascript
const channel = socket.channel("events:type:order.placed", {});
channel.join();

// Only receives events with event_type "order.placed"
channel.on("new_event", event => {
  console.log("Order placed:", event);
});
```

#### Projection Channels

Subscribe to real-time projection state updates:

```javascript
const channel = socket.channel("projections:user_stats", {});
channel.join();

// Receive state updates
channel.on("state_updated", update => {
  console.log("State updated:", update);
  // {
  //   entity_id: "user-456",
  //   state: {...},
  //   version: 42,
  //   updated_at: "2026-02-02T12:00:00Z"
  // }
});

// Receive errors
channel.on("projection_error", error => {
  console.log("Projection error:", error);
  // {
  //   entity_id: "user-456",
  //   error: "Failed to process event",
  //   event_id: "..."
  // }
});

// Request current state
channel.push("get_state", {entity_id: "user-456"})
  .receive("ok", state => console.log("Current state:", state))
  .receive("error", err => console.log("Error:", err));
```

#### Presence Tracking

All channels support Phoenix Presence for tracking connected clients:

```javascript
import {Presence} from "phoenix"

const presence = new Presence(channel);

presence.onSync(() => {
  const users = presence.list();
  console.log("Online users:", users);
});

presence.onJoin((id, current, newPres) => {
  console.log("User joined:", id);
});

presence.onLeave((id, current, leftPres) => {
  console.log("User left:", id);
});
```

#### Channel Topics Summary

| Topic Pattern | Description |
|--------------|-------------|
| `events:all` | All events from the event store |
| `events:{entity_id}` | Events for a specific entity |
| `events:type:{event_type}` | Events of a specific type |
| `projections:{name}` | Projection state updates |

#### Authentication

WebSocket connections require a valid JWT token passed as the `token` parameter:

```javascript
const socket = new Socket("ws://localhost:3902/ws", {
  params: {token: jwtToken}
});
```

Connections without a valid token are rejected.

---

## Authentication

All protected endpoints require a JWT token in the Authorization header:

```bash
curl -H "Authorization: Bearer <token>" http://localhost:3901/api/v1/tenants
```

### Token Structure

```json
{
  "sub": "user-456",
  "tenant_id": "tenant-123",
  "role": "developer",
  "exp": 1738512000,
  "iat": 1738508400,
  "iss": "allsource"
}
```

### Roles and Permissions

| Role | Permissions |
|------|-------------|
| `admin` | Full system access |
| `developer` | Read/write events, manage schemas and pipelines |
| `readonly` | Read-only access to events and metrics |
| `serviceaccount` | Programmatic access for services |

---

## Error Responses

All services return consistent error responses:

```json
{
  "error": {
    "code": "VALIDATION_ERROR",
    "message": "Invalid event type format",
    "details": {
      "field": "event_type",
      "constraint": "must be lowercase with dot notation"
    }
  }
}
```

### Common Error Codes

| Code | HTTP Status | Description |
|------|-------------|-------------|
| `VALIDATION_ERROR` | 400 | Request validation failed |
| `UNAUTHORIZED` | 401 | Missing or invalid authentication |
| `FORBIDDEN` | 403 | Insufficient permissions |
| `NOT_FOUND` | 404 | Resource not found |
| `CONFLICT` | 409 | Resource conflict (e.g., duplicate) |
| `RATE_LIMITED` | 429 | Too many requests |
| `INTERNAL_ERROR` | 500 | Internal server error |

---

## Rate Limiting

Rate limits are applied per tenant and API key:

| Tier | Events/Day | Queries/Hour | API Keys |
|------|------------|--------------|----------|
| Free | 10K | 1K | 2 |
| Standard | 1M | 100K | 10 |
| Professional | 10M | 100K | 25 |
| Enterprise | Unlimited | Unlimited | Unlimited |

When rate limited, responses include headers:

```
X-RateLimit-Limit: 1000
X-RateLimit-Remaining: 0
X-RateLimit-Reset: 1738512000
```

---

## WebSocket Protocol

### Connection

Connect to the WebSocket endpoint:

```javascript
const ws = new WebSocket('ws://localhost:3900/api/v1/events/stream');
```

### Message Format

**Incoming events:**
```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "entity_id": "user-456",
  "event_type": "user.created",
  "payload": {},
  "timestamp": "2026-02-02T12:00:00Z"
}
```

### Subscriptions (Elixir Query Service)

```json
// Subscribe
{"type": "subscribe", "topic": "events:all"}

// Unsubscribe
{"type": "unsubscribe", "topic": "events:all"}
```

---

## SDK Examples

### JavaScript/TypeScript

```typescript
const response = await fetch('http://localhost:3900/api/v1/events', {
  method: 'POST',
  headers: {
    'Content-Type': 'application/json',
    'X-Tenant-ID': 'tenant-123'
  },
  body: JSON.stringify({
    entity_id: 'user-456',
    event_type: 'user.created',
    payload: { name: 'John Doe' }
  })
});
```

### Python

```python
import requests

response = requests.post(
    'http://localhost:3900/api/v1/events',
    headers={
        'Content-Type': 'application/json',
        'X-Tenant-ID': 'tenant-123'
    },
    json={
        'entity_id': 'user-456',
        'event_type': 'user.created',
        'payload': {'name': 'John Doe'}
    }
)
```

### Go

```go
event := map[string]interface{}{
    "entity_id":  "user-456",
    "event_type": "user.created",
    "payload":    map[string]string{"name": "John Doe"},
}

body, _ := json.Marshal(event)
req, _ := http.NewRequest("POST", "http://localhost:3900/api/v1/events", bytes.NewBuffer(body))
req.Header.Set("Content-Type", "application/json")
req.Header.Set("X-Tenant-ID", "tenant-123")

client := &http.Client{}
resp, _ := client.Do(req)
```
