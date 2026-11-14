---
design: docs/plans/2026-11-12-core-memory-bound-design.md
design-sha256: 129dcea7d5baf4d8eef2a42fe610cfd9bf0def28ee0d50c98faaf8d6b40c3f5e
verdict: findings
approval: by hand (owner's decision, 2026-11-12); `cos ritual approve` refuses a review that is not clean
---

# Core memory bound design: review record

The design had twenty fresh-context review rounds. No round came back clean.

The owner chose to approve by hand at v21. Since the rescope, each round's blocking findings have
been code facts in the owner table or the harness, not flaws in decisions. Build's named tests are
built to catch that class of error.

## Rounds

| Rounds | Scope | Outcome |
|---|---|---|
| [r1](2026-11-12-core-memory-bound-design-review-r1.md)–[r3](2026-11-12-core-memory-bound-design-review-r3.md) | full S0–S4 | Found 5–6 blocking each round. The owner narrowed approval to S0, S1 and the harness. |
| [r4](2026-11-12-core-memory-bound-design-review-r4.md)–[r5](2026-11-12-core-memory-bound-design-review-r5.md) | S0, S1, harness | Found 7, then 11 blocking. The owner chose a contract-level design. |
| [r6](2026-11-12-core-memory-bound-design-review-r6.md)–[r10](2026-11-12-core-memory-bound-design-review-r10.md) | contract level | Found 7, 5, 4, 5, 6 blocking. The owner chose delta reviews. |
| [r11](2026-11-12-core-memory-bound-design-review-r11-delta.md)–[r16](2026-11-12-core-memory-bound-design-review-r16-delta.md) | deltas | Kept finding blockers in subsystems that assume eviction never happens. The owner split routine eviction out (v17). |
| [r17](2026-11-12-core-memory-bound-design-review-r17.md) | full, rescoped | Found 3 blocking. |
| [r18](2026-11-12-core-memory-bound-design-review-r18-delta.md)–[r20](2026-11-12-core-memory-bound-design-review-r20-delta.md) | deltas | Found 2, 1, 1 blocking. Each blocker was one code fact. |

## State at approval

- **v21 fixes every r20 finding, but no reviewer has read v21.** The fixes:
  - the pipeline reset release, with `pipeline.state` as an owner;
  - the geo resolver chain;
  - broadcast as an estimated owner;
  - read-model triggers;
  - the `parse_cache_byte_budget` extraction in the oracle list;
  - the `replay_expected_status` and `eviction_policy_diff_allowed` gates;
  - the overlay eviction check in Acceptance 4;
  - the eviction-rise definition;
  - the S1 eviction-path release list;
  - the exact `index_vec_growth` measurement.
- **Residual risk.** Owner rows and harness details may still miss code facts of the kind rounds
  r18 to r20 found. The safeguards:
  - `ledger_owns_heap`, `ledger_two_sided` (±10%), `evictions_match`,
    `eviction_policy_unchanged`, `replay_expected_status` and `gate_negative_controls` fail on
    such a miss during Build;
  - the S1 structural commit waits for the S0 report.
- **Rule for gaps found in Build.** A Build test that exposes a missing owner or release is design
  drift if it changes a decision. If it only adds an owner-table row, it is recorded in the S1
  report.

## v22: S0 drift (2026-11-13)

- **Trigger.** The [S0 report](../reports/2026-11-13-core-heap-attribution.md) attributes 96.7% of
  steady live heap on the fixture and finds two owners absent from the S1 owner table:
  `RateLimiter` maps (528,394 B) and `MetricsRegistry` histograms (524,480 B). The S1 gate
  routes any missing owner back to Spec.
- **Change.** One owner-table row, `fixed: rate_limiter, metrics`, charged once at startup by the
  measured constant (`rate_limiter` also per new bucket key) and released never (`rate_limiter`
  at bucket removal). No decision, gate or constraint changes.
- **Owner decisions recorded with it.**
  - Fixed owners rather than an exemption floor.
  - The fixture's unattributed share is 3.28% against production's 33.70%, a gap over the
    5-point line in the S0 Producer section. The owner chose one production `heap-profile`
    build, emitting attribution numbers only, deployed in a morning window on the owner's go.
- **Review.** No reviewer has read v22; the change is one table row.
- **Later the same day.** The owner dropped the production `heap-profile` build after six Fly
  builds of it failed the Dockerfile's phase deadlines; production stayed on v61. The Dockerfile
  returns to its US-001 form. Production attribution is left to the S1 ledger's unattributed
  gauge after deploy.
