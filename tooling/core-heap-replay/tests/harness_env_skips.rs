//! The evaluator skips a harness test only when the artifacts that test needs are
//! absent from the environment, and records each skip.

use core_heap_replay::gates::env_skipped_tests;

fn only<'a>(present: &'a [&'a str]) -> impl Fn(&str) -> bool + 'a {
    move |name: &str| present.contains(&name)
}

#[test]
fn all_artifacts_present_skips_nothing() {
    let env = ["HEAP_REPLAY_BIN", "HEAP_FIXTURE_DIR", "ROLLBACK_BIN"];
    assert!(env_skipped_tests(only(&env)).is_empty());
}

#[test]
fn no_artifacts_skips_both_env_tests() {
    assert_eq!(
        env_skipped_tests(only(&[])),
        vec!["worktree_stays_clean", "rollback_volume"]
    );
}

#[test]
fn replay_artifacts_alone_leave_rollback_skipped() {
    let env = ["HEAP_REPLAY_BIN", "HEAP_FIXTURE_DIR"];
    assert_eq!(env_skipped_tests(only(&env)), vec!["rollback_volume"]);
}

#[test]
fn rollback_binary_with_fixture_leaves_worktree_skipped() {
    let env = ["ROLLBACK_BIN", "HEAP_FIXTURE_DIR"];
    assert_eq!(env_skipped_tests(only(&env)), vec!["worktree_stays_clean"]);
}

#[test]
fn fixture_alone_skips_both() {
    let env = ["HEAP_FIXTURE_DIR"];
    assert_eq!(
        env_skipped_tests(only(&env)),
        vec!["worktree_stays_clean", "rollback_volume"]
    );
}

#[test]
fn rollback_binary_alone_skips_both() {
    let env = ["ROLLBACK_BIN"];
    assert_eq!(
        env_skipped_tests(only(&env)),
        vec!["worktree_stays_clean", "rollback_volume"]
    );
}
