use super::*;
use crate::{
    domain::entities::Event,
    error::Result as CoreResult,
    infrastructure::{
        search::QueryEmbedder,
        security::{
            auth::{AuthManager, Role},
            middleware::{AuthState, auth_middleware},
        },
    },
    store::EventStore,
};
use axum::{
    body::{Body, to_bytes},
    http::Request,
    middleware,
};
use tower::ServiceExt;
use uuid::Uuid;

const DIMS: usize = 256;

/// Bag-of-words hashed into a fixed space: texts sharing words are close.
struct HashingEmbedder;

impl QueryEmbedder for HashingEmbedder {
    fn embed(&self, text: &str) -> CoreResult<Vec<f32>> {
        let mut v = vec![0.0f32; DIMS];
        for word in text
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
        {
            let bucket = word
                .to_lowercase()
                .bytes()
                .fold(7usize, |h, b| h.wrapping_mul(31).wrapping_add(b as usize));
            v[bucket % DIMS] += 1.0;
        }
        let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in &mut v {
                *x /= norm;
            }
        } else {
            v[0] = 1.0;
        }
        Ok(v)
    }
}

struct Harness {
    app: Router,
    store: Arc<EventStore>,
    search: Arc<EventSearch>,
    key_a: String,
    key_b: String,
}

fn harness(search_state: impl FnOnce(Arc<EventSearch>) -> SearchApiState) -> Harness {
    let store = Arc::new(EventStore::new());
    let search = EventSearch::new(Arc::clone(&store)).unwrap();
    let auth_manager = Arc::new(AuthManager::new("search-api-test-secret"));
    let (_, key_a) =
        auth_manager.create_api_key("a".into(), "tenant-a".into(), Role::ReadOnly, None);
    let (_, key_b) =
        auth_manager.create_api_key("b".into(), "tenant-b".into(), Role::ReadOnly, None);
    let app = Router::new()
        .merge(router(search_state(Arc::clone(&search))))
        .layer(middleware::from_fn_with_state(
            AuthState { auth_manager },
            auth_middleware,
        ));
    Harness {
        app,
        store,
        search,
        key_a,
        key_b,
    }
}

fn ready_harness() -> Harness {
    let h = harness(|s| SearchApiState::new(Some(s)));
    h.search.set_embedder(Arc::new(HashingEmbedder));
    h
}

fn ingest(store: &EventStore, tenant: &str, event_type: &str, entity: &str, text: &str) -> Uuid {
    let event = Event::from_strings(
        event_type.to_string(),
        entity.to_string(),
        tenant.to_string(),
        json!({ "text": text }),
        None,
    )
    .unwrap();
    let id = event.id();
    store.ingest(&event).unwrap();
    id
}

async fn send(
    app: &Router,
    key: Option<&str>,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(key) = key {
        builder = builder.header("authorization", format!("Bearer {key}"));
    }
    let req = match body {
        Some(b) => builder
            .header("content-type", "application/json")
            .body(Body::from(b.to_string()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    };
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn ids(body: &Value) -> Vec<String> {
    body["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["event_id"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn semantic_search_returns_the_callers_closest_event_in_the_wire_contract() {
    let h = ready_harness();
    let invoice = ingest(
        &h.store,
        "tenant-a",
        "billing.invoice_paid",
        "inv-1",
        "invoice paid by acme",
    );
    let other = ingest(
        &h.store,
        "tenant-a",
        "user.signed_up",
        "user-1",
        "new user signed up",
    );
    let foreign = ingest(
        &h.store,
        "tenant-b",
        "billing.invoice_paid",
        "inv-9",
        "invoice paid by acme",
    );
    h.search.catch_up().await.unwrap();

    let (status, body) = send(
        &h.app,
        Some(&h.key_a),
        "GET",
        "/api/v1/search/semantic?query=invoice%20paid&limit=5",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let found = ids(&body);
    assert_eq!(found[0], invoice.to_string());
    assert!(found.contains(&other.to_string()));
    assert!(!found.contains(&foreign.to_string()));
    assert_eq!(body["count"], found.len());
    let top = &body["results"][0];
    assert_eq!(top["event_type"], "billing.invoice_paid");
    assert_eq!(top["entity_id"], "inv-1");
    assert!(top["score"].as_f64().unwrap() > 0.5);
    assert!(top["timestamp"].as_str().is_some());
    let keys: Vec<&String> = top.as_object().unwrap().keys().collect();
    assert_eq!(keys.len(), 5, "unexpected fields: {keys:?}");
}

#[tokio::test]
async fn semantic_search_honours_event_type_and_min_similarity_filters() {
    let h = ready_harness();
    ingest(
        &h.store,
        "tenant-a",
        "billing.invoice_paid",
        "inv-1",
        "invoice paid",
    );
    let refund = ingest(
        &h.store,
        "tenant-a",
        "billing.refund",
        "inv-1",
        "invoice refunded",
    );
    ingest(
        &h.store,
        "tenant-a",
        "user.signed_up",
        "user-1",
        "completely unrelated words",
    );
    h.search.catch_up().await.unwrap();

    let (status, body) = send(
        &h.app,
        Some(&h.key_a),
        "GET",
        "/api/v1/search/semantic?query=invoice&event_type=billing.refund",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(ids(&body), vec![refund.to_string()]);

    let (status, body) = send(
        &h.app,
        Some(&h.key_a),
        "GET",
        "/api/v1/search/semantic?query=invoice&min_similarity=0.3",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["count"], 2);
}

#[tokio::test]
async fn hybrid_search_fuses_semantic_and_keyword_scores_for_the_callers_tenant_only() {
    let h = ready_harness();
    let shipped = ingest(
        &h.store,
        "tenant-a",
        "order.shipped",
        "order-7",
        "order shipped to berlin",
    );
    ingest(
        &h.store,
        "tenant-a",
        "order.created",
        "order-8",
        "basket checkout started",
    );
    let foreign = ingest(
        &h.store,
        "tenant-b",
        "order.shipped",
        "order-7",
        "order shipped to berlin",
    );
    h.search.catch_up().await.unwrap();

    let (status, body) = send(
        &h.app,
        Some(&h.key_a),
        "POST",
        "/api/v1/search/hybrid",
        Some(json!({ "query": "shipped berlin", "limit": 5 })),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let found = ids(&body);
    assert_eq!(found[0], shipped.to_string());
    assert!(!found.contains(&foreign.to_string()));
    assert_eq!(body["count"], found.len());
    let top = &body["results"][0];
    assert!(top["semantic_score"].as_f64().unwrap() > 0.5);
    assert!(top["keyword_score"].as_f64().unwrap() > 0.0);
    assert!(top["score"].as_f64().unwrap() > 0.0);
    assert_eq!(top["event_type"], "order.shipped");
    assert_eq!(top["entity_id"], "order-7");
    assert!(top["timestamp"].as_str().is_some());
    assert_eq!(top.as_object().unwrap().len(), 7);
}

#[tokio::test]
async fn hybrid_search_filters_by_entity_and_event_type() {
    let h = ready_harness();
    ingest(
        &h.store,
        "tenant-a",
        "order.shipped",
        "order-1",
        "order shipped",
    );
    let wanted = ingest(
        &h.store,
        "tenant-a",
        "order.shipped",
        "order-2",
        "order shipped",
    );
    ingest(
        &h.store,
        "tenant-a",
        "order.created",
        "order-2",
        "order created",
    );
    h.search.catch_up().await.unwrap();

    let (status, body) = send(
        &h.app,
        Some(&h.key_a),
        "POST",
        "/api/v1/search/hybrid",
        Some(json!({
            "query": "order",
            "entity_id": "order-2",
            "event_type": "order.shipped",
            "semantic_weight": 0.2,
            "keyword_weight": 0.8
        })),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(ids(&body), vec![wanted.to_string()]);
}

#[tokio::test]
async fn events_ingested_after_the_first_build_become_searchable_on_the_next_catch_up() {
    let h = ready_harness();
    ingest(&h.store, "tenant-a", "note.added", "n-1", "first note");
    h.search.catch_up().await.unwrap();
    let later = ingest(
        &h.store,
        "tenant-a",
        "note.added",
        "n-2",
        "zeppelin sighting",
    );

    assert_eq!(h.search.catch_up().await.unwrap(), 1);
    let (status, body) = send(
        &h.app,
        Some(&h.key_a),
        "POST",
        "/api/v1/search/hybrid",
        Some(json!({ "query": "zeppelin", "limit": 1 })),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(ids(&body), vec![later.to_string()]);
    assert_eq!(h.search.catch_up().await.unwrap(), 0);
}

#[tokio::test]
async fn tenant_b_sees_only_its_own_events_through_both_routes() {
    let h = ready_harness();
    ingest(
        &h.store,
        "tenant-a",
        "doc.saved",
        "d-1",
        "quarterly forecast",
    );
    let own = ingest(
        &h.store,
        "tenant-b",
        "doc.saved",
        "d-1",
        "quarterly forecast",
    );
    h.search.catch_up().await.unwrap();

    let (_, semantic) = send(
        &h.app,
        Some(&h.key_b),
        "GET",
        "/api/v1/search/semantic?query=quarterly%20forecast",
        None,
    )
    .await;
    let (_, hybrid) = send(
        &h.app,
        Some(&h.key_b),
        "POST",
        "/api/v1/search/hybrid",
        Some(json!({ "query": "quarterly forecast" })),
    )
    .await;

    assert_eq!(ids(&semantic), vec![own.to_string()]);
    assert_eq!(ids(&hybrid), vec![own.to_string()]);
}

#[tokio::test]
async fn a_tenant_id_naming_another_tenant_is_refused_and_the_own_tenant_is_accepted() {
    let h = ready_harness();
    ingest(&h.store, "tenant-b", "doc.saved", "d-1", "secret plans");
    h.search.catch_up().await.unwrap();

    let (status, body) = send(
        &h.app,
        Some(&h.key_a),
        "GET",
        "/api/v1/search/semantic?query=secret&tenant_id=tenant-b",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .contains("authenticated tenant")
    );

    let (status, body) = send(
        &h.app,
        Some(&h.key_a),
        "POST",
        "/api/v1/search/hybrid",
        Some(json!({ "query": "secret", "tenant_id": "tenant-b" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].is_string());

    let (status, body) = send(
        &h.app,
        Some(&h.key_a),
        "GET",
        "/api/v1/search/semantic?query=secret&tenant_id=tenant-a",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["count"], 0);
}

#[tokio::test]
async fn without_an_embedding_model_hybrid_answers_keyword_only_and_semantic_503() {
    let h = harness(|s| SearchApiState::new(Some(s)));
    let saved = ingest(
        &h.store,
        "tenant-a",
        "doc.saved",
        "d-1",
        "quarterly invoice",
    );
    ingest(
        &h.store,
        "tenant-b",
        "doc.saved",
        "d-2",
        "quarterly invoice",
    );
    h.search.catch_up().await.unwrap();

    let (semantic_status, semantic) = send(
        &h.app,
        Some(&h.key_a),
        "GET",
        "/api/v1/search/semantic?query=invoice",
        None,
    )
    .await;
    let (hybrid_status, hybrid) = send(
        &h.app,
        Some(&h.key_a),
        "POST",
        "/api/v1/search/hybrid",
        Some(json!({ "query": "invoice" })),
    )
    .await;

    assert_eq!(semantic_status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        semantic["error"]
            .as_str()
            .unwrap()
            .contains("embedding model")
    );
    assert!(semantic.get("results").is_none());

    assert_eq!(hybrid_status, StatusCode::OK);
    assert_eq!(ids(&hybrid), vec![saved.to_string()]);
    assert!(hybrid["results"][0]["semantic_score"].is_null());
    assert!(hybrid["results"][0]["keyword_score"].is_number());
}

#[tokio::test]
async fn without_an_embedding_model_hybrid_answers_503_until_the_index_is_built() {
    let h = harness(|s| SearchApiState::new(Some(s)));
    ingest(&h.store, "tenant-a", "doc.saved", "d-1", "anything");

    let (status, body) = send(
        &h.app,
        Some(&h.key_a),
        "POST",
        "/api/v1/search/hybrid",
        Some(json!({ "query": "anything" })),
    )
    .await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .contains("still being built")
    );
}

#[tokio::test]
async fn both_routes_answer_503_until_the_index_is_built() {
    let h = ready_harness();
    ingest(&h.store, "tenant-a", "doc.saved", "d-1", "anything");

    let (status, body) = send(
        &h.app,
        Some(&h.key_a),
        "POST",
        "/api/v1/search/hybrid",
        Some(json!({ "query": "anything" })),
    )
    .await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .contains("still being built")
    );
}

#[tokio::test]
async fn both_routes_answer_503_when_the_search_engine_is_absent() {
    let h = harness(|_| SearchApiState::new(None));

    let (semantic_status, semantic) = send(
        &h.app,
        Some(&h.key_a),
        "GET",
        "/api/v1/search/semantic?query=anything",
        None,
    )
    .await;
    let (hybrid_status, hybrid) = send(
        &h.app,
        Some(&h.key_a),
        "POST",
        "/api/v1/search/hybrid",
        Some(json!({ "query": "anything" })),
    )
    .await;

    assert_eq!(semantic_status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(hybrid_status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(semantic["error"].is_string());
    assert!(hybrid["error"].is_string());
}

#[tokio::test]
async fn a_request_without_a_credential_is_rejected_before_searching() {
    let h = ready_harness();
    h.search.catch_up().await.unwrap();

    let (status, _) = send(
        &h.app,
        None,
        "GET",
        "/api/v1/search/semantic?query=anything",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn malformed_search_requests_answer_400_with_an_error_body() {
    let h = ready_harness();
    h.search.catch_up().await.unwrap();

    for uri in [
        "/api/v1/search/semantic?query=%20",
        "/api/v1/search/semantic?query=x&limit=0",
        "/api/v1/search/semantic?query=x&limit=101",
        "/api/v1/search/semantic?query=x&min_similarity=2",
        "/api/v1/search/semantic",
    ] {
        let (status, body) = send(&h.app, Some(&h.key_a), "GET", uri, None).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {body}");
        assert!(body["error"].is_string(), "{uri}: {body}");
    }

    for payload in [
        json!({ "query": "" }),
        json!({ "query": "x", "semantic_weight": -1.0 }),
        json!({ "limit": 3 }),
    ] {
        let (status, body) = send(
            &h.app,
            Some(&h.key_a),
            "POST",
            "/api/v1/search/hybrid",
            Some(payload.clone()),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{payload}: {body}");
        assert!(body["error"].is_string());
    }
}
