---
title: "Copy — Reddit (per-subreddit)"
status: READY (fill blockers first; read each sub's rules)
last_updated: 2026-07-17
---

# Reddit — per-subreddit copy

> **Golden rule:** never cross-post the same text. Each sub gets its own angle,
> or you get flagged + banned. Post as a human sharing a build, not an ad.
> Reply to every comment in the first 2 hours. Lead with substance, link last.
> Most of these subs require you to have comment history — warm up the account.

---

## r/rust  (post FIRST — warm, technical, forgiving)
**Angle:** the substrate. Rust engineering, not the AI pitch.

**Title options**
- `I built a durable event store in Rust that does 469K events/sec with 11.9μs reads — using it as memory for AI agents`
- `Show r/rust: event-sourced agent memory — WAL + Parquet + DashMap, 11.9μs p99 recall`

**Body**
> I've been building AllSource, a purpose-built event store in Rust, and using it as the memory layer for AI agents (the thing that makes them not forget everything on restart).
>
> The engine side, since that's what this sub cares about:
> - **Durability:** write-ahead log with CRC32 checksums + configurable fsync, then Parquet (Snappy) for columnar persistence. In-memory reads via DashMap.
> - **Reads:** 11.9μs p99. The hot path is an O(1) DashMap lookup on projections rebuilt from the WAL on boot — no SQL parse, no disk on the read path.
> - **Ingest:** 469K events/sec.
> - **Footprint:** ~129MB for the whole stack.
> - No Postgres in the event path — it *is* the database.
>
> The reason it exists: agent memory stacks are usually a vector DB that's too slow to hit every turn. At ~12μs you can query memory on every message, and because it's an event log you get provenance and time-travel for free.
>
> It's open source (core is Apache-2.0). Repo, and happy to talk through the durability/WAL design or the lock-free read path: github.com/all-source-os/all-source
>
> Honest question for this sub: where would you push back on the fsync/recovery design?

**First comment (seed the technical thread):** drop the WAL recovery flow or the DashMap-vs-RwLock benchmark. Invite critique.

---

## r/LocalLLaMA  (primary ICP — post the DEMO)
**Angle:** agents that don't forget; self-hostable; show, don't tell.

**Title options**
- `I got tired of my agents forgetting everything on restart, so I built durable memory that recalls in ~12μs [self-hostable]`
- `Event-sourced memory for local agents — survives restarts, ~12μs recall, MCP-native, Apache-2.0`

**Body**
> Every local-agent setup I built hit the same wall: memory is a chat log stuffed into context, and it's gone the moment the process restarts. Vector DBs "solve" it but they're slow enough that you ration lookups, and they only tell you what's *similar*, not what actually *happened*.
>
> So I built AllSource — an event store used as the agent's memory:
> - **Durable:** every event the agent emits hits a write-ahead log + Parquet. Restart the process, memory's still there.
> - **Fast:** ~12μs recall (11.9μs p99), so you can query memory on *every* turn instead of rationing it.
> - **Ordered + provenanced:** it's a log, so you get "what happened, in what order, at what time" and can trace any memory back to its source event.
> - **MCP-native:** 73 tools, drops into Claude Desktop / any MCP client. Agent reads and writes memory directly.
> - **Self-hostable:** core is Apache-2.0, runs in ~129MB, embeddings in-process (no external embedding API).
>
> 2-min demo (agent forgets → restart → recalls in 11.2μs → shows the provenance chain): [VIDEO/GIF]
>
> Repo: github.com/all-source-os/all-source — `docker compose -f docker-compose.community.yml up -d` to try it.
>
> Curious what this sub thinks: for local agents, is exact event recall more useful than semantic/vector recall, or do you want both?

**First comment:** paste the exact install + a minimal "store an event, recall it" snippet. Offer to help anyone wiring it into their stack.

**Rules note:** r/LocalLLaMA tolerates self-builds if they're genuinely local/self-hostable and you engage. Lead with the demo, not the pricing. Do **not** mention hosted tiers unless asked.

---

## r/LLMDevs  (agent builders)
**Angle:** the architecture decision — event log vs vector store for memory.

**Title:** `Why I moved my agent's memory from a vector DB to an event log (provenance + time-travel + ~12μs recall)`

**Body**
> Sharing an architecture change that paid off. My agents' memory was a vector store. Two problems kept biting:
> 1. **Speed** — similarity search was too slow to hit every turn, so I rationed memory lookups. A memory you ration isn't memory.
> 2. **Truth** — an embedding tells you what's *similar*, not what *happened*, in order. Agents reason over sequences ("user did X, then I decided Y"). That's a log, not nearest-neighbor.
>
> I switched to an event log as the substrate (built AllSource for this): store every event durably (WAL + Parquet), serve O(1) recall from in-memory projections in ~12μs, and keep vectors as a *projection* on top for when you actually want fuzzy search. Bonus: every memory has provenance, and you can time-travel the agent's memory to any past state for debugging.
>
> 73 MCP tools so it drops into Claude/any MCP client. Core's Apache-2.0, self-hostable.
>
> Writeup + comparisons to mem0/Zep/Letta: all-source.xyz/event-sourcing-for-ai-agents · repo: github.com/all-source-os/all-source
>
> Has anyone else moved off pure-vector memory? What broke for you?

---

## r/AI_Agents  (week 2 — long tail)
**Angle:** practical "make your agent remember" + the provenance/audit hook for anyone shipping to prod.

**Title:** `Giving agents memory that survives restarts AND can explain itself (event-sourced, MCP-native)`

**Body:** condense the LLMDevs body; emphasize provenance/audit ("when your agent does something in prod, you can prove *why*") for the fin/health/legal builders. Link `/solutions/agent-memory` + repo. End with a question about how people currently debug agent decisions.

---
**Universal reminders:** disclose you're the builder. No upvote-begging. Answer the hard questions honestly (including "why not just use mem0?"). If a mod removes it, message them — don't repost.
