# mammoth — X / Show HN launch thread

Status: DRAFT for bead t-dc63 (P2 launch). Numbers are real (bench2.py, 60-memory
run). Do not publish until the GATE is formally GO and commits are pushed.
Pair each tweet with the noted visual. Cross-link caveman — complementary, not
competitive.

---

## X thread

**1/ (hook — pin the meme + the demo)**
your coding agent has goldfish memory. every session it forgets your decisions,
your gotchas, why you chose X over Y. so you paste it all back. again.

meet mammoth 🦣 — durable memory for any coding agent.

caveman make few token. mammoth never forget token.

[ATTACH: 15-sec screen recording — fresh session, ask "why did we pick local-only?",
agent answers from memory with nothing pasted. The magic moment.]

**2/ (what it is)**
mammoth gives your agent durable, cross-session memory:
• decisions, task history, code context — recalled automatically
• local-first, zero signup (your disk, survives reboots)
• MCP-native: Claude Code, Cursor, Cline, Windsurf, Codex, Gemini

one server, 13 recall tools, no per-agent shim.

**3/ (proof — the bench table)**
claims are cheap. mammoth ships a reproducible benchmark — run it yourself.

recall vs a search+grep baseline, queries worded UNLIKE the stored text:
• hit@5: 0.90 (baseline 0.83)
• recall latency: 3.0ms p50 / 3.6ms p95
• durability (write→restart→read): PASS

[ATTACH: benchmark table image]

**4/ (honesty — the trust move)**
honest bit: on a smaller set the edge over grep was +0.17; at 3x the corpus it
narrows to +0.07 (denser corpus = more keyword overlap for grep). memory still
wins every metric.

we publish the softening instead of hiding it. same move caveman made with
"output tokens only."

**5/ (install — the CTA is a command, not a signup)**
```
cargo install allsource-prime
curl -fsSL https://.../plugin/mammoth/install.sh | bash
```
or in Claude Code:
```
/plugin marketplace add all-source-os/all-source
/plugin install mammoth
```
free, local, no account. cross-machine sync when you want it.

**6/ (compose with caveman + link)**
mammoth doesn't compete with @caveman — they compose. caveman compresses what
your agent says; mammoth persists what it learned. run both: fewer tokens AND a
memory that survives.

repo + benchmark + docs 👇
[LINK: github.com/all-source-os/all-source + blog post]

---

## Show HN

**Title:** Show HN: mammoth – durable memory for coding agents (local-first, MCP-native)

**Body:**
mammoth gives any MCP-capable coding agent (Claude Code, Cursor, Cline, Windsurf,
Codex, Gemini) durable cross-session memory — it remembers decisions, gotchas,
and project context and recalls them automatically, so you stop re-explaining
your project every session.

It's local-first by default: `cargo install allsource-prime` runs a durable
on-disk store (WAL + Parquet, the same AllSource Core engine), no account, memory
survives restarts. Cross-machine/team sync is an opt-in upgrade, not a gate.

Recall is hybrid (in-process fastembed vectors + graph + temporal). There's a
reproducible benchmark in the repo (`tooling/mammoth-bench/`) scoring recall vs a
keyword search+grep baseline on deliberately-reworded queries: hit@5 0.90 vs
0.83, ~3ms recall latency, durability PASS. The edge over grep narrows from +0.17
to +0.07 as the corpus grows — published, not hidden.

It pairs with caveman (token compression): caveman shrinks what the agent says,
mammoth persists what it learned.

Plugin layer is MIT; engines (allsource-prime/core) are Apache-2.0, chronis MIT.

Repo: github.com/all-source-os/all-source
Benchmark: tooling/mammoth-bench/

---

## LinkedIn (1 post)

Coding agents forget everything between sessions. You re-explain your project,
your decisions, your gotchas — every single time.

We built mammoth to fix that: durable memory for any coding agent. It records
decisions and recalls them automatically across sessions. Local-first, zero
signup, works in every MCP-capable editor.

And we benchmarked it honestly — recall beats a search+grep baseline (hit@5 0.90
vs 0.83) at ~3ms, with the edge-narrowing-at-scale published rather than hidden.

caveman make few token. mammoth never forget token.

[link]
