---
title: "Copy — X / Twitter (@ddonprogramming)"
status: READY (verify blockers first)
last_updated: 2026-07-17
---

# X / Twitter — @ddonprogramming

> Founder POV, first person. Each tweet <280 chars and stands alone. Post the
> launch thread on Product Hunt morning to cross-amplify. Then run ONE hook
> thread per week. Closer on outbound: `Self-host free. £18.99/mo. Apache-2.0. → all-source.xyz`

---

## A. LAUNCH THREAD (fire on PH morning)

**1/**
Your AI agents forget everything the moment the process restarts.

I got tired of it, so I built durable memory for agents. Records every event, recalls it in ~12μs, survives restarts.

It's live 👇 [PH link]

**2/**
The usual fixes both fail:

• chat-log stuffed in context → gone on restart
• vector DB → too slow to hit every turn, and only tells you what's *similar*, not what *happened*

Memory you ration isn't memory.

**3/**
AllSource is an event store used as the agent's memory.

Every event hits a write-ahead log (CRC32 + fsync) + Parquet. Restart the process — it's all still there. It's a real database, not a cache.

469K events/sec ingest.

**4/**
Recall is ~12μs (11.9μs p99).

That's an O(1) hash-map lookup on in-memory projections rebuilt from the durable log — no SQL parse, no network on the hot path.

At that speed you query memory on *every* message. It stops being a tool you call and is just… there.

**5/**
Because it's an event log, you get two things a vector DB can't give you:

🔎 provenance — trace any memory to its source event. Ask *why* the agent believes something.
⏪ time-travel — recall the agent's memory as-of any past moment.

**6/**
It drops into Claude Desktop via 73 MCP tools — the agent reads and writes memory directly. Embeddings run in-process, so no external embedding API.

Vectors are still there when you want fuzzy search — as a projection, not the source of truth.

**7/**
Already on mem0 / Zep / Letta? I wrote honest side-by-sides:
all-source.xyz/vs/mem0 · /vs/zep · /vs/letta

Core is Apache-2.0. Self-host the whole thing free, forever, on your hardware.

**8/**
Try it:

`docker compose -f docker-compose.community.yml up -d`
repo → github.com/all-source-os/all-source

Your agents already forget. Stop letting them.

Self-host free. £18.99/mo hosted. Apache-2.0. → all-source.xyz

---

## B. PINNED TWEET

> AI agents forget everything on restart.
>
> AllSource is durable memory for agents: records every event, recalls it in ~12μs, survives restarts, and can tell you *why* it remembers something.
>
> 73 MCP tools → drops into Claude. Self-host free (Apache-2.0).
>
> 60s demo 👇 [video]
> → all-source.xyz

---

## C. WEEKLY HOOK THREADS (run one per week)

### Week 1 — NUMBERED: "3 reasons your agent forgets"
**1/** 3 reasons your AI agent forgets everything between sessions — and none of them are the model's fault. 🧵
**2/** Reason 1: its "memory" is a chat log. You stuff yesterday into the context window and pray. Window rolls over or the process restarts → gone. That's short-term recall with amnesia on a timer.
**3/** Reason 2: the store isn't durable. Most agent-memory stacks keep state in RAM or a cache that evaporates on restart. AllSource is the opposite: every event hits a WAL (CRC32 + fsync) + Parquet. Restart → events still there.
**4/** Reason 3: recall is too slow to use every turn. At 200-500ms you ration it. AllSource recalls in ~12μs (11.9μs p99) — query memory on *every* message and the user never feels it.
**5/** Fix all three: record every event, durably, recall in microseconds. 469K events/sec. 73 MCP tools into Claude. Your agents already forget. Stop letting them.
Self-host free. £18.99/mo. Apache-2.0. → all-source.xyz

### Week 2 — BENCHMARK: "12μs recall. Here's the code."
**1/** ~12μs agent-memory recall. Not milliseconds. Microseconds. Here's the lookup path and how you reproduce it yourself. 🧵
**2/** For scale: a typical HTTP round-trip is 50-200ms; cloud memory APIs land in the hundreds of ms. At 11.9μs (p99) the lookup is invisible — faster than printing the response.
**3/** Why: the read path is a concurrent hash-map lookup, not a query.
`query → DashMap projection → O(1) lookup → result` (no SQL parse, no network, no disk on the hot path). Projections rebuild from the durable WAL on boot, so fast never means lossy.
**4/** Run it:
```
cargo install allsource-prime
allsource-prime --data-dir ~/.prime/memory --mode http --port 3905
curl -w "time_total: %{time_total}s\n" http://localhost:3905/api/v1/prime/stats
```
**5/** Same engine: 469K events/sec ingest, ~129MB footprint, 73 MCP tools. 12μs recall — here's the code, the rest is on GitHub.
Self-host free. £18.99/mo. Apache-2.0. → all-source.xyz

### Week 3 — CONTRARIAN: "stop putting agent memory in a vector DB"
**1/** Hot take: a vector database is the wrong default for agent memory. The "just embed everything" reflex quietly costs you the two things memory needs most. 🧵
**2/** It costs you *time*. Vector search is a similarity scan — great for fuzzy retrieval, terrible as the thing you hit every turn. Ration it and it's not memory.
**3/** It costs you *truth*. An embedding tells you what's *similar*, not what *happened*, in order, at what time. Agents reason over sequences of events. That's a log, not nearest-neighbor.
**4/** Reframe: store the events, project them. AllSource keeps an immutable durable log (WAL + Parquet) and serves O(1) recall in ~12μs. Vectors stay — as a projection, not the source of truth. Bonus: provenance + time-travel.
**5/** Events as substrate, ~12μs exact recall on the hot path, vectors when you actually need similarity. 469K events/sec, 73 MCP tools, Apache-2.0.
Your agents already forget. Stop letting them. → all-source.xyz

---

## D. Reply-guy angles (engage, don't broadcast)
When you see someone posting about: mem0/Zep/Letta pain, agents forgetting context, RAG memory being slow, MCP servers, or "how do I give my agent long-term memory" — reply with ONE specific helpful point (the provenance or the 12μs number), link only if they ask. Never copy-paste. Tag no one gratuitously.

**Fact-check:** all numbers → `00-LAUNCH-PLAN.md §2`. Verify `allsource-prime` install command against current `/install` before posting Week 2.
