use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    path::Path,
    process::{Child, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

use serde_json::{Value, json};

#[path = "../src/toon.rs"]
mod encoder;

const OPERATION_TIMEOUT: Duration = Duration::from_secs(90);
const CHILD_TIMEOUT: Duration = Duration::from_secs(20);
const NODE_TYPE: &str = "toon_key_case";

fn corpus() -> Vec<Value> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/toon-key-corpus.json"))
        .expect("frozen reference corpus JSON");
    assert_eq!(fixture["reference"], "@toon-format/toon@4.4.0");
    let cases = fixture["cases"].as_array().expect("corpus cases").clone();
    assert_eq!(cases.len(), 132, "all frozen cases must execute");
    let names: BTreeSet<_> = cases.iter().map(case_name).collect();
    assert_eq!(names.len(), cases.len(), "unique corpus case names");
    for shape in ["object", "nested", "scalar_array", "tabular"] {
        let suffix = format!("_{shape}");
        assert_eq!(
            names.iter().filter(|name| name.ends_with(&suffix)).count(),
            33,
            "frozen key coverage for {shape}"
        );
    }
    cases
}

fn case_name(case: &Value) -> &str {
    case["name"].as_str().expect("corpus case name")
}

fn expected_toon(case: &Value) -> &str {
    case["expected_toon"].as_str().expect("reference TOON")
}

#[test]
fn actual_encoder_matches_all_frozen_reference_cases() {
    let cases = corpus();
    let mut failures = Vec::new();
    for case in &cases {
        let actual = encoder::encode(case.get("input").expect("corpus input"));
        let expected = expected_toon(case);
        if actual != expected {
            failures.push(format!(
                "{}: expected {expected:?}, actual {actual:?}",
                case_name(case)
            ));
        }
    }
    eprintln!(
        "frozen encoder corpus: {} matched, {} failed, {} total",
        cases.len() - failures.len(),
        failures.len(),
        cases.len()
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// The operation owner retains the child handle until reaped, including on unwind.
struct OwnedChild {
    child: Child,
    reaped: bool,
}

impl OwnedChild {
    fn finish(&mut self, deadline: Instant) -> (ExitStatus, bool) {
        loop {
            if let Some(status) = self.child.try_wait().expect("poll Prime child") {
                self.reaped = true;
                return (status, false);
            }
            if Instant::now() >= deadline {
                let _ = self.child.kill();
                let status = self.child.wait().expect("reap timed-out Prime child");
                self.reaped = true;
                return (status, true);
            }
            thread::sleep(
                Duration::from_millis(10).min(deadline.saturating_duration_since(Instant::now())),
            );
        }
    }
}

impl Drop for OwnedChild {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn tool_text(response: &Value) -> &str {
    let result = &response["result"];
    assert!(
        matches!(result.get("isError"), None | Some(Value::Bool(false))),
        "tool failed: {response}"
    );
    let content = result["content"].as_array().expect("tool content array");
    assert_eq!(content.len(), 1, "{response}");
    assert_eq!(content[0]["type"], "text", "{response}");
    content[0]["text"].as_str().expect("tool text payload")
}

fn tool_json(response: &Value) -> Value {
    serde_json::from_str(tool_text(response)).expect("JSON tool payload")
}

fn run_session(
    root: &Path,
    label: &str,
    format: &str,
    calls: &[(u64, &str, Value)],
    operation_deadline: Instant,
) -> Vec<Value> {
    assert!(
        Instant::now() < operation_deadline,
        "operation deadline before {label}"
    );
    let input = root.join(format!("{label}.stdin"));
    let output = root.join(format!("{label}.stdout"));
    let errors = root.join(format!("{label}.stderr"));
    let mut requests = vec![
        json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05", "capabilities": {},
                "clientInfo": {"name": "toon-keys-test", "version": "1"}
            }
        }),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
    ];
    requests.extend(calls.iter().map(|(id, name, arguments)| {
        json!({
            "jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": {"name": name, "arguments": arguments}
        })
    }));
    let mut wire = requests
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    wire.push('\n');
    fs::write(&input, wire).expect("write file-backed MCP input");

    let mut command = Command::new(env!("CARGO_BIN_EXE_allsource-prime"));
    command
        .env_clear()
        .env("HOME", root)
        .env("TMPDIR", root)
        .env("PRIME_EMBED_MODEL_DIR", root.join("no-model"))
        .current_dir(root)
        .arg("--data-dir")
        .arg(root.join("store"))
        .args(["--mode", "mcp", "--format", format])
        .stdin(Stdio::from(File::open(input).expect("open MCP input")))
        .stdout(Stdio::from(File::create(&output).expect("create stdout")))
        .stderr(Stdio::from(File::create(&errors).expect("create stderr")));
    let deadline = operation_deadline.min(Instant::now() + CHILD_TIMEOUT);
    let mut child = OwnedChild {
        child: command.spawn().expect("spawn actual Prime binary"),
        reaped: false,
    };
    let (status, timed_out) = child.finish(deadline);
    let stdout = fs::read_to_string(output).expect("read child stdout");
    let stderr = fs::read_to_string(errors).expect("read child stderr");
    let diagnostic = format!("{label}: status={status}, stdout={stdout:?}, stderr={stderr:?}");
    assert!(!timed_out, "child deadline exceeded: {diagnostic}");
    assert_eq!(status.code(), Some(0), "{diagnostic}");
    assert!(
        Instant::now() < operation_deadline,
        "operation deadline: {diagnostic}"
    );
    assert!(root.join("store").is_dir(), "store missing: {diagnostic}");
    assert!(!stderr.contains("panicked"), "{diagnostic}");
    let responses: Vec<Value> = stdout
        .lines()
        .map(|line| serde_json::from_str(line).expect("JSON-RPC stdout line"))
        .collect();
    assert_eq!(responses.len(), calls.len() + 1, "{diagnostic}");
    for (response, id) in responses
        .iter()
        .zip(std::iter::once(1).chain(calls.iter().map(|c| c.0)))
    {
        assert_eq!(response["jsonrpc"], "2.0", "{diagnostic}");
        assert_eq!(response["id"], id, "{diagnostic}");
        assert!(response.get("error").is_none(), "{diagnostic}");
        assert!(response["result"].is_object(), "{diagnostic}");
    }
    let initialized = &responses[0]["result"];
    assert_eq!(initialized["protocolVersion"], "2024-11-05");
    assert_eq!(initialized["serverInfo"]["name"], "allsource-prime");
    assert_eq!(
        initialized["serverInfo"]["version"],
        env!("CARGO_PKG_VERSION")
    );
    assert!(initialized["capabilities"]["tools"].is_object());
    for response in &responses[1..] {
        tool_text(response);
    }
    responses
}

fn assert_stats(stats: &Value, nodes: u64) {
    assert_eq!(stats["total_nodes"], nodes, "{stats}");
    assert_eq!(stats["total_edges"], 0, "{stats}");
    assert_eq!(stats["deleted_nodes"], 0, "{stats}");
    assert_eq!(stats["deleted_edges"], 0, "{stats}");
    assert_eq!(stats["edges_by_relation"], json!({}), "{stats}");
    let by_type = if nodes == 0 {
        json!({})
    } else {
        json!({(NODE_TYPE): nodes})
    };
    assert_eq!(stats["nodes_by_type"], by_type, "{stats}");
    assert_eq!(stats["sync"]["enabled"], false, "{stats}");
    assert_eq!(stats["sync"]["remote_url"], Value::Null, "{stats}");
}

#[test]
fn stdio_reopen_preserves_json_properties_and_reference_toon_keys() {
    let operation_deadline = Instant::now() + OPERATION_TIMEOUT;
    let directory = tempfile::tempdir().expect("isolated Prime directory");
    let root = directory.path();
    let cases = corpus();
    let object_cases: Vec<_> = cases
        .iter()
        .filter(|case| case_name(case).ends_with("_object"))
        .collect();
    assert_eq!(object_cases.len(), 33);
    let mut properties = serde_json::Map::new();
    let mut fields = BTreeMap::new();
    for case in &object_cases {
        let input = case["input"]
            .as_object()
            .expect("object-shaped corpus case");
        assert_eq!(input.len(), 1, "{}", case_name(case));
        let (key, value) = input.iter().next().expect("single corpus property");
        assert!(properties.insert(key.clone(), value.clone()).is_none());
        fields.insert(key.clone(), expected_toon(case).to_string());
    }
    for (key, value) in [
        ("name", "toon-key-corpus"),
        ("domain", "toon-key-integrity"),
    ] {
        assert!(properties.insert(key.into(), json!(value)).is_none());
        fields.insert(key.into(), format!("{key}: {value}"));
    }
    assert_eq!(properties.len(), 35);
    let original = Value::Object(properties);
    let created = run_session(
        root,
        "create",
        "json",
        &[
            (2, "prime_stats", json!({})),
            (
                3,
                "prime_add_node",
                json!({"type": NODE_TYPE, "properties": original}),
            ),
            (4, "prime_stats", json!({})),
        ],
        operation_deadline,
    );
    let fresh = tool_json(&created[1]);
    assert_stats(&fresh, 0);
    assert_eq!(fresh["event_count"], 0, "{fresh}");
    let added = tool_json(&created[2]);
    let node_id = added["node_id"].as_str().expect("created node ID");
    assert!(!node_id.is_empty(), "{added}");
    let entity_id = format!("node:{NODE_TYPE}:{node_id}");
    assert_eq!(added["entity_id"], entity_id, "{added}");
    let written = tool_json(&created[3]);
    assert_stats(&written, 1);
    assert!(
        written["event_count"]
            .as_u64()
            .expect("written event count")
            > 0
    );

    let search = json!({"type": NODE_TYPE, "fields": "full", "limit": 1, "offset": 0});
    let reopened = run_session(
        root,
        "reopen-json",
        "json",
        &[
            (10, "prime_stats", json!({})),
            (11, "prime_search", search.clone()),
        ],
        operation_deadline,
    );
    let retained = tool_json(&reopened[1]);
    assert_stats(&retained, 1);
    assert_eq!(retained["event_count"], written["event_count"]);
    let json_result = tool_json(&reopened[2]);
    assert_eq!(json_result["nodes"][0]["properties"], original);
    assert_eq!(
        json_result,
        json!({
            "nodes": [{"id": entity_id, "type": NODE_TYPE, "properties": original}],
            "offset": 0, "returned": 1, "total": 1
        })
    );

    let toon = run_session(
        root,
        "reopen-toon",
        "toon",
        &[(20, "prime_search", search)],
        operation_deadline,
    );
    let payload = tool_text(&toon[1]);
    let mut failures = Vec::new();
    for case in &object_cases {
        let field = format!("      {}", expected_toon(case));
        if payload.lines().filter(|line| *line == field).count() != 1 {
            failures.push(format!("{}: expected one {field:?}", case_name(case)));
        }
    }
    eprintln!(
        "stdio TOON object keys: {} matched, {} failed, 33 total",
        object_cases.len() - failures.len(),
        failures.len()
    );
    assert!(
        failures.is_empty(),
        "{}\nactual: {payload:?}",
        failures.join("\n")
    );
    let mut property_lines = String::new();
    for line in fields.values() {
        property_lines.push_str("      ");
        property_lines.push_str(line);
        property_lines.push('\n');
    }
    let expected = format!(
        "format: toon\nnodes[1]:\n  - id: {entity_id}\n    properties:\n{property_lines}    type: {NODE_TYPE}\noffset: 0\nreturned: 1\ntotal: 1"
    );
    assert_eq!(payload, expected, "full reopened TOON response");
    assert!(
        Instant::now() < operation_deadline,
        "reopen operation exceeded 90 seconds"
    );
}
