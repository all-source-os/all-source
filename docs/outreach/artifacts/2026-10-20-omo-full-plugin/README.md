# Full OMO plugin compaction evidence

[Report and unsent draft](../../2026-10-20-omo-full-plugin-evaluation.md) contain findings and limits. results-summary.json records verified fixture integrity, native hook counts, source/build hashes and runtime versions. *-round-*.json contain full native messages; *-hooks.jsonl record actual delegated full-plugin hooks; *-compaction-events.txt retain native SSE connection/compaction frames. Server logs and isolated OMO configs accompany each case. effective-config-summary.json confirms remaining LSP registration.

## Reproduction

Use pinned PR 93d44a0760448f7f07d2795c6239d51db5ab6576. The harness paths use /tmp/allsource-omo-compaction; adapt them consistently for another workspace. Install locked source dependencies with lifecycle scripts disabled. Build/stage sequentially:

```text
bun build packages/omo-opencode/src/index.ts --outdir dist --target bun --format esm --external zod
cp -R packages/shared-skills/skills dist/skills
```

Place full-plugin.ts and full-run.ts at the harness root. Supply OpenCode 1.18.22 at runtime/opencode. The observer imports source/dist/index.js. Supply OPENROUTER_API_KEY securely through the environment; the local-profile fallback was used on the evaluation machine. Do not print or commit credentials. Use fresh QA_RUN names. Run serially under a 300-second outer deadline:

```text
QA_RUN=repeated-valid QA_ROUNDS=3 bun full-run.ts
QA_RUN=single-1m QA_SIZES='[1048576]' bun full-run.ts
QA_RUN=multi-512k QA_SIZES='[65536,65536,65536,65536,65536,65536,65536,65536]' bun full-run.ts
```

Bun/TypeScript calls the upstream plugin API directly; rewriting the harness in Rust would exercise different modules. The observer delegates full hooks and logs entry/exit. Only skill fixture loading is programmatic; capture, native compaction and recovery are performed by the loaded full plugin. Clean config uses the [opencode] block. LSP tools, native tool output truncation, automatic pressure selection and downstream large-context generation remain outside this matrix.
