# Optimistic concurrency without loading history

**Date:** 2026-10-04
**Status:** ✅ investigation complete; read-side and eviction fixes landed, write side open
**Surface:** `apps/core/src/store.rs`, `apps/core/src/infrastructure/persistence/storage.rs`

## Summary

Core derives an entity's current version by counting the events it holds in
memory. Every other production event store reads a stored version instead. The
counting approach forces a conditional write to hydrate a tenant's entire
archive, which is what exhausts memory on a 4 GB machine, and it also produces
wrong versions after a cache eviction.

Core already persists the authoritative number: `version` is a Parquet column
written with every event. The fix is to read it, not to add an index, a sidecar
or a key-value store.

## What Core does today

A conditional write forces a complete tenant load (`store.rs:613`):

```rust
if expected_version.is_some() {
    self.ensure_tenant_loaded_budgeted(event.tenant_id_str(), true, cancellation.cloned())?;
}
```

`entity_versions: Arc<DashMap<String, u64>>` (`store.rs:169`) holds the number
the OCC check reads. Six sites populate it, and every one counted rather than
read:

```rust
*store.entity_versions.entry(event.entity_id_str().to_string()).or_insert(0) += 1;
```

Three are write paths (`ingest`, `ingest_batch`, and the pipeline append), where
the count is the number being *assigned*. Three are load paths — WAL replay,
archive hydration, and the rebuild inside eviction — where the count is a guess
at a number already written down.

The comment at the eviction site stated the intent plainly: the counter reflects
"how many events of this entity remain". That is a different quantity from the
entity's version, and the two diverge as soon as any history is not resident.

## Three defects follow from this

### 1. The archive read decodes every column

`read_parquet_file` builds its reader with no projection
(`storage.rs:884`):

```rust
let mut builder = ParquetRecordBatchReaderBuilder::try_new(file)?;
```

`record_batch_to_events` then parses each row's payload and metadata:

```rust
serde_json::from_str(payloads.value(i))?,
```

Each payload becomes a `serde_json::Value` — nested `BTreeMap`s with
individually heap-allocated `String`s, commonly five to twenty times the size of
the JSON text. That expansion dominates resident memory during a hydration, and
it is incurred even when the caller needs only two columns.

The Parquet schema (`storage.rs:177` onward) puts `entity_id` at leaf 2 and
`version` at leaf 6, so a `ProjectionMask` over those two can skip the payload's
decompression and its `serde_json` parse entirely.

**Projection was tried and then deliberately dropped.** A projected read only
touches the bytes it selects, so corruption anywhere else goes unnoticed, and
`later_batch_failure_cannot_authorize_a_conditional_append` exists precisely to
stop a damaged archive authorising a write. The landed version decodes every
column and keeps only two, discarding each batch as it goes. Peak heap is one
batch rather than the tenant's history, which is the term that was causing the
OOM; the payload's decompression cost stays, and it is CPU, not memory.

### 2. The archive budget meters the wrong quantity

`ArchiveReadBudget::decoded_metadata` charges `group.total_byte_size()`, the
uncompressed *Parquet* size. Actual memory is dominated by the
`serde_json::Value` expansion, which the budget never observes.

This explains the tuning recorded in `archive_budget.rs`: 600,000 rows fit the
measured envelope and 750,000 rows OOM'd under the same reservation. The ceiling
was fitted to a proxy for the thing that runs out, so it holds only for the event
shape it was measured against. A tenant with larger payloads breaches the real
limit while still inside the configured one.

### 3. Cache eviction corrupts versions for other tenants

`try_evict_tenant` (`store.rs:1855`) is reachable from ordinary cache-budget
enforcement. On any eviction that drops at least one event
(`store.rs:1903`):

```rust
self.index.clear();
self.entity_versions.clear();
for (offset, event) in events.iter().enumerate() {
    ...
    *self.entity_versions.entry(event.entity_id_str().to_string()).or_insert(0) += 1;
}
```

`entity_versions` is global, but the rebuild counts only events still resident.
Evicting tenant B therefore rewrites tenant A's counters from A's resident events
alone. Under lazy loading A's history is routinely partial, and an entity whose
events live only in the archive drops to zero. The next write to it is assigned a
version that already exists on disk.

An eviction that drops nothing returns early and is harmless.

This makes counting a correctness bug independent of the memory exhaustion.

## How other event stores solve it

| System | Where the version lives |
|---|---|
| EventStoreDB / KurrentDB | separate index: in-memory memtable plus on-disk PTables with bloom filters |
| Marten | `mt_streams.version` column, checked before inserting into `mt_events` |
| Axon | sequence number assigned at write time, carried in event metadata |
| DynamoDB | per-aggregate HEAD item, conditional write on `attribute_not_exists` |
| MongoDB | snapshot version document, unique compound index on `(aggregateId, sequenceNumber)` |
| Message DB | gapless `position` column plus a `stream_version()` function |
| Postgres, generic | version stored with the event, `UNIQUE (aggregate_id, version)` |

The mechanisms differ; the principle does not. The version is written once and
read back, never recomputed from the event count.

Core's closest analogue is the generic Postgres pattern, because Core already
stores the version alongside the event. What Postgres adds is a constraint making
a duplicate version unwritable, and an index answering the lookup. Core has
neither.

EventStoreDB's shape is the closest architectural fit for a fix: a hot in-memory
index backed by durable on-disk tables, rebuildable from the log.

## Rust crates surveyed

| Crate | Version | Verdict |
|---|---|---|
| fjall | 3.1.12 | ✅ best pick *if* a KV store is ever needed — pure Rust LSM, suits append-heavy writes |
| redb | 4.3.0 | acceptable, copy-on-write B+trees, conservative durability |
| rust-rocksdb | 0.25.0 | ⛔ C/C++ FFI with no advantage over fjall here |
| heed (LMDB) | 0.22.1 | ⛔ mmap single-writer design complicates multi-process |
| persy | 1.8 | ⛔ single-file bottleneck |
| sled | — | ⛔ maintenance mode since April 2026, no new features |

Event-sourcing frameworks (`cqrs-es`, `eventually-rs`, `disintegrate`,
`event_sourcing.rs`, `kameo_es`) all assume a SQL backend and none exposes a
reusable version-index primitive. `thalo` is unmaintained.

**None of these should be adopted.** Core already has a durable log (the WAL) and
already stores the version in Parquet. Adding a key-value store or an append-only
sidecar would make a third copy of a fact that is already written twice.

## Parquet scan cost, measured against our own data

The production archive reported in the incident analysis holds 108,145 events in
18,144 files at 131.6 MB compressed. That is 7.25 KB and 5.96 events per file — a
pathological small-file layout, but a small dataset in absolute terms.

A two-column scan of the whole archive is therefore seconds of work, not minutes.
Row-group statistics cannot shortcut it: min/max are global per row group, so they
answer `MAX(version)` overall but never `MAX(version) GROUP BY entity_id`. The two
columns have to be read. At this size that is fine.

Projection pushdown via `ProjectionMask` is supported and stable in arrow-rs. The
page index exists in the crate but is not wired into the default reader's
filtering, so it buys nothing here.

A manifest or sidecar index in the Iceberg style earns its keep at roughly 100×
this data. Compacting 18,144 files into about 100 is the larger and independent
lever.

## The invariant

**The version is read, never counted.**

Six sites violated it. The three load paths now read; the three write paths still
count, and that is the remaining door. A change that only stops the memory
exhaustion, or only caches the number, leaves the rest open.

## The HTTP layer hydrated too, and that was the production path

The store's conditional-write guard was not the only full load. `prepare_http_append`
called `prepare_http_archive`, which runs
`ensure_tenant_loaded_with_limits(tenant_id, require_complete = true, …)` whenever
`http_archive_warmup_timeout` is set — and `store.rs:3145` sets it to 30s for any
store with a `storage_dir`, so it is on in production.

`archive_work::append` short-circuits unconditional writes, so this never affected
an ordinary append. A **conditional** write over HTTP hydrated the tenant twice:
once in the admission check, once in the store. Fixing only the store would have
left the production path exactly as it was.

The admission check no longer warms anything. It validates the event, confirms the
store is writable, and takes its slot; the conditional write then reads the versions
it needs. Rejections that the warmup used to produce — a corrupt archive, an
over-budget one — now come from the read itself, which is what
`http_append_rejects_corrupt_over_budget_or_read_only_archives` asserts.

One behaviour changed deliberately. The warmup ignored caller cancellation so an
abandoned request's work was not wasted. The version resolve honours cancellation
instead: under memory pressure, abandoning the read is the point.

## What landed

1. `observe_entity_version` raises an entity's mark to the version carried by the
   event, as a max. Archive hydration uses it, so a reload can no longer lower a
   mark or inflate one.
2. `try_evict_tenant` no longer clears the global `entity_versions` map. The mark
   is a high-water mark; dropping a cache lowers nothing on disk.
3. `ParquetStorage::load_entity_versions_for_tenant` folds a tenant's archived
   versions without building events, and a conditional write resolves its version
   through it instead of `ensure_tenant_loaded_budgeted(.., true)`. A conditional
   write no longer hydrates the tenant, and no longer takes the tenant load lock.
4. A damaged archive still refuses a conditional write, as a `StorageError`.
5. `read_parquet_file` logs the ratio between the bytes the budget charged and the
   heap actually decoded, so the ceiling can be re-derived against the quantity
   that runs out.

Tests: `eviction_does_not_lower_an_entity_version` and
`conditional_write_resolves_version_without_loading_the_tenant`. The first fails
with `left: 0, right: 2` against the old eviction behaviour.

## What did not land, and why

**The three write paths still count.** `ingest`, `ingest_batch` and the pipeline
append each bump `entity_versions` by one and report that number, while
persisting whatever `version` the caller put on the event. Memory and disk
therefore disagree for any entity written unconditionally: after a restart, the
mark reflects the persisted versions and not the count.

Making them agree means having `ingest` assign and persist the version it
reports. That is a write-semantics change on a path also used by
`embedded/core.rs` peer replication, `prime/sync.rs` and `prime/import_export.rs`,
where renumbering a caller-supplied version would be wrong. It needs an owner
decision about which of those paths may renumber, so it is a separate task rather
than part of this fix.

The budget's ceiling is also unchanged. Re-deriving it needs the measurement that
item 5 above now produces; inventing a number would replace a mis-metered limit
with a guessed one.

## Limits

- The eviction corruption is now demonstrated:
  `eviction_does_not_lower_an_entity_version` reports version 0 for an entity
  with two events on disk when the old eviction behaviour is restored. What is
  still not demonstrated is a full end-to-end duplicate-version write.
- The 108,145 / 18,144 / 131.6 MB figures come from the incident analysis, not
  from a measurement taken for this document. No run against that archive has
  been done, so the memory fix is verified by construction and by test, not
  against the archive that OOM'd.
- The `serde_json::Value` expansion factor of five to twenty is a general
  characteristic of the type, not a measurement of our payloads. The direction is
  certain; the multiplier for our event shape is not.
- Whether TTL retention reaches the same counting rebuild was not established.
  Cache eviction reaches it, which was sufficient to classify the defect.
