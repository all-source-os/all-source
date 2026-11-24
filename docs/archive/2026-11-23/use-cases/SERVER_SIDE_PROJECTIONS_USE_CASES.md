# Server-Side Projections — Use Cases

> **Related Proposal**: [SERVER_SIDE_PROJECTIONS.md](../proposals/SERVER_SIDE_PROJECTIONS.md)

---

## UC-1: Query Projected Index State

**Actor**: Client SDK (Rust apiV1, Python SDK, TypeScript SDK)
**Precondition**: Events exist in Core for entity type `index`. Projection `indices` is registered in QS.

### Current Flow (client-side fold)

1. Client calls `GET /api/events/type/index.created` — fetches all create events
2. Client calls `GET /api/events/type/index.updated` — fetches all update events
3. Client calls `GET /api/events/type/index.deleted` — fetches all delete events
4. Client groups events by `entity_id`
5. Client sorts events by timestamp per group
6. Client reduces each group into `IndexState` (name, holdings, is_deleted, etc.)
7. Client filters `is_deleted = false`
8. Client slices for pagination (can't paginate before fold)
9. Client returns `Vec<IndexState>`

**Problems**: ~60 lines of fold logic per entity type. Fetches all events including deleted entities. Can't paginate server-side. Must be reimplemented in every SDK.

### Target Flow (server-side projection)

1. Client calls `POST /api/query/projected`
   ```json
   {
     "projection": "indices",
     "filters": {"is_deleted": false, "user_id": "user-123"},
     "page": 1,
     "page_size": 20,
     "sort_by": "updated_at",
     "sort_order": "desc"
   }
   ```
2. QS looks up `indices` projection from registry
3. QS checks Core for latest snapshot per entity
4. QS fetches only events after snapshot timestamp
5. QS folds delta events using `IndexState.apply_event/2`
6. QS filters on `is_deleted = false` and `user_id`
7. QS paginates on projected results
8. Client receives `IndexState[]` directly

**Result**: Zero fold logic in client. Server-side filtering and pagination. Snapshot-aware for performance.

---

## UC-2: Real-Time Portfolio Dashboard

**Actor**: Web dashboard (Next.js)
**Precondition**: User has portfolios with holdings. Continuous projection `portfolios` is running (Phase 2).

### Flow

1. Dashboard connects via WebSocket to `events:type:portfolio`
2. Dashboard calls `GET /api/query/projected?projection=portfolios&user_id=user-123`
3. QS returns current `PortfolioState[]` from ETS (sub-millisecond)
4. When a new `portfolio.updated` event arrives:
   a. Core broadcasts via WebSocket
   b. QS `ProjectionServer(portfolios)` folds the event into ETS state
   c. QS broadcasts projected state change via PubSub
   d. Dashboard receives updated `PortfolioState` over WebSocket
5. Dashboard renders updated portfolio — no client-side fold needed

**Result**: Real-time projected state pushed to UI. No polling, no client-side event replay.

---

## UC-3: Trade History with Pagination

**Actor**: Client SDK
**Precondition**: User has thousands of trade events across many entities.

### Current Problem

Client must fetch ALL trade events to fold state, then paginate client-side. For a user with 10,000 trade events wanting page 5 of 20 items, the client downloads all 10,000 events, folds them into ~500 trade states, then returns items 80-100.

### Target Flow

1. Client calls:
   ```json
   POST /api/query/projected
   {
     "projection": "trades",
     "filters": {"user_id": "user-123", "status": "filled"},
     "page": 5,
     "page_size": 20,
     "sort_by": "executed_at",
     "sort_order": "desc"
   }
   ```
2. QS folds events (snapshot-aware) into `TradeState[]`
3. QS filters on `status = filled`
4. QS sorts by `executed_at` descending
5. QS returns items 80-100 with `total: 342`

**Result**: Client receives exactly 20 items. No over-fetching. Correct total count for pagination UI.

---

## UC-4: Soft-Delete Filtering

**Actor**: Any client querying entities with soft-delete semantics
**Precondition**: Some entities have been deleted (event `*.deleted` emitted).

### Current Problem

Core doesn't understand `is_deleted` — it's a projected field derived from fold logic. Clients must:
1. Fetch all events (including deleted entities)
2. Fold to determine `is_deleted` per entity
3. Filter out deleted entities
4. This means deleted entities still consume bandwidth and fold time

### Target Flow

1. Client includes `"is_deleted": false` in projection filters
2. QS folds events, sets `is_deleted` on projected state
3. QS filters before pagination — deleted entities excluded from count and results
4. Deleted entity events are still fetched for folding but don't appear in results

**Result**: Clean API — clients never see deleted entities unless they ask for them. Correct pagination counts.

---

## UC-5: Snapshot-Accelerated Queries

**Actor**: Any client querying an entity type with high event volume
**Precondition**: Entity has 5,000+ events. Snapshots are enabled.

### Flow

1. Client calls `POST /api/query/projected` for entity `portfolio-abc`
2. QS calls `GET /api/v1/snapshots/portfolio-abc/latest` on Core
3. Core returns snapshot:
   ```json
   {
     "state": { "name": "Growth Portfolio", "holdings": [...], "total_value": 150000 },
     "as_of": "2026-02-17T10:00:00Z",
     "event_count": 4950
   }
   ```
4. QS calls `GET /api/v1/events/query` with `after_timestamp=2026-02-17T10:00:00Z`
5. Core returns 50 events (delta since snapshot)
6. QS folds 50 events onto snapshot state (not 5,000)
7. QS returns projected state
8. Since `events_after_snapshot > 100` threshold is not met, no new snapshot is created
9. If threshold were exceeded, QS would async-create a new snapshot in Core

**Result**: Fold cost proportional to delta, not total event count. Snapshots created lazily on read.

---

## UC-6: MCP Agent State Reconstruction

**Actor**: MCP Server (Elixir)
**Precondition**: AI agent asks "show me the current state of index XYZ"

### Current Flow

MCP server's `reconstruct_state` tool fetches raw events and folds them inline.

### Target Flow

1. MCP tool calls `POST /api/query/projected`
   ```json
   {
     "projection": "indices",
     "filters": {"id": "index-xyz"}
   }
   ```
2. QS returns single `IndexState` — already folded
3. MCP tool returns structured state to the AI agent

**Result**: MCP tools become thin wrappers over projected queries. No fold logic in MCP server.

---

## UC-7: Saga Progress Tracking

**Actor**: Client monitoring long-running sagas (rebalancing, multi-step workflows)
**Precondition**: Saga has events: `saga.created`, `saga.step_completed`, `saga.failed`, `saga.completed`

### Flow

1. Client calls:
   ```json
   POST /api/query/projected
   {
     "projection": "sagas",
     "filters": {"status": "in_progress", "user_id": "user-123"}
   }
   ```
2. QS folds saga events into `SagaState`:
   ```json
   {
     "id": "saga-abc",
     "type": "rebalancing",
     "status": "in_progress",
     "steps_completed": 3,
     "steps_total": 5,
     "current_step": "execute_trades",
     "started_at": "2026-02-17T10:00:00Z",
     "last_activity": "2026-02-17T10:05:00Z"
   }
   ```
3. Client renders progress bar / status indicator

**Result**: Saga state is a first-class queryable entity, not something the client must reconstruct.

---

## UC-8: Wire Format Migration

**Actor**: All API consumers
**Precondition**: Existing clients use v1 response format.

### Current Inconsistency

| Endpoint | Format |
|----------|--------|
| `GET /api/events` | `{data, count}` |
| `GET /api/streams` | `{data, count, total}` |
| `GET /api/snapshots` | `{data, count}` (count = total, bug) |
| `GET /api/webhooks` | `{webhooks, total}` |
| `GET /api/replay` | `{replays, total}` |

### Target

All list endpoints return:
```json
{
  "data": [...],
  "count": 20,
  "total": 234,
  "page": 1,
  "page_size": 20
}
```

### Migration

1. New projected query endpoints use v2 format from day one
2. Existing endpoints accept `Accept: application/vnd.allsource.v2+json` for new format
3. Default behavior unchanged for existing clients (no breaking change)
4. Deprecate v1 format in next major version

---

## Non-Functional Requirements

| Requirement | Target |
|-------------|--------|
| Fold-on-read latency (1000 events, no snapshot) | < 100ms |
| Fold-on-read latency (1000 events, snapshot at 900) | < 10ms |
| Continuous projection read latency (Phase 2) | < 1ms |
| Projection registration | Compile-time, no runtime code injection |
| Snapshot creation overhead | Async, non-blocking to read path |
| Memory overhead per projection (ETS) | < 100MB for 100K entities |
