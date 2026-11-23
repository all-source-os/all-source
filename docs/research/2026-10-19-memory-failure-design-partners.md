# Memory-failure design-partner research

Status: research complete; synthetic qualification probes complete within documented limits; outreach held. Access date: 2026-10-19.

## Scope and coverage

Find 30 distinct public projects with evidence of cross-session memory loss, restart recovery failures, stale recall, or missing provenance. Identify the most plausible candidates for the [AllSource design-partner program](https://www.all-source.xyz/design-partners).

- Research mode: focused, worldwide developer audience. Preferred recent window: 2026-04-19 through 2026-10-19. Older evidence is dated and discounted.
- Coverage: 30 distinct project repositories; 30 primary issue/discussion pages in the ledger below, plus nanobot's second failure report. Each primary body was opened. These are 31 retained evidence URLs across one platform family, GitHub, not 30 independent markets or confirmed customers.
- Conversion: an affected builder agrees to integrate one real workflow and run an identical failure/recovery acceptance test. Budget, purchase intent, integration availability, and program eligibility are unknown for every candidate.
- Public professional handles only. A reporter's unnamed application is not counted as another project. Repository popularity does not establish willingness to use AllSource.
- Status means the state visible on the retrieved public page. Web retrieval can use cached pages; this is not a live GitHub API snapshot. Recheck issues, linked PRs, and releases immediately before taking action.
- Method: parallel source-family research, followed by source-body checks, deduplication, and ranking. treg Crawl4AI search supplemented web search; its two calls returned broad off-target results, so those snippets were not evidence.

## Verdict

Current OMO qualification: [provider-payload evaluation](../outreach/2026-10-21-omo-provider-payload-evaluation.md). Provider-captured rerun: skill text on passed 4/6 post-compaction workflows; off passed 0/6. All four baselines passed; reload stayed blocked. Actual outbound bodies distinguish complete restored skills from generated summaries; 73 calls covered. Native regression contribution remains appropriate; no confirmed AllSource requirement.

Earlier storage-readiness comparison: [readiness-barrier evaluation](../outreach/2026-10-21-omo-readiness-barrier-evaluation.md). Skill text on passed 4/6 post-compaction workflows; off passed 0/6 across two opposite-order pairs with readiness barriers. All four baselines passed. Reload remained blocked. All 16 barriers released before native task creation; no recovery arrived during task turns. Native contribution remains appropriate; no confirmed AllSource requirement.

Earlier fixed-wait OMO comparison: [counterbalanced skill-only evaluation](../outreach/2026-10-20-omo-skill-only-counterbalanced-evaluation.md). Skill text on passed 4/6 post-compaction workflows; off passed 0/6 across two opposite-order pairs. All baselines passed; reload stayed blocked. Agent/model/tools recovery and cooldown stayed active, and all 12 summary-injection outputs matched. Native regression contribution remains the appropriate approach; no confirmed AllSource requirement.

Earlier whole-hook OMO comparison: [matched recovery/no-reload evaluation](../outreach/2026-10-20-omo-matched-no-reload-evaluation.md) passed 2/3 post-compaction checkpoints with recovery on and 0/3 with it off, under the same fixture/tool policy. Both baselines passed; immediate second compaction failed inside cooldown with recovery enabled. This is native regression evidence from one restricted matched pair, not confirmed AllSource demand. Retain contribution-only disposition and prior-contact coordination.

Earlier reload-available workflow result: [model-selected skill and tool evaluation](../outreach/2026-10-20-omo-model-selected-evaluation.md) completed four checkpoints with native specialized tools, including a skill reload after compaction 2. The synthetic workflow works with fallback available; uninterrupted reuse and a need for AllSource remain unproved. Retained nano counter-evidence prevents a model-independent claim. Keep contribution-only status and prior-contact coordination.

Previous full-plugin result: [October 20 full-plugin evaluation](../outreach/2026-10-20-omo-full-plugin-evaluation.md) resolves the loader gap and confirms repeated reinjection beyond cooldown with complete large bodies. Existing author/reviewer ownership remains; technical contribution only after prior-contact coordination. AllSource need is unconfirmed.

[Oh-my-openagent #7183 real compaction results](../outreach/2026-10-19-omo-compaction-evaluation.md): native-event integration restored the same skill again beyond cooldown and stored complete large payloads. Existing patch owner and reviewer already cover this problem; offer test evidence only after coordinating prior contact. The October 20 run verifies full plugin-loader wiring. A need for AllSource remains unverified.

Current qualification update: [live evaluation and revised unsent drafts](../outreach/2026-10-19-live-memory-evaluation.md) supersede the original ranking. Nanobot passed 3/3 value-retention cases; native history remained readable in a separately seeded control, while model retrieval missed two originals. Hermes warned and retained both markers in separate stores during a synthetic-provider outage. Neither result supports a current memory-loss pitch. Oh-my-openagent remains first technical-contribution candidate, subject to existing ownership/contact coordination.

Current action: offer technical evidence to existing owners where useful. Oh-my-openagent remains first contribution candidate. Nanobot is downgraded to retrieval/test work, and Hermes remains held for real-provider qualification. No design-partner invitation is justified by the live synthetic results alone.

This is a list of 30 projects with public memory-failure or memory-limitation evidence, not 30 qualified design partners. Resolved bugs, feature requests, and weak reports remain visible as counter-evidence.

| Dimension | Rating | Evidence and limit |
| --- | --- | --- |
| Problem demand | Strong within this sample | Multiple unrelated repositories report lost history, lost tool receipts, stale memory, or missing provenance. All observations come from GitHub; prevalence outside this selected sample is unknown. |
| Solution demand | Mixed | Builders seek durable history and source preservation, but many need routing, reducers, or native bug fixes. No source establishes demand for AllSource specifically. |
| Channel access | Mixed | Public reporters and maintainers are identifiable. A public profile is not a verified contact method or permission to solicit. Existing AllSource ledger contains prior contacts. |

## Ranking method

Scores are analyst judgments about qualification priority, not measured conversion probabilities. Components are **E/F/B/C**, each 0–3:

- **E, evidence:** 3 = concrete failure with trace/code or corroboration; 2 = detailed firsthand report; 1 = vague report or architectural request; 0 = no direct evidence.
- **F, fit:** 3 = source-event history/provenance/replay is central; 2 = useful complementary storage, but another layer owns the defect; 1 = mostly model/UX/config behavior; 0 = unrelated.
- **B, bounded pilot:** 3 = one clear adapter or workflow; 2 = plausible integration needing discovery; 1 = substantial upstream dependency or overlapping vendor; 0 = no plausible test.
- **C, current opportunity:** 3 = open source page with no linked remedy observed; 2 = open with linked remedy or closed without demonstrated resolution; 1 = old evidence, workarounds, or withdrawn design; 0 = documented fix.

Scores do not infer maintainer interest. Closed threads are never outreach destinations. Existing fixes and contributor ownership take precedence over ranking. Every individual source is grade **C** under the sea-of-demand framework because it is an isolated report; recurrence across projects supports a **B** problem finding within one platform family. No grade A claim is supported.

## Original qualification candidates; see current audit

| Priority | Candidate and evidence | Score E/F/B/C | Proposed useful artifact | Qualification gate / caveat |
| --- | --- | --- | --- | --- |
| 1 | **nanobot**, [#4212](https://github.com/HKUDS/nanobot/issues/4212), [#4307](https://github.com/HKUDS/nanobot/issues/4307) | 3/3/3/3 = **12** | A source-role and correction fixture: record an inferred preference, explicit correction, consolidation, restart, then show which source each recalled fact came from. Separate fixture preserves numbered options after consolidation. | Ask whether a builder owns a current affected workflow and will test a provenance-aware adapter. An immutable store cannot fix a reducer that continues injecting the wrong projection. |
| 2 | **Hermes Agent**, [#49200](https://github.com/NousResearch/hermes-agent/issues/49200) | 3/3/3/2 = **11** | Provider-disappearance fixture with explicit provider identity, accepted-write receipt, and restart/recall checks. | Linked #49302 may already address the native fallback bug. Establish remaining need first. AllSource only helps if runtime routes writes correctly and checks acknowledgements. |
| 3 | **oh-my-openagent**, [#7158](https://github.com/code-yeongyu/oh-my-openagent/issues/7158) | 3/3/3/2 = **11** | Reproduce loading a skill, compacting, and attempting its specialized tools; restore the versioned skill reference with source and load event. | Linked #7183 means possible existing ownership/fix. Check it before building anything. Native checkpoint fields may fully solve this without another service. |
| 4 | **Graphiti #1728 reporter's workload**, [source](https://github.com/getzep/graphiti/issues/1728) | 3/3/2/2 = **10** | A replay fixture that records why a fact was invalidated and verifies that an unrelated additive fact cannot retire it. | Owning application is unnamed. Qualify reporter's application, not Graphiti as a buyer. Linked #1729 and existing fork work may already cover the fix. |
| 5 | **Goose user's refactoring workflow**, [discussion #8318](https://github.com/aaif-goose/goose/discussions/8318) | 2/3/3/1 = **9** | MCP task-memory example: save goals, accepted decisions, source paths, and next action; start another process and recover those items. | Answered/closed thread; `.goosehints` and scratch notes helped. Validate native resume and whether manual upkeep remains painful. Do not post on the closed discussion. |

Public routes for these five are the handles in the source ledger and each repository's existing discussion/community surface. Those are identity and discovery routes, not verified off-platform addresses. No emails were enriched or inferred.

## Thirty-project source ledger

All rows accessed 2026-10-19. Publication is the visible opening date. Evidence is paraphrased; no claim here establishes an independently reproduced bug. `Open` does not mean unfixed in every current release. Public source links also identify the relevant professional participants.

### Priority and conditional consumer candidates

| ID | Project / source / published | Observed evidence | State and limits | Fit and public participant |
| --- | --- | --- | --- | --- |
| 01 | **nanobot** — [#4212](https://github.com/HKUDS/nanobot/issues/4212), 2026-06-05 | Consolidation mixes user statements with agent inference, removes origin, and reinjects guesses as facts. Explicit corrections can lose to repeated guesses. Contributor confirms missing source/provenance fields. | **Open**. Architecture report, not independently replayed here. Companion [#4307](https://github.com/HKUDS/nanobot/issues/4307), 2026-06-12, open, describes lost delivered options causing a follow-up to select a stale task. | **First qualification**. `joaoinacio`, `hamb1y`; companion reporter `MARJORIESHA-pBAD`. Provenance and correction replay. |
| 02 | **Hermes Agent** — [#49200](https://github.com/NousResearch/hermes-agent/issues/49200), 2026-06-19 | Docker update removed external-provider packages; writes silently fell back to a small flat file while old database still answered recall. Reporter says six days elapsed before discovery, then failure recurred. | **Open**, linked #49302. Provider loading/routing defect; replacing storage alone does not repair it. Data-loss extent is reporter testimony. | **First qualification, conditional on fix review**. `bedpan`. Provider identity, write receipts, stale-read evidence. |
| 03 | **oh-my-openagent** — [#7158](https://github.com/code-yeongyu/oh-my-openagent/issues/7158), 2026-08-23 | Mid-session skill bodies disappear from compacted context; skill names survive. Agent used generic file reads rather than specialized team tools. Body identifies missing skill checkpoint/restoration path. | **Open**, linked #7183. Skill guidance loss, not destroyed disk data. Release status of linked remedy unverified. | **Native regression contribution only; design-partner demand unqualified**. `sghng`; repository owner `code-yeongyu`. Versioned skill-load history. |
| 04 | **Goose** — [discussion #8318](https://github.com/aaif-goose/goose/discussions/8318), 2026-04-05 | User restarts CLI and re-explains React debugging/refactoring context. Follow-up says persistent hints and scratch task notes improved continuity. | **Answered/closed**. Existing workaround. Answer's broad claim about ephemeral sessions is not verified product documentation. Current resume behavior must be tested. | **Qualification reserve**. `BittuChan`. MCP handoff memory; target the affected user workflow, not assumptions about upstream defaults. |
| 05 | **Aider** — [#2219](https://github.com/Aider-AI/aider/issues/2219), 2024-11-01 | User manually summarizes goals after long debugging loops and before closing. Exported transcript differs from compressed model state; asks for editable current history and import/export. | **Open**, enhancement. Old v0.60.1 report; reporter corrects an initial inference about full history being sent. Current failure unverified. | **Reserve; refresh evidence first**. `Shadetail`. Portable handoff with source-backed decisions. |
| 06 | **n8n** — [#38685](https://github.com/n8n-io/n8n/issues/38685), 2026-09-15 | `/new` advances cached generation; after restart the older persisted conversation can become current again. Body identifies durable-history/volatile-generation mismatch. | **Open**, in Linear. Reporter explicitly says analysis against source, not packaged end-to-end reproduction. Claims about tests are not independently verified. | **Conditional artifact candidate**. `hunter3x3-tech`. Monotonic reset fixture; native database authority may be sufficient. |
| 07 | **OpenClaw** — [#95042](https://github.com/openclaw/openclaw/issues/95042), 2026-06-19 | Operator reports 27-agent deployment, empty memory search, and new session identities on reconnect. Existing transcripts become inaccessible through UI. | **Closed not planned**, needs-info/stale. Root-cause section is hypothesis. Transcripts on disk mean inaccessible history, not established destruction. | **Hold**. `1attila2`. Session identity/replay fit, but same thread already screened by AllSource; see deduplication. |
| 08 | **PicoClaw** — [#3150](https://github.com/sipeed/picoclaw/issues/3150), 2026-06-19 | Chinese-language report: user requested storing a directory-use rule; agent overwrote the whole memory instead. | **Closed not planned/stale**, linked #3226. Very short report with template environment fields unfilled. No deployed fix verified. | **Reserve**. `svier0`. Append-only mutation history and recovery. Verify maintained owner and current version first. |
| 09 | **ZeroClaw** — [#5849](https://github.com/zeroclaw-labs/zeroclaw/issues/5849), 2026-04-18 | Proposal and user discussion describe missing cross-session lessons and stale memory; maintainer asks future work to retain raw episodes and measure quality/cost. | **Closed 2026-07-11 after implementation withdrawn**. Residual accepted/in-progress labels do not mean active implementation. This is a feature discussion rather than a controlled loss incident. | **Backend-experiment reserve**. `Svtter`, collaborator `Audacity88`. Strong architectural alignment, weak immediate partner signal. |
| 10 | **Agent Zero** — [#935](https://github.com/agent0ai/agent-zero/issues/935), 2026-01-21 | User reports repeated self-modification breaks installation; restoring operation requires new mount and manually moving memory. | **Closed** after inactivity. Author calls report vague. Memory loss is collateral to app breakage; no deterministic reproduction. | **Low-priority reserve**. `SGC41`. Independent memory backup/recovery can help continuity, not self-modification safety. |
| 11 | **Cherry Studio** — [#12925](https://github.com/CherryHQ/cherry-studio/issues/12925), 2026-02-14 | Web-search assistant usage reportedly causes renderer OOM restart and loses in-flight agent task/session state; log attachments provided. | **Closed not planned**. Description generated by a model from logs; proposed leak causes not independently validated. | **Low-priority reserve**. `JohnXu22786`, assignee `DeJeune`. Durable checkpoints limit crash damage; storage does not repair renderer leaks. |
| 12 | **Dify** — [#21809](https://github.com/langgenius/dify/issues/21809), 2025-07-02 | Developer extending agent chat finds new conversations do not receive prior conversation memory; asks how to implement long-term memory despite stored messages. | **Closed**, question. Product-scope/integration limitation, not demonstrated destroyed records. Old evidence. | **Discovery only**. `orcharddd2024`. Cross-conversation adapter need; confirm current product support and owning app. |
| 13 | **LocalAI / bundled LocalAGI** — [#11975](https://github.com/mudler/LocalAI/issues/11975), 2026-09-11 | Agent memory save without embedding model panics whole server. Restart wipes in-memory job/observable history for other agents. Reporter says encountered crash, not speculative review. | **Closed/fixed 2026-09-28**, linked #12302. Existing dependency fix skips memory writes without RAG database. | **Historical failure; do not pitch as open**. `DetectiveDarklight`, maintainer `mudler`. Error-visible write/recovery fixture only if remaining need appears. |

### Coding-agent ecosystem and recovery evidence

| ID | Project / source / published | Observed evidence | State and limits | Fit and public participant |
| --- | --- | --- | --- | --- |
| 14 | **OpenCode** — [#3031](https://github.com/anomalyco/opencode/issues/3031), 2025-10-08 | BUILD-mode compaction reportedly loses working context and produces wasted changes. Collaborator agrees summary prompt needs improvement. | **Closed by inactivity**, not demonstrated fix. Collaborator rejects suspicion that AGENTS.md is omitted. Current release needs reproduction. | **Reserve**. `agladysh`, assignee `thdxr`. Decision/file handoff receipts; prompt change may be sufficient. |
| 15 | **Qwen Code** — [#6487](https://github.com/QwenLM/qwen-code/issues/6487), 2026-07-07 | `/remember` updates disk index but model sees stale instruction; report also describes compaction evicting loaded memories. | **Closed 2026-08-07**, linked #8640. Maintainer narrows confirmed issue to index refresh; some memory-path protections already existed. | **Historical regression corpus**. `Aleks-0`; maintainers `yiliang114`, `wenshao`. No current unmet need established. |
| 16 | **Cline** — [#4359](https://github.com/cline/cline/issues/4359), 2025-06-21 | Umbrella report covers unloadable task histories, corruption, and ephemeral devcontainer loss. Users cannot resume work. | **Closed 2026-03-03** after substantial storage/history rework. Individual cause claims are reporter analysis. | **Historical evidence**. `celestial-vault`, `abeatrix`. Need a fresh failure before proposing integration. |
| 17 | **Kilo Code** — [#7865](https://github.com/Kilo-Org/kilocode/issues/7865), 2026-03-29 | Chats disappear after restart/update. Other users corroborate visible loss; one locates intact records in cloud view. | **Closed 2026-04-13**. Contributor explains session-version incompatibility and migration path. Visible absence is not proof records were destroyed. | **Historical migration evidence**. `brainRottedCoder`, assignee `marius-kilocode`. Session portability, not an established new storage need. |
| 18 | **Codex** — [#35935](https://github.com/openai/codex/issues/35935), 2026-07-29 | Automatic compaction reportedly loses completed task/subagent state and repeats work. Later commenter adds structural history observations. | **Open**. Private raw rollouts not audited here. Supported integration capabilities and upstream remedy status need checking. | **Affected-user discovery, low upstream buyer fit**. `Lexoid2`, `post1917a`. Decision/action receipts as a complementary experiment. |
| 19 | **Gemini CLI** — [#27368](https://github.com/google-gemini/gemini-cli/issues/27368), 2026-05-22 | `--resume`, then normal launch, makes latest session disappear from `/chat`; commenters describe duplicate session files and cleanup behavior. | **Open**, possible duplicate, linked proposed #28767/#28653 in discussion. Merge/release unverified. Missing list entry alone does not prove deletion. | **Affected-user discovery, low Google buyer fit**. `N1kky-wed`, `chan-yuu`. Session-index/recovery regression. |
| 20 | **OpenHands** — [#13349](https://github.com/OpenHands/OpenHands/issues/13349), 2026-03-11 | Local Docker restart creates new container/conversation rather than resuming old one. Reporter supplies orphan container examples; persisted event files still exist. | **Closed not planned**, stale. Related draft #12685 exists; current continuation support unverified. | **Reserve**. `kbbonawitz-stack`. History replay into fresh runtime; existing event durability means recovery wiring may suffice. |

### Frameworks and memory vendors: discovery, integration, or counter-evidence

| ID | Project / source / published | Observed evidence | State and limits | Fit and public participant |
| --- | --- | --- | --- | --- |
| 21 | **LangGraph** — [#8298](https://github.com/langchain-ai/langgraph/issues/8298), 2026-07-08 | Personal app behind `langgraph dev` loses checkpoints after kill while thread registry survives; state/history requests fail. Body includes flush-loop analysis. | **Closed completed 2026-09-25**. Dev/in-memory runtime scope, not universal production persistence behavior. | **Historical integration proof**. `markpalfreyman`. AllSource repo already contains a checkpointer integration; inspect it before proposing more. |
| 22 | **CrewAI** — [#1222](https://github.com/crewAIInc/crewAI/issues/1222), 2024-08-20 | SQL-chunking user finds long-term SQLite memory remains empty despite file creation. Other commenters report similar symptoms. | **Closed not planned**, needs-info/inactivity. Version 0.41.1; no present-day failure established. | **Low urgency**. `pg179-krishna-innoway`. Inspectable writes; old complaint does not justify replacing current memory system. |
| 23 | **AutoGen** — [#5327](https://github.com/microsoft/autogen/issues/5327), 2025-02-03 | Request identifies asyncio queue's lack of task persistence across restarts; maintainer discusses persisting tasks and agent state with external queue. | **Open**, feature request, draft #5371 discussed. Architectural limitation, not a logged production loss incident. | **Ecosystem only**. `linznin`, maintainer `ekzhu`. A durable store alone is not an execution queue or replay-safe worker. |
| 24 | **PydanticAI** — [#4676](https://github.com/pydantic/pydantic-ai/issues/4676), 2026-03-16 | Same tool's metadata survives ordinary run but becomes `None` across Temporal serialization. Reproducer compares returned history. | **Closed completed 2026-03-25**, #4684. Resolved provenance loss; metadata is app-facing, not sent to LLM. | **Historical integration evidence**. `alexandreczg`. Preserve full tool-event envelope; no current buyer signal. |
| 25 | **Semantic Kernel** — [#12612](https://github.com/microsoft/semantic-kernel/issues/12612), 2025-06-27 | ChatHistoryTruncationReducer deletes system prompt after history exceeds target; executable example in comments. | **Closed completed 2026-03-19**, #13610. Context-reducer defect, not storage/restart failure. | **Low acquisition priority**. `imperonas`. Durable authoritative inputs can support validation, but reducer semantics own the fix. |
| 26 | **LlamaIndex** — [#19128](https://github.com/run-llama/llama_index/issues/19128), 2025-06-18 | Developer cannot reconstruct full tool artifacts/source_nodes from persisted chat. Follow-up traces missing properties when converting tool results to ChatMessages. | **Closed not planned/stale 2025-08-26**. Maintainer and author provide serialization/custom-agent workaround. | **Provenance discovery reserve**. `lsg551`. Full result-envelope persistence; owning application not independently identified. |
| 27 | **Letta Code** — [#2666](https://github.com/letta-ai/letta-code/issues/2666), 2026-06-02 | New agent starts with blank memory despite previous agents' memory files remaining on disk. Reporter manually copies files between identities. | **Open**. Identity/projection isolation, not deletion. Intentional agent isolation must be distinguished from desired shared memory. | **Affected-user discovery; overlapping vendor**. `tcwillis2-max`. Explicit memory identity/linking contract, never indiscriminate sharing. |
| 28 | **Mem0** — [#3931](https://github.com/mem0ai/mem0/issues/3931), 2026-01-22 | Async memory UPDATE fails with KeyError and poor error visibility; production reporter confirms recurrence in follow-up. | **Closed completed 2026-04-08**, #4674. Reporter explicitly says no direct user impact because extraction is asynchronous. | **Historical evidence; overlapping vendor**. `mecampbellsoup`. Failed-write lineage; do not describe as proven customer-visible loss. |
| 29 | **Graphiti** — [#1728](https://github.com/getzep/graphiti/issues/1728), 2026-08-04 | Unrelated additive facts retire still-true facts. Reporter audits four invalidated records: three judged wrong. | **Open**, linked #1729. Fork environment. Only four audited: no estimate of total erroneous retirements is justified. | **Strong reporter-workload qualification; overlapping vendor**. `brentkearney`. Explainable invalidation and source replay. |
| 30 | **Mastra** — [#18551](https://github.com/mastra-ai/mastra/issues/18551), 2026-06-27 | Web chat persists tool call without acknowledgement; trace shows four background tasks dispatched for same tool because next model step lacks result. | **Closed**, linked fix #18556. Concrete historical trace, not an unresolved need. Storage cannot repair incorrect message merge by itself. | **Follow only if need survives fix**. `panliji`. Durable receipts and duplicate-dispatch acceptance fixture. |

For every row, source type is a direct issue/discussion contribution, geography is unknown, and comparable visible-engagement metrics were not collected. Neither stars nor issue reaction counts were used for ranking. The two nanobot sources are separate observations within one project, not two prospects.

## Existing-contact and ownership checks

Read local [qualified builder outreach ledger](../outreach/qualified-builder-outreach.md) before considering any action. This run does not change its approvals or send state.

| Candidate | Local record found | Consequence |
| --- | --- | --- |
| oh-my-openagent | `DevNewbie1826`, #7764, sent 2026-09-10 | Different reporter from #7158, same upstream. Coordinate existing conversation before another approach. |
| Hermes family | `yexxx`, upstream #9055, sent 2026-09-11; `Lei-k` fork sent 2026-08-31; Hermes WebUI thread skipped after fix | No exact `bedpan` match found, but ecosystem already contacted. Do not treat as untouched account. |
| OpenClaw | Exact `1attila2` / #95042 research exists; 2026-09-11 action skipped because closed | Reused evidence, not a newly discovered lead. No comment on closed issue. |
| ZeroClaw | `NiuBlibing`, #10526, awaiting batch approval | Same project, different thread. Existing unsent batch is retired under current contribution policy. No implied approval. |
| Graphiti | #1837 and other issues already screened; documented contributor/assignment constraints | #1728 reporter differs, but repository-level engagement needs coordination and linked PR review. |
| nanobot, Goose, Aider, n8n | No exact candidate/project match found in searched local ledger | This establishes only local-ledger absence, not absence from inboxes, CRM, other docs, or prior private contact. |

Contribution policy in that ledger requires a repository-native artifact and explicit founder approval before public action. Do not send sales comments, comment on closed issues, or compete with an existing owned fix. Re-read current CONTRIBUTING guidance and linked PRs before proposing a contribution. No outgoing message or public action occurred in this run.

## Pain patterns and bounded validation

Counts below describe selected ledger rows. Categories overlap; they are not market shares.

| Pain / trigger | Direct examples | What a useful test must show |
| --- | --- | --- |
| Compaction drops working context | nanobot, oh-my-openagent, Aider, OpenCode, Qwen Code, Codex | Accepted decisions and necessary instructions survive compaction with pointers to original sources. |
| Restart loses or misroutes continuity | Hermes, Goose, n8n, OpenClaw, Gemini CLI, OpenHands, LangGraph | Acknowledged state remains discoverable under the correct identity after process loss. Existing records must be inspected before declaring deletion. |
| Provenance or correction disappears | nanobot, Graphiti, PydanticAI, LlamaIndex | User statement, model inference, correction, and retirement remain distinguishable and reconstructable. |
| Model repeats external actions | Mastra | Issued and confirmed actions remain distinct; retry does not silently duplicate a completed external mutation. |

The first three tests below are the proposed qualification sequence. They are plans, not artifacts built or verified in this research run.

1. **nanobot correction test.** Build a tiny source/correction fixture and ask an affected builder to supply one real equivalent. Record role, source event, sequence, and explicit supersession. Run original consolidation and provenance-aware projection against identical input. Success: correction wins, each recall points to its source, and stale inferred preference does not reappear after rebuild. Stop if current upstream already fixes it or builder has no remaining workflow.
2. **Hermes provider-loss test.** After checking #49302, remove configured provider in an isolated fixture. Success: runtime reports actual provider, accepted writes have receipts, restart retrieves acknowledged fact from intended store, and missing provider cannot produce a false durable-write success. Stop if native fix fully addresses need or integration requires unsupported interception.
3. **oh-my-openagent skill-restoration test.** After checking #7183, load a versioned skill, compact, then invoke its specialized workflow. Success: same authorized skill version is restored with load/source evidence and required tool behavior survives. Stop if linked fix already covers this or team only needs a native prompt/checkpoint patch.
4. **Graphiti invalidation test.** Ask reporter whether replay/audit remains needed after #1729. Preserve raw source facts and record invalidation reasons. Success: additive facts do not retire unrelated facts, and a reviewer can reconstruct both prior and current state. Stop if no owning project or existing solution suffices.
5. **Goose handoff test.** Verify supported native resume first. Compare it with MCP-backed decision/next-action recovery on a real refactoring workflow. Success: a new process resumes correct goal and accepted decisions without manually pasted summary. Stop if native resume already meets the user's need.

For each accepted partner, capture current version, before/after transcript, reproducible input, source references, acknowledged writes, recovered state, and the builder's assessment of usefulness. Track application → agreed fixture → integration completed → successful restart/correction test → repeat use → paid continuation. No budgets or willingness-to-pay values were observed.

AllSource's event store is a potential substrate, not a guarantee of correct recall. Correct routing, projection logic, model context selection, and external-side-effect reconciliation still require integration. Existing local references to inspect before building: [restart proof](../demos/event-store-restart-proof.md), [embedded Prime integration](../guides/EMBEDDED_PRIME_INTEGRATION.md), and `integrations/langgraph-checkpoint-allsource/`.

## Research progress and remaining work

| Stage | State | Evidence / next dependency |
| --- | --- | --- |
| Locate repository and existing context | Complete | `/Users/decebaldobrica/Projects/founder-mode/all-source`; existing outreach policy and prior contacts inspected. |
| Public discovery | Complete for requested coverage | 30 distinct project rows and 31 primary evidence URLs retained; additional weak feature requests excluded from core ledger. |
| Evidence and status checks | Complete within stated retrieval limits | Primary bodies opened; closed/fixed/withdrawn and source-analysis-only caveats recorded. No live runtime reproductions performed. |
| Rank and deduplicate | Complete for research scope | Five provisional candidates, three first qualification targets; project-level prior-contact conflicts recorded. |
| Persist and index | Complete locally | This report linked from [documentation hub](../README.md). Existing unrelated changes preserved. |
| Qualify builders / build artifacts | Not started | Requires current PR/release check and real workflow owner agreement; no acceptance inferred. |
| Outreach | Not started | No messages sent; current ledger policy requires exact public-action approval after artifact exists. |
| Version control | Not committed or pushed | User's current instructions forbid commits unless asked and branch creation without explicit permission. Existing checkout is dirty `main`, behind remote. No trunk push, branch creation, reset, or stash performed. |

Tracker note: local `cn list --toon --all` returned records but also reported WAL initialization denied by filesystem permissions. No tracker mutation or completed-task claim was made from that read. Research progress is recorded here.

### Tool cost and access limits

treg discovery calls used `crawl4ai.web.search.rich` at quoted $0.00015 each. Two successful calls reported $0.00030 total:

- `ee741d69076c4172b80e9a1b66516494`
- `c52607a1ef444692990e2b2ff0158d01`

Both returned broad results despite GitHub-restricted queries. An unsupported-argument call was rejected before sending; no charge reported. Search snippets from these calls were excluded. Source verification used opened GitHub bodies through web tools. `gh pr list` could not connect under the current shell network restriction; no PR-state claim or git mutation depends on it.

Limits: no private communities, no personal contact enrichment, no inbox/CRM audit, no installed-version reproduction, no comprehensive current PR ownership audit, and no confirmation that any project or reporter will join the program. This sample provides a qualification queue, not an approved send list.
