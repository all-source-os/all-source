# Counterbalanced skill-only #7183 evidence

[Report and unsent draft](../../../archive/2026-11-23/outreach/2026-10-20-omo-skill-only-counterbalanced-evaluation.md). results-summary.json contains recomputed scores and control checks.

- s1a: skill text on; s1b: off. Pair s2 reverses order: s2a off; s2b on.
- *-hooks.jsonl: actual recovery-dispatch-control before/after bodies, native HTTP response status, summary-injection-control output and reload gate events.
- *-stage-*.json: actual native tool arguments/results and model answers.
- *-compaction-*.json: native stored recovery messages and generated summaries after each compaction.
- *-manifest.json / *-effective-config.json: identical requested settings and native effective config; credentials redacted.
- *-source-integrity.json: native OpenCode read-only SELECT and verified source bodies.
- Pair fixture files and frozen controller/observer/analyzer are retained beside results.

## Reproduce

Use the pinned full-plugin bundle and runtime from [full-plugin setup](../2026-10-20-omo-full-plugin/README.md). Place scripts under /tmp/allsource-omo-compaction and fixture files under skill-only/. Supply OPENROUTER_API_KEY securely. Use fresh QA_SLOT values and preserve pair order:

```text
QA_PAIR=s1 QA_SLOT=fresh-1 QA_SKILL_TEXT=on bun skill-only-run.ts
QA_PAIR=s1 QA_SLOT=fresh-2 QA_SKILL_TEXT=off bun skill-only-run.ts
QA_PAIR=s2 QA_SLOT=fresh-3 QA_SKILL_TEXT=off bun skill-only-run.ts
QA_PAIR=s2 QA_SLOT=fresh-4 QA_SKILL_TEXT=on bun skill-only-run.ts
```

Run serially under separate 285-second outer deadlines. Controller seeds native dependency files from the prior b1 profile; adapt that path to a verified local @opencode-ai/plugin 1.18.22 install before replay. No source/session database is copied. The parent owns and reaps each server. Analyzer currently names the retained s1a/s1b/s2a/s2b slots; adapt labels consistently for a replay.

Native hook remains enabled. HTTP-boundary ablation retains agent/model/tools/noReply, markers and metadata; the model loads skill once, then a native execution latch denies reload. Summary-context instructions must hash identically; second compaction events must be inside 60,000 ms and third beyond it. Do not infer matched timing or dispatch interception without checking actual records. Generated summaries are allowed to differ.
