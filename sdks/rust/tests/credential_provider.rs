//! A credential provider is asked for a bearer on every attempt; a static key
//! behaves as before.

use allsource::{ClientConfig, Error, QueryClient, QueryEventsParams};
use serde_json::json;
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

const QUERY_PATH: &str = "/api/v1/events/query";

fn empty_page() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({"events": [], "count": 0}))
}

async fn authorizations(server: &MockServer) -> Vec<String> {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter_map(|request| request.headers.get("authorization"))
        .filter_map(|value| value.to_str().ok().map(str::to_string))
        .collect()
}

#[tokio::test]
async fn a_static_key_is_sent_unchanged() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(QUERY_PATH))
        .respond_with(empty_page())
        .mount(&server)
        .await;
    let client = QueryClient::new(&server.uri(), "test-key").expect("client");

    client
        .query_events(QueryEventsParams::new())
        .await
        .expect("query");

    assert_eq!(authorizations(&server).await, vec!["Bearer test-key"]);
}

#[tokio::test]
async fn every_retry_attempt_asks_the_provider_again() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(QUERY_PATH))
        .respond_with(ResponseTemplate::new(500).set_body_json(json!({"error": "boom"})))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(QUERY_PATH))
        .respond_with(empty_page())
        .mount(&server)
        .await;
    let minted = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&minted);
    let mut config = ClientConfig::new(&server.uri(), "");
    config.retry.base_delay = Duration::from_millis(1);
    let client = QueryClient::with_config_and_credential_provider(config, move || {
        Ok::<_, Error>(format!("token-{}", counter.fetch_add(1, Ordering::SeqCst)))
    })
    .expect("an empty api_key is fine with a provider");

    client
        .query_events(QueryEventsParams::new())
        .await
        .expect("query after one retry");

    assert_eq!(
        authorizations(&server).await,
        vec!["Bearer token-0", "Bearer token-1"]
    );
    assert_eq!(minted.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn a_provider_failure_is_a_config_error_and_sends_nothing() {
    let server = MockServer::start().await;
    let client = QueryClient::with_config_and_credential_provider(
        ClientConfig::new(&server.uri(), ""),
        || Err(Error::Config("no signing key".into())),
    )
    .expect("client");

    let err = client
        .query_events(QueryEventsParams::new())
        .await
        .expect_err("provider failed");

    assert!(
        matches!(err, Error::Config(ref message) if message == "no signing key"),
        "{err:?}"
    );
    assert!(authorizations(&server).await.is_empty());
}

#[test]
fn client_config_still_builds_from_a_struct_literal() {
    let base = ClientConfig::new("http://localhost:3902", "test-key");
    let config = ClientConfig {
        base_url: base.base_url,
        api_key: base.api_key,
        timeout: base.timeout,
        retry: base.retry,
        circuit_breaker_threshold: 9,
        circuit_breaker_recovery: base.circuit_breaker_recovery,
    };
    QueryClient::with_config(config).expect("client");
}

#[test]
fn an_empty_api_key_without_a_provider_is_still_refused() {
    let err = QueryClient::new("http://localhost:3902", "").expect_err("refused");
    assert!(matches!(err, Error::Config(_)), "{err:?}");
}
