# Isolated dense archive capacity measurement

Chronis: `t-40896f`, under compatibility gate `t-e1d5cf`.
Production remains release 46. The candidate source now selects 600,000 rows;
the previous source policy admitted 250,000. No deployment accompanies this report.

## Why a separate process

The preceding 750,000-row experiment generated its fixture and attempted a
direct read before timing HTTP. Its process peak therefore included allocations
that a production cold HTTP request would not perform. This probe generates the
synthetic archive once, exits, then measures a new process against a fresh copy.

The baseline fixture contains 566,486 unique synthetic events across 112 Parquet files.
A marker binds the expected tenant, protocol and counts. The measured helper
only accepts this owned fixture, skips the preliminary direct read, and sets
750,000 rows in its embedding config. This experimental ceiling is separate
from the 600,000-row candidate runtime policy.
Each measurement gets a separate writable copy because successful verification
appends one event and closing the store may flush it.

The test uses the same mimalloc allocator as the server. The optional
`ALLSOURCE_CAPACITY_BACKGROUND_BYTES` reservation is touched and retained for
the entire load, with a hard maximum of 3 GiB. This models unavailable headroom;
it is not a realistic simulation of another tenant, projections or cache eviction.

## Native measurements

| Fresh process | HTTP response | Verified warmup | Peak RSS |
| --- | --- | --- | --- |
| Initial system-allocator control | 503 at 5.003 s | 9.223 s | 1,263,222,784 bytes |
| Server allocator, mimalloc | 503 at 5.007 s | 9.742 s | 1,427,767,296 bytes |

Both runs used macOS debug builds with all features and no background
reservation. Both verified no append from the expired request, then a fresh
request produced version 2 and exactly 566,487 resident events. They do not prove
Linux performance or capacity on the deployed 2 CPU / 4 GiB machine.

## Linux release procedure

Append `apps/core/tests/archive_capacity.Dockerfile.fragment` to the existing
Core Dockerfile in a clean local export. The fragment extends the production
enterprise + analytics build stages, compiles the explicit test in release mode,
and exports its Linux executable. The export and build use the local OrbStack
Unix socket. No repository source is uploaded to a remote builder and no image
is pushed to a registry.

Run the binary in a container with 2 CPUs, 4 GiB memory, no additional swap and
network disabled. Mount the executable read-only and a fresh fixture copy
writable. Reserve 2 GiB of touched background memory, then use the explicit
`proposed_row_policy_http_only_in_existing_synthetic_fixture` test. Capture its
HTTP/result transcript, peak RSS, cgroup peak/events and container OOM status.

The local host is Apple Silicon, so the Linux amd64 executable is translated.
Its bounded-memory result is useful evidence; its elapsed time does not establish
performance on Fly shared x86 CPUs or the production volume.

## Provenance and outstanding gates

The local build export starts at `ebc772d501306d1454c000c1e1b14eb439bb43a7`,
with the timestamp fix later signed as `99a161d6` and the capacity helper copied
in explicitly. Its helper precedes the semantically equivalent Clippy cleanup
from `map(...).unwrap_or(0)` to `map_or(0, ...)`; separate hashes record both.

Linux results and the row-ceiling decision are recorded below; complete
production compatibility and rollout remain outstanding. A bounded one-or-two worker setting has separate
[local verification](../2026-09-27-archive-worker-setting/README.md); the
capacity export predates that setting and measures one request with the original
pool. Raising the row limit based only on
the native result would not establish headroom for two concurrent loaders plus
the configured 2 GiB cache. Corruption, file/byte/entry bounds, request deadlines,
cancelled-write protection and the original archive-shape regression must remain
covered by subsequent verification. Production stays on release 46.

## Local build attempts

The first local build completed the production enterprise binary stage, then
reached its 1,800-second execution limit while compiling test-only dependencies.
The exec process returned 124 and BuildKit marked build
`x7ahho4nu8ygk73gyh5rgpall` as `Error` with the test stage `CANCELED`. No capacity
executable was exported. An observation timeout was not treated as termination.

After that terminal result, a second local build reused the completed stages
with a 3,600-second execution limit. The test build now uses four compiler jobs,
matching the production build. The export also includes the signed helper's
equivalent `map_or` cleanup; `export-retry.sha256` records this distinct input.
The original `export.sha256` remains historical evidence for the first attempt.
No remote builder or registry push was involved.

## Linux baseline results

The second build completed successfully. Its exported static Linux amd64
executable has SHA-256
`77c9e34dbb6c80c92f263f4b266d57a57baeca7af25c9f41c135253fd2ca2a7a`.
Both measurements used the same executable, a fresh fixture copy and local Alpine
image `sha256:c83674e1999044d33d751661371b873539f47e5b5c5ca3320c7e0377acca6238`.

| Touched background reserve | Cold HTTP | Peak process RSS | Cgroup memory peak |
| --- | --- | --- | --- |
| 2 GiB | 200 at 1.762 s | 3,494,920 KiB | 3,636,641,792 bytes |
| 2.5 GiB | 200 at 1.656 s | 4,027,108 KiB | 4,165,627,904 bytes |

Each process asserted version 2 and exactly 566,487 events after the one command.
Both containers exited zero, reported `OOMKilled=false`, and had zero cgroup
OOM, OOM-kill and memory-max events. Kernel controls confirmed
`memory.max=4294967296`, `memory.swap.max=0`, and `cpu.max=200000 100000`.
No network interface or host service port was exposed. Full timing/cgroup
transcripts and final container states are retained beside this report.

These Linux runs completed before the five-second response deadline. They do
not themselves exercise the abandoned-request branch; that remains covered by
the slower native measurements and controlled real HTTP warmup tests. Local
amd64 translation and synthetic records also do not establish production I/O
latency or the memory cost of every real payload shape.

The 2.5 GiB reservation covers the observed production RSS baseline, but leaves
only approximately 123 MiB below the container ceiling for this synthetic shape.
Passing at 566,486 rows therefore does not justify admitting 750,000 rows.
The explicit fixture helpers now accept `ALLSOURCE_CAPACITY_ROWS` from 566,486
through 750,000, bind that count into the marker, and verify the resulting count
after the command. This permits measuring the proposed ceiling before choosing
the final policy.

## Upper-bound experiment and selected policy

The first parameterized build compiled the new test successfully, but its broad
`find ... -exec cp` export selected a second, stale executable left in the Cargo
cache. Comparing the artifact hash caught this: it still matched the baseline
binary. No larger-row result was accepted from that artifact. The corrected
export copied the exact executable reported by Cargo. Its SHA-256 is
`b44bd261de656b9fe95c65c62df5bc4e78f54d92cd4fbabe2d54a51f382bba61`.
The checked-in Docker fragment now removes old matching test executables before
building and requires exactly one executable afterward, preventing silent reuse.
This is Docker build plumbing; all fixture and measurement logic remains Rust.

This build selected the Core package explicitly, changing some dependency
feature unification. Therefore the baseline was repeated with the same corrected
binary before comparing larger fixtures. Every measurement below used a fresh
fixture copy, 2 CPUs, 4 GiB, no swap, and 2.5 GiB of touched background memory.
Generators ran separately and confirmed each marker's exact event count.
`upper-bound-source.sha256` identifies the immutable exported helper before the
final comment and ignore-reason updates. `selected-policy-source.sha256` records
the candidate source, including its new default row limit and export correction.

| Unique synthetic rows | HTTP result | Peak process RSS | Cgroup peak | Kernel OOM kills |
| --- | --- | --- | --- | --- |
| 566,486 | 200 at 1.609 s | 4,016,692 KiB | 4,148,899,840 bytes | 0 |
| 600,000 | 200 at 1.615 s | 4,065,128 KiB | 4,203,413,504 bytes | 0 |
| 750,000 | Process killed before response | 4,193,272 KiB | 4,294,967,296 bytes | 1 |

Successful runs asserted version 2 and exactly `rows + 1` resident events.
The 750,000-row container reported `OOMKilled=true`, exit code 9, and one cgroup
OOM kill. The timing utility printed an inconsistent `Exit status: 0` after its
signal-9 line; the kernel counters and Docker exit state establish failure.
The experiment was bounded to an owned local container with no network access.

Select **600,000 rows with one strict archive worker** for the 4 GiB Fly
configuration. This admits the observed 566,486-row archive while rejecting the
750,000-row proposal. The measured 600,000-row shape leaves 91,553,792 bytes
(approximately 87 MiB) after the deliberately conservative 2.5 GiB reservation.
That is narrow headroom, not an RSS guarantee for arbitrary payloads, generic
queries, projections or competing work. Existing compressed/uncompressed byte,
file and entry limits remain necessary. Do not increase workers or row bounds
on this machine based on these measurements.

The source change retains the four-second direct budget, five-second HTTP
deadline, bounded warmup, corruption refusal and cancelled-write protection.
The default metadata-budget regression accepts the existing archive, reaches
600,000 exactly, and rejects the next row. Full source/default-policy and actual
HTTP validation are recorded below; production deployment still
requires the existing compatibility, CI and authorization gates.

## Final candidate verification

- All-feature Core tests: 47 suites, 2,351 passed, 14 explicitly ignored.
- Both expensive compatibility regressions executed separately and passed.
  The 16,100-file / 90,064-row case returned 503 at 5.005 seconds and completed
  verified warmup at 5.757 seconds. The 112-file / 566,486-row case returned 503
  at 5.003 seconds and completed warmup at 9.014 seconds. Each asserted that the
  expired request did not append and that a fresh request produced version 2
  and exactly one new event. The dense case also deduplicated the unverified
  65,537-event prefix left by the preceding direct-read deadline.
- Strict all-target/all-feature Clippy and Rust formatting passed.
- The enterprise + analytics Core binary was rebuilt after Cargo testing and
  frozen before actual HTTP tests. SHA-256:
  `47c455dc4bd3eeb91a7ea1d548ea68538597d94224e5db1facaaaf6096bfcb44`.
  Its hash was unchanged afterward.
- Six actual HTTP suites passed: **18 tests**, seed 342434. Coverage includes
  ordering, conditional capture and retries, strict retained reads, corruption
  and oversized-input refusal, evidence comparison, pending review recovery,
  source revocation, and hard restart. The fixture uses one archive worker.

These native default-policy checks complement the constrained Linux measurements;
they do not convert the native timing into a production performance claim.
Commands, summaries and focused transcripts are retained alongside this report.
No Fly deployment, customer activation, private payload export or production
archive mutation occurred.
