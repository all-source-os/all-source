---
title: "Copy — awesome-lists, GitHub Discussions, framework Discords"
status: READY (verify install snippet first)
last_updated: 2026-07-17
---

# MCP ecosystem, lists & communities

> Highest-ROI, lowest-effort inbound: get listed where agent builders already
> look, and drop a genuinely helpful blurb in the communities wiring memory
> right now. One PR / one message per venue — not a blast.

---

## 1. awesome-lists (open a PR each — evergreen inbound)

Target repos (search current names before PR'ing; these are the usual):
- `punkpeye/awesome-mcp-servers` (+ the `modelcontextprotocol/servers` community list)
- `e2b-dev/awesome-ai-agents`
- `kyrolabs/awesome-agents` / any "awesome-agent-memory" list
- `rust-unofficial/awesome-rust` (databases section)

**Entry line (MCP-servers lists):**
```markdown
- [AllSource](https://github.com/all-source-os/all-source) — Durable, event-sourced **memory for AI agents**. Records every event to WAL + Parquet, recalls in ~12μs, with provenance and time-travel. 73 MCP tools, self-host free (Apache-2.0).
```

**Entry line (agent-memory / agents lists):**
```markdown
- [AllSource](https://all-source.xyz) — Event-sourced agent memory: durable (survives restarts), ~12μs recall, provenance + time-travel, MCP-native. Alternative to vector-only memory (mem0/Zep/Letta). Apache-2.0 core.
```

**PR description template:**
> Adds AllSource — an event-sourced memory layer for AI agents (MCP-native, self-hostable, Apache-2.0 core). Fits the [memory/database] section. Repo: github.com/all-source-os/all-source. Happy to adjust the wording or placement.

---

## 2. GitHub Discussions — announcement post
(`siteConfig.links.discord` points here; there's no standalone Discord server yet — consider creating one, see checklist.)

**Title:** `AllSource is live: durable, event-sourced memory for AI agents (~12μs recall, MCP-native)`

**Body:**
> Hey all — AllSource is officially out. It's durable memory for AI agents built on an event store: every event your agent emits is persisted (WAL + Parquet), recall comes back in ~12μs, and because it's a log you get provenance ("why does the agent believe this?") and time-travel for free. 73 MCP tools so it drops into Claude Desktop and any MCP client.
>
> - Quickstart: `docker compose -f docker-compose.community.yml up -d`
> - Prime (agent memory, install in ~30s): see /prime
> - Comparisons vs mem0/Zep/Letta: all-source.xyz/vs/mem0
>
> This thread is the place for questions, bug reports, and "how do I wire it into X." I'm around. 🦫

---

## 3. Framework / AI-eng Discords (one tailored message each)

**Rules:** post in the right channel (#showcase / #show-and-tell / #integrations), read pinned rules, engage after. No @everyone, no repeat drops. Value first.

**Cursor / Cline / Continue.dev** (#showcase or #mcp)
> If you're wiring persistent memory into your agent via MCP — I built AllSource, an event-sourced memory server: durable (survives restarts), ~12μs recall so you can hit it every turn, provenance + time-travel. 73 MCP tools, self-host free. Drops into any MCP client: github.com/all-source-os/all-source — happy to help anyone set it up.

**LangChain / LlamaIndex** (#integrations / #show-and-tell)
> Built a memory backend for agents that's an event log instead of a vector store — durable, ~12μs recall, provenance + ordered history (vectors still available as a projection for fuzzy search). MCP-native. Would love feedback from folks who've hit the limits of pure-vector memory: all-source.xyz/event-sourcing-for-ai-agents

**Latent Space / AI Engineer** (#projects)
> Sharing a project: AllSource — event-sourced memory for agents. The bet is that agents reason over *sequences of events*, not nearest-neighbors, so memory should be a durable log you can query in ~12μs and trace back to source (provenance) — with vectors as a projection on top. MCP-native, Apache-2.0 core. Curious what this crowd thinks of event-log-as-memory: github.com/all-source-os/all-source

**MCP community (modelcontextprotocol Discord/Discussions)** (#servers / #showcase)
> New MCP server: AllSource — durable agent memory (73 tools). Read/write the agent's memory, semantic search, temporal queries, provenance lookups. Self-host free (Apache-2.0). github.com/all-source-os/all-source

---

## 4. Claude Desktop MCP install snippet (verify against /install before using)

Include this wherever people ask "how do I add it to Claude":
```jsonc
// ~/Library/Application Support/Claude/claude_desktop_config.json
{
  "mcpServers": {
    "allsource-prime": {
      "command": "allsource-prime",
      "args": ["--data-dir", "~/.prime/memory", "--mode", "stdio"]
    }
  }
}
```
> ⚠️ Confirm the exact command/args from all-source.xyz/install — this is reconstructed from `cargo install allsource-prime` + the HTTP-mode example, and the stdio invocation must be verified.
