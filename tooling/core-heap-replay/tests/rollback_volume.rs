//! `rollback_volume`: a volume written by this branch boots under the deployed
//! `f4b53930` server binary.
//!
//! Needs `ROLLBACK_BIN` (the `allsource-core` binary built from `f4b53930`) and
//! `HEAP_FIXTURE_DIR` (a volume written by `heap_fixture` from this branch).
//! The test fails when either is absent so it cannot pass by not running.

use core_heap_replay::replay::clone_tree;
use std::{net::TcpListener, path::PathBuf, process::Command};

fn required_path(name: &str) -> PathBuf {
    let value = std::env::var(name)
        .unwrap_or_else(|_| panic!("{name} must name the built artifact; run with it set"));
    PathBuf::from(value)
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .expect("a free loopback port")
        .port()
}

#[test]
fn rollback_volume() {
    let binary = required_path("ROLLBACK_BIN");
    let fixture = required_path("HEAP_FIXTURE_DIR");
    let scratch = std::env::temp_dir().join(format!("rollback-volume-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).expect("scratch dir");
    let volume = scratch.join("volume");
    std::fs::create_dir_all(&volume).expect("volume dir");
    clone_tree(&fixture, &volume.join("storage")).expect("volume clones");

    let output = Command::new(env!("CARGO_BIN_EXE_volume-boot"))
        .arg("--binary")
        .arg(&binary)
        .arg("--volume")
        .arg(&volume)
        .arg("--port")
        .arg(free_port().to_string())
        .output()
        .expect("volume-boot runs");

    let stdout = String::from_utf8(output.stdout).expect("utf-8 envelope");
    let envelope: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("one JSON envelope");
    assert_eq!(
        envelope["gates"]["health_ok"], "pass",
        "the f4b53930 binary did not answer /health on the branch's volume: {envelope}"
    );
    let _ = std::fs::remove_dir_all(&scratch);
}
