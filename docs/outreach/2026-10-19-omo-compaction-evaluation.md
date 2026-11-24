# Oh-my-openagent #7183: real compaction and payload evaluation

Date: 2026-10-19. Status: evaluation complete within the integration boundary below; outreach unsent.

Current result: [full-plugin evaluation and replacement unsent draft](./2026-10-20-omo-full-plugin-evaluation.md) resolve the loader gap below. This report retains the earlier external-bridge measurements.

## Result and shortlist

**The PR restores skill instructions, suppresses an immediate duplicate, and restores the same instructions again after the recovery cooldown. No per-skill cap was observed through a 1 MiB body; no aggregate cap was observed through eight 64 KiB bodies.** These are measured byte sizes, not tokenizer counts or evidence of a provider outage.

Oh-my-openagent remains the strongest **technical contribution** candidate among the three evaluated projects. Its author already owns the native restoration patch, and reviewer feedback already requests this coverage. A native bounded checkpoint fix may meet the need without AllSource. Hold the design-partner invitation until an affected builder establishes an unmet durability/provenance requirement. Nanobot remains a retrieval/evaluation contribution; Hermes remains held for real-provider qualification.

## Pin, runtime and integration boundary

- Source: [PR #7183](https://github.com/code-yeongyu/oh-my-openagent/pull/7183), head **93d44a0760448f7f07d2795c6239d51db5ab6576**. Source archive extracted separately under `/tmp`; production source unchanged.
- Runtime used for reported results: OpenCode **1.18.22**, Bun **1.4.3**. Real OpenRouter `openai/gpt-4.1-mini` summary calls, output limited to 1,024 tokens. Five successful summaries across three disposable sessions; no model errors in reported snapshots.
- Actual PR `createSkillTool().execute()` loaded synthetic skills. Actual `createSessionCompactingHandler()` captured checkpoint state. Real HTTP `/session/{id}/summarize` performed compaction; native SSE `session.compacted` events drove the unchanged PR injector's `event()` method. Actual SDK recovery requests wrote synthetic messages into native OpenCode storage. HTTP message reads verified full body and end markers.
- This is an **external integration bridge**, not a full installed OMO plugin test. Configured local QA plugin initialization could not be established on either OpenCode 1.19.3 or 1.18.22. Those loader attempts are excluded from patch conclusions. The bridge invokes capture before native compaction and forwards observed native events; it does not prove native plugin registration, autocontinue dispatch, or injected compaction context in the summary prompt.
- Skill loading used the native tool API with precomputed fixtures and a test permission callback; it was not selected by a model. Skill bodies enter the real recovery message, but are not supplied as large pre-compaction tool outputs. This isolates recovery payload bounds without charging a model to summarize the large fixtures.
- Explicit summarize requests test compaction behavior; automatic context-pressure selection, downstream model acceptance of the restored large body, specialized tool use afterward, and process-restart durability were not tested.
- Each server used isolated HOME/XDG/config/data directories, loopback HTTP, and a parent-owned child handle. Servers exited; no QA server remained at final process check. No production DB or messaging account was mounted. No AllSource dependency was present.

## Repeated compaction

One skill was loaded once. Its actual body was **1,018 bytes**; the early fixture requested 1,024 but reserved six excess padding bytes. Recorded tool body size is authoritative. The retained harness has `QA_LEGACY_FIXTURE=1` to reproduce this fixture exactly.

| Native compaction | Successful summaries stored | Recovery messages stored | Recovery text bytes | Interpretation |
| --- | ---: | ---: | ---: | --- |
| 1 | 1 | 1 | 1,259 | Body and end marker restored. |
| 2, immediate | 2 | 1 | 1,259 total | Immediate duplicate suppressed. Same stored part ID. |
| 3, after cooldown | 3 | 2 | 2,518 total | New part ID contains identical complete recovery text. No intervening skill reload. |

First and third processed compaction events were **68,168 ms** apart. The driver inserted a real 61,000 ms wait before the third summarize request; clocks were not advanced or mocked. The native recovery cooldown is 60,000 ms. Each summary completed without an API error. The checkpoint was captured again from the session-loaded skill registry, so clearing only a previous checkpoint would not establish one-shot semantics while the registry retains the skill.

The two recovery messages are retained history. This does not prove that every old message remains in the model's active post-compaction context, or that token cost grows linearly forever.

## Payload bounds

| Fixture | Skill-body bytes | Stored recovery text bytes | Complete bodies/end markers | Observed bound |
| --- | ---: | ---: | --- | --- |
| One large skill | 1,048,576 | **1,048,817** | Yes | No per-skill truncation through 1 MiB. |
| Eight skills, 65,536 bytes each | 524,288 total | **524,655** | All eight | No aggregate truncation through 512 KiB. |

These counts include the recovery prompt and skill wrappers. Earlier formatter-only measurements counted a narrower string and therefore differ. They are not contradictory results. Successful storage does not establish that a later model request can afford or accept the payload. No universal infinite/unbounded transport claim, tokenizer estimate, memory exhaustion, or destroyed data is asserted.

## Ownership and prior contact

Live GitHub API check: #7183 remains **open/unmerged**, authored by **AceRothstein71**, unchanged head above, updated September 1. No assignees or requested reviewers were listed. Repository owner is `code-yeongyu`; original [#7158](https://github.com/code-yeongyu/oh-my-openagent/issues/7158) reporter is `sghng`. Neither author nor reporter has agreed to a design-partner program.

[MoerAI's September 1 comment](https://github.com/code-yeongyu/oh-my-openagent/pull/7183#issuecomment-5489222380) already asks for per-skill/total caps or one-shot clearing, tests beyond cooldown, and injector/skill/typecheck gates. Its token estimates remain reviewer claims, not our measurements. Offer evidence to the existing owner; do not present the defect as newly discovered or open a competing implementation.

The local outreach ledger records a prior AllSource approach to **DevNewbie1826** on the same project. Its [actual public comment](https://github.com/code-yeongyu/oh-my-openagent/issues/7764#issuecomment-5616313504), by `decebal`, was created **2026-09-10T09:26:47Z**. It offered an identity-binding regression fixture and promised no follow-up after “pass.” The current #7764 comment list contains a maintainer fix-status comment and that outbound comment; no recipient reply or opt-out was observed there. Absence of a reply is not consent.

Connected Gmail searches covered project names and `AceRothstein71`, `sghng`, `DevNewbie1826`. All-mail search returned one unrelated newsletter; sent-mail search returned zero matches, both without another result page. This does not audit other mailboxes, private chats, aliases, or a separate CRM. Coordinate the existing project contact before a new approach.

## Current outreach draft: unsent

Use the [replacement full-plugin draft](./2026-10-20-omo-full-plugin-evaluation.md#current-outreach-draft-unsent). Existing ownership and prior-contact restrictions still apply.

## Verification and artifacts

- Existing skill-reinjection test file: **3 passed, 8 assertions**.
- Existing skill-tool factory test file: **9 passed, 17 assertions**.
- These scoped tests ran in separate Bun processes from `/tmp`, without the repository-wide preload. The root command failed before running tests because unrelated vendored LSP `npm ci` failed. Scoped successes do not certify the repository-wide setup or full typecheck. No upstream code change was made.
- [Evidence and reproduction index](./artifacts/2026-10-19-omo-compaction/README.md), [machine-readable summary](./artifacts/2026-10-19-omo-compaction/results-summary.json), native message snapshots, event receipts, server logs and source hashes are retained.
- Initial OpenRouter output-budget rejection and loader attempts are setup exclusions, not scored memory failures. Final reported runs used bounded output. Evidence excludes credential values and provider cookies.
- Documents updated locally. No branches, commits, pushes, public comments, email drafts or sends created.
