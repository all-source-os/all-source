//! Negative controls: every hard gate fails on a value it must reject, and every
//! item digested by `eviction_policy_unchanged` fails when it changes. Each
//! control also checks the accepting value, so a gate that always fails cannot
//! pass its own control.

use core_heap_replay::{
    gates, listing,
    policy::{collect_items, listing_from_items},
    replay::ReplayReport,
    scan::repo_root,
    steps::TestCounts,
};

const EXPECTED_POLICY: &str = "tooling/core-heap-replay/expected/expected-eviction-policy.txt";

fn report() -> ReplayReport {
    ReplayReport {
        scenario: "main".to_string(),
        hasher: "deterministic".to_string(),
        peak_live_bytes: 1_000,
        steady_live_bytes: 1_000,
        ledger_bytes: Some(1_000),
        rows_decoded: Some(7),
        query_digest: "q".to_string(),
        geo_digest: "g".to_string(),
        codec_digest: "c".to_string(),
        evictions: vec!["t1".to_string(), "t2".to_string()],
        requests: 3,
        status_mismatches: 0,
        epoch_mismatches: 0,
        model_event_leaks: 0,
        cold_load_largest_ms: 1,
    }
}

fn counts(passed: u64, failed: u64) -> Option<TestCounts> {
    Some(TestCounts {
        passed,
        failed,
        ignored: 0,
    })
}

#[test]
fn query_digest_equal_fails_without_a_recorded_value() {
    assert!(gates::digest_equal(Some("q"), "q"));
    assert!(!gates::digest_equal(None, "q"));
}

#[test]
fn query_digest_equal_fails_on_a_different_value() {
    assert!(!gates::digest_equal(Some("q"), "q2"));
}

#[test]
fn codec_digest_equal_fails_on_a_different_value() {
    assert!(gates::digest_equal(Some("c"), "c"));
    assert!(!gates::digest_equal(Some("c"), "c-changed"));
}

#[test]
fn geo_digest_equal_fails_on_a_different_value() {
    assert!(gates::digest_equal(Some("g"), "g"));
    assert!(!gates::digest_equal(Some("g"), "g-changed"));
}

#[test]
fn evictions_match_fails_on_a_reordered_sequence() {
    let recorded = "t1\nt2";
    assert!(gates::digest_equal(Some(recorded), "t1\nt2"));
    assert!(!gates::digest_equal(Some(recorded), "t2\nt1"));
}

#[test]
fn prod_hasher_digest_equal_fails_on_a_mismatch() {
    assert!(gates::prod_hasher_digest_equal("q", "q"));
    assert!(!gates::prod_hasher_digest_equal("q", "q2"));
}

#[test]
fn peak_nonzero_fails_on_zero_peak() {
    assert!(gates::peak_nonzero(1));
    assert!(!gates::peak_nonzero(0));
}

#[test]
fn replay_repeatable_fails_when_a_repeat_run_differs() {
    let first = report();
    let mut second = report();
    assert!(gates::replay_repeatable(&first, &second));
    second.query_digest = "q2".to_string();
    assert!(!gates::replay_repeatable(&first, &second));
}

#[test]
fn replay_expected_status_fails_on_a_status_mismatch() {
    let ok = report();
    let mut bad = report();
    bad.status_mismatches = 1;
    assert!(gates::replay_expected_status(&[&ok, &ok]));
    assert!(!gates::replay_expected_status(&[&ok, &bad]));
}

#[test]
fn replay_expected_status_fails_on_an_epoch_or_model_leak() {
    let ok = report();
    let mut epoch = report();
    epoch.epoch_mismatches = 1;
    let mut leak = report();
    leak.model_event_leaks = 1;
    assert!(!gates::replay_expected_status(&[&epoch]));
    assert!(!gates::replay_expected_status(&[&leak]));
    assert!(gates::replay_expected_status(&[&ok]));
}

#[test]
fn ledger_two_sided_fails_outside_ten_percent_in_either_direction() {
    assert!(gates::ledger_two_sided(1_000, 1_000));
    assert!(gates::ledger_two_sided(1_000, 1_100));
    assert!(gates::ledger_two_sided(1_000, 900));
    assert!(
        !gates::ledger_two_sided(1_000, 1_101),
        "over-count must fail"
    );
    assert!(
        !gates::ledger_two_sided(1_000, 899),
        "under-count must fail"
    );
    assert!(
        !gates::ledger_two_sided(0, 0),
        "an empty heap is not a pass"
    );
}

#[test]
fn tests_unchanged_fails_without_a_recorded_set() {
    assert!(!gates::tests_unchanged(None, "default a::b\n"));
}

#[test]
fn tests_unchanged_fails_when_a_test_is_added_or_removed() {
    let recorded = listing::listing(&[
        ("default", vec!["a::b".to_string(), "a::c".to_string()]),
        ("embedded", vec!["a::b".to_string()]),
    ]);
    let same = listing::listing(&[
        ("default", vec!["a::b".to_string(), "a::c".to_string()]),
        ("embedded", vec!["a::b".to_string()]),
    ]);
    assert!(gates::tests_unchanged(Some(&recorded), &same));
    let removed = listing::listing(&[
        ("default", vec!["a::b".to_string()]),
        ("embedded", vec!["a::b".to_string()]),
    ]);
    assert!(
        !gates::tests_unchanged(Some(&recorded), &removed),
        "a removed test must fail"
    );
    let added = listing::listing(&[
        (
            "default",
            vec!["a::b".to_string(), "a::c".to_string(), "a::d".to_string()],
        ),
        ("embedded", vec!["a::b".to_string()]),
    ]);
    assert!(
        !gates::tests_unchanged(Some(&recorded), &added),
        "an added test must fail"
    );
}

#[test]
fn test_names_skips_summary_and_duplicate_lines() {
    let output = "a::b: test\na::b: test\nc::d: test\n\n2 tests, 0 benchmarks\n";
    assert_eq!(listing::test_names(output), vec!["a::b", "c::d"]);
    assert!(listing::test_names("2 tests, 0 benchmarks\n").is_empty());
}

#[test]
fn tests_pass_fails_on_a_failed_test_or_exit() {
    assert!(gates::tests_pass(Some(0), counts(5, 0)));
    assert!(!gates::tests_pass(Some(0), counts(5, 1)));
    assert!(!gates::tests_pass(Some(101), counts(5, 0)));
    assert!(
        !gates::tests_pass(Some(0), None),
        "no summary line is not a pass"
    );
}

#[test]
fn expected_tests_ran_fails_when_a_listed_test_did_not_run() {
    assert!(gates::expected_tests_ran(counts(3, 0), 3));
    assert!(
        gates::expected_tests_ran(
            Some(TestCounts {
                passed: 2,
                failed: 0,
                ignored: 1,
            }),
            3
        ),
        "an ignored test is a test that ran"
    );
    assert!(!gates::expected_tests_ran(counts(2, 0), 3));
    assert!(!gates::expected_tests_ran(counts(4, 0), 3));
    assert!(
        !gates::expected_tests_ran(counts(0, 0), 0),
        "nothing listed"
    );
    assert!(!gates::expected_tests_ran(None, 3));
}

#[test]
fn tooling_tests_pass_fails_when_nothing_ran() {
    assert!(gates::tooling_tests_pass(Some(0), counts(3, 0)));
    assert!(!gates::tooling_tests_pass(Some(0), counts(0, 0)));
    assert!(!gates::tooling_tests_pass(Some(1), counts(3, 0)));
}

#[test]
fn eviction_policy_unchanged_fails_without_a_recorded_listing() {
    assert!(!gates::eviction_policy_unchanged(None, "x a\n"));
}

#[test]
fn eviction_policy_unchanged_passes_on_the_recorded_tree() {
    let root = repo_root();
    let recorded = std::fs::read_to_string(root.join(EXPECTED_POLICY)).expect("recorded listing");
    let items = collect_items(&root).expect("policy items locate");
    assert_eq!(items.len(), 23, "decision 2 digests 23 items");
    assert!(gates::eviction_policy_unchanged(
        Some(&recorded),
        &listing_from_items(&items)
    ));
}

/// Mutate one item's tokens and require the frozen listing to reject it.
fn item_change_is_rejected(key: &str) {
    let root = repo_root();
    let recorded = std::fs::read_to_string(root.join(EXPECTED_POLICY)).expect("recorded listing");
    let items = collect_items(&root).expect("policy items locate");
    assert!(
        items.iter().any(|(k, _)| k == key),
        "{key} is not a digested item"
    );
    assert!(gates::eviction_policy_unchanged(
        Some(&recorded),
        &listing_from_items(&items)
    ));
    let mutated: Vec<(String, String)> = items
        .iter()
        .map(|(k, tokens)| {
            if k == key {
                (k.clone(), format!("{tokens} ;"))
            } else {
                (k.clone(), tokens.clone())
            }
        })
        .collect();
    assert!(
        !gates::eviction_policy_unchanged(Some(&recorded), &listing_from_items(&mutated)),
        "a change to {key} must fail eviction_policy_unchanged"
    );
}

macro_rules! item_controls {
    ($( $test:ident => $key:literal ),* $(,)?) => {
        $(
            #[test]
            fn $test() {
                item_change_is_rejected($key);
            }
        )*
    };
}

item_controls! {
    control_store_admit_hydration => "apps/core/src/store.rs::admit_hydration",
    control_store_enforce_cache_budget => "apps/core/src/store.rs::enforce_cache_budget",
    control_store_enforce_cache_budget_now => "apps/core/src/store.rs::enforce_cache_budget_now",
    control_store_evict_lru_until => "apps/core/src/store.rs::evict_lru_until",
    control_store_has_memory_headroom => "apps/core/src/store.rs::has_memory_headroom",
    control_store_try_evict_tenant => "apps/core/src/store.rs::try_evict_tenant",
    control_store_parse_cache_byte_budget => "apps/core/src/store.rs::parse_cache_byte_budget",
    control_loader_over_budget => "apps/core/src/infrastructure/persistence/tenant_loader.rs::over_budget",
    control_loader_pick_lru_excluding => "apps/core/src/infrastructure/persistence/tenant_loader.rs::pick_lru_excluding",
    control_loader_touch => "apps/core/src/infrastructure/persistence/tenant_loader.rs::touch",
    control_loader_mark_loaded_with_integrity => "apps/core/src/infrastructure/persistence/tenant_loader.rs::mark_loaded_with_integrity",
    control_loader_mark_unloaded => "apps/core/src/infrastructure/persistence/tenant_loader.rs::mark_unloaded",
    control_loader_set_byte_budget => "apps/core/src/infrastructure/persistence/tenant_loader.rs::set_byte_budget",
    control_loader_add_bytes => "apps/core/src/infrastructure/persistence/tenant_loader.rs::add_bytes",
    control_loader_total_bytes => "apps/core/src/infrastructure/persistence/tenant_loader.rs::total_bytes",
    control_memory_available_bytes_cfg_one => "apps/core/src/infrastructure/observability/memory.rs::available_bytes#1",
    control_memory_available_bytes_cfg_two => "apps/core/src/infrastructure/observability/memory.rs::available_bytes#2",
    control_memory_has_headroom => "apps/core/src/infrastructure/observability/memory.rs::has_headroom",
    control_hydration_default_reserve => "apps/core/src/infrastructure/persistence/hydration_admission.rs::DEFAULT_MEMORY_RESERVE_BYTES",
    control_hydration_reserve_from_env => "apps/core/src/infrastructure/persistence/hydration_admission.rs::reserve_from_env",
    control_event_estimated_size_bytes => "apps/core/src/domain/entities/event.rs::estimated_size_bytes",
    control_read_model_evict_model => "apps/core/src/store_read_model.rs::evict_model",
    control_read_model_bound_cache => "apps/core/src/store_read_model.rs::bound_cache",
}
