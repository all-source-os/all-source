//! Compaction must bound its own memory and actually reduce the file count.
//!
//! Production ran zero compactions for the life of the feature, so a tenant
//! reached 240,602 Parquet files and the configured 30-day retention never
//! applied. Scheduling the sweep is only safe if one pass cannot read a whole
//! tenant into memory at once (gh#321).

use allsource_core::{
    domain::entities::Event,
    infrastructure::persistence::{
        ParquetStorage, ParquetStorageConfig,
        compaction::{CompactionConfig, CompactionManager},
    },
};
use std::{collections::BTreeSet, path::Path, time::Duration};
use tempfile::TempDir;

/// One event per file, which is the production shape: the affected tenant
/// held 20,851 files averaging 7.5 KB.
fn seed_files(root: &Path, tenant: &str, count: usize) {
    let storage = ParquetStorage::with_config(
        root,
        ParquetStorageConfig {
            batch_size: 1,
            ..Default::default()
        },
    )
    .unwrap();
    for index in 0..count {
        storage
            .append_event(
                Event::from_strings(
                    "synthetic.updated".into(),
                    format!("entity-{}", index % 4),
                    tenant.into(),
                    serde_json::json!({ "index": index }),
                    None,
                )
                .unwrap(),
            )
            .unwrap();
        storage.flush().unwrap();
    }
}

fn file_count(root: &Path, tenant: &str) -> usize {
    ParquetStorage::new(root)
        .unwrap()
        .list_parquet_files_for_tenant(tenant)
        .unwrap()
        .len()
}

/// Every `index` still readable under the tenant. Unreadable files are
/// skipped so a fixture can plant one without this helper being the thing
/// that fails.
fn all_payload_indices(root: &Path, tenant: &str) -> BTreeSet<i64> {
    let storage = ParquetStorage::new(root).unwrap();
    storage
        .list_parquet_files_for_tenant(tenant)
        .unwrap()
        .into_iter()
        .filter_map(|path| storage.load_events_from_file_path(&path, tenant).ok())
        .flatten()
        .map(|event| event.payload["index"].as_i64().unwrap())
        .collect()
}

#[test]
fn a_tenant_larger_than_the_input_bound_is_compacted_in_several_groups() {
    let directory = TempDir::new().unwrap();
    let tenant = "synthetic-grouped";
    seed_files(directory.path(), tenant, 12);
    let before = all_payload_indices(directory.path(), tenant);
    assert_eq!(before.len(), 12);

    // A bound far below one file's size forces one group per file, which is
    // the degenerate case the grouping has to survive.
    let manager = CompactionManager::new(
        directory.path(),
        CompactionConfig {
            max_input_bytes_per_snapshot: 1,
            ..CompactionConfig::default()
        },
    );
    manager.compact_tenant(tenant).unwrap();

    assert_eq!(
        all_payload_indices(directory.path(), tenant),
        before,
        "grouped compaction lost or duplicated events"
    );
}

#[test]
fn a_whole_tenant_inside_the_bound_becomes_one_file() {
    let directory = TempDir::new().unwrap();
    let tenant = "synthetic-single-group";
    seed_files(directory.path(), tenant, 9);
    let before = all_payload_indices(directory.path(), tenant);

    let manager = CompactionManager::new(directory.path(), CompactionConfig::default());
    manager.compact_tenant(tenant).unwrap();

    assert_eq!(
        file_count(directory.path(), tenant),
        1,
        "nine small files inside the input bound must collapse to one snapshot"
    );
    assert_eq!(all_payload_indices(directory.path(), tenant), before);
}

#[test]
fn successive_passes_merge_their_own_snapshots_instead_of_accumulating() {
    // The regression this guards: excluding every `snapshot.*` from the
    // candidate set left one snapshot per pass behind. The affected tenant
    // takes ~7 new files an hour, so hourly passes would have traded 160
    // files a day for 24 — still unbounded.
    let directory = TempDir::new().unwrap();
    let tenant = "synthetic-merging";
    let mut expected = BTreeSet::new();

    for pass in 0..3 {
        seed_files(directory.path(), tenant, 3);
        expected.extend(all_payload_indices(directory.path(), tenant));
        let manager = CompactionManager::new(directory.path(), CompactionConfig::default());
        manager.compact_tenant(tenant).unwrap();
        assert_eq!(
            file_count(directory.path(), tenant),
            1,
            "pass {pass} left more than one file behind"
        );
    }

    assert_eq!(
        all_payload_indices(directory.path(), tenant),
        expected,
        "merging snapshots lost events"
    );
}

#[test]
fn an_expired_tenant_loses_its_events_only_once_the_sweep_runs() {
    let directory = TempDir::new().unwrap();
    let tenant = "synthetic-expiring";
    seed_files(directory.path(), tenant, 6);

    let mut config = CompactionConfig::default();
    config.retention.set(tenant, Some(Duration::ZERO));
    let manager = CompactionManager::new(directory.path(), config);

    assert_eq!(file_count(directory.path(), tenant), 6);
    manager.compact_tenant(tenant).unwrap();
    assert_eq!(
        file_count(directory.path(), tenant),
        0,
        "a zero TTL must drop every event, and retention runs only inside the sweep"
    );
}

#[test]
fn a_spent_time_budget_stops_the_sweep_without_losing_events() {
    let directory = TempDir::new().unwrap();
    let tenant = "synthetic-budgeted";
    seed_files(directory.path(), tenant, 8);
    let before = all_payload_indices(directory.path(), tenant);

    // A deadline already in the past: no group may run, and nothing may be
    // deleted on the way out.
    let manager = CompactionManager::new(
        directory.path(),
        CompactionConfig {
            max_input_bytes_per_snapshot: 1,
            ..CompactionConfig::default()
        },
    );
    let result = manager
        .compact_tenant_until(tenant, Some(std::time::Instant::now()))
        .unwrap();

    assert_eq!(result.files_compacted, 0);
    assert_eq!(file_count(directory.path(), tenant), 8);
    assert_eq!(all_payload_indices(directory.path(), tenant), before);
}

#[test]
fn a_sweep_reports_the_budget_it_was_given() {
    // `should_compact` is the gate the scheduler reads; a manager that has
    // never compacted must say yes, or the loop never starts.
    let directory = TempDir::new().unwrap();
    let manager = CompactionManager::new(directory.path(), CompactionConfig::default());
    assert!(
        manager.should_compact(),
        "a manager that has never compacted must admit the first sweep"
    );
    manager.compact().unwrap();
    assert!(
        !manager.should_compact(),
        "a sweep that just ran must not be repeated until its interval elapses"
    );
}

#[test]
fn zero_retention_days_keeps_history_rather_than_deleting_all_of_it() {
    // The literal reading of "0 days" expires every event. An operator
    // turning the number down is not asking for that, and with no cold-tier
    // archive configured it is unrecoverable, so 0 means keep forever.
    let config = CompactionConfig::from_env_vars(None, Some("0".to_string()));
    assert_eq!(config.retention.ttl_for("system"), None);

    let directory = TempDir::new().unwrap();
    seed_files(directory.path(), "system", 4);
    let manager = CompactionManager::new(directory.path(), config);
    manager.compact_tenant("system").unwrap();

    assert_eq!(
        all_payload_indices(directory.path(), "system").len(),
        4,
        "zero retention days deleted history"
    );
}

#[test]
fn an_unset_retention_variable_still_applies_the_thirty_day_default() {
    // Unsetting does not disable: the default lives in the code, so the
    // only way to turn it off through the environment is the explicit 0.
    let config = CompactionConfig::from_env_vars(None, None);
    assert_eq!(
        config.retention.ttl_for("system"),
        Some(Duration::from_hours(30 * 24))
    );
}

#[test]
fn a_sweep_that_runs_out_of_budget_resumes_at_the_tenant_it_did_not_reach() {
    // Starvation is the failure this guards. A sweep has a time budget and
    // visits tenants in a stable order, so a sweep that always restarts at
    // the first tenant never reaches the last while there is a backlog — and
    // the tenant with 240,602 files sorts near the end.
    let directory = TempDir::new().unwrap();
    for tenant in ["aaa-first", "mmm-middle", "zzz-last"] {
        seed_files(directory.path(), tenant, 4);
    }
    let manager = CompactionManager::new(directory.path(), CompactionConfig::default());

    // A deadline already past: nothing is visited, so the next sweep must
    // still begin at the first tenant rather than skipping it.
    manager
        .compact_until(Some(std::time::Instant::now()))
        .unwrap();
    assert_eq!(manager.sweep_cursor().as_deref(), Some("aaa-first"));
    assert_eq!(file_count(directory.path(), "aaa-first"), 4);

    // A sweep with room finishes every tenant and parks the cursor back at
    // the start, so a steady state keeps one stable order.
    manager.compact().unwrap();
    assert_eq!(manager.sweep_cursor().as_deref(), Some("aaa-first"));
    for tenant in ["aaa-first", "mmm-middle", "zzz-last"] {
        assert_eq!(
            file_count(directory.path(), tenant),
            1,
            "{tenant} was not compacted"
        );
    }
}

#[test]
fn a_cursor_naming_a_vanished_tenant_still_sweeps_everyone() {
    let directory = TempDir::new().unwrap();
    for tenant in ["aaa-first", "zzz-last"] {
        seed_files(directory.path(), tenant, 4);
    }
    let manager = CompactionManager::new(directory.path(), CompactionConfig::default());
    manager
        .compact_until(Some(std::time::Instant::now()))
        .unwrap();

    // Remove the tenant the cursor points at, as a retention pass that aged
    // out an entire tenant would.
    std::fs::remove_dir_all(directory.path().join("aaa-first")).unwrap();
    manager.compact().unwrap();

    assert_eq!(file_count(directory.path(), "zzz-last"), 1);
}

#[test]
fn one_unreadable_file_does_not_block_the_rest_of_the_tenant() {
    let directory = TempDir::new().unwrap();
    let tenant = "synthetic-partial";
    seed_files(directory.path(), tenant, 8);
    std::fs::write(
        directory.path().join(tenant).join("events-bad.parquet"),
        "not a parquet file",
    )
    .unwrap();

    // One group per file, so the unreadable one is isolated in its own.
    let manager = CompactionManager::new(
        directory.path(),
        CompactionConfig {
            max_input_bytes_per_snapshot: 1,
            ..CompactionConfig::default()
        },
    );
    let result = manager.compact_tenant(tenant);

    assert!(
        result.is_err(),
        "a sweep that could not read a candidate must not report success"
    );
    assert!(
        std::fs::metadata(directory.path().join(tenant).join("events-bad.parquet")).is_ok(),
        "the unreadable candidate must still be on disk"
    );
    assert_eq!(
        all_payload_indices(directory.path(), tenant).len(),
        8,
        "the groups that were readable must have kept every event"
    );
}
