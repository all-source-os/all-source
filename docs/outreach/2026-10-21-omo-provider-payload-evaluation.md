# Oh-my-openagent #7183: provider-payload capture

Date: 2026-10-21. Status: two counterbalanced matched pairs completed; outreach unsent.

## Findings and qualification

**Provider-captured rerun: skill text on passed 4/6 post-compaction workflows; off passed 0/6. All four baselines passed; reload stayed blocked.** Full skill body reached the first provider-bound continuation request at **4/4** fresh-recovery skill-on checkpoints, **0/2** cooldown skill-on checkpoints and **0/6** skill-off checkpoints. The exact current query appeared anywhere in those request bodies at **4/4**, **0/2** and **0/6** respectively.

All **73** captured HTTPS request bodies correspond to native model steps and received HTTP 200. Coverage includes all checkpoint calls, later calls after tool results, and all 12 compaction calls. Capture verifies actual **provider-bound content** beyond the [previous storage-readiness evidence](./2026-10-21-omo-readiness-barrier-evaluation.md). Provider-internal prompt construction and model attention remain unobserved.

Disposition remains **native regression contribution; AllSource design-partner demand unqualified**. These traces support the existing owner's native work and establish no requirement for another service.

## Matched setup and capture boundary

Pinned [#7183](https://github.com/code-yeongyu/oh-my-openagent/pull/7183) head **93d44a0760448f7f07d2795c6239d51db5ab6576**, unchanged full plugin bundle and packaged assets. OpenCode **1.18.22**, Bun **1.3.9**, OpenRouter **minimax/minimax-m3.1-flash:free**. Same controlled audit-agent prompt, five-step cap, temperature 0 request, low reasoning and 2,048 output-token cap. No seed or provider-backend pin.

Pair c1 ran **on → off**; c2 ran **off → on**, serially with fresh HOME/XDG profiles and native session databases. Each pair reuses its earlier fixture byte-for-byte: c1 = b1/s1; c2 = b2/s2. The model selects the initial audit skill from metadata. No controller skill execution or forced initial skill-tool choice. Tasks after loading contain checkpoint/source identifiers, not expected queries or receipts.

The original skill-only dispatch observer remains byte-identical and is delegated directly. Its off arm removes only checkpointed skill text at native HTTP recovery dispatch. Agent/model/tools, noReply, synthetic metadata and internal markers stay intact; the native recovery hook, capture and cooldown remain enabled. The recovery-barrier implementation is also unchanged: eligible rounds require accepted and stored recovery before task dispatch; immediate round 2 requires completed native handling with cooldown suppression.

A separate outer observer installs a capture function before native plugin initialization. For POST requests to **https://openrouter.ai/api/v1/chat/completions**, it clones/reads the UTF-8 body, saves that content and its SHA-256, then invokes the original fetch with the **original resource and options**. It never rewrites the outgoing body, URL, headers or response stream. HTTP response status is recorded; response stream content remains available to the native runtime and is verified through native outputs. Authorization headers, cookies and credentials are not retained. The API route and request counts are observed, not inferred from configuration.

Controller-only phase tags distinguish compaction, readiness and checkpoint calls; tags are not added to model request bodies. A checkpoint fails infrastructure validation if captured requests do not match native step-start parts. Final analysis repeats that comparison for every checkpoint and compaction, verifies every retained body's hash and checks all response statuses. No unclassified provider calls or provider transport failures occurred.

## First provider request after each compaction

These are the first calls before the model acts on each checkpoint. Later requests can contain queries/receipts produced by the model or returned by tools; those are separately indexed and do not count as proof of initial instruction availability. Query stages are the opaque map entries visible **anywhere in the complete body**, including assistant tool-call arguments and tool schemas. Procedure coverage counts three literal procedure clauses in message content; it does not classify every paraphrase.

| Arm / skill text | Compaction | Full skill body | Literal procedure clauses | Query stages anywhere | Current query anywhere | Workflow | Captured request |
| --- | --- | --- | --- | --- | --- | --- | --- |
| c1a / on | 1 | yes | 3/3 | 0, 1, 2, 3 | yes | PASS | [5](./artifacts/2026-10-21-omo-provider-payload/c1a-provider-0005.json) |
| c1a / on | 2 | no | 0/3 | 0, 1 | no | FAIL | [8](./artifacts/2026-10-21-omo-provider-payload/c1a-provider-0008.json) |
| c1a / on | 3 | yes | 3/3 | 0, 1, 2, 3 | yes | PASS | [14](./artifacts/2026-10-21-omo-provider-payload/c1a-provider-0014.json) |
| c1b / off | 1 | no | 0/3 | 0 | no | FAIL | [5](./artifacts/2026-10-21-omo-provider-payload/c1b-provider-0005.json) |
| c1b / off | 2 | no | 0/3 | 0 | no | FAIL | [11](./artifacts/2026-10-21-omo-provider-payload/c1b-provider-0011.json) |
| c1b / off | 3 | no | 0/3 | 0 | no | FAIL | [17](./artifacts/2026-10-21-omo-provider-payload/c1b-provider-0017.json) |
| c2a / off | 1 | no | 0/3 | 0 | no | FAIL | [5](./artifacts/2026-10-21-omo-provider-payload/c2a-provider-0005.json) |
| c2a / off | 2 | no | 0/3 | 0 | no | FAIL | [11](./artifacts/2026-10-21-omo-provider-payload/c2a-provider-0011.json) |
| c2a / off | 3 | no | 0/3 | 0 | no | FAIL | [17](./artifacts/2026-10-21-omo-provider-payload/c2a-provider-0017.json) |
| c2b / on | 1 | yes | 3/3 | 0, 1, 2, 3 | yes | PASS | [5](./artifacts/2026-10-21-omo-provider-payload/c2b-provider-0005.json) |
| c2b / on | 2 | no | 0/3 | 0, 1 | no | FAIL | [8](./artifacts/2026-10-21-omo-provider-payload/c2b-provider-0008.json) |
| c2b / on | 3 | yes | 3/3 | 0, 1, 2, 3 | yes | PASS | [14](./artifacts/2026-10-21-omo-provider-payload/c2b-provider-0014.json) |

The request index also records exact message roles/indices, restoration headers, skill names, query entries in content versus tool-call arguments, and exact matches to native initial skill output or the latest native summary. Full-body matches require the complete fixture skill text, not its name or end marker alone.

## What the summarizer received and retained

Compaction requests are different model calls from subsequent workflow continuations. A full skill in the summarizer's input does not establish its presence in the next continuation. The table compares captured summary inputs with actual native summary output; the first-request table above checks the resulting continuation independently.

| Arm / skill text | Compaction | Full body in summary input | Query stages in summary input | Full body in generated summary | Query stages retained in summary |
| --- | --- | --- | --- | --- | --- |
| c1a / on | 1 | yes | 0, 1, 2, 3 | no | 0 |
| c1a / on | 2 | yes | 0, 1, 2, 3 | no | 0, 1 |
| c1a / on | 3 | no | 0, 1 | no | 0, 1 |
| c1b / off | 1 | yes | 0, 1, 2, 3 | no | 0 |
| c1b / off | 2 | no | 0 | no | 0 |
| c1b / off | 3 | no | 0 | no | 0 |
| c2a / off | 1 | yes | 0, 1, 2, 3 | no | 0 |
| c2a / off | 2 | no | 0 | no | 0 |
| c2a / off | 3 | no | 0 | no | 0 |
| c2b / on | 1 | yes | 0, 1, 2, 3 | no | 0 |
| c2b / on | 2 | yes | 0, 1, 2, 3 | no | 0, 1 |
| c2b / on | 3 | no | 0, 1 | no | 0, 1 |

All four baselines provide positive capture controls: the first request lacks the initial query, the model invokes native skill loading, and a later provider request contains the complete native skill output as a tool message. Provider analysis records every subsequent call, including expected reload-denial messages. Rejected reloads never supply skill content.

## Workflow results and controls

| Arm | Skill text | Baseline | Compaction 1 | Compaction 2 | Compaction 3 | Blocked reloads |
| --- | --- | --- | --- | --- | --- | --- |
| c1a | on | PASS | PASS | FAIL | PASS | 1 |
| c1b | off | PASS | FAIL | FAIL | FAIL | 3 |
| c2a | off | PASS | FAIL | FAIL | FAIL | 3 |
| c2b | on | PASS | PASS | FAIL | PASS | 1 |

All 16 readiness barriers released before native task creation; no recovery arrived during a task turn. Eight eligible native recovery requests were accepted and stored first; four immediate checkpoints settled inside the original 60,000 ms cooldown without a new dispatch. The third compaction retained the real 62,000 ms wait. No clock override or forced restoration.

All eight recovery dispatches passed non-skill-envelope and part-metadata checks. All 12 summary-injection output.context objects were byte-identical, SHA-256 **e96ccb7666e4ea77dd708545774377cf7a8cb64be0212345eaadbb9095de50a3**. Generated summaries and histories may diverge as treatment consequences. Four initial native skill loads succeeded; all 8 later attempts were denied before native code, including cached reloads. Broad searches, other-session reads, general file tools and MCPs were equally unavailable; the scope gate validates source/query syntax without revealing answers.

All 16 first checkpoint requests also matched on actual provider settings and tool schemas. They exposed only session_info, session_search and skill, with tool_choice=auto; schema SHA-256 **27cb06f46160cb92558af10f2e0a870671db500d9b2754b9bf85dc3e7ee25498**. Later runtime calls can use tool_choice=none at the step cap. No first checkpoint request contained its expected receipt anywhere in the JSON body; receipts had to come from source-tool results.

Host/OMO configs and dependency locks match across arms; original dispatch observer, barrier and compiled plugin hashes were verified. Each native plugin initialized 14 tools and 15 hook/object entries. Exact-subject native logs show one skill and agent/model/tools captured each round, plus two successful restorations per arm. All 16 original source records remained intact when checked after evaluation through OpenCode's own read-only db CLI. Owned servers were reaped; final owned-runtime process check was empty.

## Interpretation and remaining limits

This test distinguishes stored recovery, provider-bound continuation input and actual specialized-tool success. Skill names or partial instructions can survive in summaries while unused query entries do not; use the per-message index and generated-summary records for the exact retained content. Full-body absence is not evidence that all guidance or underlying history disappeared.

Captures show what the client sent to the provider and that the endpoint accepted it. They do not expose provider-side transformations, backend prompt caches, the final internal model context or model attention. Literal procedure matching does not detect all paraphrases. Current-query presence is checked across the full JSON body; exact body/marker matches and message provenance are reported separately.

Two reused synthetic fixtures, opposite order, one requested model route and unpinned seed/backend. No statistical effect estimate or production-wide reliability claim. A short audit prompt, five-step cap, supplied skill metadata and constrained fallback routes remain. Manual auto:false compactions only; automatic pressure selection, autocontinue, restarts, changed/missing skill files and production/external MCP tools untested. [Earlier payload bounds](./2026-10-20-omo-full-plugin-evaluation.md) remain separate byte measurements; these small workflow payloads do not establish large-payload acceptance.

## Ownership and prior contact

Existing [ownership/contact audit](./2026-10-19-omo-compaction-evaluation.md#ownership-and-prior-contact) applies; PR/inbox state was not refreshed. AceRothstein71 owns the existing patch; [MoerAI's review](https://github.com/code-yeongyu/oh-my-openagent/pull/7183#issuecomment-5489222380) already requests bounds/cooldown coverage. Prior [#7764 approach to DevNewbie1826](https://github.com/code-yeongyu/oh-my-openagent/issues/7764#issuecomment-5616313504) must be coordinated before any new project contact. Absence of reply is not consent.

## Current outreach draft: unsent

Use the [send-ready final comment and evidence package](./2026-10-21-omo-send-ready.md#exact-comment). Ownership and prior-contact checks were refreshed for that package. No send performed.

## Evidence and progress

[Artifact index](./artifacts/2026-10-21-omo-provider-payload/README.md), [provider request analysis](./artifacts/2026-10-21-omo-provider-payload/provider-analysis.json) and [combined results](./artifacts/2026-10-21-omo-provider-payload/results-summary.json) retain provider bodies, hash/HTTP receipts, roles and instruction markers per call, native model-step coverage, barrier evidence and native workflow/source checks. Docs index, shortlist and outreach ledger updated locally. No branches, commits or pushes created.
