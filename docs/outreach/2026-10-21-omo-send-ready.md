# OMO #7183 send-ready package

Prepared: 2026-10-21. Status: ready for user review; no send authorised or performed.

## Destination and sender

One top-level technical comment on [existing PR #7183](https://github.com/code-yeongyu/oh-my-openagent/pull/7183), addressed to @AceRothstein71, from verified GitHub identity `decebal`. Keep the conversation with the existing patch owner. The note adds provider-input evidence alongside [existing review](https://github.com/code-yeongyu/oh-my-openagent/pull/7183#issuecomment-5489222380); it makes no design-partner invitation.

## Live ownership and prior-contact checks

- PR remains open/unmerged, head 93d44a0760448f7f07d2795c6239d51db5ab6576. Author AceRothstein71; mergeable=false. The note describes this pinned head and makes no ready-to-merge claim.
- Direct PR timeline returned the existing MoerAI review comment and no prior comment from decebal.
- Public project comment search returned one issue, #7764, and zero PRs for commenter decebal, with complete result sets. Search indexing can lag; both relevant threads were also read directly.
- [Prior #7764 approach](https://github.com/code-yeongyu/oh-my-openagent/issues/7764#issuecomment-5616313504) is to DevNewbie1826 on a different, now-closed identity-binding issue. Current thread contains the maintainer status and that outbound comment, with no recipient reply or pass observed. The new note discloses this prior project contact; silence is not consent.
- Connected Gmail all-mail search returned one unrelated newsletter; sent search returned zero matching project/handle messages. No pagination remained. This covers that mailbox, not private chats, other accounts or another CRM.
- AllSource repository is public, but the provider report returned 404 on remote main. Local evidence links must not be presented as already public. A prepared ZIP supplies reviewable evidence without inventing a hosted URL.

Structured check snapshot: [live-checks.json](./artifacts/2026-10-21-omo-send-ready/live-checks.json). The snapshot contains no mailbox addresses or message contents.

## Exact comment

@AceRothstein71 I tested `93d44a0` through the full OMO plugin and captured 73 accepted OpenRouter requests.

With skill reload blocked after the initial model-selected load, two matched pairs ran on → off and off → on:

| Skill reinjection | Compaction 1 | Compaction 2, inside cooldown | Compaction 3, beyond cooldown |
| --- | --- | --- | --- |
| On, both runs | Pass | Fail | Pass |
| Off, both runs | Fail | Fail | Fail |

All baselines passed. Skill-off removed only the dispatched skill block; agent/model/tools recovery and summary injection stayed intact. Readiness barriers completed before all 16 tasks.

Full instructions and the current query appeared in all four fresh skill-on first requests. Both cooldown first requests and all six skill-off first requests lacked them. Summaries retained skill names and used queries, dropping unused map entries. This adds provider-input traces to the existing cooldown review.

These are manual compactions of a synthetic skill with source-scoped tools and one `minimax/minimax-m3.1-flash:free` route on OpenCode 1.18.22. Provider-internal handling remains unobserved.

I build AllSource and previously commented on #7764. A sanitised reproducer, provider bodies and hashes are ready to share. Would they help your regression tests?

## Evidence for attachment

[omo-7183-evidence.zip](./artifacts/2026-10-21-omo-send-ready/omo-7183-evidence.zip) is the prepared file for a later approved upload. [Package README](./artifacts/2026-10-21-omo-send-ready/omo-7183-evidence/README.md), [findings](./artifacts/2026-10-21-omo-send-ready/omo-7183-evidence/FINDINGS.md) and [plain comment](./artifacts/2026-10-21-omo-send-ready/comment.md) are reviewable locally. The ZIP includes all 73 exact provider bodies, native tool/compaction/barrier proof, synthetic fixtures, original observers, a portable runner and offline verifier. Authentication headers, credentials, mail/CRM audit data, server logs and unrelated workspace files are excluded.

Portable runner uses an environment key and caller-provided pinned dependency seed. Static/hash checks preserve the readiness function and observers. The portability edits have not had a live model rerun; retained evidence remains from the completed, recorded evaluation. [Archive verification](./artifacts/2026-10-21-omo-send-ready/verification.json) passed before and after ZIP extraction: 73 provider calls, 73 native model steps, 16 barriers, skill-on 4/6 and off 0/6. ZIP contains 170 files (990957 bytes); its SHA-256 is recorded in the verification file.

## What changed

The note now opens on the pinned-head result, uses a small per-compaction table, and removes repeated experiment setup. It preserves synthetic/model limits and AllSource/prior-contact disclosure. One request remains: whether the traces help the owner's tests. The no-ai-slop editing checks passed; no new product claim, urgency or buyer qualification was added.

## Dispatch state

Prepared only. No GitHub comment, attachment upload, external email draft, email, follow-up schedule, commit or push created. Recipient remains uncontacted for this PR.

When a later user instruction authorises sending: recheck head/replies/pass, use this exact comment, attach the reviewed ZIP through the existing PR, and retain the actual public comment/attachment URLs in the outreach ledger. If attachment upload is unavailable, the self-contained note offers the prepared evidence without claiming it is attached. Do not substitute a gist, hosted page or unverified public docs link.
