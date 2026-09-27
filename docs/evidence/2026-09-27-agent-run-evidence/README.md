# Internal agent-run evidence verification

Date: 27 September 2026. Base: `47ed3dfc4185925abe24c5ad82c7fd7726bdc82c`.
Scope: existing epic `t-e3d99f`, reused by customer review. No deployment, host
disclosure, production credentials, new API/MCP route or model processing.

## Reproduction and changes

An actual Core HTTP test appended three synthetic events using expected versions
0, 1, 2. Acknowledgements were 1, 2, 3, but the read returned `[1,1,1]`; the test
failed before the fix. Core now stamps the assigned version before WAL append
and hydrates cold tenant history before its optimistic-concurrency check.

The Query Service implementation validates the metadata-only run vocabulary,
tenant/entity/run identity, contiguous server versions, causation and lifecycle.
It folds attempts and changes, compares recorded evidence without execution,
pages an exact digest, and reports advisory retry/unknown/stop decisions. A
bounded source port reads the actual Core leader. See the
[contract and limits](../../plans/2026-09-27-agent-run-evidence-contract.md).

## Verified checks

All commands used `gtimeout -k 15 300` and existing repository dependencies.
Core command cwd was the repository root; Mix command cwd was
`apps/query-service`. Integration calls set `ALLSOURCE_CORE_BINARY` to the
repository's freshly built `target/debug/allsource-core` and used private
synthetic WAL directories, loopback listeners, and owned SIGKILL/await cleanup.

| Check | Result |
| --- | --- |
| `cargo test -p allsource-core --features enterprise --test acknowledged_event_versions --test optimistic_concurrency_tests` | 29 passed |
| `cargo test -p allsource-core --features enterprise --lib` | 1,694 passed, 2 ignored |
| `cargo test -p allsource-core --locked --lib --all-features` | 1,990 passed, 5 ignored |
| `cargo clippy -p allsource-core --locked --all-targets --all-features -- -D warnings` | Passed |
| `cargo fmt --all --check` | Passed |
| Full `mix test` before the final comparison-evidence delta | 6 doctests, 1,076 tests, 0 failures, 2 skipped, 116 excluded |
| Final run domain tests | 16 passed, including changed approval evidence and late reversion of an old attempt |
| Final actual-Core ordering/comparison tests after that delta | 3 passed, including hard restart parity |
| Combined run domain/source/Core plus existing remote OAuth/MCP regression before final comparison-evidence delta | 28 tests, 0 failures, 1 optional browser fixture skipped |
| Query Service format, warnings-as-errors compilation, strict Credo | Passed |
| Dialyzer | Passed with the existing seven filtered warnings and one unnecessary skip; filters unchanged |
| Tenant isolation architecture gate | Passed; existing seven documented PubSub exceptions unchanged |

The real Core read/comparison tests use two recorded runs, verify that reads
leave the event count unchanged, reject another tenant and injected private
fields, and compare the exact same result after a hard restart. The ordering
fixture performs two hard restarts and writes before the first read on the final
boot. Separate Rust tests cover subscriber parity, unchanged caller objects,
one winner among simultaneous conditional writers, WAL and cold Parquet.

Actual HTTP fixtures verify fixed tenant/entity query parameters, no retries,
response byte-limit abort, upstream unavailability, partial history refusal and
invalid-input rejection before network access. Unit fixtures cover uncertain
ordering, typed field rejection, abandoned changes, capture gaps, unresolved
actions, evidence-backed reconciliation, the two-failure stop rule, comparison
direction, and stale pagination digests.

One full Query Service run exposed an existing `ProjectionControllerTest`
teardown race: its previous linked Agent could disappear during the next test's
reset. The fixture now uses ExUnit supervision; the subsequent full suite passed.
An initial narrower-feature Clippy command hit the unrelated existing
`HybridSearchEngine.index_event` no-feature async-stub lint. The repository's
actual all-feature Clippy gate passed without changing or suppressing that code.

## Limits and open acceptance

This is an internal evidence reader, not a completed customer workflow. Current
MCP consent remains eligibility/validation only. No source grant, proposal store,
human review UI, SDK execution wrapper, typed append idempotency, host installation,
production rollout or qualified customer outcome is claimed. The epic remains
open, as do the access/preparation/display/human-gate beads.

Old events are not renumbered. Generic embedded ingest paths, Core's entity-only
counter key, eviction/retention high-water semantics and failover remain outside
this fix. Ambiguous retained history fails sequence/causation validation.
Comparisons pin independently read revisions, not an atomic cross-run snapshot.
The source cap is 1,000 events and 2 MiB; larger runs are unavailable, not sampled.
Hash-only records do not prove an external action happened. No production
restart attestation or human authority can be inferred from synthetic tests.

`source-sha256.txt` identifies the source tested here. `artifact-sha256.txt`
identifies the local Core binary; it is not a deployed-image or hosted-connector
attestation. Unrelated worktree edits were preserved and excluded.
