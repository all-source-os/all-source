# Isolated dense archive capacity measurement

Chronis: `t-40896f`, under compatibility gate `t-e1d5cf`.
This experiment does not change production's 250,000-row admission limit.

## Why a separate process

The preceding 750,000-row experiment generated its fixture and attempted a
direct read before timing HTTP. Its process peak therefore included allocations
that a production cold HTTP request would not perform. This probe generates the
synthetic archive once, exits, then measures a new process against a fresh copy.

The fixture contains 566,486 unique synthetic events across 112 Parquet files.
A marker binds the expected tenant, protocol and counts. The measured helper
only accepts this owned fixture, skips the preliminary direct read, and sets
750,000 rows in its embedding config. Ordinary runtime defaults remain unchanged.
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

Linux result and production compatibility remain outstanding until recorded
below. A bounded one-or-two worker setting has separate
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
