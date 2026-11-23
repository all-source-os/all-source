# Current memory fixes and artifact-led outreach drafts

Checked: 2026-10-19. Current status: live probes completed; every draft unsent.

[Live results, revised shortlist and replacement exact drafts](./2026-10-19-live-memory-evaluation.md) govern current outreach. The source/fix audit below remains a pinned snapshot.

Updates the [30-project research](../research/2026-10-19-memory-failure-design-partners.md). Evidence comes from public GitHub API records, pinned source and release records. Moving branches are represented by explicit snapshots. No candidate deployment accessed.

## Decision

Current priority: **oh-my-openagent technical contribution**, then **nanobot retrieval/test contribution**. **Hermes stays held for real-provider qualification**. Neither nanobot nor Hermes live result supports a current loss pitch; none is a confirmed design partner.

| Candidate | Current fix finding | Remaining work | Draft disposition |
| --- | --- | --- | --- |
| nanobot | Replacement continuity architecture shipped. Live synthetic run retained 3/3 values. | Offer retrieval/test fixtures. Two literal searches missed separately seeded originals. | Downgraded contribution draft. No memory-loss invitation. |
| oh-my-openagent | Restoration patch still open; inspected release lacks skill checkpoint field. Existing owner and budget feedback already present. | Supply captured provider-input traces and payload fixtures; coordinate with existing owner. | Technical note, no design-partner pitch; coordinate prior project contact. |
| Hermes | Live synthetic-provider outage warned and retained both markers across restoration. | Qualify a real provider destination or migration requirement. None demonstrated here. | Held diagnostic draft. No data-loss or vendor-migration claim. |

## Nanobot: shipped changes replace original continuity path

[#4212](https://github.com/HKUDS/nanobot/issues/4212) and [#4307](https://github.com/HKUDS/nanobot/issues/4307) remain open; latest visible discussion updates July 3. Open issue state does not establish current runtime failure.

| Change | Verified state | Outreach consequence |
| --- | --- | --- |
| [#4326](https://github.com/HKUDS/nanobot/pull/4326) | Merged June 13, be6419b289f68df352d8c93c6b20f03855426964 | Idle summary includes full tail, including recent corrections. |
| [#4348](https://github.com/HKUDS/nanobot/pull/4348) | Merged June 15, 3ce0cd972eee57318bb7eb472494c00367cb22a1 | Earlier suffix fix; later checkpoint replay supersedes it. |
| [#5610](https://github.com/HKUDS/nanobot/pull/5610) | Merged August 31, bb34b58f477c066d23ee08d068cc434c1fa65bee | Cumulative replacement checkpoints; ordinary prompts stop Recent History journal injection; raw transcript retained. |
| [#5568](https://github.com/HKUDS/nanobot/pull/5568) | Merged September 2, d81aa5a4ab420877e43a2a878c26f5635bbef743 | Runner owns pre-request context checks and synchronous compaction, replacing post-turn background pressure compaction. |
| [#5694](https://github.com/HKUDS/nanobot/pull/5694) | Merged September 8, 3548aec57677f0a6b91d3c4cb6f74cf564599928 | Manual, idle and pressure triggers share summary/replay boundary; fixed eight-message suffix removed. |
| [v0.3.5](https://github.com/HKUDS/nanobot/releases/tag/v0.3.5) | Published September 15; explicitly lists #5610, #5568, #5694 | Replacement fixes are released, not merely merged. |
| [#4424](https://github.com/HKUDS/nanobot/pull/4424), [#4621](https://github.com/HKUDS/nanobot/pull/4621) | Closed without merge; merged_at null | Provenance/promotion proposals did not land through these PRs. Equivalent behavior elsewhere remains possible. |
| [#4373](https://github.com/HKUDS/nanobot/pull/4373), [#4627](https://github.com/HKUDS/nanobot/pull/4627) | Closed without merge | Do not count these delivery proposals as deployed fixes. Later architecture is relevant. |
| [#4280](https://github.com/HKUDS/nanobot/pull/4280) | Open/unmerged | Avoid a competing generic rolling-summary implementation. |

Audited main: 8778c697744f8334c5794cd37502e8eb77ee5585.

- [Journal schema](https://github.com/HKUDS/nanobot/blob/8778c697744f8334c5794cd37502e8eb77ee5585/nanobot/agent/memory.py#L310) stores cursor, timestamp, content and optional session key. This record lacks structured per-fact origin/confidence/supersession. Raw transcripts elsewhere can still provide evidence; this is not a universal inability to reconstruct provenance.
- [Archive contract](https://github.com/HKUDS/nanobot/blob/8778c697744f8334c5794cd37502e8eb77ee5585/nanobot/templates/agent/consolidator_archive.md) already prioritises latest correction and active work state. Repeating that instruction is not a new fix.
- [Existing correction test](https://github.com/HKUDS/nanobot/blob/8778c697744f8334c5794cd37502e8eb77ee5585/tests/agent/test_consolidator.py#L1025) checks what reaches a mocked provider; it does not measure actual archive/Dream semantics.

Artifact: [three-case corpus](./artifacts/2026-10-19-memory-fix-audit/nanobot-correction-corpus.json). [Live run completed](./2026-10-19-live-memory-evaluation.md): three supplied cases, two native cycles each, fresh-process recall and separate seeded source-retrieval diagnostic. No AllSource comparison run.

## Hermes: warning PR state is not current behavior

[#49200](https://github.com/NousResearch/hermes-agent/issues/49200) remains open. Reporter bedpan describes lost writes after provider packages disappeared. [#49302](https://github.com/NousResearch/hermes-agent/pull/49302) remains open/unmerged, head 907ac936dd646e5fab3a1e4ca96d0573f911f6b2. It adds warning delivery and replay but retains fallback. Author-reported tests are not our run.

Audited main: 8f5693af418256b89da17ddda18a9242f113dcf6. Latest release checked: [v0.22.2](https://github.com/NousResearch/hermes-agent/releases/tag/v0.22.2), October 15, target d923a5eb487a17c0a68801bba9ef0f56c81040a8.

[Release source](https://github.com/NousResearch/hermes-agent/blob/d923a5eb487a17c0a68801bba9ef0f56c81040a8/agent/agent_init.py#L1348) and [main](https://github.com/NousResearch/hermes-agent/blob/8f5693af418256b89da17ddda18a9242f113dcf6/agent/agent_init.py#L1345) attempt startup recovery and emit unavailable-provider warnings. In inspected path, no activated external providers leaves manager None while built-in memory remains available.

Existing tests cover unavailable warnings, CLI startup delivery, gateway first-turn replay, migration consent/backoff and home isolation:

- tests/agent/test_memory_provider_unavailable_warning.py
- tests/agent/test_memory_migration_notifications.py
- tests/agent/test_memory_provider_init.py
- tests/hermes_cli/test_memory_provider_migration.py

No claim established that writes lack receipts, that fail-closed control is absent repo-wide, or that current deployment still loses data. Artifact: [destination probe procedure](./artifacts/2026-10-19-memory-fix-audit/hermes-provider-outage-probe.md), now [executed with a synthetic standalone provider](./2026-10-19-live-memory-evaluation.md#hermes-outage-results).

[Contribution rules](https://github.com/NousResearch/hermes-agent/blob/8f5693af418256b89da17ddda18a9242f113dcf6/CONTRIBUTING.md#memory-providers-ship-as-a-standalone-plugin) require new memory backends and third-party integrations to ship as standalone plugins. Do not offer an in-tree AllSource provider.

## Oh-my-openagent: unresolved patch, owned work

[#7158](https://github.com/code-yeongyu/oh-my-openagent/issues/7158) remains open, reporter sghng; no issue comments observed. [#7183](https://github.com/code-yeongyu/oh-my-openagent/pull/7183) remains open/unmerged, author AceRothstein71, head 93d44a0760448f7f07d2795c6239d51db5ab6576.

Default dev inspected: f6289c20fa8d542f0080edb27ba18f17958c938d. Latest release checked: [v5.1.38](https://github.com/code-yeongyu/oh-my-openagent/releases/tag/v5.1.38), October 19, tag ce910506c13f6f9dfb973e686769e22dbd91a5d1. [Release checkpoint source](https://github.com/code-yeongyu/oh-my-openagent/blob/ce910506c13f6f9dfb973e686769e22dbd91a5d1/packages/omo-opencode/src/shared/compaction-agent-config-checkpoint.ts) has agent/model/tools but no skill snapshots. Installed package/runtime not tested.

Patch adds per-session in-memory skill Map, snapshots and restoration. Inspected helpers do not add process-restart persistence. [September 1 reviewer feedback](https://github.com/code-yeongyu/oh-my-openagent/pull/7183#issuecomment-5489222380) already asks for bounds and multi-compaction coverage. Reviewer token estimates are not repeated as measurements.

Our [native probe](./artifacts/2026-10-19-memory-fix-audit/omo-payload-probe.ts) runs hash-verified formatter/store helpers. [Actual results](./artifacts/2026-10-19-memory-fix-audit/omo-payload-results.json): four controls pass, four synthetic payload shapes measured. A 1,048,576-byte skill produces 1,048,693 formatted bytes. Eight 65,536-byte skills produce 524,580 bytes.

No cap in this formatter; other runtime budgets not evaluated. Repeated pure calls do not establish repeated live compaction, duplication, token counts or failure. This is complementary evidence for existing owner, not a replacement implementation or unsolicited PR review.

## Current drafts and shortlist

[Current OMO provider-payload results and draft](./2026-10-21-omo-provider-payload-evaluation.md): Provider-captured rerun: skill text on passed 4/6 post-compaction workflows; off passed 0/6. All four baselines passed; reload stayed blocked. Provider inputs captured for every native model step; readiness/skill-only controls retained. Contribution-only disposition and prior-contact coordination remain.

[Earlier storage-readiness results and superseded draft](./2026-10-21-omo-readiness-barrier-evaluation.md): Skill text on passed 4/6 post-compaction workflows; off passed 0/6 across two opposite-order pairs with readiness barriers. All four baselines passed. Reload remained blocked. All 16 pre-task barriers verified; eligible recovery stored before tasks, immediate suppression retained. Existing contact coordination and contribution-only disposition remain.

[Earlier fixed-wait skill-only results and superseded draft](./2026-10-20-omo-skill-only-counterbalanced-evaluation.md): Skill text on passed 4/6 post-compaction workflows; off passed 0/6 across two opposite-order pairs. All baselines passed; reload stayed blocked. Agent/model/tools recovery and cooldown stayed active, and all 12 summary-injection outputs matched. Contribution-only disposition and prior-contact coordination remain.

[Earlier whole-hook results and superseded draft](./2026-10-20-omo-matched-no-reload-evaluation.md): enabled 2/3 vs disabled 0/3 post-compaction checkpoints; both baselines passed. Immediate second compaction failed inside cooldown despite recovery enabled. No successful reload occurred. Contribution-only status and prior-contact coordination remain.

[Earlier reload-available evaluation and superseded draft](./2026-10-20-omo-model-selected-evaluation.md): one four-checkpoint workflow passed through full OMO, with a middle skill reload. Earlier wrong-query/provider failure is retained. The contribution remains native regression evidence; no design-partner need is established.

[October 20 full-plugin results and exact unsent draft](./2026-10-20-omo-full-plugin-evaluation.md) supersede the prior bridge-only result: native loader works after staging real skill assets, recovery counts remain 1 → 1 → 2, and full large fixture bodies survive. The contribution remains owned native regression work; product invitation held.

[Earlier #7183 external-bridge results](./2026-10-19-omo-compaction-evaluation.md) supersede the formatter-only scope above: recovery-message counts 1 → 1 → 2; 1 MiB body stored as 1,048,817 bytes; eight 64 KiB bodies as 524,655 bytes. That earlier run excluded plugin-loader wiring; the October 20 full-plugin report verifies it. Existing author/reviewer ownership and prior project contact confirmed; technical contribution only.

Use [replacement exact drafts and live results](./2026-10-19-live-memory-evaluation.md#updated-exact-drafts-unsent). Prior pending-evaluation copy is retired.

- Nanobot: 3/3 projection values passed; native retrieval control reads all supplied originals; model query planning missed two. Downgrade to test/retrieval contribution, no memory-loss design-partner pitch.
- Hermes: startup warning worked, built-in fallback retained outage marker, restored external provider retained baseline. Hold product invitation; real vendor migration unmet need unconfirmed.
- Oh-my-openagent: first technical contribution candidate; [real compaction integration](./2026-10-19-omo-compaction-evaluation.md) confirms repeated restoration beyond cooldown and complete large payloads. Existing owner/reviewer work and prior project contact require coordination. Product invitation held.

## Prior contacts and send state

Read local [ledger](./qualified-builder-outreach.md). No exact joaoinacio/hamb1y/sghng/AceRothstein71/bedpan match found. This is local-ledger absence, not an inbox/CRM audit.

- Oh-my-openagent: [DevNewbie1826 contact](https://github.com/code-yeongyu/oh-my-openagent/issues/7764#issuecomment-5616313504), September 10. Coordinate before another project approach. Related [#8017](https://github.com/code-yeongyu/oh-my-openagent/pull/8017) merged before contact; do not revive its identity-binding fix.
- Hermes: prior yexxx/Lei-k contacts and skipped Hermes WebUI thread recorded. No duplicate cold project pitch.
- Nanobot: no project/contact match observed in searched local ledger.

| Draft | State | Conditions before any send |
| --- | --- | --- |
| Nanobot | Unsent; downgraded test/retrieval contribution | Ownership check, attachment availability, explicit founder send approval; live result limits and seeded-history distinction must accompany draft. |
| Oh-my-openagent | Unsent; technical only | Prior-contact coordination, patch-owner duplication check, attachment availability, explicit founder approval. |
| Hermes | Unsent; held diagnostic | Confirm remaining need, recheck ownership, attach procedure, explicit founder approval. |

No comments, messages, issues, reviews, PRs, publishing, commits or pushes performed. Existing approvals unchanged. No candidate added as contacted. This is preparation, not an approved send list.

## Verification and progress

| Work | Result |
| --- | --- |
| Issue/fix/release audit | Complete at pinned public snapshots; source checked beyond PR state. |
| OMO artifact | Executed with Bun 1.3.9 against hash-verified helpers; four controls and four payload shapes. |
| Nanobot artifact | Live three-case evaluation complete; see replacement report for scope and results. |
| Hermes artifact | Baseline/outage/restore complete with synthetic SQLite plugin; no production vendor inference. |
| Documents | Audit, artifacts, original-report update, docs index and local ledger cross-link saved locally. |
| Version control | No commits/branches authorised. Existing dirty main preserved. |
| Tracker | Previous local cn WAL access denied; no completed-task claim or tracker mutation. |

Voice grounding: prime_voice recall returned claim/evidence/limits and bounded PAS guidance before drafting. No invented deployment experience or personal memory write. OpenRouter model calls used for live probes; no contact enrichment or new treg calls needed.
