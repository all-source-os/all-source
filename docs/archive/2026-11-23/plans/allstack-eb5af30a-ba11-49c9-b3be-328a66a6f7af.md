# Native delivery CI prerequisites

Status: accepted. The user authorized `fix/native-ci-prerequisites` from merged main and one successor PR. AllSource PR #370 merged as `fd2b2c0a1d24a9766ce6dcf1cfaa0fc13de161eb` before these CI corrections were published.

## Destination and retained behavior

Merged main contains the qualified Prime C0-value encoder and unchanged frozen 375-case reference corpus. Their bytes match the authenticated Codex/Claude qualification. The successor carries only the missing CI prerequisites and indexed evidence. It must preserve unrelated main changes.

## Prepared corrections

- Criterion dependency cooking already completes, but the real source phase reused an empty recipe library. Touch the real PR/base source inputs and require a fresh Core compiler artifact from the actual source path. Apply the same assertion to the existing instruction-benchmark pipeline.
- Combine the two existing Mint warning classes into one equivalent Dialyzer expression across OTP releases. Keep unused-filter enforcement active.
- Exclude only unauthenticated background GET `/health` requests from the query-usage test fixture. A regression test must still capture credential-bearing health requests.
- Bound Core CI compilation and execution separately. Remove development/test debug symbols, use the test profile for Clippy, require actual enterprise/community test executables, and retain both editions and existing integration features. Separate release dependencies from Core code generation. Preserve strict lint, release and documentation checks.
- Compile the RocksDB native dependency closure before the rest of the enterprise test graph. Generate its temporary manifest from the actual locked test unit graph, retain exact dependency versions and features, and reject different compiler fingerprints. Use one target directory and profile across both phases. Preserve completed C++ compilation through ccache, including failed runs.
- Measure instruction counts from the actual PR base before measuring the candidate. Disable benchmark symbol stripping, use a fresh per-run baseline directory, and compare against that measured base. A standard-library-only Rust gate must reject missing, zero or unpaired comparison counters and enforce the exact 3% limit without rounding.
- Give excluded standalone packages in the nested baseline checkout their own workspace boundary, so Cargo cannot attach them to the outer PR workspace. Leave the Core baseline in its existing workspace.

## Observed verification

The unchanged Prime runtime passed 165 tests with five existing environment-dependent ignores. Independent authenticated Claude qualified the final immutable application artifact and original retained store before merge. Local OTP 28 checks pass eleven scoped query-service tests and the complete suite: six doctests, 1,162 tests, zero failures, one skipped and 168 excluded. Dialyzer reports nine skipped warnings and zero unnecessary skips. Both workflow files pass actionlint.

Independent Claude source review inspected six actual inputs and accepted the corrected Criterion freshness and query-service changes. Its instruction-benchmark freshness finding is addressed by the matching artifact assertion. Core CI phase changes require Linux execution.

The first successor Linux run passed the query-service checks but exhausted the bounded enterprise compilation phase inside `librocksdb-sys`; no enterprise tests ran. A local metadata-only prototype matches all 64 compiler units in the native dependency closure. This validates the phase fingerprint, not its Linux execution time.

The prior successful instruction job emitted five `Instructions: 0|N/A` measurements and reported a 0.00% regression. The revised gate rejects that captured output. Eight Rust tests pass, including a real CLI rejection, exact threshold boundaries, ANSI output, missing baselines and full-width counters. Strict Clippy passes. Real positive Linux base/candidate measurements remain required.

Linux accepted the 64-unit native fingerprint check, but the cold RocksDB C++ phase reached its deadline. The failed run saved its partial ccache; subsequent runs must report actual cache statistics and complete within the existing cap. A separate Chronis baseline failure reproduced through local Cargo metadata: its nested excluded package selected the outer workspace. The explicit boundary repairs both Chronis and Prime metadata without changing application source. A fresh authenticated Claude review of supplied native-phase and parser source found no actionable blockers; this was source review, not Linux execution.

## Completion

Start from current merged main after authorization. Apply only the unpublished correction delta, adopt the indexed plan through native Allstack, prepare one commit and verify its frozen gates. Read back actual CI results and correct any remaining prerequisite on that same PR. Keep AllSource history, Chronis ownership and source-linked Prime memory underneath the native interface. Preserve raw streams and state before cleaning owned workspaces and caches. Do not merge or deploy.

Confidence: sand for cold Core CI compilation. A passing bounded Linux run establishes the remaining build-phase assumption.
