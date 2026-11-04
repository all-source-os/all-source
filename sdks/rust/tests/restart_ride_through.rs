//! Calls wait out a Core restart instead of failing, against a mock server.

use allsource::{
    CircuitState, ClientConfig, CoreClient, Error, IngestEventInput, QueryClient, QueryEventsParams,
};
use serde_json::json;
use std::time::{Duration, Instant};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

const QUERY_PATH: &str = "/api/v1/events/query";
const INGEST_PATH: &str = "/api/v1/events";

fn config(uri: &str, window: Duration) -> ClientConfig {
    let mut config = ClientConfig::new(uri, "test-key");
    config.retry.base_delay = Duration::from_millis(1);
    config.retry.unavailable_window = window;
    config.circuit_breaker_threshold = 2;
    config
}

fn unavailable(body: serde_json::Value) -> ResponseTemplate {
    ResponseTemplate::new(503)
        .insert_header("Retry-After", "1")
        .set_body_json(body)
}

fn empty_page() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({"events": [], "count": 0}))
}

async fn hits(server: &MockServer) -> usize {
    server.received_requests().await.unwrap_or_default().len()
}

async fn refuse_then_answer(
    server: &MockServer,
    verb: &str,
    route: &str,
    refusals: u64,
    body: serde_json::Value,
    answer: ResponseTemplate,
) {
    Mock::given(method(verb))
        .and(path(route))
        .respond_with(unavailable(body))
        .up_to_n_times(refusals)
        .mount(server)
        .await;
    Mock::given(method(verb))
        .and(path(route))
        .respond_with(answer)
        .mount(server)
        .await;
}

#[tokio::test]
async fn a_read_waits_out_core_not_ready_more_times_than_max_retries() {
    let server = MockServer::start().await;
    refuse_then_answer(
        &server,
        "GET",
        QUERY_PATH,
        5,
        json!({"error": "not_ready"}),
        empty_page(),
    )
    .await;
    let qs = QueryClient::with_config(config(&server.uri(), Duration::from_secs(10))).unwrap();

    qs.query_events(QueryEventsParams::new()).await.unwrap();

    assert_eq!(hits(&server).await, 6);
    assert_eq!(
        qs.circuit_state(),
        CircuitState::Closed,
        "a restart never opens the breaker"
    );
}

#[tokio::test]
async fn a_write_is_resent_when_the_gateway_says_core_is_unavailable() {
    let server = MockServer::start().await;
    refuse_then_answer(
        &server,
        "POST",
        INGEST_PATH,
        4,
        json!({"error": "core_unavailable", "message": "The event store is restarting; retry shortly."}),
        ResponseTemplate::new(200).set_body_json(json!({"event_id": "e-1", "timestamp": "2026-11-04T00:00:00Z"})),
    )
    .await;
    let core = CoreClient::with_config(config(&server.uri(), Duration::from_secs(10))).unwrap();

    let response = core
        .ingest_event(IngestEventInput {
            event_type: "sdk.test.created".into(),
            entity_id: "entity-1".into(),
            payload: json!({}),
            ..Default::default()
        })
        .await
        .unwrap();

    assert_eq!(response.event_id, "e-1");
    assert_eq!(hits(&server).await, 5);
    assert_eq!(core.circuit_state(), CircuitState::Closed);
}

#[tokio::test]
async fn the_query_service_refusal_shape_is_recognised() {
    let server = MockServer::start().await;
    refuse_then_answer(
        &server,
        "GET",
        QUERY_PATH,
        4,
        json!({"error": {"code": "core_unavailable"}}),
        empty_page(),
    )
    .await;
    let qs = QueryClient::with_config(config(&server.uri(), Duration::from_secs(10))).unwrap();

    qs.query_events(QueryEventsParams::new()).await.unwrap();

    assert_eq!(hits(&server).await, 5);
}

#[tokio::test]
async fn a_read_waits_out_an_unreachable_egress_ledger() {
    let server = MockServer::start().await;
    refuse_then_answer(
        &server,
        "GET",
        QUERY_PATH,
        4,
        json!({"error": "egress_budget_unavailable", "reason": "ledger_unavailable"}),
        empty_page(),
    )
    .await;
    let qs = QueryClient::with_config(config(&server.uri(), Duration::from_secs(10))).unwrap();

    qs.query_events(QueryEventsParams::new()).await.unwrap();

    assert_eq!(hits(&server).await, 5);
}

#[tokio::test]
async fn a_restart_longer_than_the_window_fails_once_it_is_spent() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(QUERY_PATH))
        .respond_with(unavailable(json!({"error": "not_ready"})))
        .mount(&server)
        .await;
    let mut config = config(&server.uri(), Duration::from_millis(900));
    config.retry.max_retries = 0;
    let qs = QueryClient::with_config(config).unwrap();

    let started = Instant::now();
    let err = qs.query_events(QueryEventsParams::new()).await.unwrap_err();
    let elapsed = started.elapsed();

    assert!(err.is_core_restarting(), "{err:?}");
    assert!(elapsed >= Duration::from_millis(700), "{elapsed:?}");
    assert!(elapsed < Duration::from_millis(1500), "{elapsed:?}");
    assert_eq!(
        qs.circuit_state(),
        CircuitState::Closed,
        "one spent window is one breaker failure"
    );
}

#[tokio::test]
async fn a_zero_window_keeps_the_ordinary_retries() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(QUERY_PATH))
        .respond_with(unavailable(json!({"error": "not_ready"})))
        .mount(&server)
        .await;
    let qs = QueryClient::with_config(config(&server.uri(), Duration::ZERO)).unwrap();

    qs.query_events(QueryEventsParams::new()).await.unwrap_err();

    assert_eq!(
        hits(&server).await,
        4,
        "max_retries 3 plus the first attempt"
    );
}

#[tokio::test]
async fn a_write_is_not_held_through_an_egress_ledger_outage() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(INGEST_PATH))
        .respond_with(unavailable(
            json!({"error": "egress_budget_unavailable", "reason": "ledger_unavailable"}),
        ))
        .mount(&server)
        .await;
    let mut config = config(&server.uri(), Duration::from_secs(10));
    config.retry.max_retries = 0;
    let core = CoreClient::with_config(config).unwrap();

    let err = core
        .ingest_event(IngestEventInput {
            event_type: "sdk.test.created".into(),
            entity_id: "entity-3".into(),
            payload: json!({}),
            ..Default::default()
        })
        .await
        .unwrap_err();

    assert!(err.is_egress_ledger_unavailable(), "{err:?}");
    assert_eq!(hits(&server).await, 1, "Core may have run it");
}

#[tokio::test]
async fn other_server_errors_keep_the_retry_budget() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(QUERY_PATH))
        .respond_with(ResponseTemplate::new(503).set_body_json(json!({"error": "overloaded"})))
        .mount(&server)
        .await;
    let qs = QueryClient::with_config(config(&server.uri(), Duration::from_secs(10))).unwrap();

    let err = qs.query_events(QueryEventsParams::new()).await.unwrap_err();

    assert!(!err.is_core_restarting());
    assert_eq!(
        hits(&server).await,
        4,
        "max_retries 3 plus the first attempt"
    );
}

#[tokio::test]
async fn a_refused_connection_is_resent_until_core_listens() {
    let reserved = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = reserved.local_addr().unwrap();
    drop(reserved);
    let core = CoreClient::with_config(config(
        &format!("http://{address}"),
        Duration::from_secs(10),
    ))
    .unwrap();

    let server = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(600)).await;
        let listener = std::net::TcpListener::bind(address).unwrap();
        let server = MockServer::builder().listener(listener).start().await;
        Mock::given(method("POST"))
            .and(path(INGEST_PATH))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"event_id": "e-2", "timestamp": "2026-11-04T00:00:00Z"})),
            )
            .mount(&server)
            .await;
        tokio::time::sleep(Duration::from_secs(5)).await;
        server
    });

    let started = Instant::now();
    let response = core
        .ingest_event(IngestEventInput {
            event_type: "sdk.test.created".into(),
            entity_id: "entity-2".into(),
            payload: json!({}),
            ..Default::default()
        })
        .await
        .unwrap();

    assert_eq!(response.event_id, "e-2");
    assert!(started.elapsed() >= Duration::from_millis(500));
    assert_eq!(core.circuit_state(), CircuitState::Closed);
    server.abort();
}

#[test]
fn a_bad_gateway_is_not_a_restart() {
    let err = Error::Api {
        status: 502,
        message: "bad gateway".into(),
        body: Some(json!({"error": "core_unavailable"})),
    };
    assert!(!err.is_core_restarting(), "only a 503 says no handler ran");
}
