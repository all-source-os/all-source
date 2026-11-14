//! Runs one `heap_replay` process on a private clone of the fixture volume.

use crate::runner::{Job, Outcome, run_bounded};
use serde::Deserialize;
use std::{path::Path, process::Command, time::Duration};

/// Hard ceiling for one replay process. The evaluator's own deadline is 270 s.
pub const REPLAY_DEADLINE: Duration = Duration::from_secs(270);
const OUTPUT_CAP: u64 = 64 << 20;

/// Granularity of the objective. Two runs of one binary on one fixture differ
/// by up to ~1.4 KB of peak heap from allocations outside the fixed hasher's
/// reach; flooring to whole MiB makes the objective repeat byte for byte.
pub const PEAK_QUANTUM_BYTES: u64 = 1 << 20;

/// `peak_live_bytes` floored to [`PEAK_QUANTUM_BYTES`]: the objective value.
pub fn objective_peak(peak_live_bytes: u64) -> u64 {
    peak_live_bytes / PEAK_QUANTUM_BYTES * PEAK_QUANTUM_BYTES
}

pub struct ReplayRequest<'a> {
    pub bin: &'a Path,
    /// Fixture directory written by `heap_fixture`. Never modified.
    pub fixture: &'a Path,
    /// `main` or `eviction`.
    pub scenario: &'a str,
    /// Directory the clone and the child's output files are written under.
    pub scratch: &'a Path,
    pub envs: &'a [(String, String)],
}

/// The one-line report `heap_replay` prints on stdout.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ReplayReport {
    pub scenario: String,
    pub hasher: String,
    pub peak_live_bytes: u64,
    pub steady_live_bytes: u64,
    pub ledger_bytes: Option<u64>,
    pub rows_decoded: Option<u64>,
    pub query_digest: String,
    pub geo_digest: String,
    pub codec_digest: String,
    pub evictions: Vec<String>,
    pub requests: u64,
    pub status_mismatches: u64,
    pub epoch_mismatches: u64,
    pub model_event_leaks: u64,
    pub cold_load_largest_ms: u64,
}

#[derive(Debug)]
pub enum ReplayError {
    Io(String),
    Timeout,
    OutputTooLarge,
    Exit(Option<i32>),
    NoReport,
    BadReport(String),
}

impl ReplayReport {
    /// The outputs a repeat run must reproduce byte for byte. Timings and
    /// peaks are measurements and are excluded.
    pub fn same_outputs(&self, other: &Self) -> bool {
        self.query_digest == other.query_digest
            && self.geo_digest == other.geo_digest
            && self.codec_digest == other.codec_digest
            && self.evictions == other.evictions
            && self.rows_decoded == other.rows_decoded
            && self.requests == other.requests
    }
}

#[derive(Deserialize)]
struct Wrapped {
    heap_replay: ReplayReport,
}

/// Clone the fixture, run `heap_replay` on the clone, and parse its report.
pub fn run_replay_on_clone(request: &ReplayRequest<'_>) -> Result<ReplayReport, ReplayError> {
    // A volume left by an earlier run carries its committed read models, and
    // `cp` into an existing directory nests the clone instead of replacing it.
    let _ = std::fs::remove_dir_all(request.scratch);
    std::fs::create_dir_all(request.scratch).map_err(|e| ReplayError::Io(e.to_string()))?;
    let volume = request.scratch.join("volume");
    clone_tree(request.fixture, &volume)?;

    let args = vec![
        "--volume".to_string(),
        volume.display().to_string(),
        "--scenario".to_string(),
        request.scenario.to_string(),
    ];
    let outcome = run_bounded(&Job {
        program: request.bin,
        args: &args,
        envs: request.envs,
        cwd: None,
        deadline: REPLAY_DEADLINE,
        output_cap: OUTPUT_CAP,
        stdout_file: request.scratch.join("stdout.txt"),
        stderr_file: request.scratch.join("stderr.txt"),
    })
    .map_err(|e| ReplayError::Io(e.to_string()))?;

    let stdout = match outcome {
        Outcome::Exited {
            code: Some(0),
            stdout,
        } => stdout,
        Outcome::Exited { code, .. } => return Err(ReplayError::Exit(code)),
        Outcome::TimedOut => return Err(ReplayError::Timeout),
        Outcome::OutputTooLarge => return Err(ReplayError::OutputTooLarge),
    };
    parse_report(&String::from_utf8_lossy(&stdout))
}

/// The report is the last line that is a JSON object with a `heap_replay` key.
pub fn parse_report(stdout: &str) -> Result<ReplayReport, ReplayError> {
    let line = stdout
        .lines()
        .rev()
        .find(|line| line.starts_with("{\"heap_replay\""))
        .ok_or(ReplayError::NoReport)?;
    serde_json::from_str::<Wrapped>(line)
        .map(|w| w.heap_replay)
        .map_err(|e| ReplayError::BadReport(e.to_string()))
}

/// APFS clone on macOS; reflink where the filesystem has it, copy otherwise.
/// Copy-on-write clone of a volume directory (APFS on macOS, reflink elsewhere).
pub fn clone_tree(from: &Path, to: &Path) -> Result<(), ReplayError> {
    let mut command = Command::new("/bin/cp");
    if cfg!(target_os = "macos") {
        command.arg("-c").arg("-R");
    } else {
        command.arg("-R").arg("--reflink=auto");
    }
    let status = command
        .arg(from)
        .arg(to)
        .status()
        .map_err(|e| ReplayError::Io(e.to_string()))?;
    if status.success() {
        Ok(())
    } else {
        Err(ReplayError::Io("clone failed".to_string()))
    }
}
