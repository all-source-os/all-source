//! The frozen test set: the names `cargo test -- --list` prints, once under
//! default features and once under the feature set `ci.yml` runs for the
//! integration tests.

/// The feature set of the `Run integration tests` step in `.github/workflows/ci.yml`.
pub const EMBEDDED_FEATURES: &str = "embedded-phase2,embedded-sync,embedded-replicant,embedded-streaming,embedded-projections,embedded-toon,prime-recall,replication,multi-tenant,cold-tier-s3";

/// The sets the listing covers, in the order the recorded file lists them.
pub const SETS: &[&str] = &["default", "embedded"];

/// Test names from `--list --format terse` output: each `name: test` line,
/// sorted and without duplicates. The summary line and any other output are
/// not test names and are skipped.
pub fn test_names(list_output: &str) -> Vec<String> {
    let mut names: Vec<String> = list_output
        .lines()
        .filter_map(|line| line.strip_suffix(": test"))
        .map(str::to_string)
        .collect();
    names.sort();
    names.dedup();
    names
}

/// One line per test: `<set> <name>`, sets in the order given.
pub fn listing(sets: &[(&str, Vec<String>)]) -> String {
    sets.iter()
        .flat_map(|(set, names)| names.iter().map(move |name| format!("{set} {name}\n")))
        .collect()
}
