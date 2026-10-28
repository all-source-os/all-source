# Native delivery CI prerequisites

Status: accepted. The user authorized `fix/native-ci-prerequisites` from merged main and one successor PR. AllSource PR #370 merged as `fd2b2c0a1d24a9766ce6dcf1cfaa0fc13de161eb` before these CI corrections were published.

## Destination and retained behavior

Merged main contains the qualified Prime C0-value encoder and unchanged frozen 375-case reference corpus. Their bytes match the authenticated Codex/Claude qualification. The successor carries only the missing CI prerequisites and indexed evidence. It must preserve unrelated main changes.

## Prepared corrections

- Criterion CI run 40678542842 hit exit 124 in the 285-second dependency phase before any benchmark ran, because the dependency graph and the real Core library shared that phase. The workflow cooks the unchanged registry dependencies against an empty local Core library and harness, asserts identical registry package, target, mode, platform, feature and profile identities (506 units on nightly-2026-07-16), then compiles the real source and benchmark in their own bounded phases. Benchmark cases, optimization settings, sampling and thresholds are unchanged.
- Criterion dependency cooking already completes, but the real source phase reused an empty recipe library. Touch the real PR/base source inputs and require a fresh Core compiler artifact from the actual source path. Apply the same assertion to the existing instruction-benchmark pipeline.
- Combine the two existing Mint warning classes into one equivalent Dialyzer expression across OTP releases. Keep unused-filter enforcement active.
- Exclude only unauthenticated background GET `/health` requests from the query-usage test fixture. A regression test must still capture credential-bearing health requests.
- Bound Core CI compilation and execution separately. Remove development/test debug symbols, use the test profile for Clippy, require actual enterprise/community test executables, and retain both editions and existing integration features. Separate release dependencies from Core code generation. Preserve strict lint, release and documentation checks.
- Compile the RocksDB native dependency closure before the rest of the enterprise test graph. Generate its temporary manifest from the actual locked test unit graph, retain exact dependency versions and features, and reject different compiler fingerprints. Use one target directory and profile across both phases. Preserve completed C++ compilation through ccache, including failed runs.
- Measure instruction counts from the actual PR base before measuring the candidate. Disable benchmark symbol stripping, use a fresh per-run baseline directory, and compare against that measured base. A standard-library-only Rust gate must reject missing, zero or unpaired comparison counters and enforce the exact 3% limit without rounding.
- Pair counters by benchmark case name against the measured base output. A case the base never measured is reported as added and does not fail the gate; a case the base measured must have a nonzero paired count, and at least one case must be paired. Adding or renaming an instruction benchmark therefore passes, while a lost baseline still fails.
- Compile the enterprise test graph's registry dependencies in their own phase before the Core test crate: a warm pass that may pause at its deadline, then a strict pass. On a cold Rust cache, run 40802867296 spent its whole 285-second enterprise phase on 952 dependency artifacts and was still compiling `datafusion` when it was killed, before Core started. The passing run on a warm cache took 3m53s for the same phase.
- Prime's `bench_stats_over_populated_graph` populated 1,000 nodes and dropped its runtime inside the measured frame. On unchanged source it measured 4,155,078, 3,752,814, 3,725,228 and 4,147,502 instructions across runs, and the last pairing failed the 3% gate at +11.34%. Population now runs in an iai `setup` function and the runtime is returned, so only `stats()` is counted. `bench_add_node_batch` stays as is: it counts 82,070 instructions for 100 nodes and 82,527 for 1,000, because ingest runs on `spawn_blocking` threads outside the measured frame. It is stable, but it does not measure ingest.
- Measure the base with the `iai-callgrind-runner` version its own lockfile pins. The runner refuses a mismatched `iai-callgrind` crate, so a PR that bumps the crate and the runner pin together would otherwise fail on the base side.
- Give excluded standalone packages in the nested baseline checkout their own workspace boundary, so Cargo cannot attach them to the outer PR workspace. Leave the Core baseline in its existing workspace.

## Observed verification

The unchanged Prime runtime passed 165 tests with five existing environment-dependent ignores. Independent authenticated Claude qualified the final immutable application artifact and original retained store before merge. Local OTP 28 checks pass eleven scoped query-service tests and the complete suite: six doctests, 1,162 tests, zero failures, one skipped and 168 excluded. Dialyzer reports nine skipped warnings and zero unnecessary skips. Both workflow files pass actionlint.

Independent Claude source review inspected six actual inputs and accepted the corrected Criterion freshness and query-service changes. Its instruction-benchmark freshness finding is addressed by the matching artifact assertion. Core CI phase changes require Linux execution.

Before this change the all-feature Core gate spent 9m22 in Clippy and 11m33 compiling unit tests before any test executed, and its log records two librocksdb-sys compilations.

The first successor Linux run passed the query-service checks but exhausted the bounded enterprise compilation phase inside `librocksdb-sys`; no enterprise tests ran. A local metadata-only prototype matches all 64 compiler units in the native dependency closure. This validates the phase fingerprint, not its Linux execution time.

The prior successful instruction job emitted five `Instructions: 0|N/A` measurements and reported a 0.00% regression. The revised gate rejects that captured output. Eight Rust tests pass, including a real CLI rejection, exact threshold boundaries, ANSI output, missing baselines and full-width counters. Strict Clippy passes. Real positive Linux base/candidate measurements remain required.

Linux accepted the 64-unit native fingerprint check, but the cold RocksDB C++ phase reached its deadline. The failed run saved its partial ccache; subsequent runs must report actual cache statistics and complete within the existing cap. A separate Chronis baseline failure reproduced through local Cargo metadata: its nested excluded package selected the outer workspace. The explicit boundary repairs both Chronis and Prime metadata without changing application source. A fresh authenticated Claude review of supplied native-phase and parser source found no actionable blockers; this was source review, not Linux execution.

## Completion

Start from current merged main after authorization. Apply only the unpublished correction delta, adopt the indexed plan through native Allstack, prepare one commit and verify its frozen gates. Read back actual CI results and correct any remaining prerequisite on that same PR. Keep AllSource history, Chronis ownership and source-linked Prime memory underneath the native interface. Preserve raw streams and state before cleaning owned workspaces and caches. Do not merge or deploy.

Confidence: sand for cold Core CI compilation. A passing bounded Linux run establishes the remaining build-phase assumption.
