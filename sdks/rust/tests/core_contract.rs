//! Contract tests: the SDK driven through a fake Core that *implements* Core's
//! wire semantics instead of asserting the ones the SDK wishes for.
//!
//! Why this file exists. Every other SDK test mocks a response per URL, so the
//! mock honours whatever parameter the SDK happened to send — including
//! parameters Core does not implement. That is how issue #250 shipped: Core's
//! `GET /api/v1/events/query` dropped `offset` (the DTO never declared it, and
//! `serde_urlencoded` silently ignores unknown query fields), every page came
//! back as page one with `has_more: true`, and `EventPaginator::collect_all`
//! looped forever accumulating duplicates — while the SDK suite stayed green
//! because its wiremock stubs matched on `offset` and paged correctly.
//!
//! `sdks/` may not depend on `apps/` (CLAUDE.md isolation), so the real Core
//! cannot be booted here. What *can* be done is encode Core's behaviour once,
//! from `apps/core/src/infrastructure/web/api.rs`, and let the SDK meet it:
//!
//! - unknown query parameters are ignored, never rejected (`Query<T>` +
//!   `serde_urlencoded`) — so a parameter the SDK invents is a silent no-op;
//! - `event_type` is an exact match, `event_type_prefix` is the prefix one;
//! - ordering is `order=asc|desc`; any other value is a 400, and there is no
//!   `sort` parameter at all;
//! - `/events/query` applies `offset` before `limit` and reports
//!   `has_more = offset + count < total_count`;
//! - `/entities` sorts by last-event time with an `entity_id` tie-break, then
//!   skips `offset` and takes `limit`.
//!
//! `OffsetSupport::Ignored` reproduces Core *before* the #250 fix — the version
//! a released SDK still has to survive talking to.

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use allsource::{ListEntitiesParams, QueryClient, QueryEventsParams};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, Request, Respond, ResponseTemplate,
};

/// Whether the fake Core applies `offset`. `Ignored` is Core before the fix for
/// issue #250: the parameter is dropped and `has_more` is computed as
/// `count < total_count`, so every page is page one and `has_more` never flips.
#[derive(Clone, Copy, PartialEq, Eq)]
enum OffsetSupport {
    Honoured,
    Ignored,
}

fn query_pairs(request: &Request) -> Vec<(String, String)> {
    request
        .url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect()
}

fn param(pairs: &[(String, String)], key: &str) -> Option<String> {
    pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
}

/// Core rejects an `order` it does not understand with a 400.
fn invalid_order(value: &str) -> ResponseTemplate {
    ResponseTemplate::new(400).set_body_json(serde_json::json!({
        "error": format!("invalid 'order' value '{value}': expected 'asc' or 'desc'")
    }))
}

/// `/events/query` defaults to ascending. `Err` carries the unrecognized value
/// so the caller can 400 with it; anything else in the query string Core simply
/// never looks at.
fn descending(pairs: &[(String, String)]) -> Result<bool, String> {
    match param(pairs, "order").as_deref() {
        None => Ok(false),
        Some(o) if o.eq_ignore_ascii_case("asc") => Ok(false),
        Some(o) if o.eq_ignore_ascii_case("desc") => Ok(true),
        Some(other) => Err(other.to_string()),
    }
}

/// One seeded event. `seq` doubles as the `(timestamp, version)` position, which
/// is the total order Core sorts by.
#[derive(Clone)]
struct SeedEvent {
    id: String,
    event_type: String,
    entity_id: String,
    seq: u32,
}

fn seed(id: &str, event_type: &str, entity_id: &str, seq: u32) -> SeedEvent {
    SeedEvent {
        id: id.to_string(),
        event_type: event_type.to_string(),
        entity_id: entity_id.to_string(),
        seq,
    }
}

impl SeedEvent {
    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "event_type": self.event_type,
            "entity_id": self.entity_id,
            "payload": {},
            "metadata": null,
            "timestamp": format!("2026-01-01T00:00:{:02}Z", self.seq),
            "version": self.seq,
            "tenant_id": "default",
        })
    }
}

/// `GET /api/v1/events/query`, with Core's filtering, ordering and windowing.
struct FakeCoreEvents {
    events: Vec<SeedEvent>,
    offset_support: OffsetSupport,
    requests: Arc<AtomicUsize>,
}

impl Respond for FakeCoreEvents {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        self.requests.fetch_add(1, Ordering::SeqCst);
        let pairs = query_pairs(request);
        let descending = match descending(&pairs) {
            Ok(d) => d,
            Err(bad) => return invalid_order(&bad),
        };

        let mut matches: Vec<&SeedEvent> = self
            .events
            .iter()
            .filter(|e| param(&pairs, "entity_id").is_none_or(|v| e.entity_id == v))
            // exact match — NOT a prefix. `event_type_prefix` is the prefix one.
            .filter(|e| param(&pairs, "event_type").is_none_or(|v| e.event_type == v))
            .filter(|e| {
                param(&pairs, "event_type_prefix").is_none_or(|v| e.event_type.starts_with(&v))
            })
            .collect();
        matches.sort_by_key(|e| e.seq);
        if descending {
            matches.reverse();
        }

        let total_count = matches.len();
        let offset: usize = match self.offset_support {
            OffsetSupport::Honoured => param(&pairs, "offset")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
            OffsetSupport::Ignored => 0,
        };
        let limit: Option<usize> = param(&pairs, "limit").and_then(|v| v.parse().ok());
        let page: Vec<serde_json::Value> = matches
            .into_iter()
            .skip(offset)
            .take(limit.unwrap_or(usize::MAX))
            .map(SeedEvent::to_json)
            .collect();

        let count = page.len();
        let has_more = match self.offset_support {
            OffsetSupport::Honoured => offset + count < total_count,
            // Pre-fix Core: offset was not part of the comparison either.
            OffsetSupport::Ignored => count < total_count,
        };
        ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "events": page,
            "count": count,
            "total_count": total_count,
            "has_more": has_more,
        }))
    }
}

/// `GET /api/v1/entities`: group by entity, sort by last-event time with an
/// `entity_id` tie-break, then `offset`/`limit`.
struct FakeCoreEntities {
    events: Vec<SeedEvent>,
    offset_support: OffsetSupport,
    requests: Arc<AtomicUsize>,
}

impl Respond for FakeCoreEntities {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        self.requests.fetch_add(1, Ordering::SeqCst);
        let pairs = query_pairs(request);
        // Core's entity list defaults to DESC (most recently active first) and
        // takes `asc` to invert — the opposite default to /events/query.
        let ascending = match param(&pairs, "order").as_deref() {
            None => false,
            Some(o) if o.eq_ignore_ascii_case("desc") => false,
            Some(o) if o.eq_ignore_ascii_case("asc") => true,
            Some(other) => return invalid_order(other),
        };

        let mut summaries: Vec<(String, usize, String, u32)> = Vec::new();
        for event in self.events.iter().filter(|e| {
            param(&pairs, "event_type_prefix").is_none_or(|v| e.event_type.starts_with(&v))
        }) {
            match summaries
                .iter_mut()
                .find(|(id, _, _, _)| *id == event.entity_id)
            {
                Some(entry) => {
                    entry.1 += 1;
                    if event.seq > entry.3 {
                        entry.2 = event.event_type.clone();
                        entry.3 = event.seq;
                    }
                }
                None => summaries.push((
                    event.entity_id.clone(),
                    1,
                    event.event_type.clone(),
                    event.seq,
                )),
            }
        }
        summaries.sort_by(|a, b| {
            let by_time = a.3.cmp(&b.3);
            let by_time = if ascending {
                by_time
            } else {
                by_time.reverse()
            };
            by_time.then_with(|| a.0.cmp(&b.0))
        });

        let total = summaries.len();
        let offset: usize = match self.offset_support {
            OffsetSupport::Honoured => param(&pairs, "offset")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
            OffsetSupport::Ignored => 0,
        };
        let windowed: Vec<(String, usize, String, u32)> =
            summaries.into_iter().skip(offset).collect();
        let limit: Option<usize> = param(&pairs, "limit").and_then(|v| v.parse().ok());
        let (has_more, windowed) = match limit {
            Some(limit) => (
                windowed.len() > limit,
                windowed.into_iter().take(limit).collect::<Vec<_>>(),
            ),
            None => (false, windowed),
        };

        let entities: Vec<serde_json::Value> = windowed
            .into_iter()
            .map(|(entity_id, event_count, last_event_type, seq)| {
                serde_json::json!({
                    "entity_id": entity_id,
                    "event_count": event_count,
                    "last_event_type": last_event_type,
                    "last_event_at": format!("2026-01-01T00:00:{seq:02}Z"),
                })
            })
            .collect();
        ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "entities": entities,
            "total": total,
            "has_more": has_more,
        }))
    }
}

struct Harness {
    client: QueryClient,
    requests: Arc<AtomicUsize>,
    _server: MockServer,
}

async fn events_harness(events: Vec<SeedEvent>, offset_support: OffsetSupport) -> Harness {
    let server = MockServer::start().await;
    let requests = Arc::new(AtomicUsize::new(0));
    Mock::given(method("GET"))
        .and(path("/api/v1/events/query"))
        .respond_with(FakeCoreEvents {
            events,
            offset_support,
            requests: Arc::clone(&requests),
        })
        .mount(&server)
        .await;
    let client = QueryClient::new(&server.uri(), "test-key").unwrap();
    Harness {
        client,
        requests,
        _server: server,
    }
}

async fn entities_harness(events: Vec<SeedEvent>, offset_support: OffsetSupport) -> Harness {
    let server = MockServer::start().await;
    let requests = Arc::new(AtomicUsize::new(0));
    Mock::given(method("GET"))
        .and(path("/api/v1/entities"))
        .respond_with(FakeCoreEntities {
            events,
            offset_support,
            requests: Arc::clone(&requests),
        })
        .mount(&server)
        .await;
    let client = QueryClient::new(&server.uri(), "test-key").unwrap();
    Harness {
        client,
        requests,
        _server: server,
    }
}

fn five_orders() -> Vec<SeedEvent> {
    vec![
        seed("e1", "order.placed", "o-1", 1),
        seed("e2", "order.shipped", "o-1", 2),
        seed("e3", "order.placed", "o-2", 3),
        seed("e4", "order.shipped", "o-2", 4),
        seed("e5", "order.placed", "o-3", 5),
    ]
}

// ---------------------------------------------------------------------------
// The #250 class: the server drops `offset`.
// ---------------------------------------------------------------------------

/// Against a Core that ignores `offset`, the paginator must give up rather than
/// re-serve page one forever. Before the guard this loop never terminated:
/// `has_more` stayed `true` and every page was `[e1, e2]` again, so
/// `collect_all()` grew without bound. Bounded here so the failure is a clear
/// assertion instead of a hung test run.
#[tokio::test]
async fn event_paginator_refuses_to_loop_when_the_server_ignores_offset() {
    let h = events_harness(five_orders(), OffsetSupport::Ignored).await;
    let mut pages = h
        .client
        .query_events_paged(QueryEventsParams::new().limit(2));

    let mut seen: Vec<String> = Vec::new();
    let mut error: Option<allsource::Error> = None;
    for _ in 0..10 {
        match pages.next_page().await {
            Ok(Some(page)) => seen.extend(page.into_iter().map(|e| e.id)),
            Ok(None) => break,
            Err(e) => {
                error = Some(e);
                break;
            }
        }
    }

    assert!(
        seen.len() <= 5,
        "paginator emitted {} events from a 5-event server: it is re-reading page one \
         because the server ignored `offset` (issue #250 seen from the client side)",
        seen.len()
    );
    let error = error.expect(
        "a server that ignores `offset` must surface an error, not silently \
         truncate or spin — the caller asked for every page",
    );
    let message = error.to_string();
    assert!(
        message.contains("offset"),
        "error must name the parameter the server dropped, got: {message}"
    );
    assert!(
        pages.is_exhausted(),
        "the paginator must stay stopped after reporting a broken server"
    );
}

/// Same guard on the entity walk: `list_entities` pages with the same
/// `limit`/`offset` contract, so it fails the same way against a server that
/// drops the parameter.
#[tokio::test]
async fn entity_paginator_refuses_to_loop_when_the_server_ignores_offset() {
    let h = entities_harness(five_orders(), OffsetSupport::Ignored).await;
    let mut pages = h
        .client
        .list_entities_paged(ListEntitiesParams::new().limit(1));

    let mut seen: Vec<String> = Vec::new();
    let mut error: Option<allsource::Error> = None;
    for _ in 0..10 {
        match pages.next_page().await {
            Ok(Some(page)) => seen.extend(page.into_iter().map(|e| e.entity_id)),
            Ok(None) => break,
            Err(e) => {
                error = Some(e);
                break;
            }
        }
    }

    assert!(
        seen.len() <= 3,
        "paginator emitted {} entities from a 3-entity server: it is re-reading page one",
        seen.len()
    );
    let error = error.expect("a server that ignores `offset` must surface an error");
    assert!(
        error.to_string().contains("offset"),
        "error must name the parameter the server dropped, got: {error}"
    );
    assert!(pages.is_exhausted());
}

// ---------------------------------------------------------------------------
// Paging over a server that does implement the contract.
// ---------------------------------------------------------------------------

/// The happy path over Core's real windowing, including the exact-boundary case
/// (`total` is a multiple of the page size). Core answers the last full page
/// with `has_more: false`, so the paginator must stop there — trusting only the
/// short-page heuristic would spend an extra round trip on every exact
/// boundary, which is why the request count is asserted.
#[tokio::test]
async fn event_paginator_stops_on_has_more_false_at_an_exact_page_boundary() {
    let events = five_orders()[..4].to_vec();
    let h = events_harness(events, OffsetSupport::Honoured).await;

    let all = h
        .client
        .query_events_paged(QueryEventsParams::new().limit(2))
        .collect_all()
        .await
        .unwrap();

    let ids: Vec<&str> = all.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids, ["e1", "e2", "e3", "e4"]);
    assert_eq!(
        h.requests.load(Ordering::SeqCst),
        2,
        "4 events at page size 2 is exactly two requests: `has_more: false` on the \
         last full page ends the walk without a trailing empty fetch"
    );
}

/// Descending order composed with paging. `order=desc` is also the parameter
/// name Core actually reads — the gateway used to send `sort=timestamp:desc`,
/// which Core ignored, so the "recent events" feed served the *oldest* events
/// (issue #252). A mock that ignores unknown parameters is what catches that.
#[tokio::test]
async fn event_paginator_pages_descending_through_the_order_parameter() {
    let h = events_harness(five_orders(), OffsetSupport::Honoured).await;

    let all = h
        .client
        .query_events_paged(QueryEventsParams::new().order_desc().limit(2))
        .collect_all()
        .await
        .unwrap();

    let ids: Vec<&str> = all.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(
        ids,
        ["e5", "e4", "e3", "e2", "e1"],
        "newest first across page boundaries — an ignored order parameter would \
         return the ascending stream instead"
    );
}

// ---------------------------------------------------------------------------
// Filter semantics.
// ---------------------------------------------------------------------------

/// `event_type` is exact and `event_type_prefix` is the prefix filter. The two
/// builders map to two different Core parameters; swapping them is invisible to
/// a stub that returns a canned body regardless of the query string.
#[tokio::test]
async fn event_type_filters_exactly_and_the_prefix_builder_is_the_prefix_one() {
    let h = events_harness(five_orders(), OffsetSupport::Honoured).await;

    let exact_on_a_prefix = h
        .client
        .query_events(QueryEventsParams::new().event_type("order."))
        .await
        .unwrap();
    assert_eq!(
        exact_on_a_prefix.events.len(),
        0,
        "`event_type` is an exact match in Core: \"order.\" matches nothing"
    );

    let exact = h
        .client
        .query_events(QueryEventsParams::new().event_type("order.placed"))
        .await
        .unwrap();
    assert_eq!(exact.events.len(), 3);

    let prefixed = h
        .client
        .query_events(QueryEventsParams::new().event_type_prefix("order."))
        .await
        .unwrap();
    assert_eq!(prefixed.events.len(), 5);
}

/// Core rejects an `order` value it does not know with a 400. The SDK's
/// [`SortOrder`] must therefore serialize to exactly `asc`/`desc`.
#[tokio::test]
async fn sort_order_serializes_to_the_values_core_accepts() {
    let h = events_harness(five_orders(), OffsetSupport::Honoured).await;

    for params in [
        QueryEventsParams::new().order_asc(),
        QueryEventsParams::new().order_desc(),
    ] {
        let result = h.client.query_events(params).await;
        assert!(
            result.is_ok(),
            "Core 400s on any order value other than asc/desc: {:?}",
            result.err()
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Issue #257 — optimistic-concurrency ingest
//
// Core reads `expected_version` off each ingest request (`api.rs:286`, `:399`)
// and enforces it in `store.rs:503` under the entity's version lock. A mismatch
// is `AllSourceError::VersionConflict`, rendered by `error.rs:138-145` as a 409
// with {"error":"version_conflict","expected_version":N,"current_version":M}.
//
// Note Core marks VersionConflict `is_retryable()` server-side (`error.rs:74`),
// but a CAS rejection cannot succeed on an unchanged retry — the caller has to
// re-read and recompute. The SDK must therefore surface it as its own variant
// and keep it out of the retry loop.
// ═══════════════════════════════════════════════════════════════════════════

struct FakeCoreIngest {
    /// Current version of the single entity this fake tracks.
    current_version: u64,
    /// Bodies the fake actually received, so a test can assert the wire.
    seen: Arc<std::sync::Mutex<Vec<serde_json::Value>>>,
    attempts: Arc<AtomicUsize>,
}

impl Respond for FakeCoreIngest {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        self.attempts.fetch_add(1, Ordering::SeqCst);
        let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap_or_default();
        self.seen.lock().unwrap().push(body.clone());

        let expected = body
            .get("expected_version")
            .and_then(serde_json::Value::as_u64);
        match expected {
            Some(expected) if expected != self.current_version => ResponseTemplate::new(409)
                .set_body_json(serde_json::json!({
                    "error": "version_conflict",
                    "expected_version": expected,
                    "current_version": self.current_version,
                })),
            // Core's real success body: api.rs:304 IngestEventResponse.
            _ => ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "event_id": "evt-1",
                "timestamp": "2026-08-12T00:00:00Z",
                "version": self.current_version + 1,
            })),
        }
    }
}

async fn ingest_harness(
    current_version: u64,
) -> (
    allsource::CoreClient,
    Arc<std::sync::Mutex<Vec<serde_json::Value>>>,
    Arc<AtomicUsize>,
    MockServer,
) {
    let server = MockServer::start().await;
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let attempts = Arc::new(AtomicUsize::new(0));
    Mock::given(method("POST"))
        .and(path("/api/v1/events"))
        .respond_with(FakeCoreIngest {
            current_version,
            seen: Arc::clone(&seen),
            attempts: Arc::clone(&attempts),
        })
        .mount(&server)
        .await;
    let client = allsource::CoreClient::new(&server.uri(), "test-key").unwrap();
    (client, seen, attempts, server)
}

#[tokio::test]
async fn expected_version_reaches_the_wire_and_a_match_succeeds() {
    let (client, seen, _attempts, _server) = ingest_harness(7).await;

    client
        .ingest_event(
            allsource::IngestEventInput::new(
                "tenant.updated",
                "tenant-1",
                serde_json::json!({ "plan": "studio" }),
            )
            .with_expected_version(7),
        )
        .await
        .expect("CAS write at the current version must succeed");

    let bodies = seen.lock().unwrap();
    assert_eq!(
        bodies[0]
            .get("expected_version")
            .and_then(serde_json::Value::as_u64),
        Some(7),
        "expected_version must be serialized onto the request"
    );
}

#[tokio::test]
async fn omitting_expected_version_sends_no_field_at_all() {
    // The wire must stay byte-identical for callers that never opt in —
    // `expected_version: null` would be a behaviour change for Core's DTO.
    let (client, seen, _attempts, _server) = ingest_harness(7).await;

    client
        .ingest_event(allsource::IngestEventInput::new(
            "tenant.updated",
            "tenant-1",
            serde_json::json!({}),
        ))
        .await
        .expect("unconditional write");

    let bodies = seen.lock().unwrap();
    assert!(
        bodies[0].get("expected_version").is_none(),
        "absent expectation must be omitted, not sent as null: {}",
        bodies[0]
    );
}

#[tokio::test]
async fn version_conflict_surfaces_as_a_typed_error_and_is_not_retried() {
    let (client, _seen, attempts, _server) = ingest_harness(9).await;

    let result = client
        .ingest_event(
            allsource::IngestEventInput::new("tenant.updated", "tenant-1", serde_json::json!({}))
                .with_expected_version(4),
        )
        .await;

    match result {
        Err(allsource::Error::VersionConflict { expected, current }) => {
            assert_eq!(expected, 4);
            assert_eq!(current, 9, "the caller needs the real version to recompute");
        }
        Err(other) => panic!("expected Error::VersionConflict, got {other:?}"),
        Ok(_) => panic!("a CAS write against the wrong version must not succeed"),
    }

    assert_eq!(
        attempts.load(Ordering::SeqCst),
        1,
        "a compare-and-swap rejection must not be retried — an unchanged retry \
         either fails identically or lands on a version the caller never read"
    );
}

#[tokio::test]
async fn version_conflict_is_not_classified_as_retryable() {
    let err = allsource::Error::VersionConflict {
        expected: 1,
        current: 2,
    };
    assert!(
        !err.is_retryable(),
        "is_retryable() gates the transport retry loop; a CAS failure is a \
         decision point for the caller, not a transient fault"
    );
}
