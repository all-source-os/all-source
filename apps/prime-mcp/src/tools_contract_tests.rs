use std::time::Duration;

use allsource_core::prime::{
    hosted::HostedPrime,
    recall::{IndexConfig, RecallEngine},
};
use serde_json::{Value, json};
use wiremock::MockServer;

use super::*;
use crate::{hosted_dispatch, protocol};

struct Embedded {
    prime: Prime,
    recall: RecallEngine,
}

impl Embedded {
    async fn new() -> Self {
        let prime = Prime::open_in_memory().await.unwrap();
        let recall = RecallEngine::with_deps(prime.recall_deps(), &IndexConfig::default());
        Self { prime, recall }
    }

    async fn call(&self, name: &str, args: Value) -> Value {
        call_tool(&self.prime, &self.recall, name, &args).await
    }
}

async fn hosted_call(hosted: &HostedPrime, tenant: &str, name: &str, args: Value) -> Value {
    let req: protocol::Request = serde_json::from_value(json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": { "name": name, "arguments": args }
    }))
    .unwrap();
    let resp = hosted_dispatch::handle_request_hosted(hosted, tenant, None, &req)
        .await
        .unwrap();
    serde_json::to_value(&resp).unwrap()["result"].clone()
}

fn text(result: &Value) -> &str {
    result["content"][0]["text"].as_str().unwrap_or_default()
}

fn ok(result: &Value) -> Value {
    assert_ne!(
        result["isError"],
        json!(true),
        "expected success: {}",
        text(result)
    );
    serde_json::from_str(text(result)).unwrap()
}

fn refused(result: &Value) -> String {
    assert_eq!(
        result["isError"],
        json!(true),
        "expected a refusal: {}",
        text(result)
    );
    text(result).to_string()
}

const STUB_MARKERS: [&str; 5] = [
    "Unknown tool",
    "not yet available",
    "not yet implemented",
    "not implemented",
    "not served by the hosted backend",
];

fn assert_not_stub(name: &str, result: &Value) {
    let t = text(result);
    for marker in STUB_MARKERS {
        assert!(
            !t.contains(marker),
            "advertised tool {name} answered like a stub: {t}"
        );
    }
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // serial test guard, single-threaded — no deadlock
async fn user_correction_outranks_a_newer_extraction_under_highest_priority() {
    let _guard = crate::projection_registry::test_guard();
    let e = Embedded::new().await;
    ok(&e
        .call(
            "prime_define_projection",
            json!({ "entity_type": "acct_owner_hp", "field_policies": { "owner": "highest_priority" } }),
        )
        .await);

    let created = ok(&e
        .call(
            "prime_add_node",
            json!({
                "type": "acct_owner_hp",
                "properties": { "owner": "Bob" },
                "source": "user",
                "actor": "alice@example.com",
            }),
        )
        .await);
    let node_id = created["entity_id"].as_str().unwrap().to_string();

    ok(&e
        .call(
            "prime_update_node",
            json!({
                "node_id": node_id,
                "properties": { "owner": "Robert" },
                "source": "extraction",
                "model": "extractor-1",
                "run_id": "run-1",
                "derived_from": ["crm-export.csv"],
                "confidence": 0.3,
            }),
        )
        .await);

    let p = ok(&e
        .call(
            "prime_node_provenance",
            json!({ "node_id": node_id, "field": "owner" }),
        )
        .await);
    assert_eq!(p["value"], "Bob");
    assert_eq!(p["merge_policy_applied"], "highest_priority");
    assert_eq!(p["policy_source"], "projection");
    assert_eq!(p["source_priority"], 1000);
    assert_eq!(p["assertion"]["source"], "user");
    assert_eq!(p["assertion"]["actor"], "alice@example.com");

    let snap = ok(&e
        .call("prime_project_node", json!({ "node_id": node_id }))
        .await);
    assert_eq!(snap["fields"]["owner"], "Bob");
}

#[tokio::test]
async fn extracted_field_provenance_returns_model_run_derivation_and_confidence() {
    let e = Embedded::new().await;
    let created = ok(&e
        .call(
            "prime_add_node",
            json!({
                "type": "unprojected_fact",
                "properties": { "headline": "Acme raises" },
                "source": "extraction",
                "model": "extractor-2",
                "run_id": "run-9",
                "derived_from": "https://news.example/acme",
                "confidence": 0.8,
            }),
        )
        .await);
    let node_id = created["entity_id"].as_str().unwrap();

    let p = ok(&e
        .call(
            "prime_node_provenance",
            json!({ "node_id": node_id, "field": "headline" }),
        )
        .await);
    assert_eq!(p["value"], "Acme raises");
    assert_eq!(p["policy_source"], "default");
    assert_eq!(p["merge_policy_applied"], "last_write");
    assert_eq!(p["assertion"]["source"], "extraction");
    assert_eq!(p["assertion"]["model"], "extractor-2");
    assert_eq!(p["assertion"]["run_id"], "run-9");
    assert_eq!(
        p["assertion"]["derived_from"],
        json!(["https://news.example/acme"])
    );
    assert_eq!(p["assertion"]["confidence"], 0.8);
    assert!(p["source_event_id"].as_str().is_some_and(|s| !s.is_empty()));
    assert_eq!(p["contributors"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn invalid_confidence_is_refused_before_any_event_is_appended() {
    let e = Embedded::new().await;
    let before = e.prime.stats().event_count;
    for bad in [1.5, -0.1] {
        let msg = refused(
            &e.call(
                "prime_add_node",
                json!({ "type": "c", "properties": { "name": "x" }, "confidence": bad }),
            )
            .await,
        );
        assert!(msg.contains("assertion.confidence"), "{msg}");
    }
    assert_eq!(e.prime.stats().event_count, before);
}

fn invoice_kind() -> Value {
    json!({
        "type_name": "invoice_t",
        "kind": "node",
        "schema": {
            "properties": {
                "amount": { "kind": "number", "min": 0 },
                "status": { "kind": "enum", "values": ["open", "paid"] }
            }
        }
    })
}

#[tokio::test]
async fn strict_mode_refuses_typed_violations_that_open_mode_accepts() {
    let e = Embedded::new().await;
    let kind = ok(&e.call("prime_define_type", invoice_kind()).await);
    assert_eq!(kind["version"], 1);

    let wrong_type = json!({ "type": "invoice_t", "properties": { "amount": "lots" } });
    let out_of_range = json!({ "type": "invoice_t", "properties": { "amount": -5 } });
    let bad_enum = json!({ "type": "invoice_t", "properties": { "status": "void" } });

    for write in [&wrong_type, &out_of_range, &bad_enum] {
        ok(&e.call("prime_add_node", write.clone()).await);
    }

    ok(&e
        .call("prime_set_enforcement", json!({ "mode": "strict" }))
        .await);
    let listed = ok(&e.call("prime_list_types", json!({})).await);
    assert_eq!(listed["enforcement"], "strict");
    assert_eq!(listed["types"][0]["type_name"], "invoice_t");

    let msg = refused(&e.call("prime_add_node", wrong_type).await);
    assert!(msg.contains("amount") && msg.contains("\"lots\""), "{msg}");
    let msg = refused(&e.call("prime_add_node", out_of_range).await);
    assert!(msg.contains("amount") && msg.contains("-5"), "{msg}");
    let msg = refused(&e.call("prime_add_node", bad_enum).await);
    assert!(msg.contains("status") && msg.contains("\"void\""), "{msg}");

    let msg = refused(
        &e.call(
            "prime_add_node",
            json!({ "type": "unregistered_t", "properties": {} }),
        )
        .await,
    );
    assert!(msg.contains("unregistered_t"), "{msg}");
}

#[tokio::test]
async fn breaking_kind_re_registration_is_refused_and_a_compatible_one_is_versioned() {
    let e = Embedded::new().await;
    ok(&e.call("prime_define_type", invoice_kind()).await);
    let mut compatible = invoice_kind();
    compatible["schema"]["properties"]["note"] = json!({ "kind": "string" });
    let v2 = ok(&e.call("prime_define_type", compatible).await);
    assert_eq!(v2["version"], 2);

    let mut breaking = invoice_kind();
    breaking["schema"]["properties"]["amount"] = json!({ "kind": "string" });
    let msg = refused(&e.call("prime_define_type", breaking).await);
    assert!(msg.contains("re-registration refused"), "{msg}");
}

#[tokio::test]
async fn embedded_and_hosted_refuse_the_same_invalid_write_identically() {
    let e = Embedded::new().await;
    ok(&e.call("prime_define_type", invoice_kind()).await);
    ok(&e
        .call("prime_set_enforcement", json!({ "mode": "strict" }))
        .await);

    let server = MockServer::start().await;
    crate::hosted_test_core::mount_stateful_core(&server).await;
    let hosted = HostedPrime::connect(server.uri(), None, 8, Duration::from_mins(1));
    let t = "tenant-parity";
    ok(&hosted_call(&hosted, t, "prime_define_type", invoice_kind()).await);
    ok(&hosted_call(
        &hosted,
        t,
        "prime_set_enforcement",
        json!({ "mode": "strict" }),
    )
    .await);

    let writes = [
        json!({ "type": "invoice_t", "properties": { "amount": "lots" } }),
        json!({ "type": "invoice_t", "properties": { "amount": -5 } }),
        json!({ "type": "invoice_t", "properties": { "status": "void" } }),
        json!({ "type": "invoice_t", "properties": {}, "confidence": 1.5 }),
    ];
    for write in writes {
        let embedded_msg = refused(&e.call("prime_add_node", write.clone()).await);
        let hosted_msg = refused(&hosted_call(&hosted, t, "prime_add_node", write).await);
        assert_eq!(embedded_msg, hosted_msg);
    }
}

#[tokio::test]
async fn hosted_provenance_reports_the_assertion_written_through_the_hosted_tool() {
    let server = MockServer::start().await;
    crate::hosted_test_core::mount_stateful_core(&server).await;
    let hosted = HostedPrime::connect(server.uri(), None, 8, Duration::from_mins(1));
    let t = "tenant-prov";

    let created = ok(&hosted_call(
        &hosted,
        t,
        "prime_add_node",
        json!({ "type": "contact", "properties": { "email": "a@x.io" }, "source": "user", "actor": "ops" }),
    )
    .await);
    let node_id = created["entity_id"].as_str().unwrap().to_string();
    ok(&hosted_call(
        &hosted,
        t,
        "prime_update_node",
        json!({ "node_id": node_id, "properties": { "email": "b@x.io" }, "source": "import" }),
    )
    .await);

    let p = ok(&hosted_call(
        &hosted,
        t,
        "prime_node_provenance",
        json!({ "node_id": node_id, "field": "email" }),
    )
    .await);
    assert_eq!(p["value"], "b@x.io");
    assert_eq!(p["assertion"]["source"], "import");
    assert_eq!(p["contributors"].as_array().unwrap().len(), 1);

    ok(&hosted_call(
        &hosted,
        t,
        "prime_define_projection",
        json!({ "entity_type": "contact", "field_policies": { "email": "highest_priority" } }),
    )
    .await);
    let p = ok(&hosted_call(
        &hosted,
        t,
        "prime_node_provenance",
        json!({ "node_id": node_id, "field": "email" }),
    )
    .await);
    assert_eq!(p["value"], "a@x.io");
    assert_eq!(p["assertion"]["source"], "user");
    assert_eq!(p["assertion"]["actor"], "ops");
}

#[tokio::test]
async fn hosted_tool_call_with_a_conflicting_tenant_argument_is_refused() {
    let server = MockServer::start().await;
    crate::hosted_test_core::mount_stateful_core(&server).await;
    let hosted = HostedPrime::connect(server.uri(), None, 8, Duration::from_mins(1));
    let msg = refused(
        &hosted_call(
            &hosted,
            "tenant-a",
            "prime_recall",
            json!({ "vector": [1.0, 0.0], "tenant_id": "tenant-b" }),
        )
        .await,
    );
    assert!(
        msg.contains("conflicts with the authenticated tenant"),
        "{msg}"
    );
}

#[tokio::test]
async fn every_advertised_embedded_tool_is_served() {
    let e = Embedded::new().await;
    let Value::Array(tools) = tool_definitions() else {
        panic!("tool_definitions is not an array");
    };
    assert!(!tools.is_empty());
    for tool in &tools {
        let name = tool["name"].as_str().unwrap();
        let result = e.call(name, json!({})).await;
        assert_not_stub(name, &result);
    }
}

#[tokio::test]
async fn every_advertised_hosted_tool_is_served() {
    let server = MockServer::start().await;
    crate::hosted_test_core::mount_stateful_core(&server).await;
    let hosted = HostedPrime::connect(server.uri(), None, 8, Duration::from_mins(1));

    let req: protocol::Request =
        serde_json::from_value(json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }))
            .unwrap();
    let resp = hosted_dispatch::handle_request_hosted(&hosted, "tenant-list", None, &req)
        .await
        .unwrap();
    let listed = serde_json::to_value(&resp).unwrap()["result"]["tools"].clone();
    let names: Vec<String> = listed
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(names.len(), hosted_dispatch::HOSTED_TOOLS.len());
    for absent in [
        "hound_ingest",
        "hound_ingest_docs",
        "prime_index",
        "inbox_draft",
    ] {
        assert!(!names.iter().any(|n| n == absent), "{absent} advertised");
    }
    for name in &names {
        let result = hosted_call(&hosted, "tenant-list", name, json!({})).await;
        assert_not_stub(name, &result);
    }
}

#[tokio::test]
async fn update_of_a_missing_node_is_refused() {
    let e = Embedded::new().await;
    refused(
        &e.call(
            "prime_update_node",
            json!({ "node_id": "node:contact:nope", "properties": { "a": 1 } }),
        )
        .await,
    );
}
