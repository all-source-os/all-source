use super::{
    CHILD_DIRECTORY, CHILD_MODE, TENANT, assert_same_event, fingerprint, run_with_deadline,
};
use allsource_core::{
    domain::entities::Event,
    infrastructure::persistence::{
        CompactionConfig, CompactionManager, CompactionStrategy, ParquetStorage,
    },
};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use uuid::Uuid;

const TEST_NAME: &str = "retention_tests::age_tests::invalid_repair_age_receipts_do_not_block_unrelated_healthy_compaction";

fn event(index: u128) -> Event {
    let mut event = Event::from_strings(
        "synthetic.updated".into(),
        format!("healthy-{index}"),
        TENANT.into(),
        serde_json::json!({"synthetic": true, "index": index}),
        None,
    )
    .expect("synthetic event");
    event.id = Uuid::from_u128(index);
    event.version = index as i64;
    event
}

fn invalid_age_flow(directory: &Path) {
    for corrupted in [false, true] {
        let case = directory.join(if corrupted { "corrupt" } else { "missing" });
        let storage = ParquetStorage::new(&case).expect("case storage");
        let expected = [event(701), event(702)];
        let first = storage
            .write_atomic_parquet(TENANT, "events-first", &expected[..1])
            .expect("first healthy raw archive");
        let second = storage
            .write_atomic_parquet(TENANT, "events-second", &expected[1..])
            .expect("second healthy raw archive");
        let repair_id = Uuid::new_v4().as_simple().to_string();
        let stem = format!(
            "events.repacked.{repair_id}.{}.part-1",
            Uuid::new_v4().as_simple()
        );
        let repaired = storage
            .write_atomic_parquet(TENANT, &stem, &[event(900)])
            .expect("repaired output awaiting age evidence");
        let repaired_hash = fingerprint(&repaired);
        let receipt = repaired
            .parent()
            .expect("repair partition")
            .join(format!("events.original.parquet.repair.{repair_id}.json"));
        let invalid_bytes = b"{broken receipt";
        if corrupted {
            fs::write(&receipt, invalid_bytes).expect("corrupted age receipt");
        }
        let manager = CompactionManager::new(
            &case,
            CompactionConfig {
                min_files_to_compact: 2,
                strategy: CompactionStrategy::FullCompaction,
                auto_compact: false,
                ..Default::default()
            },
        );

        let error = manager
            .compact_tenant(TENANT)
            .expect_err("invalid age evidence must remain reported");
        if !corrupted {
            assert!(
                error
                    .to_string()
                    .contains("Repaired archive receipt missing")
            );
        }
        assert!(
            !first.exists() && !second.exists(),
            "healthy raw pair must compact independently"
        );
        assert_eq!(fingerprint(&repaired), repaired_hash);
        if corrupted {
            assert_eq!(
                fs::read(&receipt).expect("retained corrupted receipt"),
                invalid_bytes
            );
        } else {
            assert!(!receipt.exists());
        }
        let completed = storage
            .list_parquet_files_for_tenant(TENANT)
            .expect("active archives")
            .into_iter()
            .filter(|path| path != &repaired)
            .collect::<Vec<_>>();
        assert_eq!(completed.len(), 1);
        let actual = storage
            .load_events_from_file_path(&completed[0], TENANT)
            .expect("healthy compacted events");
        let by_id = actual
            .iter()
            .map(|event| (event.id, event))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(actual.len(), expected.len());
        assert_eq!(by_id.len(), expected.len());
        for event in &expected {
            assert_same_event(by_id.get(&event.id).expect("retained healthy event"), event);
        }
    }
}

#[test]
fn invalid_repair_age_receipts_do_not_block_unrelated_healthy_compaction() {
    if std::env::var_os(CHILD_MODE).is_some_and(|value| value == "1") {
        invalid_age_flow(&PathBuf::from(
            std::env::var_os(CHILD_DIRECTORY).expect("owned child fixture path"),
        ));
    } else {
        run_with_deadline(TEST_NAME);
    }
}
