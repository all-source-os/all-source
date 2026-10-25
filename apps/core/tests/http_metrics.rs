use allsource_core::infrastructure::{
    observability::metrics::MetricsRegistry,
    web::{http_metrics, response_limit},
};
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
    middleware,
    routing::get,
};
use std::sync::Arc;
use tower::ServiceExt;

const BODY: &str = r#"{"events":[{"event_type":"synthetic.updated","payload":{"n":1}}]}"#;

fn served_router(metrics: Arc<MetricsRegistry>) -> Router {
    Router::new()
        .route("/events/{id}", get(|| async { BODY.repeat(32) }))
        .layer(middleware::from_fn(response_limit::bounded))
        .layer(middleware::from_fn_with_state(
            metrics,
            http_metrics::record,
        ))
        .layer(response_limit::compression())
}

fn get_request(uri: &str, accept_encoding: Option<&str>) -> Request<Body> {
    let mut request = Request::builder().uri(uri);
    if let Some(encoding) = accept_encoding {
        request = request.header(header::ACCEPT_ENCODING, encoding);
    }
    request.body(Body::empty()).unwrap()
}

#[tokio::test]
async fn response_bytes_count_the_uncompressed_body_per_route_template() {
    let metrics = MetricsRegistry::new();
    let response = served_router(metrics.clone())
        .oneshot(get_request("/events/workflow-7", Some("gzip")))
        .await
        .unwrap();
    assert_eq!(response.headers()[header::CONTENT_ENCODING], "gzip");
    let encoded = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    let uncompressed = BODY.repeat(32).len() as u64;
    assert!((encoded.len() as u64) < uncompressed);

    let labels = ["GET", "/events/{id}", "200"];
    assert_eq!(
        metrics
            .http_response_bytes_total
            .with_label_values(&labels)
            .get(),
        uncompressed
    );
    assert_eq!(
        metrics.http_requests_total.with_label_values(&labels).get(),
        1
    );
    assert_eq!(metrics.http_requests_in_flight.get(), 0);
}

#[tokio::test]
async fn unrouted_requests_share_one_endpoint_label() {
    let metrics = MetricsRegistry::new();
    let response = served_router(metrics.clone())
        .oneshot(get_request("/no/such/tenant-specific/path", None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        metrics
            .http_requests_total
            .with_label_values(&["GET", "unmatched", "404"])
            .get(),
        1
    );
}
