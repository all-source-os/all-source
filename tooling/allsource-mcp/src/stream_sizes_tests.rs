use allsource_core::embedded::{Config, EmbeddedCore, IngestEvent};
use serde_json::{Value, json};

use super::super::{execute_tool, execute_tool_inner, tool_definitions};
use crate::{
    diagnostics::{AccessProfile, DiagnosticPolicy},
    remote::{RemoteStore, mock::serve_routes},
    stores::StoreRegistry,
};

async fn ingest(core: &EmbeddedCore, entity_id: &str, payload: Value) {
    core.ingest(IngestEvent {
        entity_id,
        event_type: "fixture.written",
        payload,
        metadata: None,
        tenant_id: None,
    })
    .await
    .expect("ingest");
}

/// Two `skill_run` streams of 4 and 1 events, one `auth_member` stream of 2.
async fn fixture_store() -> StoreRegistry {
    let core = EmbeddedCore::open(
        Config::builder()
            .single_tenant(true)
            .build()
            .expect("valid config"),
    )
    .await
    .expect("in-memory core");
    for n in 0..4 {
        ingest(
            &core,
            "t1:u1:skill_run:a1",
            json!({ "n": n, "text": "x".repeat(200) }),
        )
        .await;
    }
    ingest(&core, "t1:u1:skill_run:b2", json!({ "n": 0 })).await;
    for n in 0..2 {
        ingest(&core, "auth-member:u7", json!({ "n": n })).await;
    }
    StoreRegistry::from_cores(vec![("default", core)])
}

async fn registry_with_gateway(base: &str) -> StoreRegistry {
    fixture_store().await.with_remote(
        "remote",
        RemoteStore::new(base, Some("key-1".to_string())).expect("remote"),
    )
}

fn local() -> DiagnosticPolicy {
    DiagnosticPolicy::new(AccessProfile::Local, None, "local").expect("local policy")
}

fn class<'a>(result: &'a Value, name: &str) -> &'a Value {
    result["classes"]
        .as_array()
        .expect("classes")
        .iter()
        .find(|row| row["stream_class"] == name)
        .unwrap_or_else(|| panic!("class {name} in {result}"))
}

const STREAMS_BODY: &str = r#"{"streams":[
    {"stream_id":"t1:u1:skill_run:a1","event_count":900,"last_event_at":"2026-11-17T10:00:00Z","estimated_bytes":90000,"events_last_7d":40},
    {"stream_id":"t1:u1:skill_run:b2","event_count":100,"last_event_at":"2026-11-16T10:00:00Z","estimated_bytes":5000,"events_last_7d":0},
    {"stream_id":"auth-member:u7","event_count":50,"last_event_at":null,"estimated_bytes":2500,"events_last_7d":3}
],"total":3}"#;

const STATS_BODY: &str = r#"{"total_events":1050,"total_entities":3,"total_event_types":2,"total_ingested":1050,"event_types":{"fixture.written":1050},"oldest_event":"2026-01-01T00:00:00Z","newest_event":"2026-11-17T10:00:00Z"}"#;

#[test]
fn the_three_cost_tools_are_advertised_read_only() {
    let tools = tool_definitions(&local());
    for name in ["stream_sizes", "read_costs", "cap_check"] {
        let tool = tools
            .iter()
            .find(|tool| tool.name == name)
            .unwrap_or_else(|| panic!("{name} advertised"));
        assert!(tool.annotations.read_only_hint, "{name}");
        assert!(!tool.annotations.destructive_hint, "{name}");
        assert!(
            tool.input_schema["properties"]["store"].is_object(),
            "{name}"
        );
        assert!(
            tool.input_schema["properties"]["diagnostic"].is_object(),
            "{name}"
        );
    }
}

#[tokio::test]
async fn stream_sizes_on_an_embedded_store_ranks_streams_and_sums_classes() {
    let stores = fixture_store().await;

    let result = execute_tool_inner(
        &stores,
        &local(),
        "stream_sizes",
        &json!({ "sort": "events", "limit": 2 }),
    )
    .await
    .expect("sizes");

    let streams = result["streams"].as_array().expect("streams");
    assert_eq!(streams.len(), 2, "limit cuts the stream rows: {result}");
    assert_eq!(streams[0]["stream_id"], "t1:u1:skill_run:a1");
    assert_eq!(streams[0]["stream_class"], "skill_run");
    assert_eq!(streams[0]["events"], 4);
    assert_eq!(streams[0]["events_last_7d"], 4);
    assert!(streams[0]["bytes"].as_u64().expect("bytes") > 800);
    assert_eq!(streams[1]["stream_id"], "auth-member:u7");

    let skill_run = class(&result, "skill_run");
    assert_eq!(
        skill_run["streams"], 2,
        "classes cover every stream, not the cut"
    );
    assert_eq!(skill_run["events"], 5);
    assert_eq!(skill_run["max_stream_events"], 4);
    assert_eq!(class(&result, "auth_member")["events"], 2);
    assert_eq!(result["completeness"]["complete"], true);
    assert_eq!(result["context"]["store"]["kind"], "embedded");
}

#[tokio::test]
async fn stream_sizes_sorts_by_bytes_when_asked() {
    let stores = fixture_store().await;

    let result = execute_tool_inner(
        &stores,
        &local(),
        "stream_sizes",
        &json!({ "sort": "bytes" }),
    )
    .await
    .expect("sizes");

    assert_eq!(result["streams"][0]["stream_id"], "t1:u1:skill_run:a1");
    assert_eq!(result["classes"][0]["stream_class"], "skill_run");
}

#[tokio::test]
async fn stream_sizes_refuses_an_unknown_sort() {
    let stores = fixture_store().await;

    let error = execute_tool_inner(&stores, &local(), "stream_sizes", &json!({ "sort": "age" }))
        .await
        .expect_err("sort is events or bytes");

    assert!(
        error.to_string().starts_with("invalid argument:"),
        "{error}"
    );
}

#[tokio::test]
async fn stream_sizes_through_the_gateway_reads_streams_with_bytes_and_stats() {
    let (base, heads) = serve_routes(vec![
        ("/api/v1/streams", "200 OK", STREAMS_BODY),
        ("/api/v1/stats", "200 OK", STATS_BODY),
    ])
    .await;
    let stores = registry_with_gateway(&base).await;

    let result = execute_tool_inner(
        &stores,
        &local(),
        "stream_sizes",
        &json!({ "store": "remote", "sort": "bytes", "limit": 1 }),
    )
    .await
    .expect("sizes");
    let heads = heads.await.expect("fake gateway");

    let streams_head = heads
        .iter()
        .find(|head| head.starts_with("GET /api/v1/streams?"))
        .expect("streams requested");
    assert!(streams_head.contains("with_bytes=true"), "{streams_head}");
    assert!(streams_head.contains("sort=bytes"), "{streams_head}");
    assert!(
        streams_head
            .to_ascii_lowercase()
            .contains("authorization: bearer key-1"),
        "{streams_head}"
    );
    assert!(
        heads
            .iter()
            .any(|head| head.starts_with("GET /api/v1/stats "))
    );

    assert_eq!(result["streams"].as_array().expect("streams").len(), 1);
    assert_eq!(result["streams"][0]["bytes"], 90000);
    assert_eq!(class(&result, "skill_run")["events"], 1000);
    assert_eq!(class(&result, "skill_run")["bytes"], 95000);
    assert_eq!(class(&result, "skill_run")["events_last_7d"], 40);
    assert_eq!(result["tenant"]["totalEvents"], 1050);
    assert_eq!(result["tenant"]["oldestEvent"], "2026-01-01T00:00:00Z");
    assert_eq!(result["completeness"]["complete"], true);
    assert_eq!(result["context"]["store"]["kind"], "remote");
}

#[tokio::test]
async fn stream_sizes_through_the_gateway_says_when_classes_miss_streams() {
    let (base, heads) = serve_routes(vec![
        (
            "/api/v1/streams",
            "200 OK",
            r#"{"streams":[{"stream_id":"auth-member:u7","event_count":5,"last_event_at":null,"estimated_bytes":10,"events_last_7d":0}],"total":20001}"#,
        ),
        ("/api/v1/stats", "200 OK", STATS_BODY),
    ])
    .await;
    let stores = registry_with_gateway(&base).await;

    let result = execute_tool_inner(
        &stores,
        &local(),
        "stream_sizes",
        &json!({ "store": "remote" }),
    )
    .await
    .expect("sizes");
    heads.await.expect("fake gateway");

    assert_eq!(result["completeness"]["complete"], false);
    assert_eq!(result["completeness"]["reason"], "stream_limit_reached");
}

#[tokio::test]
async fn read_costs_on_an_embedded_store_is_unsupported_and_names_the_tool() {
    let stores = fixture_store().await;

    let result = execute_tool(&stores, &local(), "read_costs", &json!({})).await;

    assert_eq!(result["isError"], true);
    assert_eq!(result["structuredContent"]["error"]["code"], "UNSUPPORTED");
    let message = result["structuredContent"]["error"]["message"]
        .as_str()
        .expect("message");
    assert!(message.starts_with("unsupported:"), "{message}");
    assert!(message.contains("read_costs"), "{message}");
    assert!(message.contains("--remote"), "{message}");
}

#[tokio::test]
async fn read_costs_through_the_gateway_returns_caller_and_class_rows() {
    let (base, heads) = serve_routes(vec![(
        "/api/v1/read-costs",
        "200 OK",
        r#"{"callers":[{"caller":"lh.skill_catalog","stream_class":"skill","logical_reads":3,"pages":40,"whole_reads":3,"body_bytes":9000000,"single_truncated":1,"walk_abandoned":0,"approximate":false}],
            "classes":[{"stream_class":"skill","logical_reads":3,"pages":40,"whole_reads":3,"single_truncated":1,"walk_abandoned":2,"walk_evicted":1,"matched_sum":60000,"matched_max":20000,"max_pages":20}]}"#,
    )])
    .await;
    let stores = registry_with_gateway(&base).await;

    let result = execute_tool_inner(
        &stores,
        &local(),
        "read_costs",
        &json!({ "store": "remote", "order": "pages", "limit": 10 }),
    )
    .await
    .expect("costs");
    let heads = heads.await.expect("fake gateway");

    assert!(
        heads[0].starts_with("GET /api/v1/read-costs?"),
        "{}",
        heads[0]
    );
    assert!(heads[0].contains("order=pages"), "{}", heads[0]);
    assert!(heads[0].contains("limit=10"), "{}", heads[0]);
    assert_eq!(result["callers"][0]["caller"], "lh.skill_catalog");
    assert_eq!(result["callers"][0]["body_bytes"], 9_000_000);
    assert_eq!(result["classes"][0]["max_pages"], 20);
    assert_eq!(result["truncated"]["single"], 1);
    assert_eq!(result["truncated"]["walk_abandoned"], 2);
    assert_eq!(result["truncated"]["walk_evicted"], 1);
    assert_eq!(result["context"]["store"]["kind"], "remote");
}

#[tokio::test]
async fn read_costs_refuses_an_order_core_does_not_know() {
    let (base, _heads) = serve_routes(vec![]).await;
    let stores = registry_with_gateway(&base).await;

    let error = execute_tool_inner(
        &stores,
        &local(),
        "read_costs",
        &json!({ "store": "remote", "order": "latency" }),
    )
    .await
    .expect_err("order is closed");

    assert!(
        error.to_string().starts_with("invalid argument:"),
        "{error}"
    );
}

#[tokio::test]
async fn read_costs_maps_a_gateway_refusal_to_access_denied() {
    let (base, heads) = serve_routes(vec![(
        "/api/v1/read-costs",
        "403 Forbidden",
        r#"{"error":"scope"}"#,
    )])
    .await;
    let stores = registry_with_gateway(&base).await;

    let error = execute_tool_inner(
        &stores,
        &local(),
        "read_costs",
        &json!({ "store": "remote" }),
    )
    .await
    .expect_err("refused");
    heads.await.expect("fake gateway");

    assert!(error.to_string().starts_with("access denied:"), "{error}");
}

#[tokio::test]
async fn cap_check_on_an_embedded_store_lists_classes_at_the_threshold() {
    let stores = fixture_store().await;

    let result = execute_tool_inner(
        &stores,
        &local(),
        "cap_check",
        &json!({
            "caps": [
                { "stream_class": "skill_run", "cap_events": 5 },
                { "stream_class": "auth_member", "cap_events": 100 },
                { "stream_class": "billing", "cap_events": 10 }
            ]
        }),
    )
    .await
    .expect("check");

    let over: Vec<&str> = result["over"]
        .as_array()
        .expect("over")
        .iter()
        .map(|row| row["stream_class"].as_str().expect("class"))
        .collect();
    assert_eq!(over, ["skill_run"], "4 of 5 is 80%: {result}");
    assert_eq!(result["over"][0]["measured_events"], 4);
    assert_eq!(result["over"][0]["pct"], 80);
    assert_eq!(result["thresholdPct"], 80);
    assert_eq!(result["unmeasured"], json!(["billing"]));
    assert_eq!(result["completeness"]["complete"], true);
}

#[tokio::test]
async fn cap_check_can_compare_a_class_total_and_a_custom_threshold() {
    let stores = fixture_store().await;

    let result = execute_tool_inner(
        &stores,
        &local(),
        "cap_check",
        &json!({
            "caps": [{ "stream_class": "skill_run", "cap_events": 10, "measure": "class_total" }],
            "threshold_pct": 50
        }),
    )
    .await
    .expect("check");

    assert_eq!(result["over"][0]["measured_events"], 5);
    assert_eq!(result["over"][0]["pct"], 50);
}

#[tokio::test]
async fn cap_check_through_the_gateway_reads_stream_sizes() {
    let (base, heads) = serve_routes(vec![
        ("/api/v1/streams", "200 OK", STREAMS_BODY),
        ("/api/v1/stats", "200 OK", STATS_BODY),
    ])
    .await;
    let stores = registry_with_gateway(&base).await;

    let result = execute_tool_inner(
        &stores,
        &local(),
        "cap_check",
        &json!({
            "store": "remote",
            "caps": [
                { "stream_class": "skill_run", "cap_events": 1000 },
                { "stream_class": "auth_member", "cap_events": 1000 }
            ]
        }),
    )
    .await
    .expect("check");
    heads.await.expect("fake gateway");

    assert_eq!(result["over"].as_array().expect("over").len(), 1);
    assert_eq!(result["over"][0]["stream_class"], "skill_run");
    assert_eq!(result["over"][0]["measured_events"], 900);
    assert_eq!(result["context"]["store"]["kind"], "remote");
}

#[tokio::test]
async fn cap_check_refuses_caps_it_cannot_compare() {
    let stores = fixture_store().await;
    for args in [
        json!({}),
        json!({ "caps": [] }),
        json!({ "caps": [{ "stream_class": "skill_run", "cap_events": 0 }] }),
        json!({ "caps": [{ "stream_class": "Skill Run", "cap_events": 5 }] }),
        json!({ "caps": [{ "stream_class": "skill_run", "cap_events": 5, "measure": "mean" }] }),
        json!({ "caps": [{ "stream_class": "skill_run", "cap_events": 5 }], "threshold_pct": 0 }),
    ] {
        let error = execute_tool_inner(&stores, &local(), "cap_check", &args)
            .await
            .expect_err("refused");
        assert!(
            error.to_string().starts_with("invalid argument:"),
            "{args}: {error}"
        );
    }
}

#[tokio::test]
async fn a_view_tool_on_an_embedded_store_names_the_tool_it_refused() {
    let stores = fixture_store().await;

    let error = execute_tool_inner(
        &stores,
        &local(),
        "list_latest",
        &json!({ "view": "ops_status" }),
    )
    .await
    .expect_err("views need the gateway");

    assert!(
        error.to_string().starts_with("unsupported: list_latest"),
        "{error}"
    );
}
