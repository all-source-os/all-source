# Oh-my-openagent #7183: counterbalanced skill-only recovery test

Date: 2026-10-20. Status: two counterbalanced matched pairs completed; outreach remains unsent.

Current readiness-checked results: [October 21 barrier evaluation](./2026-10-21-omo-readiness-barrier-evaluation.md). Skill text on passed 4/6 post-compaction workflows; off passed 0/6 across two opposite-order pairs with readiness barriers. All four baselines passed. Reload remained blocked. All 16 checkpoints started after settled native recovery handling; the fixed-wait results below retain their timing limit.

## Findings and qualification

**With reload blocked, skill-text on passed 4/6 post-compaction workflows; skill-text off passed 0/6. All four model-selected baselines passed.** Each arm completed three native compactions. Individual results below govern interpretation; six checkpoints per condition are related observations within two pairs, not six independent trials.

This isolates the outgoing skill block from the rest of recovery. It supersedes the whole-hook comparison in the [previous matched evaluation](./2026-10-20-omo-matched-no-reload-evaluation.md) for that causal question. Retain **native regression contribution; design partner unqualified**. The experiment establishes no AllSource product requirement, and a native fix may satisfy the builder's need.

2/2 skill-on arms passed both fresh-restoration checkpoints; 2/2 failed the immediate second compaction inside cooldown.

## Protocol and intervention boundary

Pinned [#7183 head](https://github.com/code-yeongyu/oh-my-openagent/pull/7183) **93d44a0760448f7f07d2795c6239d51db5ab6576**, unchanged compiled full OMO plugin and packaged assets; OpenCode **1.18.22**, harness Bun **1.3.9**. Requested model route: OpenRouter **minimax/minimax-m3.1-flash:free**, temperature 0, low reasoning, 2,048 output-token cap. No provider seed or backend pin. All arms used the same controlled build-agent audit prompt and five-step cap; production-agent prompt behavior remains untested.

Pair s1 ran **on → off**; pair s2 ran **off → on**, serially in fresh isolated profiles. Each pair shared exactly the same skill, decoy, four opaque queries and source-record contents. A new fixture was generated for the second pair. Session IDs, profile paths and timestamps differ; paths use opaque arm labels. Both pairs used identical frozen controller/observer code, host configuration and OMO configuration. The native compaction-context-injector hook remained enabled in every arm.

The model selected and loaded the audit skill through its real native tool. Later tasks supplied only the checkpoint number and source session, never the expected query or receipt. A pass requires a completed exact-query native session_search, completed session_info on the requested source, and the correct receipt in actual assistant output.

The test-only observer intercepts the **live HTTP fetch for /session/:id/prompt_async**, after native capture, checkpoint lookup, recovery decisions and internal dispatch gating. In the off condition it removes the complete checkpointed-skill section, including its skill header and body, between the agent-recovery prefix and internal-marker suffix. It retains the original recovery prefix, both internal markers, synthetic/compaction_continue metadata and all non-text fields. It does not clear checkpoint.skills, replace the formatter, disable the hook, alter clocks or change cooldown constants. The on condition uses the same observer and forwards the text unchanged.

Both conditions preserve the rest of the Request by constructing the replacement from the original Request; no URL, method, headers or routing settings are changed. A pre/post structural assertion rejects any difference outside the targeted text. Captured native messages independently confirm the text that the runtime stored.

## Results

| Pair / arm | Skill text | Baseline | Compaction 1 | Compaction 2 | Compaction 3 | Blocked reloads |
| --- | --- | --- | --- | --- | --- | --- |
| s1 / s1a | on | PASS | PASS | FAIL | PASS | 2 |
| s1 / s1b | off | PASS | FAIL | FAIL | FAIL | 5 |
| s2 / s2a | off | PASS | FAIL | FAIL | FAIL | 4 |
| s2 / s2b | on | PASS | PASS | FAIL | PASS | 3 |

All 16 workflow turns and 12 native compactions completed without terminal provider/runtime errors. Expected denied tool calls are test-policy events. All 16 original source bodies were subsequently verified intact with a read-only SELECT through OpenCode's own db CLI. That query ran after model evaluation and supplied no information to the model.

| Arm | First–second event gap, ms | First–third event gap, ms | Raw snapshot config counts | End-of-checkpoint config counts | End-of-checkpoint complete-skill counts |
| --- | --- | --- | --- | --- | --- |
| s1a | 17,825 | 99,401 | 1 → 1 → 2 | 1 → 1 → 2 | 1 → 1 → 2 |
| s1b | 25,081 | 110,735 | 1 → 1 → 2 | 1 → 1 → 2 | 0 → 0 → 0 |
| s2a | 22,461 | 102,209 | 1 → 1 → 2 | 1 → 1 → 2 | 0 → 0 → 0 |
| s2b | 13,627 | 101,530 | 1 → 1 → 1 | 1 → 1 → 2 | 1 → 1 → 2 |

The second compaction was inside the unchanged **60,000 ms cooldown** in every arm; the third was beyond it. The controller waited 62,000 real milliseconds before the third request. Native recovery dispatched after the first and third compactions, with no fresh dispatch after the second. Histories and generated summaries can diverge after treatment; the summary **instruction injection** remained identical.

The fixed 1,400 ms post-compaction wait was not a readiness barrier. In s2b round 3, the raw snapshot still had one configuration recovery. Dispatch occurred **580 ms after that snapshot**, and the second recovery message was stored **367 ms after the checkpoint task message**. The completed turn contains the second recovery and a successful exact-query workflow after a denied reload. Counts at checkpoint completion use the union of the raw snapshot and native turn messages, deduplicated by message ID; both inputs are retained. This run does not establish restoration readiness before the next task starts.

## Control checks

| Control | Check and result |
| --- | --- |
| Same native recovery hook | Identical OMO configs; compaction-context-injector enabled in all four arms; compiled bundle and pinned source hashes unchanged. |
| Dispatch-only skill removal | Eight actual recovery dispatches captured. Four off requests changed exactly one text part; four on requests changed none. Every before/after pair passed the non-skill-field equality assertion. Native endpoints accepted all eight requests. |
| Agent/model/tools recovery | All eight before/after envelopes retained identical agent, model, tools and noReply=true. Envelopes also matched across arms. Part metadata and internal markers remained intact. |
| Summary injection | All 12 post-hook output.context objects were byte-identical. SHA-256: `e96ccb7666e4ea77dd708545774377cf7a8cb64be0212345eaadbb9095de50a3`. This checks injected instructions, not model-generated summaries. |
| Cooldown and bookkeeping | No clock override; unchanged native source; observed two accepted dispatches per arm and config-recovery counts 1 → 1 → 2 at checkpoint completion in every arm. One raw snapshot preceded the second dispatch. No native state reset between rounds. |
| Skill checkpoint still captured | All four off-arm dispatch inputs contained the full original skill body before stripping. Off-arm stored recovery messages contained no skill block. |
| Reload blocked | Exactly one successful native skill execution and one latch closure per arm. All later skill calls were rejected before native execution, including cached reloads. Total blocked attempts: 14; successful post-baseline reloads: zero. |
| Equal fallback policy | Only current-source inspection and opaque-format queries allowed. Gate checks syntax and source scope, never the correct query. Broad searches, other-session reads, general file tools and MCPs unavailable in every arm. |
| Shared inputs | Skill/source fixtures identical within each pair; host and OMO configs identical across all arms; native IDs, paths, timestamps differ. |
| Scoring | Recomputed from actual native tool parts and assistant text; all baselines passed; no task prompt leaked query/receipt. |
| Source integrity | OpenCode native db engine verified all 16 original query/receipt bodies after evaluation. |

## Interpretation and limits

The per-round table tests whether reinjected skill text changes specialized-tool success under this controlled policy. Recovery on/off findings from the older whole-hook pair must not be presented as this skill-only treatment. Immediate second-compaction failures, when present, occur with no fresh dispatch inside the native cooldown; they do not establish that underlying history was deleted.

Two independent fixtures and opposite order reduce the original order confound. They do not establish a statistical effect estimate or reliability across models. Temperature 0 does not guarantee deterministic replay, and provider backends were not pinned. Later summaries and tool histories are treatment consequences and were not forced to match.

The dispatch checks prove that agent/model/tools recovery fields were preserved. They do not separately test recovery from deliberately incorrect agent, model or permission settings.

This uses synthetic opaque ledger queries, manual auto:false compactions, supplied skill metadata and restricted fallback routes. It does not test automatic pressure selection, autocontinue, restart recovery, missing/changed skill files, original production tools, external MCP operations or large-payload acceptance by a live model. [Payload-bound measurements](./2026-10-20-omo-full-plugin-evaluation.md) remain separate: the tested native recovery payloads reached 1,049,000 and 526,127 bytes; these are byte measurements, not token counts or provider-acceptance results.

Three preliminary runs were excluded before the frozen scored protocol: p1a stalled before plugin initialization; q1a was interrupted during startup diagnosis before a baseline result; r1a exercised recovery but proved that intercepting only the original SDK client missed native live-route dispatch. The final observer covers the HTTP boundary. None of those runs contributes to the result counts. Native TypeScript/Bun is used to wrap the real plugin API; production source was not patched.

## Ownership and prior contact

Existing [ownership/contact audit](./2026-10-19-omo-compaction-evaluation.md#ownership-and-prior-contact) applies. PR author AceRothstein71 owns the existing work; [MoerAI's review](https://github.com/code-yeongyu/oh-my-openagent/pull/7183#issuecomment-5489222380) already asks for bounds and cooldown coverage. Decebal previously contacted DevNewbie1826 on [#7764](https://github.com/code-yeongyu/oh-my-openagent/issues/7764#issuecomment-5616313504). Coordinate that project-level contact before any future approach. This run did not refresh inbox or PR ownership state; absence of a reply is not consent.

## Current outreach draft: unsent

Use the [readiness-barrier replacement draft](./2026-10-21-omo-readiness-barrier-evaluation.md#current-outreach-draft-unsent). Existing ownership and prior-contact conditions apply.

## Evidence

[Artifact index](./artifacts/2026-10-20-omo-skill-only-counterbalanced/README.md) links frozen scripts, pair fixtures, actual dispatch before/after bodies, summary-injection outputs, native tool/message snapshots, native source-integrity queries and recomputed [results](./artifacts/2026-10-20-omo-skill-only-counterbalanced/results-summary.json). Docs index, shortlist and outreach ledger reflect these results. No branch, commit or push was created.
