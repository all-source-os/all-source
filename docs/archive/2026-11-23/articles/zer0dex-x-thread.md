# X Thread: zer0dex vs AllSource — Agent Memory Showdown

---

**1/**
zer0dex (@roli_lpci) just dropped a two-layer memory system for AI agents that scores 80% on cross-domain recall where pure vector RAG scores 37.5%.

We looked at it honestly. Here's what they got right, where they fall short, and what we're building to combine the best of both.

---

**2/**
The insight is simple and correct:

When you ask an agent "How does X relate to Y?", vector similarity finds X or Y — rarely both.

zer0dex fixes this with a compressed markdown index (~782 tokens) that maps cross-domain relationships. The agent knows the shape of what it knows before it searches.

---

**3/**
Their results (n=97):

- 91.2% recall (vs 80.3% full RAG, 52.2% flat files)
- 80% cross-reference accuracy (vs 37.5% full RAG)
- 70ms latency, $0/month, fully local

The cross-reference number is the real story. 2x improvement on the hardest memory retrieval problem.

---

**4/**
But zer0dex has three blind spots:

1. No temporal reasoning. If Alice led the project until January and Bob took over in February, both facts sit in the vector store with equal weight. No way to ask "who was lead last month?"

2. No provenance. Memories are mutable blobs. When an agent "remembers" something wrong, you can't trace where the bad memory came from.

3. The compressed index is manually authored. Scales with human effort, not with data.

---

**5/**
AllSource already solves 1 and 2.

Every memory mutation is an immutable event in a WAL + Parquet store. Full audit trail. Time-travel queries reconstruct agent knowledge at any past point.

469K events/sec ingest. 12 microsecond queries. Durable across restarts.

What we didn't have: the compressed index.

---

**6/**
So we're building AllSource Recall.

Three layers, all derived from events:

- Auto-generated compressed index (projection-based, not manual)
- Semantic vectors (fastembed in-process, HNSW indexed)
- Temporal graph (relationships with validity windows)

One API: `recall.remember()` / `recall.context()`

---

**7/**
The key difference from zer0dex: the compressed index isn't a static file you maintain by hand.

It's a projection. As events flow in, the index rebuilds incrementally — new domains, new cross-references, updated summaries. Zero human maintenance.

---

**8/**
What Recall adds that no current agent memory system has:

- Compressed index (zer0dex's idea) + temporal queries (Zep's strength) + immutable provenance + offline sync — in one engine
- Embedded or service mode. Same binary.
- Rust, Go, Python, TypeScript SDKs
- Multi-agent with CRDT merge

---

**9/**
Credit to Hermes Labs for proving the compressed index thesis. The 80% cross-reference number changed our roadmap.

zer0dex is the right idea. AllSource Recall is that idea made durable, temporal, automatic, and production-ready.

Repo: github.com/all-source-os/chronos-core
zer0dex: github.com/roli-lpci/zer0dex

---

**10/**
We'll publish LoCoMo benchmark results for Recall when it ships. No custom benchmarks, no asterisks. Standard eval, public numbers.

If the compressed index is as good as zer0dex's results suggest — and we think it is — the numbers will speak for themselves.

---
