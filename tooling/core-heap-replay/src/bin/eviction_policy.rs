//! `eviction-policy`: print the digest listing of the eviction-policy items.
//! With `--record`, write it to `expected/expected-eviction-policy.txt`, which
//! `eviction_policy_unchanged` compares against.
//!
//! Usage:
//!   eviction-policy [--repo <abs>] [--record]

use core_heap_replay::{policy, scan::repo_root};
use std::path::PathBuf;

const EXPECTED: &str = "tooling/core-heap-replay/expected/expected-eviction-policy.txt";

fn main() {
    let mut repo = repo_root();
    let mut record = false;
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--repo" => repo = args.next().map(PathBuf::from).unwrap_or(repo),
            "--record" => record = true,
            _ => {}
        }
    }
    let text = match policy::listing(&repo) {
        Ok(text) => text,
        Err(reason) => {
            eprintln!("eviction-policy: {reason}");
            std::process::exit(1);
        }
    };
    if record {
        if let Err(err) = std::fs::write(repo.join(EXPECTED), &text) {
            eprintln!("eviction-policy: write failed: {err}");
            std::process::exit(1);
        }
    }
    print!("{text}");
}
