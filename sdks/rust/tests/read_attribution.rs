//! Read attribution: the caller header on every GET, the per-request override,
//! one `ReadId` per paginated walk, and `body_bytes` on every decoded response.

use allsource::{ClientConfig, EventPaginator, QueryClient, QueryEventsParams, ReadId};
use serde_json::json;
use wiremock::{
    matchers::{method, path, query_param},
    Mock, MockServer, Request, ResponseTemplate,
};

const CALLER_HEADER: &str = "x-allsource-caller";
const READ_ID_HEADER: &str = "x-allsource-read-id";
const QUERY_PATH: &str = "/api/v1/events/query";

fn event_json(id: &str) -> serde_json::Value {
    json!({
        "id": id,
        "event_type": "auth.org.created",
        "entity_id": id,
        "payload": {},
        "timestamp": "2026-01-01T00:00:00Z",
        "version": 1,
        "tenant_id": "default"
    })
}

fn empty_page() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({"events": [], "count": 0}))
}

async fn received(server: &MockServer) -> Vec<Request> {
    server.received_requests().await.unwrap_or_default()
}

fn header(request: &Request, name: &str) -> Option<String> {
    request
        .headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

fn client_with_caller(server: &MockServer, caller: &'static str) -> QueryClient {
    let config = ClientConfig::new(&server.uri(), "test-key").with_caller(caller);
    QueryClient::with_config(config).expect("client")
}

#[tokio::test]
async fn default_caller_is_sent_on_every_transport_get() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(QUERY_PATH))
        .respond_with(empty_page())
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/entities"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "entities": [], "total": 0, "has_more": false
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/prime/nodes/n1/fields/f1/provenance"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": "ok", "version": "0.30.0"
        })))
        .mount(&server)
        .await;
    let client = client_with_caller(&server, "svc.reader");

    client
        .query_events(QueryEventsParams::new())
        .await
        .expect("query_events");
    client
        .list_entities(allsource::ListEntitiesParams::new())
        .await
        .expect("list_entities");
    let provenance = client
        .node_field_provenance("n1", "f1")
        .await
        .expect("provenance");
    assert!(provenance.is_none(), "404 is the get_optional path");
    client.health().await.expect("health");

    let requests = received(&server).await;
    assert_eq!(requests.len(), 4);
    for request in &requests {
        assert_eq!(
            header(request, CALLER_HEADER).as_deref(),
            Some("svc.reader"),
            "{}",
            request.url
        );
    }
}

#[tokio::test]
async fn per_request_caller_overrides_the_default() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(QUERY_PATH))
        .respond_with(empty_page())
        .mount(&server)
        .await;
    let client = client_with_caller(&server, "svc.default");

    client
        .query_events(QueryEventsParams::new().caller("svc.walker"))
        .await
        .expect("query");
    client
        .query_events(QueryEventsParams::new())
        .await
        .expect("query");

    let requests = received(&server).await;
    assert_eq!(
        header(&requests[0], CALLER_HEADER).as_deref(),
        Some("svc.walker")
    );
    assert_eq!(
        header(&requests[1], CALLER_HEADER).as_deref(),
        Some("svc.default")
    );
}

#[tokio::test]
async fn attribution_travels_as_headers_never_as_query_parameters() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(QUERY_PATH))
        .respond_with(empty_page())
        .mount(&server)
        .await;
    let client = QueryClient::new(&server.uri(), "test-key").expect("client");

    client
        .query_events(
            QueryEventsParams::new()
                .caller("svc.walker")
                .read_id(ReadId::new()),
        )
        .await
        .expect("query");

    let requests = received(&server).await;
    let query = requests[0].url.query().unwrap_or_default();
    assert!(!query.contains("caller"), "{query}");
    assert!(!query.contains("read_id"), "{query}");
    assert!(header(&requests[0], READ_ID_HEADER).is_some());
}

#[tokio::test]
async fn event_paginator_sends_one_read_id_on_every_page() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(QUERY_PATH))
        .and(query_param("offset", "0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "events": [event_json("e1"), event_json("e2")],
            "count": 2, "total_count": 3, "has_more": true
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(QUERY_PATH))
        .and(query_param("offset", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "events": [event_json("e3")],
            "count": 1, "total_count": 3, "has_more": false
        })))
        .mount(&server)
        .await;
    let client = QueryClient::new(&server.uri(), "test-key").expect("client");

    let all = client
        .query_events_paged(QueryEventsParams::new().limit(2))
        .collect_all()
        .await
        .expect("walk");
    assert_eq!(all.len(), 3);

    let requests = received(&server).await;
    assert_eq!(requests.len(), 2);
    let first = header(&requests[0], READ_ID_HEADER).expect("read id on page 1");
    let second = header(&requests[1], READ_ID_HEADER).expect("read id on page 2");
    assert_eq!(first, second);
    assert_eq!(first.len(), 16);
    assert!(first.bytes().all(|b| b.is_ascii_hexdigit()), "{first}");
}

#[tokio::test]
async fn two_walks_get_two_read_ids() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(QUERY_PATH))
        .respond_with(empty_page())
        .mount(&server)
        .await;
    let client = QueryClient::new(&server.uri(), "test-key").expect("client");
    let walk = || -> EventPaginator { client.query_events_paged(QueryEventsParams::new()) };

    walk().collect_all().await.expect("first walk");
    walk().collect_all().await.expect("second walk");

    let requests = received(&server).await;
    assert_ne!(
        header(&requests[0], READ_ID_HEADER),
        header(&requests[1], READ_ID_HEADER)
    );
}

#[tokio::test]
async fn body_bytes_equals_the_body_length() {
    let server = MockServer::start().await;
    let body = serde_json::to_vec(&json!({
        "events": [event_json("e1")],
        "count": 1, "total_count": 1, "has_more": false
    }))
    .expect("body");
    Mock::given(method("GET"))
        .and(path(QUERY_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_raw(body.clone(), "application/json"))
        .mount(&server)
        .await;
    let client = QueryClient::new(&server.uri(), "test-key").expect("client");

    let resp = client
        .query_events(QueryEventsParams::new())
        .await
        .expect("query");

    assert_eq!(resp.meta.body_bytes, Some(body.len() as u64));
}

#[tokio::test]
async fn read_id_is_sixteen_lowercase_hex_characters() {
    let id = ReadId::new();
    let text = id.as_str();
    assert_eq!(text.len(), 16);
    assert!(text
        .bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
    assert_ne!(ReadId::new().as_str(), ReadId::new().as_str());
}
