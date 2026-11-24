# mammoth — marketplace listing

Listing-ready copy + the pre-publish checklist for bead `t-8882`. The actual
publish is an **owner action** (push to `main`, then `/plugin marketplace add` /
a GitHub release) — this file is the source copy and the gate before that.

## Tagline (≤ 80 chars)

> Durable memory for your coding agent. Never re-explain your project again.

## One-liner

> caveman make few token. mammoth never forget token.

## Short description (marketplace card)

Durable, cross-session memory for any MCP coding agent. Decisions, task history,
and code context get remembered and recalled automatically — local-first, zero
signup, works in Claude Code, Cursor, Cline, Windsurf, Codex, and Gemini.

## Value prop (3 bullets)

- **Remembers across sessions** — decisions, gotchas, and project context recalled
  automatically, so you stop re-explaining your project every time.
- **Local-first, zero signup** — durable on-disk store (WAL + Parquet), survives
  reboots, no account. Cross-machine/team sync is an opt-in upgrade.
- **MCP-native** — one server, 23 `prime_*` tools, no per-agent shim. Claude Code
  also gets a skill + `/remember` `/recall` `/memory-status` `/memory-sync`.

## Proof line (real, reproducible)

Benchmarked vs a keyword search+grep baseline on deliberately-reworded queries
(60 memories): **recall hit@5 0.90 vs 0.83**, **~3ms** recall latency,
**durability PASS** (write→restart→read). Harness: `tooling/mammoth-bench/`.

## Install (the card's CTA)

```
/plugin marketplace add all-source-os/all-source
/plugin install mammoth
```

Or any MCP agent: `cargo install allsource-prime` + the one-line `install.sh`.

## Category / keywords

Category: productivity / AI agents. Keywords (mirrors `plugin.json`): memory,
recall, agent-memory, mcp, vector, knowledge-graph, event-sourcing, allsource,
prime, mammoth.

---

## Pre-publish checklist (owner)

Capture these before the listing goes live — the marketplace card and launch
thread both need them:

- [ ] **Demo GIF / 15-sec video** — the magic moment: fresh session recalls an
      earlier-session decision from a reworded query, nothing pasted. This is the
      single most important asset.
- [ ] **Benchmark table image** — the `bench2.py` results (for the card + tweet 3).
- [ ] **Plugin icon / logo** — a mammoth mark (caveman ships an SVG; match the
      register). Drop in `plugin/mammoth/assets/`.
- [ ] **Verify `repository` URL** in `plugin.json` resolves once the repo is public
      (`github.com/all-source-os/all-source`).
- [ ] **Verify `homepage`** `https://www.all-source.xyz/prime` is live.
- [ ] **Smoke-test the install path on a clean machine** — `/plugin marketplace
      add all-source-os/all-source` → `/plugin install mammoth` → approve prime MCP →
      tools appear. Don't list until this passes end-to-end (the README's
      verify-before-listing rule).
- [ ] **Push `main`** — the marketplace resolves `source: ./plugin/mammoth` from
      the repo, so commits must be pushed first.

## Publish steps (owner, after checklist green)

1. `git push origin main` (the marketplace reads the manifest from the repo).
2. Tag a release if the marketplace keys off tags (follow the repo's immutable-tag
   policy; scoped tag if SDK-style versioning applies).
3. Announce add path: `/plugin marketplace add all-source-os/all-source`.
4. Then bead `t-dc63` (launch) — blog + X/HN threads are drafted and ready.

## Not in scope here

Pricing/metering and gateway abuse controls for the hosted tier are owner
decision `t-a238`, separate from getting the (free, local-first) plugin listed.
