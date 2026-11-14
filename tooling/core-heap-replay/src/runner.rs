//! Runs a child under a deadline and an output cap.
//!
//! The child gets its own process group. On deadline or cap the whole group is
//! killed with `/bin/kill -KILL -- -<pgid>`, then the direct child is reaped.
//! Output goes to files, so a runaway writer is caught by its file size.

use std::{
    fs::File,
    io,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

const POLL: Duration = Duration::from_millis(20);

pub struct Job<'a> {
    pub program: &'a Path,
    pub args: &'a [String],
    /// The child's whole environment. The parent's is not inherited.
    pub envs: &'a [(String, String)],
    pub cwd: Option<&'a Path>,
    pub deadline: Duration,
    /// Maximum bytes across stdout and stderr.
    pub output_cap: u64,
    pub stdout_file: PathBuf,
    pub stderr_file: PathBuf,
}

#[derive(Debug)]
pub enum Outcome {
    Exited { code: Option<i32>, stdout: Vec<u8> },
    TimedOut,
    OutputTooLarge,
}

pub fn run_bounded(job: &Job<'_>) -> io::Result<Outcome> {
    let stdout = File::create(&job.stdout_file)?;
    let stderr = File::create(&job.stderr_file)?;
    let mut command = Command::new(job.program);
    command
        .args(job.args)
        .env_clear()
        .envs(job.envs.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr)
        .process_group(0);
    if let Some(cwd) = job.cwd {
        command.current_dir(cwd);
    }
    let mut child = command.spawn()?;
    let pgid = child.id();
    let started = Instant::now();

    loop {
        if let Some(status) = child.try_wait()? {
            if output_len(job) > job.output_cap {
                return Ok(Outcome::OutputTooLarge);
            }
            let stdout = std::fs::read(&job.stdout_file)?;
            return Ok(Outcome::Exited {
                code: status.code(),
                stdout,
            });
        }
        if output_len(job) > job.output_cap {
            kill_group(pgid);
            child.wait()?;
            return Ok(Outcome::OutputTooLarge);
        }
        if started.elapsed() >= job.deadline {
            kill_group(pgid);
            child.wait()?;
            return Ok(Outcome::TimedOut);
        }
        std::thread::sleep(POLL);
    }
}

fn output_len(job: &Job<'_>) -> u64 {
    let size = |path: &Path| std::fs::metadata(path).map_or(0, |m| m.len());
    size(&job.stdout_file) + size(&job.stderr_file)
}

fn kill_group(pgid: u32) {
    let _ = Command::new("/bin/kill")
        .arg("-KILL")
        .arg("--")
        .arg(format!("-{pgid}"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}
