---
title: "Copy — Product Hunt"
status: READY (fill launch date + verify P0/P1 blockers first)
last_updated: 2026-07-17
---

# Product Hunt — launch kit

> Drop-in. Founder voice (@ddonprogramming). Numbers match `00-LAUNCH-PLAN.md §2`.
> **Before posting:** clear P0 (license, tool count) + P1 (USD vs GBP) blockers.

## Name
**AllSource**

## Tagline (≤60 chars) — pick one
1. **Durable memory for AI agents — recall in microseconds**  ← recommended
2. Your AI agents forget. AllSource makes them remember.
3. Event-sourced memory for agents. ~12μs recall. Apache-2.0.

## Topics
AI Agents · Developer Tools · Open Source · Databases · Artificial Intelligence

## Description (the "what is it" blurb, ≤260 chars)

> AllSource is durable memory for AI agents. It records every event your agent emits to a write-ahead log + Parquet (survives restarts), then serves recall from in-memory projections in ~12μs — fast enough to hit every turn. 469K events/sec, 73 MCP tools for Claude, self-host free (Apache-2.0).

## Gallery (in order)
1. **Hero video (2 min)** — the demo: JSON events stream in → Claude asks "what did the user do yesterday at 3pm?" → answer renders out of those events → **"returned in 11.2μs ✓"** stamped on it → then **click the answer to reveal its provenance chain** (the differentiator).
2. 5-tier pricing screenshot.
3. Architecture / latency diagram (WAL + Parquet + DashMap → 11.9μs).
4. One comparison table: AllSource vs mem0 vs Zep vs Letta (provenance / time-travel / durable / recall latency).

## First comment (post immediately on launch)

Hey Product Hunt 👋 I'm Decebal, building AllSource.

I kept hitting the same wall with AI agents: they forget. You stuff yesterday into the context window, the process restarts, and it's gone. The fixes on offer were either a chat-log hack or a vector DB that's too slow to hit every turn and only tells you what's *similar*, not what actually *happened*.

So I built the thing I wanted: an event store as the agent's memory. Every event the agent emits is durable (WAL with CRC32 + fsync, then Parquet), and recall comes back in ~12μs from in-memory projections — invisible, so you can query on every single message. Because it's an event log, every memory has provenance (you can ask *why* the agent believes something) and you can time-travel its memory to any past moment.

469K events/sec, 73 MCP tools so Claude reads and writes memory directly, and x402 so the agent pays per call instead of you renting capacity you don't use.

It's Apache-2.0-licensed — self-host the whole thing free, forever, on your own hardware. Hosted starts at £18.99/mo if you'd rather I run it. There are honest comparison pages up too (vs mem0, Zep, Letta) if you're already using one.

Would love your hardest questions — especially on durability and the microsecond recall numbers. AMA below. 🦫

## "What's new" bullets (trim to 5–6)

- 🧠 **Durable agent memory, not a chat log** — every event hits WAL (CRC32 + fsync) + Parquet; survives restarts.
- ⚡ **~12μs recall (11.9μs p99)** — query memory on every turn without the user feeling it.
- 🔎 **Provenance built in** — every memory traces to its source event. Ask *why* the agent believes something.
- ⏪ **Time-travel memory** — recall the agent's state as-of any past moment.
- 🔌 **73 MCP tools** — drop into Claude Desktop; the agent reads and writes memory directly.
- 🚀 **469K events/sec** — the memory layer is never the bottleneck.
- 🆓 **Self-host free (Apache-2.0)** — unlimited events on your hardware. Hosted from £18.99/mo.

## Canned replies (predictable questions)

- **"Isn't it just in-memory / lost on restart?"** → No. Event data is durable: WAL (CRC32, configurable fsync) + Parquet (Snappy). Only the in-memory projections are rebuilt from the log on boot — the source of truth is on disk.
- **"Why not a vector DB / Postgres?"** → A vector DB tells you what's *similar*, not what *happened*, in order, at what time. Agents reason over sequences of events — that's a log. Vectors are still there as a projection when you want fuzzy search. (Postgres is for operational metadata, not the event log.)
- **"How is recall that fast?"** → O(1) concurrent hash-map (DashMap) lookup on in-memory projections — no SQL parse, no network, no disk on the hot path. ~12μs.
- **"How does it compare to mem0 / Zep / Letta?"** → Short version: they're vector-memory; we're an event log with provenance + time-travel + durability. Long version, honestly laid out: all-source.xyz/vs/mem0 (also /vs/zep, /vs/letta).
- **"Free tier?"** → Self-host is genuinely free and unlimited (Apache-2.0). Hosted starts at £18.99/mo.
- **"Does it need an embedding API?"** → No. Prime ships embeddings in-process (fastembed + HNSW). Install in ~30s.

## Maker reply template
> Thanks [name]! [one-sentence direct answer.] [the grounded number.] If you want to poke at it, it's open source and self-host is free — github.com/all-source-os/all-source. Happy to go deeper. 🦫

---
**Closer for any linked post:** `Self-host free. £18.99/mo hosted. Apache-2.0. → all-source.xyz`
