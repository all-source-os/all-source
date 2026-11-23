# Oh-my-openagent #7183: matched recovery test with reload blocked

Date: 2026-10-20. Status: matched pair completed; artifacts and unsent draft saved.

Current skill-only evidence: [counterbalanced dispatch-only evaluation](./2026-10-20-omo-skill-only-counterbalanced-evaluation.md). Skill text on passed 4/6 post-compaction workflows; off passed 0/6 across two opposite-order pairs. All baselines passed; reload stayed blocked. Agent/model/tools recovery and cooldown stayed active, and all 12 summary-injection outputs matched. The results below remain the older whole-hook experiment.

## Findings and qualification

**With skill reload blocked, recovery enabled passed 2/3 post-compaction checkpoints; recovery disabled passed 0/3. Both arms passed their initial model-selected skill load and baseline workflow. The enabled arm failed the immediate second compaction.** Thus continuous success across all three compactions without reload was not established.

These results replace the fallback-dependent interpretation in the [previous model-selected run](./2026-10-20-omo-model-selected-evaluation.md). They support a native regression contribution for the existing #7183 owner. They do not qualify a design partner or establish demand for AllSource. All eight original source bodies were verified intact after the tests; this is a controlled working-guidance test, not evidence of destroyed history.

## Match and intervention

- Source: [#7183](https://github.com/code-yeongyu/oh-my-openagent/pull/7183), open at **93d44a0760448f7f07d2795c6239d51db5ab6576** when rechecked. Both arms imported the same unchanged compiled full OMO plugin and packaged assets through OpenCode's native loader.
- Runtime/model: OpenCode **1.18.22**, harness Bun **1.3.9**, OpenRouter **`minimax/minimax-m3.1-flash:free`**. Same requested temperature 0, low reasoning, 2,048 output-token cap, short audit-agent prompt, five-step limit and allowed tools. No forced tool choice or provider seed was supplied.
- One shared fixture file generated once: identical audit skill, decoy, four opaque checkpoint queries and four receipts. Each arm received a fresh isolated profile and four source sessions containing identical records. The initial task supplied skill names/descriptions, and the model selected the audit skill itself.
- Native session IDs, paths and timestamps necessarily differ. All user-task templates, source-record contents, skill bytes and controller/observer code match. Opaque profile labels `a1`/`b1` did not expose the treatment to the model. The arms ran sequentially, **off first, on second**, without counterbalancing.
- Treatment: the off arm adds only `compaction-context-injector` to OMO's native `disabled_hooks`. This disables that hook's capture, summary-context injection and restoration together. It does not isolate skill reinjection from the rest of the hook. Ordinary native compaction and the remaining OMO hooks stay active.
- Each checkpoint requests an exact skill-defined query through native `session_search`, followed by `session_info` on the specified source, and a matching receipt in the answer. Scoring reads actual completed-tool arguments/results and native response text, not a model's claim about its actions.

## Reload block and fallback controls

The same test-policy wrapper in both arms allows the initial native skill execution. Once it succeeds, a per-session latch rejects every later skill execution **before calling the native implementation**, including attempts that could use cached skill content. The skill tool remains visible, so blocked attempts appear as real native tool-error parts. No synthetic success result is returned.

Each arm recorded exactly **one successful native skill execution** and one gate closure. Off arm blocked **3** reload attempts; on arm blocked **1**. There were no successful post-baseline reloads. The skill file remained unchanged on disk, and the wrapper never executed it on its own.

Other fallbacks were constrained equally. Native searches and source inspections were restricted to the requested source session; search queries had to use the opaque `ZEPHYR_` plus ten hexadecimal-character format. The gate does not compare against or reveal the correct query. Broad searches, other-session history lookups, general file access and MCP tools were unavailable. These controls make this a guidance-retention experiment; they are not claims about every recovery option in a normal installation.

## Native results

| Checkpoint | Recovery enabled | Recovery disabled | Successful skill reload allowed? |
| --- | --- | --- | --- |
| Baseline, before compaction | PASS | PASS | Initial native load only |
| After compaction 1 | PASS | FAIL | No |
| After compaction 2, inside cooldown | FAIL | FAIL | No |
| After compaction 3, beyond cooldown | PASS | FAIL | No |

For every passing checkpoint, exact-query search, source inspection and correct receipt text all passed. For a failed checkpoint, a refusal or source metadata alone did not count as success. All eight checkpoint turns and six native compactions completed without terminal provider/runtime errors. The blocked tool errors are expected test-policy enforcement, not provider failures.

| Native evidence | Recovery enabled | Recovery disabled |
| --- | --- | --- |
| Stored complete-skill restoration messages after compactions 1/2/3 | 1 → 1 → 2 | 0 → 0 → 0 |
| First-to-second compaction-event gap | 8,736 ms | 23,029 ms |
| First-to-third compaction-event gap | 94,251 ms | 106,737 ms |
| Native skill executions | 1 | 1 |

Both second compactions occurred inside the source's [60,000 ms recovery cooldown](https://github.com/code-yeongyu/oh-my-openagent/blob/93d44a0760448f7f07d2795c6239d51db5ab6576/packages/omo-opencode/src/hooks/compaction-context-injector/constants.ts#L4); both third compactions occurred beyond it. The driver inserted 62,000 real milliseconds before each third request. No clock or cooldown override was used.

After both model runs ended, a separate read-only `SELECT` through OpenCode's own `db` CLI checked all four source bodies in each isolated profile. Every original query/receipt pair remained present. This post-test control did not feed any data back to the model or change checkpoint scoring.

In the enabled arm, compaction 2 produced no fresh restoration message, the model attempted a reload, the gate denied it, and the checkpoint failed. The source's [cooldown branch](https://github.com/code-yeongyu/oh-my-openagent/blob/93d44a0760448f7f07d2795c6239d51db5ab6576/packages/omo-opencode/src/hooks/compaction-context-injector/recovery.ts#L50) and native receipts agree with a cooldown-linked gap under these conditions. The off arm had no complete-skill restorations and failed all post-compaction exact-query workflows. This is stronger evidence than the earlier reload-available run, while remaining one whole-hook matched comparison.

## Interpretation and remaining limits

- Recovery benefits appeared at the enabled arm's passing post-compaction checkpoints in this pair. The hook did not satisfy uninterrupted no-reload use through the immediate second compaction.
- One pair, four related checkpoints per arm, one requested model route and a fixed off-then-on order. No statistical effect estimate, provider-backend pin, seed guarantee or model-independent reliability claim.
- The intervention disables the entire context-injector hook, including capture and summary-context injection. It is not a post-dispatch-only or skill-field-only ablation. Summaries and later conversation histories also diverge as consequences of the intervention.
- Opaque synthetic queries, supplied skill metadata, a short audit prompt, strict source scope and blocked fallback routes. Failure means this restricted task could not finish, not that the underlying record was gone or an unrestricted agent could never recover it.
- Reload stayed blocked in both arms, including cached execution. The test does not cover changed/missing skill files, authorized workflow-version changes, original team tools or external MCP operations.
- Manual `auto:false` compactions only. No automatic pressure selection, autocontinue, restart, UI or large-model-context acceptance claim. Earlier [payload-bound measurements](./2026-10-20-omo-full-plugin-evaluation.md) remain a separate experiment.
- Production source was unchanged. No full release build/typecheck claim or unsolicited upstream patch. A native fix may resolve the builder's requirement without another memory service.

## Current outreach draft: unsent

Use the [counterbalanced skill-only draft](./2026-10-20-omo-skill-only-counterbalanced-evaluation.md#current-outreach-draft-unsent). Existing ownership and prior-contact conditions apply.

## Evidence and progress

[Artifact index](./artifacts/2026-10-20-omo-matched-no-reload/README.md) and [machine-readable matched results](./artifacts/2026-10-20-omo-matched-no-reload/results-summary.json) retain shared fixture hashes, normalized-config comparison, raw native checkpoint/compaction messages, gate decisions, observer/controller code, native SSE receipts and effective configuration.

Both baselines verified; reload block exercised; repeated compactions completed; recovery state checked against stored messages; exact-query workflow scores recomputed; source hashes and document links checked. Docs index, shortlist and current draft updated locally. No branches, commits, pushes, external drafts, messages or sends created.
