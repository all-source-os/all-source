use allsource_core::infrastructure::web::response_limit;
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
    middleware,
    routing::get,
};
use std::{
    io::Read,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tower::ServiceExt;

fn served_router() -> Router {
    Router::new()
        .route(
            "/events",
            get(|| async { r#"{"events":[{"event_type":"synthetic.updated"}]}"#.repeat(64) }),
        )
        .route("/large", get(|| async { vec![b'x'; 16 * 1024 * 1024 + 1] }))
        .layer(middleware::from_fn(response_limit::bounded))
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
async fn gzip_request_gets_the_same_body_compressed() {
    let plain = served_router()
        .oneshot(get_request("/events", None))
        .await
        .unwrap();
    assert!(plain.headers().get(header::CONTENT_ENCODING).is_none());
    let plain = to_bytes(plain.into_body(), 1 << 20).await.unwrap();

    let gzipped = served_router()
        .oneshot(get_request("/events", Some("gzip")))
        .await
        .unwrap();
    assert_eq!(gzipped.status(), StatusCode::OK);
    assert_eq!(gzipped.headers()[header::CONTENT_ENCODING], "gzip");
    let encoded = to_bytes(gzipped.into_body(), 1 << 20).await.unwrap();
    assert!(encoded.len() < plain.len());

    let mut decoded = Vec::new();
    flate2::read::GzDecoder::new(encoded.as_ref())
        .read_to_end(&mut decoded)
        .unwrap();
    assert_eq!(decoded, plain.as_ref());
}

#[tokio::test]
async fn response_bound_measures_uncompressed_bytes() {
    let response = served_router()
        .oneshot(get_request("/large", Some("gzip")))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

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

#[tokio::test]
async fn response_bursts_wait_without_increasing_active_memory_concurrency() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let handler_active = active.clone();
        let handler_peak = peak.clone();
        let router = Router::new()
            .route(
                "/small",
                get(move || {
                    let active = handler_active.clone();
                    let peak = handler_peak.clone();
                    async move {
                        let count = active.fetch_add(1, Ordering::SeqCst) + 1;
                        peak.fetch_max(count, Ordering::SeqCst);
                        tokio::time::sleep(Duration::from_millis(30)).await;
                        active.fetch_sub(1, Ordering::SeqCst);
                        "workflow-ok"
                    }
                }),
            )
            .layer(middleware::from_fn(response_limit::bounded));
        let mut requests = tokio::task::JoinSet::new();
        for _ in 0..32 {
            let router = router.clone();
            requests.spawn(async move {
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
            });
        }
        while let Some(result) = requests.join_next().await {
            result.unwrap();
        }
        assert!(peak.load(Ordering::SeqCst) <= 16);
        assert_eq!(active.load(Ordering::SeqCst), 0);
    })
    .await
    .expect("response admission exceeded five seconds");
}
