//! Opt-in hosted writes. Run the writer first, then copy its receipt to another
//! host and run only the reader there. Neither test reads a global Prime store.

use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::Duration,
};

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use uuid::Uuid;

#[path = "hosted_sync/remote.rs"]
mod remote;
use remote::{Config, Proof, Receipt};

const BINARY: &str = env!("CARGO_BIN_EXE_allsource-prime");
const SCHEMA: &str = "allstack.hosted-prime-sync.v1";
const EVENT: &str = "prime.node.created";

struct Session {
    child: Child,
    output: PathBuf,
    next_id: u64,
}

impl Drop for Session {
    fn drop(&mut self) {
        // Child lifetime belongs to this handle, including panic and timeout paths.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Session {
    async fn start(config: &Config, root: &Path, name: &str) -> Self {
        let output = root.join(name);
        let child = Command::new(BINARY)
            .env_clear()
            .env("HOME", root)
            .env("TMPDIR", root)
            .env("XDG_DATA_HOME", root)
            .env("XDG_CACHE_HOME", root)
            .env("PRIME_EMBED_MODEL_DIR", root.join("disabled-model"))
            .env("PRIME_SYNC_TO", &config.endpoint)
            .env("PRIME_API_KEY", &config.key)
            .current_dir(root)
            .args([
                "--mode",
                "mcp",
                "--format",
                "json",
                "--sync-interval-ms",
                "1000",
                "--data-dir",
            ])
            .arg(root.join("store"))
            .stdin(Stdio::piped())
            .stdout(Stdio::from(
                File::create(&output).expect("isolated MCP output"),
            ))
            .stderr(Stdio::from(
                File::create(root.join(format!("{name}.stderr"))).expect("private MCP stderr"),
            ))
            .spawn()
            .expect("start actual Prime MCP binary");
        let mut session = Self {
            child,
            output,
            next_id: 2,
        };
        session.send(
            &json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "protocolVersion":"2024-11-05","capabilities":{},
            "clientInfo":{"name":"allstack-hosted-sync-proof","version":"1"}}}),
        );
        let _ = session.response(1).await;
        session.send(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
        let stats = session.call("prime_stats", json!({})).await;
        assert!(stats["sync"]["enabled"] == true, "sync must be enabled");
        assert!(
            stats["sync"]["remote_url"] == config.endpoint,
            "wrong sync endpoint"
        );
        session
    }

    fn send(&mut self, message: &Value) {
        let input = self.child.stdin.as_mut().expect("owned MCP stdin");
        writeln!(input, "{message}").expect("send MCP request");
        input.flush().expect("flush MCP request");
    }

    async fn response(&mut self, id: u64) -> Value {
        loop {
            assert!(
                self.child.try_wait().expect("poll Prime child").is_none(),
                "Prime exited early"
            );
            let output = fs::read_to_string(&self.output).expect("read isolated MCP output");
            let complete = output.rfind('\n').map_or("", |end| &output[..=end]);
            for line in complete.lines() {
                let response: Value = serde_json::from_str(line).expect("MCP JSON response");
                if response["id"].as_u64() == Some(id) {
                    assert!(response.get("error").is_none(), "MCP request failed");
                    assert!(response["result"]["isError"] != true, "MCP tool failed");
                    return response["result"].clone();
                }
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    async fn call(&mut self, name: &str, arguments: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({"jsonrpc":"2.0","id":id,"method":"tools/call",
            "params":{"name":name,"arguments":arguments}}));
        let result = self.response(id).await;
        serde_json::from_str(
            result["content"][0]["text"]
                .as_str()
                .expect("MCP tool text"),
        )
        .expect("MCP tool JSON payload")
    }

    async fn create(&mut self, run: Uuid, ordinal: u8) -> Proof {
        let properties = json!({"name":format!("hosted-sync-proof-{run}-{ordinal}"),
            "domain":"hosted-sync-proof", "proof_run":run.to_string(), "ordinal":ordinal});
        let result = self
            .call(
                "prime_add_node",
                json!({"type":"decision","properties":properties}),
            )
            .await;
        let entity_id = result["entity_id"]
            .as_str()
            .expect("MCP created entity_id")
            .to_owned();
        assert!(
            entity_id.starts_with("node:decision:"),
            "expected decision entity"
        );
        Proof {
            entity_id,
            expected_properties: properties,
            remote_event_ids: Vec::new(),
        }
    }

    async fn close(&mut self) {
        self.child.stdin.take();
        loop {
            if let Some(status) = self.child.try_wait().expect("reap Prime child") {
                assert!(status.success(), "Prime shutdown failed");
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }
}

fn cursor(root: &Path) -> Option<DateTime<Utc>> {
    let bytes = match fs::read(root.join("store/.prime_sync_cursor.json")) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => panic!("cannot read isolated persisted cursor: {error}"),
    };
    let value: Value = serde_json::from_slice(&bytes).ok()?;
    let timestamp = value["last_pushed_at"].as_str()?;
    Some(
        DateTime::parse_from_rfc3339(timestamp)
            .expect("cursor timestamp")
            .with_timezone(&Utc),
    )
}

async fn delivered(
    config: &Config,
    root: &Path,
    proof: &mut Proof,
    after: Option<DateTime<Utc>>,
) -> DateTime<Utc> {
    loop {
        let ids = config.event_ids(proof).await;
        if !ids.is_empty()
            && let Some(cursor) = cursor(root)
            && after.is_none_or(|previous| cursor > previous)
        {
            proof.remote_event_ids = ids;
            return cursor;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "writes synthetic hosted data; requires explicit --ignored and proof environment"]
async fn hosted_prime_sync_write_restart() {
    tokio::time::timeout(Duration::from_mins(2), async {
        let config = Config::load();
        assert!(
            !config.receipt.exists(),
            "use a new receipt path for each writer run"
        );
        let run = Uuid::new_v4();
        config
            .reject_auth(&format!("node:decision:auth-proof-{run}"), true)
            .await;
        let parent = config.receipt.parent().unwrap_or(Path::new("."));
        let directory = tempfile::Builder::new()
            .prefix("hosted-sync-")
            .tempdir_in(parent)
            .expect("private proof directory");
        let root = directory.keep();
        let root = root.as_path();
        let mut first = Session::start(&config, root, "first.stdout").await;
        let mut one = first.create(run, 1).await;
        let before = delivered(&config, root, &mut one, None).await;
        first.close().await;
        drop(first);
        assert!(
            cursor(root) == Some(before),
            "first cursor must survive shutdown"
        );

        let mut second = Session::start(&config, root, "restart.stdout").await;
        assert!(cursor(root) == Some(before), "restart must preserve cursor");
        let recovered = second
            .call("prime_search", json!({"type":"decision", "fields":"full"}))
            .await;
        assert!(
            recovered["nodes"]
                .as_array()
                .expect("recovered decision nodes")
                .iter()
                .any(|node| node["id"] == one.entity_id
                    && node["properties"] == one.expected_properties),
            "same local store must recover first decision"
        );
        // Two configured intervals give a replaying loop an opportunity to run.
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert!(
            config.event_ids(&one).await == one.remote_event_ids,
            "restart replayed first event"
        );
        let mut two = second.create(run, 2).await;
        assert!(one.entity_id != two.entity_id, "proof entities must differ");
        let after = delivered(&config, root, &mut two, Some(before)).await;
        second.close().await;
        drop(second);
        assert!(
            cursor(root) == Some(after),
            "second cursor must survive shutdown"
        );
        for proof in [&one, &two] {
            assert!(
                config.event_ids(proof).await == proof.remote_event_ids,
                "remote IDs changed after restart"
            );
        }
        assert!(
            one.remote_event_ids != two.remote_event_ids,
            "remote event IDs must differ"
        );
        let receipt = Receipt {
            schema: SCHEMA.to_owned(),
            endpoint: config.endpoint.clone(),
            binary_path: fs::canonicalize(BINARY).expect("actual binary path"),
            cursor_before: before,
            cursor_after: after,
            proofs: vec![one, two],
        };
        let bytes = serde_json::to_vec_pretty(&receipt).expect("sanitized receipt JSON");
        assert!(
            !String::from_utf8_lossy(&bytes).contains(&config.key),
            "receipt contains credential"
        );
        let parent = config
            .receipt
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut file = tempfile::NamedTempFile::new_in(parent).expect("private receipt file");
        file.write_all(&bytes).expect("write receipt");
        file.as_file().sync_all().expect("persist receipt");
        assert!(
            file.persist_noclobber(&config.receipt).is_ok(),
            "publish receipt without overwrite"
        );
    })
    .await
    .expect("hosted writer exceeded single 120-second deadline");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "independent hosted readback; requires explicit --ignored and writer receipt"]
async fn hosted_prime_sync_independent_readback() {
    tokio::time::timeout(Duration::from_mins(2), async {
        let config = Config::load();
        let bytes = fs::read(&config.receipt).expect("copy writer receipt to reader host first");
        let receipt = serde_json::from_slice::<Receipt>(&bytes);
        assert!(receipt.is_ok(), "invalid hosted proof receipt");
        let receipt = receipt.unwrap();
        assert!(
            receipt.schema == SCHEMA && receipt.endpoint == config.endpoint,
            "receipt schema/endpoint mismatch"
        );
        assert!(
            receipt.binary_path.is_absolute(),
            "receipt must identify writer binary"
        );
        assert!(
            receipt.cursor_after > receipt.cursor_before,
            "cursor must advance"
        );
        assert!(
            receipt.proofs.len() == 2,
            "receipt must contain exactly two proof entities"
        );
        assert!(
            receipt.proofs[0].entity_id != receipt.proofs[1].entity_id,
            "duplicate proof entities"
        );
        config
            .reject_auth(&receipt.proofs[0].entity_id, false)
            .await;
        let mut observed = std::collections::HashSet::new();
        for proof in &receipt.proofs {
            assert!(
                proof.entity_id.starts_with("node:decision:"),
                "receipt proof entity type"
            );
            assert!(
                proof.expected_properties["domain"] == "hosted-sync-proof",
                "receipt proof domain"
            );
            assert!(
                proof.remote_event_ids.len() == 1,
                "expected exactly one remote event per entity"
            );
            let ids = config.event_ids(proof).await;
            config.graph_node_matches(proof).await;
            assert!(
                ids == proof.remote_event_ids,
                "hosted IDs/properties differ from writer receipt"
            );
            assert!(observed.insert(ids[0].clone()), "duplicate remote event ID");
        }
        assert!(
            observed.len() == 2,
            "independent reader must observe both proof events"
        );
    })
    .await
    .expect("hosted reader exceeded single 120-second deadline");
}
