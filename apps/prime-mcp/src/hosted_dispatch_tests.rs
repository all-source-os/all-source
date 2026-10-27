use super::*;
use std::time::Duration;
use wiremock::MockServer;

async fn mount_empty_core(server: &MockServer) {
    crate::hosted_test_core::mount_stateful_core(server).await;
}

#[allow(clippy::needless_pass_by_value)] // test helper; clarity over &Value
fn req(id: i64, method: &str, params: Value) -> Request {
    // Build a Request via JSON so we don't depend on field visibility.
    serde_json::from_value(json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    }))
    .unwrap()
}

#[tokio::test]
async fn initialize_returns_server_info() {
    let server = MockServer::start().await;
    let hosted = HostedPrime::connect(server.uri(), None, 8, Duration::from_mins(1));
    let r = req(1, "initialize", json!({}));
    let resp = handle_request_hosted(&hosted, "tenant-a", &r)
        .await
        .unwrap();
    let v = serde_json::to_value(&resp).unwrap();
    assert_eq!(v["result"]["serverInfo"]["name"], "allsource-prime");
}

#[tokio::test]
async fn notifications_initialized_has_no_reply() {
    let server = MockServer::start().await;
    let hosted = HostedPrime::connect(server.uri(), None, 8, Duration::from_mins(1));
    let r = req(0, "notifications/initialized", json!({}));
    assert!(
        handle_request_hosted(&hosted, "tenant-a", &r)
            .await
            .is_none()
    );
}

#[tokio::test]
async fn add_node_then_search_round_trips_with_tenant() {
    let server = MockServer::start().await;
    mount_empty_core(&server).await;
    let hosted = HostedPrime::connect(server.uri(), None, 8, Duration::from_mins(1));

    let add = req(
        1,
        "tools/call",
        json!({ "name": "prime_add_node", "arguments": { "type": "contact", "properties": { "name": "Alice" } } }),
    );
    let resp = handle_request_hosted(&hosted, "tenant-a", &add)
        .await
        .unwrap();
    let v = serde_json::to_value(&resp).unwrap();
    assert_ne!(
        v["result"]["isError"],
        json!(true),
        "add_node should succeed"
    );

    let search = req(
        2,
        "tools/call",
        json!({ "name": "prime_search", "arguments": { "type": "contact" } }),
    );
    let resp = handle_request_hosted(&hosted, "tenant-a", &search)
        .await
        .unwrap();
    let v = serde_json::to_value(&resp).unwrap();
    let text = v["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("Alice"),
        "search should return the added node; got {text}"
    );
}

#[tokio::test]
async fn unsupported_tool_errors_without_crashing() {
    let server = MockServer::start().await;
    let hosted = HostedPrime::connect(server.uri(), None, 8, Duration::from_mins(1));
    let r = req(
        1,
        "tools/call",
        json!({ "name": "prime_index", "arguments": {} }),
    );
    let resp = handle_request_hosted(&hosted, "tenant-a", &r)
        .await
        .unwrap();
    let v = serde_json::to_value(&resp).unwrap();
    assert_eq!(v["result"]["isError"], json!(true));
    let text = v["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("not yet available on the hosted backend"));
}

#[tokio::test]
async fn hound_read_tools_over_shared_hosted_graph() {
    let server = MockServer::start().await;
    mount_empty_core(&server).await; // stateful fake Core
    let hosted = HostedPrime::connect(server.uri(), None, 8, Duration::from_mins(1));
    let t = "team-a";
    let _ = hosted.stats(t).await; // warm the tenant so writes apply to the cache

    let _f = hosted
        .add_node(t, "file", json!({ "path": "a.rs", "domain": "code" }))
        .await
        .unwrap();
    let a = hosted
        .add_node(
            t,
            "function",
            json!({ "name": "alpha", "file": "a.rs", "domain": "code" }),
        )
        .await
        .unwrap();
    let b = hosted
        .add_node(
            t,
            "function",
            json!({ "name": "beta", "file": "a.rs", "domain": "code" }),
        )
        .await
        .unwrap();
    hosted
        .add_edge_weighted(
            t,
            &format!("node:function:{}", a.as_str()),
            &format!("node:function:{}", b.as_str()),
            "calls",
            0.6,
            None,
        )
        .await
        .unwrap();

    let rep = call_hound_report_hosted(&hosted, t, &json!({})).await;
    let text = rep["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("\"node_count\": 3"), "report: {text}");

    let imp = call_hound_impact_hosted(&hosted, t, &json!({ "target": "beta" })).await;
    let text = imp["content"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("alpha"),
        "impact should surface caller alpha: {text}"
    );

    let pri = call_hound_pr_impact_hosted(&hosted, t, &json!({ "files": ["a.rs"] })).await;
    let text = pri["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("review_queue"), "pr_impact: {text}");
}
