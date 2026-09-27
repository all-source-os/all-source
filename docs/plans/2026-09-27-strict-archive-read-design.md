# Strict archive work: bounded input and HTTP isolation

This implements part of Chronis `t-e1d5cf`, within the existing customer-agent
delivery goal. Conditional writes need complete retained history, but archive
traversal and decoding currently run synchronously inside HTTP handlers. A slow
archive can monopolize request workers. A timeout waiting for another loader
does not bound the subsequent archive walk.

## Decision

Keep tolerant legacy queries compatible. Apply explicit work budgets to the
strict archive loader used by conditional writes. Run conditional HTTP appends
in admitted blocking workers, with cancellation checked before durable mutation.
Ordinary appends preserve lazy archive loading. No customer permission, feature
flag, additional datastore or external model call is introduced.

Three possible read contracts were considered: change every query to strict,
add an explicit strict query mode, or add a separate evidence endpoint. An
explicit strict mode is the intended protocol direction because old consumers
retain their behavior and new consumers can require a response attestation.
However, the current cache can be evicted between hydration and query. A marker
alone would certify an incomplete result. **No strict query response attestation
is implemented by this increment.** Cache lifetime and snapshot consistency must
be solved before that API or customer evidence disclosure is enabled.

## Admission limits

`EventStoreConfig.strict_archive_limits` is controlled by the embedding service,
not deserialized from a request. Its defaults are:

| Dimension | Default | Enforcement point |
| --- | --- | --- |
| Elapsed cooperative work | 4 seconds | Lock admission, traversal, metadata, batch decoding, cache application |
| Directory entries | 100,000 | Every visited entry, including non-Parquet entries |
| Parquet files | 50,000 | Before adding a file to the archive list |
| Individual compressed file | 32 MiB | File metadata, before parsing the footer |
| Total compressed file bytes | 256 MiB | Accumulated before each file is parsed |
| Declared uncompressed bytes | 256 MiB | Row-group metadata, before decoding |
| Declared rows | 250,000 | Row-group metadata, before decoding |
| Decode batch size | 256 rows | Strict reader construction |
| Concurrent conditional HTTP workers | 2 per process | Semaphore admission |
| Waiting conditional HTTP requests | 16 per process | Separate bounded admission semaphore |
| Admission wait | 100 milliseconds | Included inside the response deadline |
| Conditional HTTP response deadline | 5 seconds | Awaiting the blocking worker |

The four-second work budget leaves space inside the five-second Core response
deadline and six-second internal customer store transport deadline. These are
conservative admission defaults, not claims about how quickly an arbitrary
production archive will load. A legitimate larger or slower cold archive is
refused. There are currently no environment-variable overrides for these limits.

Arithmetic overflow, negative metadata, a missing budget, or elapsed/cancelled
work cannot silently widen strict admission. Generic reads do not use this
budget. Strict reads propagate enumeration and decode failures introduced by
the preceding integrity repair.

## Worker ownership and cancellation

Single and batch HTTP handlers, including versioned handlers, route conditional
appends through the same helper. A short bounded queue lets bursts of quick
writes drain. A full queue or expired admission wait returns the existing
`QueueFull` error (HTTP 503) before starting work. There is no unbounded admission queue. The permit
moves into the blocking closure; dropping or timing out its caller never frees
capacity while the underlying operation remains alive.

A caller-drop guard sets an atomic cancellation flag. The store checks it at
entry, during archive work, after the durability gate, and after acquiring the
entity version entry, before appending to WAL. Cancellation observed before the
append refuses the write. A timeout after mutation begins remains uncertain:
the five-second response explicitly says the append outcome may be uncertain.
There is no write retry or promise to undo a durable event.

Batch handlers retain their existing per-event behavior and partial-success
semantics. The deadline is per conditional event, not an overall atomic batch
deadline. Warm conditional requests also use the pool; ordinary requests do not.

## Limits of this increment

These checks bound admitted input and cooperative work, not allocator RSS or
kernel I/O latency. Parquet metadata is not a proof against malicious forged
metadata. An in-flight filesystem operation or decoder call cannot be forcibly
cancelled. Its worker keeps its permit until it returns. Generic query handlers,
cache application locks, eviction and post-WAL work still have their existing
behavior; unrelated traffic is not guaranteed a latency bound under every form
of contention.

If cancellation occurs during cache application, some historical events may
already be cached. That load is not marked complete and cannot authorize a
conditional write. The existing cache eviction/completeness race, global event
counter accounting and entity-only version keys remain open work. Neither this
increment nor the preceding decoder fix establishes indefinite high-water marks,
retention-safe idempotency, atomic strict evidence snapshots or human authority.

## Verification scope

Use synthetic stores and owned loopback servers. Verify every admission
dimension, intact healthy cold writes, tolerant query compatibility, cancellation
before WAL, cancellation after checkpoint contention, retained worker capacity
after timeout/disconnect, and real HTTP health/ordinary writes while a
conditional request waits for cold-load admission. Rebuild the real enterprise
Core binary and verify oversized-file refusal across HTTP and process restarts,
alongside the existing concurrency, retry and review evidence cases.

Source boundaries: `apps/core/src/infrastructure/persistence/archive_budget.rs`,
`apps/core/src/infrastructure/persistence/storage.rs`, `apps/core/src/store.rs`,
`apps/core/src/infrastructure/web/archive_work.rs`, and
`apps/core/src/infrastructure/web/api.rs`. Verification is recorded separately
under `docs/evidence/2026-09-27-strict-archive-work/`.

## Cache lifetime follow-up

Two synthetic reproductions confirmed stale writes: eviction removed a buffered
predecessor before its Parquet flush, and eviction between strict hydration and
the version check reset the counter to zero. Both accepted `expected_version=0`
for an entity with an existing event.

A cache residency read/write gate now protects writers, cache application and
eviction's index/version rebuild. Conditional appends acquire a read lease and
recheck completeness before their version check; a lost verified cache refuses
the write. Cold archive I/O runs outside this gate. A per-tenant generation is
captured before reading and checked under the lease before applying history, so
eviction during the read cannot certify an outdated combination of disk/cache.
Cache application and its completeness marker share the lease.

Eviction acquires the exclusive lease but never performs or waits for disk I/O.
It needs immediate exclusive access to the storage owner and no pending batch
for the tenant. An in-flight flush, pending batch, read-only store or missing
archive retains resident history. Existing cache budgets are soft: if the LRU
candidate cannot safely be evicted, enforcement returns instead of spinning or
discarding its only queryable copy. Normal flush/checkpoint can later make it
eligible. Replicated events also enter the configured Parquet buffer before
cache eviction can consider them eligible.

Hydration deduplicates and accounts each inserted event under the event lock;
it no longer subtracts global vector lengths across concurrent work. Batch
ingestion adds the new batch size to the resident counter, not the whole store
size. Projection replay effects, tenant/entity counter keying, external archive
mutation and retained-history high-water marks remain separate constraints.
This follow-up still does not implement the strict HTTP read attestation.

## Strict retained-entity protocol follow-up

With cache residency protected, callers can now explicitly request
`GET /api/v1/events/query?integrity=retained-entity-v1`. The request requires one
authoritative tenant, one entity and an explicit limit from 1 to 1,001. It refuses
time/type/payload filters, nonzero offsets, descending order and unknown protocol
versions. Requests without this selector retain the legacy tolerant behavior.

The store strictly hydrates retained archives, acquires a residency lease,
rechecks completeness, then holds both that lease and the event read lock through
selection/materialization. The entity index is capped before copying offsets;
each offset must still match its event ID and entity. Tenant and explicit
`ReadScope` filtering apply before returning events. A concurrent append cannot
change the vector during materialization, and eviction cannot remove verified
history until the read finishes. Cancellation and the original work deadline
are checked before success, including after materialization.

The operation refuses more than 1,001 indexed entity entries. It counts serialized
event bytes without allocating a second payload-sized buffer, leaving 16 KiB of
envelope space inside the customer's 2 MiB transport cap. All retained matching
events must fit that byte budget even when the requested page is smaller.
Index entries are still keyed by entity alone, so another tenant using the same
entity can conservatively consume the entry cap; this never authorizes access.

Strict HTTP reads share the existing two-worker/sixteen-waiter admission pool
with conditional appends. Blocking archive work stays off Tokio request workers,
and cancellation retains its worker permit until the closure exits. Success
includes exactly this attestation object, alongside ordinary page counts:

```json
{
  "archive_integrity": {
    "protocol": "retained-entity-v1",
    "tenant_id": "authoritative-tenant",
    "entity_id": "requested-entity"
  }
}
```

`entity_version` is omitted for strict results rather than reading an unlocked
global counter after the snapshot. `has_more` and `total_count` remain explicit;
an attestation alone is not proof that the returned page includes every event.
The internal customer run adapter requests 1,001 rows, requires the exact bound
attestation, verifies complete-page counts, and refuses histories above its
1,000-event domain limit. An older Core silently ignoring the new parameter
returns no attestation and is refused. There is no fallback or retry.

Strict snapshots return timestamps truncated to the microsecond precision of
the existing Parquet schema and sort by `(timestamp_micros, version)`. This
canonicalization applies to bounded returned copies, not the resident events,
WAL, ingest responses or generic queries. It prevents a nanosecond live timestamp
from changing the review digest or event order after archive reload. Encoded
admission still counts the original event, conservatively, before copying it.
No storage schema migration is required.

This establishes completeness of the verified retained cache/archive snapshot,
not proof that retention or external file deletion never removed history. It
does not establish durable high-water marks, indefinite operation deduplication,
human approval, customer credentials or feature activation. Existing production
conditional users include Resend ingestion into arbitrary routed tenants;
compatibility cannot be inferred solely from the small operator tenants.
