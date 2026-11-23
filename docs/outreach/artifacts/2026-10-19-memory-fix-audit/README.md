# Memory contribution artifacts

Date: 2026-10-19. All files local; no publication or sending authorised.

[Live execution results and new harnesses](../../2026-10-19-live-memory-evaluation.md) supersede prepared-only statuses in this pack.

## Runnable oh-my-openagent measurement

[omo-payload-probe.ts](./omo-payload-probe.ts) runs the actual formatter and session store from PR #7183 at commit `93d44a0760448f7f07d2795c6239d51db5ab6576`. Source hashes must match before either module is imported. Native TypeScript executes the project's own helpers; a Rust rewrite would measure different code.

From AllSource root, supply a checkout or extracted source directory containing the two pinned files at their original paths:

```console
gtimeout -k 5 60 bun run docs/outreach/artifacts/2026-10-19-memory-fix-audit/omo-payload-probe.ts /absolute/path/to/oh-my-openagent
```

Inspected sources: [formatter](https://github.com/code-yeongyu/oh-my-openagent/blob/93d44a0760448f7f07d2795c6239d51db5ab6576/packages/omo-opencode/src/hooks/compaction-context-injector/skill-reinjection.ts) and [session store](https://github.com/code-yeongyu/oh-my-openagent/blob/93d44a0760448f7f07d2795c6239d51db5ab6576/packages/omo-opencode/src/shared/session-loaded-skills.ts). No upstream source copies are included in this pack. The probe needs Bun; it does not install dependencies, fetch code, start OpenCode or call a model.

Actual run: Bun 1.3.9, isolated copies of those helpers. [Results](./omo-payload-results.json) include hashes and limits. Four controls passed: empty input, latest skill version replaces prior body, session isolation and explicit clear.

| Skills | Body bytes in | Formatted bytes out |
| --- | --- | --- |
| 1 | 1,024 | 1,141 |
| 1 | 65,536 | 65,653 |
| 1 | 1,048,576 | 1,048,693 |
| 8 | 524,288 | 524,580 |

This measures formatter growth. It does not show whether runtime admits those inputs or another budget intervenes. Bytes are not tokens. Calling formatter twice is not a second compaction event. No live model, restart, dispatch, prompt placement or AllSource integration tested. Existing reviewer already raised the budget concern; artifact supplies reproducible evidence, not a new issue or competing implementation.

## Nanobot evaluation corpus

[nanobot-correction-corpus.json](./nanobot-correction-corpus.json) supplies three cases: correction over repeated inference, scoped preference and a delivery-reference control. Each includes source messages, expected values/source IDs, and a two-cycle execution contract. [Live-provider evaluation completed](../../2026-10-19-live-memory-evaluation.md); results are linked from the corpus.

Native interfaces at audited main `8778c697744f8334c5794cd37502e8eb77ee5585`: reuse MemoryStore and provider/runtime fixtures in tests/agent/test_consolidator.py, LLMRuntime.capture, consolidator.archiver.archive, project Dream execution and fresh-session recall. The corpus is not a ready-to-drop pytest implementation.

Corpus message IDs are evaluation metadata, not claimed nanobot fields. Record raw messages independently. Score factual correctness, source attribution and scope separately; unsupported attribution is unsupported, not a fabricated citation. Preserve actual prompts, summaries, memory files, versions and outputs. Mocked summaries prove plumbing only. Any AllSource comparison must use identical input events and disclose additional metadata.

## Hermes destination probe

[hermes-provider-outage-probe.md](./hermes-provider-outage-probe.md) defines a disposable-container procedure and unfilled result record. [Executed with a synthetic provider](../../2026-10-19-live-memory-evaluation.md). Production vendor migration remains untested.

Prepared procedures cannot be described as observed results. This pack establishes no working AllSource adapter or end-to-end customer outcome.
