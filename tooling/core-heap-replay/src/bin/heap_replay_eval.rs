//! `heap-replay-eval`: an autoresearch evaluator. Reads one request line on
//! stdin, builds the replay in the candidate worktree, runs it, checks every
//! gate, and writes one autoresearch protocol response on stdout. Exits 0 on
//! every outcome.
//!
//! Usage (literal arguments carried by `autoresearch.toml`):
//!   heap-replay-eval --artifacts <abs> --fixture-dir <abs> [--verify-fixture]
//!                    [--env KEY=VALUE]... [--self-sha256 <hex>]
//!                    [--policy-sha256 <hex>] [--record]
//!
//! The worktree comes from the request. `--record` writes the expected files
//! from this run (oracle creation only).

use core_heap_replay::{
    envelope::{Envelope, guarded},
    gates, policy, protocol,
    replay::{ReplayReport, ReplayRequest, run_replay_on_clone},
    runner::{Job, Outcome, run_bounded},
    sha256_hex,
    steps::{Budget, mirror_dir, run_step, sync_mirror},
};
use std::path::{Path, PathBuf};

const NAME: &str = "heap_replay";
/// Features shared by every replay build; the hasher feature is added per build.
const REPLAY_FEATURES: &str = "community,analytics";
const EXPECTED_DIR: &str = "tooling/core-heap-replay/expected";

struct Args {
    repo: PathBuf,
    artifacts: PathBuf,
    fixture: Option<PathBuf>,
    verify_fixture: bool,
    record: bool,
    envs: Vec<(String, String)>,
    self_sha: Option<String>,
    policy_sha: Option<String>,
}

fn parse_args(request: &protocol::Request) -> Args {
    let mut args = std::env::args().skip(1);
    let mut artifacts = None;
    let mut fixture = None;
    let mut verify_fixture = false;
    let mut record = false;
    let mut envs = Vec::new();
    let mut self_sha = None;
    let mut policy_sha = None;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--artifacts" => artifacts = args.next().map(PathBuf::from),
            "--fixture-dir" => fixture = args.next().map(PathBuf::from),
            "--verify-fixture" => verify_fixture = true,
            "--record" => record = true,
            "--env" => envs.extend(args.next().as_deref().and_then(protocol::split_env)),
            "--self-sha256" => self_sha = args.next(),
            "--policy-sha256" => policy_sha = args.next(),
            _ => {}
        }
    }
    Args {
        repo: PathBuf::from(&request.candidate_worktree),
        artifacts: artifacts.unwrap_or_else(|| std::env::temp_dir().join("core-heap-replay")),
        fixture,
        verify_fixture,
        record,
        envs,
        self_sha,
        policy_sha,
    }
}

#[derive(Clone, Copy)]
enum Hasher {
    Deterministic,
    Production,
}

impl Hasher {
    fn tag(self) -> &'static str {
        match self {
            Hasher::Deterministic => "deterministic",
            Hasher::Production => "production",
        }
    }

    fn features(self) -> String {
        match self {
            Hasher::Deterministic => format!("{REPLAY_FEATURES},deterministic-hash"),
            Hasher::Production => REPLAY_FEATURES.to_string(),
        }
    }
}

/// Build the named examples with the heap-replay profile, in a target dir per
/// hasher so each build is a warm no-op. Cargo writes every example into the
/// same profile directory, so each binary is copied out before the next build.
fn build_examples(
    args: &Args,
    budget: &Budget,
    hasher: Hasher,
    examples: &[&str],
) -> Result<Vec<PathBuf>, &'static str> {
    let target = args.artifacts.join(format!("target-{}", hasher.tag()));
    let mut argv: Vec<String> = [
        "build",
        "--profile",
        "heap-replay",
        "-p",
        "allsource-core",
        "--features",
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();
    argv.push(hasher.features());
    argv.extend(["--locked", "--offline", "--target-dir"].map(String::from));
    argv.push(target.display().to_string());
    for example in examples {
        argv.push("--example".to_string());
        argv.push((*example).to_string());
    }
    if run_step(
        &args.artifacts,
        budget,
        "cargo",
        &argv,
        &mirror_dir(&args.artifacts),
    )? != Some(0)
    {
        return Err("build_failed");
    }

    let bin_dir = args.artifacts.join("bin");
    std::fs::create_dir_all(&bin_dir).map_err(|_| "artifacts_unwritable")?;
    let profile_dir = target.join("heap-replay/examples");
    examples
        .iter()
        .map(|example| {
            let dest = bin_dir.join(format!("{example}-{}", hasher.tag()));
            std::fs::copy(profile_dir.join(example), &dest).map_err(|_| "build_copy_failed")?;
            Ok(dest)
        })
        .collect()
}

fn read_expected(repo: &Path, name: &str) -> Option<String> {
    std::fs::read_to_string(repo.join(EXPECTED_DIR).join(name))
        .ok()
        .map(|s| s.trim().to_string())
}

fn write_expected(repo: &Path, name: &str, value: &str) -> std::io::Result<()> {
    let dir = repo.join(EXPECTED_DIR);
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(name), format!("{value}\n"))
}

/// One evicted tenant id per line, in eviction order.
fn eviction_text(report: &ReplayReport) -> String {
    report.evictions.join("\n")
}

fn run_replay(
    args: &Args,
    budget: &Budget,
    bin: &Path,
    scenario: &str,
    label: &str,
) -> Result<ReplayReport, &'static str> {
    if budget.remaining().is_zero() {
        return Err("budget_exhausted");
    }
    let fixture = args.fixture.as_deref().ok_or("fixture_missing")?;
    let scratch = args.artifacts.join("runs").join(label);
    run_replay_on_clone(&ReplayRequest {
        bin,
        fixture,
        scenario,
        scratch: &scratch,
        envs: budget.envs(),
    })
    .map_err(|_| "replay_failed")
}

fn evaluate(args: &Args) -> Envelope {
    let mut env = Envelope::new(NAME);
    env.gate(
        "heap_eval_self_sha",
        gates::pinned(protocol::self_sha256(), args.self_sha.as_deref()),
    );
    env.gate(
        "protected_paths_exist",
        gates::protected_paths_exist(&args.repo),
    );
    env.gate(
        "policy_digest_pinned",
        gates::pinned(
            protocol::file_sha256(
                &args
                    .repo
                    .join(EXPECTED_DIR)
                    .join("expected-eviction-policy.txt"),
            ),
            args.policy_sha.as_deref(),
        ),
    );
    let Some(fixture) = args.fixture.clone() else {
        env.gate("fixture_present", false);
        env.fail("fixture_missing");
        return env;
    };
    let present = fixture.join("fixture.json").is_file();
    env.gate("fixture_present", present);
    if !present {
        env.fail("fixture_missing");
        return env;
    }

    let budget = Budget::with_env(args.envs.clone());
    let mirror = mirror_dir(&args.artifacts);
    let synced = sync_mirror(&args.artifacts, &budget, &args.repo, &mirror);
    let det = synced.and_then(|()| {
        build_examples(
            args,
            &budget,
            Hasher::Deterministic,
            &["heap_replay", "heap_fixture"],
        )
    });
    let prod =
        synced.and_then(|()| build_examples(args, &budget, Hasher::Production, &["heap_replay"]));
    env.gate("build_ok", det.is_ok() && prod.is_ok());
    let (Ok(det), Ok(prod)) = (det, prod) else {
        env.fail("build_failed");
        return env;
    };
    let (det_replay, det_fixture) = (&det[0], &det[1]);
    let prod_replay = &prod[0];

    if args.verify_fixture {
        let ok = verify_fixture(args, &budget, det_fixture, &fixture);
        env.gate("fixture_verified", ok);
    }

    let main_a = run_replay(args, &budget, det_replay, "main", "main-deterministic-1");
    let main_b = run_replay(args, &budget, det_replay, "main", "main-deterministic-2");
    let main_prod = run_replay(args, &budget, prod_replay, "main", "main-production");
    let evict = run_replay(
        args,
        &budget,
        det_replay,
        "eviction",
        "eviction-deterministic",
    );

    let (Ok(main_a), Ok(main_b), Ok(main_prod), Ok(evict)) = (main_a, main_b, main_prod, evict)
    else {
        env.gate("replay_completed", false);
        env.fail("replay_failed");
        return env;
    };
    env.gate("replay_completed", true);

    env.gate(
        "replay_repeatable",
        gates::replay_repeatable(&main_a, &main_b),
    );
    env.gate("peak_nonzero", gates::peak_nonzero(main_a.peak_live_bytes));
    env.gate(
        "replay_expected_status",
        gates::replay_expected_status(&[&main_a, &main_prod, &evict]),
    );
    env.gate(
        "ledger_two_sided",
        main_a
            .ledger_bytes
            .is_some_and(|ledger| gates::ledger_two_sided(main_a.steady_live_bytes, ledger)),
    );
    let listing = policy::listing(&args.repo).unwrap_or_default();
    env.gate(
        "eviction_policy_unchanged",
        gates::eviction_policy_unchanged(
            read_expected(&args.repo, "expected-eviction-policy.txt").as_deref(),
            &listing,
        ),
    );
    let ceiling = read_expected(&args.repo, "ceilings-S1.json")
        .and_then(|text| gates::rows_decoded_max(&text));
    env.gate(
        "rows_decoded_ok",
        gates::rows_decoded_ok(main_a.rows_decoded, ceiling),
    );

    if args.record {
        let wrote = write_expected(&args.repo, "expected-digest.txt", &main_a.query_digest)
            .and_then(|()| {
                write_expected(
                    &args.repo,
                    "expected-codec-digest.txt",
                    &main_a.codec_digest,
                )
            })
            .and_then(|()| {
                write_expected(&args.repo, "expected-geo-digest.txt", &main_a.geo_digest)
            })
            .and_then(|()| {
                write_expected(&args.repo, "expected-evictions.txt", &eviction_text(&evict))
            });
        env.gate("expected_recorded", wrote.is_ok());
    }

    let expected_query = read_expected(&args.repo, "expected-digest.txt");
    let expected_codec = read_expected(&args.repo, "expected-codec-digest.txt");
    let expected_geo = read_expected(&args.repo, "expected-geo-digest.txt");
    let expected_evictions = read_expected(&args.repo, "expected-evictions.txt");

    env.gate(
        "query_digest_equal",
        gates::digest_equal(expected_query.as_deref(), &main_a.query_digest),
    );
    env.gate(
        "prod_hasher_digest_equal",
        gates::prod_hasher_digest_equal(&main_prod.query_digest, &main_a.query_digest),
    );
    env.gate(
        "codec_digest_equal",
        gates::digest_equal(expected_codec.as_deref(), &main_a.codec_digest),
    );
    env.gate(
        "geo_digest_equal",
        gates::digest_equal(expected_geo.as_deref(), &main_a.geo_digest),
    );
    env.gate(
        "evictions_match",
        gates::digest_equal(expected_evictions.as_deref(), &eviction_text(&evict)),
    );

    env.metric("hasher", Hasher::Deterministic.tag());
    env.metric(
        "peak_live_bytes",
        core_heap_replay::replay::objective_peak(main_a.peak_live_bytes),
    );
    env.metric("raw_peak_live_bytes", main_a.peak_live_bytes);
    env.metric("steady_live_bytes", main_a.steady_live_bytes);
    env.metric("ledger_bytes", main_a.ledger_bytes);
    env.metric("rows_decoded", main_a.rows_decoded);
    env.metric("requests", main_a.requests);
    env.metric("query_digest", &main_a.query_digest);
    env.metric("geo_digest", &main_a.geo_digest);
    env.metric("codec_digest", &main_a.codec_digest);
    env.metric("evictions", &evict.evictions);
    env.metric("production_peak_live_bytes", main_prod.peak_live_bytes);
    env.metric("cold_load_largest_ms", main_a.cold_load_largest_ms);
    env
}

/// Regenerate the fixture with the same `heap_fixture` binary and compare the
/// manifest byte for byte. The manifest carries the content digest of every
/// Parquet file, so equal manifests mean equal fixtures.
fn verify_fixture(args: &Args, budget: &Budget, fixture_bin: &Path, fixture: &Path) -> bool {
    let out = args.artifacts.join("fixture-verify");
    let _ = std::fs::remove_dir_all(&out);
    let shape = args.repo.join("tooling/core-heap-replay/shape.toml");
    let argv = vec![
        "--shape".to_string(),
        shape.display().to_string(),
        "--out".to_string(),
        out.display().to_string(),
    ];
    let outcome = run_bounded(&Job {
        program: fixture_bin,
        args: &argv,
        envs: budget.envs(),
        cwd: None,
        deadline: budget.remaining(),
        output_cap: 256 << 20,
        stdout_file: args.artifacts.join("fixture-verify-stdout.txt"),
        stderr_file: args.artifacts.join("fixture-verify-stderr.txt"),
    });
    if !matches!(outcome, Ok(Outcome::Exited { code: Some(0), .. })) {
        return false;
    }
    let read = |dir: &Path| std::fs::read(dir.join("fixture.json")).ok();
    match (read(fixture), read(&out)) {
        (Some(a), Some(b)) => sha256_hex(&a) == sha256_hex(&b),
        _ => false,
    }
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
        protocol::response(&request, &envelope, &protocol::HEAP_REPLAY)
    );
}
