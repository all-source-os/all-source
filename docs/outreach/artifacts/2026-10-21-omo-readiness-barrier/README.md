# Readiness-barrier #7183 evidence

[Superseded report](../../../archive/2026-11-23/outreach/2026-10-21-omo-readiness-barrier-evaluation.md); results-summary.json contains recomputed scores and controls.

- b1a on, b1b off; b2a off, b2b on. Run serially in that order.
- *-barrier-0/1/2/3.json: every poll, expected count/state, native hook/HTTP acceptance times, release time and stored message IDs.
- *-ready-messages-*.json: full native pre-task messages at barrier release.
- *-stage-*.json: actual native tool results and assistant text; task creation must follow release; no recovery may arrive in the turn delta.
- *-compaction-*.json: post-barrier native summaries and stored recovery snapshots.
- *-hooks.jsonl: actual full-plugin execution, reload denials, dispatch before/after bodies, HTTP status and summary injection.
- *-source-integrity.json: post-test native OpenCode read-only SELECT results.
- *-native-checkpoint-log.json: exact-subject native checkpoint capture/restoration records and chronology checks.
- Manifests/effective configs, native SSE receipts, server logs, pair fixtures and native dependency lockfiles retained.

## Reproduce

Use pinned full-plugin bundle/runtime from [setup](../2026-10-20-omo-full-plugin/README.md). Put scripts at /tmp/allsource-omo-compaction, fixture files under readiness/, runtime at runtime/opencode. Supply OPENROUTER_API_KEY securely. Controller seeds native dependencies from earlier matched/runs/b1; adapt to a verified local @opencode-ai/plugin 1.18.22 install if needed. Source/session databases are not copied.

```text
QA_PAIR=b1 QA_SLOT=fresh-1 QA_SKILL_TEXT=on bun readiness-run.ts
QA_PAIR=b1 QA_SLOT=fresh-2 QA_SKILL_TEXT=off bun readiness-run.ts
QA_PAIR=b2 QA_SLOT=fresh-3 QA_SKILL_TEXT=off bun readiness-run.ts
QA_PAIR=b2 QA_SLOT=fresh-4 QA_SKILL_TEXT=on bun readiness-run.ts
```

Separate 285-second outer deadlines; never concurrent. Adapt analyzer/source-check slot labels consistently for a replay. Barrier is capped at 20,000 ms and only reads native status/messages and observer logs. Baseline requires initialized idle session; rounds 1/3 require accepted and stored recovery; round 2 requires completed handler and suppressed dispatch inside cooldown. A barrier timeout aborts the arm. No forced recovery or clock override. Parent owns and reaps native server.

The TypeScript wrapper uses the real native plugin API and HTTP dispatch; production source is unchanged. Reload stays visible but every post-initial execution is rejected before native skill code, including cached calls. Policy permits only current-source inspection and opaque-format queries, without an answer oracle. Native source checks run only after model tests end. Secrets/cookies/account identifiers are redacted from retained records.
