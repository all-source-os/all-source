# Real-Time Event Streaming via Phoenix Channels

AllSource provides real-time event streaming through [Phoenix Channels](https://hexdocs.pm/phoenix/channels.html), a multiplexed WebSocket protocol with built-in heartbeats, reconnection, and topic-based subscriptions.

## Connection

The WebSocket endpoint is served by the Query Service at `/ws`.

### JavaScript/TypeScript

Install the Phoenix client:

```bash
npm install phoenix
# or
bun add phoenix
```

Connect with a JWT token:

```typescript
import { Socket } from "phoenix";

const socket = new Socket("wss://your-query-service.example.com/ws", {
  params: { token: "your-jwt-token" },
});

socket.connect();
```

### Non-JS Clients

Phoenix Channels use a JSON-based protocol over standard WebSocket. Any WebSocket client can connect — send and receive JSON frames in the Phoenix wire format:

```
["join_ref", "ref", "topic", "event", payload]
```

See the [Phoenix Channel protocol docs](https://hexdocs.pm/phoenix/channels.html) for frame details.

## Authentication

Pass a JWT token in the socket `params`. The token must contain:

- `sub` — user ID
- `exp` — expiration timestamp
- `tenant_id` — tenant context

Tokens are validated against the `JWT_SECRET` configured on the Query Service.

## Topics

### `events:all`

Subscribe to all events across all entities.

```typescript
const channel = socket.channel("events:all", {});
channel.join();

channel.on("new_event", (event) => {
  console.log("Event received:", event);
  // { id, entity_id, event_type, payload, timestamp }
});
```

### `events:{entity_id}`

Subscribe to events for a specific entity.

```typescript
const channel = socket.channel("events:user-123", {});
channel.join();

channel.on("new_event", (event) => {
  // Only events where entity_id === "user-123"
});
```

### `events:type:{event_type}`

Subscribe to events of a specific type.

```typescript
const channel = socket.channel("events:type:order.created", {});
channel.join();

channel.on("new_event", (event) => {
  // Only events where event_type === "order.created"
});
```

### `projections:{name}`

Subscribe to projection state updates.

```typescript
const channel = socket.channel("projections:user-profile", {});
channel.join();

channel.on("state_updated", (update) => {
  console.log("Projection updated:", update);
  // { entity_id, state, version, updated_at }
});

channel.on("projection_error", (error) => {
  console.error("Projection error:", error);
  // { entity_id, error, event_id }
});

// Request current state
channel.push("get_state", { entity_id: "user-123" }).receive("ok", (state) => {
  console.log("Current state:", state);
});
```

## Message Formats

### `new_event`

```json
{
  "id": "evt_abc123",
  "entity_id": "user-456",
  "event_type": "user.updated",
  "payload": { "name": "Alice", "email": "alice@example.com" },
  "timestamp": "2025-01-15T10:30:00Z"
}
```

### `state_updated`

```json
{
  "entity_id": "user-456",
  "state": { "name": "Alice", "order_count": 42 },
  "version": 15,
  "updated_at": "2025-01-15T10:30:01Z"
}
```

## Presence

Channels include Phoenix Presence tracking. After joining, you'll receive:

- `presence_state` — initial list of connected users
- `presence_diff` — subsequent joins/leaves

Each presence entry contains `{ user_id, tenant_id, online_at }`.

## Cross-Origin (CORS)

The frontend (`www.all-source.xyz`) connects cross-origin to the Query Service (`allsource-query.fly.dev`). Both HTTP API calls and WebSocket upgrades require origin validation.

The Query Service handles both through a single `ALLOWED_ORIGINS` env var:

- **HTTP CORS** — `CORSPlug` in the endpoint pipeline adds `Access-Control-Allow-Origin`, handles preflight `OPTIONS` requests, and exposes the `x-correlation-id` response header.
- **WebSocket origin check** — Phoenix's `check_origin` validates the `Origin` header on WebSocket upgrade requests. If the origin isn't allowed, the upgrade is rejected with a 403.

### Configuration

Set `ALLOWED_ORIGINS` to a comma-separated list of allowed origin URLs:

```bash
ALLOWED_ORIGINS="https://www.all-source.xyz,https://all-source.xyz"
```

If not set, it defaults to `https://www.all-source.xyz` and `https://all-source.xyz`.

Both CORS and WebSocket origin checks read from this single value — no need to configure them separately.

### Allowed headers

The CORS configuration allows these request headers:

| Header | Purpose |
|--------|---------|
| `authorization` | JWT Bearer token |
| `content-type` | JSON request bodies |
| `x-api-key` | API key authentication |
| `x-correlation-id` | Request tracing |
| `x-requested-with` | XHR identification |

The `x-correlation-id` header is also exposed in responses for client-side tracing.

Preflight responses are cached for 24 hours (`Access-Control-Max-Age: 86400`).

### Self-hosted deployments

Set `ALLOWED_ORIGINS` to the origin(s) your frontend is served from:

```bash
ALLOWED_ORIGINS="https://app.yourdomain.com"
```

### Same-origin deployments

If your frontend and Query Service share the same domain (e.g., behind a reverse proxy), CORS and origin validation pass automatically — no configuration needed.

### Non-browser clients

Origin validation only applies to browsers. Server-side clients (Node.js, Python, Go, etc.) don't send an `Origin` header by default and are unaffected.

## Reconnection

The Phoenix JS client handles reconnection automatically with exponential backoff. Configure the backoff schedule:

```typescript
const socket = new Socket(url, {
  params: { token },
  reconnectAfterMs: (tries) => [1000, 2000, 5000, 10000][Math.min(tries - 1, 3)],
});
```

The client maintains channel subscriptions across reconnects — no manual rejoin needed.
