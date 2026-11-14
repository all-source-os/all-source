# Core memory bound: brief

Date: 2026-11-11.

## Problem

AllSource Core runs on one Fly machine with 16 GB of RAM because its resident memory is not
bounded by anything it controls. The 16 GB machine is most of the $144.87 monthly Fly bill;
the 8 GB machine would bring it to $94.11.

Measured in production on 2026-11-11 at 18:17Z, about 90 s after a deploy of `f4b53930`:

| Measure | Bytes |
|---|---|
| Process resident (RSS) | 6,292,746,240 |
| Live heap (`GLOBAL.allocated()`) | 5,574,260,718 |
| Allocator retained (RSS − live) | 718,485,522 |
| Resident event structure, sampled | 3,634,360,625 for 229,258 events (~15.9 KB/event) |
| Indexes (entity, type, id, entity_versions) | ~61,600,000 |
| Live heap no gauge attributes | ~1,878,000,000 |
| Cache budget counter `allsource_core_cache_bytes` | 950,872,362 against a 2,147,483,648 budget |

One tenant (Longhand's control plane, 201,806 events) took RSS from 253 MB to 5.77 GB when it
hydrated, in 14.2 s, on the first query after boot.

The causes, read from the code at `f4b53930`:

1. **The budget counts the wrong unit.** Each event is charged `256 + compact JSON length`
   (`domain/entities/event.rs:307-316`), but its payload and metadata are held as a
   `serde_json::Value` tree, about 3.8× larger. The counter reads 0.95 GB while the events
   occupy 3.63 GB, so eviction never fires. The logs show no refused eviction.
2. **Bytes with no owner.** About 1.88 GB of live heap belongs to no counted structure. The
   suspects are the `entity_snapshots` projection (a merged copy of every entity's payload,
   never evicted, with no production reader of `get_snapshot`), pending write batches, the
   broadcast ring and the feed index. None is in `resident_structures()`.
3. **The tenant is the unit of everything.** Any query hydrates the tenant's whole history into
   one `Vec<Event>` (`storage.rs:1505-1516`) and eviction removes a whole tenant, so memory grows
   with history, not with the working set.

## Goal

Core's resident memory is bounded by a budget it measures in the same unit it spends, every byte
has a counted owner, and reclaim cannot be refused forever. With that true, Core runs on the
8 GB machine with headroom for compaction and a second large tenant, queries return
byte-identical results to today, and cold reads are no slower than the 14.2 s whole-tenant
hydration they replace.

## Approaches

Three designs were scored by the research run of 2026-11-11 (sources and versions verified
against docs.rs and project repositories):

- **Chosen: staged, Arrow-native endpoint reached through a small first step.**
  - S0: attribute the live heap by allocation site, with `tikv-jemallocator` 0.7 and
    `jemalloc_pprof` 0.9 behind a `heap-profile` feature.
  - S1: one ledger that owns every byte, plus removal of `entity_snapshots`.
  - S2: payloads and metadata held as `serde_json` `Box<RawValue>` (canonical compact JSON
    written once at ingest).
  - S3: Parquet row groups as the unit of residency, with DataFusion's `MemoryPool` as the
    only ledger and eviction of flushed groups only.
  - S4: EventQL runs over resident batches instead of cloning the tenant.
- **Lost: keep `Vec<Event>` and only change the representation.** S2 alone removes about 2.6 GB
  but leaves the tenant as the eviction unit, so the bound breaks again as history grows.
- **Lost: a weighted cache crate (moka, quick_cache, foyer) as the residency layer.** Each brings
  its own budget beside the ledger, which is two counts that can drift again; foyer's disk tier
  duplicates Parquet and the WAL.

Each stage is refined with autoresearch-rs: a frozen contract, one hypothesis per candidate, and a
deterministic objective (live heap bytes after replaying a fixed fixture) gated on byte-identical
query output.

## Constraints

- Query results, API responses and on-disk Parquet stay byte-identical; the replay gate proves it.
- No deploy downtime beyond today's checkpoint restart, and no change to the volume format that a
  rollback to `f4b53930` cannot read.
- Core runs as uid 1000 on Alpine/musl; any allocator or profiler must work there or stay behind a
  feature that release builds do not enable.
- Every gate and evaluator finishes within 5 minutes.
- No customer data is committed: the replay fixture is synthetic, generated from shape statistics
  (event-type counts and payload-size distribution), never from event content.
- Dependency changes land as ordinary commits before an autoresearch baseline; candidates cannot
  touch `Cargo.toml` or `Cargo.lock`.
- Changed at Spec (2026-11-12). These are the design's three numbered constraint changes, in the
  same order. Byte-identical output (design I5) excludes the responses they alter.
  1. `entity_snapshots` leaves server mode. It disappears from `GET /api/v1/projections` and from
     its admin routes.
  2. `POST /api/v1/geospatial/query` becomes tenant-scoped, and returns at most 10,000 results.
  3. When two or more archive files fail in one feed read, the reported error is now the first by
     file path. Before, it depended on hash order.
- Also changed at Spec:
  - "Every byte has a counted owner" holds within a 10% attribution tolerance (design I2).
  - "Reclaim cannot be refused forever" moves out of S1. S1 counts and cuts memory without
    changing eviction. Routine eviction gets its own specification, which starts from the design's
    audit list. The path to the 8 GB machine is expected to need no routine eviction, because S1's
    cut and S2's canonical text should fit the whole resident set.

## Unknowns

| Unknown | Tier | What moves it up |
|---|---|---|
| Owners of the unattributed 1.88 GB | water | S0 heap profile attributing at least 90% of live heap |
| Bytes `entity_snapshots` holds, and whether any consumer outside Core reads it | sand | Profile delta with it unregistered; grep of desktop, control-plane and SDK callers |
| Resident size of a `RawValue` event against JSON text | sand | Byte count on the fixture after S2 |
| Payload size distribution per event type (uniform or heavy-tailed) | sand | Shape statistics from the store, recorded as numbers only |
| DataFusion `MemoryPool` governing a resident row store, not query operators | sand | S3 rehearsal at 0.5 GiB and 2 GiB budgets: eviction fires, no query fails |
| Parquet page-index API on the locked arrow/parquet 59.3 | sand | Read the 59.3 docs and compile against it |
| Per-candidate build time inside the 5-minute evaluator budget | sand | A warm, pinned shared target directory measured on the first baseline |
