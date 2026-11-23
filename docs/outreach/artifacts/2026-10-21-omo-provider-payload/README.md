# Provider-payload #7183 evidence

[Report and current unsent draft](../../2026-10-21-omo-provider-payload-evaluation.md); provider-analysis.json indexes every captured call by phase/stage, message role, skill markers, query map and body hash. results-summary.json combines workflow and control results.

- *-provider-NNNN.json: captured UTF-8 provider request body, preserved without reformatting so SHA-256 matches receipt. No authorization headers or credentials retained.
- *-hooks.jsonl: provider-request/provider-response receipts, native recovery before/after control and tool/gate events.
- *-stage-*.json: actual native tool results, answers and model-step parts used for capture coverage checks.
- *-compaction-*.json: native generated summaries, independently compared with captured summary inputs and continuation requests.
- *-barrier-*.json / *-ready-messages-*.json: pre-task settlement polls and native storage snapshots.
- *-native-checkpoint-log.json / *-source-integrity.json: exact-subject native capture/restoration chronology and read-only source checks.
- Frozen scripts, fixture pairs, dependency locks, effective configs and native runtime/SSE logs included.

## Reproduce

Use pinned bundle/runtime from [full-plugin setup](../2026-10-20-omo-full-plugin/README.md). Place scripts at /tmp/allsource-omo-compaction, fixtures under provider/, runtime at runtime/opencode. Supply OPENROUTER_API_KEY securely. Native dependencies are seeded from earlier matched/runs/b1; adapt to a verified @opencode-ai/plugin 1.18.22 installation if needed. No session database is copied.

```text
QA_PAIR=c1 QA_SLOT=fresh-1 QA_SKILL_TEXT=on bun provider-run.ts
QA_PAIR=c1 QA_SLOT=fresh-2 QA_SKILL_TEXT=off bun provider-run.ts
QA_PAIR=c2 QA_SLOT=fresh-3 QA_SKILL_TEXT=off bun provider-run.ts
QA_PAIR=c2 QA_SLOT=fresh-4 QA_SKILL_TEXT=on bun provider-run.ts
```

Run serially with separate 285-second outer deadlines. Adapt analyzer slot names consistently. Observer delegates unchanged readiness-observer.ts and wraps outbound OpenRouter fetch observationally; original request and options go to original fetch. Capture failure or request/native-step mismatch aborts the arm. Readiness barrier remains capped at 20,000 ms; immediate round confirms cooldown suppression rather than forcing recovery. Model loads skill once; later reloads denied before native execution.

Inspect first continuation requests separately from later tool-result calls. Provider analysis checks complete fixture body, three literal procedure clauses, exact summary/tool-output matches, and current query across the entire JSON body, including tool-call arguments and schemas. Literal markers do not prove every semantic instruction or provider-internal model context. Parent owns/reaps native server. Production source unchanged; TypeScript wraps the real plugin API. Native source queries run after all model tests.
