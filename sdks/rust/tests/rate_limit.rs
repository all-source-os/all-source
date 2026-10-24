//! Rate-limit, circuit-breaker, conditional-read and compression behaviour of
//! the HTTP transport, against a mock server.

use allsource::{CircuitState, ClientConfig, Error, EventsQuery, QueryClient, QueryEventsParams};
use serde_json::json;
use std::time::Duration;
use wiremock::{
    matchers::{header, method, path},
    Mock, MockServer, ResponseTemplate,
};

const QUERY_PATH: &str = "/api/v1/events/query";

fn empty_page() -> serde_json::Value {
    json!({"events": [], "count": 0})
}

fn refusal(retry_after: &str, scope: &str) -> ResponseTemplate {
    ResponseTemplate::new(429)
        .insert_header("Retry-After", retry_after)
        .set_body_json(json!({
            "error": "tenant_rate_limited",
            "scope": scope,
            "retry_after": retry_after.parse::<u64>().unwrap_or(0),
        }))
}

fn client(server: &MockServer, configure: impl FnOnce(&mut ClientConfig)) -> QueryClient {
    let mut config = ClientConfig::new(&server.uri(), "test-key");
    config.retry.base_delay = Duration::from_millis(1);
    configure(&mut config);
    QueryClient::with_config(config).unwrap()
}

async fn hits(server: &MockServer) -> usize {
    server.received_requests().await.unwrap_or_default().len()
}

#[tokio::test]
async fn retry_after_makes_one_attempt_then_fails_fast_without_the_network() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(QUERY_PATH))
        .respond_with(refusal("3", "tenant"))
        .mount(&server)
        .await;
    let qs = client(&server, |_| {});

    let first = qs.query_events(QueryEventsParams::new()).await.unwrap_err();
    match &first {
        Error::RateLimited { retry_after, scope } => {
            assert_eq!(*retry_after, Some(Duration::from_secs(3)));
            assert_eq!(scope.as_deref(), Some("tenant"));
        }
        other => panic!("expected RateLimited, got {other:?}"),
    }
    assert_eq!(hits(&server).await, 1, "a 429 is never retried in-client");

    for _ in 0..5 {
        let err = qs.query_events(QueryEventsParams::new()).await.unwrap_err();
        let wait = err.retry_after().expect("fast fail carries the time left");
        assert!(
            wait > Duration::ZERO && wait <= Duration::from_secs(3),
            "{wait:?}"
        );
        assert!(
            matches!(err, Error::RateLimited { ref scope, .. } if scope.as_deref() == Some("tenant"))
        );
    }
    let clone = qs.clone();
    assert!(clone
        .query_events(QueryEventsParams::new())
        .await
        .unwrap_err()
        .is_rate_limited());
    assert_eq!(
        hits(&server).await,
        1,
        "calls inside Retry-After stay offline"
    );
    assert_eq!(qs.circuit_state(), CircuitState::Closed);
}

#[tokio::test]
async fn repeated_429s_never_open_the_breaker() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(QUERY_PATH))
        .respond_with(refusal("0", "global_fair_share"))
        .mount(&server)
        .await;
    let qs = client(&server, |c| c.circuit_breaker_threshold = 2);

    for _ in 0..6 {
        let err = qs.query_events(QueryEventsParams::new()).await.unwrap_err();
        assert!(matches!(err, Error::RateLimited { .. }), "{err:?}");
    }
    assert_eq!(hits(&server).await, 6);
    assert_eq!(qs.circuit_state(), CircuitState::Closed);
}

#[tokio::test]
async fn calls_resume_once_retry_after_has_passed() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(QUERY_PATH))
        .respond_with(refusal("1", "tenant"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(QUERY_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(empty_page()))
        .mount(&server)
        .await;
    let qs = client(&server, |_| {});

    assert!(qs.query_events(QueryEventsParams::new()).await.is_err());
    tokio::time::sleep(Duration::from_millis(1100)).await;
    qs.query_events(QueryEventsParams::new()).await.unwrap();
    assert_eq!(hits(&server).await, 2);
}

#[tokio::test]
async fn a_503_with_a_scope_is_a_refusal_and_a_bare_503_is_a_failure() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(QUERY_PATH))
        .respond_with(
            ResponseTemplate::new(503)
                .insert_header("Retry-After", "2")
                .set_body_json(json!({"error": "egress_capacity_unavailable", "scope": "global"})),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    let qs = client(&server, |c| c.retry.max_retries = 1);
    let err = qs.query_events(QueryEventsParams::new()).await.unwrap_err();
    assert!(
        matches!(err, Error::RateLimited { retry_after: Some(d), ref scope } if d == Duration::from_secs(2) && scope.as_deref() == Some("global")),
        "{err:?}"
    );
    assert_eq!(hits(&server).await, 1);

    let bare = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(QUERY_PATH))
        .respond_with(ResponseTemplate::new(503).set_body_json(json!({"error": "down"})))
        .mount(&bare)
        .await;
    let qs = client(&bare, |c| c.retry.max_retries = 1);
    let err = qs.query_events(QueryEventsParams::new()).await.unwrap_err();
    assert!(err.is_server_error(), "{err:?}");
    assert_eq!(hits(&bare).await, 2, "a bare 503 keeps its retry");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn half_open_admits_exactly_one_probe_among_twenty_callers() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(QUERY_PATH))
        .respond_with(ResponseTemplate::new(500).set_body_json(json!({"error": "boom"})))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(QUERY_PATH))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(empty_page())
                .set_delay(Duration::from_millis(300)),
        )
        .mount(&server)
        .await;
    let qs = client(&server, |c| {
        c.retry.max_retries = 0;
        c.circuit_breaker_threshold = 1;
        c.circuit_breaker_recovery = Duration::from_millis(100);
    });

    assert!(qs.query_events(QueryEventsParams::new()).await.is_err());
    assert_eq!(qs.circuit_state(), CircuitState::Open);
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(qs.circuit_state(), CircuitState::HalfOpen);

    let callers: Vec<_> = (0..20)
        .map(|_| {
            let qs = qs.clone();
            tokio::spawn(async move { qs.query_events(QueryEventsParams::new()).await })
        })
        .collect();
    let mut admitted = 0;
    let mut refused = 0;
    for caller in callers {
        match caller.await.unwrap() {
            Ok(_) => admitted += 1,
            Err(e) if e.is_circuit_open() => refused += 1,
            Err(e) => panic!("unexpected error {e:?}"),
        }
    }
    assert_eq!((admitted, refused), (1, 19));
    assert_eq!(hits(&server).await, 2, "the failure plus one probe");
    assert_eq!(qs.circuit_state(), CircuitState::Closed);
}

#[tokio::test]
async fn a_304_is_an_explicit_not_modified_result() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(QUERY_PATH))
        .and(header("if-none-match", "\"rev-7\""))
        .respond_with(
            ResponseTemplate::new(304)
                .insert_header("ETag", "\"rev-7\"")
                .insert_header("RateLimit", "\"meta\";r=99;t=1"),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(QUERY_PATH))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("ETag", "\"rev-7\"")
                .insert_header("RateLimit", "\"data\";r=9;t=2")
                .insert_header("RateLimit-Policy", "\"data\";q=10;w=1")
                .set_body_json(empty_page()),
        )
        .mount(&server)
        .await;
    let qs = client(&server, |_| {});

    let fresh = qs.query_events(QueryEventsParams::new()).await.unwrap();
    assert_eq!(fresh.meta.etag.as_deref(), Some("\"rev-7\""));
    assert_eq!(fresh.meta.rate_limit.as_deref(), Some("\"data\";r=9;t=2"));
    assert_eq!(
        fresh.meta.rate_limit_policy.as_deref(),
        Some("\"data\";q=10;w=1")
    );

    let etag = fresh.meta.etag.unwrap();
    let params = QueryEventsParams::new()
        .entity_id("e-1")
        .if_none_match(&etag);
    match qs.query_events_conditional(params.clone()).await.unwrap() {
        EventsQuery::NotModified(meta) => {
            assert_eq!(meta.etag.as_deref(), Some("\"rev-7\""));
            assert_eq!(meta.rate_limit.as_deref(), Some("\"meta\";r=99;t=1"));
        }
        EventsQuery::Modified(_) => panic!("expected NotModified"),
    }
    let err = qs.query_events(params).await.unwrap_err();
    assert!(
        matches!(err, Error::NotModified { etag: Some(ref e) } if e == "\"rev-7\""),
        "{err:?}"
    );
    assert_eq!(qs.circuit_state(), CircuitState::Closed);
}

fn accept_encoding_of_last(requests: &[wiremock::Request]) -> Option<String> {
    requests
        .last()?
        .headers
        .get("accept-encoding")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

#[cfg(feature = "gzip")]
#[tokio::test]
async fn gzip_is_negotiated_and_decoded_when_the_server_compresses() {
    use flate2::{write::GzEncoder, Compression};
    use std::io::Write;

    let body = json!({"events": [], "count": 0, "total_count": 42}).to_string();
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(body.as_bytes()).unwrap();
    let gzipped = encoder.finish().unwrap();

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(QUERY_PATH))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Encoding", "gzip")
                .insert_header("Content-Type", "application/json")
                .set_body_bytes(gzipped),
        )
        .mount(&server)
        .await;
    let qs = client(&server, |_| {});

    let resp = qs.query_events(QueryEventsParams::new()).await.unwrap();
    assert_eq!(resp.total_count, Some(42));
    let requests = server.received_requests().await.unwrap();
    assert!(
        accept_encoding_of_last(&requests).is_some_and(|v| v.contains("gzip")),
        "Accept-Encoding must offer gzip"
    );
}

#[cfg(feature = "gzip")]
#[tokio::test]
async fn gzip_feature_still_reads_identity_responses() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(QUERY_PATH))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"events": [], "count": 0, "total_count": 7})),
        )
        .mount(&server)
        .await;
    let qs = client(&server, |_| {});
    let resp = qs.query_events(QueryEventsParams::new()).await.unwrap();
    assert_eq!(resp.total_count, Some(7));
}

#[cfg(not(feature = "gzip"))]
#[tokio::test]
async fn without_the_gzip_feature_no_compression_is_offered() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(QUERY_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(empty_page()))
        .mount(&server)
        .await;
    let qs = client(&server, |_| {});
    qs.query_events(QueryEventsParams::new()).await.unwrap();
    let requests = server.received_requests().await.unwrap();
    assert!(
        !accept_encoding_of_last(&requests).is_some_and(|v| v.contains("gzip")),
        "identity only without the feature"
    );
}
