# #7183 provider-input evidence

Tested head: 93d44a0760448f7f07d2795c6239d51db5ab6576. OpenCode 1.18.22; harness Bun 1.3.9. Full bundle was built using Bun 1.4.3. Model route: minimax/minimax-m3.1-flash:free through OpenRouter. Requested temperature 0; low reasoning; output cap 2,048. Two reused synthetic fixture pairs, no seed/backend pin.

| Arm | Skill text | Baseline | After compaction 1 | After compaction 2 | After compaction 3 |
| --- | --- | --- | --- | --- | --- |
| c1a | on | pass | pass | fail | pass |
| c1b | off | pass | fail | fail | fail |
| c2a | off | pass | fail | fail | fail |
| c2b | on | pass | pass | fail | pass |

All 73 provider requests returned HTTP 200 and match native step-start counts: 61 checkpoint calls plus 12 compaction calls. All four baselines first lacked the query; a native skill load then supplied full instructions through a tool message. Initial queries/receipts were not put in task prompts.

First requests after compaction contain the complete skill and exact current query only in the four fresh skill-on checkpoints. Cooldown skill-on requests retain prior queries 0/1, not current query 2. Off requests retain baseline query 0, not the later requested query. Summarizer input and continuation input are indexed separately in provider-analysis.json, including roles and exact text/hash matches. A full skill in summarizer input does not prove its presence in the next continuation.

Skill-off removes only skill text at native recovery dispatch. Agent/model/tools, markers and metadata remain. All 12 injected summary instruction objects match. The native cooldown is unchanged: accepted/stored recovery counts 1 → 1 → 2. Sixteen readiness barriers precede tasks; no recovery arrives during task turns. Four initial skill executions succeeded; eight later attempts were denied before native execution, including cached reloads. All 16 original source records remained intact in native read-only db checks.

This verifies provider-bound request content and controlled tool behavior. Provider-internal transformations, model attention and production-wide reliability remain unobserved. The source-scope policy restricts broad/history/file fallbacks; the controller uses a short audit prompt and five-step cap. Manual compactions only. Native large-payload bounds were measured separately and are not claimed here as live-model acceptance.

The fixture has no AllSource dependency. Full provider bodies contain synthetic sessions and upstream plugin prompts. Headers, credentials, mail/CRM audits and unrelated workspace files are excluded.
