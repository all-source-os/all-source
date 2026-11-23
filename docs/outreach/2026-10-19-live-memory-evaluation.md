# Live memory evaluation and updated outreach

Date: 2026-10-19. Status: live runs complete within the scope below; nothing sent.

Current OMO qualification: [full-plugin results and replacement unsent draft](./2026-10-20-omo-full-plugin-evaluation.md) resolve the earlier loader gap. Native recovery counts remain 1 → 1 → 2; no design-partner need is established.

## Findings

**Nanobot retained the expected value in all three synthetic cases after two archive/Dream cycles and fresh-container recall. Hermes warned during a missing-provider outage, saved the outage marker in built-in memory, and retained both stores after restoration. Neither result supports a current memory-loss pitch.**

| Candidate | Measured result | Updated disposition |
| --- | --- | --- |
| nanobot | 3/3 value-retention cases passed. Native history tools could read all supplied originals. Model-led source retrieval found one original; two queries missed existing history. | Downgrade from memory-loss design-partner target to retrieval/evaluation contribution. A real builder's unmet requirement remains necessary. |
| Hermes | Baseline external write/recall, missing-plugin fallback write, and restored-provider recall completed. Warning visible. Baseline and outage markers remained in different stores. | Hold design-partner invitation. Discuss explicit destination/migration requirements only if a real provider/user needs them. |
| oh-my-openagent | [Native-event compaction integration](./2026-10-20-omo-full-plugin-evaluation.md) restored complete bodies, suppressed immediate duplicates, then restored again beyond cooldown. Full loader and native compaction hooks verified on the pinned PR. | First technical contribution candidate; existing owner/reviewer and prior project contact require coordination. Product invitation held. |

Sources and fix state remain in the [fix audit](./2026-10-19-memory-fix-audit-and-drafts.md). [Machine-readable summary](./artifacts/2026-10-19-live-memory-evaluation/results-summary.json) and [artifact/evidence index](./artifacts/2026-10-19-live-memory-evaluation/README.md) accompany these findings.

## Isolation and method

- Nanobot source: HKUDS/nanobot commit 8778c697744f8334c5794cd37502e8eb77ee5585. Native Python APIs execute archive, Dream and recall. This main snapshot reports package version 0.3.5; it is not an exact v0.3.5 tag test.
- Hermes source: release v0.22.2, commit d923a5eb487a17c0a68801bba9ef0f56c81040a8.
- Requested model: OpenRouter openai/gpt-4.1-mini. Actual remote provider calls, no mocked model or tool routing. Nanobot final arm: temperature 0, max output 1,024 tokens. Hermes: max output 1,200, max six agent iterations.
- Docker images pinned by digest in [manifest](./artifacts/2026-10-19-live-memory-evaluation/manifest.json). Containers use disposable homes and profiles; production homes, messaging credentials and Docker socket were not mounted.
- Only an existing OpenRouter model key entered the live containers. It is absent from retained evidence. No top-up, account setting, or external message action occurred.
- Each live stage used an outer 300-second deadline and native request/runtime budgets; final harness also has a 270-second process alarm. Baseline Hermes used its native 150-second run budget and outer deadline before the alarm was added. Containers exited and were removed.
- One synthetic run per case in the final bounded arm. These are observations, not statistical reliability estimates.
- Native Python harnesses call the projects' Python interfaces. Reimplementing those interfaces in Rust would test different code.

## Nanobot results

The supplied corpus contains two transcript chunks per case. The harness uses the native context builder to include previous checkpoint and current memory, then the real archiver and restricted Dream file tools. Each case runs two cycles. A new container then loads only persisted projection files for the first recall arm.

| Case | Two Dream cycles | Fresh projection-only recall | Original-source result |
| --- | --- | --- | --- |
| Explicit correction over repeated inference | Completed | Light mode retained; dark-mode guess not promoted as current preference. | Projection response cited USER.md. Separate history-enabled arm searched “display mode,” got no match, and admitted original was not found. Native control found and read the original using “light mode.” |
| Situational preference stays scoped | Completed | Mobile onboarding remained undecided; blue stayed specific to landing-page mockup. | With native history tools enabled, model searched colour/color variants, read source session, and quoted the original user statement. |
| Delivery-reference control | Completed | Option 2 retained: build parallel projection, compare, then switch reads. | History-enabled model searched a long paraphrased phrase and missed original. Native control found/read original using “Option 2.” |

Final arm: **31 instrumented provider calls**, six completed Dream cycles, three projection recalls, three history-enabled recalls. No error-finish responses in this final arm. The corpus is small and synthetic; this does not estimate production recall accuracy.

### Source attribution limits

Projection-only recall intentionally had no history retrieval tools. It cannot establish that the product lacks source recovery. The diagnostic history arm persisted the supplied original transcripts using native SessionManager in a separate persistent session root, then explicitly asked a fresh agent to use search_sessions/read_session. That checks retrieval availability; it does not prove automatic source capture through normal UI ingress.

Native engine control found all three source sessions and read their originals. Search match counts also include the fresh recall session, so they are not independent corroboration. Two model searches failed because their query text did not match; records were still present. No claim of destroyed history, absent native provenance capability, or an AllSource cure is supported.

Minimal workspaces were used, not the full first-run onboarding profile. Archive and Dream were invoked through native APIs with supplied fixture transcripts; conversations were not generated through the normal UI. Automatic pressure-trigger selection was not tested.

### Excluded setup attempts

Earlier runs are excluded from product conclusions. An initial harness omitted native prior-checkpoint prompt construction. A later concurrent run received OpenRouter 402 in_flight_budget_exhausted responses. Final cases ran serially with bounded output in new workspaces. These errors are retained as execution limits, not scored as memory failures. Final source and raw traces are supplied; preliminary scratch runs are not the reported sample.

## Hermes outage results

The standalone provider is a synthetic SQLite plugin, not AllSource, Mnemosyne, Honcho, Hindsight or a production vendor. It implements explicit write/recall and intentionally has no mirroring/migration hook. Hermes startup, plugin discovery, tool injection, tool dispatch and model execution are real.

| Stage, separate container | Activation and evidence | Store outcome |
| --- | --- | --- |
| Baseline | Provider active; model called probe_memory_write and probe_memory_recall; supported SQLite query confirmed baseline canary. | Baseline canary external only; built-in memory empty. |
| Plugin absent, same disposable profile volume | Recovery could not install unknown test plugin. Startup stdout and warning log named missing provider and instructed install/config change. Only built-in memory tool available; write receipt reported success and target memory. | Outage canary saved to memories/MEMORY.md. External SQLite unchanged. |
| Plugin restored, new container | Provider active; native recall listed external facts. SQLite and built-in file inspections agreed with model's separation of sources. | Baseline still external. Outage canary still built-in. Neither was destroyed. No generic migration observed for this adapter. |

All three conversations completed without model failure: six native API calls total.

The outage prompt explicitly told the model that the provider was missing and asked it to distinguish destinations. Therefore its disclosure does not establish that an ordinary uninformed user would receive equally clear destination guidance. Independently, startup warning was visible in stdout/logs. The notice callback list was empty; that must not be misread as no warning.

Missing outage marker in synthetic external DB is expected without an adapter migration hook. It proves split destination for this setup, not a defect in production provider migration. No current-main comparison, actual Docker image update of a vendor package, crash-kill/power-loss test, or AllSource provider integration was run.

## Updated exact drafts: unsent

These replace the earlier pending-evaluation wording. Attach the named local evidence files only after making them available to the recipient through an approved attachment/source route. Local filesystem links are not outgoing URLs. User explicitly requested no sending; no approval or send is implied.

### Nanobot: evaluation contribution; downgrade acquisition priority

Recipient/proposed destination: joaoinacio, open #4212, only after current owner/duplicate checks. No design-partner invitation.

Attach corpus, native harness, summary, and relevant result traces.

> I ran the three synthetic cases against nanobot at 8778c697 with OpenRouter gpt-4.1-mini: two archive/Dream cycles, then recall in a new container. All three retained the expected value: light mode after correction, no global blue preference, and the correct meaning of Option 2.
>
> Source retrieval was less consistent. With saved-session tools enabled, one case found and quoted the original; two searches missed it. A direct check through the same native tools could read all three originals, so this is not evidence of lost history.
>
> I work on AllSource. The attached corpus, harness and traces have no AllSource dependency. Would the retrieval cases be useful for your existing regression work? I would not propose a memory-store change from these results.

Stop if equivalent corpus already exists, maintainers decline the contribution, or no real user workflow needs better retrieval. Do not revive the original compaction-loss pitch.

### Hermes: held destination-contract discussion

Recipient/proposed destination: bedpan, open #49200, only if current real-provider pain remains and exact action is later approved.

Attach synthetic provider, harness, three-stage summary and warning/tool receipts. No hosted-trial invitation.

> I ran a provider-outage probe on Hermes v0.22.2 using a test-only SQLite plugin. After removing the plugin and recreating the container, Hermes warned that the provider was missing and saved the new marker through built-in memory. After restoring the plugin, the baseline marker remained external and the outage marker remained built-in. Neither was lost.
>
> This adapter has no migration hook, so the result says nothing about migration in your provider. I work on AllSource, but this probe does not use it. Does your current setup still need a different destination or recovery contract, or has the newer warning/recovery path resolved the issue?

Hold remains until a real user/provider establishes unmet behavior. Do not claim current silent loss or automatic vendor migration failure.

### Oh-my-openagent: provider-payload results

[Current report and exact unsent draft](./2026-10-21-omo-provider-payload-evaluation.md). Provider-captured rerun: skill text on passed 4/6 post-compaction workflows; off passed 0/6. All four baselines passed; reload stayed blocked. 73 provider calls captured with full native-step coverage. Full skill reached 4/4 fresh-on first continuation requests; 0/2 cooldown-on and 0/6 off. Native contribution only; AllSource demand unqualified.

## Progress and remaining limits

- Live nanobot memory cycles and fresh-process recall: complete for three supplied cases.
- Native source-retrieval diagnostic and store control: complete.
- Hermes baseline/outage/restore: complete for synthetic standalone provider.
- Evidence, harnesses, source/dependency/image identities: retained under docs and indexed.
- Shortlist and earlier draft documents: updated to point here; prior contact/send states unchanged.
- Sending, publishing, commits, pushes, and production deployment changes: none.

No confirmed design partner, production vendor failure, improved outcome from AllSource, or purchase intent emerged from these tests. Further qualification needs a builder's actual unmet workflow, not another generic loss claim.
