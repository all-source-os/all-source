use super::*;
use crate::application::dto::QueryEventsRequest;
use crate::domain::entities::Event;
use crate::infrastructure::security::{
    auth::{Claims, Role},
    middleware::AuthContext,
};
use crate::infrastructure::web::api;
use crate::store::EventStore;
use axum::{
    Router,
    body::Body,
    http::{HeaderName, HeaderValue, Request, StatusCode},
    middleware,
    routing::{get, put},
};
use serde_json::json;
use tower::ServiceExt;

fn read_cost() -> Arc<ReadCost> {
    ReadCost::new(MetricsRegistry::new(), true)
}

fn base() -> Instant {
    Instant::now()
}

fn at(base: Instant, seconds: u64) -> Instant {
    base + Duration::from_secs(seconds)
}

fn page(
    tenant: &str,
    class: &str,
    read_id: Option<&str>,
    matched: usize,
    returned: usize,
    has_more: bool,
) -> ReadCostFacts {
    ReadCostFacts {
        read: ReadFacts {
            tenant: tenant.to_owned(),
            stream_class: class.to_owned(),
            counts: QueryCounts {
                matched,
                returned,
                served_limit: 10,
                offset: 0,
                has_more,
                since_set: false,
                entity_set: false,
                shape: Shape::Partial,
            },
        },
        caller: None,
        read_id: read_id.map(str::to_owned),
        rejected: Vec::new(),
    }
}

fn class_row_of(rc: &ReadCost, tenant: &str, class: &str) -> ClassRow {
    let tables = rc.lock();
    tables
        .tenants
        .get(tenant)
        .and_then(|state| state.classes.get(class))
        .copied()
        .unwrap_or_default()
}

fn caller_row_of(rc: &ReadCost, tenant: &str, caller: Option<&str>, class: &str) -> CallerRow {
    let tables = rc.lock();
    tables
        .tenants
        .get(tenant)
        .and_then(|state| {
            state
                .callers
                .get(&(caller.map(str::to_owned), class.to_owned()))
        })
        .copied()
        .unwrap_or_default()
}

fn walk_count(rc: &ReadCost) -> usize {
    rc.lock().walks.len()
}

fn truncated_count(rc: &ReadCost, class: &str, kind: &str) -> u64 {
    rc.metrics
        .read_truncated_total
        .with_label_values(&[class, kind])
        .get()
}

#[test]
fn read_cost_walk_enters_the_table_only_on_a_page_with_more() {
    let rc = read_cost();
    let t0 = base();

    rc.observe(
        t0,
        &page("t", "walk", Some("0123456789abcdef"), 25, 10, true),
        Outcome::HasMore,
        0,
    );
    assert_eq!(walk_count(&rc), 1);

    rc.observe(
        at(t0, 1),
        &page("t", "walk", Some("0123456789abcdef"), 25, 5, false),
        Outcome::Complete,
        0,
    );
    assert_eq!(walk_count(&rc), 0);
}

#[test]
fn read_cost_final_page_with_no_entry_leaves_the_table_alone() {
    let rc = read_cost();

    rc.observe(
        base(),
        &page("t", "walk", Some("0123456789abcdef"), 3, 3, false),
        Outcome::Complete,
        0,
    );

    assert_eq!(walk_count(&rc), 0);
    assert_eq!(class_row_of(&rc, "t", "walk").logical_reads, 1);
}

#[test]
fn read_cost_walk_of_25_events_at_page_size_10_is_three_pages_and_no_abandon() {
    let rc = read_cost();
    let t0 = base();
    let id = Some("0123456789abcdef");

    rc.observe(
        t0,
        &page("t", "walk", id, 25, 10, true),
        Outcome::HasMore,
        0,
    );
    rc.observe(
        at(t0, 1),
        &page("t", "walk", id, 25, 10, true),
        Outcome::HasMore,
        0,
    );
    rc.observe(
        at(t0, 2),
        &page("t", "walk", id, 25, 5, false),
        Outcome::Complete,
        0,
    );
    rc.sweep(at(t0, 120));

    let row = class_row_of(&rc, "t", "walk");
    assert_eq!(row.pages, 3);
    assert_eq!(row.logical_reads, 1);
    assert_eq!(row.max_pages, 3);
    assert_eq!(row.walk_abandoned, 0);
    assert_eq!(walk_count(&rc), 0);
}

#[test]
fn read_cost_walk_stopped_after_page_one_is_abandoned_between_30_and_45_seconds() {
    let rc = read_cost();
    let t0 = base();

    rc.observe(
        t0,
        &page("t", "walk", Some("0123456789abcdef"), 25, 10, true),
        Outcome::HasMore,
        0,
    );
    rc.sweep(at(t0, 29));
    assert_eq!(class_row_of(&rc, "t", "walk").walk_abandoned, 0);

    rc.sweep(at(t0, 40));
    let row = class_row_of(&rc, "t", "walk");
    assert_eq!(row.walk_abandoned, 1);
    assert_eq!(walk_count(&rc), 0);
    assert_eq!(truncated_count(&rc, "walk", "walk_abandoned"), 1);
}

#[test]
fn read_cost_sweep_ends_only_walks_idle_for_thirty_seconds() {
    let rc = read_cost();
    let t0 = base();

    rc.observe(
        t0,
        &page("t", "walk", Some("aaaaaaaaaaaaaaaa"), 25, 10, true),
        Outcome::HasMore,
        0,
    );
    rc.observe(
        at(t0, 25),
        &page("t", "walk", Some("bbbbbbbbbbbbbbbb"), 25, 10, true),
        Outcome::HasMore,
        0,
    );
    rc.sweep(at(t0, 30));

    assert_eq!(walk_count(&rc), 1);
    assert_eq!(class_row_of(&rc, "t", "walk").walk_abandoned, 1);
}

#[test]
fn read_cost_page_after_thirty_seconds_starts_a_new_walk() {
    let rc = read_cost();
    let t0 = base();
    let id = Some("0123456789abcdef");

    rc.observe(
        t0,
        &page("t", "walk", id, 25, 10, true),
        Outcome::HasMore,
        0,
    );
    rc.observe(
        at(t0, 31),
        &page("t", "walk", id, 25, 10, true),
        Outcome::HasMore,
        0,
    );

    let row = class_row_of(&rc, "t", "walk");
    assert_eq!(row.walk_abandoned, 1);
    assert_eq!(row.logical_reads, 2);
    assert_eq!(walk_count(&rc), 1);
}

#[test]
fn read_cost_full_table_evicts_the_least_recently_seen_walk_as_evicted() {
    let rc = read_cost();
    let t0 = base();

    for i in 0..=IN_FLIGHT_CAP {
        let id = format!("{i:016x}");
        rc.observe(
            at(t0, 0),
            &page("t", "walk", Some(&id), 25, 10, true),
            Outcome::HasMore,
            0,
        );
    }

    assert_eq!(walk_count(&rc), IN_FLIGHT_CAP);
    let row = class_row_of(&rc, "t", "walk");
    assert_eq!(row.walk_evicted, 1);
    assert_eq!(row.walk_abandoned, 0);
    assert_eq!(truncated_count(&rc, "walk", "walk_evicted"), 1);
}

#[test]
fn read_cost_invalid_read_id_counts_as_absent() {
    let mut headers = HeaderMap::new();
    headers.insert(
        HeaderName::from_static(READ_ID_HEADER),
        HeaderValue::from_static("ZZZZ"),
    );
    let facts = ReadCostFacts::from_headers(page("t", "walk", None, 25, 10, true).read, &headers);
    assert_eq!(facts.read_id, None);
    assert_eq!(facts.rejected.len(), 1);
    assert_eq!(facts.rejected[0].name, "read_id");
    assert_eq!(facts.rejected[0].length, 4);

    let rc = read_cost();
    rc.observe(base(), &facts, Outcome::HasMore, 0);

    assert_eq!(walk_count(&rc), 0);
    assert_eq!(class_row_of(&rc, "t", "walk").logical_reads, 1);
}

#[test]
fn read_cost_invalid_caller_is_labelled_invalid() {
    let mut headers = HeaderMap::new();
    headers.insert(
        HeaderName::from_static(CALLER_HEADER),
        HeaderValue::from_static("../../etc"),
    );
    let facts = ReadCostFacts::from_headers(page("t", "walk", None, 3, 3, false).read, &headers);
    assert_eq!(facts.caller.as_deref(), Some("invalid"));
    assert_eq!(facts.rejected[0].name, "caller");

    let rc = read_cost();
    rc.observe(base(), &facts, Outcome::Complete, 7);

    assert_eq!(
        caller_row_of(&rc, "t", Some("invalid"), "walk").body_bytes,
        7
    );
}

#[test]
fn read_cost_label_overflow_folds_into_other_and_is_counted() {
    let rc = read_cost();
    let t0 = base();

    for i in 0..=LABEL_CAP {
        let class = format!("c{i:03}");
        rc.observe(
            t0,
            &page("t", &class, None, 1, 1, false),
            Outcome::Complete,
            0,
        );
    }

    assert_eq!(
        rc.metrics
            .read_label_overflow_total
            .with_label_values(&["stream_class"])
            .get(),
        1
    );
}

#[test]
fn read_cost_exact_table_caps_classes_per_tenant_then_other() {
    let rc = read_cost();
    let t0 = base();

    for i in 0..=CLASSES_PER_TENANT {
        let class = format!("c{i:03}");
        rc.observe(
            t0,
            &page("t", &class, None, 1, 1, false),
            Outcome::Complete,
            0,
        );
    }

    let tables = rc.lock();
    let state = tables.tenants.get("t").unwrap();
    assert_eq!(state.classes.len(), CLASSES_PER_TENANT + 1);
    assert_eq!(state.classes.get(OTHER).map(|row| row.pages), Some(1));
}

#[test]
fn read_cost_caller_table_replaces_the_fewest_body_bytes_as_approximate() {
    let rc = read_cost();
    let t0 = base();

    for i in 0..CALLERS_PER_TENANT {
        let mut facts = page("t", "walk", None, 1, 1, false);
        facts.caller = Some(format!("c{i:03}"));
        rc.observe(t0, &facts, Outcome::Complete, (i + 1) as u64);
    }
    let mut newcomer = page("t", "walk", None, 1, 1, false);
    newcomer.caller = Some("newcomer".to_owned());
    rc.observe(t0, &newcomer, Outcome::Complete, 1);

    let tables = rc.lock();
    let state = tables.tenants.get("t").unwrap();
    assert_eq!(state.callers.len(), CALLERS_PER_TENANT);
    assert!(
        !state
            .callers
            .contains_key(&(Some("c000".to_owned()), "walk".to_owned()))
    );
    let replacement = state
        .callers
        .get(&(Some("newcomer".to_owned()), "walk".to_owned()))
        .copied()
        .unwrap();
    assert!(replacement.approximate);
    assert_eq!(replacement.pages, 2);
}

#[test]
fn read_cost_whole_reads_count_only_whole_shape() {
    let rc = read_cost();
    let mut facts = page("t", "whole", None, 3, 3, false);
    facts.read.counts.shape = Shape::Whole;

    rc.observe(base(), &facts, Outcome::Complete, 0);

    assert_eq!(
        rc.metrics
            .read_whole_reads_total
            .with_label_values(&["whole"])
            .get(),
        1
    );
    assert_eq!(class_row_of(&rc, "t", "whole").whole_reads, 1);
}

#[test]
fn read_cost_outcome_marks_bodies_over_the_bound_too_large() {
    assert_eq!(
        Outcome::of(StatusCode::OK, MAX_RESPONSE_BYTES + 1, true),
        Outcome::TooLarge
    );
    assert_eq!(
        Outcome::of(StatusCode::NOT_MODIFIED, 0, true),
        Outcome::NotModified
    );
    assert_eq!(Outcome::of(StatusCode::OK, 10, true), Outcome::HasMore);
    assert_eq!(Outcome::of(StatusCode::OK, 10, false), Outcome::Complete);
}

#[test]
fn read_cost_shape_follows_limit_order_and_filters() {
    assert_eq!(Shape::of(0, false, false, false), Shape::CountProbe);
    assert_eq!(Shape::of(1, true, false, false), Shape::Latest);
    assert_eq!(Shape::of(5, false, true, false), Shape::Partial);
    assert_eq!(Shape::of(5, false, false, true), Shape::Partial);
    assert_eq!(Shape::of(5, false, false, false), Shape::Whole);
}

#[test]
fn read_cost_threshold_log_key_holds_a_fingerprint_not_the_tenant() {
    let fingerprint_value = fingerprint("acme-tenant");
    assert_eq!(fingerprint_value.len(), 12);
    assert!(!fingerprint_value.contains("acme"));
}

fn app(store: Arc<EventStore>, rc: Arc<ReadCost>, auth: Option<AuthContext>) -> Router {
    let router = Router::new()
        .route("/api/v1/events/query", get(api::query_events))
        .route("/api/v1/admin/read-cost", put(set_read_cost))
        .with_state(store)
        .layer(middleware::from_fn_with_state(Arc::clone(&rc), record))
        .layer(Extension(rc));
    match auth {
        Some(auth) => router.layer(Extension(auth)),
        None => router,
    }
}

fn bulk_store(events: usize, event_type: &str) -> Arc<EventStore> {
    let store = Arc::new(EventStore::new());
    for i in 0..events {
        let event = Event::from_strings(
            event_type.to_owned(),
            format!("entity-{i}"),
            "t".to_owned(),
            json!({ "i": i }),
            None,
        )
        .unwrap();
        store.ingest(&event).unwrap();
    }
    store
}

async fn get_query(router: &Router, uri: &str, read_id: Option<&str>) -> StatusCode {
    let mut request = Request::builder().uri(uri);
    if let Some(read_id) = read_id {
        request = request.header(READ_ID_HEADER, read_id);
    }
    router
        .clone()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap()
        .status()
}

#[tokio::test]
async fn read_cost_single_truncated_counts_a_full_unbounded_page() {
    let rc = read_cost();
    let router = app(bulk_store(6_000, "bulk.item"), Arc::clone(&rc), None);

    let status = get_query(
        &router,
        "/api/v1/events/query?tenant_id=t&event_type=bulk.item&limit=5000",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(class_row_of(&rc, "t", "bulk").single_truncated, 1);
    assert_eq!(truncated_count(&rc, "bulk", "single"), 1);
}

#[tokio::test]
async fn read_cost_limit_zero_probe_is_not_single_truncated() {
    let rc = read_cost();
    let router = app(bulk_store(6_000, "bulk.item"), Arc::clone(&rc), None);

    get_query(
        &router,
        "/api/v1/events/query?tenant_id=t&event_type=bulk.item&limit=0",
        None,
    )
    .await;

    assert_eq!(class_row_of(&rc, "t", "bulk").single_truncated, 0);
}

#[tokio::test]
async fn read_cost_descending_limit_one_is_not_single_truncated() {
    let rc = read_cost();
    let router = app(bulk_store(6_000, "bulk.item"), Arc::clone(&rc), None);

    get_query(
        &router,
        "/api/v1/events/query?tenant_id=t&event_type=bulk.item&limit=1&order=desc",
        None,
    )
    .await;

    assert_eq!(class_row_of(&rc, "t", "bulk").single_truncated, 0);
}

#[tokio::test]
async fn read_cost_offset_page_without_read_id_is_not_single_truncated() {
    let rc = read_cost();
    let router = app(bulk_store(6_000, "bulk.item"), Arc::clone(&rc), None);

    get_query(
        &router,
        "/api/v1/events/query?tenant_id=t&event_type=bulk.item&limit=5000&offset=100",
        None,
    )
    .await;

    assert_eq!(class_row_of(&rc, "t", "bulk").single_truncated, 0);
}

#[tokio::test]
async fn read_cost_cursor_page_with_since_is_not_single_truncated() {
    let rc = read_cost();
    let router = app(bulk_store(6_000, "bulk.item"), Arc::clone(&rc), None);

    get_query(
        &router,
        "/api/v1/events/query?tenant_id=t&event_type=bulk.item&limit=5000&since=2000-01-01T00:00:00Z",
        None,
    )
    .await;

    assert_eq!(class_row_of(&rc, "t", "bulk").single_truncated, 0);
}

#[tokio::test]
async fn read_cost_walk_of_25_events_over_http_is_three_pages() {
    let rc = read_cost();
    let router = app(bulk_store(25, "walk.item"), Arc::clone(&rc), None);
    let id = "0123456789abcdef";

    for offset in [0, 10, 20] {
        get_query(
            &router,
            &format!(
                "/api/v1/events/query?tenant_id=t&event_type=walk.item&limit=10&offset={offset}"
            ),
            Some(id),
        )
        .await;
    }

    let row = class_row_of(&rc, "t", "walk");
    assert_eq!(row.pages, 3);
    assert_eq!(row.logical_reads, 1);
    assert_eq!(row.walk_abandoned, 0);
    assert_eq!(walk_count(&rc), 0);
}

#[tokio::test]
async fn read_cost_switch_off_records_nothing_and_needs_an_admin() {
    let rc = read_cost();
    let store = bulk_store(3, "quiet.item");
    let anonymous = app(Arc::clone(&store), Arc::clone(&rc), None);

    let refused = anonymous
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/admin/read-cost")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"enabled":false}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(refused.status(), StatusCode::UNAUTHORIZED);
    assert!(rc.enabled());

    let admin = AuthContext {
        claims: Claims::new(
            "admin-user".to_owned(),
            "t".into(),
            Role::Admin,
            chrono::Duration::minutes(5),
        ),
    };
    let router = app(store, Arc::clone(&rc), Some(admin));
    let switched = router
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/admin/read-cost")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"enabled":false}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(switched.status(), StatusCode::OK);
    assert!(!rc.enabled());

    get_query(
        &router,
        "/api/v1/events/query?tenant_id=t&event_type=quiet.item&limit=2",
        None,
    )
    .await;
    assert_eq!(class_row_of(&rc, "t", "quiet").pages, 0);
}

async fn get_read_costs(rc: Arc<ReadCost>, uri: &str) -> (StatusCode, serde_json::Value) {
    let response = Router::new()
        .route("/api/v1/read-costs", get(serve_read_costs))
        .layer(Extension(rc))
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, serde_json::from_slice(&body).unwrap_or_default())
}

#[tokio::test]
async fn read_costs_without_tenant_is_400() {
    let rc = read_cost();
    rc.observe(
        base(),
        &page("a", "walk", None, 3, 3, false),
        Outcome::Complete,
        9,
    );
    assert_eq!(
        get_read_costs(Arc::clone(&rc), "/api/v1/read-costs")
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        get_read_costs(rc, "/api/v1/read-costs?tenant_id=").await.0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn read_costs_returns_only_the_named_tenants_rows() {
    let rc = read_cost();
    rc.observe(
        base(),
        &page("a", "alpha", None, 3, 3, false),
        Outcome::Complete,
        9,
    );
    rc.observe(
        base(),
        &page("b", "beta", None, 3, 3, false),
        Outcome::Complete,
        9,
    );

    let (status, body) = get_read_costs(rc, "/api/v1/read-costs?tenant_id=a").await;
    assert_eq!(status, StatusCode::OK);
    let classes: Vec<&str> = body["classes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["stream_class"].as_str().unwrap())
        .collect();
    assert_eq!(classes, vec!["alpha"]);
    let callers: Vec<&str> = body["callers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["stream_class"].as_str().unwrap())
        .collect();
    assert_eq!(callers, vec!["alpha"]);
    assert_eq!(body["callers"][0]["caller"], "unattributed");
}

#[tokio::test]
async fn read_costs_orders_by_the_requested_key_and_honours_limit() {
    let rc = read_cost();
    rc.observe(
        base(),
        &page("a", "small", None, 3, 3, false),
        Outcome::Complete,
        1,
    );
    for _ in 0..3 {
        rc.observe(
            base(),
            &page("a", "large", None, 3, 3, false),
            Outcome::Complete,
            1,
        );
    }

    let (_, body) = get_read_costs(rc, "/api/v1/read-costs?tenant_id=a&order=pages&limit=1").await;
    assert_eq!(body["classes"].as_array().unwrap().len(), 1);
    assert_eq!(body["classes"][0]["stream_class"], "large");
    assert_eq!(body["callers"][0]["stream_class"], "large");
}

#[test]
fn read_cost_scope_without_filters_is_unscoped_whole() {
    let req = QueryEventsRequest::default();
    let scope = ReadScope::of(&req, 0, false, 10_000);
    let facts = scope.facts("t".to_owned(), 0, 0, false);
    assert_eq!(facts.stream_class, "unscoped");
    assert_eq!(facts.counts.shape, Shape::Whole);
}
