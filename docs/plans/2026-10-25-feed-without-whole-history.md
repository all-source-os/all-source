# Event feed without whole-tenant history

The event feed and read-model commits load a tenant's entire history on every
call (`store_feed.rs:105`), so their cost grows with the tenant, not with what
they return. For Longhand's control-plane tenant (~585 MB decoded, +14 MB a
day) the strict 768 MiB limit is reached in about 16 days
([outage report](../reports/2026-10-25-control-plane-tenant-outage.html)). This
plan removes that dependency in phases that each ship on their own.

Chosen after three independent designs (persisted ordinals, content cursor,
per-prefix stream) were judged against the code: one append-only feed log per
tenant, with a narrow-column index first as the stopgap and later as the
backfill.

## Facts the design rests on

- Positions are ordinals over a per-process, random-epoch index rebuilt from
  the whole loaded history (`store_feed.rs:12-24`, `:57-70`). A Core restart
  changes the epoch and the projector rebuilds from position 0.
- Ingest order is entity lock → WAL append → `events.write()` → parquet batch →
  feed index (`store.rs:678/699/732/736`), so WAL sequence order differs from
  feed order. `wal.rs:260-263` drops the sequence lock before the file lock.
- Boot WAL recovery re-buffers every recovered event, including ones already
  flushed (`store.rs:534-543`), and truncates outside `checkpoint()`
  (`store.rs:555-557`).
- Compaction excludes large snapshots (`compaction.rs:848`), groups candidates
  by file creation time (`:882`) and sorts rows by timestamp (`:1122`), so any
  ordering tied to file layout must survive it.
- Longhand's projector pages with prefixes
  `team_queue.work_item.,team_queue.retention_set,classification_queue.,auth.member.,auth.org.`
  (`shared_projection.rs:327`); its entity feed passes `entity_id` and no
  prefixes (`:242`). It resets only on an epoch change and requires positions
  to increase strictly, not to be dense.

## Phases

### P1: narrow feed index (removes the 16-day limit)

Build the feed index from a projected archive scan of `(id, event_type,
entity_id)` in path order, deduplicated by id, plus pending batches and live
ingest. Each id keeps a locator: `(file, row_group, row)` or pending. A page
reads payloads only for the rows it returns, charging the budget for those
rows only. On a file that disappears mid-page (compaction swap), rescan the
narrow columns of the new files and remap ids; positions do not move.

- Head and read-model commits stop decoding history. A cold build reads about
  10 MB of narrow columns for this tenant instead of 585 MB.
- Epoch stays per process: no contract change, no Longhand change.
- Kill switch: `ALLSOURCE_FEED_NARROW_INDEX=0` restores the hydrating path.
- Failing-first tests: `head_probe_under_1MiB_strict_budget_ok`,
  `commit_read_model_does_not_hydrate`, `page_payload_charge_le_returned_rows`,
  `compaction_mid_process_positions_unchanged`.
- Confidence: sand. Moves to rock once the eviction path
  (`store.rs:2096-2132`) and row-group skipping on existing snapshot files are
  verified.

### P2: durable per-tenant feed log

An append-only log of non-model events per enabled tenant (allowlist), with a
narrow index for filtering. The reservation moves inside the WAL file lock;
records are published only after a group-commit fsync; the log is fsynced
before `remove_sealed` and before the boot-recovery truncate; replay appends
missing ids in WAL order. Backfill uses the P1 reader. Epoch persists, so a
deploy no longer forces the projector to rebuild.

- Confidence: sand. Rock once the crash matrix passes
  (`boot_truncate_after_recovery_fsyncs_log_first`,
  `crash_before_fsync_never_served`, `recover_order_differs_no_reorder`).

### P3: default on

Retention floor for TTL tenants (epoch bump when it advances), follower
behaviour, and charging read-model journal restores only for row groups that
can hold the entity (`storage_retained_entity.rs:44-46` charges every row
group's rows today). Confidence: water for replication; spike leader-only
versus replicated first.

## Not verified

- Per-event-type bytes and model-event share for the production tenant.
- Whether Fly deploys reach `flush_on_shutdown` (`storage.rs:1944`).
- The file order of `recover_entries`.
- Eviction preconditions (`store.rs:2096-2132`).
