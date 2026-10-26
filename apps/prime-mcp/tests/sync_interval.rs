use std::{
    fs::{self, File},
    path::PathBuf,
    process::{Child, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

use serde_json::{Value, json};
use tempfile::TempDir;

const CHILD_TIMEOUT: Duration = Duration::from_secs(20);
const MCP_REQUESTS: &str = concat!(
    "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2024-11-05\",\"capabilities\":{},\"clientInfo\":{\"name\":\"sync-interval-test\",\"version\":\"1\"}}}\n",
    "{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n",
    "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"prime_stats\",\"arguments\":{}}}\n",
);

// The guard owns the child until it is reaped, including during unwinding.
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

#[derive(Debug)]
struct Run {
    directory: TempDir,
    status: ExitStatus,
    stdout: String,
    stderr: String,
}

impl Run {
    fn store(&self) -> PathBuf {
        self.directory.path().join("store")
    }

    fn assert_no_store(&self) {
        assert!(
            !self.store().exists(),
            "store created before rejection: {self:?}"
        );
        assert!(
            !self.directory.path().join(".prime").exists(),
            "default home store created: {self:?}"
        );
    }
}

fn run(args: &[&str], interval_env: Option<&str>) -> Run {
    let directory = tempfile::tempdir().expect("disposable Prime directory");
    let root = directory.path();
    let input = root.join("stdin");
    let output = root.join("stdout");
    let errors = root.join("stderr");
    fs::write(&input, MCP_REQUESTS).expect("write MCP requests");

    let mut command = Command::new(env!("CARGO_BIN_EXE_allsource-prime"));
    command
        .env_clear()
        .env("HOME", root)
        .env("TMPDIR", root)
        .env("PRIME_EMBED_MODEL_DIR", root.join("no-model"))
        .current_dir(root)
        .arg("--data-dir")
        .arg(root.join("store"))
        .args(args)
        .stdin(Stdio::from(File::open(&input).expect("open MCP input")))
        .stdout(Stdio::from(File::create(&output).expect("create stdout")))
        .stderr(Stdio::from(File::create(&errors).expect("create stderr")));
    if let Some(value) = interval_env {
        command.env("PRIME_SYNC_INTERVAL_MS", value);
    }

    let deadline = Instant::now() + CHILD_TIMEOUT;
    let mut child = OwnedChild {
        child: command.spawn().expect("spawn actual Prime binary"),
        reaped: false,
    };
    let (status, timed_out) = child.finish(deadline);
    let result = Run {
        stdout: fs::read_to_string(output).expect("read stdout"),
        stderr: fs::read_to_string(errors).expect("read stderr"),
        directory,
        status,
    };
    assert!(!timed_out, "Prime exceeded {CHILD_TIMEOUT:?}: {result:?}");
    result
}

fn assert_rejected(args: &[&str], interval_env: Option<&str>) {
    let result = run(args, interval_env);
    assert_eq!(result.status.code(), Some(2), "{result:?}");
    assert!(
        result.stdout.is_empty(),
        "protocol output on parse error: {result:?}"
    );
    assert!(result.stderr.contains("error:"), "{result:?}");
    assert!(result.stderr.contains("--sync-interval-ms"), "{result:?}");
    result.assert_no_store();
}

fn assert_mcp_session(args: &[&str], interval_env: Option<&str>) {
    let result = run(args, interval_env);
    assert_eq!(result.status.code(), Some(0), "{result:?}");
    assert!(result.store().is_dir(), "MCP store missing: {result:?}");
    assert!(!result.stderr.contains("panicked"), "{result:?}");
    assert!(
        !result.stderr.contains("`period` must be non-zero"),
        "{result:?}"
    );

    let responses: Vec<Value> = result
        .stdout
        .lines()
        .map(|line| serde_json::from_str(line).expect("JSON-RPC response"))
        .collect();
    assert_eq!(responses.len(), 2, "{result:?}");
    for (response, id) in responses.iter().zip([1, 2]) {
        assert_eq!(response["jsonrpc"], "2.0", "{response}");
        assert_eq!(response["id"], id, "{response}");
        assert!(response.get("error").is_none(), "{response}");
        assert!(response["result"].is_object(), "{response}");
    }
    let initialized = &responses[0]["result"];
    assert_eq!(initialized["protocolVersion"], "2024-11-05");
    assert_eq!(initialized["serverInfo"]["name"], "allsource-prime");
    assert_eq!(
        initialized["serverInfo"]["version"],
        env!("CARGO_PKG_VERSION")
    );
    assert!(initialized["capabilities"]["tools"].is_object());

    let stats_result = &responses[1]["result"];
    assert_ne!(stats_result["isError"], true, "{stats_result}");
    let content = stats_result["content"].as_array().expect("tool content");
    assert_eq!(content.len(), 1, "{stats_result}");
    assert_eq!(content[0]["type"], "text");
    let stats: Value = serde_json::from_str(content[0]["text"].as_str().expect("stats text"))
        .expect("stats JSON payload");
    assert_eq!(stats["total_nodes"], 0, "{stats}");
    assert_eq!(stats["total_edges"], 0, "{stats}");
    assert_eq!(stats["nodes_by_type"], json!({}), "{stats}");
    assert_eq!(stats["sync"]["enabled"], false, "{stats}");
    assert!(stats["sync"]["remote_url"].is_null(), "{stats}");
}

#[test]
fn explicit_zero_is_rejected_before_store_creation() {
    assert_rejected(&["--sync-interval-ms", "0"], None);
}

#[test]
fn environment_zero_is_rejected_before_store_creation() {
    assert_rejected(&[], Some("0"));
}

#[test]
fn explicit_zero_overrides_positive_environment_and_is_rejected() {
    assert_rejected(&["--sync-interval-ms", "0"], Some("25"));
}

#[test]
fn explicit_malformed_interval_is_rejected() {
    assert_rejected(&["--sync-interval-ms", "invalid"], None);
}

#[test]
fn environment_malformed_interval_is_rejected() {
    assert_rejected(&[], Some("invalid"));
}

#[test]
fn explicit_negative_interval_is_rejected() {
    assert_rejected(&["--sync-interval-ms=-1"], None);
}

#[test]
fn environment_negative_interval_is_rejected() {
    assert_rejected(&[], Some("-1"));
}

#[test]
fn explicit_overflow_interval_is_rejected() {
    assert_rejected(&["--sync-interval-ms", "18446744073709551616"], None);
}

#[test]
fn environment_overflow_interval_is_rejected() {
    assert_rejected(&[], Some("18446744073709551616"));
}

#[test]
fn zero_is_rejected_in_one_shot_modes_before_store_creation() {
    for mode in ["warm", "hound", "install"] {
        assert_rejected(&["--mode", mode, "--sync-interval-ms", "0"], None);
    }
}

#[test]
fn positive_cli_overrides_zero_environment_and_serves_mcp() {
    assert_mcp_session(&["--sync-interval-ms", "1"], Some("0"));
}

#[test]
fn positive_cli_overrides_malformed_environment_and_serves_mcp() {
    assert_mcp_session(&["--sync-interval-ms", "25"], Some("invalid"));
}

#[test]
fn default_interval_serves_initialize_and_prime_stats() {
    assert_mcp_session(&[], None);
}

#[test]
fn positive_environment_serves_initialize_and_prime_stats() {
    assert_mcp_session(&[], Some("25"));
}

#[test]
fn positive_cli_boundaries_serve_initialize_and_prime_stats() {
    for value in ["1", "1000", "18446744073709551615"] {
        assert_mcp_session(&["--sync-interval-ms", value], None);
    }
}

#[test]
fn help_documents_minimum_default_and_environment_without_store_creation() {
    let result = run(&["--help"], None);
    assert_eq!(result.status.code(), Some(0), "{result:?}");
    assert!(result.stderr.is_empty(), "{result:?}");
    let interval_help = result
        .stdout
        .split("--sync-interval-ms")
        .nth(1)
        .expect("interval help");
    assert!(interval_help.contains("at least 1"), "{result:?}");
    assert!(interval_help.contains("[default: 1000]"), "{result:?}");
    assert!(
        interval_help.contains("PRIME_SYNC_INTERVAL_MS"),
        "{result:?}"
    );
    result.assert_no_store();
}

#[test]
fn help_short_circuits_invalid_interval_environment() {
    for value in ["0", "invalid", "-1", "18446744073709551616"] {
        let result = run(&["--help"], Some(value));
        assert_eq!(result.status.code(), Some(0), "{result:?}");
        assert!(result.stdout.contains("--sync-interval-ms"), "{result:?}");
        assert!(result.stderr.is_empty(), "{result:?}");
        result.assert_no_store();
    }
}

#[test]
fn version_short_circuits_invalid_interval_environment() {
    for value in [
        None,
        Some("0"),
        Some("invalid"),
        Some("-1"),
        Some("18446744073709551616"),
    ] {
        let result = run(&["--version"], value);
        assert_eq!(result.status.code(), Some(0), "{result:?}");
        assert_eq!(
            result.stdout.trim(),
            concat!("allsource-prime ", env!("CARGO_PKG_VERSION"))
        );
        assert!(result.stderr.is_empty(), "{result:?}");
        result.assert_no_store();
    }
}
