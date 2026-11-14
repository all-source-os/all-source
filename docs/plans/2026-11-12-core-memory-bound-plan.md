# Core memory bound: plan

Design: [docs/plans/2026-11-12-core-memory-bound-design.md](2026-11-12-core-memory-bound-design.md)
(approved at v22, sha256 8d09426f18e446ccf4116d9f5ea781a78d614caae10390f34da871febfa0af00).

End-to-end job when done: the operator runs the `heap_replay` evaluator on the dev host and reads
`peak_live_bytes`, a ledger total within 10% of live heap and every hard gate passing; the deployed
S1 build exports `allsource_heap_unattributed_bytes` within 10%, no longer lists
`entity_snapshots`, and answers geo queries for the caller's tenant only.

### Quality gates

#### Epic-level (once, when every story is done)

- `make -C apps/core ci` (clippy `-D warnings`, cargo-sort, `cargo test --lib`, build)
- `cargo test -p allsource-core` under default features and under the embedded feature set that
  `ci.yml` runs
- `tenant-isolation-check` passes
- `autoresearch doctor` reports ready at the baseline commit

#### Story-level

- Backend stories: the named test runs and passes; route stories call the route in an
  integration test and check status and body.
- Harness stories: the evaluator emits one JSONL v1 envelope within 270 s.
- Every command runs under `timeout -k 15 300`; one build on the machine at a time.

### Testing trophy

Layers: unit + contract + integration. No frontend. Every behaviour below blocks merge.

#### Unit
- [x] [Logic] `resident_bytes_accuracy` (file: `apps/core/tests/resident_bytes.rs`)
- [x] [Logic] `index_vec_growth` (file: `apps/core/src/infrastructure/memory/ledger.rs` test module per repo convention, else `apps/core/tests/ledger.rs`)
- [x] [Logic] `tie_break_orderings` (file: `apps/core/src/store_tie_break_tests.rs`, lib suite)
- [x] [Logic] harness `evaluator_failure_paths`, `scan_paths_exist`, source scans (file: `tooling/core-heap-replay`)

#### Contract
- [x] [Handler] `server_projections_list`: server omits `entity_snapshots`, embedded keeps it
- [x] [Handler] `server_admin_routes_do_not_expose_entity_snapshots`: get, delete, reset, pause, start on `entity_snapshots` are 404 on the server router
- [x] [Handler] `tenant_scope_geo`: caller tenant only, default and clamped limit 10,000
- [x] [Handler] `pipeline_reset_releases`
- [x] [Build] `release_build_has_no_heap_profile`, `heap_profile_nonempty`

#### Integration
- [x] [Flow] `ledger_owns_heap` (embedded open in `ledger_owns_heap_embedded`, `--features embedded`) across ingest, hydration, eviction, embedded open, WAL recovery, token compaction, follower apply, webhook dispatch
- [x] [Flow] `ledger_counts_only` with `evictions_match`
- [x] [Flow] `broadcast_charging` in four ring configurations
- [x] [Flow] `geo_no_foreign_clone`
- [x] [Flow] `rollback_volume`
- [x] [Flow] `replay_two_runs_one_volume`, `worktree_stays_clean`, `gate_negative_controls`

Block merge: all of the above. Block release: Acceptance 5 production checks (owner's go).
Follow-up allowed: none.

### Stories

#### US-001: Manifest commits [Schema]
As the harness author, I want every dependency and feature in place before any baseline, so that
autoresearch candidates never touch `Cargo.toml` or the lock.

- [x] `apps/core/Cargo.toml`: `heap-profile` (Linux-only optional `tikv-jemallocator` 0.7 with
  `profiling,stats,unprefixed_malloc_on_supported_platforms`, `tikv-jemalloc-ctl` 0.7,
  `jemalloc_pprof` 0.9), `deterministic-hash`, `cap` `stats` dev-dependency,
  `[profile.heap-replay]`
- [x] root `Cargo.toml`: `tooling/core-heap-replay` in `workspace.exclude`
- [x] `make` in the Alpine builder (`apps/core/Dockerfile`)
- [x] `cargo tree -p allsource-core -e normal --features <release set>` shows no jemalloc crate
- [x] `cargo build -p allsource-core --locked` green; one commit

#### US-002: Oracle neutral moves [Backend]
As the harness, I want output to be reproducible, so that response digests can gate candidates.

- [x] Test first: `tie_break_orderings` (journal `(version, id)`, `by_file` `BTreeMap`)
- [x] Parquet schema and payload write moved to `event_codec.rs`; `HashState` routing under
  `deterministic-hash`; `build_v1_router` extracted from `serve_v1`
- [x] `StorageConfig::file_namer` seam, default unchanged; LRU counter seam under
  `deterministic-hash`; `parse_cache_byte_budget` extracted
- [x] Full `apps/core` suite green; diff against `f4b53930` holds only the listed moves

#### US-003: Harness crate, fixture and replay [Integration]
As the operator, I want a deterministic replay with digests and evaluators, so that every stage
is measured the same way.

- [x] Tests first: `replay_two_runs_one_volume`, `evaluator_failure_paths`, `scan_paths_exist`,
  hash-constructor source scan
- [x] `tooling/core-heap-replay` with its own lock; `shape.toml` numbers only; `heap_fixture`
  writes through `event_codec.rs`
- [x] `examples/heap_replay.rs` with main and eviction scenarios, epoch normalization, status
  recording
- [x] `expected-digest.txt`, `expected-codec-digest.txt`, `expected-evictions.txt` committed
- [x] Each evaluator finishes under 270 s on the dev host

#### US-004: S0 jemalloc profiling [Backend]
As the operator, I want heap attribution by allocation site, so that S1's owner table is checked
against real bytes.

- [x] Tests first: `heap_profile_nonempty`, `release_build_has_no_heap_profile`
- [x] One global allocator per build as the design's table; `heap_live_bytes()` beside each global
- [x] Admin-only `GET /api/v1/admin/heap-profile` in `heap-profile` builds only
- [x] Linux musl build of `heap_replay` in the builder container, each step under
  `timeout -k 15 290`, driven by `tooling/core-heap-replay` `heap_profile_run`. The container
  uses the builder image's stable 1.94 toolchain, as the production Dockerfile does; design.md:399
  governs the host autoresearch builds, not this one.

#### US-005: S0 profile run and report [Integration]
As the owner, I want the S0 report, so that S1 starts on measured numbers.

- [x] `docs/reports/2026-11-DD-core-heap-attribution.md` with every item the design's S0 report
  lists, the 8 GB projection term by term, labelled measured or estimated. `system_store`
  per-domain growth stays unmeasured: on 2026-11-13 the owner dropped the production
  heap-profile deploy that would have measured it
- [x] ≥ 90% of live heap attributed and every owner found is in the S1 owner table (96.7%; owners
  `rate_limiter` and `metrics` are v22 rows, design line 320); otherwise the flow goes back to
  Spec (`drift`)

#### US-006: Ledger and event measurer [Backend]
As the operator, I want one counter of resident bytes, so that unattributed heap is visible.

- [x] Tests first: `resident_bytes_accuracy`, `index_vec_growth`, ledger-mutation source scan
- [x] `apps/core/src/infrastructure/memory/ledger.rs` `ResidentLedger`; `Event::resident_bytes()`
- [x] Metrics `allsource_heap_unattributed_bytes`, `allsource_core_ledger_owner_bytes{owner}`,
  `allsource_core_permanent_bytes`, `allsource_core_rows_decoded_total`

#### US-007: Owner charge and release wiring [Backend]
As the operator, I want every owner in the table charged and released, so that I2 holds.

- [x] Tests first: `ledger_owns_heap`, `ledger_counts_only`, `broadcast_charging`,
  `pipeline_reset_releases`
- [x] Every row of the design's owner table wired; eviction policy items gain only ledger calls
- [x] `expected-geo-digest.txt` committed; `heap-replay-eval --fixture-dir <heap_fixture output> --repo <worktree> --artifacts <dir> --verify-fixture` passes every gate, `geo_digest` `a5f0fa5a9404989eaf09dfeeb56fa87a26d29aea5605fafa6aa4929c4d78794f`
  Run record: `docs/reports/2026-11-13-core-memory-bound-us007-eval.jsonl` (`status` pass, 12 of 12 gates).
  Fixture: `heap_fixture --shape tooling/core-heap-replay/shape.toml --out <fixture>`; its `fixture.json`
  sha256 `7841288a8a971ef59a0df19749439e58cbbefac2ab5765c1e51f5f5b68b70d73`, regenerated byte-equal by
  `--verify-fixture`. Replay test: `env HEAP_REPLAY_BIN=<artifacts>/bin/heap_replay-deterministic
  HEAP_FIXTURE_DIR=<fixture> cargo test --manifest-path tooling/core-heap-replay/Cargo.toml --locked --test replay_two_runs_one_volume`
  passes (1 passed, 14.17s).

US-007 owner audit (design owner table, line 306 onward; file:line at commit of this table):

| Design row | Charge | Release |
|---|---|---|
| `events.resident` | `store.rs:556` (WAL recovery), `:819`, `:968`, `:1090`, `:1157` (push), `:1538` (token compaction push), `:2407` (splice) | `store.rs:1531` (token compaction), `:2273` (eviction `release_matching`) |
| `storage.pending` | `storage.rs:323`, `:353` | `storage.rs:490-491` (flush success) |
| `index.*` | `index.rs:270-304` (`push_entry`, `charge_table`) | `index.rs:257-261` (`clear`); `store.rs:2296` (eviction rebuild) |
| `feed_index` | `store_feed.rs:123`, `:221`, `:251`, `:262`, `:436`; `store_feed.rs:272` rebuild | `store_feed.rs:246`; `store.rs:2276` (eviction `forget`) |
| `cache_generations` | `store.rs:2253` | never (permanent) |
| `tenant_loader` entries | `tenant_loader.rs:149`, `:216`, `:224`, `:255`, `:295` | `tenant_loader.rs:240-248` (`mark_unloaded`, `forget`) |
| `read_models` | `store_read_model.rs:351`, `:382`, `:416` (`set_to`) | same calls, on shrink (LRU bound) |
| `pipeline.windows`, `pipeline.state` | `pipeline.rs:176-178` (state), `:188` (windows) | `pipeline.rs:214` (`evict_window`); `:221`, `:223` (`clear`, reset) |
| `webhook.queue` | `store.rs:1596` (`Lease::new`, field `store.rs:257`) | `Lease` drop when `webhook_worker.rs` delivery task ends |
| `broadcast` | `store.rs:1087`, `:1206-1209` (store ring `observe`); `websocket.rs:92-95` | `observe(delivered, bytes)` at `store.rs:1209`, `websocket.rs:95` |
| `projection.event_counters` | `projection.rs:245`, `:286` (`ensure`) | `projection.rs:268`, `:284` (`clear_owned`) |
| `query.*` | `api.rs:664`, `:713`, `:1835`, `:1887`, `:1984`, `:2092` (`Lease::for_events`) | `Lease` drop at request end |
| permanent `entity_versions` | `store.rs:1221` | never |
| permanent `version_index_entities`, `version_index_tenants` | `store.rs:1867`, `:2088` | never |
| permanent `geo_index` | `geospatial.rs:166`, `:175` | never |
| permanent `schema_evolution` history | `schema_evolution.rs:279` (`Tally`) | never |
| permanent `projection.entity_snapshots` and Prime projections | `projection.rs:103`, `:106`, `:137`; `prime/projections/charge.rs:14` and its callers | never |
| permanent search (`EventSearch`) | `event_search.rs:169`, `:421` | never |
| permanent `system_store` outside Tenant | `system_store.rs:58`, `:70`, `:243` | `system_store.rs:324-353` (`release_folded`, Tenant domain only) |
| permanent `misc.*` | `store.rs:446`, `:1440`; `webhook.rs:133-141`; `schema.rs:180-189`; `pipeline.rs:749`, `:759`, `:827`; `consumer.rs` (`consumers_charge`); `replay.rs:155`, `:218`, `:220`; `exactly_once.rs:107-118` | `replay.rs:479`; `pipeline.rs:827`; `consumer.rs` `remove_entry` |
| fixed `rate_limiter` | `rate_limit.rs:155` (startup), `:172`, `:199` | `rate_limit.rs:183`, `:249` |
| fixed `metrics` | `store.rs:418` (startup constant, once per `EventStore`) | never |

Eviction policy (`store.rs:2235-2300`): the only additions over the baseline are ledger calls
(`cache_generations_charge.ensure`, `events_resident.release_matching`, `feed_indexes_charge.forget`).
Baseline lines (`events.retain`, `feed_indexes.remove`, `index.clear`) are kept unchanged.

Consumer inserts (`register`, `get_or_create`, `ack`, ACK replay) charge through `MapCharge::upsert`
under one entry guard (`consumer.rs`); `MapCharge::insert` uses the same guard. Contention check:
`apps/core/tests/map_charge_contention.rs`.

#### US-008: `entity_snapshots` out of server mode [Backend]
As the operator, I want the server not to hold `entity_snapshots`, so that its bytes are freed.

- [x] Test first: `server_projections_list`
- [x] `EventStoreConfig::register_entity_snapshots`, server default false, embedded true; callers
  updated per the design's caller table

#### US-009: Geospatial tenant scope [Backend]
As a tenant, I want geo queries to return only my events, so that no other tenant reads them.

- [x] Tests first: `tenant_scope_geo`, `geo_no_foreign_clone`
  Disclosure: tests and implementation landed together in c10b26d8; no commit records the red state.
- [x] Tenant from `AuthContext`; body `tenant_id` ignored; four-step resolver; limit default and
  clamp 10,000; legacy route shares the handler
  Gate run on the tree at 2262e370: `heap-replay-eval --verify-fixture` 12 of 12 gates pass,
  `geo_digest` `a5f0fa5a9404989eaf09dfeeb56fa87a26d29aea5605fafa6aa4929c4d78794f`;
  `replay_two_runs_one_volume` 1 passed (7.59s).
  Reconciled run at b036d2b2 (code identical to 2262e370): `heap-replay-eval --verify-fixture` status
  pass, 12 of 12 gates, `geo_digest` `a5f0fa5a9404989eaf09dfeeb56fa87a26d29aea5605fafa6aa4929c4d78794f`,
  `peak_live_bytes` 1301855656 (measured; not byte-stable across runs); `replay_two_runs_one_volume`
  1 passed (7.75s).

#### US-010: S1 structural gates [Integration]
As the harness, I want the S1 structure frozen, so that candidates cannot change policy or tests.

- [x] `eviction_policy_diff_allowed` passes once; eviction-policy digest frozen
- [x] `tests-S1.txt` under default and embedded features; `expected-geo-digest.txt`
- [x] `gate_negative_controls`, `worktree_stays_clean`, `rollback_volume`
  Run record: `docs/reports/2026-11-14-core-memory-bound-us010-gates.md` (inputs, results, limits)
  and `docs/reports/2026-11-14-core-memory-bound-us010-gates.jsonl`.

#### US-011: Baseline [Integration]
As the operator, I want the autoresearch manifest ready, so that the S1 run can start.

- [x] `ceilings-S1.json`, `program.md`, `autoresearch.toml`, `.autoresearch/` ignored
- [x] `autoresearch doctor` ready; every hard gate passes at the baseline

#### US-012: S1 run and report [Integration]
As the owner, I want the S1 run and its report, so that I can decide the deploy.

- [x] Autoresearch run ends with `verify: matched` on a kept candidate, or a report that none was
  kept (`docs/reports/2026-11-13-core-memory-s1-run-ledger.md`: none kept)
- [x] `docs/reports/2026-11-13-core-memory-s1-run-ledger.md` and the Acceptance 4 records in
  `docs/reports/2026-11-14-core-memory-s1-report.md`

### Functional requirements

- FR-1: The ledger counts and never refuses, reserves or evicts.
- FR-2: Eviction policy items change only by added ledger calls.
- FR-3: Responses are byte-identical except constraint changes 1–3.
- FR-4: Release builds contain no jemalloc crate.

### Non-goals

- Routine eviction, reclaim, pins, durable cursors (own spec).
- S2–S4.
- Deploy: the owner's action, morning window, not a bead.

### Open questions

- None. Defaults chosen for the two plan questions: epic gates as above; trophy unit + contract +
  integration, all blocking merge.
