# Oracle 26ai vs. AllSource: Converged Database vs. Purpose-Built Event Spine

*May 2026*

---

## The TL;DR

Oracle 26ai and AllSource are not really competitors. They are two different answers to a question most engineering teams ask once a year and get wrong every time:

> "What is the shape of our data, and how many engines do we want to run?"

Oracle's answer is **collapse every specialty engine into one converged relational database** — vectors, JSON, text, knowledge graph, spatial, all queryable in one SQL statement, all sitting on Oracle's redo/undo durability story.

AllSource's answer is **collapse the event log, the projections, the vector index, and the agent memory layer into one purpose-built engine** — built around the observation that for AI agents, audit-grade workflows, and time-travel reasoning, the natural primitive is an immutable log of what happened, not a mutable table of what is currently true.

Both approaches are defensible. Both are right for their target workload. This article is an honest comparison of where each one wins, where each one loses, and how to know which side of the line your problem lives on.

---

## What Oracle 26ai Is

Oracle 26ai is the latest evolution of Oracle's converged-database thesis. The pitch is precise:

- **Relational** is still the spine: tables, rows, ACID transactions, SQL.
- **Vector** is a first-class column type. AI Vector Search lets you store embeddings next to the row they describe and query them with `VECTOR_DISTANCE()` inside ordinary SQL.
- **JSON** is native (JSON Relational Duality keeps the document and the table in lockstep).
- **Text** search is built in.
- **Knowledge Graph** support means RDF triples and SPARQL alongside relational queries.
- **Spatial** types and operators live in the same engine.

The selling point is **cross-modal joins in a single transaction**: find the 10 customers whose embedded support tickets are closest to a given query vector, *and* whose accounts are in the western region polygon, *and* who have an open invoice over $50K, *and* whose CRM JSON document marks them as enterprise tier — in one SQL statement, with consistent snapshot isolation.

For organizations already running Oracle, this is genuinely powerful. It replaces five specialty engines (Postgres + Elasticsearch + Neo4j + pgvector + PostGIS) with one. The operational tax of running fewer engines is real money.

**Stack:** Proprietary, Oracle Database 26ai (formerly 23ai re-branded with stronger AI positioning), commercial license, runs in Oracle Cloud Infrastructure, on-prem, or Exadata.

---

## What AllSource Is

AllSource is a purpose-built event store and AI memory layer. One engine, three modalities:

1. **Events.** Every state change is an immutable, timestamped event in a durable WAL + Parquet store. Append-only. Nothing is ever silently overwritten. The event log *is* the source of truth — projections, snapshots, and indexes are derived views.
2. **Projections.** Materialized views computed incrementally from the event stream. Entity state, counters, timelines, snapshots, knowledge graphs, vector indexes — all are projections. Rebuildable from events. Core indexed reads measure 11.9 μs p99 (benchmark of indexed reads, not vector or graph recall).
3. **Vectors + Graph + Recall (Prime).** Semantic search via fastembed, indexed as a projection over the event stream. Temporal graph relationships with validity windows. Agent memory designed for the workload of an LLM repeatedly asking "what do I know about X, and when did I know it?"

**Stack:** Rust Core (469K events/sec ingest, 11.9μs queries, WAL with CRC32 checksums, Parquet with Snappy compression, DashMap for hot reads). Elixir Query Service as API gateway (auth, tenancy, billing). SDKs in Rust, Go, Python, TypeScript. Core is Apache 2.0; designated enterprise features use BSL 1.1. Self-hostable. Embedded or service mode from the same binary.

---

## Where They Actually Overlap

Despite the marketing, the overlap is narrow:

| Capability | Oracle 26ai | AllSource |
|---|---|---|
| Vector search | Yes — as one column type | Yes — as a projection over events |
| Time-travel queries | Flashback (limited retention window, transaction log) | First-class via event replay (`as_of`, unbounded) |
| Append-only history | Possible via temporal tables / audit | Native — the only way data exists |
| Cross-modal SQL joins | Yes — the headline feature | No — different question entirely |
| Knowledge graph | RDF / SPARQL | Projection-based, schema-less |

That's it. Beyond vector search and a loose notion of "time-aware queries," these systems answer different questions.

---

## Head-to-Head

### Where Oracle 26ai Wins

| Dimension | Oracle 26ai | AllSource |
|---|---|---|
| **SQL across modalities in one query** | Native — vector + spatial + JSON + relational in one statement | Not the model; compose via projections |
| **Existing enterprise estate** | Drop-in for Oracle shops | Net-new infrastructure |
| **Transactional integrity across types** | Full ACID across all data types | ACID per event, not across modalities |
| **Mature tooling** | 40+ years of DBAs, monitoring, replication, backup | Younger ecosystem |
| **Compliance certifications** | Every certification you can name | Self-managed; you bring compliance |
| **Spatial-first workloads** | PostGIS-class capability built in | Not a target use case |
| **Mixed OLTP + analytic** | Real Application Clusters, Exadata, columnar caches | Event sourcing is read-optimized differently |

If you have a billing system, a CRM, and an inventory database that all need to participate in the same transaction as a vector search, Oracle 26ai is the right shape of answer. There is no AllSource equivalent because **AllSource isn't trying to be that.**

### Where AllSource Wins

| Dimension | Oracle 26ai | AllSource |
|---|---|---|
| **Event-sourced primitive** | Tables with optional audit | Immutable log is the foundation |
| **Audit-grade provenance** | Possible with effort (temporal tables, audit triggers) | Free — every change is an event with metadata |
| **Time-travel (unbounded)** | Flashback within retention window | Replay from any point in history |
| **Agent memory workload** | Generic vector + SQL | Purpose-built recall API (Prime) |
| **Ingest throughput** | OLTP-tuned, not throughput-tuned | 469K events/sec sustained |
| **Query latency (hot path)** | Sub-ms with Exadata flash cache | 11.9μs DashMap reads |
| **License + cost model** | Proprietary, per-core licensing | Core Apache 2.0, self-host or hosted |
| **Operational footprint** | Heavy — full DBA story | Single Rust binary, embedded or service |
| **Developer ergonomics** | SQL + PL/SQL | Event SDK in 4 languages |
| **Embedded mode** | No | Yes — same binary runs in-process |
| **Replay / rebuild projections** | Re-derive from temporal tables, with caveats | First-class operation |
| **Multi-agent CRDT sync** | Not in scope | Native (offline + sync) |

### The Event-Sourcing Gap Is the Real Story

Oracle 26ai treats history as an *afterthought*: temporal tables, audit triggers, Flashback. You can reconstruct the past, but it's bolted onto a model that fundamentally cares about "the current state."

AllSource treats history as *the only thing that's real*. The current state is a projection — derived, rebuildable, disposable. The event log is the truth.

This matters for three workloads where Oracle's model fights you:

1. **AI agent memory.** "What did the agent know last Tuesday?" is a first-class query in AllSource. In Oracle, you can approximate it with Flashback inside the retention window, but it isn't the natural shape of the data.
2. **Compliance and incident reconstruction.** Regulatory questions like "show me the exact sequence of decisions that led to this trade" are trivial against an event log. Against a mutable table — even one with audit triggers — you're stitching together inferred history.
3. **Workflow state.** Long-running agents, multi-step pipelines, sagas. The event log *is* the workflow. In a relational model, the workflow is implicit in row updates.

### The Workload Question

Oracle 26ai is built for **mixed OLTP + analytic + multi-modal queries on the current state of the world**, with optional history.

AllSource is built for **append-only event ingest with projections and recall**, where the current state is always derived.

If your data naturally wants to be answered with "what is the state of X right now?", Oracle is shaped right. If your data naturally wants to be answered with "what happened, in what order, and what does that imply now?", AllSource is shaped right.

---

## Cost and Operational Footprint

A real comparison has to include cost:

| | Oracle 26ai | AllSource |
|---|---|---|
| License | Per-core, commercial | Core Apache 2.0; enterprise features BSL 1.1 |
| Minimum viable deployment | Oracle Database server + license | One Rust binary |
| Operational team needed | DBA + Oracle expertise | Standard Rust ops |
| Managed offering | Oracle Cloud / Autonomous | Hosted AllSource live: Indie £18.99/mo, Studio £78.99, Scale £298.99 |
| Lock-in profile | High (SQL dialect, PL/SQL, Oracle-specific features) | Low (events are portable, SDKs are open) |

For an enterprise that already pays Oracle's bill, 26ai is "more value from the existing line item." For a greenfield AI-native team, AllSource is "the right primitive without the enterprise tax."

---

## When You'd Pick Which

**Pick Oracle 26ai if:**

- You're an Oracle shop and consolidating engines is your KPI.
- You need ACID transactions that span relational, JSON, vector, and spatial in one statement.
- Your compliance regime explicitly asks for Oracle.
- Your workload is OLTP-shaped with vector search bolted on, not the other way around.
- You have an in-house DBA team and the operational maturity to run Oracle well.

**Pick AllSource if:**

- Your system of record naturally wants to be *what happened, in order* — agent memory, audit trails, financial events, IoT telemetry, workflow state.
- You need unbounded time-travel, not "Flashback within retention."
- You're building AI agents that need durable, temporal, provable memory.
- You want event-sourcing as the primitive, with projections derived from the log.
- You want to embed the database in your binary or run it as a service from the same code.
- Core Apache 2.0 and self-hosting matter to your business model.

**Run them side by side if:**

- Your OLTP system of record is Oracle (customers, accounts, inventory) and your event spine is AllSource (everything that happened to those entities, plus agent reasoning over them).
- This is the most common real-world deployment shape. Oracle answers "what is true now?" AllSource answers "what happened, what do we know, and what should the agent do next?"

---

## What Oracle Has That We Don't Have Yet

Honest gaps in AllSource today:

1. **Spatial.** We don't have first-class spatial types or operators. If your workload is GIS-heavy, Oracle is the better answer.
2. **SQL across modalities.** AllSource doesn't speak SQL. Events are queried via the SDK or REST API. For analysts who live in SQL, this is friction.
3. **Compliance certifications.** Oracle has every cert. AllSource customers bring their own compliance posture for now.
4. **Mature tooling ecosystem.** Oracle has 40 years of monitoring, backup, replication, query tuning. AllSource has Prometheus metrics, structured logs, and the basics. The gap closes every quarter but it's real.

We're not trying to close all of these. Some of them are tradeoffs that come from being purpose-built. Trying to be Oracle would make AllSource worse at what it's actually for.

---

## What We Have That Oracle Doesn't (And Won't)

1. **Event sourcing as the foundation.** Not bolted on. The log is the data.
2. **11.9μs p99 indexed reads** from Core's DashMap-backed index (see the README benchmarks).
3. **Embedded mode.** Same binary, in-process. No network hop. Critical for agent toolchains.
4. **Core Apache 2.0 + self-hostable.** No per-core license, no vendor lock-in.
5. **Prime.** A recall API designed for LLM agents — compressed index + vectors + temporal graph, all derived from events. Not a generic vector column. A purpose-built memory layer.
6. **CRDT sync for offline agents.** Multi-agent, multi-device, eventual consistency that converges deterministically.

These aren't features Oracle is going to ship next quarter. They're consequences of the underlying model.

---

## The Honest Summary

Oracle 26ai is an excellent answer to **"we run too many databases and want them to talk to each other in one transaction."** That's a real, important problem and Oracle solves it well.

AllSource is an excellent answer to **"we need an event spine that AI agents and audit-critical workflows can reason over, with temporal queries and vector recall as first-class operations."** That's a different problem and Oracle is not shaped to solve it.

If you read about Oracle 26ai and your reaction is *"finally, one engine for all my modalities"* — Oracle is for you. If your reaction is *"that's interesting but my real problem is the agent forgot what happened yesterday"* — AllSource is for you.

The "vector search" overlap in the marketing materials hides a much deeper difference in worldview. Oracle's world is **state**. AllSource's world is **events**. Most engineering teams need both — and the smart play is to know which one belongs at which layer of the stack.

---

*Sources: [Oracle Database 26ai](https://www.oracle.com/database/) | [AllSource Core](https://github.com/all-source-os/all-source) | [AllSource Prime Proposal](../proposals/archived/ALLSOURCE_PRIME.md) | [Core Replication Design](../proposals/archived/CORE_REPLICATION_DESIGN.md)*
