//! Owns and reaps the evaluator and mail router; either child failing restarts the stack.

mod probe;
mod process;
mod provision;

use std::{
    path::Path,
    process::{Command, ExitCode},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

fn main() -> ExitCode {
    match execute() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("monitor: {error}");
            ExitCode::FAILURE
        }
    }
}

fn execute() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "import-secrets") {
        return provision::import(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg.starts_with("probe-")) {
        return probe::execute(&args[0]);
    }
    provision::prepare(Path::new("/run/monitor"))?;
    for directory in ["/data/prometheus", "/data/alertmanager", "/data/rules"] {
        std::fs::create_dir_all(directory).map_err(|_| "cannot create monitor state directory")?;
    }
    process::capture(
        Command::new("promtool").args(["check", "config", "/app/prometheus.yml"]),
        None,
    )?;
    process::capture(
        Command::new("amtool").args(["check-config", "/run/monitor/alertmanager.json"]),
        None,
    )?;
    if args.first().is_some_and(|arg| arg == "check") {
        println!("monitor: evaluator and mail-router configuration validated");
        return Ok(());
    }
    let stopping = Arc::new(AtomicBool::new(false));
    for signal in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGINT] {
        signal_hook::flag::register(signal, stopping.clone())
            .map_err(|_| "cannot register shutdown")?;
    }
    let mut router = process::OwnedChild::spawn(Command::new("alertmanager").args([
        "--config.file=/run/monitor/alertmanager.json",
        "--storage.path=/data/alertmanager",
        "--web.listen-address=127.0.0.1:9093",
        "--cluster.listen-address=",
        "--log.level=warn",
    ]))?;
    let mut evaluator = process::OwnedChild::spawn(Command::new("prometheus").args([
        "--config.file=/app/prometheus.yml",
        "--storage.tsdb.path=/data/prometheus",
        "--storage.tsdb.retention.time=2d",
        "--storage.tsdb.retention.size=512MB",
        "--web.listen-address=127.0.0.1:9090",
        "--web.enable-lifecycle",
        "--log.level=warn",
    ]))?;
    println!("monitor: evaluator and email router started; private listeners, persistent state");
    while !stopping.load(Ordering::Relaxed) {
        if router.exited()? || evaluator.exited()? {
            return Err("monitor child exited; restarting both services".into());
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    Ok(())
}
