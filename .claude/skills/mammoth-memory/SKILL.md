---
name: mammoth-memory
description: Durable cross-session memory for this repo via AllSource Prime. Recall what was learned in past sessions and record salient new facts as you work — so the user never re-explains decisions, architecture, or task history twice. Use at the START of a session to orient (recall prior context before answering non-trivial questions), and DURING work to record decisions, gotchas, and outcomes worth remembering. Triggers on "what did we decide about X", "remember this", "why did we do Y", "recall", "have we discussed", "load context", or any question that depends on prior-session knowledge. Also use PROACTIVELY: recall before answering architecture/decision/history questions; record after a non-obvious decision is made. mammoth = the memory layer; caveman makes few token, mammoth never forget token.
---

# mammoth-memory

Durable agent memory for this repo. Backed by the AllSource Prime MCP server
(`prime_*` tools) running local-only against `.prime/` — durable WAL + Parquet,
no account, survives restarts. Generalized from the shipped `pr-atom-reviewer`
recall/record pattern into a general-purpose memory loop.

**Hard rule — provenance:** record your own analysis (decisions, rationale,
gotchas, outcomes, summaries). NEVER write raw proprietary source code, secrets,
API keys, or full file contents into Prime nodes/embeddings unless the user
explicitly says it's OK. Metadata, file paths, and your conclusions are fine.

**Availability gate:** only act if `prime_*` tools are present. If they are not,
skip silently — do not announce "memory unavailable" unless the user asks. (The
project `.mcp.json` wires the server; tools surface after the user approves the
project MCP server on session start.)

---

## When to RECALL (read memory)

Do this before answering any question that depends on prior-session knowledge —
architecture choices, "why did we…", task history, who/what/when — and at the
start of a fresh session for orientation.

1. **Orient (cheap).** Call `prime_stats` to see if memory has anything at all.
   Cold/empty store → skip recall, proceed normally, don't fabricate context.
2. **Hybrid recall.** Call `prime_context` with the user's question as `query`
   (tier `L2`, the default — adds the compressed index + vectors + graph). For a
   pure "what do I know about X" lookup, `prime_recall` with `text` = the query
   is enough. Ask for `top_k` 5.
3. **Use it, cite it.** If recall returns relevant facts, fold them into your
   answer and tell the user this came from memory ("from an earlier session: …")
   so the recall is visible — that visibility IS the product's magic moment.
4. **Cold start is fine.** Empty results on the first sessions of a repo are
   expected. Never invent prior context to fill the gap.

Tool params come from each tool's input schema — `prime_context` requires
`query`; `prime_recall` takes `text` (server embeds in-process via fastembed) or
a precomputed `vector`, plus optional `depth` (0 = vector-only, 1+ = graph
expansion) and `top_k`. Don't guess names; the schemas are authoritative.

---

## When to RECORD (write memory)

Record after something worth remembering happens. Best-effort: if a call fails,
note it briefly and continue — recording never blocks the real work.

**What counts as salient** (default policy — owner may refine, chronis `t-7cdc`):
- A **decision** and its rationale ("chose X over Y because Z").
- A **non-obvious gotcha / constraint** discovered ("Core can't cross-compile to
  amd64 on Apple Silicon — QEMU fails on Erlang NIF").
- A **durable fact about the project** not derivable from code (ownership,
  conventions, external dependencies, deadlines).
- An **outcome** (what shipped, what was verified, what failed and why).

**What is NOT salient** (do not write — keeps the store signal-dense):
- Restating what's already in the codebase, git history, or CLAUDE.md.
- Transient state ("currently editing foo.rs"), chit-chat, or step narration.
- Anything covered by an existing node — update it instead of duplicating.

Recording steps:
1. **Find or create the subject node.** `prime_search` (by `type`) or
   `prime_neighbors` to check if a node already exists. Create with
   `prime_add_node` only if absent. Always include a `domain` property (e.g.
   `"domain": "mammoth"`, `"engineering"`, `"architecture"`) — domains power the
   compressed index's cross-references. Pick a `type` from the tool's set:
   `decision`, `insight`, `concept`, `project`, `event`, `person`, `metric`.
2. **Embed it for semantic recall.** `prime_embed` with `text` = your written
   fact/decision (NOT source code), keyed by the node's `entity_id` (the id
   `prime_add_node` returned). This is what makes it findable later.
3. **Connect it.** `prime_add_edge` to link the new node to related ones
   (`depends_on`, `causes`, `impacts`, `requires`). Cross-domain edges are the
   highest-value: they make the index's cross-reference section useful.

---

## The loop (proactive default)

- **Session start / new topic:** `prime_stats` → if non-empty and the topic is
  non-trivial, `prime_context` to pull relevant prior knowledge before answering.
- **After a decision or discovery:** record it (node + embed + edges).
- **On "remember this":** record the user's note verbatim as an `insight`/
  `decision` node + embed.
- **On "what did we decide / why / have we discussed":** recall first, answer
  from memory, cite it.

`--auto-inject` is on in `.mcp.json`, so a compressed index is injected as the
`prime://auto-context` resource each conversation — you often already have a
high-level map before calling anything. Use `prime_context`/`prime_recall` to go
from that map to specifics.

---

## Verification checklist

- [ ] Checked `prime_*` tools exist before acting (silent skip if not).
- [ ] Recalled (via `prime_context`/`prime_recall`) before answering a
      prior-knowledge question; cited memory when used.
- [ ] Recorded only salient, non-proprietary facts; updated rather than
      duplicated existing nodes.
- [ ] Every recorded node has a `domain`; salient nodes are embedded so they're
      recallable.
- [ ] No raw source code, secrets, or full file contents written to Prime.
