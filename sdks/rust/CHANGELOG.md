# Changelog

All notable changes to the `allsource` Rust SDK.

Format based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versioning follows [SemVer](https://semver.org).

## [Unreleased]

## [0.26.0] — 2026-10-27

Implements the latest-state read contract
(`docs/contracts/latest-state/README.md`). Every new call is on both
`QueryClient` and `CoreClient`, and `tests/contract_fixtures.rs` runs the SDK
against every shared fixture in `sdks/contract/fixtures/`.

### Added

- `entity_state(id, &EntityStateParams)`: current state of one entity without
  its history (`history` is sent only when set), with `as_of` and
  `if_none_match`. Returns `Conditional::Modified(EntityState)` carrying
  `version` and `etag`, or `Conditional::NotModified { etag }` on a 304.
- `QueryEventsResponse::entity_version`, sent by Core when the query filters by
  `entity_id`.
- `feed(&FeedParams)` for `GET /api/v1/event-feed`, with
  `FeedPage::next_cursor`. A cursor from an older epoch fails with the new
  `Error::FeedEpochChanged { expected, current }`.
- `read_model(name)` returning a `ReadModel` handle with `page` and `commit`.
  `ReadModelCommit::delete` sends the row as JSON `null`; a stale
  `expected_revision` fails with `Error::VersionConflict` and is not retried.
- Views: `define_view`, `get_view`, `delete_view`, `list_views`;
  `latest(view, key, if_none_match)` returning `LatestRow::Found`,
  `NotModified` or `NotFound { ready }`; `latest_many` (a
  `LatestManyPaginator` over every page); `view_entities` and `view_changes`
  for single calls.
- `sync(view, SyncOptions)` returning a `ViewSync`: every entity page, then a
  `changes` loop that long-polls with `wait` once caught up. It re-reads the
  pages with `reset: true` on `view_reset`, waits out `Retry-After` on 429 and
  503, and yields `SyncBatch` values of `SyncChange::Upsert` / `Delete` plus a
  checkpoint to store and pass back through `SyncOptions::checkpoint`.
- `Error::ViewReset { generation }` and `Error::InvalidView { detail }`.

### Changed

- `ProjectionWorker` state flush (`state_flush_entities`) commits to the Core
  read model named after the worker instead of the retired projection-state
  routes, at the feed head, resetting a model committed in an older epoch.
  `ProjectionHandle::get_state` reads that read model. Worker names must now be
  valid read-model names (1..64 of `[A-Za-z0-9_-]`).
- `Error` has new variants, so exhaustive matches need updating.

### Removed

- `CoreClient::get_projection_state`, `get_projection_state_summary`,
  `get_projection_state_summary_paged`, `bulk_get_projection_states`,
  `put_projection_state`, `bulk_put_projection_state`, and
  `ProjectionStateSummaryParams` / `ProjectionStateSummaryPage`. Core removed
  `/api/v1/projections/{name}/state`, `/{name}/{entity_id}/state`, `/bulk` and
  `/bulk/save` because their store carries no tenant key. Use a read model or
  a view.

## [0.25.0] — 2026-10-24

### Added

- `Error::RateLimited { retry_after, scope }` for a 429, and for a 503 whose
  body names a refusal `scope` (the control plane's global capacity refusal).
  `retry_after` is `Retry-After` in delta-seconds; `scope` is `tenant`,
  `global_fair_share` or `global`. `Error::retry_after()` reads it.
- A client-wide not-before deadline: after a refusal with `Retry-After`, every
  call on that client and its clones fails with `RateLimited` (carrying the
  time left) without a network call until the deadline passes.
- `QueryEventsParams::if_none_match` and `QueryClient::query_events_conditional`,
  which returns `EventsQuery::NotModified(ResponseMeta)` on a 304.
  `query_events` returns `Error::NotModified` for the same case.
- `QueryEventsResponse::meta: ResponseMeta` with the response's `ETag`,
  `RateLimit` and `RateLimit-Policy` headers.
- `gzip` cargo feature: sends `Accept-Encoding: gzip` and decodes gzip bodies.
- `CircuitBreaker::acquire` and `CircuitPermit`; `QueryClient::circuit_state`
  and `CoreClient::circuit_state`; `CircuitState` re-export.

### Changed

- A 429 is no longer retried inside the client and no longer counts as a
  circuit-breaker failure. Retrying a refusal inside its `Retry-After` only
  adds load to the server that refused it.
- Half-open admits exactly one probe, with no retries; concurrent callers get
  `CircuitOpen` until it resolves. `CircuitBreaker::check` reports a half-open
  breaker with a probe in flight as closed to new requests.
- `QueryEventsResponse` and `QueryEventsParams` have new public fields, so
  struct literals of either need updating.

## [0.24.0] — 2026-08-12

### Added

- `IngestEventInput::expected_version` — compare-and-swap ingest. Core has read
  and enforced this field since 0.23.0 (`api.rs:286`, `store.rs:503`), but the
  SDK's input type had no way to express it, so any consumer needing a versioned
  write stood up a second `reqwest::Client` beside the SDK — forfeiting the retry
  loop and circuit breaker, and duplicating auth. Honoured by single ingest and
  by every item of a batch, matching Core, which reads the field per event rather
  than per request. Resolves issue #257.
- `Error::VersionConflict { expected, current }` — Core answers a CAS rejection
  with `409 {"error":"version_conflict",...}`; the SDK now surfaces that as its
  own variant carrying both versions instead of an opaque `Error::Api`. It is
  deliberately **not** retryable: an unchanged retry either fails identically or
  lands on a version the caller never inspected. (Note Core classifies the same
  error `is_retryable()` server-side; that is Core's internal storage-retry
  notion, not a licence for a client to replay a rejected CAS.)
- `IngestResponse::version` — the entity's version after the append. Core has
  always returned it; the SDK dropped it, which forced a re-read between chained
  CAS writes. Feed it straight into the next `with_expected_version`.
- `IngestEventInput::new` / `with_metadata` / `with_expected_version` builders,
  so future field additions stop breaking construction.
- `CoreClient::get_projection_state_summary_paged` with
  `ProjectionStateSummaryParams` (`limit` / `offset` / `entity_id_prefix`) and
  `ProjectionStateSummaryPage` (`states` / `total` / `has_more`). Core shipped
  this pagination for issue #249; the SDK sent none of the parameters and
  discarded `total`/`has_more`, so from Rust the feature did not exist. This is
  the only endpoint that can *enumerate* a projection —
  `bulk_get_projection_states` needs the ids up front — so a projection with one
  entry per tenant was unusable without pulling every tenant in one unbounded
  response. Resolves issue #256.

### Changed

- **Breaking:** `IngestEventInput` gained a public field, so struct-literal
  construction must add `..Default::default()` or move to
  `IngestEventInput::new(..)`. No wire change: the field is skipped when unset,
  so requests from callers that do not opt in are byte-identical.
- `get_projection_state_summary_paged` returns `Error::Protocol` when the server
  answers with more states than `limit` allows. Core's extractor drops unknown
  query parameters silently, so against a pre-#249 Core the request *looks*
  accepted and quietly returns the whole projection. Failing loudly is the
  point — issue #250 was this exact shape, an ignored `offset` that turned a
  paginator into an infinite loop of duplicate pages while CI stayed green.
  `get_projection_state_summary` (unbounded) is unchanged and still works
  against an older Core.

## [0.23.0] — 2026-08-11

### Added

- `CoreClient::get_projection_state_summary` (`GET /api/v1/projections/:name/state`),
  `CoreClient::bulk_get_projection_states` (`POST /api/v1/projections/:name/bulk`)
  and `CoreClient::poll_consumer_events` (`GET /api/v1/consumers/:id/events`).
  These three Core routes had no SDK method, and `CoreClient::transport()` is
  `pub(crate)`, so reaching them meant standing up a second HTTP client without
  the SDK's retry loop and circuit breaker. Polling returns the new
  `ConsumerEvent` type (WAL `position` + `event`) to ack with `ack_consumer`.
  Resolves issue #246.
- `Error::Protocol` — the server answered 2xx with a body that breaks the API
  contract (currently: it ignored a pagination parameter). Distinct from
  `Error::Json` (the body did not parse) and `Error::Api` (the server said no).

### Fixed

- Docs: `CoreClient::get_projection_state` and `ProjectionHandle::get_state` no
  longer claim Core's `GET /api/v1/projections/:name/:entity_id/state` requires
  a registered projection. Core ≥ 0.19.1 resolves the registered projection
  first and falls back to the projection state cache, so state pushed with
  `put_projection_state` / `bulk_put_projection_state` reads back through the
  same endpoint — the "one worker computes, many stateless readers read" shape.
  The stale caveat predates the Core v0.19.1 fallback. Resolves issue #247.
- `EventPaginator` and `EntityPaginator` no longer loop forever against a server
  that ignores `offset`. Core's `/api/v1/events/query` dropped the parameter
  until the fix for issue #250 — every page came back as page one with
  `has_more: true`, so `collect_all()` spun and grew until it ran out of memory.
  Both paginators now detect a page that repeats the previous page's first item
  and return the new `Error::Protocol` naming the dropped parameter, instead of
  spinning. A released SDK still meets older Core deployments, so the client
  needs its own guard.

## [0.21.0] — 2026-05-16

### Added

- `QueryEventsParams::order` plus the `SortOrder` enum (`Asc`/`Desc`) and the
  `order_desc()` / `order_asc()` builder shorthands. This sends `order=desc` to
  Core's `/api/v1/events/query`, so clients can fetch newest-first (e.g. the
  latest event for an entity with `.order_desc().limit(1)`) without folding the
  whole stream in memory. Resolves issue #178.
- `EventPaginator` / `EntityPaginator` and the `QueryClient::query_events_paged`
  / `list_entities_paged` constructors. They drive `limit`/`offset` paging:
  `next_page()` fetches the next batch until the server reports no more,
  `collect_all()` drains everything. `has_more` is used when present, with
  short-page fallback for older Core. `DEFAULT_PAGE_SIZE` (100) is the per-page
  size when params set no `limit`.
- `list_entities` now accepts an `order` parameter (`asc`/`desc`) via the new
  `ListEntitiesParams::order`. Core's entity-listing endpoint sorts by
  last-event time with an `entity_id` tie-break, making the order total and
  offset pagination stable.

### Changed

- **Breaking:** `QueryClient::list_entities` now takes a single
  `ListEntitiesParams` (builder: `event_type_prefix` / `limit` / `offset` /
  `order`) instead of three positional `Option` arguments. Migration:
  `list_entities(Some("p."), Some(50), None)` →
  `list_entities(ListEntitiesParams::new().event_type_prefix("p.").limit(50))`.
- `QueryEventsParams` / `QueryEventsResponse` / `query_events` rustdoc now state
  the result ordering guarantee: events are ordered by `(timestamp, version)`,
  ascending by default, so `limit`/`offset` pagination over a sorted view is
  well-defined.

## [0.19.2] — 2026-04-17

### Changed

- Documentation-focused release. No public API changes in the SDK.
- README rewritten with a "Which client do I want?" decision table, "Performance cheatsheet", and "API keys" section so consumers don't need a separate perf guide.
- Crate-level rustdoc updated to mirror the same guidance (shows in rust-analyzer hover and docs.rs).
- Per-type rustdoc expanded on `CoreClient`, `QueryClient`, and `ProjectionWorker` explaining when each is the right default.

### Fixed (monorepo, no SDK-code impact)

- Core `Dockerfile` now stubs `sdks/rust/examples/asset_projection.rs` so cargo-chef parses the workspace correctly during image build. v0.19.1's Docker tag build failed on this; v0.19.2's ships.
- Monorepo `Makefile` `set-version` regex handles 3-segment versions in `apps/prime-mcp/Cargo.toml`'s `allsource-core` dep constraint (was silently skipping on 3-segment patterns).

[0.19.2]: https://github.com/all-source-os/all-source/releases/tag/v0.19.2

## [0.19.1] — 2026-04-17

### Changed

- Version bump as part of monorepo-wide v0.19.1 release. No SDK source changes. Pairs with Core v0.19.1, which adds cache-fallback to `GET /api/v1/projections/:name/:entity_id/state` so `ProjectionHandle::get_state` now works for SDK-managed projections (no registered projection required). The caveat previously documented in the README / use-case guide is resolved against any Core ≥ 0.19.1.

[0.19.1]: https://github.com/all-source-os/all-source/releases/tag/v0.19.1

## [0.19.0] — 2026-04-17

### Added

- **`ProjectionWorker`** — first-party worker for building custom projections from Core's event stream ([#155]). Handles WebSocket subscription, durable-consumer registration, replay → live transition, per-entity version dedup, checkpointing, and exponential-backoff reconnection. Users provide only the reducer closure.
  - Builder API: `ProjectionWorker::<S>::builder(core).name(...).event_types(...).reducer(...).checkpoint_interval(...).build()`
  - Optional state push-back: `.state_flush_entities(...).state_flush_every(...).state_flush_interval(...)`
  - Lifecycle: `worker.start().await?` → `ProjectionHandle<S>` with `state()`, `get_state(id)`, `is_caught_up()`, `current_position()`, `stop().await`
- **`ws` feature** — WebSocket client (`EventStreamClient`, `EventStream`, `StreamItem`) for Core's `/api/v1/events/stream`. Parses replay/replay_complete/live/lagged frames into a typed stream.
- **`projection-worker` feature (default-on)** — pulls in `ws` + async-trait for the worker. Disable with `default-features = false` for HTTP-only builds.
- **`CoreClient` projection + consumer helpers**: `get_projection_state`, `put_projection_state`, `bulk_put_projection_state`, `register_consumer`, `get_consumer`, `ack_consumer`, `save_checkpoint`, `load_checkpoint`.
- **`ConsumerState` type** for durable-consumer responses.
- Integration tests against a live Core (skip gracefully without `ALLSOURCE_TEST_CORE_URL`): cold start, restart resume, version dedup, reducer-error propagation.
- Runnable example: `examples/asset_projection.rs` (mirrors the API shape requested in #155).
- Documentation: README "Building custom projections" section + full guide in `docs/use-cases/custom-projections.md`.

### Changed

- README rewritten: the old `Client::new` snippet referenced a type that hasn't existed for several releases; it's now `QueryClient` + `CoreClient`.
- `Error::WebSocket(String)` variant added (behind `ws` feature).

### Internal

- `HttpTransport` exposes `pub(crate)` methods for `put` and `get_optional` (404 → None). No user-facing change.

[#155]: https://github.com/all-source-os/all-source/issues/155
[0.19.0]: https://github.com/all-source-os/all-source/releases/tag/sdk-rust-v0.19.0
