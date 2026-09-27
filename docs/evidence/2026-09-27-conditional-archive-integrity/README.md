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
is involved in these synthetic fixtures. The production image built from the
base commit does not contain this subsequent repair.
