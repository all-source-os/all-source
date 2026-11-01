use allsource_core::infrastructure::web::boot_gate::BootGate;
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
    routing::get,
};
use tower::ServiceExt;

fn get_request(uri: &str) -> Request<Body> {
    Request::builder().uri(uri).body(Body::empty()).unwrap()
}

#[tokio::test]
async fn every_route_answers_not_ready_until_the_gate_opens() {
    let gate = BootGate::new();

    for uri in ["/health", "/api/v1/events/query", "/internal/promote"] {
        let response = gate.router().oneshot(get_request(uri)).await.unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE, "{uri}");
        assert_eq!(response.headers()[header::RETRY_AFTER], "1", "{uri}");
        let body = to_bytes(response.into_body(), 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"], "not_ready", "{uri}");
    }
}

#[tokio::test]
async fn the_opened_router_serves_every_later_request() {
    let gate = BootGate::new();
    let served = gate.router();

    gate.open(Router::new().route("/health", get(|| async { "healthy" })));

    let response = served
        .clone()
        .oneshot(get_request("/health"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1024).await.unwrap();
    assert_eq!(&body[..], b"healthy");

    let missing = served.oneshot(get_request("/nope")).await.unwrap();
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    assert!(gate.is_open());
}

#[tokio::test]
async fn a_second_open_keeps_the_first_router() {
    let gate = BootGate::new();
    gate.open(Router::new().route("/health", get(|| async { "first" })));
    gate.open(Router::new().route("/health", get(|| async { "second" })));

    let response = gate.router().oneshot(get_request("/health")).await.unwrap();
    let body = to_bytes(response.into_body(), 1024).await.unwrap();
    assert_eq!(&body[..], b"first");
}
