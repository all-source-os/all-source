//! `heap_profile_run`: the S0 jemalloc profile of `heap_replay`, built and run
//! in the `apps/core/Dockerfile` builder image on aarch64-unknown-linux-musl.
//!
//! Usage:
//!   heap_profile_run --worktree <abs repo> --out <abs dir> [--max-chunks <n>]
//!
//! Stages run in order: `image` (the Dockerfile's `chef` stage), `test`
//! (`heap_profile_nonempty` on linux), `build` (`heap_replay` and
//! `heap_fixture` with `heap-profile`), `run` (fixture, clone, main scenario,
//! pprof dump). A stage repeats its chunk until the command exits 0 or
//! `--max-chunks` chunks have run.
//!
//! Every chunk is `timeout -k 15 290 <command>`, run and reaped by this
//! process. A chunk that hits its deadline leaves its container running, so
//! the container is removed with `docker rm -f` before the stage retries.
//! Cargo output lives in the named volume `core-heap-profile-target`, so a
//! rerun resumes; the fixture lives in `core-heap-profile-data`. Results land
//! in `<out>/heap_replay.json` and `<out>/heap.pprof`, and every chunk's
//! output in `<out>/logs/`.
//!
//! Nothing here runs two builds at once: invoke it only when no rustc runs on
//! the host.

use core_heap_replay::runner::{Job, Outcome, run_bounded};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

const IMAGE: &str = "allsource-core-heap-builder";
/// Shared with the earlier musl builds, so crate sources are already fetched.
const REGISTRY_VOLUME: &str = "core-musl-cargo-registry";
const TARGET_VOLUME: &str = "core-heap-profile-target";
const DATA_VOLUME: &str = "core-heap-profile-data";
const PLATFORM: &str = "linux/arm64";
const FEATURES: &str = "community,analytics,heap-profile";
const CHUNK_SECONDS: &str = "290";
const KILL_AFTER_SECONDS: &str = "15";
/// Must exceed the deadline plus the `-k` grace, or a KILL-escalated chunk is
/// cut short here instead of by `timeout`.
const CHUNK_RUNNER_DEADLINE: Duration = Duration::from_secs(320);
/// Pins the builder image's toolchain. The repo's rust-toolchain.toml names a
/// nightly that the production Dockerfile never copies.
const TOOLCHAIN: &str = "1.94.1-aarch64-unknown-linux-musl";
const OUTPUT_CAP: u64 = 256 << 20;
const DEFAULT_MAX_CHUNKS: usize = 24;
/// `timeout` exits 124 on its deadline and 137 when `-k` escalates to KILL.
const DEADLINE_EXIT_CODES: [i32; 2] = [124, 137];

#[derive(Clone, Copy)]
enum Stage {
    Image,
    Test,
    Build,
    Run,
}

const STAGES: [Stage; 4] = [Stage::Image, Stage::Test, Stage::Build, Stage::Run];

impl Stage {
    fn name(self) -> &'static str {
        match self {
            Stage::Image => "image",
            Stage::Test => "test",
            Stage::Build => "build",
            Stage::Run => "run",
        }
    }
}

struct Config {
    worktree: PathBuf,
    out: PathBuf,
    max_chunks: usize,
}

enum Chunk {
    Done,
    Deadline,
}

fn main() {
    let config = match parse_args(std::env::args().skip(1)) {
        Ok(config) => config,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };
    if let Err(message) = run(&config) {
        eprintln!("{message}");
        std::process::exit(1);
    }
    println!("ok:heap_profile_run:{}", config.out.display());
}

fn parse_args(mut args: impl Iterator<Item = String>) -> Result<Config, String> {
    let mut worktree = None;
    let mut out = None;
    let mut max_chunks = DEFAULT_MAX_CHUNKS;
    while let Some(flag) = args.next() {
        let mut value = || args.next().ok_or_else(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--worktree" => worktree = Some(PathBuf::from(value()?)),
            "--out" => out = Some(PathBuf::from(value()?)),
            "--max-chunks" => {
                max_chunks = value()?
                    .parse()
                    .map_err(|_| "--max-chunks needs a positive integer".to_string())?;
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    let worktree = worktree.ok_or("--worktree <abs repo> is required")?;
    let out = out.ok_or("--out <abs dir> is required")?;
    if !worktree.is_absolute() || !out.is_absolute() {
        return Err("--worktree and --out must be absolute paths".to_string());
    }
    if max_chunks == 0 {
        return Err("--max-chunks must be at least 1".to_string());
    }
    Ok(Config {
        worktree,
        out,
        max_chunks,
    })
}

fn run(config: &Config) -> Result<(), String> {
    std::fs::create_dir_all(config.out.join("logs"))
        .map_err(|e| format!("cannot create {}: {e}", config.out.display()))?;
    for stage in STAGES {
        run_stage(config, stage)?;
    }
    Ok(())
}

fn run_stage(config: &Config, stage: Stage) -> Result<(), String> {
    for chunk in 1..=config.max_chunks {
        let label = format!("{}-{chunk}", stage.name());
        match run_chunk(config, stage, &label)? {
            Chunk::Done => {
                println!("stage {}: done in {chunk} chunk(s)", stage.name());
                return Ok(());
            }
            Chunk::Deadline => {
                println!(
                    "stage {}: chunk {chunk} hit its deadline, rerun resumes",
                    stage.name()
                );
            }
        }
    }
    Err(format!(
        "stage {}: not done after {} chunks, see {}",
        stage.name(),
        config.max_chunks,
        config.out.join("logs").display()
    ))
}

fn run_chunk(config: &Config, stage: Stage, label: &str) -> Result<Chunk, String> {
    let container = container_name(stage);
    let command = stage_command(config, stage);
    let mut args: Vec<String> = ["-k", KILL_AFTER_SECONDS, CHUNK_SECONDS]
        .map(String::from)
        .into();
    args.extend(command);

    let logs = config.out.join("logs");
    let stderr_file = logs.join(format!("{label}.err"));
    let outcome = run_bounded(&Job {
        program: Path::new("timeout"),
        args: &args,
        envs: &[],
        cwd: None,
        deadline: CHUNK_RUNNER_DEADLINE,
        output_cap: OUTPUT_CAP,
        stdout_file: logs.join(format!("{label}.out")),
        stderr_file: stderr_file.clone(),
    })
    .map_err(|e| format!("{label}: cannot spawn timeout: {e}"))?;

    let succeeded = matches!(outcome, Outcome::Exited { code: Some(0), .. });
    if !succeeded && let Some(name) = container {
        remove_container(config, name, label);
    }
    match outcome {
        Outcome::Exited { code: Some(0), .. } => Ok(Chunk::Done),
        Outcome::Exited {
            code: Some(code), ..
        } if DEADLINE_EXIT_CODES.contains(&code) => Ok(Chunk::Deadline),
        Outcome::Exited { code, .. } => Err(format!(
            "{label}: exit {code:?}, see {}",
            stderr_file.display()
        )),
        Outcome::TimedOut => Err(format!("{label}: runner deadline passed")),
        Outcome::OutputTooLarge => Err(format!("{label}: output over cap")),
    }
}

/// `docker rm -f` on a container whose `docker run` client was killed. Owned
/// and bounded like any chunk; a failure here only means no container to remove.
fn remove_container(config: &Config, name: &str, label: &str) {
    let args = ["rm", "-f", name].map(String::from);
    let _ = run_bounded(&Job {
        program: Path::new("docker"),
        args: &args,
        envs: &[],
        cwd: None,
        deadline: Duration::from_secs(30),
        output_cap: 1 << 20,
        stdout_file: config.out.join("logs").join(format!("{label}-rm.out")),
        stderr_file: config.out.join("logs").join(format!("{label}-rm.err")),
    });
}

fn container_name(stage: Stage) -> Option<&'static str> {
    match stage {
        Stage::Image => None,
        Stage::Test => Some("heap-profile-test"),
        Stage::Build => Some("heap-profile-build"),
        Stage::Run => Some("heap-profile-run"),
    }
}

/// The argv after `timeout -k 15 290`, starting with `docker`.
fn stage_command(config: &Config, stage: Stage) -> Vec<String> {
    let worktree = config.worktree.display();
    let dockerfile = config.worktree.join("apps/core/Dockerfile");
    match stage {
        Stage::Image => [
            "docker".to_string(),
            "build".into(),
            "--platform".into(),
            PLATFORM.into(),
            "--target".into(),
            "chef".into(),
            "-f".into(),
            dockerfile.display().to_string(),
            "-t".into(),
            IMAGE.into(),
            worktree.to_string(),
        ]
        .into(),
        Stage::Test => docker_run(
            config,
            "heap-profile-test",
            &format!(
                "cargo test --profile heap-replay -p allsource-core --features {FEATURES} --locked --test heap_profile"
            ),
        ),
        Stage::Build => docker_run(
            config,
            "heap-profile-build",
            &format!(
                "cargo build --profile heap-replay -p allsource-core --features {FEATURES} --locked --example heap_replay --example heap_fixture"
            ),
        ),
        Stage::Run => docker_run(config, "heap-profile-run", RUN_SCRIPT),
    }
}

/// Fixture generation is skipped once `fixture.json` exists, so a chunk that
/// hit its deadline mid-write restarts the fixture; a finished one is reused.
/// Each run replays on a fresh copy of the fixture.
const RUN_SCRIPT: &str = "set -e; \
    B=/target/heap-replay/examples; \
    if [ ! -f /data/fixture/fixture.json ]; then \
        rm -rf /data/fixture; mkdir -p /data/fixture; \
        $B/heap_fixture --shape /src/tooling/core-heap-replay/shape.toml --out /data/fixture; \
    fi; \
    rm -rf /data/volume; cp -R /data/fixture /data/volume; \
    $B/heap_replay --volume /data/volume --scenario main --heap-profile /out/heap.pprof > /out/heap_replay.json";

fn docker_run(config: &Config, name: &str, script: &str) -> Vec<String> {
    let worktree = config.worktree.display();
    let out = config.out.display();
    [
        "docker".to_string(),
        "run".into(),
        "--rm".into(),
        "--name".into(),
        name.into(),
        "--platform".into(),
        PLATFORM.into(),
        "-v".into(),
        format!("{worktree}:/src:ro"),
        "-v".into(),
        format!("{REGISTRY_VOLUME}:/usr/local/cargo/registry"),
        "-v".into(),
        format!("{TARGET_VOLUME}:/target"),
        "-v".into(),
        format!("{DATA_VOLUME}:/data"),
        "-v".into(),
        format!("{out}:/out"),
        "-e".into(),
        format!("RUSTUP_TOOLCHAIN={TOOLCHAIN}"),
        "-e".into(),
        "CARGO_TARGET_DIR=/target".into(),
        "-e".into(),
        "CARGO_BUILD_JOBS=6".into(),
        "-w".into(),
        "/src".into(),
        IMAGE.into(),
        "sh".into(),
        "-c".into(),
        script.into(),
    ]
    .into()
}
