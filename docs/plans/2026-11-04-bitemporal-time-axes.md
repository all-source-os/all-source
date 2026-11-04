# Bitemporal design draft: second time axis for AllSource

Status: accepted 2026-11-04 with the recommended answers to section 8 (questions 1, 2, 4, 7, 9 and 10: keep `timestamp` as record time and add `valid_from`/`valid_to`; unqualified `as_of` stays record time; a correction is a plain event; retention, archive and partitioning stay on record time; checkpoint `FORMAT` stays 1 with `serde(default)`). Questions 5 and 6 are slice 11's spike. Inventory read on origin/main at `089798e`. Paths below are relative to `apps/core/src/` unless marked otherwise.

## 1. Problem

Agent memory gets corrected after the fact. A fact recorded on 2026-03-02 ("Acme's contract renews in June") is later found to be wrong. The correction is filed on 2026-10-15 and says the renewal is in September, effective from 2026-03-02. Two questions then have different answers:

- What did the agent believe at 2026-06-01? The old fact. This is the recorded-time question.
- What was true on 2026-06-01, given everything we know today? The corrected fact, so the renewal is in September, not June. This is the valid-time question.

AllSource has one time axis, so it can answer only the first. A correction is stamped with the time it was filed, so the store cannot place it in the world at the time it applies. Prime's `get_node_as_of` and `neighbors_as_of` answer "what did we know at T" by accident of the single axis, and `RecallContextQuery.as_of` is declared but never read (`prime/recall/api.rs:40`).

## 2. Current model: one axis, `event.timestamp`

- `domain/entities/event.rs:29-41`: `Event` has one time field, `timestamp: DateTime<Utc>`. No valid-time or recorded-time field exists.
- `domain/entities/event.rs:89,109,145`: `timestamp = Utc::now()` at construction. For local writes this is record time. Replicated events keep the origin value (`store.rs:1024`), so on followers it is not purely record time.
- `application/dto/event_dto.rs:7-17`: `IngestEventRequest` has no timestamp field. Callers cannot set event time. Every write gets server now.
- `store.rs:2614-2615` (`filter_entries`) and `store.rs:2654-2655` (`apply_filters`): `as_of` drops events where `ts > as_of`, comparing `event.timestamp` / `IndexEntry.timestamp`. The index and the full-scan path must stay identical; `store.rs:6350-6398` pins both.
- `infrastructure/persistence/wal.rs:119,132`: `WALEntry.wal_timestamp = Utc::now()` is the only real record time on disk. `wal.rs:142` includes it in the checksum. `wal.rs:683-686` drops it on read.
- `store.rs:654-668`: per-entity `version` (i64) is the only persisted per-entity order. It is an OCC key, not a global record clock.
- `application/dto/event_dto.rs:21-27`: ingest response returns `timestamp` (server now). This is the only record-time anchor writers see.

Consequence: today `as_of` means "what the store knew at T" for local writes. "What was true at T" cannot be expressed at all.

Note on the source ask: the `as_of` comparison is at `store.rs:2614` and `2654`. The range `2368-2409` cited in the request is the `query_window_scoped` doc and lazy-load block (`store.rs:2355-2412`), not the comparison.

## 3. Inventory

Each row: location, symbol, kind, what it uses today, what two axes need. "Verified" means the lane agent read the cited code. Items marked `unverified` were grepped or not read. Critic additions are merged into their lane. Disputed line numbers are corrected where the critic showed drift.

### 3.1 Core store and query path (`apps/core/src`)

| Location | Symbol | Kind | Uses today | Needs under two axes |
|---|---|---|---|---|
| `store.rs:2614-2615` | `filter_entries` (def at 2609) | filter | `event.timestamp` via `IndexEntry.timestamp`; `as_of`, `since`, `until` inclusive | one predicate per axis; request names axis; index entry carries second timestamp |
| `store.rs:2654-2655` | `apply_filters` (def at 2637) | filter | same three bounds on full-scan path; comment 2650-2653 says index timestamp equals event timestamp | same dual predicate; comment becomes false once index carries two axes |
| `store.rs:2486` (body; `query_window` def 2340) | `query_window` ordering | order | sort `(event.timestamp, version, scan pos)`; descending reverses | choose default order axis; `(valid, record)` tie-break; cross-entity version ties arbitrary |
| `store.rs:2587` | `query_window_from_archive` ordering | order | same key on archive-loaded entity events; comment 2534 says both paths agree | mirror resident ordering exactly |
| `store.rs:2721`, body 2726 | `reconstruct_state` / `entity_state` | fold | `as_of` forwarded to query; fold = payload key overwrite in timestamp-asc order; `last_updated = events.last().timestamp`; echoes `as_of` | two folds: valid-as-of-T and recorded-as-of-T; fold order follows chosen axis |
| `store.rs:2732`, body 2743 | `entity_state` request construction | fold | `QueryEventsRequest` with `as_of`, no since/until, no limit | axis-aware `as_of` fields |
| `store.rs:2994` | tenant entity state call | fold | bare `Option<DateTime>` | axis-aware |
| `store.rs:1243` (sort 1255-1260) | `register_projection_with_backfill` | order | replay sorted by `(timestamp, version)`; LWW projections need chronological replay | valid-time projections need valid order; record-time projections need record order |
| `store.rs:1423` | compaction re-index | index | rebuilds `(id, entity, type, event.timestamp, offset)` after `retain` shifts offsets | record-order key must survive compaction; Vec position is not a stable record id |
| `store.rs:2126`, 2223 | eviction re-index | index | `index_event(..., event.timestamp, offset)`; 8 call sites pass `event.timestamp` (457, 703, 849, 985, 1037, 1433, 2136, 2223) | pass second timestamp through `index_event` at all 8 sites |
| `store.rs:623` (body 655-668) | `ingest_with_expected_version_cancellable` | persist | record-order stamp = `entity_versions[entity]+1` under DashMap entry lock, before WAL append | global or per-tenant record stamp; per-entity version cannot order across entities; do not repurpose it as transaction time |
| `store.rs:779` | `ingest` | persist | same version stamp; WAL append; `event.timestamp` passed through unchanged | valid time set by caller here; record time assigned here or in WAL |
| `store.rs:933` (body 956-962) | `ingest_batch` | persist | per-entity version in batch loop | `unverified`: whether batch shares one wall-clock value; batch needs consistent record stamp |
| `store.rs:1024`, adopt 1121-1123 | `ingest_replicated` | wire | keeps origin `event.timestamp`; `offset = events.len()` is local position; adopts leader version | replicated events need leader record time plus local arrival; origin valid time preserved; follower record order = arrival order, not leader's |
| `store.rs:445` | boot WAL recovery re-index | persist | `index_event(event.timestamp, offset)` in WAL sequence order | keep WAL sequence as durable record order when rebuilding |
| `store.rs:2262` | `select_window` (generic) | order | caller-supplied comparator | comparator must carry axis; no time semantics inside |
| `store.rs:2941-2962` | `stats` / `stats_for_tenant` oldest/newest | fold | min/max `event.timestamp` | per-axis range |
| `infrastructure/persistence/system_store.rs:497-518` | system store window | filter | `e.timestamp()` inclusive start/end; sort by event timestamp at 516; truncate 518 (enclosing fn not read) | second-axis bounds plus sort key choice |
| `application/use_cases/query_events.rs:34` | `execute` as_of branch | filter | `as_of` honoured only on entity path (`find_by_entity_as_of`); silently ignored on event_type and time-range paths | consistent `as_of` across strategies or reject on non-entity paths |
| `application/use_cases/query_events.rs:49-66` | since/until post-filter | filter | `occurred_after` (strict >), `occurred_before` (strict <); store uses inclusive; test `api.rs:2946-2950` expects inclusive | one boundary convention for use case and store |
| `query_events.rs` time-range branch | strategy requirement | filter | requires both since AND until, else `InvalidInput` | per-axis pairs |
| `domain/entities/event.rs:310-320` | `occurred_between` / `occurred_before` / `occurred_after` (def 314 for before) | filter | strict < and > on `event.timestamp`; source of query_events boundary mismatch | per-axis helpers matching store convention |
| `domain/repositories/event_repository.rs:30,38` | `find_by_time_range` / `find_by_entity_as_of` | api | single `DateTime`; only test mocks found (`query_events.rs:147,161`, `ingest_event.rs:157,171`, `semantic_search.rs:370,384,693,707`) | `unverified`: production impl; trait may be dead on HTTP path (api.rs calls `store.query`); verify before changing |
| `application/dto/common_dto.rs:106-137` (has_time_filter at 137) | `QueryFilter` as_of/since/until builders | api | `event.timestamp` | second axis field; `has_time_filter` must count it or fast path drops it |
| `application/dto/filter_dto.rs:420,454-455,226-227` | `EventFilter.as_of` doc "Time-travel", `at_time()`, `in_date_range()` | api | `event.timestamp` per doc | name axis; `at_time()` will read as valid time to callers and silently bind to record time |
| `application/dto/event_dto.rs:41-58` | `QueryEventsRequest` as_of/since/until | api | three bare `Option<DateTime>`, one axis, no selector; derives `Deserialize`, so body controls axis (see store.rs:2358 comment) | axis selector or `valid_as_of`/`recorded_as_of` pairs; since/until per axis |
| `application/use_cases/ingest_event.rs:171-183`, `semantic_search.rs:384-396,707-719` | test-double `find_by_entity_as_of` using `occurred_before` | api | `event.timestamp` | mocks must mirror trait signature when axis added |
| `application/services/replay.rs:69,144-154` | `ReplayRequest.from/to_timestamp` | api | `to` maps to both `as_of` and `until`; `from` to `since`; all event axis | explicit axis; one value setting both `as_of` and `until` must be split |
| `application/services/analytics.rs:217,249,272,278` | frequency buckets | fold | `until` defaults to `Utc::now`; buckets by `event.timestamp`; `fill_time_gaps`; `time_range` reported | bucket axis choice; per-axis time_range |
| `application/services/event_coordinator.rs:101,122,130` | entity snapshot | snapshot | `as_of` forwarded to trait; echoes `as_of` | axis-typed snapshot; depends on unwired trait |
| `infrastructure/persistence/index.rs:8-11,41,49-53` | `IndexEntry { event_id, offset, timestamp }`, `index_event` | index | one time per entry; in-memory only (`unverified` whether ever persisted) | second timestamp per entry; widening is in-memory, so no on-disk migration |
| `infrastructure/persistence/simd_filter.rs:92-96,200,295-372` | `FilterPredicate::TimestampAfter/Before/Between` | filter | `e.timestamp()`; no production caller outside tests (556-791) | dead today; axis parameter needed if wired; must not repurpose variant silently |
| `infrastructure/web/api.rs:878-905` (`EntityStateParams.as_of`) | `get_entity_state` | api | single `as_of` query param; ETag path near 905 | second param; cache key must include both axes |
| `infrastructure/web/api.rs:2056-2095` | sync pull handler | api | `since = min(version_vector.physical_ms)` to `store.query`; HLC synthesized from `event.timestamp` at 2080-2095 | sync cursor must key on record axis; HLC not on `Event` |
| `infrastructure/web/api.rs:2884` | time-window contract test | test | asserts since=at(2) to ids[2..4], until=at(1) to ids[0..1], as_of=at(1) to ids[0..1]; inclusive | extend per axis, keep inclusive |
| `infrastructure/web/retained_query.rs:20-29` | retained strict read gate | filter | rejects `since`/`until`/`as_of` with `InvalidInput`; fail-closed | reject new axis params too, or allow explicitly |
| `infrastructure/resp/commands.rs:164,225-252,273` | XADD id / XRANGE | wire | stream id = `event.timestamp` ms + `-0`; same-ms collide; XRANGE maps ms to since/until; exposes `timestamp` at 273 | id from record axis or WAL sequence; default must not change silently |
| `infrastructure/repositories/event_sourced_config_repository.rs:219` | `get_at` | fold | iterates in stored order, `break` on `event.timestamp > as_of`; assumes timestamp-ordered input | break-on-first is wrong when a later-recorded event has earlier timestamp |
| `embedded/core.rs:378,407-412,762-780,412-416` | peer since / HLC stamping | wire | since = peer HLC `physical_ms+1`, inclusive; events_to_sync uses original timestamp; HLC on `ReplicatedEvent` wrapper only (207, 251, 427) | replication cursor on record axis; decide HLC vs WAL sequence as record clock; same-ms collisions already noted in code |

### 3.2 Event model and durable storage

| Location | Symbol | Kind | Uses today | Needs under two axes |
|---|---|---|---|---|
| `event.rs:29-41` | `Event` struct | api/persist | single `timestamp`; `#[serde(default)]` only on `tenant_id` (line 33) | new fields need `serde(default)` so existing WAL, Parquet and checkpoint rows deserialize |
| `event.rs:152,175` | `Event::reconstruct` / `reconstruct_from_strings` | api | positional args; callers at `store.rs:5532`, `storage.rs:1163,1983,2376`, `projection.rs:508,528`, `websocket.rs`, `ingest_event.rs:114`, `semantic_search.rs:650` | compile-time breakage on any field addition; cfg(test) manual Clone at 48-73 must match |
| `event.rs:89,109,145` / `from_strings` (128-149) | constructors | wire | `Utc::now()`; used by `api.rs:261,377,425,480` and `ingest_event.rs:31,70` | valid time from caller DTO; record time fixed at ingest, not construction |
| `infrastructure/web/api.rs:255-290` | ingest handler | api | server clock only; body not fully read | `unverified` body; add optional valid time on request |
| `infrastructure/persistence/wal.rs:113-142` | `WALEntry { sequence, wal_timestamp, event, checksum }` | persist | `wal_timestamp = Utc::now()` at construction (132); sequence monotonic (268-272) | this is transaction time; `wal.rs:142` checksum string format change invalidates existing CRCs |
| `wal.rs:276,280` | write-lock order | order | `wal_timestamp` set before write lock, so not monotonic vs sequence under concurrency | sequence is the order authority; never order by `wal_timestamp` |
| `wal.rs:664-686`, 472, 488, 566, 462 | `recover_entries` / `recover` / `recover_for_append` / `read_all_entries` | persist | `wal_timestamp` dropped on read; returns `(u64, Event)` | carry record time out of replay; else boot loses transaction time |
| `wal.rs:843` | feed broadcast | wire | `WALEntry` broadcast keeps `wal_timestamp` | none; good carrier |
| `wal.rs:20-24,455,475,540,559,573` | `WalSegmentStamp` mtime; segment ordering | order | mtime is wall clock; sort by path/sequence | mtime must not become recorded_at (changes on copy) |
| `storage.rs:221` | Parquet Arrow schema | persist | `timestamp[us]` column (index 4 per schema order in lane); no recorded column; no tenant column (path is source of truth, 1101-1103) | append `valid_*` and `recorded_at` at end (index 7+); existing constants must hold |
| `storage.rs:57-66` | `ENTITY_ID_COLUMN=2`, `PAYLOAD_COLUMN=3`, `METADATA_COLUMN=5`, `VERSION_COLUMN=6` | persist | hand constants; used at 1748 and 1405-1428 | do not insert mid-schema |
| `storage.rs:1105-1172` | `record_batch_to_events` | persist | positional `column(0..6)` downcast; no schema-name check | new reader must look up by name with null fallback |
| `storage.rs:45` | `decoded_text_bytes` via `schema().index_of(name)` | persist | only name-based lookup; precedent for tolerant reads | pattern to copy |
| `storage.rs:827,855,864` | `events_to_record_batch` | persist | writes `timestamp_micros()`; version as u64 | two builders; microsecond precision ties |
| `storage.rs:939-967` | `load_events_from_file_with_budget` | persist | Parquet read, no schema validation (960); budget charged from metadata | budget changes for new files (more columns) |
| `storage.rs:417-421` vs `614-619`, `652-657`, `1609` | partition key | persist | comment says wall clock at flush; code anchors by min `event.timestamp`; migration groups by `event.timestamp` | doc disagrees with code; axis choice decides directory layout and retention |
| `storage.rs:1354-1400` | `record_flushed_versions` | persist | per-entity max version folded from flushed events | `unverified` body; version is the only per-entity order on disk |
| `version_manifest.rs:29,73-88` | version manifest | persist | `_version_manifest.json`; keyed by entity; `try_from(event.version)` rejects negatives | no axis change; correction events get higher version |
| `system_checkpoint.rs:29,46,50,63-69` | `SystemCheckpoint` | persist | verbatim `Vec<(u64, Event)>`; `covered_seq` = WAL seq; `created_at` = wall clock; `FORMAT=1`; footer CRC | new Event fields flow in; `serde(default)` required or old binaries skip checkpoint and replay whole WAL |
| `system_checkpoint.rs:35,100` | retention / listing | persist | `RETAINED=2`; ordered by `covered_seq` hex | no change |
| `compaction.rs:1041-1042` | retention TTL cutoff | filter | `partition(|e| e.timestamp < cutoff)`, cutoff = `Utc::now() - ttl`; destructive | axis choice decides what survives; needs migration plan |
| `compaction.rs:1067-1076,1122` | snapshot compaction | index | `sort_by_key(e.timestamp)`; from/to = min/max; filename embeds valid range | sort key `(valid, record, version)`; filename range names valid range only |
| `compaction.rs:520,882` | `sort_by_key(f.created)` | order | filesystem metadata, not event time | third time source; not portable across copy |
| `cold_tier.rs:56,123` | `ArchiveTarget::archive(from, to, events)` | index | from/to = event timestamp; file named by valid time | two ranges per file or a manifest; S3 key (`cold_tier_s3.rs:78-81`) orphans old objects if format changes |
| `cold_tier.rs:188,227,257` | archive cutoffs `Utc::now()` | filter | wall clock | `unverified` (grep only) |
| `archive_output.rs:97` | Parquet KV metadata | persist | unused; no schema version stamp | cheap place for format marker |
| `storage.rs:1627,740,506` | file name stamps | persist | `Utc::now()` in names | `unverified` whether pruning reads names; should not encode an axis pruning depends on |

### 3.3 Prime agent memory (`apps/core/src/prime/**`, `apps/prime-mcp`, examples, bench)

| Location | Symbol | Kind | Uses today | Needs under two axes |
|---|---|---|---|---|
| `prime/facade.rs:2228` | `get_node_as_of` | fold | `core.query` entity until(as_of); `created_at` first ts; `updated_at` last ts; deleted gives None | two reads: believed-at-T (recorded <= T) and true-at-T (valid <= T, recorded <= now) |
| `prime/facade.rs:2330` | `neighbors_as_of` | fold | edge CREATED/DELETED pairs by payload.id until(as_of); targets via `get_node_as_of`; no tenant filter | same split for edges; a later correction deleting an edge must hide it at valid T |
| `prime/facade.rs:2047` | `diff(from, to)` | filter | since/until on `event.timestamp`, prefix `prime.`; counts by type | name the window: valid vs recorded |
| `prime/facade.rs:2091` | `timeline(entity, from, to)` | filter | optional since/until on `event.timestamp`; single timestamp per entry | axis selector; entry carries both |
| `prime/facade.rs:1881` / `prime/types.rs:145` | `history(entity_id)` | api | all events, no time args; `HistoryEntry.timestamp = e.timestamp` | entry carries `valid_at` and `recorded_at`; sort key choice |
| `prime/facade.rs:1902,1945-1954` | `observations` | fold | `observed_at = e.timestamp` (ingest time, not valid); source_priority / specificity from metadata | `observed_at` must be a named choice: valid for LastWrite of corrections; recorded for audit |
| `prime/projections/declarative.rs:314` | `fold_with_provenance` / LastWrite | fold | LastWrite and provenance use `Observation.observed_at`; tie-break by `source_event_id` | `unverified` LastWrite body (150-250); policy must choose axis; MergeArray has no provenance (257-265) |
| `prime/projections/node_state.rs:126,164-199` | `NodeStateProjection::process` | fold | `timestamp = event.timestamp` into `created_at`/`updated_at`; UPDATED deep-merge; UPDATED silent no-op if node missing; DELETED tombstone | out-of-order sync replay relies on arrival order; `updated_at` moves with event time |
| `prime/projections/node_state.rs:227-235` | snapshot restore | persist | single `updated_at` | serialize both axes if node carries both |
| `prime/recall/api.rs:264` | `candidate_nodes.sort_by_key(Reverse(updated_at))` then truncate(20) | order | `NodeEntry.updated_at` (event time) | choose recency axis: latest known vs latest true; changes top-20 set |
| `prime/recall/api.rs:40,57` | `RecallContextQuery.as_of` | api | declared, default None; no reader under `prime/recall`, vectors, projections | implement via axis-aware `get_node_as_of`/`neighbors_as_of`, or delete; today a caller's `as_of` is silently ignored |
| `prime/facade.rs:1418,1464,1520,1529,1574` | `recall` recency | order | `now = Utc::now()`; `recency_score(updated_at, now)`; type-scan sort by `updated_at` at 1520 | recall has no `as_of` param; needs declared axis and as-of clock; `recency_score` (2679-2683) uses event hours |
| `prime/facade.rs:708,1735` | `created_at: Utc::now()` | persist | wall clock at write | node_state uses event time for same field; the two diverge on replay |
| `prime/recall/types.rs:235,244` | `stored_at: Utc::now()` | persist | only genuinely record-time field in prime | name it as record time; not used for as_of |
| `prime/facade.rs:1976-2036` | `define_projection` / `load_projection_defs` | order | latest by `event.timestamp`, ties by WAL order (>=) at 2036 | latest by which axis; a def recorded later but valid earlier wins wrongly if axis changes |
| `prime/facade.rs:480-510` | `update_node` (correction path) | write | payload `{properties}`; correction = NODE_UPDATED overwrite; only time is `event.timestamp` | `valid_at` in payload or metadata; without it a correction cannot be placed in valid time |
| `prime/facade.rs:437` | `add_node` | write | payload `{id, node_type, properties}`; no `valid_at` | `valid_at` for facts true before recording |
| `prime/facade.rs:1376,518` | `forget` / `delete_node` | write | `delete_vector` then `delete_node` (EDGE_DELETED + NODE_DELETED); no reason, no `valid_to` | forget is a record-time event; "no longer true from T" needs `valid_to` on a fact, not a delete |
| `prime/facade.rs:913` | `compact` doc | write/doc | promises state reconstructable via `as_of`; soft-deletes sources | doc must name axis |
| `prime/facade.rs:2139` | `conversation_history` | filter | full scan of `prime.` then in-memory filter on `metadata.conversation_id` | ordering axis; cost unchanged |
| `prime/hosted.rs:305,567,730` | hosted history / event construction | api/write | `Utc::now()` at 567 and 730; raw `core.query`, no time args | mirror embedded; `unverified` whether valid_at passes through same ingest |
| `prime/projections/relevance.rs:89,119-130` | `relevance_at` / `touch` | fold | decay clock is `Utc::now()` (89); `last_accessed` = event ts | decide whether access is record time; decay is neither axis |
| `apps/prime-mcp/src/tools.rs:1390` | `prime_context` ctx_query | wire | `as_of: None` hard-coded; no as_of in any MCP schema (grep) | expose as_of and axis; MCP cannot time-travel recall today |
| `apps/prime-mcp/src/tools.rs:758` | `drafted_at: Utc::now()` | persist | wall clock in draft payload | confirm not used as valid time |
| `apps/core/examples/prime_recall.rs:200` | time-travel demo | doc | `t_before_update` wall clock vs `get_node_as_of`; `diff` | name axis once both exist |
| `tooling/recall-bench/src/evaluate_prime.rs:495` | temporal arm | doc/order | comment claims `history()`+`as_of()`; code uses `recall()` and sorts by caller field `session_date` | stale comment; `session_date` is de facto caller-supplied valid time, outside the store |
| `tooling/recall-bench/src/evaluate.rs:364,393` | temporal-aware retrieval | order | sort by a date in hit tuple (source unverified) | benchmark arm for each axis |
| `prime/types.rs:335` | projection def doc | doc | "latest-by-timestamp wins on replay" | name the axis |

### 3.4 Wire surfaces outside Core

| Location | Symbol | Kind | Uses today | Needs under two axes |
|---|---|---|---|---|
| `sdks/rust/src/latest.rs:104,137-138,158` | `EntityStateParams.as_of` (pub); response echo | api | sent as `as_of`; Core folds event timestamp | second selector; echo both; caller asking truth-at-T gets record answer with no error |
| `sdks/rust/src/latest.rs:1391,1400,1424` | test `entity_state_sends_history_and_as_of_only_when_asked` | test | pins single `as_of` key | new test per key |
| `sdks/rust/src/types.rs:79,88,122,372` | `EventRecord.timestamp`, `version` | order | ordering key `(timestamp, version)` | ordering names the axis |
| `sdks/rust/src/types.rs` (grep) | event query builder | api | no `as_of`; `since`/`until` only | `unverified` builder body; Rust SDK cannot time-travel events |
| `sdks/typescript/src/types.ts:70,86-89,122,133` | `Event.timestamp` doc "Server-assigned"; `EventQuery.since/until`; `EntityStateOptions.asOf` | api | single timestamp; no client-settable fact time | `valid_time` input and output; per-axis bounds; additive `asOf` sibling |
| `sdks/typescript/src/client.ts:215-228,270-281` | ingest ack; `entityState` | wire | ack returns id/timestamp/version; `as_of: options.asOf` | echo valid time if writers set it; `withQuery` unknown-key behaviour unverified |
| `sdks/python-client/src/allsource_client/_protocol.py:230-249,273-279` | `events_query_request`, `entity_state_request` | wire | `as_of` str verbatim | new kwarg, additive |
| `sdks/python-client/src/allsource_client/client.py:147,233,239` and `async_client.py:155-157,172-174,243-249` | client methods | api | forwards `as_of` | mirror on both sync and async |
| `sdks/python-client/src/allsource_client/types.py:106,118,157,170` | `Event.timestamp`, `EntityState.as_of` | api | parsed from data | `valid_time` field; dataclass addition additive if not positional |
| `sdks/go/latest_state.go:22,36,47-49` | `EntityStateOptions.AsOf string` | api | empty string omits param; zero value means now | second field; "as of time zero" inexpressible |
| `sdks/go/types.go:16-17,26-28` | `Event.Timestamp`, `Version` | api | record data | additive field |
| `sdks/contract/fixtures/entity_state.json:19` | `"as_of": null` | api | cross-SDK wire pin | new fixtures per axis; fixture change = contract version bump |
| `sdks/contract/fixtures/event_feed.json:25` | `"timestamp"` | api | single key in feed | `valid_time` key; SDK strict parsers may reject unknown keys (`unverified`) |
| `sdks/contract/fixtures/view_define.json:13-44`, `view_get.json:18-30` | view DSL `{day:timestamp}`, `max last_seen_at` | fold | DSL folds on event timestamp | DSL must choose axis per field; silent switch if default changes |
| `apps/query-service/.../event_controller.ex:106,121-129` | `@core_compat_filters` whitelist; `query_core_compat` | wire | forwards whitelist verbatim; unlisted params dropped by `maybe_put` | add axis param to whitelist; public compat surface |
| `apps/query-service/.../rust_core_client.ex:1439-1454` | `flatten_predicate` for since/until/as_of | filter | `as_of` kept only for `:eq`; other operators hit catch-all; range predicate becomes unfiltered | explicit error for unsupported op on time fields |
| `apps/query-service/.../replay_analysis.ex:50-61,122-131,194-195` | replay cutoff `as_of: cutoff` | fold | Core as_of on event timestamp; first/last from event ts | decide which axis a replay cutoff means |
| `apps/query-service/.../replay_analysis_store.ex:16-25` | `perform(tenant, at)` | wire | UTC-only RFC3339 validated then forwarded | second time param validated the same way |
| `apps/query-service/.../tenant_projections.ex:522,584,744-745,778-780,531` | `start_build` cutoff = `now_iso8601()`; `fold_history` as_of; `after_cutoff?` lexical string compare; persisted single cutoff | fold/persist | wall-clock default; lexical compare on mixed formats; one persisted cutoff | two cutoffs persisted for resume; parsed compare per axis |
| `apps/query-service/.../trade_state.ex:24-32` | `created_at`/`updated_at` = event timestamp | fold | event time as business time | choose valid vs recorded per field; corrections change displayed times |
| `apps/query-service/.../prime/graph_fold.ex:43-44,138-170,199` | Elixir prime graph fold | fold | events in `(timestamp, version)` order; `created_at`/`updated_at` = event timestamp | second independent as-of fold outside Core; must adopt same rules as `node_state.rs` or diverge |
| `apps/query-service/.../application/dsl/query_dsl.ex:171,233-245` | since/until/between; `order_by_timestamp` default desc | filter | `since` is exclusive (gt) here | boundary must match Core; Core not read for this |
| `apps/mcp-server-elixir/.../protocol/mcp_tools.ex:617` | `query_events` description | doc | "excludes events ingested after" (ingest-time semantics) | doc must state axis after change; currently matches recorded axis |
| `mcp_tools.ex:636,640-641` | `query_events` inputSchema | api | LLM-facing; `Map.take` (2420) drops unknown args | new schema property is an LLM tool contract change |
| `mcp_tools.ex:690` / handler 2444-2454 | `reconstruct_state` inputSchema; "As of:" label | api | single `as_of`, label implies one meaning | agent cannot ask "true at T given corrections filed later" |
| `mcp_tools.ex:4244` | prose "immutable proof of state" | doc | claims immutability for `as_of` | false under valid axis when backdated correction lands after audit date (assumes backdating; unverified) |
| `mcp_tools.ex:1707,1752-1762,3395-3462` | `refine_query` override; string-to-atom mapping | wire | newest wins; mapping `{"as_of", v}` | mirror mapping for each new key; missing mapping drops silently |
| `mcp_tools.ex:2675` | `timeframe` | wire | raw string forwarded | `unverified` Core parse of relative strings |
| `apps/mcp-server-elixir/.../core_client.ex:113-116`, `core_backend.ex:18`, `core_embedded.ex:28-29` | `reconstruct_state` behaviour and impls | api | `as_of` behaviour callback; nil sent as JSON null into NIF | callback change touches both impls; NIF null handling unverified |
| `apps/mcp-server-elixir/.../conversation_context.ex:47-49,356-373,423-425` | session `time_since/time_until/time_as_of` | fold | strings stored and forwarded | per-axis session fields |
| `apps/control-plane/internal/infrastructure/clients/core_client.go:103,141-152,874-878` | `ReplayRequest.AsOf *time.Time`; `QueryEventsRequest` since/until (no AsOf) | api | RFC3339Nano sent; control plane cannot send `as_of` on queries | `AsOf` field if control plane needs time-travel; asymmetry with query-service compat |
| `apps/control-plane/internal/application/usecases/start_replay.go:38-40` | `params["as_of"] = asOf.Format(RFC3339)` | persist | operation record second-truncated; Core gets nanoseconds | one canonical format |
| `apps/control-plane/internal/application/dto/operation_dto.go:17-22`, `operations_handler.go:122-130` | `StartReplayRequest.AsOf`; handler passes unvalidated | api | no axis, no future check | validation per axis |
| `apps/control-plane/internal/interfaces/http/recovery_handler.go:45,57-59` | `recoveryBody.AsOf` raw string into Extra | api | accepts any string | parse per axis |
| `apps/control-plane/internal/application/usecases/recovery_destructive.go:179-183,195` | recovery as_of parse | filter | `time.Parse` error silently yields nil, widening a destructive op | reject on parse error; per-axis parse; execution past 201 unread |
| `apps/control-plane/docs/openapi.yaml:254-257,306-311` | `EntityState.as_of`; `AsOf` query component | api | public spec | new additive component; must match SDKs and fixtures |

Summary by count: 61 core and event rows, 18 prime rows, 36 wire-surface rows. Duplicates were merged where the same path appears in two lanes (for example `store.rs:2614` and `prime/facade.rs` as_of).

### 3.5 Cross-lane findings

- Boundary convention conflict: store is inclusive (`store.rs:2614-2665`, `api.rs:2946-2950`), `query_events.rs:49-66` is exclusive, QueryDSL `since` is exclusive (`query_dsl.ex:233`). Two of three callers disagree today. Fix once, before adding axes.
- `as_of` documented two ways: `mcp_tools.ex:617` says ingest time (matches the record axis), while `store.rs:2654` filters on event timestamp (matches the record axis only for local writes, not followers).
- Parallel folds: Rust `node_state.rs`, Rust `reconstruct_state` (`store.rs:2721`), `config_repository get_at` (`event_sourced_config_repository.rs:219`), and Elixir `graph_fold.ex`. Each has its own time rule. An axis change must land in all four or they diverge.
- Dead or fake time parameters: `RecallContextQuery.as_of` (no reader), `simd_filter` timestamp predicates (no prod caller), `repo find_by_entity_as_of` (mocks only, unverified production).
- Destructive uses of time: retention (`compaction.rs:1041`), recovery as_of (`recovery_destructive.go:179`), replay (`start_replay.go`). These are where a wrong axis deletes or replays wrong data.

## 4. Proposed model

Field names. Keep the existing `Event.timestamp` as the record time (it already means that for local writes) and do not rename it. Renaming touches every constructor and every serialized form, for no semantic gain. Add:

- `valid_from: Option<DateTime<Utc>>`: when the fact became true in the world. `serde(default)`, `None` means "same as `timestamp`".
- `valid_to: Option<DateTime<Utc>>`: when it stopped being true. `serde(default)`, `None` means open-ended. Optional in slice 2; needed for "no longer true from T" without a delete.

Alternative, owner decision (see section 8): a single `valid_at` plus `valid_to`, or rename `timestamp` to `recorded_at` with a serde alias. I recommend keeping `timestamp` and adding `valid_from`. It is the smaller change and the wire name `timestamp` is already public in every SDK.

Record time is assigned by the server and never by the caller. `IngestEventRequest` gets only `valid_from` and `valid_to` as optional inputs. `recorded_at` comes from the existing WAL `wal_timestamp` or the store's stamp, and the request cannot set it. Ingest response returns both `valid_from` (echoed or defaulted) and `recorded_at`, so writers can tell which record they corrected.

How a correction is expressed. A correction is an ordinary event whose `valid_from` is the time the corrected fact became true, and whose `recorded_at` is when it was filed. No supersede event type is required for the fold. For a payload-overwrite fact (NODE_UPDATED), the fold rule is "latest `valid_from` wins among events with `recorded_at <= R`, tie by `recorded_at` then `version`." A correction that places an earlier `valid_from` than the existing fact wins only for valid queries at or after that time. Retraction is `valid_to` on the old fact plus an optional new fact; delete remains record-only.

Query semantics. Two optional inputs, `as_of_valid` (V) and `as_of_recorded` (R), each an RFC3339 instant.

- Neither set: no time filter (unchanged).
- Only `as_of` (old name): maps to `as_of_recorded`. This is the only default that keeps every existing answer unchanged, because today's single axis is record time for local writes.
- `as_of_recorded = R`: events with `recorded_at <= R`. Answers "what did we know at R."
- `as_of_valid = V`: events with `valid_from <= V` and (`valid_to` is None or `valid_to > V`). Combined with a recorded bound, it answers "what was true at V, given everything filed by R." If R is unset, use now for the recorded bound. That gives "true at V, given all knowledge today."
- `since`/`until`: per axis (`since_valid`, `until_valid`, `since_recorded`, `until_recorded`). Old `since`/`until` map to recorded. Boundary: inclusive, the store convention; fix `query_events.rs:49-66` to match.
- Ordering: default stays `(recorded, version)` to match today's per-record semantics for local writes. A valid-time order is opt-in. Fold order for "true at V" is `valid_from` ascending, ties by `recorded_at` then `version`. Do not sort by per-entity `version` across entities.

Defaults that keep every existing caller's answer unchanged:

- Old `as_of` and `since`/`until` mean recorded axis, unchanged code path.
- Events without `valid_from` (every event written before this change) read as `valid_from = timestamp`. This is the existing single-axis meaning, not a backfill of truth. State this in the docs: old events have a valid time equal to their record time.
- Replicated events keep origin `timestamp` for the recorded axis (current behaviour). Open question on whether followers should restamp.
- Unreadable or absent new columns (old Parquet, old checkpoints) read as the default above, with no error.

## 5. Storage and compatibility

- WAL (`wal.rs`): add `valid_from`/`valid_to` inside `Event` (JSON, serde default). Do not change the checksum input format at `wal.rs:142`; it would invalidate existing CRCs. Carry `wal_timestamp` out of `recover_entries` (`wal.rs:683-686`) so the store can rebuild recorded time on boot without a fallback.
- Parquet (`storage.rs:221`): append `valid_from` and `valid_to` as nullable `timestamp[us]` columns at index 7 and 8, and `recorded_at` at 9 if it is persisted there. Do not insert mid-schema; the reader is positional (`storage.rs:1105-1172`). Read by name with a null fallback, the pattern at `storage.rs:45`. Microsecond precision: two events in the same microsecond get the same valid time; ties broken by version. Record a schema marker in KV metadata (`archive_output.rs:97`) so readers stop guessing.
- Checkpoints (`system_checkpoint.rs`): `verbatim` carries `Event`, so new fields flow in. Bump `FORMAT` only if old binaries cannot read the new shape; with `serde(default)` they can. A checkpoint that fails to parse is skipped and the whole WAL replays (`system_checkpoint.rs:9-10`), so `serde(default)` is required, not optional.
- Partition key (`storage.rs:418`, `1609`): keep partitioning by record time. Partitioning by valid time would route a late correction about a March fact into the March file, and retention would then judge it by valid time. Fix the doc at `storage.rs:417-421` to match the code, which anchors by min timestamp.
- Compaction and retention (`compaction.rs:1041`): keep retention on record time until the owner decides otherwise. This is destructive; see slice 9.
- SDK and API versioning: additive only. New optional request fields (`valid_from`, `valid_to`, `as_of_valid`, `as_of_recorded`, `since_valid`, and similar). New response fields. Keep `as_of`, `since`, `until` meaning record time. New fixtures in `sdks/contract/fixtures/`, one per axis. Update `openapi.yaml` in the same change. Python sync and async clients together. MCP schemas are LLM-facing, so reword `mcp_tools.ex:617` and `4244` in the same slice that adds the axis.
- The retained strict read gate (`retained_query.rs:20-29`) must reject the new params, or allow them explicitly. Default: reject, fail closed.

## 6. Prime recall

Under two axes, `as_of` for prime becomes two reads, named explicitly.

- `get_node_as_of(id, R)` becomes believed-at-R (recorded <= R). This is today's behaviour, and it is what "what did the agent know at T" means.
- `get_node_as_of(id, V, R)` (true-at-V given knowledge at R) folds valid order over events recorded by R. A correction filed after V but before R with an earlier `valid_from` is applied.
- `neighbors_as_of`: same split for edges. A later correction that deletes an edge hides it at valid V, and the record-only view still shows it.
- `history(entity_id)`: entries carry `valid_at` and `recorded_at`. Sort key is explicit.
- `observations`: `observed_at` must be named. For LastWrite of corrections, use `valid_from`. For audit, use `recorded_at`. The current `observed_at = e.timestamp` (`facade.rs:1954`) is the recorded axis and must be stated.
- `recall`: no `as_of` today. The recency sort (`recall/api.rs:264`, `facade.rs:1520`) has to pick an axis: "latest known" (recorded) or "latest true" (valid). The two sorts must agree, or the top-20 set differs between call sites.
- `RecallContextQuery.as_of` (`recall/api.rs:40`): either implement through the axis-aware reads, or delete. Silent ignore is not acceptable.
- `forget`: a record-time tombstone. "No longer true from T" needs `valid_to` on the fact, not a delete. Adding `valid_to` makes it expressible without deleting history.
- `compact`: its doc promises reconstruction via `as_of` (`facade.rs:913`). Name the axis in the doc.
- Benchmarks: `evaluate_prime.rs:495` has a stale comment; the temporal arm uses a caller field `session_date` as valid time outside the store. Under two axes this arm should measure valid-time recall, which the store cannot yet provide.

## 7. Slices

Each slice ships alone, is reversible by reverting its commit, and keeps every existing answer unchanged. Confidence tier per slice. No time estimates.

1. Fix the boundary convention. Make `query_events.rs:49-66` inclusive to match the store, and correct the stale comments (`store.rs:2650-2653`, `mcp_tools.ex:617`, `evaluate_prime.rs:495`). Confidence: rock. Unknowns named: whether any client depends on the exclusive path (grep only; `api.rs:2946` test pins inclusive).
2. Carry `wal_timestamp` out of WAL recovery. Surface it from `recover_entries` through `recover` without changing ordering. Confidence: rock. Named unknown: `recover()` callers beyond `wal.rs` not read.
3. Add `valid_from` and `valid_to` to `Event` (serde default), to `IngestEventRequest` (optional), and echo them in `IngestEventResponse`. No query behaviour changes. Confidence: sand. It moves because `Event::reconstruct` has about 10 positional call sites and the cfg(test) Clone must match. Would move to rock by listing every constructor and changing them in one commit with a compile check.
4. Persist the new fields in WAL JSON and checkpoints (serde default). Add a round-trip test for an old WAL line and an old checkpoint. Confidence: sand. Moves up once the checkpoint FORMAT question (section 8) is answered.
5. Persist in Parquet as trailing nullable columns, name-based read with null fallback. Add a test that reads a file written before the change. Confidence: sand. Reader is positional and `storage.rs:57-66` constants must stay valid. Moves to rock with a golden-file test of an old file.
6. Axis-selected filter in `filter_entries` and `apply_filters`. Widen `IndexEntry` with a second timestamp. Both paths updated in one commit, with a test pinning index and full-scan parity (`store.rs:6350-6398` pattern). Confidence: sand. Moves up if index widening is measured for memory on a real tenant.
7. Axis-aware ordering in `query_window` and `query_window_from_archive` (default stays recorded). Test that resident and archive paths agree (`store.rs:6454` pattern). Confidence: sand. Cross-entity tie order is the risk; moves up once tie-break is decided.
8. Axis-aware fold: `reconstruct_state` / `entity_state` take `as_of_valid` and `as_of_recorded`. Also fix `config_repository get_at` break-on-first (`event_sourced_config_repository.rs:219`). Confidence: sand. Four independent folds must agree; moves up with a parity test across them.
9. Prime: axis-aware `get_node_as_of`, `neighbors_as_of`, `history`, `observations`, recall recency. Resolve `RecallContextQuery.as_of`. Confidence: sand. Depends on slice 8 and on the observed_at decision (section 8).
10. Surface and fixtures: SDKs (Rust, TS, Python sync and async, Go), `openapi.yaml`, contract fixtures, query-service whitelist and `flatten_predicate` error on unsupported op, MCP schemas. Confidence: rock per surface enumeration (additive, listed). Gated on slices 3 and 8 existing.
11. Replication and sync cursor: keep the sync cursor on the record axis, and decide HLC vs WAL sequence as record clock. Confidence: sand (was water before the spike). Spike: `apps/core/tests/bitemporal_replication_spike.rs` (feature `embedded-sync`, in-process, no infra, passing).
   - In-process `sync_to` carries `timestamp`, `valid_from` and `valid_to` to the peer. The peer's `store.ingest` restamps the per-entity `version` (store.rs:802-860, read, not asserted by the spike).
   - Wire path `receive_sync_push` carries `valid_from`/`valid_to` from the payload, and pins an unset `valid_from` to the origin's `timestamp`, so follower and leader agree on the valid axis (spike: both answer `draft` at valid 2026-01-15, `final` at 2026-03-15). Record time is still the receive time, so the follower has no events at a pre-receive recorded bound. The two sync paths disagree on record time.
   - `events_for_sync` cursor is `last_ms + 1` (embedded/core.rs:785-800). Events sharing the millisecond the peer last saw are not re-sent. Not exercised by the spike.
   - Backdated correction (valid 2026-02-01, filed after a 2026-03-01 fact): after both-way sync, both nodes gave identical answers for five (recorded, valid) bounds.
   - Not checked: clock skew (both nodes share one process clock); HTTP transport; HLC on `Event` (absent). Q5 and Q6 in section 8 are still open, so this is not rock.
   - Moves to rock when: the follower keeps the leader's record time (Q5), the cursor passes a same-millisecond burst, and the record clock is chosen (Q6).
12. Retention, compaction and archive axis. Decide whether retention keys on record time (current) and whether archive file names carry a valid range. Confidence: water. Destructive; the owner decides the axis before any spike. Spike: read-only report of what retention would delete under each axis for a sample tenant, through `allsource-data-access` MCP tools only.
13. Recovery and replay axis (`recovery_destructive.go`, `start_replay.go`): reject unparseable as_of, per-axis parse, canonical format. Confidence: sand. Parse-error silent widening is a bug today regardless of the axis change; this slice can ship first on its own.

Slice 13's parse fix and slice 1 are independent of the model and can ship before any decision in section 8.

## 8. Open questions (owner decisions)

1. Field naming: keep `timestamp` as record time and add `valid_from`/`valid_to` (recommended), or rename to `recorded_at` with a serde alias?
2. Default axis for an unqualified `as_of`: recorded (recommended, keeps answers unchanged) or valid (matches what most callers probably mean)?
3. Does `valid_to` belong in the first cut, or only `valid_from`? Retraction without delete needs it.
4. Is a correction a plain event with an earlier `valid_from`, or does the product need an explicit supersede marker that names the event it replaces? The fold works without the marker; the marker makes audit clearer.
5. Replicated events: should a follower restamp `recorded_at` at arrival, or keep the leader's record time? Affects slice 11.
6. Record clock: HLC on the replication wrapper, WAL sequence, or `wal_timestamp`? Two clocks with no shared field exist today.
7. Retention and archive: keep record time, or move to valid time? Destructive, so a migration plan is needed.
8. `observed_at` in prime: LastWrite on valid time (correct for corrections) or on record time (today)? This changes which value a recall returns after a correction.
9. Checkpoint `FORMAT`: bump to 2 for the new Event shape, or rely on `serde(default)` and keep 1?
10. Partition key for Parquet: record time (recommended) or valid time?

## 9. What was not checked

- Re-read by the lead: `store.rs:2355-2412` (doc and lazy-load; not the comparison), `store.rs:2614-2615` and `2654-2655` by grep only (line numbers confirmed, bodies not read), `event.rs` timestamp lines by grep, `wal.rs:119,132,142` by grep, `event_dto.rs:1-30` read (confirms no timestamp in `IngestEventRequest`), `event_dto.rs:41-58` by grep.
- Not read at all: `store.rs` ingest_batch body (933-1023, partial); version stamp body (640-680, 810-830, 950-970); `store.rs` test bodies beyond 6460; `api.rs` ingest handler body (255-290) and query handler body (~2320-2350); `storage.rs` `write_tenant_events` (503-598) and `record_flushed_versions` body (1354-1440); `compaction.rs` outside 1040-1130; `archive_repack.rs`, `archive_budget.rs` bodies; `system_checkpointer.rs`, `system_bootstrap.rs`; `index.rs` beyond line 40; `feed_log.rs`, `storage_feed.rs`, `backup.rs`, `batch_processor.rs`, `tenant_loader.rs`, `storage_integrity.rs`; `cluster/crdt.rs` (`ReplicatedEvent.hlc_timestamp` at 113) and `hlc.rs` (struct at 27).
- Out of lane or unread: `prime/vectors/*`, `prime/lexical.rs`, `sync.rs`, `import_export.rs`, `event_store.rs`, `tenant_cache.rs`, `projection_bundle.rs`, `schema.rs`, `recall/compressor.rs`, `index_builder.rs`, `ollama.rs`; `facade.rs` `add_edge` (789-885), `delete_edge` (886), `delete_node` body beyond 530, `remember` body beyond 1370, `declarative.rs` LastWrite body (150-250); `hosted.rs` construction path beyond the two lines.
- SDKs and query-service: `sdks/rust/src/client.rs`, Rust ingest struct and query builder bodies; TS `client.ts` 290-330; Python `client.py` 140-170 body; Go events query; `events_query_with_entity_version.json` body; `rust_core_client.ex` `query_events_page` beyond 1425-1454; `event_controller.ex` 139+; `tenant_projections.ex` 536-575, 600-660; `replay_analysis.ex` 1-35, 66+; `replay_analysis_store.ex` 26+; `customer_replay_*.ex`; the other ~144 query-service lib files (grep only).
- MCP and control plane: `mcp_tools.ex` bodies at 2675, 3600-3610 (grep only); `conversation_context.ex` 356-373 (grep only); `core_client.go` 741-778 (grep only); `recovery_destructive.go` 201+ execution; `openapi.yaml` events and replay paths; `operations_handler.go` beyond 121-130; other `recovery*.go` files; Core replay semantics on the control-plane side.
- Confidence on the whole model: sand. Readers are positional in Parquet, every Event field change touches the reconstruct call sites, and the same time rule is implemented in at least four places (Rust fold, config `get_at`, prime `node_state`, Elixir `graph_fold`). Any slice that changes one of these without the others is a silent divergence, which is the failure mode this inventory exists to prevent.
- Not measured: index memory for widened `IndexEntry` on a real tenant; Parquet read cost for trailing columns; follower behaviour under backdated corrections. None of these have numbers here.
