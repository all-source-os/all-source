# Allsource Query Service API Reference (v0.10.3)

Verified working endpoints for building the QueryServiceClient in apiV1.

## Connection

- **Query Service**: `http://localhost:3283` (maps to container port 3902)
- **Core (direct)**: `http://localhost:3280` (maps to container port 3900)
- **Auth**: Dev mode (`AUTH_DISABLED=true`) — no auth headers needed locally

## Event Schema (Writes)

Write to Core via `POST /api/v1/events` (port 3280) or QS via `POST /api/events` (port 3283):

```json
{
  "entity_id": "user-123",          // REQUIRED: aggregate/stream identifier
  "event_type": "user.created",     // REQUIRED: lowercase dot-notation
  "payload": { ... },               // REQUIRED: arbitrary JSON (can be {})
  "metadata": { ... }               // OPTIONAL: correlation IDs, source info
}
```

Response: `{"event_id": "uuid", "timestamp": "iso8601"}`

Key: **`entity_id` IS the stream ID**. There is no separate `stream_id` field.

## Read Endpoints (Query Service, port 3283)

### REST Endpoints (all verified working)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/events` | List events (with query params) |
| GET | `/api/events/:id` | Get event by UUID |
| GET | `/api/events/entity/:entity_id` | Get all events for an entity (stream) |
| GET | `/api/events/type/:event_type` | Get all events of a type |
| GET | `/api/streams` | List distinct entity_ids (streams) |
| GET | `/api/event-types` | List distinct event types |

### Query Parameters for GET /api/events

| Param | Type | Description |
|-------|------|-------------|
| `entity_id` | string | Filter by entity ID |
| `event_type` | string | Filter by event type |
| `stream_id` | string | Alias for entity_id (QS accepts both) |
| `since` | ISO-8601 | Events after timestamp |
| `until` | ISO-8601 | Events before timestamp |
| `limit` | int | Max results (default 100) |
| `offset` | int | Pagination offset |

### Query DSL (POST /api/query)

Simple filter format (**verified working**):
```json
{
  "entity_id": "user-123",
  "event_type": "user.created",
  "limit": 100,
  "offset": 0
}
```

DSL format (**fixed — predicates are flattened to Core query params**):
```json
{
  "from": "events",
  "where": {
    "op": "eq",
    "field": "event_type",
    "value": "order.placed"
  }
}
```

Compound predicates (supported fields extracted from `and`/`or` children):
```json
{
  "from": "events",
  "where": {
    "and": [
      {"op": "eq", "field": "event_type", "value": "order.placed"},
      {"op": "eq", "field": "entity_id", "value": "user-123"}
    ]
  },
  "limit": 50
}
```

**Supported DSL fields**: `entity_id`, `event_type`, `since`, `until`, `as_of`. Other fields are silently ignored (Core doesn't support arbitrary field filters).

### Response Format

All read endpoints return HAL-style responses:
```json
{
  "count": 1,
  "data": [
    {
      "id": "uuid",
      "entity_id": "user-123",
      "event_type": "user.created",
      "payload": { ... },
      "metadata": { ... },
      "tenant_id": "default",
      "timestamp": "2026-02-16T15:53:00.738Z",
      "version": 1,
      "_links": { ... }
    }
  ],
  "_links": {
    "self": { "href": "/api/events" },
    "next": { "href": "/api/events?offset=100&limit=100" }
  }
}
```

## Recommended Client Patterns for apiV1

### Primary read path: GET /api/events/entity/:entity_id
Use for: loading an entity's full event stream to rebuild domain state.
```
GET /api/events/entity/index-{uuid}
GET /api/events/entity/trade-{uuid}
GET /api/events/entity/balance-{user_id}
```

### Filtered reads: POST /api/query (simple format)
Use for: finding events across entities by type or combined filters.
```json
POST /api/query
{"event_type": "index.created", "limit": 100}
```

### Time-range reads: GET /api/events?since=...&until=...
Use for: get_positions_at_time, get_trade_at_time.
```
GET /api/events?entity_id=balance-user123&since=2026-01-01T00:00:00Z&until=2026-01-31T23:59:59Z
```

### Stream listing: GET /api/streams
Use for: get_user_indices (filter streams by prefix pattern).

## Health Check

```
GET /api/health → {"status":"healthy","version":"0.10.3","checks":{"websocket":"healthy","backend":"healthy"}}
```

## Error Format

```json
{
  "error": {
    "code": "not_found",
    "message": "Resource not found",
    "timestamp": "...",
    "correlation_id": "..."
  }
}
```

A managed onboarding key with no registration record, used after
`ALLSOURCE_LEGACY_KEY_SUNSET`, is refused with `401` and the reason nested under
`error`:

```json
{
  "error": {
    "code": "unauthorized",
    "reason": "legacy_key_sunset",
    "message": "This API key predates registered keys and stopped working on <date>. Register a client key or create a dashboard key."
  }
}
```

The gateway sends the same `reason` and `message` at the top level. See
[CLIENT_KEY_ROTATION.md](runbooks/CLIENT_KEY_ROTATION.md#legacy-key-sunset), and
[REGISTERED_CLIENT_KEYS.md](guides/REGISTERED_CLIENT_KEYS.md) for moving a
client to a registered key.

## Known Issues

1. **`/api/events/streams` and `/api/events/types` return 400**: These old paths don't exist. Use `/api/streams` and `/api/event-types` instead.
2. **DSL only supports `eq` on known fields**: The `where` clause flattens predicates to Core query params. Only `entity_id`, `event_type`, `since`, `until`, `as_of` with `eq` operator are supported. Other operators (`gt`, `contains`, etc.) are silently dropped.
