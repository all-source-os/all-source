use std::{
    fs::{self, File},
    io::Write,
    path::Path,
    process::{Child, Command, Stdio},
    time::Duration,
};

use serde_json::{Value, json};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

struct Session(Child);

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn responses(file: &Path) -> Vec<Value> {
    let text = fs::read_to_string(file).expect("read MCP output");
    let complete = text.rfind('\n').map_or("", |end| &text[..=end]);
    complete
        .lines()
        .map(|line| serde_json::from_str(line).expect("complete JSON-RPC response"))
        .collect()
}

fn tool_payload(response: &Value) -> Value {
    assert!(response.get("error").is_none(), "{response}");
    assert_ne!(response["result"]["isError"], true, "{response}");
    serde_json::from_str(
        response["result"]["content"][0]["text"]
            .as_str()
            .expect("tool text"),
    )
    .expect("JSON tool payload")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn configured_interval_delivers_created_node_and_runs_email_polling() {
    tokio::time::timeout(Duration::from_secs(20), async {
        let remote = MockServer::start().await;
        Mock::given(method("POST")).and(path("/api/v1/events"))
            .respond_with(ResponseTemplate::new(200)).mount(&remote).await;
        Mock::given(method("GET")).and(path("/api/v1/events/query"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"events":[],"count":0})))
            .mount(&remote).await;
        let directory = tempfile::tempdir().expect("disposable store");
        let root = directory.path();
        let stdout = root.join("stdout");
        let stderr = root.join("stderr");
        let mut command = Command::new(env!("CARGO_BIN_EXE_allsource-prime"));
        command.env_clear().env("HOME", root).env("TMPDIR", root)
            .env("PRIME_EMBED_MODEL_DIR", root.join("no-model"))
            .current_dir(root)
            .args(["--mode", "mcp", "--sync-interval-ms", "1", "--sync-to"])
            .arg(remote.uri()).args(["--api-key", "allstack-local-proof-no-credential", "--data-dir"])
            .arg(root.join("store"))
            .stdin(Stdio::piped())
            .stdout(Stdio::from(File::create(&stdout).expect("stdout file")))
            .stderr(Stdio::from(File::create(&stderr).expect("stderr file")));
        let mut session = Session(command.spawn().expect("start actual Prime MCP binary"));
        for message in [
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"allstack-sync-proof","version":"1"}}}),
            json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"prime_add_node","arguments":{"type":"decision","properties":{"name":"Interval delivery proof","domain":"native-qualification"}}}}),
            json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"prime_search","arguments":{"type":"decision","fields":"full"}}}),
            json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"prime_stats","arguments":{}}}),
        ] {
            writeln!(session.0.stdin.as_mut().expect("owned MCP stdin"), "{message}").expect("send MCP request");
        }
        session.0.stdin.as_mut().unwrap().flush().expect("flush MCP requests");
        let observed = loop {
            assert!(session.0.try_wait().expect("poll Prime").is_none(), "Prime exited early: {}", fs::read_to_string(&stderr).unwrap());
            let protocol = responses(&stdout);
            let requests = remote.received_requests().await.expect("captured HTTP requests");
            let cursor = fs::read(root.join("store/.prime_sync_cursor.json"))
                .ok().and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
            if protocol.len() == 4
                && requests.iter().any(|r| r.method.as_str() == "POST" && r.url.path() == "/api/v1/events")
                && requests.iter().any(|r| r.method.as_str() == "GET" && r.url.path() == "/api/v1/events/query")
                && cursor.is_some_and(|value| value["last_pushed_at"].is_string()) {
                break (protocol, requests);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        };
        let (protocol, requests) = observed;
        assert_eq!(protocol.iter().map(|r| r["id"].as_i64().unwrap()).collect::<Vec<_>>(), [1,2,3,4]);
        let created = tool_payload(&protocol[1]);
        let readback = tool_payload(&protocol[2]);
        assert_eq!(readback["nodes"][0]["id"], created["entity_id"]);
        assert_eq!(readback["nodes"][0]["properties"]["name"], "Interval delivery proof");
        let stats = tool_payload(&protocol[3]);
        assert_eq!(stats["total_nodes"], 1);
        assert_eq!(stats["sync"]["enabled"], true);
        let pushed: Vec<Value> = requests.iter().filter(|r| r.method.as_str() == "POST" && r.url.path() == "/api/v1/events").map(|r| serde_json::from_slice(&r.body).expect("event JSON")).collect();
        for request in requests.iter().filter(|r| r.method.as_str() == "POST") {
            assert_eq!(request.headers.get("authorization").expect("Bearer header").to_str().unwrap(), "Bearer allstack-local-proof-no-credential");
        }
        assert!(pushed.iter().any(|event| event["event_type"] == "prime.node.created" && event["entity_id"] == created["entity_id"] && event["payload"]["properties"]["name"] == "Interval delivery proof"), "{pushed:?}");
        session.0.stdin.take();
        let status = loop {
            if let Some(status) = session.0.try_wait().expect("poll shutdown") { break status; }
            tokio::time::sleep(Duration::from_millis(10)).await;
        };
        assert!(status.success());
        let errors = fs::read_to_string(&stderr).expect("read stderr");
        assert!(!errors.contains("panicked"), "{errors}");
        assert!(!errors.contains("`period` must be non-zero"), "{errors}");
    }).await.expect("MCP delivery exceeded its single deadline");
}
