use archive_budget_audit::{Limits, audit};
use std::{path::PathBuf, process::ExitCode};

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let Some(path) = args.next().filter(|_| args.len() == 0) else {
        eprintln!("Usage: archive-budget-audit <storage-directory>");
        return ExitCode::from(2);
    };
    match audit(&PathBuf::from(path), &Limits::default()) {
        Ok(report) => {
            println!(
                "{}",
                serde_json::to_string(&report).expect("serializable report")
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            // Error labels intentionally contain no paths, tenant IDs or footer content.
            eprintln!("archive audit refused: {error}");
            ExitCode::FAILURE
        }
    }
}
