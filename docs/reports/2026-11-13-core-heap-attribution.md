# Core heap attribution (S0 report)

Date: 2026-11-13. Design: [core-memory-bound design](../plans/2026-11-12-core-memory-bound-design.md) (S0 section, lines 258-299). Plan: [US-005](../plans/2026-11-12-core-memory-bound-plan.md).

## Result

- **Attribution:** 99.92% of profiled bytes (1,263,255,636 of 1,264,308,510 B) land on a design owner from the S1 owner table. Against the replay's steady live heap (1,306,138,520 B, `heap_replay.json`), attribution is **96.7%**. That clears the 90% bar.
- **Owners not in the S1 owner table:** none at design v22. At f90dcb84 the table lacked two owners, `RateLimiter` shard maps (528,394 B) and `MetricsRegistry` Prometheus histograms (524,480 B). Design v22 (sha256 `8d09426f18e446ccf4116d9f5ea781a78d614caae10390f34da871febfa0af00`, commit `80ff3d77`) adds the row `fixed: rate_limiter, metrics` at design line 320, and both sites match it. **The S1 gate passes at v22:** attribution 96.7% ≥ 90%, `owners_missing` empty.
- **Unattributed share, fixture:** 3.28% of steady live (42,882,884 B), made of 41,830,010 B the profile does not show and 1,052,874 B on the two unowned sites.
- **Unattributed share, production:** 33.70% (1,878,263,738 / 5,574,260,718, `brief` line 20 and design line 199). The two shares use different definitions, and the fixture under-states Value-tree overhead (see below). The gap is 30.4 points, above the 5-point threshold at design lines 283-285, so **the user decides on one production `heap-profile` deploy.**
- **Measured here:** `resident_bytes()` walk cost and estimator error on the fixture (walk section); the per-class estimator error (class section); the cold-load decode peak (projection section); Prime projection size in a `prime` build (Prime section). **Not measured:** `system_store` per-domain growth. The owner dropped the production heap-profile deploy on 2026-11-13, so it stays open. See the item table.

## Method

- **Profile:** `heap.pprof` from the musl `heap_replay` run (`heap_profile_run`, container build of `allsource-core-heap-builder`, rust 1.94 alpine). `inuse_space`, jemalloc `lg_prof_sample:19` (mean sample interval 512 KiB), build id `1ac3a3e74814c9ea997c0f68c482d998eefdbff5`, taken 2026-11-13 14:59:57 WET. 30 stacks, 131 locations, 1,264,308,510 B summed.
- **Symbols:** the binary was copied out of volume `core-heap-profile-target` (`/target/heap-replay/examples/heap_replay`, aarch64 static musl, not stripped). `go tool pprof` could not read it (`build-id mismatch ... != `), so the addresses were resolved with GNU `addr2line` 2.44 in `alpine:3.22`. Each location maps to one innermost function. Site labels below use the innermost application frame.
- **Mapping:** each stack was labelled by its first allocation-site frame in `apps/core`. Frames in `alloc::raw_vec`, `indexmap`, `dashmap`, `hashbrown` and `serde_json` are the allocator's path, not the owner.
- **Working files:** `scratchpad/core-heap-profile/` (`heap.pprof`, `heap_replay`, `addrs.txt`, `symbols.txt`). Not committed.

## Attribution by owner

Sum of sampled bytes per S1 owner row. Percent is of the profiled total.

| S1 owner (design row) | Bytes | % of profile | Sites (innermost app frame) |
|---|---:|---:|---|
| `events.resident` | 1,161,223,324 | 91.85 | hydration decode `ParquetStorage::record_batch_to_events` (storage.rs:972, 1496), serde `Value` trees for payload and metadata, `append_loaded_event` push (store.rs:2278, 2289) |
| `index.*` | 53,492,592 | 4.23 | `EventIndex::index_event` vec and dashmap growth (index.rs), `EventIndex::new` (store.rs:391) |
| `query.*` (per request) | 26,379,744 | 2.09 | `tenant_feed_scoped` candidate/output vec (store_feed.rs:476-560), `read_feed_payloads` `FastMap<Uuid, Event>` (store_feed.rs ~598) |
| `projection.entity_snapshots` (permanent) | 21,567,136 | 1.71 | `EntitySnapshotProjection::process` (projection.rs:123, 137) |
| `feed_index` | 592,840 | 0.05 | `observe_feed_head` vec push (store_feed.rs:187-200) |
| **no owner in S1 table** | 1,052,874 | 0.08 | `RateLimiter` shard maps (rate_limit.rs:145-150, middleware.rs:142); `MetricsRegistry` histograms (metrics.rs:17, 173) |
| **total** | **1,264,308,510** | **100** | |

Owner class view (design line 320): events 1,161,223,324 B; maps (index, feed, query) 80,465,176 B; rings 0 B (no sample); permanent (entity_snapshots only) 21,567,136 B; no owner 1,052,874 B.

Per-event cost, measured: `events.resident` / 229,246 fixture events = **5,065 B/event**. The fixture count equals the sum of the six production hydrated-tenant counts (201,785 + 16,840 + 5,484 + 3,447 + 1,662 + 28, design line 203), so every fixture event is counted as resident. `peak_live_bytes` is 0 in the JSON (not recorded by this run) and `ledger_bytes` is null (no ledger exists before US-006).

## Owners with no sample

The following owner-table rows have no sample. At a 512 KiB mean sampling interval, a site of about 1 MB would be absent roughly 13% of the time and a site of 3 MB under 1%, so these rows are expected to be under about 1 MB each in the fixture: `storage.pending`, broadcast rings (`store.rs:762,912,1027,1101`), `webhook.queue`, `pipeline.windows` and `pipeline.state`, `read_models`, `tenant_loader` entries, `cache_generations`, `entity_versions`, `version_index_*`, `geo_index`, `schema_evolution`, the search service, `system_store` (the fixture has no system tenant), `misc.*`. Their production size is not known from this profile.

## Per owner class: estimator error, fixture

Profile bytes are the table above. Estimator bytes come from the store's pre-ledger gauge, `EventStore::resident_structures()` (`store_resident.rs:38`), plus a walk of `entity_snapshots` (`get_state` per distinct entity, `hash_entry_bytes` plus key capacity plus value heap). They come from a probe that hydrates the six fixture tenants in a fresh process with no requests, so `query.*` transients are absent by construction. Error = profile − estimator.

| Owner class | Profile B (% of profile) | Estimator B | Estimator source | Error B | Error % of profile |
|---|---:|---:|---|---:|---:|
| `events.resident` | 1,161,223,324 (91.85) | 984,158,283 | gauge `events`, 229,246 entries | +177,065,041 | +15.2 |
| `events.resident`, cross-check | 1,161,223,324 | 986,292,336 | walk prototype, same events | +174,930,988 | +15.1 |
| `index.*` | 53,492,592 (4.23) | 50,664,462 | gauge `entity_index` 19,079,544 + `type_index` 25,166,030 + `id_index` 6,418,888 | +2,828,130 | +5.3 |
| `query.*` and `feed_index` | 26,972,584 (2.13) | 0 | none: `feed_indexes` reports entries (6) and no bytes; `query.*` is per request and the probe sends none | +26,972,584 | 100 |
| permanent: `entity_snapshots` | 21,567,136 (1.71) | 16,600,826 | walk, 4,036 entities | +4,966,310 | +23.0 |
| permanent: `entity_versions` | not sampled (under 1 MB) | 177,584 | gauge | n/a | n/a |
| rings | 0 (no sample) | 0 | no receivers, no slot count at S0 | 0 | 0 |
| no owner: `rate_limiter`, `metrics` | 1,052,874 (0.08) | none | S1 charges these by measured constant | 1,052,874 | 0.08 |

Profile classes sum to 1,264,308,510 B, the profile total. Estimators sum to 1,051,601,155 B against the probe's steady live 1,331,877,181 B, so 280,276,026 B (21.0%) of live heap has no estimator. That is outside the 10% two-sided bound of I2. The pre-ledger gauge does not meet I2; the ledger must charge the events class (+15.2%) and the map and feed classes.

Limits: the probe state (hydrate only) and the profile state (replay with requests) differ, so the table compares classes, not totals. The `entity_snapshots` walk and the events walk use the same model; the 23% gap to its profile is not attributed to a term.

## Owners not in the S1 owner table, and their resolution at v22

At f90dcb84 these two owners had no S1 row:

1. **`RateLimiter`**: two `DashMap` instances built in `RateLimiter::new` (`apps/core/src/infrastructure/security/rate_limit.rs:145-150`) and constructed in `RateLimitState::new` (`infrastructure/security/middleware.rs:142`). 528,394 B, allocated once at startup.
2. **`MetricsRegistry`**: Prometheus `HistogramVec` allocations in `MetricsRegistry::new` (`apps/core/src/infrastructure/observability/metrics.rs:17, 173`). 524,480 B, allocated once at startup.

Design v22 (commit `80ff3d77`, re-approved by the owner) adds the single row `fixed: rate_limiter, metrics` at design line 320: charged once at startup by the measured constant, `rate_limiter` per new bucket key, never released except `rate_limiter` at bucket removal (`rate_limit.rs:158`). Both sites above are the sites that row names, so no owner remains outside the table.

Coverage question for the S1 table (not an owner gap): the `events.resident` charge column names push, splice (`store.rs:2288`), WAL recovery and token-compaction push. The cold-load decode buffers (`storage.rs:972, 1496`) and the hydration entry (`store.rs:1946, 1983`) are not named. The design should say whether they are covered by `splice`.

## Design S0 items

| Item (design lines 286-294) | Status | Value | Label |
|---|---|---|---|
| ≥ 90% of live heap attributed by site | done | 96.7% of steady live; 99.92% of profile samples | measured (sampled) |
| Attribution mapped to S1 owners | done; the two unowned sites are v22 rows | table above | measured (sampled) |
| Unattributed split per owner class, each as ledger-estimator error, plus no-owner bytes | done | per-class table (class section); no-owner 1,052,874 B | measured (sampled profile vs pre-ledger estimator). The estimator is the store gauge `resident_structures()` plus a walk for `entity_snapshots`; the ledger does not exist until US-006 |
| Bytes held by `entity_snapshots` | done | 21,567,136 B (1.65% of steady live) | measured (fixture, sampled) |
| Cost per event of `resident_bytes()` | measured (prototype walk) | 107.7 ns per event; walk 4,302 B per event | measured, fixture, host run. Median 24,686,709 ns over 229,246 events, 7 reps, min 107.2 ns. Estimator error: profile `events.resident` is 15.1% above the walk total (walk section) |
| Per-domain growth of `system_store` | **not measured**; the production heap-profile deploy that would supply it was dropped by the owner | none | fixture has no system tenant, `shape.toml` is SHA-pinned, and no per-domain production series is in this worktree (the 2026-11-11 handoff file is not present) |
| Keying and size of Prime projections | keying from code; size **measured** in a `prime` build | 3,040 B per node, 1,520 B per edge (Prime section) | measured, synthetic shape (numbers and short strings). `prime` is not in the default feature set (`apps/core/Cargo.toml`: `default = ["server", "community"]`), so the server build has no Prime projections; production Prime counts are not measured |
| 8 GB projection, term by term | done, with gaps | see next section | labelled per term |

Prime projection keying (read from source, every key is a node or edge id; growth is unbounded by design):

- `NodeStateProjection` `nodes: DashMap<String, NodeEntry>` (node_state.rs:37)
- `CrossDomainProjection` `links: DashMap<(String, String), CrossDomainEntry>`, `node_domains: DashMap<String, String>` (cross_domain.rs:50, 52)
- `NodeTypeIndexProjection` `DashMap<String, HashSet<String>>` keyed by node type (node_type_index.rs:20)
- `DirectedAdjacencyProjection` `adj: DashMap<String, Vec<AdjEntry>>`, `edge_index: DashMap<String, String>` (adjacency.rs:50, 52)
- `DomainIndexProjection` `DashMap<String, Vec<NodeId>>` keyed by domain (domain_index.rs:24)
- `GraphStatsProjection` `DashMap<String, usize>` keyed by node type and by relation (stats.rs:44, 46)
- `RelevanceDecayProjection` `DashMap<String, RelevanceScore>` (relevance.rs:69)
- `ContradictionDetectionProjection` `DashMap<(String, String), (String, String)>`, `DashMap<String, Contradiction>` (contradiction.rs:52, 54)

`entity_snapshots` is keyed by `event.entity_id_str()` with value `(merged Value, last_ts)` (projection.rs:110-140).

## Prime projection size, measured

Probe: `prime` build (`--features prime`, heap-replay profile; scratch example `s0_prime_size_probe`, not committed). 20,000 nodes (`concept`, properties `name`, `rank`, `domain`) and 40,000 edges (`links`, two per node) are written through `Prime::add_node` / `add_edge` on `Prime::open_in_memory()`. The stored events are then re-ingested into a plain `EmbeddedCore` (same event store, no graph projections). The allocator delta of the Prime writes minus that plain delta is the Prime-only share: the graph projections and the facade state.

| Quantity | Bytes | Label |
|---|---:|---|
| Prime writes (events and projections) | 383,968,723 | measured, `cap` allocator delta |
| Plain core, same 60,000 events | 323,158,650 | measured |
| Prime-only (graph projections and facade state) | 60,810,073 | measured |
| per node / per edge / per event | 3,040 / 1,520 / 1,013 | measured |

Limits: one synthetic shape. Per-entry size scales with property size and node degree. The fields are private, so no per-projection split is measured. Production Prime node and edge counts are not measured.

## The 8 GB projection, term by term

The design says the projection is an estimate that no approved stage measures (design lines 57-71). S0 supplies the inputs. It does not supply the answer, which is S2's rehearsal.

| Term | Value | Label | Source |
|---|---|---|---|
| On-disk Parquet, all tenants | 724 MB | measured (production, 15.3 h window to 2026-11-08) | [fly-cost-reduction.md:48](../plans/2026-11-08-fly-cost-reduction.md) |
| `system` tenant on disk | 429 MB | measured (same window) | fly-cost-reduction.md:49 |
| Registered tenants | 22 | measured (same window) | fly-cost-reduction.md:42 |
| Hydrated tenants | 6 tenants, 229,246 events | count measured in production; their resident bytes are not measured in production | design lines 203-204 |
| Resident bytes of the 6 tenants, fixture | 1,161,223,324 B `events.resident` | measured in fixture; scales with payload shape, not production | this report |
| `entity_snapshots` removed by S1 | about 92 MB | **estimated**: fixture share 1.65% applied to 5.574 GB | this report |
| Cold-load decode peak | 1,337,076,908 B peak live for the six tenants hydrated in a fresh process; largest single-load transient 80,052,209 B (tenant-01) | measured (fixture, probe). Production: estimated 80.8 MB transient | `cap` `max_allocated` and `allocated` around `ensure_tenant_loaded`. Production estimate = fixture transient × 3,892 / 3,858 B (production mean payload plus metadata over fixture mean), assuming decode buffers scale with payload bytes |
| Heap per event vs payload JSON | fixture 1.23×; production about 3.8× | production ratio is the brief's claim (brief line 30), not re-measured here. The fixture ratio is `events.resident` / fixture `estimated_total_bytes` 943,060,675 | run log `logs/run-1.out` |

The fixture's `steady_live` is 23% of production's live heap for the same event count (1,306,138,520 vs 5,574,260,718 B). The likely cause is that the synthetic payloads are built from numbers only (`shape.toml`, "numbers only"), so they hold fewer owned `String` nodes than real payloads. This is an inference from the shape file, not a measurement. Because of it, the fixture's events share cannot stand in for production's, and the 8 GB projection cannot be read from these bytes.

## Fixture against production unattributed share

| | Fixture | Production |
|---|---:|---:|
| Definition | steady live not covered by an owner site (profile) | live heap minus `store_resident` gauges (`store_resident.rs:36-64`) |
| Share | 3.28% | 33.70% |
| Gap | 30.4 points | above the 5-point line (design 283-285) |

The two shares are not the same quantity. The production share is set by gauges that use the length-based `json_heap_bytes` floor (design lines 84-86). The fixture share is set by sampled allocation sites. The fixture also under-states Value overhead (1.23× vs about 3.8×), so the events share it reports is not production's. Per design, the user decides whether to deploy one production `heap-profile` build that emits attribution numbers only.

## `resident_bytes()` walk, measured on the fixture

Prototype of design decision 1's walk, timed over all 229,246 fixture events. The events are loaded from the fixture's Parquet by `ParquetStorage::load_events_for_tenant` for `tenant-01` to `tenant-06`. The probe is not committed; US-006 writes the real walk with its tests.

| Quantity | Value | Label |
|---|---:|---|
| Events walked | 229,246 | measured |
| Walk total | 986,292,336 B (4,302 B per event) | measured, prototype |
| Walk cost | median 24,686,709 ns (107.7 ns per event); min 24,575,333 ns; 7 reps | measured, host |
| Profile `events.resident` for the same events | 1,161,223,324 B (5,065 B per event) | measured, sampled |
| Estimator error, walk against profile | walk 15.1% below profile | measured; sampling noise on the profile side not bounded |
| Walk against payload JSON bytes | 1.115× (884,350,717 B of payload plus metadata JSON) | measured |
| Profile against payload JSON bytes | 1.313× | measured, sampled |
| Walk total, budget estimate on reload | 943,037,693 B, against 943,060,675 B in `fixture.json` | measured; differs by 22,982 B (0.002%) |

The walk counts `size_of::<Event>()`, the `String` capacities of the payload and metadata `Value` trees, the `Vec` capacities of arrays, and per map entry `size_of::<(String, Value)>()` plus the key capacity. The three id strings are counted at `len()`, because `EventType`, `EntityId` and `TenantId` do not expose capacity. The map entry model ignores `BTreeMap` node slack, which is the likeliest cause of the 15.1% gap. That gap is not yet attributed to a term.

The cost is measured on the host (macOS, aarch64, `--profile heap-replay`), not in the musl container, so 107.7 ns is a host figure. Against the throughput in `CLAUDE.md` (469K events/s, not measured here, which is about 2,132 ns per event), the walk is about 5% of ingest time. It runs at push and hydration, so the in-path number needs US-006's own measurement.

## Limits

- Sampling: sites of one or two samples carry large relative error. Large sites are reliable at this rate: jemalloc corrects each sample by its inclusion probability, and allocations far above the 512 KiB interval are sampled almost every time. The 896,937,756 B cold-load site is a single stack, and it alone is 71% of the profile.
- The profile is `inuse` at dump time. Transient per-request sites (`query.*`) are live only if the dump fell during a request. The 6.6 MB `read_feed_payloads` map and the 19.7 MB candidate vec are labelled `query.*` on that basis, not on a confirmed request phase.
- `steady_live_bytes` and the profile were not sampled at the same instant. The 41.8 MB gap between them is the timing difference plus sampling noise.
- No production data was read. The fixture is numbers only.
- Probes (scratch examples, not committed, same policy as the walk prototype). `s0_attribution_probe`, default build under `--profile heap-replay`, run on a private clone of the fixture: hydrates the six tenants, prints the store gauge, the `entity_snapshots` walk and the allocator counters. `s0_prime_size_probe`, `--features prime` under the same profile: the Prime-only split above. Each was run once. Allocation counts repeat across runs on the same input. Cold-load peak is the first use of a fresh process, so it includes first-use allocations (Parquet reader state).

## Decision needed

1. **S1 gate:** passes at design v22. Both owners are rows of the v22 table (design line 320).
2. **Production heap-profile deploy:** fixture 3.28% vs production 33.70%, a user decision (design 283-285). The owner first chose one production build, then dropped it on 2026-11-13 after six Fly builds of the heap-profile image failed the 240-255 s phase deadlines in the Dockerfile; production stayed on v61 throughout. Production's unattributed 1.88 GB therefore has no allocation-site attribution; the S1 ledger's `allsource_heap_unattributed_bytes` gauge is the next measurement of it.
3. **`system_store` per-domain growth:** not measured; the deploy that would supply it was dropped. Prime projection size is measured (Prime section), on synthetic input; production counts are the open part.
4. **`resident_bytes()` cost:** 107.7 ns per event on the host, prototype. The 15.1% estimator gap to the profile is open; US-006 attributes it.
