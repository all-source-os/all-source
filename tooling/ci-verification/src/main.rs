use allsource_ci_verification::{baseline, compare};
use std::{env, fs, process::ExitCode};

const USAGE: &str =
    "usage: allsource-ci-verification baseline <file> | compare <file> <max-percent> <base-file>";

fn read(path: &str) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))
}

fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.as_slice() {
        [command, path] if command == "baseline" => {
            let report = baseline(&read(path)?)?;
            println!(
                "validated {} nonzero baseline measurements",
                report.measurements
            );
        }
        [command, path, limit, base] if command == "compare" => {
            let max_percent = limit.parse().map_err(|_| "invalid percentage limit")?;
            let report = compare(&read(path)?, &read(base)?, max_percent)?;
            println!(
                "compared {} instruction measurements, worst regression {}.{:02}%",
                report.measurements,
                report.worst_basis_points / 100,
                report.worst_basis_points % 100
            );
            for name in &report.added {
                println!("added without a base measurement: {name}");
            }
        }
        _ => return Err(USAGE.to_owned()),
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
