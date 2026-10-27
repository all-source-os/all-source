use super::*;
use allsource_core::prime::{
    EntityId,
    recall::{IndexConfig, RecallEngine},
};
use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use tower::ServiceExt; // for `oneshot`

async fn test_state() -> Arc<AppState> {
    let prime = Prime::open_in_memory().await.unwrap();
    let recall = RecallEngine::with_deps(prime.recall_deps(), &IndexConfig::default());
    Arc::new(AppState {
        prime: Some(Arc::new(prime)),
        recall: Some(recall),
        hosted: None,
    })
}

fn graph_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/v1/prime/graph", get(get_full_graph))
        .with_state(state)
}

#[tokio::test]
async fn graph_endpoint_returns_written_node_and_edge() {
    let state = test_state().await;
    let prime = state.prime.as_ref().unwrap();

    let org_id = prime
        .add_node("organization", json!({ "name": "Acme" }))
        .await
        .unwrap();
    let org = EntityId::node("organization", org_id.as_str()).to_wire();
    let contact_id = prime
        .add_node("contact", json!({ "name": "Alice" }))
        .await
        .unwrap();
    let contact = EntityId::node("contact", contact_id.as_str()).to_wire();
    prime
        .add_edge(&contact, &org, "works_at", None)
        .await
        .unwrap();

    let app = graph_router(state);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/prime/graph")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();

    let nodes = body["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 2);
    let org_node = nodes.iter().find(|n| n["id"] == org).unwrap();
    assert_eq!(org_node["properties"]["name"], "Acme");
    assert_eq!(org_node["has_vector"], false);

    let edges = body["edges"].as_array().unwrap();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0]["source"], contact);
    assert_eq!(edges[0]["target"], org);
    assert_eq!(edges[0]["relation"], "works_at");

    assert_eq!(body["stats"]["node_count"], 2);
    assert_eq!(body["stats"]["edge_count"], 1);
    assert_eq!(body["has_more"], false);
}

fn mcp_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/mcp", post(mcp_handler))
        .with_state(state)
}

async fn mcp_post(app: &Router, payload: Value) -> (StatusCode, Value) {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/mcp")
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, body)
}

#[tokio::test]
async fn mcp_http_initialize_list_and_call() {
    let app = mcp_router(test_state().await);

    let (status, body) = mcp_post(
        &app,
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["result"]["serverInfo"]["name"], "allsource-prime");
    assert!(body["result"]["protocolVersion"].is_string());

    let (status, body) = mcp_post(
        &app,
        json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let tools = body["result"]["tools"].as_array().unwrap();
    assert!(!tools.is_empty(), "tools/list must return the Prime tools");

    let (status, body) = mcp_post(
        &app,
        json!({
            "jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": { "name": "prime_stats", "arguments": {} }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        !body["result"].is_null(),
        "tools/call must return a result envelope"
    );
}

#[tokio::test]
async fn mcp_http_notification_returns_202() {
    let app = mcp_router(test_state().await);
    let (status, body) = mcp_post(
        &app,
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(body, Value::Null);
}

#[tokio::test]
async fn mcp_http_parse_error_is_jsonrpc_envelope() {
    let app = mcp_router(test_state().await);
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/mcp")
                .header("content-type", "application/json")
                .body(Body::from("not json"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["error"]["code"], -32700);
}

use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method as wm_method, path as wm_path},
};

async fn hosted_state(server: &MockServer) -> Arc<AppState> {
    Mock::given(wm_method("GET"))
        .and(wm_path("/api/v1/events/query"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "events": [] })))
        .mount(server)
        .await;
    Mock::given(wm_method("POST"))
        .and(wm_path("/api/v1/events"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "ok": true })))
        .mount(server)
        .await;

    // Stateless: no embedded prime/recall — exactly the production hosted
    // shape. Every request is served through `hosted`.
    let hosted = HostedPrime::connect(server.uri(), None, 8, std::time::Duration::from_mins(1));
    Arc::new(AppState {
        prime: None,
        recall: None,
        hosted: Some(Arc::new(hosted)),
    })
}

async fn hosted_state_stateful(server: &MockServer) -> Arc<AppState> {
    crate::hosted_test_core::mount_stateful_core(server).await;
    let hosted = HostedPrime::connect(server.uri(), None, 8, std::time::Duration::from_mins(1));
    Arc::new(AppState {
        prime: None,
        recall: None,
        hosted: Some(Arc::new(hosted)),
    })
}

async fn mcp_post_with_tenant(
    app: &Router,
    tenant: Option<&str>,
    payload: Value,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header("content-type", "application/json");
    if let Some(t) = tenant {
        builder = builder.header("x-tenant-id", t);
    }
    let resp = app
        .clone()
        .oneshot(builder.body(Body::from(payload.to_string())).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, body)
}

#[tokio::test]
async fn mcp_hosted_path_add_node_then_search() {
    let server = MockServer::start().await;
    let app = mcp_router(hosted_state_stateful(&server).await);

    let (status, body) = mcp_post_with_tenant(
        &app,
        Some("tenant-a"),
        json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": { "name": "prime_add_node", "arguments": { "type": "contact", "properties": { "name": "Alice" } } }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(
        body["result"]["isError"],
        json!(true),
        "add_node should succeed on hosted path"
    );

    let (status, body) = mcp_post_with_tenant(
        &app,
        Some("tenant-a"),
        json!({
            "jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": { "name": "prime_search", "arguments": { "type": "contact" } }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let text = body["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("Alice"),
        "hosted search should return the added node; got {text}"
    );
}

#[tokio::test]
async fn mcp_hosted_path_unsupported_tool_errors() {
    let server = MockServer::start().await;
    let app = mcp_router(hosted_state(&server).await);

    let (status, body) = mcp_post_with_tenant(
        &app,
        Some("tenant-a"),
        json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": { "name": "prime_index", "arguments": {} }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["result"]["isError"], json!(true));
    let text = body["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("not yet available on the hosted backend"));
}

#[tokio::test]
async fn mcp_no_tenant_stateless_returns_400() {
    let server = MockServer::start().await;
    let app = mcp_router(hosted_state(&server).await);

    let (status, body) = mcp_post_with_tenant(
        &app,
        None,
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body["error"].as_str().unwrap().contains("X-Tenant-Id"),
        "expected an X-Tenant-Id error, got {body}"
    );
}

fn rest_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/v1/prime/nodes", post(create_node))
        .route("/api/v1/prime/nodes/{id}", get(get_node))
        .route("/api/v1/prime/graph", get(get_full_graph))
        .route("/api/v1/prime/recall", post(recall))
        .with_state(state)
}

async fn rest_request(
    app: &Router,
    method: &str,
    uri: &str,
    tenant: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(t) = tenant {
        builder = builder.header("x-tenant-id", t);
    }
    let req = if let Some(b) = body {
        builder
            .header("content-type", "application/json")
            .body(Body::from(b.to_string()))
            .unwrap()
    } else {
        builder.body(Body::empty()).unwrap()
    };
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let val = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, val)
}

#[tokio::test]
async fn rest_hosted_create_then_get_node() {
    let server = MockServer::start().await;
    let app = rest_router(hosted_state_stateful(&server).await);

    let (status, body) = rest_request(
        &app,
        "POST",
        "/api/v1/prime/nodes",
        Some("tenant-a"),
        Some(json!({ "type": "contact", "properties": { "name": "Alice" } })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let entity_id = body["entity_id"].as_str().unwrap().to_string();
    assert!(body["node_id"].is_string());

    let (status, body) = rest_request(
        &app,
        "GET",
        &format!("/api/v1/prime/nodes/{entity_id}"),
        Some("tenant-a"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["type"], "contact");
    assert_eq!(body["properties"]["name"], "Alice");
    assert!(body["created_at"].is_string());
    assert!(body.get("labels").is_some());
}

#[tokio::test]
async fn rest_hosted_full_graph_shape() {
    let server = MockServer::start().await;
    let app = rest_router(hosted_state_stateful(&server).await);

    let (_, a) = rest_request(
        &app,
        "POST",
        "/api/v1/prime/nodes",
        Some("tenant-a"),
        Some(json!({ "type": "contact", "properties": { "name": "Alice" } })),
    )
    .await;
    let _ = a;
    let (_, _b) = rest_request(
        &app,
        "POST",
        "/api/v1/prime/nodes",
        Some("tenant-a"),
        Some(json!({ "type": "contact", "properties": { "name": "Bob" } })),
    )
    .await;

    let (status, body) =
        rest_request(&app, "GET", "/api/v1/prime/graph", Some("tenant-a"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["stats"]["node_count"], 2);
    assert!(body["nodes"].is_array());
    assert_eq!(body["has_more"], false);
}

#[tokio::test]
async fn rest_hosted_recall_shape() {
    let server = MockServer::start().await;
    let app = rest_router(hosted_state(&server).await);

    let (status, body) = rest_request(
        &app,
        "POST",
        "/api/v1/prime/recall",
        Some("tenant-a"),
        Some(json!({ "vector": [1.0, 0.0, 0.0, 0.0], "top_k": 5 })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["nodes"].is_array(), "recall must return a nodes array");
    assert_eq!(body["nodes"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn rest_hosted_recall_without_vector_is_400() {
    let server = MockServer::start().await;
    let app = rest_router(hosted_state(&server).await);

    let (status, body) = rest_request(
        &app,
        "POST",
        "/api/v1/prime/recall",
        Some("tenant-a"),
        Some(json!({ "top_k": 5 })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("vector"));
}

#[tokio::test]
async fn rest_hosted_missing_tenant_returns_400() {
    let server = MockServer::start().await;
    let app = rest_router(hosted_state(&server).await);

    let (status, body) = rest_request(
        &app,
        "POST",
        "/api/v1/prime/nodes",
        None,
        Some(json!({ "type": "contact", "properties": {} })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body["error"].as_str().unwrap().contains("X-Tenant-Id"),
        "expected an X-Tenant-Id error, got {body}"
    );
}
