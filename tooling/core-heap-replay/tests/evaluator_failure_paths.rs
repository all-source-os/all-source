//! Failure paths of the evaluator runner: deadline, output cap, panic.

use core_heap_replay::{
    envelope::{Envelope, Status, guarded},
    runner::{Job, Outcome, run_bounded},
};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("heap-eval-test-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn job<'a>(
    dir: &'a Path,
    program: &'a Path,
    args: &'a [String],
    deadline: Duration,
    output_cap: u64,
) -> Job<'a> {
    Job {
        program,
        args,
        envs: &[],
        cwd: None,
        deadline,
        output_cap,
        stdout_file: dir.join("stdout.txt"),
        stderr_file: dir.join("stderr.txt"),
    }
}

#[test]
fn deadline_kills_the_whole_process_group() {
    let dir = scratch("deadline");
    let args = vec!["30".to_string()];
    let started = Instant::now();
    let outcome = run_bounded(&job(
        &dir,
        Path::new("/bin/sleep"),
        &args,
        Duration::from_secs(1),
        1 << 20,
    ))
    .expect("spawn");
    assert!(matches!(outcome, Outcome::TimedOut), "got {outcome:?}");
    assert!(
        started.elapsed() < Duration::from_secs(15),
        "deadline did not bound the run: {:?}",
        started.elapsed()
    );
}

#[test]
fn output_over_the_cap_is_killed_and_reported() {
    let dir = scratch("output");
    let args = vec!["-c".to_string(), "head -c 2000000 /dev/zero".to_string()];
    let outcome = run_bounded(&job(
        &dir,
        Path::new("/bin/sh"),
        &args,
        Duration::from_secs(60),
        1 << 20,
    ))
    .expect("spawn");
    assert!(
        matches!(outcome, Outcome::OutputTooLarge),
        "got {outcome:?}"
    );
}

#[test]
fn nonzero_exit_is_reported_with_its_code() {
    let dir = scratch("exit");
    let args = vec!["-c".to_string(), "exit 101".to_string()];
    let outcome = run_bounded(&job(
        &dir,
        Path::new("/bin/sh"),
        &args,
        Duration::from_secs(60),
        1 << 20,
    ))
    .expect("spawn");
    match outcome {
        Outcome::Exited { code, .. } => assert_eq!(code, Some(101)),
        other => panic!("expected Exited, got {other:?}"),
    }
}

#[test]
fn panic_becomes_a_sentinel_envelope() {
    let envelope: Envelope = guarded("heap_replay", || panic!("boom in gate"));
    assert_eq!(envelope.status, Status::Panic);
    assert_eq!(envelope.exit_code(), 0);
    let line = envelope.to_line();
    assert!(!line.contains('\n'), "one envelope per line: {line}");
    assert!(
        !line.contains("boom"),
        "panic text must not leak into gates: {line}"
    );
}

#[test]
fn evaluator_binary_exits_zero_with_a_response_on_a_missing_fixture() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };

    let dir = scratch("missing-fixture");
    let request = r#"{"protocol_version":1,"evaluator_id":"heap_replay","run_id":"r","baseline_commit":"b","evaluated_commit":"c","candidate_worktree":"/nonexistent","changed_paths":[],"declared_environment":{},"artifact_directory":"/nonexistent","cancellation_id":"x"}"#;
    let mut child = Command::new(env!("CARGO_BIN_EXE_heap-replay-eval"))
        .arg("--fixture-dir")
        .arg(dir.join("does-not-exist"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run evaluator");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(format!("{request}\n").as_bytes())
        .expect("write request");
    let output = child.wait_with_output().expect("evaluator exits");
    assert_eq!(output.status.code(), Some(0), "evaluator must exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 1, "exactly one response line: {stdout}");
    let response: serde_json::Value = serde_json::from_str(lines[0]).expect("response is JSON");
    assert_eq!(response["result"]["status"], "success", "{}", lines[0]);
    let fixture = response["result"]["output"]["measurements"]
        .as_array()
        .expect("measurements")
        .iter()
        .find(|m| m["name"] == "fixture_present")
        .expect("fixture_present is reported");
    assert_eq!(fixture["outcome"]["passed"], false, "{}", lines[0]);
}
