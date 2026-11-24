# Launch kit — "Building an AI-native event store in Rust"

Canonical post: `apps/web/content/building-an-ai-native-event-store-in-rust.mdx`
→ live at https://www.all-source.xyz/blog/building-an-ai-native-event-store-in-rust

Repo: https://github.com/all-source-os/all-source

**Posting notes**
- Post the blog/repo link, then be present in the comments for the first few hours — engagement drives ranking more than the title.
- Stagger, don't blast: Show HN first (weekday morning US-Eastern lands best), then r/rust, then Lobsters. Each wants a *different* framing (below).
- Do not cross-link "please upvote" — every platform's mods kill that.
- Lead with the reproducible benchmark, not the number. The hook is "run it yourself."

---

## Show HN

**Title** (80 char max, no emoji, no hype):
```
Show HN: AllSource – an AI-native event store in Rust (reproducible 494K ev/s)
```

**URL:** https://github.com/all-source-os/all-source

**Text** (first comment — HN convention: say what it is, why you built it, invite critique):
```
I wanted a real database for agent memory, not a vector DB stapled to a prompt —
so I built AllSource, an append-only event store in Rust.

Core is the database: WAL (CRC32 + fsync) for durability, Parquet (Snappy) for
columnar cold storage, DashMap for ~12µs in-memory reads. No Postgres in the
event path. It ships a native MCP server (40+ tools) so an agent can ingest
events, run time-travel queries, and traverse memory directly, and a memory
engine (knowledge graph + vector search) that's crash-safe because it's written
to the WAL before it's acted on.

The 469K events/sec figure is in the README, but the point is you can run the
harness yourself: `cargo run --release -p allsource-performance`. On my M2 Max
the batch path does 494K ev/s and a concurrent pipeline ~948K. Numbers are
hardware-dependent; the harness asserts minimum targets so regressions fail CI.

Apache 2.0, on crates.io as allsource-core. Honest about the edges: it's not a
Postgres replacement, and the non-Rust SDKs are younger. Happy to go deep on the
WAL/Parquet/DashMap design or the MCP integration — tear it apart.
```

---

## r/rust

**Title:**
```
AllSource: an AI-native event store in Rust — WAL + Parquet + DashMap, with a benchmark you can run
```

**Link:** https://github.com/all-source-os/all-source (or link the blog post for the writeup)

**Body** (r/rust rewards engineering substance, dislikes marketing):
```
Event store written in Rust. The interesting Rust bits:

- Lock-free + sharded ingestion queue, arena-pooled allocation on the hot path,
  SIMD JSON parsing. Single binary, zero external deps in the event path.
- Durability is WAL (CRC32 checksums, configurable fsync) with crash recovery;
  cold storage is columnar Parquet/Snappy; reads come off a DashMap.
- Embeddable as `allsource-core` (crates.io) or runnable as a service.

Reproducible benchmark in-repo — `cargo run --release -p allsource-performance` —
which asserts throughput targets so perf regressions fail CI. On an M2 Max the
batch ingestion path hits ~494K events/sec, a 4-thread pipeline ~948K, SIMD
filtering ~9M/s.

The "AI-native" angle is a first-party MCP server and an agent-memory engine
(graph + vector recall) on the same event log, but I'm mostly here for feedback
on the storage engine design. Apache 2.0. Writeup: <blog link>.
```

---

## Lobsters

(Lobsters prefers an article link over a bare repo. Submit the blog post.)

**URL:** https://www.all-source.xyz/blog/building-an-ai-native-event-store-in-rust

**Title:**
```
Building an AI-native event store in Rust
```

**Tags:** `rust`, `databases`, `ai`

**Optional comment:**
```
Author here. The post covers the WAL + Parquet + DashMap design and ships a
benchmark harness you can run (`cargo run --release -p allsource-performance`).
Glad to discuss the durability model or the MCP/agent-memory layer.
```

---

## After it lands

- Pin the best thread on the repo's Discussions.
- If a comment surfaces a real gap, open an issue from it and link back — visible
  responsiveness converts skeptics.
- Watch crates.io download deltas on `allsource-core` / `allsource` for the
  contracting signal.
