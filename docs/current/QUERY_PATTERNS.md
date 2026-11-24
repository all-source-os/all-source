# Query Patterns and Consumer Best Practices

This guide covers common query patterns, saga orchestration, entity deduplication, and best practices for building consumers on top of AllSource Chronos.

> **Architecture reminder:** AllSource Core is the database — a purpose-built Rust event store with WAL (CRC32 checksums, configurable fsync), Parquet columnar persistence, and DashMap in-memory indices. Event data is durable and survives restarts.

---

## Listing Entities by Type

Use the `event_type_prefix` filter to scope queries to a family of event types without specifying each one individually.

### Core endpoint

```
GET /api/v1/entities?event_type_prefix=index.&limit=50
```

Returns a list of distinct entities that have events matching the prefix, sorted by most recent activity:

```json
{
  "entities": [
    {
      "entity_id": "idx-sp500",
      "event_count": 12,
      "last_event_type": "index.rebalanced",
      "last_event_at": "2026-02-25T03:00:00Z"
    }
  ],
  "total": 3,
  "has_more": false
}
```

### Rust SDK

```rust
use allsource::QueryClient;

let client = QueryClient::from_env()?;

// List all index entities
let response = client.list_entities(Some("index."), Some(50), None).await?;
for entity in &response.entities {
    println!("{}: {} events", entity.entity_id, entity.event_count);
}
```

---

## Pagination

Core applies a **default limit of 100 events** on query responses. The response includes metadata to support cursor-based pagination:

| Field | Type | Description |
|-------|------|-------------|
| `count` | `usize` | Number of events in this page |
| `total_count` | `usize` | Total matching events (before limit) |
| `has_more` | `bool` | `true` if more results exist beyond this page |

### Paginating with the Rust SDK

```rust
use allsource::{QueryClient, QueryEventsParams};

let client = QueryClient::from_env()?;
let mut offset = 0;
let page_size = 50;

loop {
    let params = QueryEventsParams::new()
        .event_type_prefix("order.")
        .limit(page_size)
        .offset(offset);

    let response = client.query_events(params).await?;
    process_events(&response.events);

    if response.has_more == Some(false) || response.events.is_empty() {
        break;
    }
    offset += page_size;
}
```

### Important notes

- Always check `has_more` rather than comparing `count` to your limit — the server may return fewer results for other reasons.
- For large result sets, prefer streaming via WebSocket (`/api/v1/events/stream`) over repeated paginated queries.

---

## Payload Filtering

Server-side payload filtering lets you narrow results by top-level payload field values without downloading all events to the client.

### Core endpoint

```
GET /api/v1/events/query?event_type_prefix=trade.&payload_filter={"user_id":"alice","status":"filled"}
```

The filter is a JSON object. All key-value pairs must match (AND logic). Only top-level fields are compared.

### Rust SDK

```rust
let params = QueryEventsParams::new()
    .event_type_prefix("trade.")
    .payload_filter("user_id", "alice")
    .payload_filter("status", "filled")
    .limit(100);

let response = client.query_events(params).await?;
```

### When to use server-side vs client-side filtering

| Scenario | Recommendation |
|----------|---------------|
| Simple equality on 1-3 fields | Server-side `payload_filter` |
| Complex predicates (ranges, nested fields, regex) | Client-side after fetching |
| High-cardinality filtering with small result set | Server-side to reduce transfer |
| Aggregation across all events | Client-side after fetching (no EventQL route is registered) |

---

## Saga Orchestration

Sagas coordinate multi-step business processes by emitting events at each step. In Chronos, sagas are **self-contained event chains** — each step reads state from the event that triggered it, not from external queries.

### Pattern: Self-Contained Sagas

```
OrderPlaced → PaymentProcessed → InventoryReserved → OrderConfirmed
```

Each event carries all the data the next step needs:

```rust
// GOOD: Data flows forward through events
let order_placed = IngestEventInput {
    event_type: "order.placed".into(),
    entity_id: order_id.clone(),
    payload: json!({
        "customer_id": "cust-123",
        "items": [{"sku": "WIDGET-A", "qty": 2, "price": 29.99}],
        "total": 59.98
    }),
    metadata: None,
};
client.ingest(order_placed).await?;

// When processing PaymentProcessed, the handler already has the total
// from the triggering event — no need to query OrderPlaced again.
```

### Anti-pattern: Query-in-the-loop

```rust
// BAD: Don't read from Query Service in saga hot paths
async fn handle_payment(event: &Event) {
    // This creates a dependency on QS availability during the saga
    let order = qs_client.get("/api/events/entity/order-123").await?; // DON'T
    let total = order.payload["total"];
    process_payment(total).await?;
}
```

**Why this is wrong:**
- Creates a circular dependency (saga depends on read model)
- If the Query Service is temporarily unavailable, the saga stalls
- Adds latency to every saga step

### Saga Best Practices

1. **Pass data forward in events.** Each event should carry enough context for the next handler to proceed without external lookups. Rich events > thin events.

2. **Projections are read models only.** Projections exist to serve queries (dashboards, reports, API reads). Never use projection state to make saga decisions.

3. **Compensate on failure.** If step N fails, emit a compensation event (e.g., `payment.refunded`, `inventory.released`) rather than trying to "undo" previous events. Events are immutable.

4. **Use correlation IDs.** Store a `saga_id` or `correlation_id` in event metadata so you can trace the entire saga chain:
   ```rust
   let metadata = json!({"saga_id": saga_id, "step": 3});
   ```

5. **Idempotency.** Saga handlers must be idempotent — replaying an event should produce the same result. Use the event ID as a deduplication key.

---

## Safe Uniqueness Checks

When creating entities, you may need to enforce uniqueness (e.g., "only one index per name per user"). In an event-sourced system, this requires care.

### Pattern: Fail-Closed on Unavailability

```rust
// Before creating, check for duplicates via Core directly
let existing = client.query_events(
    QueryEventsParams::new()
        .event_type("index.created")
        .payload_filter("name", &index_name)
        .payload_filter("user_id", &user_id)
        .limit(1)
).await?;

if existing.count > 0 {
    return Err("Index with this name already exists for this user");
}

// Safe to create — Core is the source of truth
client.ingest(IngestEventInput {
    event_type: "index.created".into(),
    entity_id: new_entity_id,
    payload: json!({"name": index_name, "user_id": user_id}),
    metadata: None,
}).await?;
```

**Key principle: fail closed.** If you cannot reach Core to check for duplicates, reject the create request. Never assume uniqueness without verification.

### Why check Core, not the Query Service?

The Query Service's projections may be slightly behind Core (check `/api/health/replay` for lag). For invariant checks (uniqueness, balance constraints), always query Core directly to avoid stale-read races.

---

## Duplicate Detection and Resolution

Use the `/api/v1/entities/duplicates` endpoint to find entities that share the same payload field values.

### Core endpoint

```
GET /api/v1/entities/duplicates?event_type_prefix=index.&group_by=name,user_id
```

Response:

```json
{
  "duplicates": [
    {
      "key": {"name": "S&P 500", "user_id": "alice"},
      "entity_ids": ["idx-abc123", "idx-def456"],
      "count": 2
    }
  ],
  "total": 1,
  "has_more": false
}
```

### Rust SDK

```rust
let dupes = client.detect_duplicates(
    "index.",        // event_type_prefix (required)
    "name,user_id",  // group_by fields
    Some(50),        // limit
    None,            // offset
).await?;

for group in &dupes.duplicates {
    println!(
        "Duplicate group ({} entities): {:?} -> {:?}",
        group.count, group.key, group.entity_ids
    );
}
```

### Manual Deduplication Runbook

1. **Identify duplicates:**
   ```
   GET /api/v1/entities/duplicates?event_type_prefix=index.&group_by=name,user_id
   ```

2. **Review each group.** Determine which entity_id is the "canonical" one (e.g., the oldest, or the one with the most events).

3. **Merge or archive.** Emit a `index.merged` event on the canonical entity referencing the duplicates, then emit `index.archived` on the duplicate entities:
   ```json
   {"event_type": "index.archived", "entity_id": "idx-def456",
    "payload": {"reason": "duplicate", "merged_into": "idx-abc123"}}
   ```

4. **Update projections.** After archival events are ingested, projections will automatically reflect the new state.

5. **Prevent recurrence.** Add a uniqueness check (see previous section) to the creation flow.

---

## Agentic / MCP Consumers

When building Claude Code or MCP tool integrations that query Chronos, follow these patterns:

### Correct MCP tool patterns

```rust
// Tool: list_recent_events
let params = QueryEventsParams::new()
    .event_type_prefix("index.")
    .limit(20);

let response = client.query_events(params).await?;
// Return response.events to the LLM context
```

### Common pitfalls

1. **Don't request unlimited events.** Always set a reasonable `limit` (20-100). LLM context windows are finite and large result sets waste tokens.

2. **Use `event_type_prefix` to scope queries.** MCP tools should never scan the entire event store. Always constrain by prefix.

3. **Prefer `list_entities` over raw event queries** when the user wants "what entities exist." It returns summaries instead of raw event data.

4. **Check `has_more` before telling the user "that's all."** If `has_more` is `true`, inform the user that more results are available and offer to paginate.

5. **`QueryEventsParams` builder methods are additive.** Each `.payload_filter("key", "val")` call adds to the filter map. Calling `.event_type()` and `.event_type_prefix()` together sends both — Core will use the prefix if both are set.

### Rate limiting

The Query Service enforces per-tenant rate limits. MCP tools should handle HTTP 429 responses gracefully by waiting and retrying, not by escalating to the user immediately.
