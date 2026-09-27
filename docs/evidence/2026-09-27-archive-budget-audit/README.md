# Production archive budget metadata audit

Date: 27 September 2026. Task: `t-a0616c`, parent `t-e1d5cf`.

## Result

All 63 non-system tenant archives fit the proposed entry, file, individual-file,
compressed-byte and declared-uncompressed-byte caps. **One exceeds the
250,000-row cap**, with 566,486 rows across 112 files. Strict archive rollout
therefore remains held. The system archive separately exceeds entry, file and
compressed-byte caps. Its operational compatibility also remains unresolved.

The anonymous dense archive's measured shape is 119 entries, 112 files,
60,696,266 compressed bytes, largest file 6,159,815 bytes, 566,486 declared
row-group rows and 81,181,257 declared uncompressed row-group bytes.

The largest file-count and uncompressed-byte maxima belong to another archive;
do not combine them into the dense archive's claimed shape.

| Final scan measurement | Non-system archives |
| --- | ---: |
| Tenants with Parquet files | 63 |
| Files | 26,875 |
| Compressed bytes | 200,930,656 |
| Declared rows | 686,813 |
| Declared uncompressed bytes | 358,787,387 |
| Largest tenant file count | 16,136 |
| Largest tenant entry count | 16,142 |
| Largest tenant compressed bytes | 92,646,731 |
| Largest tenant uncompressed bytes | 241,680,296 |
| Tenants above row cap | 1 |
| Tenants above any other measured cap | 0 |

The system archive had 219,410 files and 1,093,182,212 compressed bytes. Its
rows/uncompressed sizes were deliberately not scanned or reported as zero.
The second scan completed in 2,138 milliseconds, examining 246,451 entries.
Both raw aggregate reports are committed alongside this document. Their count
differences reflect normal concurrent activity; neither is an atomic snapshot.

## Method and verification

Standalone Rust tooling uses Parquet 59.3.0 without Arrow, Core or application
dependencies. It reads exactly the prechecked footer bytes, capped at 1 MiB,
and never reads data pages, replays WAL, opens a Core store or invokes cleanup.
Output contains no customer identifiers, paths, schema/statistics or event
payloads. It reports the same row-group accounting used by Core's budget.

The tool has a 60-second cooperative deadline, 300,000-entry cap and depth-eight
cap. Observed symlinks, unreadable entries, malformed/oversized metadata,
negative counts, changing files and unattributed legacy flat Parquet refuse
the whole report. It reports all six Core input dimensions per tenant, with
only aggregate maxima and one anonymous largest-row archive shape exposed.

Six synthetic tests and strict all-target Clippy passed. Tests verify privacy,
no mutation, each policy dimension, footer failures, traversal bounds and
symlink/legacy refusal. Corrupt data pages still pass the footer scan by design,
proving this is capacity metadata evidence rather than payload integrity proof.

Both binaries were built locally for static Linux x86_64 and their hashes
checked again on machine `7817667a276368` before execution. A local Linux
read-only/no-network invocation also verified the first binary's runtime.
Production execution used `nice -n 19 timeout 65`; no application deployment,
archive mutation, compaction, retention or customer activation occurred.

| Source | Binary SHA-256 |
| --- | --- |
| `fc19fbd85152e59b81f75d62e5561a8d98a3c79d` | `3b0e06db82fbb8f6b8847f52290305be65412310caa35bf7368acb031987d2c9` |
| `7b26d140cd3293be52ccebd1687542905416d2c1` | `1ad2cc39ea6e81935004f4e001cd87666651d55fbc346f4361fe41a6a33bf079` |

The second version adds the anonymous archive shape; all other policy/accounting
behavior is unchanged. Both source commits were signed, pushed and verified
equal to origin/main at their respective publication points. Owned temporary
executables were removed after both scans. Production remains release 46.
Gateway backend/WebSocket health passed at 2026-09-27T15:26:45Z.

## Remaining gates

Dense-archive repair is tracked as `t-40896f`, an explicit dependency of the
parent rollout task. An ignored, explicit-capacity regression fixture now
reproduces its 112-file / 566,486-row shape with unique synthetic events.
Under the existing 250,000-row limit, the direct read refused at 1.371 seconds
with zero resident events and HTTP returned 500 at 1.359 seconds, both citing
the row budget. The test remains intentionally red when explicitly selected;
it is not part of the default passing test suite.

A temporary 750,000-row experiment restored access: direct read timed out at
4.078 seconds during cache application, leaving 96,573 unverified resident
events. The tenant was not marked loaded. HTTP returned 503 at 5.005 seconds;
independent warmup completed at 8.204 seconds, deduplicated that resident prefix,
and a fresh append produced version 2 with exactly 566,487 events. The expanded
probe checks lack of authoritative load status after a timeout, rather than
incorrectly requiring that cache application has never begun.

However, `/usr/bin/time -l` reported peak RSS **1,716,977,664 bytes (1.60 GiB)**
for the native macOS test process. This includes synthetic fixture generation
and prior allocations; it is not an isolated Linux runtime measurement or a
hard memory bound. Two workers plus the configured 2 GiB cache would leave
insufficient staging confidence on the 4 GiB host. The row-limit-only change
was reverted, never committed as policy and never deployed. Its exact patch
and red/experimental-green logs are retained here. Next proof must separate
fixture generation from cold-load memory and verify bounded admission/headroom.

After restoring the original policy, the existing 90,064-event / 16,100-file
probe passed again: direct timeout 4.016 seconds, HTTP timeout 5.005 seconds,
verified warmup 5.255 seconds, complete probe 18.36 seconds. Final all-target
Core Clippy with warnings denied and formatting passed. The production source
defaults remain unchanged by this diagnostic increment.

Calibrate bounded dense-archive access using synthetic reproduction and resource
measurements. Metadata sizes do not establish allocator RSS, cold decode latency,
retention completeness or durable sequence high-water semantics. The configured
production cache allowance is 2 GiB within a 4 GiB / two-CPU machine; admission
and staging headroom must be considered before increasing row limits.

Remote Core build-only execution was rejected by automatic approval review
because it uploads private application source to Fly.io without destination-
specific approval. A question for that concrete action is pending. The action
was not retried or bypassed. The standalone diagnostic was built locally from
its own generic metadata tooling and public dependencies, with no Core source
or credentials included. Its separate upload/execution was approved by the
normal tool review. No new Core image or deployment was created in this step.
