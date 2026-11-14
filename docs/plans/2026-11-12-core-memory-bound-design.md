# Core memory bound: design (S0, S1 and the measurement harness)

Date: 2026-11-12. Brief: [2026-11-11-core-memory-bound-brief.md](2026-11-11-core-memory-bound-brief.md).
Epic: Chronis `t-4c058a`. Branch: `feat/core-memory-bound`. Code references are to `origin/main`,
whose `apps/core` is identical to the deployed `f4b53930`.

## How to read this design

This design fixes decisions, invariants and contracts: what must be true, who owns what, which
mechanism enforces it, and which test proves it. Line-level mechanics are left to the Plan. There,
each one becomes a bead whose acceptance test is listed under
[Acceptance tests carried into the Plan](#acceptance-tests-carried-into-the-plan).

The design went through seventeen fresh-context review rounds:

- full rounds [r1](2026-11-12-core-memory-bound-design-review-r1.md) to
  [r10](2026-11-12-core-memory-bound-design-review-r10.md);
- delta rounds [r11](2026-11-12-core-memory-bound-design-review-r11-delta.md) to
  [r16](2026-11-12-core-memory-bound-design-review-r16-delta.md);
- [r17](2026-11-12-core-memory-bound-design-review-r17.md), a full review of the rescoped design.

Rounds r1 to r16 converged on one finding: making eviction routine reaches every in-memory
subsystem that assumed eviction never happens. That finding sets the scope.

## Scope

**Approved by this design:**

- **S0: heap attribution.**
- **S1: own and cut, without changing eviction.**
  - A ledger counts every resident byte.
  - `entity_snapshots` leaves server mode.
  - The geospatial route becomes tenant-scoped.
  - Eviction keeps today's trigger, budget and code. Production has recorded 0 evictions.
- **The harness that gates every stage.**

**S1 is conditional on S0.** The S1 structural commit starts only after the S0 report:

- attributes at least 90% of live heap;
- shows that every owner it found is in the S1 owner table.

If S0 finds an owner missing from the table, adding it changes this design, so the design goes
back for review and approval before S1 starts (ritual `drift`).

**Deferred to its own specification: routine eviction.** That specification covers:

- reclaim;
- refusing a write before its memory is allocated;
- pins;
- keeping not-yet-durable events through eviction;
- the background flusher;
- durable delivery cursors.

It is written after S0 and starts from the audit list in
[Input for the eviction specification](#input-for-the-eviction-specification).

**The 8 GB claim is an estimate, and no approved stage measures it.** Two cuts are expected to
bring the resident set from 5.57 GB to about 2–3 GB:

- S1 removes `entity_snapshots`. S0 measures its size.
- S2 stores payloads as canonical text, about 2.6 GB by the brief's arithmetic.

With every tenant resident, that would fit the 8 GB machine with no eviction. The S0 report states
this projection term by term, and labels each term measured or estimated:

- the on-disk total (724 MB of Parquet);
- the `system` tenant (429 MB on disk);
- the 6 hydrated tenants out of 22 registered;
- the cold-load decode peak.

The S2 rehearsal is the first measurement of the claim.

## Decisions

1. **The ledger counts residency in one unit and does not enforce anything.**
   - **Location and counters.** `ResidentLedger` is new, in
     `apps/core/src/infrastructure/memory/ledger.rs`. It keeps a per-owner count and a total
     count of resident bytes.
   - **The unit is measured heap bytes.** Each owner class has its own measurer:
     - **Events:** `Event::resident_bytes()`, computed once when the event becomes resident. It
       adds `size_of::<Event>()`, the capacities of the owned strings, and a capacity walk of the
       payload and metadata `Value` trees. The walk counts `String` and `Vec` capacities and a
       per-entry node size for `BTreeMap`. S0 measures the walk's cost per event, because it runs on
       the ingest and hydration hot paths. The `store_resident` gauges (`store_resident.rs:36-86`,
       which use the length-based `json_heap_bytes` floor at `observability/heap.rs:17-19`) stay
       as before, but only as diagnostics. I2 compares the ledger total with live heap, and no
       `store_resident` gauge enters it.
     - **Maps and vectors:** a map is charged its table, capacity × `size_of` the entry, plus each
       entry's owned heap: the key and value `String` capacity and the `Vec<IndexEntry>`
       capacity (`index.rs:39-45`).
       - The table charge is re-read whenever the map's own table grows.
       - An insert that changes an existing key's owned heap re-reads that key and charges the
         capacity delta. Pushing onto an existing key's `Vec` is the main case
         (`index.rs:82-83,88-89`).
     - **Broadcast rings:** see the owner table.
   - **Charging.** An owner is charged where its bytes become resident and released where they stop
     being resident (see the owner table). The ledger never refuses, reserves or evicts.
   - **Other counters keep their own unit.** The eviction budget counter stays on its estimate
     (`event.rs:307-316`) until the eviction specification replaces it. The `system_store`'s
     serialized-length gauge (`allsource_system_resident_bytes`) stays as a labelled, separate
     gauge. The ledger counts the same bytes in its own unit.
   - **Exports.** The ledger exports `allsource_heap_unattributed_bytes`, the signed difference
     live heap minus ledger total, and `allsource_core_ledger_owner_bytes{owner}`.
2. **Eviction behaviour is unchanged.**
   - **What S1 adds on the eviction paths.** Ledger release calls at:
     - `try_evict_tenant`;
     - token-compaction removal (`store.rs:1440-1442`);
     - read-model LRU (`store_read_model.rs:149-158`);
     - webhook delivery end (`webhook_worker.rs:35-50`);
     - `evict_window` and pipeline reset (`pipeline.rs:183,190`);
     - flush success.

     Every other charge and release, including the query release at request end, is listed in the
     owner table.

     The oracle commit adds two behaviour-neutral changes: the LRU counter seam
     (`tenant_loader.rs:112,173`) and the `parse_cache_byte_budget` extraction. The policy's
     behaviour changes in no other way.
   - **The eviction policy items**, each a whole `syn` item, so each is digested whole:

   | Item | Location |
   |---|---|
   | `admit_hydration`, `enforce_cache_budget`, `enforce_cache_budget_now`, `evict_lru_until`, `has_memory_headroom`, `try_evict_tenant` | `store.rs:1825,2035,2048,2055,2084,2127` |
   | `TenantLoader::over_budget`, `pick_lru_excluding`, `touch`, `mark_loaded_with_integrity`, `mark_unloaded`, `set_byte_budget`, `add_bytes`, `total_bytes` | `tenant_loader.rs:102,125,111,171,181,93,190,204` |
   | `memory::available_bytes`, `memory::has_headroom` | `observability/memory.rs:21-30,84-86` |
   | `DEFAULT_MEMORY_RESERVE_BYTES`, `reserve_from_env` | `hydration_admission.rs:22,65-66` |
   | `parse_cache_byte_budget`, a new function extracted at the oracle commit from `from_env` (`store.rs:3254`) and `from_env_vars` (`:3314-3330`), with no behaviour change | `store.rs` |
   | `estimated_size_bytes` | `event.rs:307` |
   | read-model eviction: `evict_model`, and `bound_cache` with both of its triggers (more than 32 models, or summed model bytes over 64 MiB) | `store_read_model.rs:149-158,160-167` |

   Three gates check this:
   - **`eviction_policy_diff_allowed`.** Between the oracle commit and the S1 structural commit,
     the diff inside the digested items may only add lines that call the `ResidentLedger` API. A
     `syn` comparison checks this, and it runs once, at step 4.
   - **`eviction_policy_unchanged`.** The digest of every item above is frozen at step 4, after
     that check, so any later candidate edit to the policy fails.
   - **`evictions_match`.** In a budget-driven eviction scenario, the evicted-tenant sequence
     equals the oracle's `expected-evictions.txt`. The S1 report shows once that `f4b53930`
     produces the same sequence (Acceptance 4).

   The headroom branch cannot fire on the darwin harness host, where `available_bytes` returns
   `None`. No harness test exercises it. Only the digest locks it.

   The geo change (decision 5) adds no hydration.
3. **Every resident structure is an owner.** The owner table is in the S1 section. Owners that
   are never released are summed in `allsource_core_permanent_bytes`.
4. **`entity_snapshots` leaves server mode.** It is registered only when a new
   `EventStoreConfig::register_entity_snapshots` is true. Embedded's `build_store_config` sets it
   to true; the server default is false.
5. **Geospatial is tenant-scoped and reads resident data only, as today.**
   - **Tenant.** S1 changes `geo_query` (`api.rs:1978-1991`) to take the tenant from the request's
     `AuthContext`. It will ignore the body's `tenant_id`, whose optional filter
     (`geospatial.rs:361-366`) is removed. The system principal gets no override, unlike
     `read_model_api.rs:94-103`, which lets the system principal name a tenant.
   - **Candidates.** `GeoIndex` has no tenant key (`geospatial.rs:127-131`), so the global index
     scan (`:320-340`) stays, and the candidate map still spans every tenant.
     - Each candidate id is resolved in four steps, without `get_event_by_id`, which clones before
       any tenant check (`store.rs:2347-2350`):
       1. `index.get_by_id(id)` returns an offset (`index.rs:136-138`).
       2. `events.read().get(offset)` borrows the event.
       3. The borrowed event's `tenant_id` is compared with the caller's.
       4. The event is cloned only when the two match.
     - `execute_geo_query` takes a resolver `&dyn Fn(Uuid) -> Option<Event>`, which runs those four
       steps, in place of the `&[Event]` slice (`geospatial.rs:314-317`). No `snapshot_events()`
       clone is made, and no other tenant's event is ever cloned.
     - The scan's cost is still proportional to every geo entry in the process. That is today's
       cost, and the eviction specification records it.
   - **Limit.** An absent limit means 10,000, and a larger one is clamped to 10,000.
   - **Unchanged.** It does not hydrate, exactly as today, so it adds no eviction trigger.
   - **Known gap, not changed here.** `geo_index` is filled only on ingest
     (`store.rs:771,921,1022`), never on hydration or WAL recovery. A cold or recovered tenant
     therefore has no geo results, today and after S1.
   - **The legacy route** (`api.rs:196`, unused `api::serve`) gets the same handler.
6. **S0 profiles with jemalloc behind the new `heap-profile` feature** (S0 section).
7. **The harness** measures peak live heap on the dev host, over a deterministic in-process replay
   through Core's full v1 router, on a private clone of a synthetic fixture.
   - **Hard gates:**
     - byte-identical responses under both hashers;
     - byte-identical Parquet output;
     - ledger accuracy;
     - decode work;
     - an unchanged eviction policy;
     - the frozen tests.
   - **Not gated per candidate:** cold-load time. It is checked once, in the S1 report, against
     the `f4b53930` overlay (Acceptance 4).

## Context

Production at 2026-11-11T18:17Z (`prod-metrics-2026-11-11T1817Z.txt` in the handoff):

| Gauge | Value |
|---|---|
| Process RSS | 6,292,746,240 |
| Live heap (`cap::Cap<MiMalloc>`, `main.rs:2-3`) | 5,574,260,718 |
| Structures gauged (`store_resident.rs:36-64`) | 3,695,996,980 |
| of which events, 229,258 | 3,634,360,625 (15,852.7 B/event, stride-sampled) |
| `entity_versions` | 3,875,710 over 35,936 entries |
| `system_store` (`allsource_system_resident_*`) | 32,396,826 over 13,265 events |
| Live heap claimed by no structure | 1,878,263,738 |
| Budget counter / budget (`apps/core/fly.toml:33`) | 950,872,362 / 2,147,483,648 |
| `allsource_core_cache_evictions_total` | 0 |

**Resident events.** The hydrated tenants hold 201,785 / 16,840 / 5,484 / 3,447 / 1,662 / 28
events. Another 12 were pushed by ingest or WAL recovery.

**Longhand's count.** The brief's 201,806 is the read count; 201,785 is the count after dedupe by
id.

**Payload size.** The mean payload-plus-metadata size from the budget counter is 3,892 B.

**Overlapping plans.** The 8 GB rehearsal belongs to S2 in
[Fly cost reduction](2026-11-08-fly-cost-reduction.md). The S1 deploy uses today's checkpoint
restart ([Zero-downtime restart](2026-11-01-core-zero-downtime-restart.md)).

## Invariants

- **I1. One residency unit.** The ledger counts residency in measured heap bytes (decision 1). The
  eviction budget counter and the `system_store` length gauge are separate, labelled estimates.
- **I2. Every resident byte has an owner, within 10% either way.** The bound is two-sided:
  `|live heap − ledger total| ≤ 0.10 × live heap`. An over-count points to a missed release, and
  an under-count to a missing owner. It is measured in the replay and in production.
- **I5. Output does not change.**
  - **Candidate gate.** For every request in the replay set except geo, response bytes equal the
    oracle build's, under the deterministic and the production hasher, after the epoch
    normalization named in the harness.
  - **Deploy gate.** The S1 report compares the oracle with the `f4b53930` overlay on the replay
    set. A mismatch blocks the deploy.
  - **What the oracle differs from `f4b53930` in:**
    - three behaviour-neutral moves:
      - the Parquet schema and payload write move to `event_codec.rs`;
      - `HashState` routing;
      - `build_v1_router`, extracted from `serve_v1` (`api_v1.rs:151-566`);
    - two tie-break orderings:
      - `read_model_journal` sorted by `(version, id)`;
      - `by_file` as a `BTreeMap`;
    - an injectable `StorageConfig::file_namer`, whose default is unchanged;
    - an LRU counter seam, gated on `deterministic-hash` and a no-op in production;
    - `parse_cache_byte_budget`, extracted from `from_env` and `from_env_vars` with no change in
      behaviour;
    - in `Cargo.lock`, origin/main's dependency bump plus the manifest entries.

    The full `apps/core` suite passes at the oracle commit.
  - **Excluded responses.** Constraint changes 1 to 3. Geo has its own digest. The replay reaches
    neither the projection list nor a failing archive file.
  - **The overlay.** `f4b53930` plus the `build_v1_router` extraction, the two examples and the
    `cap` `stats` dev-dependency. It uses production hashing and its own lock.
- **I6. Rollback reads the volume.** The WAL and Parquet formats are unchanged. The schema and
  payload write live in the protected `event_codec.rs`.

**Constraint change.** The brief requires byte-identical API responses. S1 departs from that in
three places, and the brief records all three:

1. `entity_snapshots` disappears from `GET /api/v1/projections` and its admin routes.
2. `POST /api/v1/geospatial/query` returns only the caller's tenant, at most 10,000 results.
3. When two or more archive files fail in one feed read, the reported error is the first by file
   path. Before, it depended on hash order. The error text is unchanged.

## S0: heap attribution

- **Dependencies.** `heap-profile` adds three optional, Linux-only crates:
  - `tikv-jemallocator` 0.7, with `profiling`, `stats` and
    `unprefixed_malloc_on_supported_platforms`;
  - `tikv-jemalloc-ctl` 0.7;
  - `jemalloc_pprof` 0.9.

  `make` joins the Alpine builder (`apps/core/Dockerfile:26`).
- **Globals.** Each build declares exactly one global allocator:

  | Build | Global allocator |
  |---|---|
  | server, default | `cap::Cap<MiMalloc>` (`main.rs:2-3`) |
  | server, `heap-profile` | `tikv_jemallocator::Jemalloc` with `malloc_conf` `prof:true,prof_active:true,lg_prof_sample:19` |
  | `examples/heap_replay.rs`, default | its own `cap::Cap<System>`, as `examples/archive_read_heap.rs:15-16` does |
  | `examples/heap_replay.rs`, `heap-profile` | jemalloc, with the same `malloc_conf` |
- **Gauge.** `heap_live_bytes()` lives in each binary next to its global: `main.rs` and the
  example. It reads `stats.allocated` after an epoch advance under jemalloc, and
  `GLOBAL.allocated()` otherwise.
- **Route.** An admin-only `GET /api/v1/admin/heap-profile` exists only in `heap-profile` builds.
- **Producer.** The replay example is built for `aarch64-unknown-linux-musl` in a container from
  the Dockerfile's builder stage. The build is chunked as the Dockerfile's cooks are, each step
  under `timeout -k 15 290`, with the cargo cache mounted. It runs on the fixture.

  If the fixture's unattributed share differs from production's 33.7% by more than 5 points, the
  user decides whether to deploy one production `heap-profile` build. That build would emit only
  attribution numbers.
- **Report** (`docs/reports/2026-11-DD-core-heap-attribution.md`):
  - at least 90% of live heap attributed by site;
  - attribution mapped to S1 owners. Unattributed bytes are split per owner class (events, maps,
    rings, permanent), each as ledger-estimator error, plus bytes with no owner;
  - bytes held by `entity_snapshots`;
  - the cost per event of `resident_bytes()`;
  - the per-domain growth rate of `system_store`;
  - the keying and size of Prime projections;
  - the 8 GB projection, term by term (see Scope).
- **Fixture shape is an estimate.** No production stride sample is taken, because event content
  must not leave production. The fixture's sizes come from production gauges: per-tenant counts,
  and the 3,892 B mean from the budget counter, with an assumed log-normal spread. The S0 report
  states this. If the user chooses the production `heap-profile` deploy, that run may also emit a
  payload-size histogram, numbers only.

## S1

**Owners.** Each owner is charged where its bytes become resident, measured as decision 1
describes, and released where they stop being resident.

| Owner | Charged | Released |
|---|---|---|
| `events.resident` | at push (`store.rs:756,906,1030,1095`), splice (`:2288`), WAL recovery (`:494-496`) and token-compaction push (`:1444`) | eviction `retain` (`:2158`); token-compaction removal (`:1440-1442`) |
| `storage.pending` | at the pending push (`storage.rs:336-341`) | flush success |
| `index.*` | at insert | eviction's index rebuild (`store.rs:2177-2193`) |
| `feed_index` | at build | eviction (`store.rs:2159`) |
| `cache_generations`, `tenant_loader` entries | at insert | `tenant_loader` at eviction; `cache_generations` never |
| `read_models` | at insert | the LRU bound (`store_read_model.rs:150-157`) |
| `pipeline.windows`, `pipeline.state` (`pipeline.rs:150-157`) | at push or insert | `evict_window` (`pipeline.rs:183`, called at `:573`); pipeline reset, reached through `PUT /api/v1/pipelines/{pipeline_id}/reset` (`api_v1.rs:373-375`, `api.rs:1501-1511`, `pipeline.rs:687-688`), whose `StatefulOperator::clear` (`:190-193`) empties both |
| `webhook.queue` | at the per-webhook clone (`store.rs:1496-1505`) | delivery end, including retries (`webhook_worker.rs:35-50`) |
| `broadcast` (estimated owner) | The owner's value is the number of slots each ring holds × the running mean `resident_bytes()` of the events sent to it. The number of slots is counted from the send results each call site captures, since a send returns `Ok` only when a receiver exists (`store.rs:762,912,1027,1101`); the slot count rises on each `Ok` and caps at 1,024 (store ring) or 1,000 (websocket ring). A shared `Arc` sent to both rings counts in both. That overlap is bounded by 1,000 events, and the I2 tolerance absorbs it. This keeps the public `Arc<Event>` type unchanged (`store.rs:194,1165`; `websocket.rs:77,88-94`). | Counted only while a ring has receivers. The count returns to zero when the last receiver drops. |
| `projection.event_counters` | per new key | key removal |
| `query.*` | at materialization | request end |
| permanent: `entity_versions`, `version_index_*` (`store.rs:182,186`), `cache_generations` (`store.rs:206`), `geo_index`, `schema_evolution` history, `projection.entity_snapshots` and other embedded or Prime projections, the search service (`EventSearch` vector, hybrid and `indexer.indexed`, `event_search.rs:124-160,411`), `system_store` outside the Tenant domain's fold (`system_store.rs:94-100`), `misc.*` (`store.rs:128-229`) | at insert | never (`system_store` Tenant domain: `release_folded`) |
| fixed: `rate_limiter` (`RateLimiter` `buckets` and `custom_configs`, `rate_limit.rs:135-150`, built at `middleware.rs:142`), `metrics` (`MetricsRegistry` Prometheus histograms, `metrics.rs:17,173`). Found by the S0 report: 528,394 B and 524,480 B on the fixture | once at startup, by the measured constant; `rate_limiter` per new bucket key | never; `rate_limiter` at bucket removal (`rate_limit.rs:158`) |

The ledger measures and never blocks. A source-scan test pins that owner counters are changed only
through the ledger API.

**`entity_snapshots` callers:**

| Caller | Class | S1 change |
|---|---|---|
| `store.rs:305` registration | server | made conditional |
| `api.rs:3888,3903,3959,3971,4000`, `store.rs:5054-5056` | server tests | updated |
| `benches/event_ingestion.rs:174`, `tests/integration_tests.rs:349-366` | bench, test | call removed; expects `EntityNotFound` |
| `store.rs:2894` `get_snapshot` | no non-test caller | kept |
| NIF and embedded: `native/core_nif/src/lib.rs:190`, `tests/embedded_core_api.rs:341`, `tests/bidirectional_sync.rs:301-302`, `examples/embedded_quickstart.rs:54`, Elixir `core_nif.ex`, `core_embedded.ex`, `mcp_tools.ex` | embedded, live | unchanged (flag true) |
| Elixir `core_backend.ex:22` `get_snapshot` callback | shared behaviour: the NIF path (embedded) and `CoreClient` (HTTP fold, below) | unchanged |
| Elixir `ProjectionSync`, `EventPipeline` | dead: started only by tests; their target routes are not served (`api_v1.rs:377-395`) | unchanged |
| Elixir `core_client.ex:129-130` `get_snapshot` | live; calls `/api/v1/entities/{id}/snapshot`, which folds events (`api.rs:900-927`) | unchanged |
| Docs and content: `apps/web/content/*.mdx`, `apps/mcp-server-elixir/README.md`, changelogs, `apps/query-service/.../event_pipeline_broadway.ex:1` (module name), `catalog.ex:11` (comment) | non-code | unchanged |

**Metrics:**

- `allsource_heap_unattributed_bytes`, signed;
- `allsource_core_ledger_owner_bytes{owner}`;
- `allsource_core_permanent_bytes`;
- `allsource_core_rows_decoded_total`;
- the existing `allsource_core_cache_evictions_total` (`metrics.rs:406-408`), cross-checked against
  ledger releases.

**Unchanged in S1:**

- eviction (decision 2);
- whole-tenant residency;
- ingest order;
- hydration;
- the flush paths;
- delivery cursors;
- geo's resident-only reads;
- every API except constraint changes 1 to 3.

## Harness

**Hosts.**

- The evaluators, the replay and the ceilings step run on `aarch64-apple-darwin`.
- The S0 profile runs on `aarch64-unknown-linux-musl`, in the builder container.

**Commits, in order:**

1. **Manifest commits:**
   - `heap-profile`;
   - the `stats` feature on `cap`, added as a dev-dependency entry (`cap` is already a normal
     dependency at `apps/core/Cargo.toml:122`);
   - `deterministic-hash`;
   - `[profile.heap-replay]`;
   - `tooling/core-heap-replay` in `workspace.exclude`, with its own lock.
2. **Oracle commit:**
   - the I5 moves, the orderings, the `file_namer` seam, the `parse_cache_byte_budget` extraction,
     and the `deterministic-hash`-gated LRU counter seam (`tenant_loader.rs:112,173`), which is a
     no-op in production;
   - the harness crate, `heap_fixture` and `heap_replay`;
   - `expected-digest.txt` and `expected-codec-digest.txt`;
   - `expected-evictions.txt`, the evicted-tenant sequence of the eviction scenario.
3. **S0 commit:** the S0 code and its report.
4. **S1 structural commit,** gated on the S0 report (see Scope):
   - the ledger;
   - the `entity_snapshots` and geo changes;
   - `tests-S1.txt`: every test path from `cargo test --list`, under default features and under
     the embedded set that `ci.yml:487` runs;
   - `expected-geo-digest.txt`;
   - the eviction-policy digest.
5. **Baseline commit:**
   - `ceilings-S1.json`;
   - `.autoresearch/` in `.gitignore`;
   - `program.md`;
   - `autoresearch.toml`, whose literal arguments carry the tests digest and the eviction-policy
     digest.

**Builds.**

- Every build uses `--target-dir <abs>` outside the worktrees:
  `cargo build --profile heap-replay -p allsource-core --example heap_replay --features community,analytics[,deterministic-hash] --locked --offline --target-dir <abs>`.
- `--oracle-bin` is a pinned directory beside the target dir, built once at the oracle commit. It
  holds `heap_fixture` and the two evaluators.
- Run outputs go under `artifact_directory`.
- No two builds run at once.

**Fixture.**

- `shape.toml` holds numbers only:
  - the six tenants' counts, with the largest pinned at Longhand's 201,785;
  - the sizes described in the S0 section.
- `heap_fixture` is pinned by SHA-256 and writes through `event_codec.rs`. `verify` regenerates the
  fixture with the same binary.
- Each replay process works on its own APFS clone.

**Determinism.**

- **Hashing.** `deterministic-hash` routes every replay-path map through `HashState`, with a fixed
  hasher and 32 shards.
- **Source scan.**
  - It bans these unless they are routed through `HashState`:
    - `HashMap` and `HashSet` `::new`, `::with_capacity` and `::default`;
    - `DashMap::new`, `::with_capacity` and `::default`;
    - `RandomState`, `with_hasher` and `with_capacity_and_hasher`.
  - It covers `apps/core/src/` `store.rs`, `store_feed.rs`, `store_read_model.rs` and
    `store_resident.rs`; `infrastructure/persistence/index.rs` and `storage_feed.rs`; and
    `application/services/projection.rs`. From step 4 on it also covers
    `infrastructure/memory/`.
  - It fails if a listed path is missing or matches zero files.
  - Exempt: `store.rs:398` (lookup only) and `store.rs:2980,3008,3044-3045` (outside the replay
    set).
- **Production hasher.** That leg replays without the feature.
- **Runtime.** The replay runs on a current-thread Tokio runtime. It starts no
  `checkpoint_at_boot` and no `spawn_memory_upkeep`. The driver and the generator use no clock, no
  random id and no timer.
- **Clock and randomness sources inside Core:**

  | Source | Location | Handling |
  |---|---|---|
  | LRU touch time | `Instant::now`, `tenant_loader.rs:112,173` | Under `deterministic-hash` it becomes a monotonic counter, a seam added in the oracle commit, so victim order is reproducible. |
  | Feed epoch | `Uuid::new_v4`, `store_feed.rs:87`; the persistent feed log, `feed_log.rs:389` | handled by epoch normalization (below), for feed-page cursor and head |
  | Read-model access time | `Instant::now`, `store_read_model.rs:209,409` | The replay holds one model per tenant, at most seven, against eviction at 32 models. The driver asserts that summed model bytes stay under 64 MiB, so neither `bound_cache` trigger fires (`:160-167`). The access stamps are not digested. That is safe, because they feed only `evict_model`, which the replay never reaches. |
  | Read-model commit event | random id and `Utc::now` (`event.rs:94,99`) inside `MODEL_EVENT` (`store_read_model.rs:365-371`) | excluded from digests (below) |
  | Flush file stem | `storage.rs:553-566` | handled by `file_namer` |
  | Month partition directory | `storage.rs:1839-1845` | never appears in a digested response or in Parquet bytes, so it is harmless |
- **Seeded events.** A dedicated tenant, `replay-ingest`, receives 2,000 events through the store
  API, with fixed ids and timestamps. These sit outside the pinned shape.
  - Ingest never marks a tenant loaded.
  - The budget counter does not see ingested bytes, while the ledger does.
- **Uniqueness.** `(valid_from, timestamp, version)` is unique per tenant.
- **Epoch normalization.**
  - **Requests use raw epochs.** The read-model commit's checkpoint carries the epoch exactly as
    the feed head returned it (`store_read_model.rs:329-331`), so Core accepts it.
  - **Only the digest is normalized.** Before hashing a response, the driver asserts that
    `FeedPage.cursor.epoch == FeedPage.head.epoch` and that the value is below 2^52. It then
    replaces both with `0`.
  - **Requests are never digested.**
  - **Where epochs and model events can appear.**
    - In the replay set, digested bodies carry an epoch only in `FeedPage.cursor` and
      `FeedPage.head`. The commit response (`read_model_api.rs:225`) carries none.
    - Event queries in the replay set filter by `entity_id`, `event_type` or a time window that
      excludes `MODEL_EVENT`. The driver asserts that no digested event-query body contains a
      `MODEL_EVENT`.
    - The driver records each request's status. The `replay_expected_status` gate fails the run if
      any request's status differs from its expected status. That includes a 409
      `VersionConflict`, which would carry a raw epoch.
- **No `as_of_valid`.** No replay request sets it.

**Scenarios.**

- **Main** (objective and digests), at today's 2 GiB budget, so nothing is evicted:
  1. Boot.
  2. Ingest the seeded events.
  3. Pass A; the first query per tenant is that tenant's cold load.
  4. Flush at a fixed step.
  5. Pass B, warm.
- **Eviction** (`ledger_counts_only`). This scenario is driven by the budget alone.
  - It runs with `ALLSOURCE_CACHE_BYTES` set to half the fixture's estimated total and queries
    tenants in a fixed order, so `enforce_cache_budget` evicts.
  - Its evicted-tenant sequence must equal `expected-evictions.txt`.
    - The oracle build (`deterministic-hash`, with the LRU counter seam) produces that file, and
      `evictions_match` compares against it.
    - Equivalence with `f4b53930` is shown once, in the S1 report (Acceptance 4). The overlay
      runs a spaced variant of the scenario on production timing (`Instant::now`). It sleeps
      10 ms between tenant queries and asserts that consecutive touch instants are strictly
      increasing. It must produce the same sequence.
  - The headroom branch is not exercised on darwin (decision 2).

**Measurements.**

- `peak_live_bytes`: `max_allocated` as a `u64` in bytes. The default build asserts it is non-zero.
- `steady_live_bytes` and `ledger_bytes`: read after pass B.
- `rows_decoded`: summed over the run.
- `timing.json`: holds the cold load of the largest tenant.

**Replay set,** run per tenant in this fixed order, in both passes:

1. event queries, filtered by entity id, by event type or by a time window that excludes
   `MODEL_EVENT`, including a 10,000-row page;
2. entity state and snapshot for 20 seeded entities;
3. one geo query with body `{"bbox": <fixture bbox>}` and no limit, so the default of 10,000
   applies;
4. one event-feed page with no cursor;
5. a read-model commit built from that page's head. This is last, so no later query in the pass
   can see its `MODEL_EVENT`.

Routes on the skip list get an explicit `tenant_id`; the others get the tenant's key.

**Digests.**

- `query_digest` hashes (method, path, status, body) for each non-geo request, after epoch
  normalization.
- `geo_digest` hashes the geo request the same way.
- `codec_digest` hashes the Parquet bytes for a fixed 1,000-event set.

**Gates and metrics:**

| Name | Kind |
|---|---|
| `build_ok` | hard gate |
| `replay_repeatable` (two processes: identical byte values, `rows_decoded` and digests) | hard gate |
| `query_digest_equal`, `prod_hasher_digest_equal`, `codec_digest_equal`, `geo_digest_equal` | hard gates |
| `ledger_two_sided` (`\|steady − ledger\| ≤ 0.10 × steady`) | hard gate |
| `rows_decoded_ok` (at or under the ceiling) | hard gate |
| `evictions_match` (the eviction scenario's sequence equals `expected-evictions.txt`) | hard gate |
| `replay_expected_status` (every request returns its expected status) | hard gate |
| `eviction_policy_diff_allowed` (evaluated once, at step 4; see decision 2) | step-4 check |
| `eviction_policy_unchanged` (SHA-256 over each decision-2 item, extracted whole with `syn`, equals the value frozen at step 4) | hard gate |
| `tests_pass`, `expected_tests_ran`, `tests_unchanged`, `protected_paths_exist`, `tooling_tests_pass` | hard gates |
| `peak_live_bytes` | objective, minimize, in bytes |
| steady, ledger and unattributed bytes; rows decoded; tests executed; production-hasher peak | diagnostics |

**Ties and the run ledger.** A tie-kept candidate is never reported as an improvement. The run
ledger (`docs/reports/2026-11-DD-core-memory-s1-run-ledger.md`) records a `decision_reason` for
every candidate.

**Evaluator contract.**

- **Output.** JSONL v1, with one envelope on stdout.
- **Processes.** Each child runs in its own process group and writes to files. A 270 s internal
  deadline runs `/bin/kill -KILL -- -<pgid>`. The manifest timeout is 290 s.
- **Environment.** `PATH`, `HOME`, `CARGO_HOME` and `RUSTUP_HOME` are set explicitly from literal
  arguments, along with `ALLSOURCE_MEMORY_RESERVE_BYTES`.
- **Failures.** Every failure, panics included, emits sentinels and exits 0.
- **Self-check.** The evaluator checks its own SHA-256 against a literal argument.
- **Gate details.** They never contain paths, durations or PIDs.

**Manifest:**

| Setting | Value |
|---|---|
| Objective | `peak_live_bytes`, minimize |
| Budget | `max_candidates = 8`, `max_failures = 3`, `wall_clock_seconds = 14400`; the operator tracks the last two in the run ledger |
| Mutable | `apps/core/src` |
| Protected | `apps/core/src/infrastructure/persistence/archive_output.rs`, `wal.rs`, `event_codec.rs` |
| Agent | `git status --short`, timeout 60 |
| Authority | `allow = []` |

`program.md` forbids changes to dependencies, `Cargo.toml` and the eviction policy.

**A6.** Each evaluator must finish within 270 s:

- `heap_replay`: two builds, two replays, one production-hasher replay, the eviction scenario, and
  fixture regeneration under `verify`;
- `core_tests`: the test build, the full test list and the tooling tests.

The ceilings step measures every phase, plus S0 fixture generation and the container build steps.
If a phase does not fit, the fallbacks are a separate build evaluator, a 25% fixture for the tenants
other than Longhand, and a split test list. The limit is never raised.

## Acceptance tests carried into the Plan

- **`ledger_owns_heap`:** with the two-sided 10% bound, after each of:
  - ingest on all four paths;
  - hydration;
  - today's eviction and rehydration;
  - embedded open;
  - WAL recovery;
  - token compaction;
  - follower apply;
  - webhook dispatch.
- **`ledger_counts_only`:** the eviction scenario's sequence matches `expected-evictions.txt`, and
  the ledger never refuses.
- **`broadcast_charging`:** the ledger stays within its bound in four configurations: store ring
  only, websocket ring only, both, and a websocket receiver that connects and drops during the
  run.
- **`index_vec_growth`:** after many pushes onto one existing index key, that key's charge equals
  `Vec<IndexEntry>::capacity() × size_of::<IndexEntry>()` exactly.
- **`pipeline_reset_releases`:** `PUT /api/v1/pipelines/{id}/reset` returns `pipeline.windows`
  and `pipeline.state` to zero in the ledger.
- **`geo_no_foreign_clone`:** for a geo query whose candidates span several tenants, the resolver
  clones only the caller's events, which a clone counter confirms.
- **`resident_bytes_accuracy`:** `Event::resident_bytes()` is within 10% of the `cap` allocation
  delta for constructed events across the fixture's size distribution.
- **`server_projections_list`:** server mode omits `entity_snapshots`; embedded mode keeps it.
- **`tenant_scope_geo`:**
  - no cross-tenant events;
  - no `snapshot_events()` clone;
  - candidate ids resolve through `index.get_by_id`, and only events of the caller's tenant are
    materialized (the candidate map itself still spans every tenant, as decision 5 states);
  - an absent limit returns at most 10,000, and a larger limit is clamped;
  - a cold tenant returns what it returns today.
- **`tie_break_orderings`:** unit tests for the journal and `by_file` orders.
- **`heap_profile_nonempty`:** a 64 MiB allocation moves `heap_live_bytes()` under `heap-profile`,
  and both producers emit a non-empty profile.
- **`release_build_has_no_heap_profile`:** `cargo tree` for the release features contains no
  jemalloc crate.
- **`rollback_volume`:** a volume written by this branch boots under `f4b53930`.
- **Harness tests:**
  - `replay_two_runs_one_volume`;
  - `evaluator_failure_paths`: deadline, output over 1 MiB, panic;
  - `worktree_stays_clean`;
  - `gate_negative_controls`, with one control per hard gate, plus one per item digested by
    `eviction_policy_unchanged`, each named in the test;
  - `scan_paths_exist`;
  - source scans for ledger mutation and hash constructors.
- **`tenant-isolation-check`** passes.

## Input for the eviction specification

Each subsystem below assumes that eviction never happens. The reviews found every one, and each
stays latent today only because eviction does not fire.

| Subsystem | Problem under routine eviction | Found in |
|---|---|---|
| Budget counter | counts 256 + JSON length, about 4× under real bytes | brief |
| Reclaim | a refused victim ends the pass; refusal on pending events and on `storage.try_write`; self-deadlock under store locks | r1, r2 |
| Readers | the warm path reads unpinned; the feed, list and stats routes, and `query()`, drop their guard before reading | r2, r12, r14, r15 |
| Ingest order | index and projections run before a storage append that can fail | r2, r3 |
| Pending data | `not_durable` races `flush_tenant`; no periodic drain without the WAL | r3, r5 |
| Derived events | token compaction merges unflushed tokens and needs its tenant pinned | r9, r15 |
| `event_counters` | counts twice after evict and rehydrate | r1, r3 |
| Feed epochs | eviction changes the epoch, so old cursors are refused | r8 |
| Delivery cursors | consumer, websocket and search cursors are `Vec` offsets; the WAL sequence resets on truncate | r14, r15 |
| Search index | rehydrated events are not reindexed, and `indexer.indexed` is never pruned | r16 |
| Geo index | never filled on hydration or recovery | r17 |
| Hydration | decodes the whole tenant under `storage.read`; compaction and repack race the file list; restarts exhaust the archive budget | r2, r3, r9 |
| Followers | cannot evict, so replication refusal needs a resume path | r3, r5 |
| `system_store` | non-tenant domains never fold | r10, r12 |

## Direction: S2–S4 and eviction (not approved here)

- **S2: canonical-text payloads and the 8 GB rehearsal.** This is the first measurement of the
  8 GB claim.
- **The eviction specification,** for keeping memory bounded as history grows.
- **S3: Parquet-derived segments.**
- **S4: no full-history reads.**

## Alternatives rejected

- **Routine eviction in S1.** It reaches every in-memory subsystem (r1 to r16).
- **DataFusion's pool as the ledger.** It pulls `object_store` into the default build.
- **A weighted cache crate.** It would be a second count of the same bytes.
- **Per-tenant `event_counters`.** `CLAUDE.md` forbids it.
- **A fixed hasher in production.** It loses resistance to hash flooding.
- **Geo hydrating cold tenants.** It would add an eviction trigger and a cold-load cost.
- **One approval for S0–S4.** The stages depend on each other and on S0's numbers.

## Security and privacy

- The repo holds numbers only, and no event content leaves production.
- Tenant names and counts are operational metadata. Only Longhand, our own product, is named. The
  other five hydrated tenants are not identified as ours. The route that hydrated them is not
  verified. The [Fly cost reduction](2026-11-08-fly-cost-reduction.md) plan (line 42) records
  4 of 22 registered tenants making authenticated requests, all of them ours. Skip-listed routes
  (`infrastructure/security/middleware.rs:40-63`) can hydrate without authentication.
- The heap-profile route exists only in `heap-profile` builds and is admin-only.
- The geospatial cross-tenant read is fixed in the first deploy. Until then, any internal key
  holder can read every tenant through it.
- Skip-listed data-plane routes trust the gateway's `tenant_id`. That is by design and stays
  unchanged.

## Confidence

| Piece | Tier | What moves it up |
|---|---|---|
| S0 profile | sand | `heap_profile_nonempty` in the musl builder |
| S0 attribution ≥ 90% | water | the first profile; S1 is conditional on it |
| `resident_bytes()` accuracy | sand | `resident_bytes_accuracy` |
| Ledger accuracy (I2) | sand | `ledger_owns_heap`; the production gauge |
| Eviction unchanged | sand | `ledger_counts_only` with `evictions_match`; `eviction_policy_unchanged` |
| `entity_snapshots` | rock | the caller table; `server_projections_list` |
| Geo tenant scope | sand | `tenant_scope_geo` and `geo_no_foreign_clone` |
| Harness determinism | sand | the harness tests |
| Evaluators within 290 s | sand | the ceilings timings |
| 8 GB without eviction after S2 | water | the S0 projection, then the S2 rehearsal |

## Acceptance

1. At the oracle commit, the build and the full `apps/core` suite pass. The diff against
   `f4b53930` holds only:
   - the I5 changes in `apps/core/src`;
   - the lock and manifest changes;
   - the harness additions;
   - the docs.
2. The S0 report attributes at least 90% of live heap, maps it to the S1 owners, and states the
   8 GB projection term by term. S1 starts only after it.
3. `autoresearch doctor` reports ready, and every gate passes at the S1 baseline. The run ends
   with `verify: matched` on a kept candidate, or with a report that none was kept. Tie-kept
   candidates are never reported as improvements.
4. The S1 report records the following, all on the dev host:
   - the ceilings and phase timings;
   - five cold-load runs each of the overlay and of S1, over which:
     - each set's band, (max − min) / median, must be at most 10%, or the run stops;
     - the S1 median must be at most the overlay median × (1 + the larger band);
   - the overlay digest comparison;
   - the overlay's spaced eviction scenario, whose sequence must equal `expected-evictions.txt`;
   - the production-hasher peak.

   The production 14.2 s figure is not checked here.
5. After the S1 deploy, on the 16 GB machine with the budget unchanged. The user decides every
   action below.
   - **Unattributed bytes.** The ledger gauge must satisfy
     `|allsource_heap_unattributed_bytes| / allsource_heap_live_bytes ≤ 0.10`.
     - If it does not, nothing rolls back, because the ledger only counts.
     - The S1 report names the gap. Adding owners is design drift: a revision, review and
       re-approval before the PR leaves draft.
   - **Projections and geo.** `entity_snapshots` must no longer be listed, and geo must be
     tenant-scoped.
     - If either fails, it is a bug, fixed on the branch and redeployed before the PR leaves draft.
       Geo's cross-tenant read is security-relevant, so a geo failure is fixed first.
   - **Evictions.** `allsource_core_cache_evictions_total` is recorded. Today's baseline is 0.
     - A "rise" means any increase within seven days of the deploy, with the budget still
       2,147,483,648 (`apps/core/fly.toml:33`).
     - On a rise, the user decides on a rollback to `f4b53930`. The rollback is recommended,
       because a rise means eviction behaviour has changed against decision 2. The cause is then
       investigated.
   - **Recorded only:** live heap and RSS, against 5.57 GB and 6.29 GB.
   - **New metrics** are exported.
6. A volume written by this branch boots under `f4b53930`.

## Delivery

- **Order:**
  1. manifest commits;
  2. oracle;
  3. S0, then its report, which is the gate for S1 (an amendment means drift and re-approval);
  4. S1 structural;
  5. baseline;
  6. the S1 run.
- **Deploy.** S1 deploys from the branch head in the morning window, only on the user's go. No
  `fly.toml` change.
- **Records.** Numbers go into `docs/reports/`, with tree hashes.
- **Collapse.** Commits are collapsed after the last autoresearch command and after the deploy, and
  the gates re-run on the collapsed head.
- **Branch and PR.** The single PR comes from the user's explicit request on 2026-11-12, an
  exception to the repo's commit-to-main default. The branch carries one commit per author before
  the PR leaves draft. It stays draft until the feature works end to end. Nothing merges without
  the user.
