use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use tower::ServiceExt;
use wiremock::MockServer;

use super::{
    tests::{TEST_KEY, hosted_state, hosted_state_stateful, test_state},
    *,
};

struct Reply {
    status: StatusCode,
    body: Value,
    empty: bool,
}

async fn send(
    app: &Router,
    method: &str,
    uri: &str,
    headers: &[(&str, &str)],
    body: Option<Value>,
) -> Reply {
    let mut builder = Request::builder().method(method).uri(uri);
    for (k, v) in headers {
        builder = builder.header(*k, *v);
    }
    let req = match body {
        Some(b) => builder
            .header("content-type", "application/json")
            .body(Body::from(b.to_string()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    };
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    Reply {
        status,
        empty: bytes.is_empty(),
        body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    }
}

fn bearer() -> String {
    format!("Bearer {TEST_KEY}")
}

#[tokio::test]
async fn every_routed_path_and_method_is_in_the_openapi_document() {
    let doc = openapi_document();
    let routes = http_routes();
    let mut op_count = 0;
    for path_item in doc["paths"].as_object().unwrap().values() {
        op_count += path_item.as_object().unwrap().len();
    }
    assert_eq!(op_count, routes.len());

    let app = router(test_state().await);
    for r in &routes {
        assert!(
            doc["paths"][r.path][r.method].is_object(),
            "{} {} missing from the OpenAPI document",
            r.method,
            r.path
        );
        let uri = r.path.replace("{id}", "x").replace("{field}", "f");
        let body = (r.method != "get" && r.method != "delete").then(|| json!({}));
        let reply = send(&app, &r.method.to_uppercase(), &uri, &[], body).await;
        assert_ne!(
            reply.status,
            StatusCode::METHOD_NOT_ALLOWED,
            "{} {uri} is documented but not routed",
            r.method
        );
        assert!(
            !(reply.status == StatusCode::NOT_FOUND && reply.empty),
            "{} {uri} fell through to the router fallback",
            r.method
        );
    }
}

#[tokio::test]
async fn the_openapi_document_is_served_under_the_prime_path() {
    let reply = send(&router(test_state().await), "GET", OPENAPI_PATH, &[], None).await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.body["openapi"], "3.1.0");
    assert!(reply.body["paths"]["/api/v1/prime/types"]["post"].is_object());

    let server = MockServer::start().await;
    let hosted = router(hosted_state(&server).await);
    let reply = send(
        &hosted,
        "GET",
        OPENAPI_PATH,
        &[("authorization", &bearer())],
        None,
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
}

#[tokio::test]
async fn a_hosted_request_without_the_service_credential_is_refused_before_its_tenant_is_read() {
    let server = MockServer::start().await;
    let app = router(hosted_state(&server).await);
    for (method, uri, body) in [
        ("GET", "/api/v1/prime/stats", None),
        (
            "POST",
            "/mcp",
            Some(json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"})),
        ),
    ] {
        let reply = send(
            &app,
            method,
            uri,
            &[("x-tenant-id", "tenant-a")],
            body.clone(),
        )
        .await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{uri}");
        let reply = send(
            &app,
            method,
            uri,
            &[
                ("x-tenant-id", "tenant-a"),
                ("authorization", "Bearer wrong-key!"),
            ],
            body,
        )
        .await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{uri}");
    }
}

#[tokio::test]
async fn a_hosted_request_with_the_credential_but_no_tenant_is_refused() {
    let server = MockServer::start().await;
    let app = router(hosted_state(&server).await);
    for (method, uri, body) in [
        ("GET", "/api/v1/prime/graph", None),
        ("GET", "/api/v1/prime/types", None),
        (
            "POST",
            "/mcp",
            Some(json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"})),
        ),
    ] {
        let reply = send(&app, method, uri, &[("authorization", &bearer())], body).await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{uri}");
        assert!(
            reply.body["error"]
                .as_str()
                .unwrap()
                .contains("X-Tenant-Id"),
            "{}",
            reply.body
        );
    }
}

#[tokio::test]
async fn a_caller_supplied_tenant_never_overrides_the_authenticated_one() {
    let server = MockServer::start().await;
    let app = router(hosted_state(&server).await);
    let auth = bearer();
    let headers = [
        ("authorization", auth.as_str()),
        ("x-tenant-id", "tenant-a"),
    ];

    let reply = send(
        &app,
        "GET",
        "/api/v1/prime/graph?tenant_id=tenant-b",
        &headers,
        None,
    )
    .await;
    assert_eq!(reply.status, StatusCode::FORBIDDEN, "{}", reply.body);
    let reply = send(
        &app,
        "GET",
        "/api/v1/prime/graph?tenant_id=tenant-a",
        &headers,
        None,
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);

    let reply = send(
        &app,
        "POST",
        "/api/v1/prime/recall",
        &headers,
        Some(json!({ "vector": [1.0, 0.0], "tenant_id": "tenant-b" })),
    )
    .await;
    assert_eq!(reply.status, StatusCode::FORBIDDEN, "{}", reply.body);

    let reply = send(
        &app,
        "POST",
        "/mcp",
        &headers,
        Some(json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": { "name": "prime_stats", "arguments": { "tenant_id": "tenant-b" } }
        })),
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.body["result"]["isError"], json!(true));
}

#[tokio::test]
async fn a_hosted_server_without_a_configured_key_binds_no_tenant() {
    let server = MockServer::start().await;
    let hosted = HostedPrime::connect(server.uri(), None, 8, std::time::Duration::from_mins(1));
    let app = router(Arc::new(AppState {
        prime: None,
        recall: None,
        hosted: Some(Arc::new(hosted)),
        api_key: None,
    }));
    let reply = send(
        &app,
        "GET",
        "/api/v1/prime/stats",
        &[("x-tenant-id", "tenant-a")],
        None,
    )
    .await;
    assert_eq!(reply.status, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn embedded_rest_stays_open_and_mcp_requires_the_key_when_one_is_set() {
    let prime = Prime::open_in_memory().await.unwrap();
    let recall = RecallEngine::with_deps(
        prime.recall_deps(),
        &allsource_core::prime::recall::IndexConfig::default(),
    );
    let app = router(Arc::new(AppState {
        prime: Some(Arc::new(prime)),
        recall: Some(recall),
        hosted: None,
        api_key: Some(TEST_KEY.to_string()),
    }));
    let reply = send(&app, "GET", "/api/v1/prime/stats", &[], None).await;
    assert_eq!(reply.status, StatusCode::OK);
    let list = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"});
    let reply = send(&app, "POST", "/mcp", &[], Some(list.clone())).await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
    let reply = send(
        &app,
        "POST",
        "/mcp",
        &[("authorization", &bearer())],
        Some(list),
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK);
}

#[tokio::test]
async fn a_strict_rest_refusal_names_the_field_expected_kind_and_received_value() {
    let app = router(test_state().await);
    let reply = send(
        &app,
        "POST",
        "/api/v1/prime/types",
        &[],
        Some(json!({
            "type_name": "person_t",
            "kind": "node",
            "schema": { "properties": { "age": { "kind": "integer", "min": 0 } } }
        })),
    )
    .await;
    assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
    assert_eq!(reply.body["version"], 1);

    let bad = json!({ "type": "person_t", "properties": { "age": "old" } });
    let reply = send(&app, "POST", "/api/v1/prime/nodes", &[], Some(bad.clone())).await;
    assert_eq!(
        reply.status,
        StatusCode::CREATED,
        "open accepts typed violations"
    );

    let reply = send(
        &app,
        "PUT",
        "/api/v1/prime/enforcement",
        &[],
        Some(json!({ "mode": "strict" })),
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK);
    let reply = send(&app, "GET", "/api/v1/prime/enforcement", &[], None).await;
    assert_eq!(reply.body["mode"], "strict");

    let reply = send(&app, "POST", "/api/v1/prime/nodes", &[], Some(bad)).await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST);
    assert_eq!(reply.body["violation"]["field"], "age");
    assert_eq!(reply.body["violation"]["received"], "old");
    assert!(
        reply.body["violation"]["expected"]
            .as_str()
            .unwrap()
            .contains("integer")
    );

    let reply = send(
        &app,
        "POST",
        "/api/v1/prime/types",
        &[],
        Some(json!({
            "type_name": "person_t",
            "kind": "node",
            "schema": { "properties": { "age": { "kind": "string" } } }
        })),
    )
    .await;
    assert_eq!(reply.status, StatusCode::CONFLICT, "{}", reply.body);

    let reply = send(&app, "GET", "/api/v1/prime/types", &[], None).await;
    assert_eq!(reply.body["total"], 1);
    assert_eq!(reply.body["enforcement"], "strict");
}

#[tokio::test]
async fn hosted_rest_writes_carry_their_assertions_into_provenance() {
    let server = MockServer::start().await;
    let app = router(hosted_state_stateful(&server).await);
    let auth = bearer();
    let headers = [
        ("authorization", auth.as_str()),
        ("x-tenant-id", "tenant-rest"),
    ];

    let reply = send(
        &app,
        "POST",
        "/api/v1/prime/nodes",
        &headers,
        Some(json!({
            "type": "contact",
            "properties": { "name": "Ada" },
            "source": "user",
            "actor": "support@example.com"
        })),
    )
    .await;
    assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
    let ada = reply.body["entity_id"].as_str().unwrap().to_string();

    let reply = send(
        &app,
        "PATCH",
        &format!("/api/v1/prime/nodes/{ada}"),
        &headers,
        Some(json!({ "properties": { "title": "CTO" }, "source": "extraction", "run_id": "r-1", "confidence": 0.6 })),
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);

    let reply = send(
        &app,
        "GET",
        &format!("/api/v1/prime/nodes/{ada}/fields/name/provenance"),
        &headers,
        None,
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    assert_eq!(reply.body["assertion"]["source"], "user");
    assert_eq!(reply.body["assertion"]["actor"], "support@example.com");

    let reply = send(
        &app,
        "GET",
        &format!("/api/v1/prime/nodes/{ada}/fields/title/provenance"),
        &headers,
        None,
    )
    .await;
    assert_eq!(reply.body["assertion"]["source"], "extraction");
    assert_eq!(reply.body["assertion"]["run_id"], "r-1");
    assert_eq!(reply.body["assertion"]["confidence"], 0.6);

    let reply = send(
        &app,
        "POST",
        "/api/v1/prime/nodes",
        &headers,
        Some(json!({ "type": "org", "properties": { "name": "Acme" } })),
    )
    .await;
    let acme = reply.body["entity_id"].as_str().unwrap().to_string();
    let reply = send(
        &app,
        "POST",
        "/api/v1/prime/edges",
        &headers,
        Some(json!({ "source": ada, "target": acme, "relation": "works_at", "assertion_source": "import", "confidence": 2.0 })),
    )
    .await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{}", reply.body);
    assert!(
        reply.body["error"]
            .as_str()
            .unwrap()
            .contains("assertion.confidence")
    );
}
