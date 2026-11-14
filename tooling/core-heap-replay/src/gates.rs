//! Hard-gate predicates. The evaluators call these with measured values, and
//! the negative controls call the same functions with values that must fail.

use crate::{replay::ReplayReport, steps::TestCounts};

/// The recorded value exists and equals the measured one.
pub fn digest_equal(expected: Option<&str>, actual: &str) -> bool {
    expected == Some(actual)
}

pub fn prod_hasher_digest_equal(production: &str, deterministic: &str) -> bool {
    production == deterministic
}

pub fn peak_nonzero(peak_live_bytes: u64) -> bool {
    peak_live_bytes > 0
}

pub fn replay_repeatable(first: &ReplayReport, second: &ReplayReport) -> bool {
    first.same_outputs(second)
}

pub fn replay_expected_status(reports: &[&ReplayReport]) -> bool {
    reports
        .iter()
        .all(|r| r.status_mismatches == 0 && r.epoch_mismatches == 0 && r.model_event_leaks == 0)
}

/// `|steady - ledger| <= 0.10 * steady`, in integers.
pub fn ledger_two_sided(steady_live_bytes: u64, ledger_bytes: u64) -> bool {
    steady_live_bytes > 0
        && (steady_live_bytes.abs_diff(ledger_bytes) as u128) * 10 <= steady_live_bytes as u128
}

/// The recorded listing equals the listing of the tree under test.
pub fn eviction_policy_unchanged(expected: Option<&str>, listing: &str) -> bool {
    expected.is_some_and(|e| e.trim() == listing.trim())
}

/// The recorded test set equals the set listed from the tree under test.
pub fn tests_unchanged(expected: Option<&str>, listing: &str) -> bool {
    expected.is_some_and(|e| e.trim() == listing.trim())
}

pub fn tests_pass(exit_code: Option<i32>, counts: Option<TestCounts>) -> bool {
    exit_code == Some(0) && counts.is_some_and(|c| c.failed == 0)
}

/// Passed, failed and ignored together equal the number of tests the same build lists.
pub fn expected_tests_ran(counts: Option<TestCounts>, listed: usize) -> bool {
    counts.is_some_and(|c| listed > 0 && c.passed + c.failed + c.ignored == listed as u64)
}

pub fn tooling_tests_pass(exit_code: Option<i32>, counts: Option<TestCounts>) -> bool {
    exit_code == Some(0) && counts.is_some_and(|c| c.failed == 0 && c.passed > 0)
}

/// Harness test functions that need built artifacts, with the variables each one reads.
/// `--skip` matches function names, so these must equal the `#[test] fn` names.
const ENV_TESTS: &[(&str, &[&str])] = &[
    (
        "worktree_stays_clean",
        &["HEAP_REPLAY_BIN", "HEAP_FIXTURE_DIR"],
    ),
    ("rollback_volume", &["ROLLBACK_BIN", "HEAP_FIXTURE_DIR"]),
];

/// Names of the `ENV_TESTS` whose variables are not all set; these are skipped
/// in the evaluator run and reported in its envelope.
pub fn env_skipped_tests(present: impl Fn(&str) -> bool) -> Vec<&'static str> {
    ENV_TESTS
        .iter()
        .filter(|(_, vars)| !vars.iter().all(|v| present(v)))
        .map(|(name, _)| *name)
        .collect()
}

/// A measured digest equals the literal the manifest carries.
pub fn pinned(actual: Option<String>, expected: Option<&str>) -> bool {
    matches!((actual.as_deref(), expected), (Some(a), Some(e)) if a == e)
}

/// `rows_decoded_max` from `ceilings-S1.json`.
pub fn rows_decoded_max(ceilings: &str) -> Option<u64> {
    serde_json::from_str::<serde_json::Value>(ceilings)
        .ok()?
        .get("rows_decoded_max")?
        .as_u64()
}

pub fn rows_decoded_ok(rows: Option<u64>, ceiling: Option<u64>) -> bool {
    matches!((rows, ceiling), (Some(r), Some(c)) if r <= c)
}

/// Paths the S1 structure relies on. Each must exist in the tree under test.
pub const PROTECTED_PATHS: &[&str] = &[
    "apps/core/src/infrastructure/persistence/archive_output.rs",
    "apps/core/src/infrastructure/persistence/wal.rs",
    "apps/core/src/infrastructure/persistence/event_codec.rs",
];

pub fn protected_paths_exist(root: &std::path::Path) -> bool {
    PROTECTED_PATHS.iter().all(|p| root.join(p).is_file())
}
