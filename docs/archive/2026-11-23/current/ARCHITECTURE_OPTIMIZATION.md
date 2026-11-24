---
title: "Architecture Optimization Plan"
status: CURRENT
last_updated: 2026-02-02
version: "2.0"
related:
  - "../archive/2025-11-04/ARCHITECTURE_OPTIMIZATION_v1.md"
---

# Architecture Optimization Plan

> **Note**: This is the current, authoritative architecture optimization plan.
> **Supersedes**: [ARCHITECTURE_OPTIMIZATION_v1.md](../archive/2025-11-04/ARCHITECTURE_OPTIMIZATION_v1.md)
> **Key Change**: Corrected to reflect **zero PostgreSQL dependency** (was incorrectly assuming 1 instance)

## Executive Summary

After comprehensive analysis of all three services (Rust Core, Go Control Plane, Elixir Query Service), we've identified **significant duplication and optimization opportunities** that can:

- **Save 6-8 weeks** of development effort
- **Eliminate all external database dependencies** (PostgreSQL, Redis)
- **Improve performance** by 50-100x for certain operations
- **Maintain clear separation of concerns** while avoiding duplication

### Critical Discovery

**Core currently uses NO external databases:**
- ✅ In-memory DashMap (11.9 μs queries)
- ✅ Parquet files (optional persistence)
- ✅ WAL (durability)
- ❌ NO PostgreSQL (it's optional, feature-gated, not compiled/used)
- ❌ NO Redis

**This is better than initially assessed - the architecture is already optimal!**

---

## Current Storage Reality

### What's Actually Running

```
┌─────────────────────────────────────────┐
│ Rust Core Storage Stack                 │
├─────────────────────────────────────────┤
│ L1: In-Memory DashMap                   │
│     • 11.9 μs query latency             │
│     • 469K events/sec throughput        │
│     • Lock-free concurrent access       │
│                                         │
│ L2: Parquet Files (Optional)            │
│     • Columnar storage                  │
│     • Snappy compression                │
│     • Analytics queries                 │
│                                         │
│ L3: Write-Ahead Log (Always On)         │
│     • Durability guarantees             │
│     • Crash recovery                    │
│     • CRC32 checksums                   │
└─────────────────────────────────────────┘

External Dependencies: ZERO ✅
```

### PostgreSQL Status

**Code exists but NOT used:**
- File: `apps/core/src/infrastructure/repositories/postgres_*.rs`
- Status: Feature-gated behind `--features postgres`
- Default build: Does NOT include PostgreSQL
- Current deployment: Does NOT use PostgreSQL

**To enable (if needed in future):**
```bash
cargo build --features postgres
# Then update main.rs to use PostgresTenantRepository
```

---

## Key Findings

### 🚨 Critical Duplications

1. **WebSocket Streaming**
   - Core has production-ready WebSocket at `/api/v1/events/stream`
   - Query-service planned to build Phoenix Channels (2-3 weeks)
   - **Duplication**: Unnecessary rebuild

2. **Projection Storage**
   - Core has DashMap (11.9 μs reads)
   - Query-service planned separate PostgreSQL (3-4 weeks)
   - **Duplication**: Slower and more complex

3. **Caching**
   - Core's DashMap: 11.9 μs
   - Query-service planned Redis: 0.5-1ms (50-100x slower!)
   - **Duplication**: Worse performance

4. **Event Pipelines**
   - Both Core and Query-service implement same 6 operators
   - **Duplication**: Duplicate implementation

---

## Recommendations

### ❌ CANCEL: Query Service PostgreSQL/Redis (Phase 2.1)

**Original Plan:**
- Add PostgreSQL for projection persistence (3-4 weeks)
- Add Redis for caching (2-3 weeks)

**Why Cancel:**
- Core uses NO PostgreSQL (it's optional, not enabled)
- Core's DashMap (11.9 μs) >> Redis (0.5-1ms)
- Adds external dependencies unnecessarily

**Alternative:**
```elixir
# Sync projection state to Core's DashMap API
defmodule ProjectionSync do
  use GenServer

  def handle_info(:sync, %{dirty: true} = state) do
    # Save to Core's in-memory storage
    RustCoreClient.save_projection_state(
      state.projection.name,
      state.entity_id,
      state.state
    )
    {:noreply, %{state | dirty: false}}
  end
end
```

**Caching Hierarchy:**
```
L1: Core DashMap (11.9 μs) ← Source of truth
L2: Query GenServer/ETS (sub-ms) ← Local cache
L3: Core Parquet (optional) ← Cold storage
```

**Savings: 5-7 weeks development + zero external dependencies**

---

### ❌ CANCEL: Phoenix Channels WebSocket (Phase 2.2)

**Original Plan:**
- Build Phoenix Channels (2-3 weeks)
- Handle 1000+ clients

**Why Cancel:**
- Core has production WebSocket
- Tested with 1000+ clients
- Per-client filtering

**Alternative:**
```elixir
# Use WebSockex to subscribe
{:websockex, "~> 0.4"}

defmodule QueryServiceEx.CoreWebSocketClient do
  use WebSockex

  def start_link(_) do
    WebSockex.start_link(
      "ws://localhost:3900/api/v1/events/stream",
      __MODULE__,
      %{}
    )
  end

  def handle_frame({:text, json}, state) do
    event = Jason.decode!(json)
    Phoenix.PubSub.broadcast(QueryServiceEx.PubSub, "events", {:new_event, event})
    {:ok, state}
  end
end
```

**Savings: 2-3 weeks development**

---

### ✅ KEEP: Broadway Integration (Phase 2.3)

**Why Keep:**
- Adds unique value (batch processing)
- Complements Core's throughput
- OTP supervision
- Automatic backpressure

**Implementation:**
```elixir
# Production-ready polling producer
# Target: 10K events/sec
# Cursor tracking & persistence
```

**Effort: 1-2 weeks (as planned)**

---

## Performance Comparison

| Operation | Original Plan | Optimized Plan | Improvement |
|-----------|---------------|----------------|-------------|
| **Projection Read** | PostgreSQL (1-5ms) | Core DashMap (11.9 μs) | **100-400x faster** |
| **Cache Access** | Redis (0.5-1ms) | DashMap (11.9 μs) | **50-100x faster** |
| **WebSocket** | Build (2-3 weeks) | Use Core (existing) | **Reuse** |
| **Storage** | +1 PostgreSQL | 0 databases | **Zero dependencies** |

---

## Timeline Comparison

### Original Plan
- Phase 2.1: PostgreSQL/Redis (5-7 weeks)
- Phase 2.2: Phoenix Channels (2-3 weeks)
- Phase 2.3: Broadway (1-2 weeks)
- **Total: 8-12 weeks**

### Optimized Plan
- Week 1: Core Projection API (Rust)
- Week 2: WebSocket client (Elixir)
- Week 2-3: Projection sync (Elixir)
- Week 3-4: Broadway refinement (Elixir)
- **Total: 3-4 weeks**

**Time Savings: 6-8 weeks (67% reduction)**

---

## Infrastructure Comparison

### Original Assessment (INCORRECT)
```
PostgreSQL: 2 → 1 instance (50% reduction)
Redis: 1 → 0 instances (eliminated)
```

### Corrected Reality
```
PostgreSQL: 0 → 0 instances (NONE NEEDED) ✅
Redis: 0 → 0 instances (NONE NEEDED) ✅
WebSocket: 1 instance (Core only) ✅
External DBs: 0 (zero dependencies) 🎉
```

**No external database infrastructure required!**

---

## Implementation Plan

### Week 1: Core Projection API (Rust)

**Add to Core:**
```rust
// POST /api/v1/projections/:name/:entity_id/state
// GET /api/v1/projections/:name/:entity_id/state

// Store in existing DashMap (no new dependencies)
pub async fn save_projection_state(
    Path((name, entity_id)): Path<(String, String)>,
    Json(state): Json<serde_json::Value>,
) -> Result<StatusCode> {
    let key = format!("{}:{}", name, entity_id);
    PROJECTION_CACHE.insert(key, state);

    // Optionally persist to Parquet (no PostgreSQL)
    if config.persist_projections {
        parquet_storage.save_projection(&name, &entity_id, &state).await?;
    }

    Ok(StatusCode::OK)
}
```

**Deliverables:**
- [ ] Projection state API endpoints
- [ ] DashMap storage
- [ ] Optional Parquet persistence
- [ ] Tests (15+ tests)

---

### Week 2: WebSocket Integration (Elixir)

**Add to Query-Service:**
```elixir
# Subscribe to Core's WebSocket
# Broadcast to local PubSub
# Auto-reconnect logic
```

**Deliverables:**
- [ ] CoreWebSocketClient module
- [ ] PubSub integration
- [ ] Tests (15+ tests)

---

### Week 2-3: Projection State Sync (Elixir)

**Sync to Core API:**
```elixir
# Periodic sync (100ms)
# ETS local cache
# Restore from Core on restart
```

**Deliverables:**
- [ ] ProjectionSync GenServer
- [ ] ETS cache
- [ ] Tests (20+ tests)

---

### Week 3-4: Broadway Refinement (Elixir)

**Production-ready:**
```elixir
# Polling producer
# Cursor persistence
# 10K events/sec target
```

**Deliverables:**
- [ ] CoreProducer
- [ ] EventPipeline
- [ ] Tests (15+ tests)

---

## Success Metrics

### Development
- [ ] 3-4 weeks total (vs 8-12 weeks)
- [ ] Zero external database setup
- [ ] All tests passing

### Performance
- [ ] 11.9 μs projection reads
- [ ] 10K events/sec Broadway
- [ ] <100ms event delivery

### Operations
- [ ] Zero PostgreSQL instances
- [ ] Zero Redis instances
- [ ] Simple deployment (no external DBs)

---

## Risk Assessment

### Low Risk ✅
- WebSocket client (proven pattern)
- Core API endpoints (simple CRUD)
- Broadway refinement (foundation exists)

### Medium Risk ⚠️
- State sync timing (may need tuning)
- Network failures (need retry logic)

### Mitigation
1. Gradual rollout
2. Feature flags
3. GenServer fallback
4. Comprehensive monitoring

---

## Architectural Principles

### Clear Separation ✅

**Rust Core:**
- High-performance storage (DashMap, Parquet, WAL)
- WebSocket streaming
- Event ingestion (469K events/sec)

**Elixir Query-Service:**
- Concurrent processing (BEAM)
- Fault tolerance (OTP)
- Query DSL, Broadway pipelines

**Go Control-Plane:**
- Orchestration
- Health monitoring
- Multi-tenancy

### Single Source of Truth ✅

- **Events**: Core's Parquet + WAL
- **Projections**: Core's DashMap → Parquet
- **WebSocket**: Core's endpoint
- **Schemas**: Core's registry

### No External Dependencies ✅

```
Current Architecture:
├── In-memory (DashMap) ← Fast
├── Parquet (optional) ← Persistent
├── WAL (always) ← Durable
└── No databases needed! ✅
```

---

## Conclusion

The architecture review revealed that:

1. **Core uses NO PostgreSQL** (it's optional, not enabled)
2. **No Redis needed** (DashMap is 50-100x faster)
3. **WebSocket exists** (don't rebuild)
4. **Zero external dependencies** (simpler than planned)

**Result:**
- ✅ 6-8 weeks saved
- ✅ 50-100x faster caching
- ✅ Zero database ops overhead
- ✅ Simpler architecture

**Recommendation**: Approve optimized plan and begin Week 1 with Core Projection API.

