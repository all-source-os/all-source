# Core memory bound: US-011 autoresearch baseline

Base commit: `506698b1`, which pins the evaluator rebuilt at `7d2aec69`. `git diff --stat 7d2aec69 506698b1`
changes only `autoresearch.toml` (one line, the `heap-replay-eval` self-sha256 literal), so the code under test is
`7d2aec69`.

The objective fix is `7d2aec69`: rate limiter and AuthManager maps go through `fast_dashmap`, and the evaluator
reports the objective floored to whole MiB (`replay::objective_peak`, `PEAK_QUANTUM_BYTES`).

Frozen identity: `45c2f6f932cd8b214d9c61323a9d6155f5375bf4d7880e6d7ea95a0139ff2590`.
Environment fingerprint: `4512daf96cd23f9c90894b2c6318421414f4eff5f8869cd0fc86bbf722df7a75`.

Evaluator binaries (outside the repo, in the scratchpad `eval-v3/oracle-bin/`), checked with `shasum -a 256`
and equal to the `--self-sha256` literals in `autoresearch.toml`:

- `heap-replay-eval` `f94e147fd9498a938e949142a18e156a155badb00d64fbb44c718ca2b48edaac`
- `core-tests-eval` `28356096a479ebda7b643c33bfcfb55290a1f35465eb86c9f123f68dffba07b9`

Pinned inputs: `expected-eviction-policy.txt` `8fd12ab8…65686` (`--policy-sha256`), `tests-S1.txt` `4221d197…78bc1`
(`--tests-sha256`). Both match the committed files.

## Runs

Two `autoresearch --json --repository <worktree> baseline` invocations at `506698b1`, each under `timeout -k 15 290`.
Each returned `evidence_status: captured`, `next_action: run`, and all 24 hard gates passed (`heap_replay` 18 of 18,
`core_tests` 6 of 6).

| Run | Journal | `peak_live_bytes` (objective) | `rows_decoded` | `requests` | `core_passed` / `core_failed` / `core_ignored` | `tooling_passed` |
|---|---|---|---|---|---|---|
| `run-1794634814564-51557-506698b1` | `.autoresearch/runs/` (ignored) | 1301282816 | 230302 | 576 | 1772 / 0 / 2 | 90 |
| `run-1794635113319-54657-506698b1` | `.autoresearch/runs/` (ignored) | 1301282816 | 230302 | 576 | 1772 / 0 / 2 | 90 |

Both runs' JSON, one line each, are in `docs/reports/2026-11-14-core-memory-bound-us011-baseline.jsonl`.

`peak_live_bytes` = 1301282816 = 1241 MiB in both runs. `rows_decoded_ok` passes against
`tooling/core-heap-replay/expected/ceilings-S1.json` (`rows_decoded_max` 230302).
`expected_tests_ran` passes: `passed + failed + ignored` equals the 1774 names in `tests-S1.txt`.

Doctor before the runs: `autoresearch --json --repository <worktree> doctor` `ready: true`, 7 of 7 checks.

## Objective unit

The objective is whole MiB, not bytes. A candidate that saves less than 1 MiB (1048576 B) ties the baseline.
The raw peak is still in the evaluator envelope as `raw_peak_live_bytes`. It moves by up to about 1.4 KB between
runs of one binary (commit `7d2aec69` message), so it is not used as the objective. The two runs above did not
record the raw value; the autoresearch baseline JSON does not carry it.

## Verification at `506698b1`

| Command | Result |
|---|---|
| `cargo clippy -p allsource-core --locked --all-targets -- -D warnings` | Finished, no errors |
| `cargo clippy -p allsource-core --locked --all-targets --features deterministic-hash -- -D warnings` | Finished, no errors |
| `cargo test -p allsource-core --locked --lib` | 1772 passed, 0 failed, 2 ignored |
| `cargo fmt --check` (apps/core) | clean |
| `cargo sort --check apps/core` | clean |
| `cargo test --locked` in `tooling/core-heap-replay`, with `HEAP_REPLAY_BIN`, `HEAP_FIXTURE_DIR`, `ROLLBACK_BIN` set | every test binary `ok`, 0 failed; includes `replay_two_runs_one_volume`, `rollback_volume`, `worktree_stays_clean` (each 1 passed) |

`HEAP_REPLAY_BIN` is `eval-v3/bin/heap_replay-deterministic` from the second baseline run. `HEAP_FIXTURE_DIR` is the
`fixture-copy` directory the manifest names. `ROLLBACK_BIN` is an `allsource-core` dev-profile build of
`f4b53930` (`cargo build -p allsource-core --bin allsource-core --features community,analytics --locked --offline`,
sha256 `70696c321a5a25f2bd3f6af14dd0f5af044527e19127b9556a50f573f6e07e32`), built outside the repo. It is a different
build from the one US-010 recorded (sha256 `6a60b856…`), so the binary hash differs even though the source is the same.

## Decision: evaluator output reading

`docs/plans/2026-11-12-core-memory-bound-design.md` says the evaluator writes "JSONL v1, with one envelope on stdout".
Orchestrator decision (US-011 fix round 1): read this as the autoresearch JSONL protocol v1, meaning one
newline-terminated JSON response line on stdout with `protocol_version: 1` and every declared gate and metric. The
runner rejects any other stdout. The internal `jsonl.v1` `Envelope` in `src/envelope.rs` stays internal and is
converted by `src/protocol.rs::response`.

## Limits

- `peak_live_bytes` is now a whole-MiB objective, so it repeated across both runs. The raw peak is not repeated
  byte-for-byte and is not declared. `verify` on a kept candidate compares the objective exactly, so a rerun may
  still report `drifted` if a candidate lands near a MiB boundary. The S1 run (US-012) decides how to handle that.
- `steady_live_bytes`, `ledger_bytes` and `production_peak_live_bytes` move between runs and are not declared.
  `ledger_two_sided` gates on them inside the evaluator.
- The evaluator binaries are checked by sha256 against the pinned literals. They were not rebuilt in this record;
  the rebuild from `7d2aec69` is recorded in commit `506698b1`.
- After each baseline run, autoresearch leaves its run worktree registered and locked under `.autoresearch/worktrees/`
  and does not remove `.autoresearch/run.lock`. The lock names the finished run's pid, which is no longer alive. The
  run worktrees were removed with `git worktree remove -f -f` after checking that each was clean. Seven empty,
  unregistered directories from earlier stopped attempts were removed with `rmdir`.
- `rollback_volume` is not a hard gate in `autoresearch.toml`. It runs in the harness suite with `ROLLBACK_BIN` set.
