//! `volume-boot`: start a built `allsource-core` server on a volume and check
//! that it answers `GET /health`. `rollback_volume` runs it with the `f4b53930`
//! binary on a volume written by this branch.
//!
//! Usage:
//!   volume-boot --binary <abs> --volume <abs> --port <u16> [--deadline-secs <u64>]
//!
//! The child is killed and reaped on every path, including a panic here. One
//! JSONL v1 envelope is printed; the exit code is 0 for every outcome.

use core_heap_replay::envelope::{Envelope, guarded};
use std::{
    io::{Read, Write},
    net::TcpStream,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

const NAME: &str = "volume_boot";
const DEFAULT_DEADLINE_SECS: u64 = 120;

struct Args {
    binary: PathBuf,
    volume: PathBuf,
    port: u16,
    deadline: Duration,
}

fn parse_args() -> Option<Args> {
    let mut args = std::env::args().skip(1);
    let (mut binary, mut volume, mut port, mut deadline_secs) =
        (None, None, None, DEFAULT_DEADLINE_SECS);
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--binary" => binary = args.next().map(PathBuf::from),
            "--volume" => volume = args.next().map(PathBuf::from),
            "--port" => port = args.next().and_then(|v| v.parse().ok()),
            "--deadline-secs" => deadline_secs = args.next().and_then(|v| v.parse().ok())?,
            _ => {}
        }
    }
    Some(Args {
        binary: binary?,
        volume: volume?,
        port: port?,
        deadline: Duration::from_secs(deadline_secs),
    })
}

/// Kills and reaps the server when dropped, so no path leaves it running.
struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// `true` when `GET /health` on `port` answers `200`.
fn health_ok(port: u16) -> bool {
    let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    if stream
        .write_all(b"GET /health HTTP/1.0\r\nHost: 127.0.0.1\r\n\r\n")
        .is_err()
    {
        return false;
    }
    let mut response = String::new();
    if stream.read_to_string(&mut response).is_err() {
        return false;
    }
    response
        .lines()
        .next()
        .is_some_and(|line| line.contains(" 200 "))
}

fn evaluate(args: &Args) -> Envelope {
    let mut env = Envelope::new(NAME);
    let logs = args.volume.with_extension("boot-stdout.txt");
    let Ok(stdout) = std::fs::File::create(&logs) else {
        env.gate("server_started", false);
        env.fail("logs_unwritable");
        return env;
    };
    let Ok(stderr) = stdout.try_clone() else {
        env.gate("server_started", false);
        env.fail("logs_unwritable");
        return env;
    };
    let child = Command::new(&args.binary)
        .env("ALLSOURCE_DATA_DIR", &args.volume)
        .env("ALLSOURCE_PORT", args.port.to_string())
        .env("ALLSOURCE_HOST", "127.0.0.1")
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr)
        .spawn();
    let Ok(child) = child else {
        env.gate("server_started", false);
        env.fail("spawn_failed");
        return env;
    };
    let mut server = Server(child);

    let started = Instant::now();
    let mut answered = false;
    let mut exited = false;
    while started.elapsed() < args.deadline {
        if matches!(server.0.try_wait(), Ok(Some(_))) {
            exited = true;
            break;
        }
        if health_ok(args.port) {
            answered = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    env.gate("server_started", !exited);
    env.gate("health_ok", answered);
    if !answered {
        env.fail(if exited {
            "server_exited"
        } else {
            "health_timeout"
        });
    }
    drop(server);
    env
}

fn main() {
    let envelope = match parse_args() {
        Some(args) => guarded(NAME, || evaluate(&args)),
        None => {
            let mut env = Envelope::new(NAME);
            env.gate("server_started", false);
            env.fail("usage");
            env
        }
    };
    println!("{}", envelope.to_line());
    std::process::exit(envelope.exit_code());
}
