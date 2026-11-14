# Core memory bound: S1 autoresearch run ledger

Run `run-1794635113319-54657-506698b1`, base commit `506698b1` (the US-011 baseline), objective
`peak_live_bytes` (minimize, whole MiB). Plan story US-012, bead t-664872.

Result: no candidate kept. Two candidates evaluated, both discarded on a tied objective. The run was
stopped by the operator after candidate 2, with 6 of the 8 manifest candidates unspent. `verify`
refuses (no kept candidate), so no `verify: matched` record exists.

## Frozen inputs

| Item | Value |
|---|---|
| Frozen identity | `45c2f6f932cd8b214d9c61323a9d6155f5375bf4d7880e6d7ea95a0139ff2590` |
| Environment fingerprint | `4512daf96cd23f9c90894b2c6318421414f4eff5f8869cd0fc86bbf722df7a75` |
| `heap-replay-eval` sha256 | `f94e147fd9498a938e949142a18e156a155badb00d64fbb44c718ca2b48edaac` |
| `core-tests-eval` sha256 | `28356096a479ebda7b643c33bfcfb55290a1f35465eb86c9f123f68dffba07b9` |
| Policy digest (`--policy-sha256`) | `8fd12ab84a4c6abed1856730619e4f062e591f8dab01226ef48a661462c65686` |
| Tests digest (`--tests-sha256`) | `4221d197a3f29fb0b836d25909d1286727f12c3765c3417305bd84e5e7b78bc1` |
| Baseline `peak_live_bytes` | 1301282816 (1241 MiB), same value in both baseline runs |
| Baseline gates | 24 of 24 hard gates pass (`heap_replay` 18, `core_tests` 6) |

The run's base is `506698b1`. The story branch moved to `c7fee74a` (docs only, see
`git diff --stat 506698b1 c7fee74a`). The run was driven from a detached checkout at `506698b1`,
and the branch was restored after the run.

## Candidate ledger

Every candidate changed only `apps/core/src`. Each hypothesis is one change. Each candidate kept
every hard gate: `replay_expected_status`, `ledger_two_sided`, `query_digest_equal`,
`prod_hasher_digest_equal`, `codec_digest_equal`, `geo_digest_equal`, `evictions_match`,
`eviction_policy_unchanged`, `tests_pass` (1772 passed, 0 failed, 2 ignored), `expected_tests_ran`,
`tooling_tests_pass` (90 passed).

| Index | Commit (autoresearch branch) | Hypothesis | Changed lines | `peak_live_bytes` | Decision | Decision reason |
|---|---|---|---|---|---|---|
| 1 | `0aea2736` | Streaming per-file tenant batches into the store, dropping each file's buffer as it is consumed, removes the full tenant accumulation Vec from the cold-load peak. | 21 | 1301282816 (tie) | discard | `tie_breaker` / `changed_lines` |
| 2 | `63181d34` | Sizing each parquet file's event Vec from its row-group counts removes doubling slack and realloc transient in cold-load decode. | 9 | 1301282816 (tie) | discard | `tie_breaker` / `changed_lines` |

Candidate 1 changed `load_events_for_tenant_with_integrity` to a new `load_tenant_file_batches_with_integrity`
in `storage.rs`, and `ensure_tenant_loaded` in `store.rs` iterates `batches.into_iter().flatten()`. The walk
still completes before any append, so error behaviour is unchanged. Its runtime was 187091 ms for the
full evaluation.

Candidate 2 pre-sized the per-file `events` Vec in `load_events_from_file_with_budget` from the sum of
row-group `num_rows`, clamped to 2^20.

The decision reason `tie_breaker` on a discard matches the autoresearch rule: an equal objective
against the untouched baseline (complexity all zeros) is discarded. No candidate reached the objective
threshold of one whole MiB (1048576 B).

## Why no candidate moved the objective

- The objective floors to whole MiB. Both candidates tied to the byte, so a saving under 1 MiB at the
  peak point would not show. The raw peak is not in the autoresearch record, so the distance from the
  floor is unknown.
- An earlier `main` replay artifact in the scratchpad (`runs/main-deterministic-1/stdout.txt`, built
  before this run) reports `peak_live_bytes` 1301808660 against `steady_live_bytes` 1197562436. The
  peak sits about 104 MB above steady state, which points to a cold-load transient rather than resident
  data. The S0 report puts the single largest tenant load transient at 80 MB.
- The transient is dominated by Vec growth: the global resident `events` Vec and the per-load
  accumulation Vec. Candidate 1 removed the accumulation Vec during the splice loop, but the peak point
  falls where both buffers coexist, so the saving did not register. Candidate 2 removed slack in the
  per-file Vec, which did not register either.
- The remaining lever found is the size of `Event` (about 208 B by field sum). Boxing `Event.metadata`
  would save 24 B per resident event, about 5.5 MB at 229k events. That change touches construction sites
  across `apps/core/src`, `tests` and `examples`, and changes a public type used by the embedded NIF.
  It is outside a one-hypothesis candidate, so it was not run.

## Not done in this run

- Candidates 3 to 8 (budget unspent). This session stopped the run after candidate 2
  (`stop_reason: operator_cancelled`) because the remaining levers were not one-hypothesis changes:
  the `Event` size reduction above, and a chunked `events` store, which is a design change. The
  owner decides whether to spend the rest of the budget on either.
- Acceptance 4 records: ceilings and phase timings, five cold-load runs each of the overlay and S1
  with the band rule, the overlay digest comparison, the overlay spaced eviction scenario, and the
  production-hasher peak. Produced in `docs/reports/2026-11-14-core-memory-s1-report.md`. The deciding
  cold-load attempt passes both rules: bands 4.2% (overlay) and 3.6% (S1), medians 1941 ms and 1631 ms.
- `verify`: refused with `no finalized kept candidate is available for verification` (exit 4).

## Operational notes

- The `run` call with `--hypothesis` exceeded a 290 s outer wrapper on both candidates. The first
  evaluator (`heap_replay`) completed and was journaled; the second (`core_tests`) was killed with the
  wrapper. `resume --run-id` completed the second evaluator. A `cargo test --list` process left by the
  killed wrapper was terminated by hand, since it was the run's own child.
- The caller checkout had to sit at the base commit for every run command, so the worktree was
  detached at `506698b1` for the run and returned to `feat/core-memory-bound` after the report.
- Candidate worktrees and the `autoresearch/run-1794635113319-54657-506698b1` branch remain under
  `.autoresearch/`, which is git-ignored.

## Artifacts

- Run report JSON: `docs/reports/2026-11-13-core-memory-s1-run-report.json`.
- Baseline record: `docs/reports/2026-11-14-core-memory-bound-us011-baseline.md`.
