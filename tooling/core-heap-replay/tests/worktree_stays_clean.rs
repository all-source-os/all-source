//! A replay writes nothing into the repository: the worktree status is the same
//! before and after a run.
//!
//! Needs the built `heap_replay` and the fixture, passed through the same
//! environment variables as `replay_two_runs_one_volume`; the test fails when
//! they are absent so it cannot pass by not running.

use core_heap_replay::{
    replay::{ReplayRequest, run_replay_on_clone},
    scan::repo_root,
};
use std::{path::PathBuf, process::Command};

fn required_path(name: &str) -> PathBuf {
    let value = std::env::var(name)
        .unwrap_or_else(|_| panic!("{name} must name the built artifact; run via the evaluator"));
    PathBuf::from(value)
}

fn status(root: &std::path::Path) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["status", "--porcelain", "--untracked-files=all"])
        .output()
        .expect("git runs");
    assert!(output.status.success(), "git status failed");
    String::from_utf8(output.stdout).expect("utf-8 status")
}

#[test]
fn worktree_stays_clean() {
    let replay_bin = required_path("HEAP_REPLAY_BIN");
    let fixture = required_path("HEAP_FIXTURE_DIR");
    let root = repo_root();
    let scratch = std::env::temp_dir().join(format!("worktree-clean-{}", std::process::id()));

    let before = status(&root);
    run_replay_on_clone(&ReplayRequest {
        bin: &replay_bin,
        fixture: &fixture,
        scenario: "main",
        scratch: &scratch,
        envs: &[],
    })
    .expect("replay runs");
    let after = status(&root);

    assert_eq!(before, after, "a replay changed the worktree");
}
