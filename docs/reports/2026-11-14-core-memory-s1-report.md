# Core memory bound: S1 report, Acceptance 4 records

Plan story US-012 (bead t-664872). Design: `docs/plans/2026-11-12-core-memory-bound-design.md`, Acceptance 4
(lines 700-707). Run ledger: `docs/reports/2026-11-13-core-memory-s1-run-ledger.md`.

Host: `aarch64-apple-darwin`, dev host. Load average at 09:44, read after the runs: 8.2 (1 min), 12.0 (5 min),
17.2 (15 min).

## Outcome

- Acceptance 4 is met. Three cold-load attempts were made; all three are recorded below.
  - Attempts 1 and 2 ran on a host under compile load and fail the band rule (design line 703).
  - Attempt 3 interleaves the two builds and starts each run only when no `rustc` process is running.
    Both bands pass (overlay 4.2%, S1 3.6%), and the comparison rule holds: S1 median 1631 ms against the
    overlay's 1941 ms.
- Overlay digests match the committed oracle digests on the replay set. The overlay eviction sequence
  matches `expected-evictions.txt`.
- The overlay peaks about 1.03 GB above S1. This is the geo query's full snapshot clone, which US-009
  removed. The same harness measured a peak of 2331127931 B before that change
  (`docs/reports/2026-11-13-core-memory-bound-us007-eval.jsonl`) and about 1301 MB after it.

## Overlay recipe

The overlay is `f4b53930` plus the changes in
`docs/reports/2026-11-14-core-memory-s1-overlay.patch` (34 KB, `git diff` against `f4b53930`). Applied
to a detached worktree at `f4b53930`:

1. `apps/core/src/infrastructure/web/api_v1.rs`: the oracle file, which contains the `build_v1_router`
   extraction. The oracle's `heap-profile` route is cfg-gated and inert without that feature.
2. `apps/core/examples/heap_replay.rs` and `heap_fixture.rs`: the oracle examples, with two edits.
   - The residency-ledger reads are replaced by zeros (`ledger_bytes`, `rows_decoded`, `ledger_owner_bytes`).
     `f4b53930` has no `infrastructure::memory` module.
   - A `spaced-eviction` scenario is added. Tenant queries run in the same order as `eviction`, with a 10 ms
     sleep after each. The scenario asserts that the tenant touch instants are strictly increasing.
3. `apps/core/Cargo.toml`: `cap = { version = "0.1", features = ["stats"] }` under `[dev-dependencies]`.
4. `Cargo.toml`: the `[profile.heap-replay]` block copied from the oracle.
5. `Cargo.lock`: `f4b53930`'s own lock, unchanged. `cap` 0.1.2 is already in it.

Build: `cargo build -p allsource-core --profile heap-replay --example heap_replay --example heap_fixture
--features community,analytics`. No `deterministic-hash`, no `heap-profile`. The production hasher is the
default `std` hasher of `f4b53930`.

Binary sha256 (overlay `heap_replay`): `f0dcae4ec3cffded03ab56059baaf6ad73bc65f48e622cd4e8c6d916901a1054`.
S1 `heap_replay-production` sha256: `3e51a652252d2ec1d4456c437539b26d5defd432c3a3be42d6c3e1f92377bf00`. It was
built by the baseline evaluator from `506698b1`. `apps/core`, `Cargo.toml` and `Cargo.lock` are unchanged
between `506698b1` and HEAD (`git diff --stat 506698b1 HEAD -- apps/core Cargo.toml Cargo.lock` is empty), so
this is the S1 build at HEAD.

## Overlay digest comparison

Replay set: the fixture at `scratchpad/fixture-copy`, main scenario, one process per run, each run on its own
APFS clone of the fixture.

| Digest | Overlay (`f4b53930`) | S1 (`506698b1` code) | Committed oracle value | Equal |
|---|---|---|---|---|
| `query_digest` | `aff95797e04833d770dd31eb458e175483f80c2435f95815dcae575161c94313` | same | `expected-digest.txt` | yes, both |
| `geo_digest` | `a5f0fa5a9404989eaf09dfeeb56fa87a26d29aea5605fafa6aa4929c4d78794f` | same | `expected-geo-digest.txt` | yes, both |
| `codec_digest` | `d8862621130c2acbdd3100978904d75ffd165a345f266b4214d3269fbb007eb1` | same | `expected-codec-digest.txt` | yes, both |

Requests 576, status mismatches 0, epoch mismatches 0, model event leaks 0, on both builds.

Eviction scenario, spaced on the overlay, unspaced on S1. Budget is half the fixture estimate.

| Build | Evicted sequence | `query_digest` | `geo_digest` | `codec_digest` |
|---|---|---|---|---|
| Overlay, `spaced-eviction` | `["tenant-01"]` | `6b9e6bf8524bbe5712c56db4c1cc1a34243072af0d249228d45d454543682ad6` | `46de0721ad33c23fe9da70ac987a37dff5778c175f6056a82844d836b5cf632f` | `d8862621...` |
| S1, `eviction` | `["tenant-01"]` | same | same | same |

The sequence equals `expected-evictions.txt` (`tenant-01`). Requests 288 on both. The spaced run's tenant touch
instants were strictly increasing, so the in-process assertion held.

## Cold-load runs (five per set)

`cold_load_largest_ms` is the first tenant's cold load in pass A, reported by each `heap_replay` process.
Every attempt used the same two binaries (sha256 above), each run on its own APFS clone of the fixture, with
`ALLSOURCE_MEMORY_RESERVE_BYTES=1073741824`.

### Attempt 3 (deciding): interleaved, no concurrent compile

Run order: overlay 1, S1 1, overlay 2, S1 2, and so on. Before each run, the script waited until
`pgrep -x rustc` matched nothing. `load1` is the one-minute load average read just before each run. It was
still falling from an earlier compile by another session.

| Run | Overlay cold load (ms) | Overlay `load1` | S1 cold load (ms) | S1 `load1` |
|---|---|---|---|---|
| 1 | 2006 | 16.31 | 1680 | 13.75 |
| 2 | 1941 | 12.73 | 1631 | 11.79 |
| 3 | 1964 | 11.41 | 1622 | 10.65 |
| 4 | 1926 | 9.88 | 1629 | 9.25 |
| 5 | 1924 | 8.83 | 1645 | 7.86 |
| median | 1941 | | 1631 | |
| band `(max - min) / median` | 4.2% | | 3.6% | |

- Band rule: both bands are at most 10%. Pass.
- Comparison rule: 1631 ≤ 1941 × (1 + 0.042) = 2022. Pass. The S1 median cold load is 16% below the
  overlay's.
- Peaks in this attempt: overlay 2330016100 to 2331090279 B, S1 1301035069 to 1301564821 B.

### Attempt 2: sequential, compile started mid-set

The overlay set ran first, then the S1 set. Another session's `cargo` build started during the S1 set (four
`rustc` processes seen right after it), and the load average went from 4.2 to 14.3.

| Run | Overlay cold load (ms) | S1 cold load (ms) |
|---|---|---|
| 1 | 2040 | 1707 |
| 2 | 2021 | 1663 |
| 3 | 2020 | 4094 |
| 4 | 1983 | 2757 |
| 5 | 2066 | 2310 |
| median | 2021 | 2310 |
| band | 4.1% | 105.2% |

The S1 set fails the band rule.

### Attempt 1: sequential, host at load 8 to 17

| Run | Overlay cold load (ms) | S1 cold load (ms) |
|---|---|---|
| 1 | 2083 | 1748 |
| 2 | 2160 | 2092 |
| 3 | 2132 | 1618 |
| 4 | 2225 | 1725 |
| 5 | 2703 | 1686 |
| median | 2160 | 1725 |
| band `(max - min) / median` | 28.7% | 27.5% |

Both sets fail the band rule (design line 703). The comparison rule would hold: 1725 ≤ 2160 × 1.287 = 2780.
The overlay's single smoke run at 2006 ms, made before the five, is not in the table.

No single run was dropped from any attempt. Each later attempt is a complete new set of five per build. Attempt
3 controls the measurement conditions that broke attempts 1 and 2: it waits out concurrent compiles and
interleaves the builds, so load drift affects both sets alike.

## Production-hasher peak

`peak_live_bytes` for the S1 production build, main scenario, five cold-load runs: 1300943603 to 1301269827
bytes (mean about 1301.1 MB). The evaluator objective (floored to whole MiB) is 1301282816 (1241 MiB), the
same as the US-011 baseline.

| Build | Scenario | `peak_live_bytes` | `steady_live_bytes` |
|---|---|---|---|
| S1 production | main (five runs) | 1300943603 to 1301269827 | about 1196.7 to 1197.0 MB |
| S1 production | eviction | 1186429107 | 154386903 (after eviction) |
| Overlay production | main (five runs) | 2329859982 to 2330332342 | about 1215.4 to 1215.9 MB |
| Overlay production | spaced-eviction | 2068617044 | 173502732 (after eviction) |

## Ceilings and phase timings

Measured in this session. Wall clock.

| Item | Measured | Limit | Source |
|---|---|---|---|
| `heap-replay-eval` (`heap_replay` evaluator, two builds, two replays, production replay, eviction, `--verify-fixture`) | 44.9 s | 270 s internal, 290 s manifest | `time` on the run |
| `core-tests-eval` (test build, full test list, tooling tests) | 256.3 s | 270 s internal, 290 s manifest | `time` on the run |
| One replay process, overlay, main | 6.4 s (cold load 2.0 to 2.7 s) | n/a | `time` on the run |
| Overlay cold build (`heap-replay` profile, cold target dir) | more than 290 s, finished on the second call | 290 s per command | not timed |

Phases of one warm `heap-replay-eval` run at HEAD `b97e2024`, sampled from the process table (19.5 s total):

| Phase | Wall (ms) |
|---|---|
| Source mirror (`rsync`) | 1353 |
| `cargo build` deterministic examples (warm) | 842 |
| `cargo build` production example (warm) | 512 |
| `heap_fixture --verify` | 1686 |
| Replay `main`, deterministic, run 1 | 4090 |
| Replay `main`, deterministic, run 2 | 3895 |
| Replay `main`, production hasher | 4236 |
| Replay `eviction`, deterministic | 2716 |

The 44.9 s run above included incremental rebuilds. For `core-tests-eval`, only the two `cargo test --list`
phases were sampled: 39.9 s and 45.7 s. The rest of its 256.3 s is the test build and the test run, which were
not timed separately.

`core-tests-eval` at 256.3 s leaves 13.7 s under the 270 s deadline. The frozen `ceilings-S1.json` records
`heap_replay` 22 s and `core_tests` 39 s. This session did not reproduce those figures. The file is frozen
and is unchanged. The `heap_replay` gate ran 44.9 s, against 22 s recorded.

S0 fixture generation and the container build steps are not re-timed here. The earlier autoresearch candidate runs
recorded 187091 ms and 254628 ms for the two candidates (`docs/reports/2026-11-13-core-memory-s1-run-report.json`,
`runtime_ms`).

## Test gates on the branch

HEAD source equals `506698b1` source (`apps/core`, `Cargo.toml`, `Cargo.lock`). The gates below were run on
that source.

- `core-tests-eval`: `tests_pass` 1772 passed, 0 failed, 2 ignored; `expected_tests_ran`; `tests_unchanged`;
  `tests_digest_pinned`; `tooling_tests_pass` 90 passed. All 6 gates pass.
- `heap-replay-eval`: all 18 hard gates pass.

## Limits

- The overlay peaks about 1.03 GB above S1, while steady bytes differ by under 20 MB. This is a transient: the
  geo query's snapshot clone, which US-009 removed. The design does not gate the overlay peak, and steady
  resident memory is unchanged by it.
- The overlay's residency-ledger fields are zero by construction. `ledger_two_sided` and `rows_decoded_ok`
  are not evaluated on the overlay. Design Acceptance 4 does not ask for them.
- Attempt 3 still ran at load 8 to 16, falling. It had no concurrent compile and the builds were
  interleaved, but this is not an idle host.
- Timings are wall clock on a loaded host.
- Overlay `api_v1.rs` is the full oracle file, not a minimal extraction. Its `heap-profile` route is cfg-gated
  and not compiled. The overlay build prints `unexpected cfg` warnings for `heap-profile` and
  `deterministic-hash`.
