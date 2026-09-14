# Replay-debugging guide autoresearch

Page: /event-replay-debugging

## Fixed rubric

| Signal | Weight |
| --- | ---: |
| Query resolved above fold | 15 |
| Concrete product-specific decision artifact | 20 |
| Visible source proximity and freshness | 15 |
| Self-contained quotable answers | 15 |
| Honest scope and limits | 10 |
| Internal and trial/docs next step | 10 |
| Semantic and responsive structure | 10 |
| No generic AI filler | 5 |

Baseline score: **79/100**. Final score: **98/100**.

## Experiments

| Run | Change | Score | Decision |
| --- | --- | ---: | --- |
| 0 | Timeline, workflow, SDK example, and safety boundary | 79 | Baseline |
| 1 | Replay-analysis contract mapped to exact SDK fields and operator decisions | 92 | Keep |
| 2 | Visible Microsoft, Fowler, and AllSource SDK references; natural-language copy pass | 98 | Keep |
| 3 | Mobile analysis records | 98 | Keep after rejecting 768px table that hid operator-decision column |

Microsoft guidance stresses immutable events, compensating corrections, and event-version handling. Local TypeScript and Rust SDK types supplied replay scope, time range, affected data, readiness, and run-evidence fields. Page now shows those concrete contracts instead of describing “safe replay” abstractly.

Client-route weight after final production build: **628,992 bytes across 10 chunks**. No client component or effect added.

Proof: apps/web/proofshot-artifacts/2026-08-25_20-42-06_verify-allsource-mobile-replay-analysis/.

