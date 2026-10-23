use allsource_core::infrastructure::web::response_limit;
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
    middleware,
    routing::get,
};
use tower::ServiceExt;

#[tokio::test]
async fn oversized_core_response_does_not_cross_service_boundary() {
    let router = Router::new()
        .route("/large", get(|| async { vec![b'x'; 16 * 1024 * 1024 + 1] }))
        .layer(middleware::from_fn(response_limit::bounded));
    let response = router
        .oneshot(
            Request::builder()
                .uri("/large")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    let body = to_bytes(response.into_body(), 1024).await.unwrap();
    assert!(
        String::from_utf8(body.to_vec())
            .unwrap()
            .contains("response_too_large")
    );
}

#[tokio::test]
async fn normal_core_response_is_unchanged() {
    let router = Router::new()
        .route("/small", get(|| async { "workflow-ok" }))
        .layer(middleware::from_fn(response_limit::bounded));
    let response = router
        .oneshot(
            Request::builder()
                .uri("/small")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        to_bytes(response.into_body(), 1024).await.unwrap().as_ref(),
        b"workflow-ok"
    );
}
