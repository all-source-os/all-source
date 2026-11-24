# Oh-my-openagent #7183: model-selected skill and post-compaction tools

Date: 2026-10-20. Status: requested model-driven workflow evaluated; docs and draft updated; nothing sent.

Current qualification: [matched no-reload results](./2026-10-20-omo-matched-no-reload-evaluation.md) supersede the fallback-dependent interpretation below. Recovery on passed 2/3 post-compaction checkpoints; off passed 0/3. The immediate second checkpoint still failed with reload blocked.

## Result and qualification

**The final controlled run completed all four checkpoints: baseline and one after each of three native compactions. The model selected the audit skill and executed real OMO `session_search` and `session_info` tools. It reloaded the skill after the second compaction.** Therefore the workflow passed with fallback reload available; uninterrupted skill reuse across every compaction was not established.

This extends the [full-plugin loader/payload evaluation](./2026-10-20-omo-full-plugin-evaluation.md). Oh-my-openagent remains an owned **regression-test contribution**, not a qualified AllSource design partner. The patch plus skill-file reload satisfied this synthetic workflow. Existing bounds/one-shot review remains relevant, but no result establishes a need for another memory service. Nanobot remains a retrieval/test contribution; Hermes remains held for real-provider qualification.

## Pin and execution boundary

- [PR #7183](https://github.com/code-yeongyu/oh-my-openagent/pull/7183) remained open at **93d44a0760448f7f07d2795c6239d51db5ab6576** when rechecked. Full compiled default OMO plugin and real packaged skill assets match the prior report's hashes. No product source changed.
- OpenCode **1.18.22**; current harness Bun **1.3.9**. The final model route was **OpenRouter `minimax/minimax-m3.1-flash:free`**, verified through its [model catalog API](https://openrouter.ai/api/v1/models) as tool-capable with zero listed input/output prices. Requested output limit 2,048 tokens, temperature 0, low reasoning. No purchase or account setting change occurred.
- Native loader initialized the complete OMO plugin. An observer delegates its hooks and logs native tool and compaction events. It contains no skill-tool execution, forced tool choice, external recovery bridge, checkpoint setter or injected tool result.
- Installed fixtures: `zephyr-ledger-audit` and an unrelated `canvas-layout` decoy. The initial user task supplied their names and short descriptions; the model chose the matching skill by calling native `skill`. This tests model selection from supplied metadata, not unaided discovery of an unknown installed skill.
- The audit skill alone provided four opaque checkpoint queries and a two-tool procedure. Each user task supplied its checkpoint number and source session ID, without its query or receipt. Four synthetic records were seeded through normal native no-reply message writes in four isolated source sessions. Those writes are fixture setup, not model-produced conversations.
- The model had to search the specified source using the skill's exact checkpoint query, then inspect that source with `session_info`, and return the matching receipt. The driver scored native completed-tool parts, arguments, actual tool outputs and answer text. A model saying it would call a tool did not count.
- The final agent used a short audit prompt and a five-step limit. Only skill loading and the two session tools were allowed; general file access and other tools were disabled. All MCP integrations were disabled and their effective native configuration checked before model requests. Full OMO compaction hooks stayed active.
- Each attempt used a new temporary HOME/XDG/config/data profile. Production configuration/stores were not used. Existing OpenRouter credential entered child environment only. Parent owned and reaped the loopback OpenCode server; no QA server remained at the final process check.

## Native results

| Checkpoint | Position | Model skill calls in checkpoint | Specialized tools | Correct receipt in answer |
| --- | --- | ---: | --- | --- |
| 0 | Before compaction | 1, matching audit skill | Exact-query `session_search`, then `session_info`, completed | Yes |
| 1 | After compaction 1 | 0 | Both completed with correct query/source | Yes |
| 2 | After compaction 2 | 1, same skill reloaded | Both completed with correct query/source | Yes |
| 3 | After compaction 3, beyond cooldown | 0 | Both completed with correct query/source | Yes |

The decoy was never loaded. Final native trace contains **10 completed tool calls**: two `skill`, four `session_search`, four `session_info`. The driver did not execute these tools itself. Returned answers included the correct receipt within the matching source row; they were not always a bare receipt string.

Native synthetic restoration-message counts were **1 → 1 → 2** at the three compaction snapshots, each restoration text **1,302 bytes**. Both recovery texts were identical and contained the complete skill instructions. First and third native compaction-event entries were **78,709 ms** apart, with a real 62,000 ms wait before the third summarize request. No clock mocking or cooldown override was used.

The latest summaries after compactions 1 and 2 retained completed checkpoint details but omitted future checkpoint queries. That is directly visible in the saved summary text. After compaction 2 there was no new restoration message and the model chose to reload the skill before using the next opaque query. This is consistent with a cooldown gap; it does not independently prove the reload's cause. No matched no-recovery or no-file-access control was run. Do not claim that restoration alone carried every successful step.

## Earlier attempts and counter-evidence

| Attempt | Actual observation | Treatment |
| --- | --- | --- |
| `gpt-4.1-mini`, default build prompt | Provider rejected initial prompt before tools: **8,879 > 6,400** allowed input tokens. | Execution limit, not memory failure. |
| `gpt-4.1-nano`, original fixture procedure | Native exact-query search/source inspection worked at baseline and after first compaction. After second, model reloaded skill but used incorrect queries; provider later returned `in_flight_budget_exhausted` 402. | Incomplete workflow with instruction-use failure and provider interruption. Not discarded as a success; not evidence of destroyed memory. |
| `gpt-4.1-mini`, short audit prompt | Provider rejected initial prompt: **6,172 > 3,864** allowed input tokens. | Execution limit, not memory failure. |
| MiniMax zero-price route, short audit prompt and clearer stop instruction | Four checkpoint workflows completed, including middle skill reload. | Final passing controlled run, with the limits below. |

The attempts differ in model, prompt and fixture wording. They are not a controlled model benchmark. The nano result prevents a model-independent reliability claim. Its original baseline did not consistently return a literal receipt in final prose even when the correct tool result was present; the final harness explicitly checks receipt text as well as execution.

## Remaining limits

- One successful conversation with four related checkpoints; no statistical success-rate claim.
- Skill reload was available and occurred after compaction 2. Continuous reuse without reload was not proved, nor was behavior with a missing/changed skill file.
- Supplied skill metadata, a short audit-agent prompt, restricted tool set and synthetic source records. This is not the stock coding-agent prompt or an unprompted production task.
- Specialized tools were actual OMO session tools. No `team_list`/`team_delete`, external MCP workflow, permissions transition or real external side effect was tested.
- Manual `auto:false` summaries. Automatic pressure selection, autocontinue, restart durability and browser/TUI behavior remain untested here.
- Small skill fixture. Earlier 1 MiB and eight-skill payload measurements remain separate tests; this run does not prove that a model accepts those large contexts or uses their tools correctly.
- No causal ablation separates native summaries, restored instructions, prior tool output and reload. Workflow evidence supports the observed actions, not an exclusive explanation of why they succeeded.
- No whole release build/typecheck claim; pinned production source was unchanged. Prior contact and maintainer interest remain qualification constraints.

## Current outreach draft: unsent

Use the [replacement matched-test draft](./2026-10-20-omo-matched-no-reload-evaluation.md#current-outreach-draft-unsent). Existing ownership and prior-contact conditions still apply.

## Evidence and progress

[Artifact index](./artifacts/2026-10-20-omo-model-selected/README.md) and [machine-readable results](./artifacts/2026-10-20-omo-model-selected/results-summary.json) retain native messages, exact tool arguments/results, source fixtures, delegated hook receipts, compaction SSE receipts, effective configuration, model metadata and all attempted runs. Provider cookies/account identifiers are redacted from retained error records; fixture values are synthetic.

Model selection verified; specialized tool execution verified at all four final checkpoints; reload explicitly distinguished; earlier failures retained; source hashes, credential scan and document links checked. Docs index, shortlist and current outreach copy updated locally. No branches, commits, pushes, external drafts, messages or sends created.
