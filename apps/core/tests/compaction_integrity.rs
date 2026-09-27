use allsource_core::{
    domain::entities::Event,
    infrastructure::persistence::{
        ParquetStorage, ParquetStorageConfig,
        compaction::{CompactionConfig, CompactionManager},
    },
};
use std::{collections::BTreeMap, path::Path, time::Duration};
use tempfile::TempDir;

fn seed(root: &Path, tenant: &str) {
    let storage = ParquetStorage::with_config(
        root,
        ParquetStorageConfig {
            batch_size: 1,
            ..Default::default()
        },
    )
    .unwrap();
    for _ in 0..2 {
        storage
            .append_event(
                Event::from_strings(
                    "synthetic.updated".into(),
                    "run".into(),
                    tenant.into(),
                    serde_json::json!({}),
                    None,
                )
                .unwrap(),
            )
            .unwrap();
        storage.flush().unwrap();
    }
}

fn retained_files(root: &Path, tenant: &str) -> BTreeMap<std::path::PathBuf, Vec<u8>> {
    ParquetStorage::new(root)
        .unwrap()
        .list_parquet_files_for_tenant(tenant)
        .unwrap()
        .into_iter()
        .map(|path| {
            let bytes = std::fs::read(&path).unwrap();
            (path, bytes)
        })
        .collect()
}

fn refuses_unreadable_candidate(expire: bool) {
    let directory = TempDir::new().unwrap();
    let tenant = "synthetic-compaction";
    seed(directory.path(), tenant);
    let corrupt = directory
        .path()
        .join(tenant)
        .join("events-unreadable.parquet");
    std::fs::write(&corrupt, "synthetic retained history with unknown contents").unwrap();
    let before = retained_files(directory.path(), tenant);
    let mut config = CompactionConfig::default();
    if expire {
        config.retention.set(tenant, Some(Duration::ZERO));
    }
    let manager = CompactionManager::new(directory.path(), config);
    let result = manager.compact_tenant(tenant);
    assert_eq!(
        retained_files(directory.path(), tenant),
        before,
        "failed candidate must leave every original byte and filename intact, with no snapshot"
    );
    assert!(result.is_err(), "unreadable candidate accepted: {result:?}");
}

#[test]
fn unreadable_candidate_cannot_be_removed_after_partial_compaction() {
    refuses_unreadable_candidate(false);
}

#[test]
fn unreadable_candidate_cannot_be_treated_as_fully_expired_history() {
    refuses_unreadable_candidate(true);
}

#[test]
fn failed_tenant_preserves_inputs_while_healthy_tenant_compacts() {
    let directory = TempDir::new().unwrap();
    let bad = "synthetic-bad";
    let healthy = "synthetic-healthy";
    seed(directory.path(), bad);
    seed(directory.path(), healthy);
    std::fs::write(
        directory.path().join(bad).join("events-unreadable.parquet"),
        "synthetic unreadable",
    )
    .unwrap();
    let before = retained_files(directory.path(), bad);
    let manager = CompactionManager::new(
        directory.path(),
        CompactionConfig {
            min_files_to_compact: 2,
            ..Default::default()
        },
    );
    let result = manager.compact().unwrap();
    assert_eq!(retained_files(directory.path(), bad), before);
    assert_eq!(result.files_compacted, 2);
    assert_eq!(result.events_compacted, 2);
    assert_eq!(retained_files(directory.path(), healthy).len(), 1);
}
