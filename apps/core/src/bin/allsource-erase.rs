//! Explicit offline maintenance; never connects to a running Core or remote storage.

use allsource_core::infrastructure::persistence::erasure_maintenance::{
    ErasureManifest, apply, preview,
};
use std::{
    path::Path,
    process::ExitCode,
    time::{Duration, Instant},
};

fn usage() {
    eprintln!(
        "Usage: allsource-erase preview <MANIFEST>\n       allsource-erase apply <MANIFEST> <INVENTORY_SHA256> --writers-stopped\nStop all Core writers, replicas, consumers and backup jobs before apply.\nEvery retained copy must be inventoried. No remote deletion is performed.\nOn failure keep consumers stopped, inspect again and resume with the same manifest."
    );
}

fn main() -> ExitCode {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() == 1 && matches!(args[0].as_str(), "--help" | "-h") {
        usage();
        return ExitCode::SUCCESS;
    }
    let valid = args.len() == 2 && args[0] == "preview"
        || args.len() == 4 && args[0] == "apply" && args[3] == "--writers-stopped";
    if !valid {
        usage();
        return ExitCode::from(2);
    }
    let deadline = Instant::now() + Duration::from_secs(240);
    let result = ErasureManifest::read(Path::new(&args[1])).and_then(|manifest| {
        if args[0] == "preview" {
            preview(&manifest, deadline)
        } else {
            apply(&manifest, &args[2], true, deadline)
        }
    });
    match result {
        Ok(report) => match serde_json::to_string(&report) {
            Ok(json) => {
                println!("{json}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("Cannot encode receipt: {error}");
                ExitCode::FAILURE
            }
        },
        Err(error) => {
            let detail = match error {
                allsource_core::error::AllSourceError::StorageError(message) => message,
                _ => "Input could not be decoded or validated. Inspect the manifest and retained files locally; record contents are omitted.".into(),
            };
            eprintln!("Erasure incomplete: {detail}. Keep writers stopped until verified.");
            ExitCode::FAILURE
        }
    }
}
