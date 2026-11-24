---
title: "AllSource vs Turso: Strategic Comparison & Market Positioning"
status: CURRENT
last_updated: 2026-02-12
category: roadmap
---

# AllSource vs Turso: Strategic Comparison & Market Positioning

**Status**: ✅ CURRENT (ANALYSIS)
**Date**: 2026-02-12
**Purpose**: Competitive analysis for launch positioning, messaging differentiation, and strategic planning

---

## TL;DR

| Aspect | AllSource | Turso | Strategic Insight |
|--------|-----------|-------|-------------------|
| **Core Competency** | Event Sourcing + Time-Travel | SQLite-as-a-Service + Edge | Different data models |
| **Primary Use Case** | Event streams, audit trails, CQRS | Edge databases, offline-first apps | Complementary markets |
| **Architecture** | Rust core + Parquet storage | libSQL (Rust SQLite rewrite) | Both Rust-based |
| **Unique Strength** | 469K events/sec + temporal queries | Trillion databases + native vectors | Non-overlapping |
| **Pricing Model** | Usage-based (events) | Usage-based (rows read/written) | Similar approach |
| **Target Audience** | Event-driven architectures | Edge/mobile/AI agents | Minimal overlap |
| **Threat Level** | ⚠️ Medium (AI positioning overlap) | - | Watch their AI narrative |

**Conclusion**: Turso is **not a direct competitor** but targets adjacent markets. Their "AI-native" messaging is worth monitoring. AllSource should emphasize **temporal intelligence** and **event sourcing** as differentiators Turso cannot match.

---

## What is Turso?

Turso is a cloud platform built on **libSQL**, a Rust rewrite of SQLite with added features:

- **Unlimited SQLite databases** - "billions of databases" for multi-tenant apps
- **Edge deployment** - Databases replicated globally, close to users
- **Offline-first** - Sync between device and cloud
- **Native vector search** - Built-in embeddings for AI/RAG (no extensions)
- **Browser support** - WebAssembly + OPFS for in-browser databases

**Key positioning**: "Built for the agentic future" - targeting AI agents that need local, sandboxed data stores.

---

## Feature Matrix

### Core Capabilities

| Feature | AllSource | Turso | Winner | Notes |
|---------|-----------|-------|--------|-------|
| **Event Ingestion** | 469K/sec | N/A (row-based) | AllSource | Core feature |
| **Time-Travel Queries** | ✅ Native | ❌ (point-in-time restore only) | AllSource | Reconstruct any historical state |
| **SQL Queries** | ⚠️ Via Elixir DSL | ✅ Full SQLite SQL | Turso | Standard SQL interface |
| **Vector Search** | ❌ | ✅ Native | Turso | Built-in, no extensions |
| **Offline-First** | ❌ | ✅ Native | Turso | Device sync built-in |
| **Edge Deployment** | ❌ | ✅ Global replication | Turso | Low-latency edge reads |
| **Event Sourcing** | ✅ Native | ❌ | AllSource | Immutable event log |
| **Projections/Views** | ✅ Native | ⚠️ Manual | AllSource | Materialized views from events |

### Data Model Comparison

| Aspect | AllSource | Turso | Analysis |
|--------|-----------|-------|----------|
| **Data Model** | Append-only event log | Mutable rows/tables | Fundamentally different |
| **Schema** | Schema registry + evolution | SQLite schema | AllSource more flexible |
| **Immutability** | ✅ Core principle | ❌ Standard CRUD | AllSource for audit |
| **History** | ✅ Complete event history | ⚠️ Point-in-time restore | AllSource for compliance |
| **Consistency** | ✅ Strong (single writer) | ⚠️ Eventual (edge sync) | Depends on use case |

### Storage & Performance

| Metric | AllSource | Turso | Winner |
|--------|-----------|-------|--------|
| **Write Throughput** | 469K events/sec | Not published | AllSource (likely) |
| **Query Latency** | 11.9μs p99 | <1ms (claimed) | Comparable |
| **Storage Format** | Parquet (columnar) | SQLite pages | Different optimizations |
| **Compression** | 60-80% (events) | SQLite standard | AllSource |
| **Scale Limit** | Millions of events | Trillions of databases | Different dimensions |

### Enterprise Features

| Feature | AllSource | Turso | Winner |
|---------|-----------|-------|--------|
| **Multi-Tenancy** | ✅ Native + RBAC | ✅ Database-per-tenant | Tie |
| **Authentication** | ✅ JWT + API Keys + OAuth | ✅ API tokens | Tie |
| **Audit Logging** | ✅ Immutable event log | ⚠️ 3-30 day retention | AllSource |
| **Encryption** | ✅ At-rest + transit | ✅ Per-database encryption | Tie |
| **SOC2/HIPAA** | ⚠️ Planned | ✅ Pro tier | Turso (today) |
| **SSO/SAML** | ⚠️ Planned | ✅ Pro tier | Turso (today) |

### Developer Experience

| Feature | AllSource | Turso | Winner |
|---------|-----------|-------|--------|
| **SDKs** | JS (planned) | Rust, JS, Python, Go, Java, WASM | Turso |
| **Framework Integration** | MCP (Claude/GPT) | LangChain, Rails, Laravel, Flutter | Turso |
| **Embedded Mode** | ❌ Server-only | ✅ Library + Edge + Browser | Turso |
| **Documentation** | ✅ Comprehensive | ✅ Comprehensive | Tie |
| **Open Source** | ✅ MIT | ✅ Open core (libSQL) | Tie |
| **Managed Service** | ✅ Fly.io hosted | ✅ Turso Cloud | Tie |

---

## Pricing Comparison

### AllSource (Proposed)

| Tier | Price | Events/mo | Key Features |
|------|-------|-----------|--------------|
| **Free** | $0 | 10K | Evaluation |
| **Pro** | $29/mo | 500K | Solo devs |
| **Team** | $99/mo | 5M | 5 seats, priority support |
| **Scale** | $299/mo | 50M | Growing companies |
| **Enterprise** | Custom | Unlimited | SOC2, SSO, dedicated |

**Overage**: $1 per 100K events

### Turso

| Tier | Price | Key Limits | Overage |
|------|-------|------------|---------|
| **Free** | $0 | 100 DBs, 5GB, 500M rows read | None |
| **Developer** | $4.99/mo | Unlimited DBs, 9GB, 2.5B rows | $1/B rows |
| **Scaler** | $24.92/mo | 24GB, 100B rows, Teams | $0.50/GB |
| **Pro** | $416.58/mo | 50GB, SOC2, HIPAA, SSO | $0.45/GB |
| **Enterprise** | Custom | Unlimited, dedicated | Custom |

### Pricing Analysis

| Aspect | AllSource | Turso | Insight |
|--------|-----------|-------|---------|
| **Entry Point** | $29/mo (Pro) | $4.99/mo | Turso cheaper to start |
| **Mid-Market** | $99/mo | $24.92/mo | Turso 4x cheaper |
| **Enterprise** | Custom | $416.58/mo (Pro) | AllSource needs published pricing |
| **Free Tier** | 10K events | 500M rows read | Turso more generous |
| **Overage Model** | Per event | Per row read/written | Different metrics |

**Strategic Insight**: Turso has very aggressive pricing. AllSource must compete on **value** (time-travel, event sourcing) not price.

---

## Architecture Comparison

### AllSource Architecture

```
┌─────────────────────────────────────────────┐
│            AllSource Event Store            │
├─────────────────────────────────────────────┤
│                                             │
│  Rust Core (3900)                          │
│  ├── Event ingestion (469K/sec)            │
│  ├── Parquet columnar storage              │
│  ├── Time-travel query engine              │
│  └── Multi-tenant isolation                │
│                                             │
│  Go Control Plane (3901)                   │
│  ├── Tenant/user management                │
│  ├── RBAC + policies                       │
│  └── Cluster orchestration                 │
│                                             │
│  Elixir Query Service (3902)               │
│  ├── GraphQL/REST API                      │
│  ├── Projections + pipelines               │
│  ├── Real-time subscriptions               │
│  └── MCP tools (27 AI tools)               │
│                                             │
└─────────────────────────────────────────────┘

Deployment: Server-side, Fly.io/K8s
Data Model: Append-only event log
Consistency: Strong (single region)
```

### Turso Architecture

```
┌─────────────────────────────────────────────┐
│              Turso Platform                 │
├─────────────────────────────────────────────┤
│                                             │
│  libSQL Engine (Rust SQLite rewrite)       │
│  ├── Async io_uring architecture           │
│  ├── Concurrent writes (coming)            │
│  ├── Native vector search                  │
│  └── SQLite backward compatible            │
│                                             │
│  Edge Network                              │
│  ├── Global replication                    │
│  ├── Read replicas everywhere              │
│  └── Write forwarding to primary           │
│                                             │
│  Embedded Runtime                          │
│  ├── Server deployment                     │
│  ├── Browser (WASM + OPFS)                 │
│  ├── Mobile (iOS/Android)                  │
│  └── Device sync                           │
│                                             │
│  Cloud Platform                            │
│  ├── Database management                   │
│  ├── Branching (copy-on-write)             │
│  └── Analytics + monitoring                │
│                                             │
└─────────────────────────────────────────────┘

Deployment: Everywhere (server, edge, browser, device)
Data Model: Relational tables (SQLite)
Consistency: Eventual (with sync)
```

---

## Target Market Comparison

### AllSource Primary Markets

1. **Event-Driven Architectures**
   - Financial services (trading systems, audit trails)
   - Healthcare (patient event histories)
   - Supply chain (provenance tracking)
   - Gaming (player state, replay systems)

2. **Compliance-Heavy Industries**
   - Banking (regulatory audit trails)
   - Healthcare (HIPAA event logging)
   - Government (immutable records)

3. **AI/ML Infrastructure**
   - Training data versioning
   - Model behavior auditing
   - Agent action logging (MCP integration)

### Turso Primary Markets

1. **Edge Applications**
   - Mobile apps with offline-first
   - IoT device databases
   - CDN-cached data

2. **AI Agents**
   - Per-agent local databases
   - RAG with local vector search
   - Sandboxed compute with data

3. **Multi-Tenant SaaS**
   - Database-per-tenant isolation
   - Instant database provisioning
   - Privacy-focused apps

### Market Overlap Analysis

```
                    Event Sourcing
                          ▲
                          │
            AllSource ────┼──── (Unique position)
                          │
                          │
  ─────────────────────┼─────────────────────► Edge/Offline
                          │
                          │
              SQLite ─────┼──── Turso
             (Local)      │    (AI Agents)
                          │
                          ▼
                     Traditional DB
```

**Overlap**: Minimal direct competition
**Threat Vector**: Turso's "AI-native" messaging could capture mindshare in AI infrastructure market where AllSource also competes.

---

## Competitive Threats & Opportunities

### Threats from Turso

| Threat | Severity | Mitigation |
|--------|----------|------------|
| **"AI-native" positioning** | Medium | Emphasize MCP + temporal AI |
| **Vector search included** | Low | Partner with LanceDB instead |
| **Aggressive pricing** | Medium | Compete on value, not price |
| **Better SDK coverage** | High | Prioritize JS/Python SDKs |
| **Edge deployment** | Low | Not our market |

### Opportunities Against Turso

| Opportunity | AllSource Advantage |
|-------------|---------------------|
| **Time-travel queries** | Turso can't reconstruct historical state |
| **Event sourcing** | Turso is CRUD, not event-native |
| **Audit compliance** | Immutable log vs point-in-time restore |
| **Stream processing** | Real-time pipelines vs batch SQL |
| **Performance** | 469K/sec benchmark (Turso unpublished) |

---

## Strategic Recommendations

### Messaging Differentiation

**DON'T compete on:**
- ❌ "AI-native" (Turso owns this narrative)
- ❌ Edge deployment (not our strength)
- ❌ Price (Turso is cheaper)
- ❌ SDK breadth (Turso has more)

**DO compete on:**
- ✅ **"Temporal Intelligence"** - Query any point in time
- ✅ **"Event Sourcing Made Easy"** - Not just storage, full CQRS
- ✅ **"Compliance Without Complexity"** - Immutable audit trails
- ✅ **"AI That Remembers"** - MCP tools with temporal context
- ✅ **"Performance at Scale"** - 469K events/sec (benchmark proof)

### Positioning Statement

> **AllSource** is the AI-native event store for applications that need to remember everything. While Turso gives AI agents a place to store data, AllSource gives them perfect memory—query any point in time, replay any sequence, audit any decision.

### Launch Plan Adjustments

Based on this analysis:

| Recommendation | Priority | Rationale |
|----------------|----------|-----------|
| **Lead with time-travel** | P0 | Turso can't match this |
| **Publish benchmarks** | P0 | Turso doesn't publish performance |
| **MCP tools as differentiator** | P0 | 27 tools vs Turso's generic SQL |
| **SDK parity (JS, Python)** | P0 | Close the gap |
| **Don't mention Turso** | P1 | Different market, don't validate |
| **"Event Store" not "Database"** | P1 | Avoid SQLite comparisons |
| **Compliance messaging** | P1 | Enterprise differentiator |

### Web App Changes

| Change | Priority | Rationale |
|--------|----------|-----------|
| **Hero: "Time-travel your data"** | ✅ Done | Differentiates from Turso |
| **Add benchmark section** | P0 | 469K/sec, 11.9μs - proof points |
| **MCP tools showcase** | P0 | 27 tools, Claude integration demo |
| **Use case: Audit trails** | P1 | Compliance angle Turso lacks |
| **Use case: Event replay** | P1 | Unique capability |
| **Remove generic "database" language** | P1 | We're an event store |
| **Add comparison page** | P2 | AllSource vs EventStoreDB (not Turso) |

---

## Feature Roadmap Implications

### Must-Have for Parity

| Feature | Turso Has | AllSource Status | Priority |
|---------|-----------|------------------|----------|
| **JavaScript SDK** | ✅ | Planned | P0 |
| **Python SDK** | ✅ | Planned | P0 |
| **SOC2 compliance** | ✅ Pro | Planned | P1 |
| **SSO/SAML** | ✅ Pro | Planned | P1 |

### Don't Build (Turso's Turf)

| Feature | Why Not |
|---------|---------|
| **Vector search** | Partner with LanceDB instead |
| **Edge deployment** | Not our architecture |
| **Offline-first sync** | Different data model |
| **Browser runtime** | Server-side is our strength |
| **SQLite compatibility** | We're event-native |

### Double Down (Our Advantages)

| Feature | Why |
|---------|-----|
| **Time-travel queries** | Unique differentiator |
| **Event replay** | Can't be replicated in Turso |
| **MCP tools** | AI integration advantage |
| **Stream processing** | Real-time pipelines |
| **Projections** | CQRS pattern support |

---

## Conclusion

**Turso is not a direct threat** but represents a different approach to the "AI-native data infrastructure" market:

- **Turso**: "Give every AI agent its own SQLite database"
- **AllSource**: "Give every application perfect memory and time-travel"

**Key insight**: Turso optimizes for **breadth** (trillions of small databases). AllSource optimizes for **depth** (comprehensive history of events).

**Strategic response**:
1. Don't compete on their terms (edge, offline, cheap)
2. Own the "temporal intelligence" narrative
3. Lead with benchmarks and MCP integration
4. Target event-driven architectures, not general storage

---

## Quick Reference: Elevator Pitches

**If asked "How are you different from Turso?"**

> "Turso is SQLite for edge and mobile apps. AllSource is an event store with time-travel—we don't just store your data, we remember every change so you can query any point in history. Think git for your application state, with 469K events per second performance."

**If asked "Why not just use Turso's vector search?"**

> "Turso adds vectors to SQLite. We're building temporal AI—your agents can not only search semantically but also ask 'what did this look like last week?' or 'replay the sequence of events that led here.' That's the difference between search and intelligence."

---

## Further Reading

- [AllSource vs LanceDB](./ALLSOURCE_VS_LANCEDB_COMPARISON.md) - Complementary technology analysis
- [SaaS Launch Roadmap](./SAAS_LAUNCH_ROADMAP.md) - Go-to-market strategy
- [Consolidated Roadmap](./2026-02-02_CONSOLIDATED_ROADMAP.md) - Feature priorities

---

**Document Owner**: AllSource Engineering Team
**Last Updated**: 2026-02-12
**Next Review**: Q2 2026
