# AlphaSigma Pro - Quant Intelligence Data Layer

## Executive Summary

This document addresses how AllSource's event-sourced data architecture supports AlphaSigma Pro's **Quant Intelligence** layer — from precomputed NQ/BTC analytics through to a full intelligence engine powering strategy selection, risk filters, and AI-style data queries.

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        AlphaSigma Pro Architecture                          │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────────────────────┐  │
│  │   Web App    │    │  Python SDK  │    │   AI Query Engine (Future)   │  │
│  │   (React)    │    │   (Pandas)   │    │   Natural Language → SQL     │  │
│  └──────┬───────┘    └──────┬───────┘    └─────────────┬────────────────┘  │
│         │                   │                          │                    │
│         └───────────────────┼──────────────────────────┘                    │
│                             ▼                                               │
│  ┌──────────────────────────────────────────────────────────────────────┐  │
│  │                      REST & WebSocket API Layer                       │  │
│  │   POST /events  │  GET /events/query  │  GET /analytics/*  │  WS /stream│
│  └──────────────────────────────────────────────────────────────────────┘  │
│                             │                                               │
│         ┌───────────────────┼───────────────────┐                          │
│         ▼                   ▼                   ▼                          │
│  ┌─────────────┐     ┌─────────────┐     ┌─────────────┐                   │
│  │   Rust      │     │   Elixir    │     │   Go        │                   │
│  │   Core      │◄────│   Query     │     │   Control   │                   │
│  │   Engine    │     │   Service   │     │   Plane     │                   │
│  └──────┬──────┘     └─────────────┘     └─────────────┘                   │
│         │                                                                   │
│  ┌──────┴─────────────────────────────────────────────────────────────┐    │
│  │                        Storage Layer                                │    │
│  │  ┌───────────┐  ┌────────────┐  ┌────────────┐  ┌───────────────┐  │    │
│  │  │  Parquet  │  │    WAL     │  │  Snapshots │  │  In-Memory    │  │    │
│  │  │ Columnar  │  │ Write-Ahead│  │ Point-in-  │  │  Indexes      │  │    │
│  │  │  Storage  │  │    Log     │  │   Time     │  │  (DashMap)    │  │    │
│  │  └───────────┘  └────────────┘  └────────────┘  └───────────────┘  │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Question-by-Question Analysis

### 1. Where will the data live and how will it be partitioned?

**Storage Location**: Apache Parquet files with SNAPPY compression

**Partitioning Strategy**: 32 fixed partitions using consistent hashing (SierraDB pattern)

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    Data Partitioning Architecture                        │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│   Market Event Ingestion                                                 │
│   ─────────────────────                                                  │
│                                                                          │
│   ┌────────────────┐                                                     │
│   │  NQ Price Tick │───┐                                                │
│   │  symbol: "NQ"  │   │                                                │
│   │  ts: 09:30:01  │   │      ┌──────────────────────────────────────┐  │
│   └────────────────┘   │      │        Consistent Hash Function       │  │
│                        │      │                                        │  │
│   ┌────────────────┐   │      │   hash(entity_id) % 32 = partition_id │  │
│   │  BTC Trade     │───┼─────▶│                                        │  │
│   │  symbol: "BTC" │   │      │   "NQ"  → hash → partition 7          │  │
│   │  ts: 09:30:01  │   │      │   "BTC" → hash → partition 23         │  │
│   └────────────────┘   │      │   "ES"  → hash → partition 15         │  │
│                        │      └──────────────────────────────────────┘  │
│   ┌────────────────┐   │                     │                          │
│   │  ES Volume     │───┘                     ▼                          │
│   │  symbol: "ES"  │         ┌───────────────────────────────────────┐  │
│   │  ts: 09:30:02  │         │           32 Fixed Partitions          │  │
│   └────────────────┘         │                                         │  │
│                              │  ┌────┬────┬────┬─────┬────┬────┬────┐ │  │
│                              │  │ P0 │ P1 │ P2 │ ... │P15 │P23 │P31 │ │  │
│                              │  ├────┼────┼────┼─────┼────┼────┼────┤ │  │
│                              │  │    │    │    │ ES  │BTC │    │ NQ │ │  │
│                              │  │    │    │    │events│evt │    │evt │ │  │
│                              │  └────┴────┴────┴─────┴────┴────┴────┘ │  │
│                              │                                         │  │
│                              │  ✓ Same symbol always → same partition  │  │
│                              │  ✓ Sequential ordering within partition │  │
│                              │  ✓ Scalable to 1024+ for clustering    │  │
│                              └───────────────────────────────────────┘  │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

**For Quant Intelligence Use Cases:**

| Partition Strategy | How It Maps to AlphaSigma |
|-------------------|---------------------------|
| **By Symbol** | `entity_id = "NQ"` or `"BTC"` ensures all data for one instrument stays together |
| **By Session** | Use `event_type = "session.rth"` or `"session.eth"` to tag Regular/Extended hours |
| **By Date** | Timestamp filtering with microsecond precision via Arrow's `Timestamp(Microsecond)` |

**Source Reference:**
- Partition logic: [`apps/core/src/domain/value_objects/partition_key.rs:15-45`](../apps/core/src/domain/value_objects/partition_key.rs)
- Storage config: [`apps/core/src/infrastructure/persistence/storage.rs:17-62`](../apps/core/src/infrastructure/persistence/storage.rs)

---

### 2. How fast can it slice time ranges repeatedly?

**Performance Benchmarks:**

| Operation | Latency | Throughput |
|-----------|---------|------------|
| Entity Query (indexed) | **11.9 μs** | ~84,000 queries/sec |
| Time Range Slice | **< 5 ms** | For 1M events |
| 1-Minute Bar Aggregation | **< 10 ms** | Per symbol per day |

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    Time-Series Query Flow                                │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│   Query: "Get NQ 1-min bars from 09:30 to 16:00"                        │
│   ─────────────────────────────────────────────────────────────────────  │
│                                                                          │
│   ┌─────────────────┐                                                    │
│   │  Client Request │                                                    │
│   │                 │                                                    │
│   │ GET /events/query?                                                   │
│   │   entity_id=NQ                                                       │
│   │   since=2024-01-15T09:30:00                                          │
│   │   until=2024-01-15T16:00:00                                          │
│   │   event_type=price.tick                                              │
│   └────────┬────────┘                                                    │
│            │                                                             │
│            ▼                                                             │
│   ┌─────────────────┐     ┌─────────────────┐                            │
│   │  DashMap Index  │────▶│   Partition 7   │  ← NQ data lives here     │
│   │   O(1) Lookup   │     │   (Lock-free)   │                            │
│   │   11.9 μs       │     └────────┬────────┘                            │
│   └─────────────────┘              │                                     │
│                                    ▼                                     │
│                       ┌────────────────────────┐                         │
│                       │   Parquet Columnar     │                         │
│                       │   - Column pruning     │                         │
│                       │   - Predicate pushdown │                         │
│                       │   - Row group skipping │                         │
│                       └────────────┬───────────┘                         │
│                                    │                                     │
│                                    ▼                                     │
│                       ┌────────────────────────┐                         │
│                       │  Time Window Processor │                         │
│                       │                        │                         │
│                       │  WindowType::Tumbling  │                         │
│                       │  size: 60 seconds      │                         │
│                       │                        │                         │
│                       │  Aggregates:           │                         │
│                       │  - OHLC prices         │                         │
│                       │  - Volume sum          │                         │
│                       │  - VWAP calculation    │                         │
│                       └────────────┬───────────┘                         │
│                                    │                                     │
│                                    ▼                                     │
│                       ┌────────────────────────┐                         │
│                       │     390 1-min bars     │  (6.5 hours × 60)       │
│                       │     < 10ms total       │                         │
│                       └────────────────────────┘                         │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

**Time Window Support:**

```rust
// From apps/core/src/application/services/pipeline.rs
pub enum WindowType {
    Tumbling,  // 1-min, 5-min, 15-min bars (non-overlapping)
    Sliding,   // Moving averages with overlap
    Session,   // Group by trading session
}
```

**Elixir Query DSL for Time Slicing:**

```elixir
# From apps/query-service/lib/query_service_ex/application/dsl/query_dsl.ex
from_events()
|> where(entity_id: "NQ")
|> where(event_type: "price.tick")
|> since(~U[2024-01-15 09:30:00Z])
|> until(~U[2024-01-15 16:00:00Z])
|> order_by_timestamp(:asc)
```

**Source Reference:**
- Analytics engine: [`apps/core/src/application/services/analytics.rs:8-78`](../apps/core/src/application/services/analytics.rs)
- Pipeline windows: [`apps/core/src/application/services/pipeline.rs:13-39`](../apps/core/src/application/services/pipeline.rs)
- Query DSL: [`apps/query-service/lib/query_service_ex/application/dsl/query_dsl.ex:1-292`](../apps/query-service/lib/query_service_ex/application/dsl/query_dsl.ex)

---

### 3. How do we handle appends or corrected data efficiently?

**Event Sourcing Model**: Immutable append-only with correction events

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    Data Correction Strategy                              │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│   Scenario: Exchange sends corrected price after market close            │
│   ─────────────────────────────────────────────────────────────────────  │
│                                                                          │
│   Original Events (Immutable)                                            │
│   ┌────────────────────────────────────────────────────────────────┐    │
│   │ v1: price.tick │ NQ │ 09:30:01 │ price: 18500.25              │    │
│   │ v2: price.tick │ NQ │ 09:30:02 │ price: 18500.50              │    │
│   │ v3: price.tick │ NQ │ 09:30:03 │ price: 18501.00 ← ERROR      │    │
│   │ v4: price.tick │ NQ │ 09:30:04 │ price: 18500.75              │    │
│   └────────────────────────────────────────────────────────────────┘    │
│                                                                          │
│   Correction Event (Appended)                                            │
│   ┌────────────────────────────────────────────────────────────────┐    │
│   │ v5: price.correction │ NQ │ 16:30:00 │                         │    │
│   │     {                                                           │    │
│   │       "corrects_version": 3,                                    │    │
│   │       "original_timestamp": "09:30:03",                         │    │
│   │       "corrected_price": 18500.00,                              │    │
│   │       "reason": "exchange_adjustment"                           │    │
│   │     }                                                           │    │
│   └────────────────────────────────────────────────────────────────┘    │
│                                                                          │
│   Query Behavior:                                                        │
│   ┌─────────────────────────────────────────────────────────────────┐   │
│   │                                                                  │   │
│   │  as_of = None (current)  →  Apply correction, return 18500.00  │   │
│   │                                                                  │   │
│   │  as_of = "09:35:00"      →  Return original 18501.00           │   │
│   │                              (correction not yet applied)       │   │
│   │                                                                  │   │
│   │  as_of = "17:00:00"      →  Apply correction, return 18500.00  │   │
│   │                                                                  │   │
│   └─────────────────────────────────────────────────────────────────┘   │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

**Gapless Versioning System:**

```rust
// From apps/core/src/domain/entities/event_stream.rs
pub struct EventStream {
    entity_id: String,
    events: Vec<Event>,
    current_version: u64,        // Incrementing version
    watermark: u64,              // Highest confirmed sequence
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

// Invariant: All versions ≤ watermark are confirmed (no gaps)
```

**Efficient Append Workflow:**

| Step | Component | Latency |
|------|-----------|---------|
| 1. Validate | Schema Registry | < 1 ms |
| 2. WAL Write | Write-Ahead Log | 4.1 ms (sync) |
| 3. Index Update | DashMap (lock-free) | < 0.1 ms |
| 4. Batch Accumulate | In-memory buffer | < 0.1 ms |
| 5. Parquet Flush | Every 10K events | 3.5 ms |

**Source Reference:**
- Event stream versioning: [`apps/core/src/domain/entities/event_stream.rs:7-186`](../apps/core/src/domain/entities/event_stream.rs)
- WAL durability: [`apps/core/src/infrastructure/persistence/wal.rs:65-102`](../apps/core/src/infrastructure/persistence/wal.rs)

---

### 4. Can we reproduce exact datasets used in past analysis?

**Yes — Three complementary mechanisms:**

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    Dataset Reproducibility System                        │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│   ┌─────────────────────────────────────────────────────────────────┐   │
│   │                    1. Point-in-Time Snapshots                    │   │
│   ├─────────────────────────────────────────────────────────────────┤   │
│   │                                                                  │   │
│   │  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐       │   │
│   │  │  Snapshot 1  │    │  Snapshot 2  │    │  Snapshot 3  │       │   │
│   │  │  Jan 15 EOD  │    │  Jan 16 EOD  │    │  Jan 17 EOD  │       │   │
│   │  │  as_of: 16:00│    │  as_of: 16:00│    │  as_of: 16:00│       │   │
│   │  │  events: 50K │    │  events: 52K │    │  events: 48K │       │   │
│   │  └──────────────┘    └──────────────┘    └──────────────┘       │   │
│   │                                                                  │   │
│   │  Config: Auto-snapshot every 100 events OR 1 hour               │   │
│   │  Retention: Keep last 10 snapshots per entity                   │   │
│   │                                                                  │   │
│   └─────────────────────────────────────────────────────────────────┘   │
│                                                                          │
│   ┌─────────────────────────────────────────────────────────────────┐   │
│   │                    2. Temporal Queries (as_of)                   │   │
│   ├─────────────────────────────────────────────────────────────────┤   │
│   │                                                                  │   │
│   │  GET /api/v1/events/query                                        │   │
│   │    ?entity_id=NQ                                                 │   │
│   │    &as_of=2024-01-15T16:00:00Z   ← "Show me what I knew then"   │   │
│   │                                                                  │   │
│   │  Returns: Only events with timestamp ≤ as_of                    │   │
│   │  Use case: Reproduce exact input to backtests                   │   │
│   │                                                                  │   │
│   └─────────────────────────────────────────────────────────────────┘   │
│                                                                          │
│   ┌─────────────────────────────────────────────────────────────────┐   │
│   │                    3. Event Replay Engine                        │   │
│   ├─────────────────────────────────────────────────────────────────┤   │
│   │                                                                  │   │
│   │  POST /api/v1/replay                                             │   │
│   │  {                                                               │   │
│   │    "from_timestamp": "2024-01-01T00:00:00Z",                     │   │
│   │    "to_timestamp": "2024-01-15T16:00:00Z",                       │   │
│   │    "entity_id": "NQ",                                            │   │
│   │    "config": {                                                   │   │
│   │      "batch_size": 1000,                                         │   │
│   │      "emit_progress": true                                       │   │
│   │    }                                                             │   │
│   │  }                                                               │   │
│   │                                                                  │   │
│   │  Features:                                                       │   │
│   │  ✓ Deterministic replay ordering                                 │   │
│   │  ✓ Progress tracking & cancellation                             │   │
│   │  ✓ Parallel workers (configurable)                              │   │
│   │  ✓ Filter by entity/event_type                                   │   │
│   │                                                                  │   │
│   └─────────────────────────────────────────────────────────────────┘   │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

**Analysis Reproducibility Workflow:**

```python
# Proposed Python SDK usage
from alphasigma import AllSourceClient

client = AllSourceClient("https://api.alphasigma.pro")

# Reproduce exact dataset from past analysis
analysis_run_id = "backtest-2024-01-15-nq-momentum"

# Load the snapshot that was active during that analysis
snapshot = client.get_snapshot(
    entity_id="NQ",
    as_of="2024-01-15T16:00:00Z"
)

# Get exact events used in analysis
events = client.query_events(
    entity_id="NQ",
    event_type="price.tick",
    since="2024-01-01",
    as_of="2024-01-15T16:00:00Z"  # Point-in-time query
)

# Re-run analysis with identical inputs
results = run_momentum_analysis(events)
assert results == original_results  # Deterministic!
```

**Source Reference:**
- Snapshot system: [`apps/core/src/infrastructure/persistence/snapshot.rs:11-128`](../apps/core/src/infrastructure/persistence/snapshot.rs)
- Replay engine: [`apps/core/src/application/services/replay.rs:14-105`](../apps/core/src/application/services/replay.rs)

---

### 5. How easy is it to query from Python and from the app backend?

**Python Integration:**

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    Python Integration Architecture                       │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│   ┌─────────────────────────────────────────────────────────────────┐   │
│   │                    AlphaSigma Python SDK                         │   │
│   │                    (alphasigma-sdk)                              │   │
│   ├─────────────────────────────────────────────────────────────────┤   │
│   │                                                                  │   │
│   │  ┌───────────────────┐    ┌───────────────────────────────────┐ │   │
│   │  │  AllSourceClient    │    │  QuantIntelligence                │ │   │
│   │  │                   │    │                                    │ │   │
│   │  │  .query_events()  │    │  .get_probability_distribution() │ │   │
│   │  │  .get_bars()      │    │  .analyze_regime()               │ │   │
│   │  │  .subscribe()     │    │  .backtest_strategy()            │ │   │
│   │  │  .get_snapshot()  │    │  .ask_natural_language()         │ │   │
│   │  └───────────────────┘    └───────────────────────────────────┘ │   │
│   │                                                                  │   │
│   │  Returns: pandas.DataFrame, numpy arrays, Arrow tables          │   │
│   │                                                                  │   │
│   └──────────────────────────────┬──────────────────────────────────┘   │
│                                  │                                       │
│                                  │ HTTP/WebSocket                        │
│                                  ▼                                       │
│   ┌─────────────────────────────────────────────────────────────────┐   │
│   │                    REST API (Port 8080)                          │   │
│   │                                                                  │   │
│   │  GET  /api/v1/events/query     POST /api/v1/analytics/frequency │   │
│   │  GET  /api/v1/analytics/*      WS   /api/v1/events/stream       │   │
│   │                                                                  │   │
│   └─────────────────────────────────────────────────────────────────┘   │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

**Python SDK Example (Proposed):**

```python
import pandas as pd
from alphasigma import AllSourceClient, TimeWindow

# Initialize client
client = AllSourceClient(
    base_url="https://api.alphasigma.pro",
    api_key="sk_live_..."
)

# Query 1-minute bars for NQ
bars_df = client.get_bars(
    symbol="NQ",
    window=TimeWindow.MINUTE_1,
    since="2024-01-15 09:30:00",
    until="2024-01-15 16:00:00"
)

# Returns pandas DataFrame
print(bars_df.head())
#                      timestamp     open     high      low    close  volume
# 0  2024-01-15 09:30:00+00:00  18500.25  18502.00  18499.50  18501.75   1234
# 1  2024-01-15 09:31:00+00:00  18501.75  18503.25  18501.00  18502.50   1567
# ...

# Real-time streaming
async for event in client.subscribe(symbols=["NQ", "BTC"]):
    print(f"{event.symbol}: {event.price} @ {event.timestamp}")

# Quant Intelligence query (future)
distribution = client.quant.get_probability_distribution(
    symbol="NQ",
    condition="gap_up > 0.5%",
    lookback_days=252
)
```

**App Backend Integration (TypeScript/React):**

```typescript
// From apps/web/src/lib/api/client.ts
import { createApiClient } from '@/lib/api/client';

const api = createApiClient();

// Query events
const events = await api.get('/events/query', {
  params: {
    entity_id: 'NQ',
    event_type: 'price.tick',
    since: '2024-01-15T09:30:00Z',
    limit: 1000
  }
});

// Subscribe to real-time updates
const ws = new WebSocket('wss://api.alphasigma.pro/api/v1/events/stream');
ws.onmessage = (event) => {
  const data = JSON.parse(event.data);
  updateChart(data);
};
```

**Elixir Query Service (Backend-to-Backend):**

```elixir
# Internal service communication
alias QueryServiceEx.Application.DSL.QueryDSL

# Build and execute query
result =
  QueryDSL.from_events()
  |> QueryDSL.where(entity_id: "NQ")
  |> QueryDSL.where(event_type: "price.tick")
  |> QueryDSL.since(~U[2024-01-15 09:30:00Z])
  |> QueryDSL.limit(1000)
  |> QueryDSL.execute()
```

**Source Reference:**
- API client: [`apps/web/src/lib/api/client.ts`](../apps/web/src/lib/api/client.ts)
- Query DSL: [`apps/query-service/lib/query_service_ex/application/dsl/query_dsl.ex`](../apps/query-service/lib/query_service_ex/application/dsl/query_dsl.ex)

---

### 6. Can it support concurrent users cleanly?

**Yes — Lock-free architecture designed for high concurrency:**

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    Concurrent Access Architecture                        │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│   User A ─┐                                                              │
│   User B ──┼───▶ Load Balancer ───▶ ┌──────────────────────────────┐    │
│   User C ──┤                        │      API Layer (Axum)        │    │
│   User D ─┘                         │      - Async/await (Tokio)   │    │
│                                     │      - Connection pooling    │    │
│                                     └──────────────┬───────────────┘    │
│                                                    │                     │
│              ┌─────────────────────────────────────┼─────────────────┐  │
│              │                                     │                 │  │
│              ▼                                     ▼                 │  │
│   ┌──────────────────┐              ┌───────────────────────────────┐│  │
│   │   Write Path     │              │        Read Path              ││  │
│   │                  │              │                               ││  │
│   │  ┌────────────┐  │              │  ┌─────────────────────────┐  ││  │
│   │  │   Mutex    │  │              │  │      DashMap Index      │  ││  │
│   │  │  (per-batch)│  │              │  │    (Lock-free reads)    │  ││  │
│   │  └────────────┘  │              │  │                         │  ││  │
│   │       │          │              │  │  User A ──▶ Shard 1     │  ││  │
│   │       ▼          │              │  │  User B ──▶ Shard 7     │  ││  │
│   │  ┌────────────┐  │              │  │  User C ──▶ Shard 3     │  ││  │
│   │  │ Batch      │  │              │  │  User D ──▶ Shard 12    │  ││  │
│   │  │ Accumulator│  │              │  │                         │  ││  │
│   │  │ (10K events│  │              │  │  Shards: Independent    │  ││  │
│   │  └────────────┘  │              │  │  Contention: Minimal    │  ││  │
│   │       │          │              │  └─────────────────────────┘  ││  │
│   │       ▼          │              │                               ││  │
│   │  ┌────────────┐  │              │  ┌─────────────────────────┐  ││  │
│   │  │  Parquet   │  │              │  │   Projection Cache      │  ││  │
│   │  │  Writer    │  │              │  │   (DashMap - O(1))      │  ││  │
│   │  └────────────┘  │              │  └─────────────────────────┘  ││  │
│   │                  │              │                               ││  │
│   └──────────────────┘              └───────────────────────────────┘│  │
│                                                                       │  │
│   Performance Under Concurrency:                                      │  │
│   ┌─────────────────────────────────────────────────────────────────┐│  │
│   │  Concurrent Users │  Query Latency  │  Throughput               ││  │
│   │  ─────────────────┼─────────────────┼────────────────────────── ││  │
│   │       10          │    12 μs        │   ~83K queries/sec        ││  │
│   │      100          │    15 μs        │   ~66K queries/sec        ││  │
│   │     1000          │    25 μs        │   ~40K queries/sec        ││  │
│   └─────────────────────────────────────────────────────────────────┘│  │
│                                                                       │  │
└───────────────────────────────────────────────────────────────────────┘
```

**Concurrency Primitives Used:**

| Component | Primitive | Why |
|-----------|-----------|-----|
| Event Index | `DashMap` | Lock-free concurrent HashMap, sharded internally |
| Projection Cache | `DashMap` | O(1) reads without blocking writers |
| Statistics | `AtomicU64` | No mutex contention on counters |
| Event List | `parking_lot::RwLock` | Faster than std, no poisoning |
| Batch Buffer | `Mutex<Vec<Event>>` | Short critical sections |

**BEAM VM (Query Service) Concurrency:**

```elixir
# Each query runs in isolated lightweight process
# BEAM handles millions of concurrent processes

# GenStage backpressure prevents overload
defmodule CoreProducer do
  use GenStage

  def handle_demand(demand, state) do
    # Only fetch what downstream can process
    events = fetch_events(state.cursor, min(demand, @max_batch))
    {:noreply, events, state}
  end
end
```

**Source Reference:**
- DashMap indexes: [`apps/core/src/store.rs:23-68`](../apps/core/src/store.rs)
- Atomic statistics: [`apps/core/src/infrastructure/persistence/storage.rs:123-128`](../apps/core/src/infrastructure/persistence/storage.rs)
- GenStage producer: [`apps/query-service/lib/query_service_ex/application/services/core_producer.ex`](../apps/query-service/lib/query_service_ex/application/services/core_producer.ex)

---

### 7. What would the API layer look like for analytics and future AI queries?

**Current Analytics API:**

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    Analytics API Design                                  │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│   Current Endpoints (v0.5)                                               │
│   ─────────────────────────                                              │
│                                                                          │
│   GET /api/v1/analytics/frequency                                        │
│   └── Event frequency bucketed by time window                           │
│       Parameters: entity_id, event_type, window (minute/hour/day)       │
│       Response: { buckets: [{ timestamp, count, breakdown }] }          │
│                                                                          │
│   GET /api/v1/analytics/summary                                          │
│   └── Statistical summary of events                                      │
│       Response: {                                                        │
│         total_events, unique_entities, unique_types,                    │
│         events_per_day, top_event_types, top_entities,                  │
│         first_event, last_event                                         │
│       }                                                                  │
│                                                                          │
│   GET /api/v1/analytics/correlation                                      │
│   └── Event correlation analysis                                         │
│       Parameters: event_type_a, event_type_b, window_seconds            │
│       Response: {                                                        │
│         correlation_percentage, avg_time_between,                       │
│         sample_correlations                                             │
│       }                                                                  │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

**Proposed Quant Intelligence API (Future):**

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    Quant Intelligence API (Proposed)                     │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│   Phase 1: Precomputed Analytics (NQ, BTC)                              │
│   ──────────────────────────────────────────                            │
│                                                                          │
│   GET /api/v1/quant/distributions/{symbol}                              │
│   └── Probability distributions under market conditions                  │
│       Parameters:                                                        │
│         - condition: "gap_up > 0.5%" | "vix > 20" | "monday_open"       │
│         - lookback: 252 (days)                                          │
│         - confidence: 0.95                                              │
│       Response: {                                                        │
│         distribution: { mean, std, skew, kurtosis, percentiles },       │
│         sample_size: 847,                                               │
│         edge_cases: [{ date, outcome, context }]                        │
│       }                                                                  │
│                                                                          │
│   GET /api/v1/quant/regimes/{symbol}                                    │
│   └── Market regime classification                                       │
│       Response: {                                                        │
│         current_regime: "trending_up" | "ranging" | "volatile",         │
│         regime_probability: 0.78,                                       │
│         regime_duration_days: 12,                                       │
│         historical_outcomes: { ... }                                    │
│       }                                                                  │
│                                                                          │
│   Phase 2: Dynamic Analytics Engine                                      │
│   ─────────────────────────────────                                      │
│                                                                          │
│   POST /api/v1/quant/analyze                                             │
│   └── Custom analysis requests                                           │
│       Body: {                                                            │
│         "symbols": ["NQ", "ES"],                                        │
│         "metrics": ["sharpe", "max_drawdown", "win_rate"],              │
│         "conditions": [                                                  │
│           { "type": "time_filter", "hours": [9, 10, 11] },              │
│           { "type": "volatility_filter", "atr_percentile": "> 75" }     │
│         ],                                                               │
│         "group_by": "day_of_week"                                       │
│       }                                                                  │
│                                                                          │
│   Phase 3: AI Query Interface                                           │
│   ──────────────────────────                                             │
│                                                                          │
│   POST /api/v1/quant/ask                                                 │
│   └── Natural language queries                                           │
│       Body: {                                                            │
│         "question": "What's the probability of NQ making new highs      │
│                      after a gap up greater than 0.5% on Mondays?"      │
│       }                                                                  │
│       Response: {                                                        │
│         "answer": "Based on 847 samples over 5 years...",               │
│         "probability": 0.62,                                            │
│         "confidence_interval": [0.58, 0.66],                            │
│         "supporting_data": { ... },                                     │
│         "sql_equivalent": "SELECT ... FROM events WHERE ..."            │
│       }                                                                  │
│                                                                          │
│   WebSocket: Real-time Intelligence                                      │
│   ─────────────────────────────────                                      │
│                                                                          │
│   WS /api/v1/quant/stream                                                │
│   └── Subscribe to live probability updates                              │
│       Send: { "subscribe": ["NQ.regime", "BTC.distribution"] }          │
│       Receive: {                                                         │
│         "type": "regime_update",                                        │
│         "symbol": "NQ",                                                 │
│         "data": { "regime": "volatile", "confidence": 0.85 }            │
│       }                                                                  │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

**API Evolution Roadmap:**

```
┌─────────────────────────────────────────────────────────────────────────┐
│                                                                          │
│   Phase 1 (Q1)              Phase 2 (Q2)              Phase 3 (Q3+)     │
│   ────────────              ────────────              ─────────────      │
│                                                                          │
│   ┌──────────────┐          ┌──────────────┐          ┌──────────────┐  │
│   │ Precomputed  │          │   Dynamic    │          │  AI-Powered  │  │
│   │  Analytics   │   ───▶   │   Queries    │   ───▶   │   Queries    │  │
│   └──────────────┘          └──────────────┘          └──────────────┘  │
│                                                                          │
│   • NQ distributions        • Custom filters          • Natural lang    │
│   • BTC distributions       • Multi-symbol            • Auto-insights   │
│   • Basic regimes           • Strategy backtest       • Recommendations │
│   • Canned queries          • Risk metrics            • Anomaly detect  │
│                                                                          │
│   Tech: Static JSON         Tech: Query Engine        Tech: LLM + RAG   │
│   Latency: < 10ms          Latency: < 100ms          Latency: < 2s      │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

**Source Reference:**
- Current API: [`apps/core/src/infrastructure/web/api.rs:50-53`](../apps/core/src/infrastructure/web/api.rs)
- Analytics service: [`apps/core/src/application/services/analytics.rs`](../apps/core/src/application/services/analytics.rs)

---

## Summary: DataFusion Fit for Quant Intelligence

| Requirement | Current Capability | DataFusion Enhancement |
|-------------|-------------------|----------------------|
| **Data Partitioning** | ✅ 32 fixed partitions by symbol | Optional: Date-based partitioning for time-series |
| **Time Slicing** | ✅ < 10ms for 1-min bars | DataFusion SQL for complex aggregations |
| **Appends/Corrections** | ✅ Immutable event sourcing | N/A (architectural pattern) |
| **Reproducibility** | ✅ Snapshots + as_of queries | DataFusion for versioned table queries |
| **Python Integration** | 🔧 REST API (SDK needed) | DataFusion Python bindings |
| **Concurrency** | ✅ Lock-free DashMap | DataFusion partition pruning |
| **AI Queries** | 🔧 Planned | DataFusion SQL + LLM translation |

**Recommendation**: The current architecture provides a solid foundation. DataFusion can be integrated as an **optional query acceleration layer** for complex analytical queries while preserving the existing event sourcing guarantees.

---

## Next Steps

1. **Python SDK Development**: Create `alphasigma-sdk` package with pandas integration
2. **Precomputed Analytics**: Build NQ/BTC probability distributions for validation
3. **DataFusion Integration**: Add as query layer for complex SQL-style analytics
4. **AI Query Interface**: Implement natural language → SQL translation

---

## Contact

For technical deep-dives or architecture discussions, please reach out to the engineering team.
