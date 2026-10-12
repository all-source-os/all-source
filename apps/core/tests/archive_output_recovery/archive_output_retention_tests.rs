use super::{
    CHILD_DIRECTORY, CHILD_MODE, FILE_LIMIT, TENANT, assert_same_event, bounded_files, fingerprint,
    fixture_events, run_with_deadline, seed_legacy_snapshot,
};
use allsource_core::infrastructure::persistence::{
    CompactionConfig, CompactionManager, CompactionStrategy, LocalFsArchive, ParquetStorage,
    archive_repack::read_repack_receipt, compaction::RetentionConfig,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

const TEST_NAME: &str =
    "retention_tests::oversized_raw_repair_keeps_full_compaction_ttl_and_cold_archive";

fn retention_flow(directory: &Path) {
    let ordinary = directory.join("fixture");
    let live = directory.join("live");
    let cold = directory.join("cold");
    let mut expected = fixture_events();
    for (index, event) in expected.iter_mut().enumerate() {
        event.timestamp = "2000-01-01T00:00:00Z".parse().expect("expired timestamp");
        event.timestamp += chrono::TimeDelta::microseconds(index as i64);
    }
    let fixture = ParquetStorage::new(&ordinary).expect("fixture storage");
    for event in &expected {
        fixture.append_event(event.clone()).expect("fixture event");
    }
    fixture.flush().expect("fixture archive parts");
    drop(fixture);
    let snapshot = seed_legacy_snapshot(&live, &bounded_files(&ordinary));
    let source = snapshot.with_file_name("events.synthetic.legacy.parquet");
    fs::rename(snapshot, &source).expect("oversized raw source");
    let metadata = fs::metadata(&source).expect("raw archive metadata");
    let original_bytes = metadata.len();
    let original_created_at = metadata.created().ok().or_else(|| metadata.modified().ok());
    assert!(original_bytes > FILE_LIMIT);
    let original_hash = fingerprint(&source);
    let archive = Arc::new(LocalFsArchive::new(&cold).expect("cold archive target"));
    let mut retention = RetentionConfig::default();
    retention.set(TENANT, Some(Duration::from_hours(168)));
    let manager = CompactionManager::new(
        &live,
        CompactionConfig {
            min_files_to_compact: 2,
            small_file_threshold: 10 * 1024 * 1024,
            max_input_bytes_per_snapshot: 20 * 1024 * 1024,
            strategy: CompactionStrategy::FullCompaction,
            retention,
            archive: Some(archive),
            auto_compact: false,
            ..Default::default()
        },
    );

    manager
        .compact_tenant(TENANT)
        .expect("raw repair and retention archive");
    assert!(!source.exists());
    let storage = ParquetStorage::new(&live).expect("retained live namespace");
    assert!(
        storage
            .list_parquet_files_for_tenant(TENANT)
            .expect("live archive files")
            .is_empty()
    );
    assert!(
        storage
            .load_events_for_tenant(TENANT)
            .expect("live retained events")
            .is_empty()
    );
    let archived = ParquetStorage::new(&cold).expect("cold archive reader");
    let cold_files = archived
        .list_parquet_files_for_tenant(TENANT)
        .expect("cold archive files");
    assert!(!cold_files.is_empty());
    assert!(
        cold_files
            .iter()
            .all(|path| fs::metadata(path).expect("cold part metadata").len() <= FILE_LIMIT)
    );
    let originals = expected
        .iter()
        .map(|event| (event.id, event))
        .collect::<BTreeMap<_, _>>();
    let mut seen = BTreeSet::new();
    for part in cold_files {
        for event in archived
            .load_events_from_file_path(&part, TENANT)
            .expect("cold event rows")
        {
            assert!(seen.insert(event.id), "duplicate cold event {}", event.id);
            assert_same_event(
                &event,
                originals.get(&event.id).expect("original cold event ID"),
            );
        }
    }
    assert_eq!(
        seen.len(),
        expected.len(),
        "cold archive must retain every expired event"
    );
    let partition = source.parent().expect("source partition");
    let receipts = fs::read_dir(partition)
        .expect("repair directory")
        .map(|entry| entry.expect("repair entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect::<Vec<_>>();
    assert_eq!(receipts.len(), 1);
    let typed_receipt = read_repack_receipt(&receipts[0]).expect("typed repair receipt");
    assert_eq!(typed_receipt.original_created_at, original_created_at);
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipts[0]).expect("repair receipt"))
            .expect("receipt JSON");
    assert_eq!(receipt["rows"].as_u64(), Some(expected.len() as u64));
    assert_eq!(receipt["original_bytes"].as_u64(), Some(original_bytes));
    let backup = partition.join(receipt["backup"].as_str().expect("retained backup name"));
    assert_eq!(fingerprint(&backup), original_hash);
    let outputs = receipt["outputs"]
        .as_array()
        .expect("repair replacement names");
    assert!(outputs.len() > 1);
    let output_bytes = receipt["output_bytes"]
        .as_u64()
        .expect("replacement byte count");
    assert!(
        output_bytes / outputs.len() as u64 >= 10 * 1024 * 1024,
        "fixture must include a repaired part above the snapshot skip threshold"
    );
    for output in outputs {
        let name = output.as_str().expect("repair output name");
        assert!(name.starts_with("events.repacked."));
        assert!(
            !partition.join(name).exists(),
            "expired raw replacement must leave live storage"
        );
    }
}

#[test]
fn oversized_raw_repair_keeps_full_compaction_ttl_and_cold_archive() {
    if std::env::var_os(CHILD_MODE).is_some_and(|value| value == "1") {
        retention_flow(&PathBuf::from(
            std::env::var_os(CHILD_DIRECTORY).expect("owned child fixture path"),
        ));
    } else {
        run_with_deadline(TEST_NAME);
    }
}

#[path = "archive_output_age_tests.rs"]
mod age_tests;
