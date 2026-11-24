# Matched #7183 recovery/no-reload evidence

[Superseded report](../../../archive/2026-11-23/outreach/2026-10-20-omo-matched-no-reload-evaluation.md) contain findings and limits. results-summary.json recomputes native workflow scores and includes complete summaries and recovery hashes. match-manifest.json proves fixture/config matching and names the single treatment toggle. a1 is recovery off; b1 is recovery on.

- shared-fixture.json: identical skill/query/receipt data used by both arms.
- *-stage-*.json: native completed/error tool parts and actual model answers.
- *-compaction-*.json: native full message snapshots after each compaction.
- *-hooks.jsonl: actual native execution completions, reload denials before native invocation, scope-policy denials and delegated hooks.
- *-manifest.json / *-effective-config.json: requested and effective configuration.
- *-requests.json / *-identity.json: actual HTTP inputs and native source/session mapping.
- *-compaction-events.txt: native SSE connection and compaction frames.
- *-server.log / *-server.err: isolated runtime logs.
- matched-run.ts / matched-observer.ts: final controller and policy wrapper; no production source patch.

## Reproduce

Use pinned PR 93d44a0760448f7f07d2795c6239d51db5ab6576 and [full-bundle setup](../2026-10-20-omo-full-plugin/README.md). Paths use /tmp/allsource-omo-compaction; adapt them consistently if needed. Put scripts at that root and OpenCode 1.18.22 at runtime/opencode. Supply OPENROUTER_API_KEY securely through environment; a local-profile fallback was used in this evaluation.

Keep one shared fixture for the pair. The controller creates it once at matched/shared-fixture.json. To reproduce the retained query values, copy shared-fixture.json there before either arm. Use fresh slot directories and run serially under separate 285-second outer deadlines:

```text
QA_SLOT=fresh-a QA_RECOVERY=off bun matched-run.ts
QA_SLOT=fresh-b QA_RECOVERY=on bun matched-run.ts
```

The model performs the first skill load. The wrapper delegates that native execution, latches afterward, and denies subsequent calls before native skill code runs. Skill remains visible so denial is observable. Searches are constrained to the current source and opaque query syntax, without consulting or revealing the correct answer. Ordinary file tools, other-session fallback and MCPs are unavailable in both arms.

Actual timing and model outputs vary. Verify second event inside 60,000 ms and third beyond it; do not assume that temperature 0 guarantees deterministic replay. The off arm disables the whole native context-injector hook, not only skill reinjection. TypeScript is required to wrap the real plugin API without replacing it with another implementation. Parent owns/reaps each isolated server. Cookies/account identifiers are redacted from retained records; synthetic fixture content is retained.
