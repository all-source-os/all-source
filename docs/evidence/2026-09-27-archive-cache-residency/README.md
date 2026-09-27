# Archive cache residency and conditional version integrity

Date: 27 September 2026. Existing open task: `t-e1d5cf`. This extends the
[archive work boundary](../2026-09-27-strict-archive-work/README.md); it does not
complete strict customer evidence reads or activate the customer product flow.

## Reproduced failures

Two new synthetic tests failed against the preceding implementation:

1. A conditional append created version 1, still buffered for Parquet. Cache
   eviction dropped it from queryable memory. Another append incorrectly
   accepted `expected_version=0`.
2. A cold conditional append hydrated version 1, then waited at a deliberately
   held checkpoint gate. Cache eviction ran before its version check. The stale
   request returned `Ok(1)` instead of refusing `expected_version=0`.

Both tests pass with the residency repair. They use owned temporary archives;
the checkpoint pause has a bounded lifetime. No production data was used.

## Resulting behavior

Writers retain a shared cache lease until indexing/persistence finishes.
Conditional writes recheck verified cache completeness while holding that lease.
Eviction requires an exclusive lease before rebuilding indexes and counters.
Hydration applies events and its completeness marker under a shared lease; a
per-tenant generation check refuses application if eviction crossed its archive
read. Regular cold reads keep disk I/O outside that lease.

Eviction is best-effort and does no disk I/O. It requires immediate exclusive
storage access and no pending tenant batch, so buffered history and in-flight
flushes retain their cache. In-memory-only and read-only stores also retain it.
Cache budget enforcement returns when its chosen victim cannot safely be
evicted; this deliberately favors retained history over a hard memory cap.

Replicated events enter the configured Parquet buffer before eviction can
discard their cache. Hydration deduplicates under the event lock and increments
resident accounting only for each inserted event. Batch ingestion counts the
new batch, fixing its previous addition of the entire resident vector size.

## Verification

- All-feature Core library: 2,008 passed, five pre-existing ignored.
- Focused archive/acknowledgement/concurrency/scope suites: 56 passed.
- Seven new consistency cases cover both stale-write reproductions, in-memory
  history retention, replicated archive reload, nonblocking eviction during
  storage work, read-only replica protection, and new-batch resident accounting.
- Actual loopback leader/follower WAL replication passed. The follower retained
  pending data, flushed it, evicted its cache, then reloaded the exact same event
  IDs received from the leader. This explicitly ran the normally ignored
  `test_leader_follower_wal_replication` case; it is not inferred from a skipped
  test count.
- Rust formatting and all-target/all-feature Clippy with warnings denied passed.
  The full suite passed again after bounding the storage-contention fixture and
  protecting read-only replicas; the enterprise + analytics binary rebuilt successfully.
- That rebuilt binary passed all 14 actual Core HTTP/restart cases with seed
  342434, including oversized archive refusal, concurrent conditional commands,
  acknowledgement recovery and pending review behavior.

The first final-binary HTTP run passed 13 cases but its first child process
missed the existing startup deadline. The helper had discarded child output.
It now retains a bounded 16 KiB tail and uses informational child logging, while
preserving the same readiness deadline. The unchanged binary and seed then
passed all 14 cases with diagnostics enabled. The original startup failure's
cause remains unconfirmed; neither that retry nor the diagnostic improvement
is a claim that a runtime startup defect was fixed. Query Service formatting,
warnings-as-errors compilation and strict Credo passed for the fixture change.

The replication fixture's first run remained active beyond 60 seconds and was
terminated by its identified process ID. Its cleanup previously awaited the
receiver without a deadline. Cleanup now aborts and joins the owned receiver and
shipper with two-second deadlines; the final test completed in 0.51 seconds.
This proves replication and archive reload, not graceful receiver shutdown or
cross-datacenter failover. The production shutdown behavior was not changed.

## Remaining limits

No strict HTTP evidence-read attestation is added here. Legacy tolerant queries
retain their corruption policy and can still race between initial hydration and
their query snapshot. A future strict read must pin completeness through result
materialization. Entity counters are still keyed by entity alone, and retained
event counts are not durable sequence high-water marks. Retention, externally
deleted files, projection reapplication and indefinite operation deduplication
are not solved by cache residency locking.

`source-sha256.txt` and `local-binary-sha256.txt` identify final local proof inputs.
Temporary logs use `/private/tmp/strict-archive-consistency-{red,green,all,clippy,binary,http,http-diagnostic,credo}.log`
and `/private/tmp/strict-archive-replication.log`; the committed tests and manifests
are the durable record. This source is
not included in production release 46, which runs `0b51f7d4`'s preceding archive
decoder/enumeration repair. Release 46's exact image, CI gates, snapshot and
fresh health checks are recorded in the
[integrity rollout](../2026-09-27-conditional-archive-integrity/README.md#production-rollout-release-46).
No customer flag, credential, host upload or human authority was enabled.
