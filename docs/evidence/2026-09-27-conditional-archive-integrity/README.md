# Conditional append archive integrity

Date: 27 September 2026. Base: `91d9a13125ab02efb194c208fa5b33a9fac15489`.
Tracked under `t-e1d5cf`; the task remains open for bounded reads and readiness
isolation. This is separate from the lazy-append production repair being built
from the base commit.

## Reproduction and repair

Two synthetic local tests reproduced conditional writes accepting an incomplete
archive: both a cold append and an append after a tolerant query accepted version
three despite an unreadable Parquet file. The archive loader deliberately skips
unreadable files for queries. Previously, that partial cache was also accepted as
the complete version history for optimistic concurrency control.

The repaired loader reports whether every discovered file was readable. Cached
tenant state retains that fact. Conditional appends require a strict load before
the version check or WAL append; an unreadable file returns a storage error.
An unverified cache, including eager whole-archive hydration, must be checked
again. A failed strict read does not append an event or mark partial history
complete. Cache eviction discards the completeness marker with the resident data.

Ordinary appends remain lazy. Generic queries retain their existing tolerant
behavior. No archive files are deleted, rewritten or quarantined by this change.

Four local integrity tests cover cold history, a query-warmed partial cache,
whole-archive hydration and healthy archive compatibility. Together with the
acknowledged-version, optimistic-concurrency and embedded cold-start suites,
37 focused tests passed. Formatting and strict all-target/all-feature Clippy
passed. The all-feature Core library suite passed 1,990 tests with five ignored.

The rebuilt enterprise binary passed 12 actual HTTP tests across conditional
archive integrity, run ordering, typed capture and pending reviews. The new HTTP
case confirms a fixed 500 error for conditional appends before and after a
tolerant query and hard restart, no stored event for the rejected entity,
successful ordinary append and its recovery, available health, and an unchanged
synthetic corrupt fixture. Existing concurrent-writer and retry-recovery cases
remain green. Elixir formatting passed. These tests use real loopback Core
processes with private temporary directories, not a mocked storage response.

The source and local binary SHA-256 manifests identify this proof. This is not
a production availability proof or a large-archive load test.

## CI harness follow-up

CI run 36317947441 exposed an existing tenant-wire fixture race. The fixture
stored the last request's query string regardless of path, so a background health
request could overwrite the actual event-query observation with an empty string.
A deterministic test inserting an HTTP health request reproduced the exact
failure. This was a test observation defect, not evidence of a tenant-routing
regression in the production client.

The fixture now records only event-query GET requests and asserts exactly one
observation. It still checks the raw encoded query for a single authenticated
tenant; duplicate or missing tenant fields remain failures. Client calls must
also return successful responses, and unrelated unknown paths return 404.
Five focused tests pass, including the forced health-request race. The full
Query Service suite with CI seed `238999` passed: six doctests, 1,108 tests,
zero failures, two skipped and 127 excluded (local Elixir 1.19 reporting).
Configured formatting, warnings-as-errors compilation and strict Credo passed.
No production adapter behavior or authorization rule changed in this correction.

## Decoder failure after a valid batch

A second reproduction found that a valid footer did not guarantee a complete
file read. The single-file decoder used `while let Some(Ok(batch))`, which
treated a later Arrow decoding error like end-of-file and returned the already
decoded prefix as success. Strict tenant hydration therefore marked that file
complete and could authorize an append at the prefix's version.

The synthetic fixture writes 2,048 events in two row groups, preserves the
footer and first group, and corrupts one column in the second group. A direct
Arrow read proves the first 1,024 rows decode and the next batch fails. Before
the repair, both regression tests failed: the file loader returned success and
the conditional append did not reject the incomplete archive.

The decoder now propagates each batch error. Tolerant loaders continue their
existing whole-file skip policy and return healthy neighboring files; they no
longer mistake a decodable prefix of a damaged file for a successful file read.
Conditional appends reject the archive both cold and after a tolerant query.
The tests also verify no event notification and unchanged fixture bytes.

The six focused integrity tests pass. The separately named archive-read
manifest records the follow-up source without replacing the earlier proof.
Production release 45 and the image built from `e5db174d` do not contain this
subsequent decoder repair. No customer feature is enabled by this proof.

## Directory enumeration integrity

Two further synthetic regressions reproduced conditional appends accepting
version zero when a partition could not be read or the tenant path was a file.
The generic directory walk deliberately skips failed child reads, directory
entries and file-type lookups, while `is_dir` maps metadata errors to false.
Those tolerant decisions cannot establish empty retained history for a write.

Conditional loads now require successful tenant-root inspection and propagate
directory enumeration errors. A missing tenant root still permits a new stream;
an unreadable or non-directory root does not. Generic queries keep their
tolerant enumeration policy. Because that policy can omit inaccessible history,
a cache populated by a tolerant query remains unverified until a strict load
completes. This can add one archive reread before the first conditional append.

The permission fixture requires an unprivileged Unix test process, confirms
`PermissionDenied` before testing Core, and restores the temporary partition's
permissions even when the test fails. Cold and query-warmed cases reject the
append, emit no event, and leave a cold tenant unmarked as loaded. Together with
the decoder and existing archive cases, eight focused integrity tests pass.

An initial HTTP run of the decoder-only binary passed 11 cases but timed out
waiting for the first child process to become ready. The same failing case,
unchanged binary and seed `342434`, passed on retry. The helper discarded child
output, so the startup-timeout cause is unconfirmed; this is not evidence of a
resolved startup defect.

After both repairs, strict all-target/all-feature Clippy and Rust formatting
passed. The Core library suite passed 1,990 tests with five ignored, and 48
focused archive, acknowledged-version, concurrency, cold-start and read-scope
tests passed. The enterprise/analytics binary was rebuilt and passed all 13
actual HTTP/restart cases with seed `342434`, including the new unreadable
partition case. That case checks refusal before and after tolerant queries,
health availability, two independently started Core processes, and unchanged
synthetic archive bytes. Elixir formatting passed. Separate source and local
binary hashes identify this follow-up proof.

## Deployment candidate

The clean `e5db174d` Alpine build completed and pushed
`registry.fly.io/allsource-core:core-e5db174d-20260927`, digest
`sha256:294fe0c37493b0eb60ab87c2467da4741ec1992a0e1ca96ee0dda3cb1240649f`.
The build exited successfully; a builder-release timeout occurred after the
registry manifest was pushed. This candidate was not deployed because it lacks
the subsequent batch-decoder and enumeration repairs recorded above.

At 12:58 UTC, production remained on release 45's digest
`sha256:898fdad1e6dd7a6273612820b481fd6e0b4ce04053dd22c458e910c9c475bbbb`,
and a fresh Query Service readiness response reported healthy Core backend and
WebSocket connectivity. No machine, volume, application flag or production data
was changed during this follow-up.

## Limits

"Complete" here means every discovered Parquet file loaded successfully. It does
not prove that files were never deleted, that retention preserved an entire
entity history, or that storage cannot change outside the process. Warm-cache
integrity is not a continuous disk scan. Existing entity-only version keys,
eviction/concurrent-write races and durable sequence high-water marks remain
separate concerns.

Archive enumeration and decoding remain synchronous and unbounded by total
duration or input size. The existing loader timeout bounds lock waiting only.
Strict conditional requests can therefore still block request processing for a
large archive. Production customer capture must remain disabled until admission,
readiness isolation and bounded archive handling are verified.

Generic reads still tolerate corruption. A customer evidence read must not infer
complete retained history merely from a contiguous partial result; a strict read
contract remains required before source disclosure is enabled. This repair adds
conditional-write integrity, not that broader read attestation.

No customer input, production credential, host upload or live feature activation
is involved in these synthetic fixtures. These limitations describe this source
revision; the subsequent budget and worker repair is recorded separately.

## Production rollout: release 46

The complete decoder/enumeration repair, signed source
`0b51f7d4a894798cadac13210b34b5520e9a10a9`, was built from a clean export using
the existing `runtime-alpine` target. Image tag
`registry.fly.io/allsource-core:core-0b51f7d4-20260927` resolves to
`sha256:bdf4922db0d9199b4acc5075fbc2d249c223e4de425b4c1394a12897a25ffd58`.
The registry manifest was independently read after refreshing the expiring Fly
CLI registry credential. The build's builder-cleanup timeout followed a
successful push and was not treated as deployment proof.

All exact-source gates passed before deployment:

- [CI 36321005295](https://github.com/all-source-os/all-source/actions/runs/36321005295)
- [Docker Build 36321005282](https://github.com/all-source-os/all-source/actions/runs/36321005282)
- [Security Scanning 36321005261](https://github.com/all-source-os/all-source/actions/runs/36321005261)

Recovery snapshot `vs_R1m58Q2aG0pUkyVZeYM8jmP` was observed in `created` status,
created `2026-09-27T13:33:19Z`, retention five days, incremental size 92,395,751
bytes, digest `e303e3e34d9720fa6877316786a11f95cb6d9018cdeca9350ae0b5d95debed05`.
The listing also contained an in-progress placeholder with the same ID; the
completed entry with creation time and digest supplied the snapshot evidence.

Deployment resolved and installed the exact digest as release 46,
`rel_k96wz9rvmd4qznl0`. The running machine reported revision `0b51f7d4`,
version 0.25.1, on existing machine `7817667a276368` in `iad`, two shared CPUs,
4,096 MiB RAM and encrypted 10 GiB volume `vol_vwjoq95l03qzy88r` at `/app/data`.
UID, entrypoint and private-network configuration remain the existing Alpine
deployment configuration. Release 45's digest remains the rollback target; no
snapshot restore or data deletion occurred.

Fresh verification at 13:45 UTC passed direct Core health, Query Service
backend/WebSocket readiness, Control Plane Core health and public web health.
Direct Core reported healthy event-sourced metadata with 27,843 system events;
the Fly startup check also passed at 13:44:06 UTC. No claim about WAL corruption
counts is inferred from this health response.
A further readiness response at 13:59:36 UTC remained healthy for both backend
and WebSocket connectivity.

This rollout does **not** include `4ea1706f`'s archive budgets, worker admission
or cancellation checks, nor the later cache-residency work. It deploys no Query
Service, Control Plane, web or MCP changes and activates no customer feature.
