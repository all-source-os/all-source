//! Server stores do not register `entity_snapshots`, so every per-name admin route
//! answers 404 for it. The `event_counters` control proves a 404 comes from the
//! handler and not from an unmatched route.

#![cfg(feature = "server")]

use allsource_core::{
    AuthManager, RateLimiter, Role,
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
use std::sync::Arc;
use tower::ServiceExt;

async fn status(app: &axum::Router, method: Method, path: &str, key: &str) -> StatusCode {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header("authorization", format!("Bearer {key}"))
        .body(Body::empty())
        .unwrap();
    app.clone().oneshot(request).await.unwrap().status()
}

#[tokio::test]
async fn server_admin_routes_do_not_expose_entity_snapshots() {
    let (config, _mode) =
        EventStoreConfig::from_env_vars(None, None, None, None, None, None, None, None);
    let store = Arc::new(EventStore::with_config(config));
    let auth = Arc::new(AuthManager::default());
    let (_, key) = auth.create_api_key(
        "projection-admin".into(),
        "admin-tenant".into(),
        Role::Admin,
        None,
    );
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

    assert_eq!(
        status(
            &app,
            Method::GET,
            "/api/v1/projections/event_counters",
            &key
        )
        .await,
        StatusCode::OK
    );

    let routes = [
        (Method::GET, "/api/v1/projections/entity_snapshots"),
        (Method::DELETE, "/api/v1/projections/entity_snapshots"),
        (Method::POST, "/api/v1/projections/entity_snapshots/reset"),
        (Method::POST, "/api/v1/projections/entity_snapshots/pause"),
        (Method::POST, "/api/v1/projections/entity_snapshots/start"),
    ];
    for (method, path) in routes {
        assert_eq!(
            status(&app, method.clone(), path, &key).await,
            StatusCode::NOT_FOUND,
            "{method} {path}"
        );
    }
}
