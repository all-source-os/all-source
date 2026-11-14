//! `core-tests-eval`: an autoresearch evaluator. Reads one request line on
//! stdin, runs the allsource-core lib suite and the harness tests in the
//! candidate worktree, and writes one autoresearch protocol response on stdout.
//! Exits 0 on every outcome.
//!
//! Usage (literal arguments carried by `autoresearch.toml`):
//!   core-tests-eval --artifacts <abs> [--env KEY=VALUE]... [--self-sha256 <hex>]
//!                   [--tests-sha256 <hex>] [--record]

use core_heap_replay::{
    envelope::{Envelope, guarded},
    gates, listing, protocol,
    steps::{Budget, TestCounts, mirror_dir, parse_test_result, run_step, sync_mirror},
};
use std::path::{Path, PathBuf};

const NAME: &str = "core_tests";
/// Needs a built `heap_replay` and a generated fixture, passed through
/// `HEAP_REPLAY_BIN` and `HEAP_FIXTURE_DIR`. This evaluator does not build
/// them, so the test is run separately with both set.
const HARNESS_SKIP: &str = "replay_two_runs_one_volume";
const EXPECTED_TESTS: &str = "tooling/core-heap-replay/expected/tests-S1.txt";

struct Args {
    repo: PathBuf,
    artifacts: PathBuf,
    record: bool,
    envs: Vec<(String, String)>,
    self_sha: Option<String>,
    tests_sha: Option<String>,
}

fn parse_args(request: &protocol::Request) -> Args {
    let mut args = std::env::args().skip(1);
    let mut artifacts = None;
    let mut record = false;
    let mut envs = Vec::new();
    let mut self_sha = None;
    let mut tests_sha = None;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--artifacts" => artifacts = args.next().map(PathBuf::from),
            "--record" => record = true,
            "--env" => envs.extend(args.next().as_deref().and_then(protocol::split_env)),
            "--self-sha256" => self_sha = args.next(),
            "--tests-sha256" => tests_sha = args.next(),
            _ => {}
        }
    }
    Args {
        repo: PathBuf::from(&request.candidate_worktree),
        artifacts: artifacts.unwrap_or_else(|| std::env::temp_dir().join("core-tests")),
        record,
        envs,
        self_sha,
        tests_sha,
    }
}

/// Test names `cargo test --tests -- --list` prints for one feature set.
/// `None` when cargo fails or lists no test.
fn list_set(
    args: &Args,
    budget: &Budget,
    target: &str,
    features: Option<&str>,
) -> Result<Option<Vec<String>>, &'static str> {
    let mut argv: Vec<String> = [
        "test",
        "-p",
        "allsource-core",
        "--locked",
        "--offline",
        "--tests",
        "--target-dir",
        target,
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();
    if let Some(features) = features {
        argv.extend(["--features".to_string(), features.to_string()]);
    }
    argv.extend(["--", "--list", "--format", "terse"].map(String::from));
    let code = run_step(
        &args.artifacts,
        budget,
        "cargo",
        &argv,
        &mirror_dir(&args.artifacts),
    )?;
    let stdout =
        std::fs::read_to_string(args.artifacts.join("steps/stdout.txt")).unwrap_or_default();
    let names = listing::test_names(&stdout);
    Ok((code == Some(0) && !names.is_empty()).then_some(names))
}

/// Number of lib tests `cargo test --lib -- --list` prints. Zero when the listing fails.
fn lib_test_count(args: &Args, budget: &Budget, target: &str) -> Result<usize, &'static str> {
    let argv: Vec<String> = [
        "test",
        "-p",
        "allsource-core",
        "--locked",
        "--offline",
        "--lib",
        "--target-dir",
        target,
        "--",
        "--list",
        "--format",
        "terse",
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();
    let code = run_step(
        &args.artifacts,
        budget,
        "cargo",
        &argv,
        &mirror_dir(&args.artifacts),
    )?;
    let stdout =
        std::fs::read_to_string(args.artifacts.join("steps/stdout.txt")).unwrap_or_default();
    Ok(match code {
        Some(0) => listing::test_names(&stdout).len(),
        _ => 0,
    })
}

/// The frozen test set for both feature sets, or `None` when either listing failed.
fn listed_tests(args: &Args, budget: &Budget, target: &str) -> Option<String> {
    let mut sets = Vec::new();
    for set in listing::SETS {
        let features = (*set == "embedded").then_some(listing::EMBEDDED_FEATURES);
        let names = list_set(args, budget, target, features).ok().flatten()?;
        sets.push((*set, names));
    }
    Some(listing::listing(&sets))
}

/// Run one `cargo test` invocation and return its exit code and counts.
fn cargo_test(
    artifacts: &Path,
    budget: &Budget,
    cwd: &Path,
    argv: &[String],
) -> Result<(Option<i32>, Option<TestCounts>), &'static str> {
    let code = run_step(artifacts, budget, "cargo", argv, cwd)?;
    let stdout = std::fs::read_to_string(artifacts.join("steps/stdout.txt")).unwrap_or_default();
    Ok((code, parse_test_result(&stdout)))
}

fn evaluate(args: &Args) -> Envelope {
    let mut env = Envelope::new(NAME);
    env.gate(
        "tests_eval_self_sha",
        gates::pinned(protocol::self_sha256(), args.self_sha.as_deref()),
    );
    let budget = Budget::with_env(args.envs.clone());
    let mirror = mirror_dir(&args.artifacts);
    if let Err(reason) = sync_mirror(&args.artifacts, &budget, &args.repo, &mirror) {
        env.fail(reason);
        return env;
    }
    let target = args.artifacts.join("target-tests");
    let target = target.display().to_string();

    let listed = listed_tests(args, &budget, &target);
    if let (true, Some(text)) = (args.record, listed.as_deref()) {
        let _ = std::fs::write(args.repo.join(EXPECTED_TESTS), text);
    }
    let recorded_path = args.repo.join(EXPECTED_TESTS);
    env.gate(
        "tests_digest_pinned",
        gates::pinned(
            protocol::file_sha256(&recorded_path),
            args.tests_sha.as_deref(),
        ),
    );
    let recorded = std::fs::read_to_string(&recorded_path).ok();
    env.gate(
        "tests_unchanged",
        gates::tests_unchanged(recorded.as_deref(), listed.as_deref().unwrap_or_default()),
    );

    let lib_listed = match lib_test_count(args, &budget, &target) {
        Ok(count) => count,
        Err(reason) => {
            env.gate("tests_pass", false);
            env.gate("expected_tests_ran", false);
            env.fail(reason);
            return env;
        }
    };

    let core_argv: Vec<String> = [
        "test",
        "-p",
        "allsource-core",
        "--locked",
        "--offline",
        "--lib",
        "--target-dir",
        &target,
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();
    let core = cargo_test(&args.artifacts, &budget, &mirror, &core_argv);
    let (core_code, core_counts) = match core {
        Ok(found) => found,
        Err(reason) => {
            env.gate("tests_pass", false);
            env.fail(reason);
            return env;
        }
    };
    let core_ok = gates::tests_pass(core_code, core_counts);
    env.gate("tests_pass", core_ok);
    env.gate(
        "expected_tests_ran",
        gates::expected_tests_ran(core_counts, lib_listed),
    );
    if let Some(counts) = core_counts {
        env.metric("core_passed", counts.passed);
        env.metric("core_failed", counts.failed);
        env.metric("core_ignored", counts.ignored);
    }

    let harness_manifest = mirror.join("tooling/core-heap-replay/Cargo.toml");
    let harness_target = args.artifacts.join("target-harness").display().to_string();
    let env_skips = gates::env_skipped_tests(|v| args.envs.iter().any(|(k, _)| k == v));
    env.metric("skipped_env_tests", &env_skips);
    let mut harness_argv: Vec<String> = [
        "test",
        "--manifest-path",
        &harness_manifest.display().to_string(),
        "--locked",
        "--offline",
        "--target-dir",
        &harness_target,
        "--no-fail-fast",
        "--",
        "--skip",
        HARNESS_SKIP,
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();
    for name in &env_skips {
        harness_argv.extend(["--skip".to_string(), (*name).to_string()]);
    }
    let source = args.repo.display().to_string();
    let harness_budget = budget.with_extra_env(&[("CORE_SOURCE_ROOT", source.as_str())]);
    let harness = cargo_test(&args.artifacts, &harness_budget, &mirror, &harness_argv);
    let (harness_code, harness_counts) = match harness {
        Ok(found) => found,
        Err(reason) => {
            env.gate("tooling_tests_pass", false);
            env.fail(reason);
            return env;
        }
    };
    env.gate(
        "tooling_tests_pass",
        gates::tooling_tests_pass(harness_code, harness_counts),
    );
    if let Some(counts) = harness_counts {
        env.metric("tooling_passed", counts.passed);
    }
    if !core_ok {
        env.fail("tests_failed");
    }
    env
}

fn main() {
    let request = match protocol::read_request(std::io::stdin().lock()) {
        Ok(request) => request,
        Err(reason) => {
            println!("{}", protocol::failure(&reason));
            return;
        }
    };
    let args = parse_args(&request);
    let envelope = guarded(NAME, || evaluate(&args));
    println!(
        "{}",
        protocol::response(&request, &envelope, &protocol::CORE_TESTS)
    );
}
