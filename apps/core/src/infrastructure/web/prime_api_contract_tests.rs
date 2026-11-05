use super::*;
use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use tower::ServiceExt;

async fn app() -> Router {
    let prime = Prime::open_in_memory().await.unwrap();
    prime_router().with_state(Arc::new(PrimeState { prime }))
}

async fn send(app: &Router, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    let builder = Request::builder().method(method).uri(uri);
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
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}

fn concrete(path: &str) -> String {
    path.split('/')
        .map(|seg| {
            if seg.starts_with('{') {
                "node:person:missing"
            } else {
                seg
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[tokio::test]
async fn every_routed_prime_path_and_method_appears_in_the_openapi_document() {
    let app = app().await;
    let doc = openapi_document();
    let routes = prime_routes();
    assert!(routes.len() >= 19, "route table shrank: {}", routes.len());
    for r in &routes {
        let documented = &doc["paths"][format!("/api/v1/prime{}", r.path)][r.method];
        assert!(
            documented.is_object(),
            "{} {} undocumented",
            r.method,
            r.path
        );
        if r.request.is_some() {
            assert!(documented["requestBody"].is_object());
        }

        let (status, body) = send(&app, &r.method.to_uppercase(), &concrete(r.path), None).await;
        assert_ne!(
            status,
            StatusCode::METHOD_NOT_ALLOWED,
            "{} {}",
            r.method,
            r.path
        );
        assert!(
            !(status == StatusCode::NOT_FOUND && body.is_null()),
            "{} {} is documented but not routed",
            r.method,
            r.path
        );
    }
    let documented_ops: usize = doc["paths"]
        .as_object()
        .unwrap()
        .values()
        .map(|p| p.as_object().unwrap().len())
        .sum();
    assert_eq!(documented_ops, routes.len());
}

#[tokio::test]
async fn openapi_document_is_served_and_lists_the_original_routes() {
    let app = app().await;
    let (status, doc) = send(&app, "GET", "/openapi.json", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(doc["openapi"], "3.1.0");
    for (p, m) in [
        ("/api/v1/prime/nodes", "post"),
        ("/api/v1/prime/nodes/{id}", "get"),
        ("/api/v1/prime/nodes/{id}", "patch"),
        ("/api/v1/prime/nodes/{id}", "delete"),
        ("/api/v1/prime/nodes/{id}/neighbors", "get"),
        ("/api/v1/prime/nodes/{id}/history", "get"),
        ("/api/v1/prime/edges", "post"),
        ("/api/v1/prime/edges/{id}", "delete"),
        ("/api/v1/prime/projections", "post"),
        ("/api/v1/prime/projections", "get"),
        ("/api/v1/prime/nodes/{id}/project", "post"),
        ("/api/v1/prime/nodes/{id}/fields/{field}/provenance", "get"),
        ("/api/v1/prime/stats", "get"),
        ("/api/v1/prime/graph", "get"),
        ("/api/v1/prime/types", "post"),
        ("/api/v1/prime/types", "get"),
        ("/api/v1/prime/enforcement", "put"),
        ("/api/v1/prime/enforcement", "get"),
    ] {
        assert!(doc["paths"][p][m].is_object(), "{m} {p} missing");
    }
    #[cfg(feature = "prime-vectors")]
    for (p, m) in [
        ("/api/v1/prime/vectors", "post"),
        ("/api/v1/prime/vectors/search", "post"),
        ("/api/v1/prime/vectors/{id}", "delete"),
        ("/api/v1/prime/recall", "post"),
    ] {
        assert!(doc["paths"][p][m].is_object(), "{m} {p} missing");
    }
}

#[tokio::test]
async fn http_strict_write_refusal_returns_the_structured_violation() {
    let app = app().await;
    let (status, _) = send(
        &app,
        "POST",
        "/types",
        Some(json!({
            "type_name": "person",
            "kind": "node",
            "schema": {"properties": {"age": {"kind": "integer"}}}
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, body) = send(&app, "PUT", "/enforcement", Some(json!({"mode": "strict"}))).await;
    assert_eq!(
        (status, body["mode"].clone()),
        (StatusCode::OK, json!("strict"))
    );

    let (status, body) = send(
        &app,
        "POST",
        "/nodes",
        Some(json!({"type": "person", "properties": {"age": "old"}})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["violation"]["field"], "age");
    assert_eq!(body["violation"]["expected"], "integer");
    assert_eq!(body["violation"]["received"], "old");

    let (_, body) = send(&app, "GET", "/enforcement", None).await;
    assert_eq!(body["mode"], "strict");
}

#[tokio::test]
async fn http_type_listing_and_breaking_reregistration_conflict() {
    let app = app().await;
    let schema = json!({"properties": {"status": {"kind": "enum", "values": ["a", "b"]}}});
    let (status, body) = send(
        &app,
        "POST",
        "/types",
        Some(json!({"type_name": "deal", "kind": "node", "schema": schema})),
    )
    .await;
    assert_eq!(
        (status, body["version"].clone()),
        (StatusCode::CREATED, json!(1))
    );

    let (status, _) = send(
        &app,
        "POST",
        "/types",
        Some(json!({"type_name": "deal", "kind": "node",
            "schema": {"properties": {"status": {"kind": "enum", "values": ["a"]}}}})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    let (_, body) = send(&app, "GET", "/types", None).await;
    assert_eq!(body["total"], 1);
    assert_eq!(body["types"][0]["version"], 1);
}

#[tokio::test]
async fn http_writes_accept_assertion_fields_and_provenance_returns_them() {
    let app = app().await;
    send(
        &app,
        "POST",
        "/projections",
        Some(json!({"entity_type": "person", "field_policies": {"name": "highest_priority"}})),
    )
    .await;
    let (status, created) = send(
        &app,
        "POST",
        "/nodes",
        Some(json!({
            "type": "person",
            "properties": {"name": "Alice Chen"},
            "source": "user",
            "actor": "alice"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let eid = created["entity_id"].as_str().unwrap().to_string();

    let (status, _) = send(
        &app,
        "PATCH",
        &format!("/nodes/{eid}"),
        Some(json!({
            "properties": {"name": "alice chen"},
            "source": "extraction",
            "model": "m",
            "run_id": "r",
            "derived_from": ["doc:1"],
            "confidence": 0.3
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = send(
        &app,
        "GET",
        &format!("/nodes/{eid}/fields/name/provenance"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["value"], "Alice Chen");
    assert_eq!(body["assertion"]["source"], "user");
    assert_eq!(body["assertion"]["actor"], "alice");
    assert_eq!(body["source_priority"], 1000);
    assert!(
        body["source_event_id"]
            .as_str()
            .is_some_and(|s| !s.is_empty())
    );
}

#[tokio::test]
async fn http_invalid_confidence_is_a_client_error() {
    let app = app().await;
    for bad in [json!(1.5), json!(-0.1)] {
        let (status, body) = send(
            &app,
            "POST",
            "/nodes",
            Some(json!({"type": "x", "properties": {}, "confidence": bad})),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body["error"].as_str().unwrap().contains("confidence"));
    }
    let (_, stats) = send(&app, "GET", "/stats", None).await;
    assert_eq!(stats["total_nodes"], 0);
}

#[test]
fn http_edge_body_carries_the_assertion_source_beside_the_edge_source_node() {
    let req: CreateEdgeReq = serde_json::from_value(json!({
        "source": "node:person:a",
        "target": "node:company:b",
        "relation": "works_at",
        "assertion_source": "user",
        "actor": "alice"
    }))
    .unwrap();
    assert_eq!(req.source, "node:person:a");
    assert_eq!(req.assertion.source.as_deref(), Some("user"));
    assert_eq!(req.assertion.actor.as_deref(), Some("alice"));
}

#[tokio::test]
async fn http_provenance_without_a_projection_is_answered_under_last_write() {
    let app = app().await;
    let (status, created) = send(
        &app,
        "POST",
        "/nodes",
        Some(json!({
            "type": "person",
            "properties": {"title": "CTO"},
            "source": "extraction",
            "model": "m",
            "run_id": "r1",
            "confidence": 0.7
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let eid = created["entity_id"].as_str().unwrap().to_string();

    let (status, body) = send(
        &app,
        "GET",
        &format!("/nodes/{eid}/fields/title/provenance"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["value"], "CTO");
    assert_eq!(body["policy_source"], "default");
    assert_eq!(body["merge_policy_applied"], "last_write");
    assert_eq!(body["assertion"]["source"], "extraction");
    assert_eq!(body["assertion"]["run_id"], "r1");
    assert_eq!(body["assertion"]["confidence"], 0.7);
}
