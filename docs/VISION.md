# AllSource — Vision Document

> **Version**: 1.0
> **Date**: 2026-11-24
> **Current release**: v0.30.0

---

## One Sentence

AllSource is the event-sourced database that becomes the memory engine for AI agents — vectors, relationships, and temporal history in one embedded binary, no infrastructure required.

---

## Where We've Been

AllSource started as a Rust event store: append-only, durable, fast. The thesis was simple — events are the purest form of data, and an event store purpose-built in Rust could serve them without an external database.

That thesis holds. Core ingests at 469K events/sec on the reproducible benchmark in `apps/web/content/reproduce-the-469k-events-benchmark.mdx`, and serves indexed reads at 11.9μs p99 (scope in `apps/web/content/12-microsecond-agent-memory.mdx`). Storage has three tiers: DashMap for in-memory reads, WAL for durability, Parquet for archival.

Core IS the database. No external dependencies. No PostgreSQL in the event path. No Redis. Just Rust, events, and projections.

---

## Where We Are

### What Works

**AllSource Core** — the database engine:

| Capability | Status | Detail |
|-----------|--------|--------|
| Event ingestion | Production | 469K events/sec, atomic batch, single-lock |
| Queries | Production | 11.9μs p99 via DashMap, time-travel via `as_of` |
| WAL durability | Production | CRC32 checksums, configurable fsync, crash recovery |
| Parquet archival | Production | Snappy compression, periodic flush, columnar analytics |
| Projections | Production | Fold-on-read + continuous, 9 built-in templates |
| Schema registry | Production | JSON Schema validation, compatibility modes |
| Snapshots | Production | Manual + automatic, snapshot-aware folding |
| Vector search | Production | fastembed + HNSW index (feature `vector-search`) |
| Keyword search | Production | BM25 via tantivy (feature `keyword-search`) |
| EventQL | Production | SQL via DataFusion over event streams (feature `analytics`) |
| Multi-tenancy | Production | Full isolation, per-tenant quotas |
| Embedded library | Production | `EmbeddedCore` facade |
| Bidirectional sync | Production | HLC + CRDT, HTTP pull/push transport |
| Merge strategies | Production | AppendOnly, LastWriteWins, FirstWriteWins per event type |
| Replication | Leader-follower | Via WAL shipping |
| WebSocket streaming | Production | Backpressure, configurable batching |

**Full stack** — five services, four languages:

```
┌──────────────────────────────────────────────────────────────┐
│                     Dashboard (Next.js)                        │
│  Events explorer · API keys · Billing · Pipelines · Demo zone │
└──────────────────────┬───────────────────────────────────────┘
                       │
┌──────────────────────▼───────────────────────────────────────┐
│              Query Service (Elixir/Phoenix, port 3902)        │
│  API gateway · Server-side projections · WebSocket proxy      │
└───────┬──────────────────────────────────┬───────────────────┘
        │                                  │
┌───────▼───────────┐           ┌──────────▼──────────────────┐
│ Control Plane     │           │ AllSource Core (Rust)        │
│ (Go, port 3901)   │           │ (port 3900)                  │
│                   │           │                              │
│ JWT/OAuth         │           │ DashMap + WAL + Parquet      │
│ RBAC              │           │ 469K events/sec              │
│ LemonSqueezy      │           │ 11.9μs queries              │
│ OpenTelemetry     │           │ Tenant isolation            │
└───────────────────┘           └──────────────────────────────┘

MCP Server (Elixir) — AI agent interface, TOON format

SDKs: Rust (crates.io) · Go · TypeScript · Python (GitHub registry)
Docker: Compose and per-service Dockerfiles
Deploy: Docker Compose · Kubernetes/Helm · Fly.io
```

Decision records are in `docs/decisions/`.

---

## Where We're Going

### The Insight

Every AI agent memory framework today — Mem0, Zep/Graphiti, Letta, Cognee — follows the same pattern:

```
Python orchestration layer
  → LLM API call (entity extraction)      ← slow, expensive, non-deterministic
  → External graph DB (Neo4j/FalkorDB)    ← separate infrastructure
  → External vector DB (Qdrant/Pinecone)  ← yet another system
  → No offline support                    ← cloud-only
  → No event sourcing                     ← mutations destroy history
```

Three databases. Three APIs. Three failure modes. An LLM on the write path that costs money and adds latency. No audit trail. No time-travel. No offline support.

**AllSource already has every primitive these frameworks glue together** — events (temporal history), projections (materialized views that can index vectors and graph structures), HLC + CRDT (offline sync), WAL + Parquet (durability), and an embeddable library API.

What's missing is the **domain layer** — an API that speaks nodes, edges, vectors, and recall instead of raw events.

### AllSource Prime

**The unified memory engine for AI agents.**

Vectors + relationships + events in one embedded binary. No Neo4j. No Pinecone. No API keys. No Docker. One data directory.

```rust
let prime = Prime::open("~/.agent/memory").await?;

// Vectors — semantic recall
prime.embed("doc-1", "CRDTs enable conflict-free replication", vector).await?;
let similar = prime.similar("doc-1", 10).await?;

// Graph — structured relationships
let alice = prime.add_node("person", json!({"name": "Alice"})).await?;
let crdt = prime.add_node("concept", json!({"name": "CRDT"})).await?;
prime.add_edge(&alice, &crdt, "expert_in", None).await?;
let experts = prime.neighbors(&crdt, Some("expert_in"), Direction::Incoming).await?;

// Temporal — full history, time-travel
let history = prime.history(&alice).await?;                    // audit trail
let past = prime.neighbors_as_of(&alice, None, last_week).await?;  // time-travel

// Hybrid recall — all three combined
let context = prime.recall("who knows about CRDTs?", top_k=10).await?;
// → semantic similarity + graph traversal + temporal recency in one query

// Offline sync — CRDT merge with cloud
prime.sync("https://cloud.example.com").await?;
```

The key insight: vectors, graph nodes, graph edges, and domain events are **all events** at the storage layer. The differentiation happens at the **projection layer** — different projections maintain different indexes (HNSW for vectors, adjacency lists for graph, node state for current properties) over the same unified event stream.

```
                What agents need          How Prime delivers it
                ─────────────────         ─────────────────────
                Semantic recall      →    VectorIndexProjection (HNSW over events)
                Structured memory    →    AdjacencyList + ReverseIndex projections
                "When did I learn X" →    Event log (it's already there — it's an event store)
                Offline + merge      →    HLC + CRDT (built in Core)
                "Forget this"        →    Soft-delete event (append-only, auditable)
                Hybrid recall        →    Combined vector + graph + temporal query
```

**No new storage engine.** Prime is a feature flag on `allsource-core` — a `prime/` module alongside `embedded/`, using the same EventStore, WAL, Parquet, and DashMap.

### Competitive Position

```
              Vectors  Graph  Events  Temporal  Embedded  Offline   LLM-free
              ───────  ─────  ──────  ────────  ────────  ───────   ────────
Pinecone      ✓        ✗      ✗       ✗         ✗         ✗         ✓
Qdrant        ✓        ✗      ✗       ✗         ✗         ✗         ✓
LanceDB       ✓        ✗      ✗       partial   ✓         ✗         ✓
Chroma        ✓        ✗      ✗       ✗         ✓(Py)     ✗         ✓
Neo4j         plugin   ✓      ✗       ✗         ✗         ✗         ✓
Mem0          ✓        ✓      ✗       ✗         ✗         ✗         ✗
Graphiti      ✓        ✓      partial ✓         ✗         ✗         ✗
Letta         ✓        bolt   ✗       partial   ✗         ✗         ✗
───────────────────────────────────────────────────────────────────────────
Prime         ✓        ✓      ✓       ✓         ✓         ✓(CRDT)   ✓
```

**Nobody occupies the center of the Venn diagram.** Every product is strong in one silo (vectors OR graph OR events) and absent from the others. Prime is the unified engine.

The pitch:

> Chroma for vectors. Neo4j for graphs. Neither for history. **Prime for all three.**

### How Agents Adopt Prime

**Step 1** — install one binary:
```bash
cargo install allsource-prime
```

**Step 2** — add to MCP config:
```json
{
  "mcpServers": {
    "memory": {
      "command": "allsource-prime",
      "args": ["--data-dir", "~/.agent/memory"]
    }
  }
}
```

**Step 3** — the agent has persistent memory. Forever. With receipts.

Prime's MCP tools are `prime_*` (served by `apps/prime-mcp`).

No Docker. No API keys. No Pinecone bill. No Neo4j instance. Writes do not call an LLM.

---

## Roadmap

This document does not carry a roadmap. Release history is in `docs/CHANGELOG.md`.

---

## The Product Line

```
┌────────────────────────────────────────────────────────────────────┐
│                                                                    │
│                         AllSource                                  │
│                                                                    │
│  ┌────────────────────────────────────────────────────────────┐    │
│  │                                                            │    │
│  │  AllSource Core                                            │    │
│  │  The event-sourced database engine                         │    │
│  │                                                            │    │
│  │  WAL · Parquet · DashMap · Projections · Schemas ·         │    │
│  │  Snapshots · Replication · Vector Search · EventQL         │    │
│  │                                                            │    │
│  │  ┌──────────────────────────────────────────────────┐      │    │
│  │  │                                                  │      │    │
│  │  │  AllSource Prime                                 │      │    │
│  │  │  The unified memory engine for AI agents         │      │    │
│  │  │                                                  │      │    │
│  │  │  Vectors · Graph · Temporal · Hybrid Recall ·    │      │    │
│  │  │  Contradiction Detection · Memory Decay ·        │      │    │
│  │  │  Offline Sync · MCP Tools                        │      │    │
│  │  │                                                  │      │    │
│  │  └──────────────────────────────────────────────────┘      │    │
│  │                                                            │    │
│  └────────────────────────────────────────────────────────────┘    │
│                                                                    │
│  ┌──────────┐  ┌──────────────┐  ┌───────────┐  ┌────────────┐   │
│  │ Query    │  │ Control      │  │ MCP       │  │ Dashboard  │   │
│  │ Service  │  │ Plane        │  │ Server    │  │ (Next.js)  │   │
│  │ (Elixir) │  │ (Go)         │  │ (Elixir)  │  │            │   │
│  └──────────┘  └──────────────┘  └───────────┘  └────────────┘   │
│                                                                    │
│  SDKs: Rust · Go · TypeScript · Python                            │
│                                                                    │
└────────────────────────────────────────────────────────────────────┘
```

**Core** is the database. Everything else is a view, a gateway, or a domain layer on top.

**Prime** is the highest-value domain layer — it turns Core from "an event store" into "the memory engine that AI agents can't live without."

The transition from Core to Prime is a feature flag, not a rewrite. Prime is built on the same storage engine. Every vector is an event. Every graph node is an event. Every relationship is an event. The storage engine doesn't change. The projections do.

---

## Principles

These hold:

1. **Core IS the database.** No PostgreSQL in the event path. No Redis. No external dependencies for data storage. If we need a capability, we build it into Core — not by adding another database.

2. **Events are the source of truth.** Everything — vectors, graph nodes, graph edges, configuration, audit logs — is an immutable event. History is never lost. Time-travel is always possible.

3. **Projections are the read model.** Different views of the same event stream. Vector indexes, adjacency lists, node state, statistics — all projections. One ingestion path, many read paths.

4. **Embeddable first.** If it can't run as a library in a Tauri app or a CLI tool, the design is wrong. Server mode is a deployment option, not a requirement.

5. **No LLM on the write path.** Agents decide what to store. Prime stores exactly what's sent — fast, deterministic, cheap. Entity extraction is the agent's job, not the database's.

6. **Offline-first.** Build knowledge locally, sync when ready. CRDT conflict resolution means multiple agents can work independently and merge without coordination.

7. **Zero infrastructure to start.** `brew install allsource-prime` → add to MCP config → agent has persistent memory. No Docker. No API keys. No cloud account.

---

## Metrics That Matter

| Metric | Value | Source |
|--------|-------|--------|
| Event ingestion throughput | 469K events/sec (batch path) | `apps/web/content/reproduce-the-469k-events-benchmark.mdx` |
| Indexed read latency (p99) | 11.9μs | `apps/web/content/12-microsecond-agent-memory.mdx` |
| SDKs | Rust, Go, TypeScript, Python | `sdks/` |

---

## The Opportunity

Agent memory is a core primitive that every framework needs. The frameworks reviewed in the Competitive Position section combine several external stores or put an LLM on the write path.

They're all orchestration layers. They all require 2-3 external databases. They all put an LLM on the write path.

**AllSource Prime is the database itself.** One binary. One data directory. Full history. Offline sync. No API keys.

> "Stop gluing together three databases. Prime is one engine — vectors, relationships, and events — with full temporal history and offline sync. The memory engine AI agents deserve."
