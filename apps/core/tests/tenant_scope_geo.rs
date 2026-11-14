//! `POST /api/v1/geospatial/query` answers for the caller's tenant only. The body
//! names no tenant, an absent limit is 10,000, and a larger limit is clamped to it.

#![cfg(feature = "server")]

use allsource_core::{
    AuthManager, Event, RateLimiter, Role,
    api_v1::NodeRole,
    infrastructure::{
        di::ContainerBuilder, repositories::InMemoryTenantRepository, web::api_v1::build_v1_router,
    },
    rate_limit::RateLimitConfig,
    store::{EventStore, EventStoreConfig},
};
use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
};
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;

const GEO_PATH: &str = "/api/v1/geospatial/query";

fn located(entity: usize, tenant: &str) -> Event {
    Event::from_strings(
        "location.update".into(),
        format!("entity-{entity}"),
        tenant.into(),
        json!({"action": "checkin"}),
        Some(json!({"location": {"lat": 40.7, "lng": -74.0}})),
    )
    .unwrap()
}

async fn geo(app: &axum::Router, key: &str, body: Value) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(Method::POST)
        .uri(GEO_PATH)
        .header("authorization", format!("Bearer {key}"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn tenant_scope_geo() {
    let (config, _mode) =
        EventStoreConfig::from_env_vars(None, None, None, None, None, None, None, None);
    let store = Arc::new(EventStore::with_config(config));
    let mut events: Vec<Event> = (0..10_001).map(|i| located(i, "tenant-a")).collect();
    events.push(located(20_000, "tenant-b"));
    events.push(located(20_001, "tenant-b"));
    store.ingest_batch(events).unwrap();

    let auth = Arc::new(AuthManager::default());
    let (_, key_a) = auth.create_api_key("geo-a".into(), "tenant-a".into(), Role::Admin, None);
    let (_, key_b) = auth.create_api_key("geo-b".into(), "tenant-b".into(), Role::Admin, None);
    let (_, key_c) = auth.create_api_key("geo-c".into(), "tenant-c".into(), Role::Admin, None);
    let app = build_v1_router(
        Arc::clone(&store),
        Arc::clone(&auth),
        Arc::new(InMemoryTenantRepository::new()),
        Arc::new(RateLimiter::new(RateLimitConfig::dev_mode())),
        ContainerBuilder::new()
            .with_in_memory_repositories()
            .build(),
        NodeRole::Leader,
        None,
        None,
        3910,
        None,
        None,
    )
    .await;
    let bbox = json!({"min_lat": 40.0, "min_lng": -75.0, "max_lat": 41.0, "max_lng": -73.0});

    let (status, body) = geo(&app, &key_a, json!({"bbox": bbox, "tenant_id": "tenant-b"})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], 10_000, "absent limit is 10,000");
    let foreign = body["results"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["event"]["tenant_id"] != "tenant-a")
        .count();
    assert_eq!(foreign, 0, "body tenant_id is ignored");

    let (_, body) = geo(&app, &key_b, json!({"bbox": bbox})).await;
    assert_eq!(body["total"], 2, "tenant-b sees its own two events");

    let (_, body) = geo(&app, &key_a, json!({"bbox": bbox, "limit": 50_000})).await;
    assert_eq!(body["total"], 10_000, "larger limit is clamped");

    let (_, body) = geo(&app, &key_a, json!({"bbox": bbox, "limit": 5})).await;
    assert_eq!(body["total"], 5, "smaller limit is honoured");

    let (_, body) = geo(&app, &key_c, json!({"bbox": bbox})).await;
    assert_eq!(body["total"], 0, "tenant with no resident events");
}

fn fn_body<'a>(source: &'a str, signature: &str, end: &str) -> &'a str {
    let start = source.find(signature).expect(signature);
    let rest = &source[start..];
    let end = rest.find(end).unwrap();
    &rest[..end]
}

#[test]
fn geo_resolves_through_the_index_without_a_snapshot() {
    let root = env!("CARGO_MANIFEST_DIR");
    let api = std::fs::read_to_string(format!("{root}/src/infrastructure/web/api.rs")).unwrap();
    let store = std::fs::read_to_string(format!("{root}/src/store.rs")).unwrap();

    let handler = fn_body(&api, "pub async fn geo_query(", "\n}\n");
    assert!(!handler.contains("snapshot_events"), "no snapshot clone");
    assert!(handler.contains("geo_query_for_tenant"));

    let query = fn_body(&store, "pub fn geo_query_for_tenant(", "\n    }\n");
    assert!(!query.contains("snapshot_events"), "no snapshot clone");
    assert!(query.contains("resident_event_for_tenant"));

    let resolver = fn_body(&store, "pub fn resident_event_for_tenant(", "\n    }\n");
    assert!(resolver.contains("self.index.get_by_id(event_id)"));
}
