use allsource_core::embedded::{Config, EmbeddedCore, IngestEvent};
use serde_json::{Value, json};

use super::exec_trace;
use crate::{
    diagnostics::{AccessProfile, DiagnosticPolicy},
    stores::StoreRegistry,
};

async fn ingest(core: &EmbeddedCore, entity_id: &str, event_type: &str, payload: Value) {
    core.ingest(IngestEvent {
        entity_id,
        event_type,
        payload,
        metadata: None,
        tenant_id: None,
    })
    .await
    .expect("ingest");
}

async fn empty_core() -> EmbeddedCore {
    EmbeddedCore::open(
        Config::builder()
            .single_tenant(true)
            .build()
            .expect("valid config"),
    )
    .await
    .expect("in-memory core")
}

fn local() -> DiagnosticPolicy {
    DiagnosticPolicy::new(AccessProfile::Local, None, "local").expect("local policy")
}

/// A step in the workspace store names its run, whose events live in the
/// profile (`default`) store.
async fn run_and_step() -> StoreRegistry {
    let profile = empty_core().await;
    let workspace = empty_core().await;
    ingest(
        &profile,
        "run-42",
        "workflow_run.started",
        json!({ "skill_id": "skill-7" }),
    )
    .await;
    ingest(&profile, "run-99", "workflow_run.started", json!({})).await;
    ingest(
        &workspace,
        "step-1",
        "step_run.started",
        json!({ "run_id": "run-42" }),
    )
    .await;
    StoreRegistry::from_cores(vec![("default", profile), ("workspace", workspace)])
}

fn entity_ids(result: &Value) -> Vec<&str> {
    result["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|item| item["entity_id"].as_str().expect("entity_id"))
        .collect()
}

#[tokio::test]
async fn depth_zero_returns_only_events_that_reference_the_id() {
    let stores = run_and_step().await;

    let result = exec_trace(&stores, &local(), &json!({ "id": "step-1", "depth": 0 }))
        .await
        .expect("trace");

    assert_eq!(entity_ids(&result), vec!["step-1"]);
    assert_eq!(result["items"][0]["store"], "workspace");
    assert_eq!(result["items"][0]["hop"], 0);
    assert_eq!(result["items"][0]["matched_by"]["via"], "entity_id");
    assert_eq!(result["completeness"]["complete"], true);
}

#[tokio::test]
async fn depth_one_follows_a_carried_run_id_into_another_store() {
    let stores = run_and_step().await;

    let result = exec_trace(&stores, &local(), &json!({ "id": "step-1", "depth": 1 }))
        .await
        .expect("trace");

    let ids = entity_ids(&result);
    assert!(ids.contains(&"run-42"), "the run is one hop away: {ids:?}");
    assert!(!ids.contains(&"run-99"), "an unrelated run never joins");
    let run = result["items"]
        .as_array()
        .expect("items")
        .iter()
        .find(|item| item["entity_id"] == "run-42")
        .expect("run event");
    assert_eq!(run["store"], "default");
    assert_eq!(run["hop"], 1);
    assert_eq!(run["matched_by"]["id"], "run-42");
    assert!(
        result["graph"]["edges"]
            .as_array()
            .expect("edges")
            .iter()
            .any(|edge| edge["from_id"] == "step-1" && edge["to_id"] == "run-42")
    );
    assert_eq!(result["completeness"]["sourcesRead"], 2);
}

#[tokio::test]
async fn a_redacted_value_is_neither_matched_nor_followed() {
    let core = empty_core().await;
    ingest(
        &core,
        "login-1",
        "auth.login",
        json!({ "token_id": "tok-secret-1" }),
    )
    .await;
    ingest(&core, "tok-secret-1", "auth.token_issued", json!({})).await;
    let stores = StoreRegistry::from_cores(vec![("default", core)]);
    let args = |id: &str| json!({ "id": id, "depth": 2, "payload_mode": "redacted" });

    let from_login = exec_trace(&stores, &local(), &args("login-1"))
        .await
        .expect("trace");
    assert_eq!(
        entity_ids(&from_login),
        vec!["login-1"],
        "a redacted id is not followed"
    );

    let by_secret = exec_trace(&stores, &local(), &args("tok-secret-1"))
        .await
        .expect("trace");
    assert_eq!(
        entity_ids(&by_secret),
        vec!["tok-secret-1"],
        "the payload holding the secret must not match it"
    );
}

#[tokio::test]
async fn a_hosted_tenant_cannot_name_a_second_store() {
    let stores = run_and_step().await;
    let hosted = DiagnosticPolicy::new(
        AccessProfile::HostedTenant,
        Some("tenant-a".to_string()),
        "prod",
    )
    .expect("hosted policy");

    let error = exec_trace(
        &stores,
        &hosted,
        &json!({ "id": "step-1", "stores": ["default", "workspace"] }),
    )
    .await
    .expect_err("a hosted tenant reads only its bound store");
    assert!(error.to_string().starts_with("access denied:"));

    let own = exec_trace(&stores, &hosted, &json!({ "id": "step-1" }))
        .await
        .expect("its own store stays readable");
    assert_eq!(own["completeness"]["sourcesRequested"], 1);
    assert!(own["completeness"]["scanned"].get("workspace").is_none());
}

#[tokio::test]
async fn a_hosted_trace_returns_only_its_own_tenants_events() {
    let core = EmbeddedCore::open(
        Config::builder()
            .single_tenant(false)
            .build()
            .expect("valid config"),
    )
    .await
    .expect("in-memory core");
    for (tenant, entity_id) in [("tenant-a", "step-a"), ("tenant-b", "step-b")] {
        core.ingest(IngestEvent {
            entity_id,
            event_type: "step_run.started",
            payload: json!({ "run_id": "run-shared" }),
            metadata: None,
            tenant_id: Some(tenant),
        })
        .await
        .expect("ingest");
    }
    let stores = StoreRegistry::from_cores(vec![("default", core)]);
    let hosted = DiagnosticPolicy::new(
        AccessProfile::HostedTenant,
        Some("tenant-a".to_string()),
        "prod",
    )
    .expect("hosted policy");

    let result = exec_trace(&stores, &hosted, &json!({ "id": "run-shared", "depth": 0 }))
        .await
        .expect("trace");

    assert_eq!(entity_ids(&result), vec!["step-a"]);
    assert_eq!(result["completeness"]["scanned"]["default"], 1);
}

#[tokio::test]
async fn an_id_that_is_a_prefix_of_another_does_not_match_it() {
    let core = empty_core().await;
    ingest(
        &core,
        "step-1",
        "step_run.started",
        json!({ "run_id": "run-420" }),
    )
    .await;
    ingest(
        &core,
        "step-2",
        "step_run.started",
        json!({ "run_id": "run-42" }),
    )
    .await;
    let stores = StoreRegistry::from_cores(vec![("default", core)]);

    let result = exec_trace(&stores, &local(), &json!({ "id": "run-42", "depth": 0 }))
        .await
        .expect("trace");

    assert_eq!(entity_ids(&result), vec!["step-2"]);
}

#[tokio::test]
async fn a_key_name_never_matches_under_the_keys_view() {
    let core = empty_core().await;
    ingest(
        &core,
        "step-1",
        "step_run.started",
        json!({ "run_id": "run-1" }),
    )
    .await;
    let stores = StoreRegistry::from_cores(vec![("default", core)]);

    let result = exec_trace(
        &stores,
        &local(),
        &json!({ "id": "run_id", "depth": 0, "payload_mode": "keys" }),
    )
    .await
    .expect("trace");

    assert!(entity_ids(&result).is_empty());
}

#[tokio::test]
async fn max_scan_is_reported_when_a_store_was_not_read_to_the_end() {
    let core = empty_core().await;
    for i in 0..5 {
        ingest(
            &core,
            &format!("item-{i}"),
            "thing.happened",
            json!({ "batch_id": "batch-1" }),
        )
        .await;
    }
    let stores = StoreRegistry::from_cores(vec![("default", core)]);

    let result = exec_trace(
        &stores,
        &local(),
        &json!({ "id": "batch-1", "depth": 0, "max_scan": 2 }),
    )
    .await
    .expect("trace");

    assert_eq!(result["completeness"]["complete"], false);
    assert_eq!(result["completeness"]["reason"], "max_scan_reached");
    assert_eq!(result["completeness"]["scanned"]["default"], 2);
    assert_eq!(result["items"].as_array().expect("items").len(), 2);
}

#[tokio::test]
async fn items_are_ordered_by_time_across_stores() {
    let first = empty_core().await;
    let second = empty_core().await;
    for i in 0..3 {
        let core = if i % 2 == 0 { &first } else { &second };
        ingest(
            core,
            &format!("e-{i}"),
            "thing.happened",
            json!({ "order_id": "order-1" }),
        )
        .await;
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }
    let stores = StoreRegistry::from_cores(vec![("default", first), ("other", second)]);

    let result = exec_trace(&stores, &local(), &json!({ "id": "order-1", "depth": 0 }))
        .await
        .expect("trace");

    assert_eq!(entity_ids(&result), vec!["e-0", "e-1", "e-2"]);
}

#[tokio::test]
async fn a_short_id_or_an_out_of_range_depth_is_refused() {
    let stores = run_and_step().await;
    for args in [json!({ "id": "ab" }), json!({ "id": "step-1", "depth": 4 })] {
        let error = exec_trace(&stores, &local(), &args)
            .await
            .expect_err("invalid trace arguments");
        assert!(error.to_string().starts_with("invalid argument:"));
    }
}

/// A step whose evidence is a JSON string holding tool results, each result's
/// content JSON again, naming a runner `run_ref` recorded in a second store.
async fn evidence_and_runner() -> StoreRegistry {
    let workspace = empty_core().await;
    let prod = empty_core().await;
    let content = json!({ "run_ref": "vr-x-es", "status": "unavailable" }).to_string();
    let evidence = json!([{ "tool": "run_browser_recipe", "content": content }]).to_string();
    ingest(
        &workspace,
        "step-1",
        "hierarchy.step_run.completed",
        json!({ "workflow_run_id": "run-1", "evidence": evidence }),
    )
    .await;
    ingest(
        &prod,
        "rec-1",
        "browser_recipe.run_recorded",
        json!({ "run_ref": "vr-x-es", "route": "hosted" }),
    )
    .await;
    StoreRegistry::from_cores(vec![("workspace", workspace), ("prod", prod)])
}

#[tokio::test]
async fn an_id_inside_a_json_string_is_followed_into_another_store() {
    let stores = evidence_and_runner().await;

    let result = exec_trace(&stores, &local(), &json!({ "id": "step-1", "depth": 1 }))
        .await
        .expect("trace");

    let ids = entity_ids(&result);
    assert!(
        ids.contains(&"rec-1"),
        "run_ref inside the evidence joins: {ids:?}"
    );
    let runner = result["items"]
        .as_array()
        .expect("items")
        .iter()
        .find(|item| item["entity_id"] == "rec-1")
        .expect("runner event");
    assert_eq!(runner["store"], "prod");
    assert_eq!(runner["matched_by"]["id"], "vr-x-es");
}

#[tokio::test]
async fn an_id_inside_a_json_string_matches_at_depth_zero() {
    let stores = evidence_and_runner().await;

    let result = exec_trace(&stores, &local(), &json!({ "id": "vr-x-es", "depth": 0 }))
        .await
        .expect("trace");

    assert_eq!(entity_ids(&result), vec!["step-1", "rec-1"]);
}

#[tokio::test]
async fn a_credential_inside_a_json_string_is_neither_matched_nor_followed() {
    let core = empty_core().await;
    ingest(
        &core,
        "login-1",
        "auth.login",
        json!({ "evidence": json!({ "token_id": "tok-secret-1" }).to_string() }),
    )
    .await;
    ingest(&core, "tok-secret-1", "auth.token_issued", json!({})).await;
    let stores = StoreRegistry::from_cores(vec![("default", core)]);
    let args = |id: &str| json!({ "id": id, "depth": 2, "payload_mode": "redacted" });

    let from_login = exec_trace(&stores, &local(), &args("login-1"))
        .await
        .expect("trace");
    assert_eq!(entity_ids(&from_login), vec!["login-1"]);

    let by_secret = exec_trace(&stores, &local(), &args("tok-secret-1"))
        .await
        .expect("trace");
    assert_eq!(entity_ids(&by_secret), vec!["tok-secret-1"]);
}

async fn five_runs_of_one_workflow() -> StoreRegistry {
    let core = empty_core().await;
    for run in 0..5 {
        ingest(
            &core,
            &format!("run-{run}"),
            "workflow_run.started",
            json!({ "workflow_id": "wf-1" }),
        )
        .await;
    }
    StoreRegistry::from_cores(vec![("default", core)])
}

#[tokio::test]
async fn an_id_shared_past_the_hub_threshold_is_reported_not_followed() {
    let stores = five_runs_of_one_workflow().await;

    let result = exec_trace(
        &stores,
        &local(),
        &json!({ "id": "run-0", "depth": 1, "hub_threshold": 3 }),
    )
    .await
    .expect("trace");

    assert_eq!(entity_ids(&result), vec!["run-0"]);
    assert_eq!(
        result["graph"]["hubs"],
        json!([{ "id": "wf-1", "hop": 1, "entities": 5 }])
    );
    assert!(
        result["graph"]["edges"]
            .as_array()
            .expect("edges")
            .is_empty()
    );
    assert_eq!(result["completeness"]["complete"], true);
}

#[tokio::test]
async fn a_run_with_many_events_is_not_a_hub() {
    let core = empty_core().await;
    ingest(
        &core,
        "step-1",
        "step_run.started",
        json!({ "run_id": "run-1" }),
    )
    .await;
    for _ in 0..10 {
        ingest(&core, "run-1", "workflow_run.progressed", json!({})).await;
    }
    let stores = StoreRegistry::from_cores(vec![("default", core)]);

    let result = exec_trace(
        &stores,
        &local(),
        &json!({ "id": "step-1", "depth": 1, "hub_threshold": 3 }),
    )
    .await
    .expect("trace");

    assert_eq!(entity_ids(&result).len(), 11);
    assert!(result["graph"]["hubs"].as_array().expect("hubs").is_empty());
}

#[tokio::test]
async fn an_id_under_the_hub_threshold_is_followed() {
    let stores = five_runs_of_one_workflow().await;

    let result = exec_trace(&stores, &local(), &json!({ "id": "run-0", "depth": 1 }))
        .await
        .expect("trace");

    assert_eq!(entity_ids(&result).len(), 5);
    assert!(result["graph"]["hubs"].as_array().expect("hubs").is_empty());
}
