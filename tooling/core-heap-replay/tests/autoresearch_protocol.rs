//! The manifest and the evaluators agree. Every hard gate and metric an
//! evaluator reports is declared in `autoresearch.toml`, and nothing else is.

use core_heap_replay::{
    envelope::{Envelope, Status},
    gates,
    protocol::{self, HEAP_REPLAY, Spec},
    scan::repo_root,
};
use serde_json::Value;
use std::collections::BTreeSet;

fn manifest() -> toml::Table {
    let text = std::fs::read_to_string(repo_root().join("autoresearch.toml"))
        .expect("autoresearch.toml at the repository root");
    toml::from_str(&text).expect("autoresearch.toml parses")
}

fn evaluator<'a>(manifest: &'a toml::Table, id: &str) -> &'a toml::Table {
    manifest["evaluators"]
        .as_array()
        .expect("evaluators array")
        .iter()
        .filter_map(|e| e.as_table())
        .find(|e| e["id"].as_str() == Some(id))
        .unwrap_or_else(|| panic!("manifest declares no evaluator {id}"))
}

fn strings(value: &toml::Value) -> BTreeSet<String> {
    value
        .as_array()
        .expect("array of strings")
        .iter()
        .map(|v| v.as_str().expect("string").to_string())
        .collect()
}

fn request() -> protocol::Request {
    protocol::read_request(
        br#"{"protocol_version":1,"evaluator_id":"heap_replay","run_id":"run-1","baseline_commit":"b","evaluated_commit":"c","candidate_worktree":"/w","changed_paths":[],"declared_environment":{},"artifact_directory":"/a","cancellation_id":"x"}
"#
        .as_slice(),
    )
    .expect("request parses")
}

fn measurements(response: &Value) -> &Vec<Value> {
    response["result"]["output"]["measurements"]
        .as_array()
        .expect("measurements")
}

fn find<'a>(measurements: &'a [Value], name: &str) -> &'a Value {
    measurements
        .iter()
        .find(|m| m["name"] == name)
        .unwrap_or_else(|| panic!("no measurement {name}"))
}

#[test]
fn manifest_declares_exactly_the_heap_replay_measurements() {
    let manifest = manifest();
    let heap = evaluator(&manifest, HEAP_REPLAY.evaluator);
    let gates: BTreeSet<String> = HEAP_REPLAY.gates.iter().map(|g| g.to_string()).collect();
    assert_eq!(strings(&heap["hard_gates"]), gates);
    let declared: BTreeSet<(String, String, String)> = heap["metrics"]
        .as_array()
        .expect("metrics")
        .iter()
        .map(|m| {
            (
                m["name"].as_str().unwrap().to_string(),
                m["kind"].as_str().unwrap().to_string(),
                m["direction"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    let spec: BTreeSet<(String, String, String)> = HEAP_REPLAY
        .metrics
        .iter()
        .map(|m| (m.name.into(), m.kind.into(), m.direction.into()))
        .collect();
    assert_eq!(declared, spec);
}

fn assert_declares(spec: &Spec) {
    let manifest = manifest();
    let declared = evaluator(&manifest, spec.evaluator);
    let gates: BTreeSet<String> = spec.gates.iter().map(|g| g.to_string()).collect();
    assert_eq!(
        strings(&declared["hard_gates"]),
        gates,
        "{}",
        spec.evaluator
    );
}

#[test]
fn manifest_declares_exactly_the_core_tests_gates() {
    assert_declares(&protocol::CORE_TESTS);
}

#[test]
fn manifest_objective_is_peak_live_bytes_and_budget_is_nonzero() {
    let manifest = manifest();
    let objective = manifest["experiment"]["objective"]
        .as_table()
        .expect("objective");
    assert_eq!(objective["name"].as_str(), Some("peak_live_bytes"));
    assert_eq!(objective["direction"].as_str(), Some("minimize"));
    let budget = manifest["experiment"]["budget"].as_table().expect("budget");
    for key in ["max_candidates", "max_failures", "wall_clock_seconds"] {
        assert!(budget[key].as_integer().is_some_and(|v| v > 0), "{key}");
    }
}

#[test]
fn manifest_protects_the_paths_the_harness_names() {
    let manifest = manifest();
    let protected = strings(&manifest["scope"]["protected_paths"]);
    let named: BTreeSet<String> = gates::PROTECTED_PATHS
        .iter()
        .map(|p| p.to_string())
        .collect();
    assert_eq!(protected, named);
}

#[test]
fn protected_paths_exist_in_this_tree() {
    assert!(gates::protected_paths_exist(&repo_root()));
}

#[test]
fn failed_run_reports_every_declared_measurement() {
    let mut envelope = Envelope::new(HEAP_REPLAY.evaluator);
    envelope.status = Status::Panic;
    let response = protocol::response(&request(), &envelope, &HEAP_REPLAY);
    let measured = measurements(&response);
    for gate in HEAP_REPLAY.gates {
        let m = find(measured, gate);
        assert_eq!(m["kind"], "hard_gate", "{gate}");
        assert_eq!(m["outcome"]["passed"], false, "{gate}");
    }
    for metric in HEAP_REPLAY.metrics {
        let m = find(measured, metric.name);
        assert_eq!(m["kind"], "numeric", "{}", metric.name);
        assert_eq!(m["value"], 0.0, "{}", metric.name);
    }
    assert_eq!(
        measured.len(),
        HEAP_REPLAY.gates.len() + HEAP_REPLAY.metrics.len()
    );
}

#[test]
fn passing_gates_and_metrics_carry_their_values() {
    let mut envelope = Envelope::new(HEAP_REPLAY.evaluator);
    for gate in HEAP_REPLAY.gates {
        envelope.gate(gate, true);
    }
    envelope.metric("peak_live_bytes", 1234_u64);
    let response = protocol::response(&request(), &envelope, &HEAP_REPLAY);
    let measured = measurements(&response);
    assert_eq!(find(measured, "build_ok")["outcome"]["passed"], true);
    let peak = find(measured, "peak_live_bytes");
    assert_eq!(peak["metric_kind"], "objective");
    assert_eq!(peak["direction"], "minimize");
    assert_eq!(peak["value"], 1234.0);
}

#[test]
fn response_echoes_the_request_identity_in_a_success_envelope() {
    let envelope = Envelope::new(HEAP_REPLAY.evaluator);
    let response = protocol::response(&request(), &envelope, &HEAP_REPLAY);
    assert_eq!(response["protocol_version"], 1);
    assert_eq!(response["result"]["status"], "success");
    let output = &response["result"]["output"];
    assert_eq!(output["evaluator_id"], "heap_replay");
    assert_eq!(output["run_id"], "run-1");
    assert_eq!(output["baseline_commit"], "b");
    assert_eq!(output["evaluated_commit"], "c");
}

#[test]
fn unreadable_request_is_a_reported_failure() {
    assert!(protocol::read_request("not json\n".as_bytes()).is_err());
    assert!(protocol::read_request("".as_bytes()).is_err());
    let failure = protocol::failure("request_unreadable");
    assert_eq!(failure["result"]["status"], "failure");
    assert_eq!(failure["result"]["failure"]["class"], "reported");
}

#[test]
fn literal_env_pairs_split_on_the_first_equals() {
    assert_eq!(
        protocol::split_env("PATH=/usr/bin:/bin"),
        Some(("PATH".to_string(), "/usr/bin:/bin".to_string()))
    );
    assert_eq!(
        protocol::split_env("A=b=c"),
        Some(("A".to_string(), "b=c".to_string()))
    );
    assert_eq!(protocol::split_env("NOEQUALS"), None);
    assert_eq!(protocol::split_env("=value"), None);
}

#[test]
fn rows_decoded_must_be_at_or_under_the_recorded_ceiling() {
    assert!(gates::rows_decoded_ok(Some(10), Some(10)));
    assert!(!gates::rows_decoded_ok(Some(11), Some(10)));
    assert!(!gates::rows_decoded_ok(None, Some(10)));
    assert!(!gates::rows_decoded_ok(Some(1), None));
    assert_eq!(
        gates::rows_decoded_max(r#"{"rows_decoded_max": 10}"#),
        Some(10)
    );
    assert_eq!(gates::rows_decoded_max(r#"{"other": 10}"#), None);
    assert_eq!(gates::rows_decoded_max("not json"), None);
}

#[test]
fn a_pin_needs_both_the_measured_and_the_literal_value() {
    assert!(gates::pinned(Some("abc".to_string()), Some("abc")));
    assert!(!gates::pinned(Some("abc".to_string()), Some("abd")));
    assert!(!gates::pinned(None, Some("abc")));
    assert!(!gates::pinned(Some("abc".to_string()), None));
}

#[test]
fn self_sha_of_this_test_binary_is_a_sha256() {
    let sha = protocol::self_sha256().expect("current executable readable");
    assert_eq!(sha.len(), 64);
    assert!(sha.chars().all(|c| c.is_ascii_hexdigit()));
}
