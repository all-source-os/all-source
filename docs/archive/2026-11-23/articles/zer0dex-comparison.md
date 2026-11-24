# zer0dex vs. AllSource: What Agent Memory Actually Needs

*March 2026*

---

## Credit Where It's Due

[zer0dex](https://github.com/roli-lpci/zer0dex) from Hermes Labs' LPCI research nails an insight that the rest of the agent memory space has overlooked: **a structured index on top of vector search dramatically improves cross-domain retrieval.** Their dual-layer approach — a compressed markdown file (~782 tokens) acting as a semantic table of contents, plus a ChromaDB vector store with automatic pre-message injection — achieves 80% cross-reference accuracy where pure vector RAG scores 37.5%.

That's not a marginal improvement. That's a 2x gain on the hardest memory retrieval problem: "How does X relate to Y?"

We respect the work. This article is an honest comparison — where zer0dex wins today, where AllSource wins, and what we're building to combine the best of both.

---

## The Two Systems

### zer0dex: Elegant Minimalism

Two layers, one job:

1. **Compressed Index (MEMORY.md):** Human-authored markdown (~3KB) with cross-domain pointers. Always loaded in context. The agent knows *where* knowledge lives before it searches.
2. **Vector Store (mem0 + ChromaDB):** Semantic fact database queried on every message (70ms). Top 5 results auto-injected into context.

**Stack:** Python 3.11+, Ollama (nomic-embed-text + Mistral 7B), ChromaDB. Fully local. Apache 2.0.

### AllSource: Event-Sourced Memory Engine

One engine, three modalities:

1. **Events:** Every memory mutation is an immutable, timestamped event in a durable WAL + Parquet store. Full audit trail. Nothing is ever silently overwritten.
2. **Vectors:** Semantic search via fastembed, indexed as projections over the event stream. Same durability guarantees as events.
3. **Projections:** Materialized views computed incrementally from events — entity state, counters, timelines, snapshots. Queryable in ~12μs.

**Stack:** Rust core (469K events/sec, 11.9μs queries), Elixir API gateway, SDKs in Rust/Go/Python/TypeScript. WAL + Parquet + DashMap. Apache 2.0.

---

## Head-to-Head

### Where zer0dex Wins Today

| Dimension | zer0dex | AllSource |
|-----------|---------|-----------|
| **Time to first memory** | `pip install` + seed + start | Deploy Core + Query Service (or embed in Rust) |
| **Cross-domain recall** | 80% (compressed index bridges domains) | No equivalent navigational layer yet |
| **Token overhead** | 886 tokens/message | Variable (depends on query strategy) |
| **Simplicity** | 2 components, 1 HTTP endpoint | Full event-sourced stack |
| **Python-native** | Yes, pip install | SDKs available but Core is Rust |

**The compressed index is zer0dex's real contribution.** When you ask "How does our pricing model relate to the Q3 churn spike?", vector similarity finds pricing docs OR churn docs — rarely both. The markdown index has explicit cross-domain pointers that bridge this gap. It's simple, it's cheap (782 tokens), and it works.

**We don't have this yet.** AllSource has vectors and projections, but no navigational scaffolding layer that tells the agent "here's the shape of what you know" before it searches.

### Where AllSource Wins

| Dimension | zer0dex | AllSource |
|-----------|---------|-----------|
| **Temporal reasoning** | None — no concept of time | Full time-travel: "what did the agent know last Tuesday?" |
| **Provenance** | Memories are mutable blobs | Every mutation is an immutable event with audit trail |
| **Durability** | ChromaDB (SQLite-backed) | WAL (CRC32 checksums, configurable fsync) + Parquet (Snappy) |
| **Multi-agent** | Single-machine, single-agent | Multi-tenant, leader-follower replication, CRDT sync |
| **Query performance** | 70ms vector search | 11.9μs projection lookups + vector search |
| **Contradiction detection** | None | Projection-based: conflicting facts surface automatically |
| **Memory management** | Manual (human maintains MEMORY.md) | Automated projections rebuild from events |
| **Scaling** | Single Ollama instance | Horizontal read replicas, WAL shipping |
| **Offline + sync** | Local-only, no sync | Embedded Core + HLC/CRDT merge across agents |
| **SDKs** | Python only | Rust, Go, Python, TypeScript |

#### The Temporal Gap Is Critical

zer0dex has no concept of *when* a memory was true. If Alice was project lead until January and Bob took over in February, zer0dex's vector store returns both facts with no way to distinguish which is current.

AllSource stores every state change as an immutable event. Time-travel queries (`as_of`) reconstruct the agent's knowledge at any point in history. This isn't a nice-to-have — in domains like compliance, incident response, or multi-session agent workflows, temporal reasoning is table stakes.

#### The Provenance Gap Matters Too

zer0dex memories are mutable — `POST /add` overwrites or appends without tracking who added what, when, or why. In AllSource, every memory mutation is an append-only event:

```
{
  "event_type": "agent.memory.stored",
  "entity_id": "agent-alpha",
  "payload": {"fact": "Q3 churn was caused by pricing change", "confidence": 0.87, "source": "analysis-session-42"},
  "metadata": {"correlation_id": "incident-2026-03-15"}
}
```

You can trace any memory back to the event that created it, the session that triggered it, and the source material it was derived from. When an agent hallucinates a "memory," you can find exactly where it came from.

### Retrieval Quality

| System | Benchmark | Score | Notes |
|--------|-----------|-------|-------|
| **zer0dex** | Custom (n=97) | 91.2% recall | 80% cross-reference. Custom benchmark — not LoCoMo/LongMemEval |
| **AllSource** | Throughput-focused | 469K events/sec ingest, 12μs query | No published recall benchmark yet — gap we need to close |
| **Mem0** | LongMemEval | 49–66% | Vector-only; graph improves ($249/mo) |
| **Letta** | LoCoMo | ~83.2% | Cloud LLM required |
| **Zep** | LoCoMo | ~85% | Temporal graph excels here |

**Honest gap:** We haven't published LoCoMo or LongMemEval scores. Our benchmarks focus on throughput and query latency, not recall accuracy for conversational memory. We need to run these benchmarks and publish results. Filing this as a deliverable.

---

## What We're Building: AllSource Recall

zer0dex proved the compressed index idea works. AllSource has the engine to make it production-grade. We're combining both.

**AllSource Recall** — a dedicated agent memory API built on Core:

```
┌────────────────────────────────────────────────────────┐
│                   AllSource Recall                       │
│                                                         │
│  ┌─────────────┐  ┌─────────────┐  ┌────────────────┐ │
│  │ Compressed   │  │  Semantic   │  │   Temporal     │ │
│  │ Index        │  │  Vectors    │  │   Graph        │ │
│  │              │  │             │  │                │ │
│  │ Auto-gen     │  │ fastembed   │  │ Projections    │ │
│  │ from events  │  │ + HNSW     │  │ over events    │ │
│  │ via          │  │ indexed as │  │ with validity  │ │
│  │ projection   │  │ projection │  │ windows        │ │
│  └──────┬───────┘  └──────┬─────┘  └───────┬────────┘ │
│         └──────────────────┼────────────────┘          │
│                            │                            │
│                   ┌────────▼────────┐                   │
│                   │  AllSource Core  │                   │
│                   │  WAL + Parquet  │                   │
│                   │  + DashMap      │                   │
│                   └─────────────────┘                   │
└────────────────────────────────────────────────────────┘
```

### Three layers, all derived from events:

1. **Compressed Index (auto-generated).** A projection that maintains a token-efficient markdown summary of what the agent knows — organized by domain, with cross-references. Unlike zer0dex's manual MEMORY.md, this rebuilds automatically as events flow in. The projection watches for new entity types, new relationships between entities, and updates the index incrementally.

2. **Semantic Vectors.** fastembed in-process (no Ollama dependency). Every memory event gets embedded and indexed via HNSW projection. Hybrid search combines vector similarity with keyword matching.

3. **Temporal Graph.** Projections that track entity relationships with validity windows. "Alice led the project from January to March" is a first-class queryable fact, not two contradictory memories in a vector store.

### The API:

```python
from allsource import Recall

recall = Recall.connect("http://localhost:3900")  # or Recall.embedded("~/.agent/memory")

# Store a memory (creates an immutable event)
await recall.remember(
    agent_id="agent-alpha",
    fact="Q3 churn correlated with June pricing change",
    source="analysis-session-42",
    confidence=0.87,
    domain="revenue"  # used for compressed index organization
)

# Recall with hybrid search (vectors + index + temporal)
context = await recall.context(
    query="How does pricing relate to churn?",
    agent_id="agent-alpha",
    top_k=5,
    as_of=None  # current state; pass datetime for time-travel
)
# Returns: compressed index excerpt + top-k vector results + temporal context

# Get the compressed index (for injection into system prompt)
index = await recall.index(agent_id="agent-alpha")
# Returns: auto-generated markdown, ~800 tokens, cross-referenced by domain

# Time-travel
past_context = await recall.context(
    query="What did we know about churn?",
    agent_id="agent-alpha",
    as_of=datetime(2026, 1, 15)
)

# Forget (creates a tombstone event — the original memory is still in the audit trail)
await recall.forget(memory_id="mem-xyz", reason="Superseded by updated analysis")
```

### What makes Recall different from zer0dex:

| Feature | zer0dex | AllSource Recall |
|---------|---------|-----------------|
| Compressed index | Manual markdown | Auto-generated projection |
| Index updates | Human edits file | Incremental as events arrive |
| Temporal queries | Not possible | `as_of` parameter on every query |
| Memory provenance | None | Full event audit trail |
| Forget/correct | Destructive delete | Tombstone event (original preserved) |
| Multi-agent | Not supported | Multi-tenant with CRDT sync |
| Deployment | Python + Ollama | Embedded Rust binary or HTTP service |
| Offline | Local only | Embedded + sync when online |

---

## The Honest Summary

**zer0dex today** is a better out-of-the-box agent memory experience than AllSource today — for the single-agent, single-machine, Python-only use case. The compressed index idea is genuinely good, and the 70ms-to-first-memory developer experience is hard to beat.

**AllSource today** has stronger infrastructure (durability, temporal queries, multi-tenant, replication) but hasn't packaged it into a focused agent memory product. We have all the primitives; we haven't assembled them into the right API.

**AllSource Recall** is how we close this gap: take zer0dex's best idea (compressed index), make it automatic (projection-based), add what zer0dex can't do (temporal reasoning, provenance, multi-agent sync), and ship it as a single `recall.remember()` / `recall.context()` API.

The compressed index was the missing piece. Credit to Hermes Labs for proving it works.

---

*Sources: [zer0dex GitHub](https://github.com/roli-lpci/zer0dex) | [AllSource Core](https://github.com/all-source-os/chronos-core) | [AllSource Prime Proposal](../proposals/ALLSOURCE_PRIME.md) | [Vectorize: Best AI Agent Memory Systems 2026](https://vectorize.io/articles/best-ai-agent-memory-systems) | [DEV: 5 AI Memory Systems Compared](https://dev.to/varun_pratapbhardwaj_b13/5-ai-agent-memory-systems-compared-mem0-zep-letta-supermemory-superlocalmemory-2026-benchmark-59p3)*
