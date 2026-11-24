---
title: "Copy — newsletter & aggregator pitches"
status: READY
last_updated: 2026-07-17
---

# Newsletter / aggregator pitches

> Time submissions to land the same week as Product Hunt. Most want a link +
> 2-3 sentences. Keep it factual — editors hate hype. One-line summary is what
> they'll actually paste, so make it carry the whole story.

**Reusable one-line summary (the paste-ready blurb):**
> **AllSource** — durable, event-sourced memory for AI agents: records every event, recalls it in ~12μs, survives restarts, with provenance and time-travel. MCP-native (drops into Claude), self-host free (Apache-2.0). github.com/all-source-os/all-source

---

## TLDR AI  (submit via the "share" / sponsor-free tip link in the newsletter footer)
**Subject:** Tool submission — AllSource (durable memory for AI agents)
> Hi — sharing a launch that fits your Tools/Open-Source section. AllSource is durable memory for AI agents: an event store (not a vector DB) that records every event an agent emits, recalls it in ~12μs, and survives restarts — with provenance and time-travel that vector memory can't give you. MCP-native, self-host free (Apache-2.0). Just launched on Product Hunt. One-liner + link above if useful.

## Console.dev  (beta/tools submission form on console.dev)
**Subject:** Beta tool for review — AllSource (event-sourced agent memory)
> AllSource is a developer tool worth a look for the newsletter: event-sourced memory for AI agents, self-hostable (Apache-2.0 core), ~129MB footprint, 73 MCP tools, 11.9μs p99 recall. It's the "why an event log beats a vector DB for agent memory" angle, with working comparison pages (vs mem0/Zep/Letta). Repo + site above. Happy to answer anything for the review.

## This Week in Rust  (PR to `rust-lang/this-week-in-rust`, or the suggestion form)
**Section:** Project/Tooling Updates or Crate of the Week nominee
> Suggestion: **AllSource** — a durable event store written in Rust (WAL + Parquet + DashMap, 469K events/sec, 11.9μs p99 reads) now shipping as memory for AI agents. Interesting to the Rust crowd for the lock-free read path and the fsync/recovery design. Crates: `allsource-core`, `allsource-prime`. Repo: github.com/all-source-os/all-source

## Latent Space  (reply to swyx / the newsletter's tip channel or Discord)
**Subject:** Project tip — event-log-as-agent-memory
> For the AI-eng audience: AllSource makes the bet that agent memory should be a durable event log queried in ~12μs (provenance + time-travel + ordered history), with vectors as a projection rather than the source of truth — instead of the default "embed everything into a vector DB." MCP-native, Apache-2.0 core. Might be a fit for the newsletter or a Discord share. Comparisons: all-source.xyz/event-sourcing-for-ai-agents

## The Changelog  (news submit at changelog.com/news/submit + pitch the pod)
**Subject:** News + possible pod topic — event sourcing meets agent memory
> Submitting for Changelog News: AllSource, an open-source Rust event store now positioned as durable memory for AI agents (self-host free, Apache-2.0 core). Possible podcast angle: why event sourcing — a 2010s idea — turns out to be the right substrate for 2026 agent memory (durability, provenance, time-travel) vs the vector-DB default. Repo: github.com/all-source-os/all-source

---

## Also worth a submission (lower priority)
- **Ben's Bites**, **The Rundown AI** — tip/submit links in footer; use the one-liner.
- **Hacker Newsletter** — auto-pulls from HN; nothing to do beyond the Show HN (done).
- **Awesome-lists** are covered in `copy-mcp-lists-discord.md`.

**Timing:** send TLDR AI / Console.dev / Latent Space 2–3 days *before* PH so they can slot it; This Week in Rust / Changelog can trail by a few days.
