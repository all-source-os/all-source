//! Budgeted child steps shared by the evaluator binaries.

use crate::runner::{Job, Outcome, run_bounded};
use std::{
    path::Path,
    time::{Duration, Instant},
};

/// Internal deadline for one evaluator run. Each child gets the remainder.
pub const EVALUATOR_BUDGET: Duration = Duration::from_secs(270);

/// Wall-clock budget shared by every child of one evaluator run, and the
/// environment those children run in.
pub struct Budget {
    end: Instant,
    envs: Vec<(String, String)>,
}

impl Budget {
    pub fn new() -> Self {
        Self::with_env(Vec::new())
    }

    pub fn with_env(envs: Vec<(String, String)>) -> Self {
        Self {
            end: Instant::now() + EVALUATOR_BUDGET,
            envs,
        }
    }

    pub fn envs(&self) -> &[(String, String)] {
        &self.envs
    }

    /// The same deadline, with `extra` added to the environment.
    pub fn with_extra_env(&self, extra: &[(&str, &str)]) -> Self {
        let mut envs = self.envs.clone();
        envs.extend(
            extra
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string())),
        );
        Self {
            end: self.end,
            envs,
        }
    }

    pub fn remaining(&self) -> Duration {
        self.end.saturating_duration_since(Instant::now())
    }
}

impl Default for Budget {
    fn default() -> Self {
        Self::new()
    }
}

/// Run one child to completion under the budget, with its output in files
/// under `artifacts/steps/`. Returns its exit code.
pub fn run_step(
    artifacts: &Path,
    budget: &Budget,
    program: &str,
    argv: &[String],
    cwd: &Path,
) -> Result<Option<i32>, &'static str> {
    let remaining = budget.remaining();
    if remaining.is_zero() {
        return Err("budget_exhausted");
    }
    let step_dir = artifacts.join("steps");
    std::fs::create_dir_all(&step_dir).map_err(|_| "artifacts_unwritable")?;
    let outcome = run_bounded(&Job {
        program: Path::new(program),
        args: argv,
        envs: budget.envs(),
        cwd: Some(cwd),
        deadline: remaining,
        output_cap: 256 << 20,
        stdout_file: step_dir.join("stdout.txt"),
        stderr_file: step_dir.join("stderr.txt"),
    })
    .map_err(|_| "spawn_failed")?;
    match outcome {
        Outcome::Exited { code, .. } => Ok(code),
        Outcome::TimedOut => Err("deadline"),
        Outcome::OutputTooLarge => Err("output_too_large"),
    }
}

/// The build mirror for an evaluator run, under its artifact directory.
pub fn mirror_dir(artifacts: &Path) -> std::path::PathBuf {
    artifacts.join("mirror")
}

/// Copy `source` into `dest` for building. `rsync -c` without `-t` rewrites only
/// the files whose content changed, so unchanged files keep their mtimes and
/// cargo's fingerprints hold across candidate worktrees, whose paths differ.
pub fn sync_mirror(
    artifacts: &Path,
    budget: &Budget,
    source: &Path,
    dest: &Path,
) -> Result<(), &'static str> {
    std::fs::create_dir_all(dest).map_err(|_| "artifacts_unwritable")?;
    let argv: Vec<String> = [
        "-rc",
        "--delete",
        "--exclude=.git",
        "--exclude=.autoresearch/",
        "--exclude=target/",
        "--exclude=node_modules/",
        &format!("{}/", source.display()),
        &format!("{}/", dest.display()),
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();
    match run_step(artifacts, budget, "/usr/bin/rsync", &argv, artifacts)? {
        Some(0) => Ok(()),
        _ => Err("mirror_failed"),
    }
}

/// Counts from libtest's summary line, `test result: ok. N passed; M failed; ...`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TestCounts {
    pub passed: u64,
    pub failed: u64,
    pub ignored: u64,
}

/// Counts summed over every `test result:` line in `output` (one per test
/// binary). `None` when the output has no summary line at all.
pub fn parse_test_result(output: &str) -> Option<TestCounts> {
    let mut total = TestCounts {
        passed: 0,
        failed: 0,
        ignored: 0,
    };
    let mut seen = false;
    for line in output
        .lines()
        .filter(|line| line.trim_start().starts_with("test result:"))
    {
        let count = |label: &str| -> Option<u64> {
            let head = line.split(&format!(" {label}")).next()?;
            head.rsplit(|c: char| !c.is_ascii_digit())
                .find(|s| !s.is_empty())?
                .parse()
                .ok()
        };
        total.passed += count("passed")?;
        total.failed += count("failed")?;
        total.ignored += count("ignored")?;
        seen = true;
    }
    seen.then_some(total)
}
