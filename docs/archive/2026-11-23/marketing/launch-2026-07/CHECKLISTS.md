---
title: "Launch Checklists — execute in order"
status: CURRENT
last_updated: 2026-07-17
---

# Launch Checklists

Work top to bottom. Don't post anything in §4+ until §0 blockers are clear —
inconsistencies get caught on HN/Reddit and cost credibility. Copy for every
channel is in the `copy-*.md` files. Numbers: `00-LAUNCH-PLAN.md §2`.

---

## §0 — BLOCKERS (clear before ANY post)

**P0 (RESOLVED 2026-07-19):**
- [x] **License** unified to Apache-2.0 (community) + BSL 1.1 (enterprise) across site (`config.ts`); README already matched; all `copy-*.md` updated off MIT. ⚠️ `apps/core/LICENSE` still MIT — reconcile that file separately if desired.
- [x] **MCP tool count** set to **73** (was 43/61, both stale) across site + README + copy. Exposed range 45–73 by config; Prime separate (19).

**P1 (fix before Product Hunt):**
- [x] **Currency = GBP (settled).** LemonSqueezy store is GBP; copy states £18.99. Site is catalog-driven (always matches checkout).
- [ ] **Verify one-command install** on a clean machine: `docker compose -f docker-compose.community.yml up -d`.
- [ ] **Verify the Claude Desktop MCP snippet** against `/install` (stdio command/args) — used in `copy-mcp-lists-discord.md §4`.

---

## §1 — Repo polish (social proof floor; do before soft launch)
- [ ] README hero matches the site: "durable memory for AI agents," the 4 numbers, one-command install ABOVE the fold.
- [ ] README top links: site, `/vs/mem0`, demo video, `/install`.
- [ ] Pin a GitHub Discussion: "AllSource is live — questions here" (`copy-mcp-lists-discord.md §2`).
- [ ] Add repo topics: `ai-agents`, `agent-memory`, `mcp`, `event-sourcing`, `rust`, `vector-search`.
- [ ] Warm the star count off 4: ask ~10 people in your network to genuinely check it out (not "please star").
- [ ] LICENSE + tool-count fixes from §0 reflected in README.
- [ ] Decide: create a real **Discord server**? (Current "Discord" link → GitHub Discussions. Fine for launch; a server helps retention. Optional.)

## §2 — Assets (reused across every channel)
- [ ] **2-min hero video:** events stream in → Claude asks "what did the user do yesterday at 3pm?" → answer renders from events → **"returned in 11.2μs ✓"** → click to reveal **provenance chain**.
- [ ] **~15s GIF** of the same, for Reddit/X/Discord inline.
- [ ] 5-tier pricing screenshot.
- [ ] Architecture/latency diagram (WAL + Parquet + DashMap → 11.9μs).
- [ ] Comparison table image: AllSource vs mem0/Zep/Letta (provenance · time-travel · durable · recall latency).

---

## §3 — Soft launch (Day 0–2, low risk)
- [ ] Open awesome-list PRs (`copy-mcp-lists-discord.md §1`): awesome-mcp-servers, awesome-ai-agents, awesome-rust (databases). One PR each.
- [ ] Post the GitHub Discussions announcement (`§2` of that file).
- [ ] Watch for friction reports; fix fast. This is your dress rehearsal.

## §4 — Reddit (sequence; NEVER cross-post identical text)
- [ ] **Day 1 — r/rust** (substrate angle, `copy-reddit.md`). Warmest crowd first. Seed a technical first-comment (WAL recovery / DashMap benchmark).
- [ ] **Day 2 — r/LocalLLaMA** (the DEMO post). Lead with the GIF, not pricing. Don't mention hosted tiers unless asked.
- [ ] **Day 2–3 — r/LLMDevs** (event-log-vs-vector architecture angle).
- [ ] **Week 2 — r/AI_Agents** (provenance/audit angle, long tail).
- [ ] For each: confirm account has comment history, disclose you're the builder, reply to every comment for 2h, no upvote-begging.

## §5 — Framework Discords (Day 3)
- [ ] Post tailored one-liners (`copy-mcp-lists-discord.md §3`) in the right channel of: Cursor / Cline / Continue, LangChain / LlamaIndex, Latent Space / AI Engineer, MCP community. One message each, engage after.

## §6 — Product Hunt (peak — Tue/Thu, 12:01am PT)
**Warm-up (week before):**
- [ ] Lock date (Tue–Thu). Add to calendar.
- [ ] Line up 10–20 people to "check it out + leave an honest question at launch" (never "upvote").
- [ ] Hero video finalized (§2). Gallery assets staged.
- [ ] First comment + canned replies loaded (`copy-product-hunt.md`).
- [ ] Newsletter pitches sent 2–3 days ahead (§8).
- [ ] X launch thread staged to fire launch morning (`copy-x-threads.md §A`).

**Launch day:**
- [ ] Ship at 12:01am PT. Post first comment immediately.
- [ ] Pin the tweet + fire the X thread linking PH.
- [ ] Block 6+ hours: reply to EVERY comment within minutes (PH rewards maker engagement).
- [ ] Drop the GIF/demo in replies liberally.
- [ ] Send hunters the thank-you DM post-launch.

## §7 — X / Twitter (@ddonprogramming)
- [ ] Launch thread on PH morning (`copy-x-threads.md §A`).
- [ ] Set pinned tweet (`§B`).
- [ ] Week 1/2/3: one hook thread each (numbered → benchmark → contrarian, `§C`).
- [ ] Daily: reply-guy on agent-memory / mem0 / MCP threads — one helpful point, no copy-paste (`§D`).

## §8 — Newsletters (time to PH week)
- [ ] TLDR AI, Console.dev, Latent Space — send 2–3 days BEFORE PH (`copy-newsletters.md`).
- [ ] This Week in Rust (PR/suggest), The Changelog (news submit + pod pitch) — can trail a few days.

## §9 — Long tail (Week 2–3)
- [ ] dev.to + Hashnode: repurpose the "12μs / event-log-as-memory" post (canonical back to blog for SEO).
- [ ] LinkedIn: the compliance/audit angle (provenance = fin/health/legal), link `/solutions/agent-memory`.
- [ ] Nominate for Crate of the Week (This Week in Rust) if not picked up.

---

## §10 — Measure (check daily launch week, then weekly)
- [ ] GitHub stars (baseline 4 → target 100+ launch week)
- [ ] Signups / trials started (from `/signup`)
- [ ] Traffic by source (which channel converted — Plausible/GA)
- [ ] PH rank + comment count
- [ ] Top-performing post per channel → double down, drop the rest
- [ ] Qualitative: the recurring objection in comments → fold the answer into the site FAQ + next copy

## §11 — Do-NOT (anti-patterns)
- ❌ Same copy on HN + Reddit + PH → auto-flagged as spam.
- ❌ Launching PH before the frictionless install works → wasted spike.
- ❌ Upvote-begging anywhere → bans + reputational hit.
- ❌ Ignoring the hard question ("why not mem0?") → answer it head-on (that's what the /vs pages are for).
- ❌ Posting with the license/tool-count/currency drift unresolved → the one commenter who notices tanks the thread.
- ❌ Leaving a channel un-monitored for the first 2h after posting.
