---
title: "Launch Plan — Memory for the AI-agents era"
status: CURRENT
last_updated: 2026-07-17
owner: "@ddonprogramming"
---

# AllSource Launch — "Memory for the AI-agents era"

Show HN is posted. This plan covers **everything after HN**: repo polish →
awesome-lists → Reddit → Product Hunt → newsletters → sustained X cadence →
long-tail content. All copy is grounded to the canonical facts below — every
number traces to `apps/web/src/lib/config.ts` (`siteConfig`) or the repo.

---

## 1. Positioning (the wedge)

**Category:** memory for AI agents. **Wedge:** everyone else is a vector DB with a
wrapper — memory that overwrites, can't explain itself, and is too slow to hit
every turn. AllSource is an **event store as the agent's memory**: durable,
ordered, and fast enough to query on every message.

- **Hero:** *"Your agents already forget. Stop letting them."*
- **One-liner:** durable memory for AI agents — records every event your agent emits, recalls it in ~12μs, survives restarts.
- **Four claims only AllSource can make** (vs mem0/Zep/Letta):
  1. **Provenance** — every memory traces to the source event ("why does the agent believe this?" is answerable).
  2. **Time-travel** — recall the agent's memory as-of any past moment.
  3. **Durable** — WAL + Parquet; survives restart, no silent loss.
  4. **Auditable** — full event log → deployable in regulated (fin/health/legal) settings.
- **The demo that closes it** (reuse everywhere — PH hero, X video, Reddit GIF):
  agent forgets → restart → **recall in 11.2μs** → **show the provenance chain**.
  Matches the homepage right-pane spec.

---

## 2. Canonical facts — SINGLE SOURCE (all copy must match)

| Fact | Value | Source |
|---|---|---|
| Ingest throughput | **469K events/sec** | `siteConfig.stats[0]` |
| Recall latency | **11.9μs p99** (~12μs; demo stamps **11.2μs**) | `siteConfig.stats[1]`, `.recallLatency` |
| MCP tools | **73** | `siteConfig.stats[2]` — all tools (45 read-tier / 55 read+write) |
| Footprint | **~129MB** | `siteConfig.stats[3]` |
| Durability | WAL (CRC32 + fsync) + Parquet (Snappy) + DashMap | CLAUDE.md |
| Pricing (GBP) | Self-Host **Free** · Indie **£18.99** · Studio **£78.99** (Popular) · Scale **£298.99** · Enterprise custom | LemonSqueezy store (GBP) |
| x402 overage | **$0.0001/call** | `siteConfig.pricing[*].x402` |
| License | **Apache-2.0** (community) · **BSL 1.1** (enterprise) | root `LICENSE`, README |
| Repo | github.com/all-source-os/all-source | `siteConfig.links.github` |
| Site | https://www.all-source.xyz | — |
| X handle | **@ddonprogramming** | `siteConfig.twitterHandle` |
| Contact | sales@wolventech.com | `siteConfig.links` |
| Comparison pages (LIVE — link these) | `/vs/mem0` `/vs/letta` `/vs/zep` `/vs/stoolap` `/event-sourcing-for-ai-agents` | `siteConfig.footer` |
| Prime | `/prime` — persistent agent memory via MCP, install in 30s, no embedding API | `siteConfig.header` |

**Mandatory outbound closer:** `Self-host free. £18.99/mo hosted. Apache-2.0. → all-source.xyz`

---

## 3. BLOCKERS — fix before any post goes out

These are consistency bugs that will get caught and cost credibility on HN/Reddit.

### P0 (RESOLVED 2026-07-19)
- [x] **License unified to Apache-2.0 (community) + BSL 1.1 (enterprise).** Site FAQ (`apps/web/src/lib/config.ts`) + all launch copy updated off "MIT"; README already matched. ⚠️ Remaining: `apps/core/LICENSE` is still MIT (a per-crate license inside the Apache-2.0 repo) — reconcile that file separately if you want the core crate under Apache-2.0.
- [x] **MCP tool count set to 73** across site + README + copy (was 43/61, both stale). Real exposed counts from `list_tools()`: 45 read-tier, 55 read+write, 73 full (incl. control-plane/admin). Prime is separate (19 tools; README updated 13→19).

### P1 (should fix — hurts conversion)
- [x] **Currency = GBP (settled).** LemonSqueezy store is GBP; the site is catalog-driven so every price surface renders the real £ charge (Indie £18.99). Launch copy now states £. US visitors see £ — accepted.
- [ ] **Repo has 4 stars.** Social proof floor. Warm up before Product Hunt (see checklist).
- [ ] **Verify the one-command install** on a clean machine and the exact Claude Desktop MCP snippet (from `/install`). Every channel links to it.

---

## 4. Channel sequence

Staggered — never cross-post identical copy (HN/Reddit auto-flag it).

| When | Channel | Copy file | Goal |
|---|---|---|---|
| **Now (done)** | Show HN | — | posted |
| **Day 0–2 (soft)** | Repo polish + awesome-lists PRs + GitHub Discussions post | `copy-mcp-lists-discord.md` | evergreen inbound, fix friction |
| **Day 1** | r/rust | `copy-reddit.md` | substrate/perf crowd (warm) |
| **Day 2–3** | r/LocalLLaMA + r/LLMDevs | `copy-reddit.md` | core ICP — the demo post |
| **Day 3** | Framework Discords (Cursor/Cline/LangChain/Latent Space) | `copy-mcp-lists-discord.md` | high-intent, MCP-native |
| **Day 4–5 (peak)** | **Product Hunt** (Tue–Thu) + newsletter pitches timed to it | `copy-product-hunt.md` + `copy-newsletters.md` | reach spike |
| **Launch morning** | X launch thread + pinned tweet | `copy-x-threads.md` | amplify PH |
| **Week 2–3 (long tail)** | X weekly hooks · dev.to repurpose · r/AI_Agents · LinkedIn compliance angle | `copy-x-threads.md` | SEO + durable reach |

---

## 5. Execute

Everything above becomes actions in **[`CHECKLISTS.md`](./CHECKLISTS.md)**.
Copy lives in the `copy-*.md` files. Keep this plan as the source of truth for
numbers — if a fact changes, change it here first, then propagate.
