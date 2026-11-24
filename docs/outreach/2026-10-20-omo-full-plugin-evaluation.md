# Oh-my-openagent #7183: full-plugin compaction evaluation

Date: 2026-10-20. Status: full-plugin loader gap resolved; reported integration cases completed; outreach unsent.

Current workflow qualification: [provider-captured rerun](./2026-10-21-omo-provider-payload-evaluation.md). Earlier [model-selected skill and specialized-tool results](../archive/2026-11-23/outreach/2026-10-20-omo-model-selected-evaluation.md) cover the prior programmatic-loading limit. Four checkpoints passed with a skill reload after the second compaction. Large-payload results below remain a separate experiment.

## Findings and shortlist

**The full OMO plugin restores the skill, suppresses an immediate duplicate, and restores the same skill again after cooldown. Large recovery messages remain untruncated at the tested per-skill and aggregate sizes.** These results support the bounds/one-shot concern already raised on the owned PR. They establish neither a current data-loss incident nor demand for an AllSource integration.

Oh-my-openagent remains first **technical contribution** candidate among the three evaluated projects. Offer the existing author regression evidence after coordinating prior project contact. Hold a design-partner invitation until an affected builder establishes a requirement beyond the native checkpoint fix. Nanobot remains a retrieval/test contribution; Hermes remains held for real-provider qualification.

## Loader gap: cause and repair

The previous QA bundle failed during import, before its server callback could run:

```text
ENOENT: no such file or directory, open '.../source/skills/frontend/SKILL.md'
```

The actual full plugin bundle reproduced the same packaging error under `dist/skills/frontend/SKILL.md`. `packages/shared-skills/index.mjs` resolves skill assets relative to the consuming bundle. Bundling JavaScript alone omitted those runtime assets. This was a QA packaging defect, not evidence that OpenCode cannot load #7183.

Repair: build the actual `packages/omo-opencode/src/index.ts` entry using the repository's Bun index-build arguments, then stage the PR archive's real `packages/shared-skills/skills` directory at `dist/skills`. No placeholder skill asset, product source patch, injected dependency implementation or `createPluginModule` override was used. The CLI/Codex/LSP release build graph was not required for this OpenCode compaction path.

Native OpenCode then imported the full module, called its default server export, registered **14 OMO tools** and **15 hook/object entries**, and invoked the OMO config hook. Startup succeeded on **1.18.22 and 1.19.3**. The five reported compactions ran on **1.18.22**, matching the PR's plugin/SDK dependency version. Runtime 1.19.3 received a loader/startup check, not the compaction matrix.

One preliminary full-plugin run placed legacy settings at the unified config's top level, which produced a migration validation warning. The reported matrix uses schema-valid `[opencode]` settings; preliminary data is excluded. An OpenRouter credit-verification retry in that preliminary run is not scored as a memory failure.

## Native execution boundary

- Pin: [#7183](https://github.com/code-yeongyu/oh-my-openagent/pull/7183), **93d44a0760448f7f07d2795c6239d51db5ab6576**, still open/unmerged on October 20. Bun 1.4.3; real OpenRouter `openai/gpt-4.1-mini`, requested output capped at 256 tokens.
- Native loader loads an observation wrapper, which imports the full compiled default plugin and delegates every returned hook. The observer records hook entry/exit; it does not construct an injector, call capture/restore, forward SSE into recovery, replace managers, or disable compaction behavior.
- Fixture `SKILL.md` files are discovered by the actual initialized OMO skill tool. On a controlled `chat.message`, the observer invokes that registered tool's `execute` method once per fixture. Skill selection and its permission callback are programmatic; no claim of model-selected skill loading or host tool-output-limit coverage is made.
- Thereafter the driver only calls native HTTP summarize/message endpoints. OpenCode itself invokes OMO `experimental.session.compacting`, message/system transforms, model parameter/header hooks, and its `event` hook on native `session.compacted`. Native hook receipts and successful summaries prove the full compaction route. No external recovery bridge remains.
- All reported compactions produced completed native summary messages without terminal model errors. Recovery messages were saved through the native SDK; the immediate second compaction added no new recovery message. Full message text was read back through native HTTP APIs, checked byte-for-byte against every fixture body, and hashed.
- Each case used a separate temporary HOME/XDG/config/data tree and parent-owned loopback server. Production configuration and stores were not used. Websearch/context7/grep_app, telemetry and auto-update checks were configured off; compaction hooks remained enabled. Native effective configuration confirms an LSP MCP registration remains, which this summary path did not exercise. Existing OpenRouter credential was passed in child environment and excluded from evidence.

## Repeated compaction and payload bounds

One **1,024-byte** fixture was loaded once. Native skill loading adds its source-directory guidance; each stored recovery message measured **1,453 bytes**.

| Compaction | Completed summaries | Stored recovery messages | Result |
| --- | ---: | ---: | --- |
| First | 1 | 1 | Complete body restored. |
| Immediate second | 2 | 1 | Same stored recovery ID; duplicate suppressed. |
| Third, after cooldown | 3 | 2 | New recovery ID; identical complete recovery text, no skill reload. |

First and third native compaction-event entries were **70,819 ms** apart. The driver waited 62,000 real milliseconds before the third summarize request; no clock mocking or cooldown override was used. Thus the cooldown does not provide one-shot semantics.

| Fixture body size | Actual stored recovery text | Integrity | Bound observed |
| --- | ---: | --- | --- |
| One 1,048,576-byte body | **1,049,000 bytes** | Full body and end marker | No per-skill truncation through 1 MiB. |
| Eight 65,536-byte bodies; 524,288 total | **526,127 bytes** | All eight full bodies and end markers | No aggregate truncation through 512 KiB. |

These measurements include source-directory instructions, wrappers and internal markers. They differ from narrower formatter-only and external-bridge measurements for that reason. They are byte counts, not tokenizer estimates. Stored historical duplicates do not establish that every old copy remains in the model's active context forever.

Limits: no pressure-triggered automatic compaction, model-selected skill/tool workflow, specialized tool execution after compaction, downstream model acceptance of the 1 MiB recovery, restart durability, browser/TUI or whole release-package test. The native autocontinue hook was registered; these manual `auto:false` runs did not exercise it. No production source was changed, so no upstream typecheck claim is made.

## Ownership and prior contact

PR author **AceRothstein71** owns the restoration implementation. [MoerAI's existing review](https://github.com/code-yeongyu/oh-my-openagent/pull/7183#issuecomment-5489222380) already requests per-skill and total caps or one-shot recovery, plus repeated-compaction regression tests. Current API check found the same open revision and comment. Do not frame this as a newly discovered defect or competing fix.

The [October 19 ownership/contact audit](./2026-10-19-omo-compaction-evaluation.md#ownership-and-prior-contact) documents the existing AllSource approach to DevNewbie1826 on [#7764](https://github.com/code-yeongyu/oh-my-openagent/issues/7764#issuecomment-5616313504), local-ledger check and connected Gmail searches. That is a different issue in the same project; no response or consent was established by that audit. Coordinate it before another project approach. No new outreach was sent during this evaluation.

## Current outreach draft: unsent

Use the [current outreach draft](./2026-10-21-omo-send-ready.md#exact-comment). Existing ownership and prior-contact constraints still apply.

## Evidence and progress

[Artifact index](./artifacts/2026-10-20-omo-full-plugin/README.md) and [machine-readable summary](./artifacts/2026-10-20-omo-full-plugin/results-summary.json) contain source/build hashes, full-plugin receipts, native messages, compaction SSE receipts, server logs and reproduction code. The earlier [external-bridge report](./2026-10-19-omo-compaction-evaluation.md) remains historical evidence; this report supersedes its loader gap and draft.

Loader cause reproduced; real assets staged; native startup checked on two runtimes; full-plugin repeated-compaction and payload matrix completed; docs index, shortlist and current draft updated. No branches, commits, pushes or outbound communication created.
