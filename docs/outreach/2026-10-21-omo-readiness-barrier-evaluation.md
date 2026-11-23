# Oh-my-openagent #7183: readiness-barrier counterbalanced evaluation

Date: 2026-10-21. Status: two matched pairs completed; outreach unsent.

Current provider-bound evidence: [captured request evaluation](./2026-10-21-omo-provider-payload-evaluation.md). Provider-captured rerun: skill text on passed 4/6 post-compaction workflows; off passed 0/6. All four baselines passed; reload stayed blocked. It extends storage readiness with actual outbound request bodies.

## Findings and qualification

**Skill text on passed 4/6 post-compaction workflows; off passed 0/6 across two opposite-order pairs with readiness barriers. All four baselines passed. Reload remained blocked.** Skill-on passed **4/4** checkpoints with fresh recovery and **0/2** immediate checkpoints inside cooldown. Six post-compaction checkpoints per condition are related observations within two pairs.

All **16 checkpoints** began after a verified readiness barrier. All **eight eligible recovery requests** were accepted and stored before their checkpoint; no recovery message arrived during any scored task turn. This removes the observed dispatch-arrival race from the [previous fixed-wait comparison](./2026-10-20-omo-skill-only-counterbalanced-evaluation.md). It does not make the cooldown-suppressed second checkpoint acquire a fresh skill restoration.

The earlier fixed-wait run passed 4/6 with skill text on and 0/6 with it off. This rerun passed 4/6 and 0/6 respectively, with no observed recovery arrival during a task turn.

Qualification stays **native regression contribution; AllSource design-partner demand unqualified**. Existing native work may satisfy the need without another service.

## Matched setup

Pinned [#7183](https://github.com/code-yeongyu/oh-my-openagent/pull/7183) head **93d44a0760448f7f07d2795c6239d51db5ab6576**, same unchanged full plugin bundle and packaged assets. OpenCode **1.18.22**, Bun **1.3.9**, OpenRouter **minimax/minimax-m3.1-flash:free**, requested temperature 0, low reasoning and 2,048 output-token cap. All arms used the same controlled build-agent audit prompt and five-step cap. No provider seed or backend pin.

Pair b1 ran **on → off**; b2 ran **off → on**, serially in fresh HOME/XDG profiles and native session databases. The two prior pair fixtures were reused byte-for-byte: b1 uses the former s1 fixture; b2 uses s2. Skill, decoy, opaque query map and source records match within each pair and with its earlier counterpart. Native session IDs, profile paths and timestamps differ.

The model chose and executed the initial skill load. Later task prompts contain only checkpoint number and source session. A pass requires the exact native session_search query and returned receipt, completed session_info on that source, and the receipt in actual assistant text. No forced tool choice or controller-executed skill load.

The dispatch observer is **byte-identical** to the previous skill-only observer. It intercepts the actual HTTP /session/:id/prompt_async request after native recovery decisions. Off strips only the checkpointed-skill section between the unchanged recovery prefix and internal-marker suffix. Agent/model/tools, noReply, synthetic metadata and both internal markers remain intact. Both arms keep the native context-injector hook enabled, with identical host and OMO settings. Production source and the 60,000 ms cooldown are unchanged.

## Recovery-readiness barrier

The fixed 1,400 ms delay was removed. Before **every checkpoint**, the controller polls native messages and session status, and reads observed native hook/dispatch completion. Polling is read-only and adds no message or skill content to model context. Each barrier has one **20,000 ms deadline**, per-probe HTTP caps up to 2,000 ms, and up to 100 ms between polls. Failure aborts that arm as infrastructure failure; it is not scored as a memory failure.

| Checkpoint | Required state before task dispatch |
| --- | --- |
| Baseline | Full plugin initialized; native session not busy; zero compaction, recovery, skill-execution or reload-latch records. |
| Compaction 1 | Native summary and compaction handler complete; one successful HTTP recovery response; one stored synthetic recovery with matching agent/model/tools and exact dispatched text; initial skill-load latch closed. |
| Compaction 2 | Second native summary/handler complete; still inside original cooldown; dispatch and stored-recovery counts remain one; latch stays closed. State is settled-cooldown-suppressed, with no fresh restoration. |
| Compaction 3 | Third native summary/handler complete beyond cooldown; two accepted and stored recoveries; last recovery text and configuration match the actual dispatch; latch stays closed. |

The driver retains the original timing regimes: immediate second compaction and a real 62,000 ms wait before the third, split into two waits. The barrier never forces recovery, clears checkpoints, changes clocks or waits for cooldown expiry at checkpoint 2. Readiness means **settled native recovery handling**; only eligible rounds require a fresh stored restoration.

Every release timestamp precedes its native task-message creation timestamp. Final analysis rejects any recovery message appearing in a task-turn delta. It also checks exact pre-task recovery counts **0 → 1 → 1 → 2** and compaction snapshots **1 → 1 → 2**, rather than repairing counts from later task messages.

## Results

| Arm | Skill text | Baseline | Compaction 1 | Compaction 2 | Compaction 3 | Blocked reloads |
| --- | --- | --- | --- | --- | --- | --- |
| b1a | on | PASS | PASS | FAIL | PASS | 1 |
| b1b | off | PASS | FAIL | FAIL | FAIL | 3 |
| b2a | off | PASS | FAIL | FAIL | FAIL | 3 |
| b2b | on | PASS | PASS | FAIL | PASS | 2 |

All 16 workflow turns and 12 native compactions completed without terminal provider/runtime errors. Expected policy-denied tool calls remain visible in native results. Exactly four initial native skill executions succeeded; all 9 subsequent attempts were denied before native execution. No successful reload, including cached execution.

| Arm | Barrier elapsed ms, stages 0/1/2/3 | Release-to-task lead ms, stages 0/1/2/3 | First–second compaction gap ms | First–third gap ms |
| --- | --- | --- | --- | --- |
| b1a | 4 / 229 / 35 / 492 | 11 / 5 / 20 / 84 | 8,917 | 95,426 |
| b1b | 105 / 1553 / 128 / 351 | 58 / 366 / 316 / 48 | 27,515 | 110,512 |
| b2a | 19 / 257 / 232 / 333 | 6 / 14 / 40 / 42 | 20,507 | 109,621 |
| b2b | 35 / 503 / 106 / 466 | 6 / 35 / 38 / 56 | 8,075 | 93,586 |

Measured barrier duration ranged from **4 to 1553 ms**; release-to-task lead ranged from **5 to 366 ms**. These are observed measurements. All second events remained inside cooldown; all third events were beyond it.

## Control checks

| Control | Evidence and result |
| --- | --- |
| Readiness | 16 barriers passed before native task creation; eight eligible dispatches accepted and stored first; zero task-turn recovery arrivals. |
| Skill-only ablation | Four off requests changed one skill-bearing text part each; four on requests were unchanged. Structural assertions passed for all eight envelopes and part metadata. |
| Agent/model/tools | All eight before/after recovery envelopes match; stored native messages match the dispatched agent, model, tools and recovery text. |
| Cooldown | Same source constant and bookkeeping. Each arm has accepted/stored counts 1 → 1 → 2; second handler completed without a fresh dispatch. |
| Native checkpoint capture | Exact-subject native OMO logs show build agent, model, tools and one loaded skill captured before every compaction, plus successful restoration before/after the suppressed round. This corroborates the cooldown interpretation rather than a missing agent checkpoint. |
| Summary instruction injection | All 12 native output.context objects byte-identical. SHA-256: `e96ccb7666e4ea77dd708545774377cf7a8cb64be0212345eaadbb9095de50a3`. Generated summaries may differ. |
| Reload latch | One initial native load and one latch closure per arm; 9 later attempts denied before native code; zero post-baseline reload successes. |
| Matching | Identical host/OMO configs and native dependency locks across arms; fixtures match within pair and earlier experiment; observer and compiled bundle hashes unchanged. |
| Native plugin | Each arm initialized all 14 native tools and 15 hook/object entries. No mock hook or replacement skill implementation. |
| Fallback scope | Same current-source and opaque-query-syntax policy. Gate never checks or reveals the correct query. General file tools, broad searches, other-session history and MCPs unavailable. |
| Source integrity | All 16 original query/receipt records verified after evaluation using read-only SELECT through OpenCode's own db engine. No data fed back to model. |
| Cleanup | Each controller owned and reaped its server. Final owned-runtime process check found no remaining QA process. |

## Interpretation and limits

The barrier addresses the previous asynchronous recovery-arrival confound for these scored workflows. It verifies that accepted recovery is in native storage before the next task; it does not inspect the exact provider request assembled from that storage. Specialized-tool success is checked separately from storage readiness.

Cooldown suppression is deliberately retained at checkpoint 2. A settled barrier there confirms that recovery handling finished without a new dispatch. It does not promise fresh skill text or uninterrupted workflow success. This experiment therefore tests the native recovery policy, including its suppression gap.

Two matched fixtures and opposite order do not establish a statistical effect estimate, model-independent reliability or production-agent behavior. The prior fixtures are reused, not new independent corpus replication. Temperature 0 does not guarantee replay; provider backends and seeds are unpinned. Later summaries and histories can diverge as treatment consequences.

Synthetic opaque ledger queries, short audit prompt, five-step cap, supplied skill metadata and restricted fallback paths remain. Manual auto:false compactions only. Automatic pressure selection, autocontinue, restarts, changed/missing skill versions, production tools and external MCP operations are untested. Agent/model/tools checks prove preserved recovery fields, not repair from deliberately wrong settings. [Earlier payload bounds](./2026-10-20-omo-full-plugin-evaluation.md) remain separate native byte measurements, not live-model acceptance claims.

## Ownership and prior contact

Existing [ownership/contact audit](./2026-10-19-omo-compaction-evaluation.md#ownership-and-prior-contact) applies; this run did not refresh PR ownership or inbox state. AceRothstein71 owns #7183. [MoerAI's existing review](https://github.com/code-yeongyu/oh-my-openagent/pull/7183#issuecomment-5489222380) already requests bounds and cooldown coverage. Decebal previously approached DevNewbie1826 on [#7764](https://github.com/code-yeongyu/oh-my-openagent/issues/7764#issuecomment-5616313504). Coordinate that project-level contact before any future approach. Absence of reply is not consent.

## Current outreach draft: unsent

Use the [provider-payload replacement draft](./2026-10-21-omo-provider-payload-evaluation.md#current-outreach-draft-unsent). Existing ownership/prior-contact conditions apply.

## Evidence and progress

[Artifact index](./artifacts/2026-10-21-omo-readiness-barrier/README.md) and [recomputed results](./artifacts/2026-10-21-omo-readiness-barrier/results-summary.json) retain frozen scripts, paired fixtures, all barrier polls and pre-task snapshots, dispatch before/after bodies, native workflow/compaction records and source-integrity queries. Docs index, shortlist and outreach ledger updated locally. No branches, commits or pushes created.
