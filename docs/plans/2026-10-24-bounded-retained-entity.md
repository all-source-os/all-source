# Bounded retained entity reads

## Failure

Production retained reads return `archive_hydration_refused` before selecting a single entity. An observed request required 2,943,518,688 bytes while 2,025,701,376 bytes were available after restart. Machine capacity remains unchanged.

## Invariant

A complete retained entity read must not require unrelated tenant events to become resident. It must combine the tenant's complete archive scan with its resident tail, deduplicate event IDs, and return no partial result on enumeration, decoding, cancellation or budget failure.

## Implementation

Scan strictly enumerated archive files in batches of 256 rows. Filter each Arrow batch by entity before decoding JSON payloads. Bound files, input rows, compressed bytes, retained payload and elapsed work. Bound the combined result to the existing event and encoded-response limits before retaining each event.

Hold the cache residency lease across archive and resident reads so eviction cannot discard the pending tail between them. Concurrent flushes may duplicate an event between the two sources; deduplicate by event ID. Preserve microsecond timestamp normalization and timestamp/version ordering.

The HTTP retained-query worker owns the scan directly instead of warming the whole tenant first. A successful scan certifies only the selected entity's maximum version, including duplicate event IDs, so a subsequent conditional write can reuse verified evidence without rescanning. Invalid versions and incomplete or cancelled reads never certify the entity. Existing bounded conditional-write version resolution stays unchanged.

## Verification

Passed 150 tests: 139 store regressions, ten retained-entity and HTTP contract regressions, and one pending-publication regression. They cover cold reads with large unrelated payloads under a small retained-payload budget, unchanged residency, archive/tail deduplication, conditional-write conflicts, version-cache reuse and isolation, cancellation, restart recovery and corrupt-file refusal.

Scoped Clippy passed with warnings denied; formatting and diff checks passed. Deployment verification remains pending: verify the production retained query and fresh Google sign-in through A Place Ahead before declaring the incident resolved.
