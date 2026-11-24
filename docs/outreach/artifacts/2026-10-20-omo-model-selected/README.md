# Model-selected skill and specialized-tool evidence

[Superseded report](../../../archive/2026-11-23/outreach/2026-10-20-omo-model-selected-evaluation.md) contain findings and limits. results-summary.json scores the four checkpoints from native tool parts and answers. Native *-stage-*.json files include exact model calls, actual tool results and replies; *-compaction-*.json preserve summary/recovery history. Hook JSONL records delegate-only full-plugin observations. Fixture, identity, controller request and effective-config files accompany every attempt. Error cookies and account identifiers are redacted. Other message content is retained.

Four attempts are preserved: initial mini rejection; nano partial run with wrong queries and later provider interruption; short-prompt mini rejection; final MiniMax run. These attempts used different prompts/procedure wording and are not a controlled benchmark. Earlier effective configs, fixture bodies and request records are authoritative for their inputs; the retained executable is the final harness version.

## Reproduce final case

Use pinned PR 93d44a0760448f7f07d2795c6239d51db5ab6576 and the staged full bundle described in [prior artifact setup](../2026-10-20-omo-full-plugin/README.md). Place model-run.ts and model-observer.ts at /tmp/allsource-omo-compaction or adapt paths consistently. Use OpenCode 1.18.22 at runtime/opencode. Supply OPENROUTER_API_KEY securely through environment; the existing local-profile fallback was used during evaluation. Use a fresh QA_RUN, a free loopback port 43822, and a 285-second outer timeout.

```text
QA_RUN=model-reproduction QA_MODEL=minimax/minimax-m3.1-flash:free bun model-run.ts
```

The harness installs a real skill and decoy, seeds four source sessions via no-reply HTTP writes, then sends normal model prompts. It never executes a skill or specialized tool directly. The observer delegates the full plugin. Native summaries and recovery run in OpenCode. Queries/receipts are randomized per run, so IDs and exact outputs differ. The middle checkpoint may trigger reload; record actual calls rather than assuming this outcome. Model/provider availability and nondeterminism can change results.

TypeScript exercises the upstream native API and host protocol directly; a Rust rewrite would not load these same JavaScript modules. Parent owns/reaps its server; credential values are excluded from retained records. LSP and other MCPs are disabled to avoid unrelated runtime installs. Large-payload and automatic-pressure workflows remain separate limits.
