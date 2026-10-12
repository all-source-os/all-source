use allsource_core::{
    domain::entities::Event,
    error::AllSourceError,
    infrastructure::persistence::{
        CompactionConfig, CompactionManager, ParquetStorage, ParquetStorageConfig,
    },
    store::{EventStore, EventStoreConfig, ReadScope},
};
use arrow::compute::concat_batches;
use parquet::{
    arrow::{ArrowWriter, arrow_reader::ParquetRecordBatchReaderBuilder},
    file::properties::WriterProperties,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use tempfile::TempDir;
use uuid::Uuid;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

const TENANT: &str = "synthetic-output-recovery";
const FILE_LIMIT: u64 = 32 * 1024 * 1024;
const TEST_NAME: &str = "bounded_flush_and_legacy_recovery_preserve_strict_reads_and_cas";
const FAILED_REPAIR_TEST: &str = "failed_oversized_repair_still_compacts_healthy_raw_files";
const CHILD_MODE: &str = "ALLSOURCE_ARCHIVE_OUTPUT_RECOVERY_CHILD";
const CHILD_DIRECTORY: &str = "ALLSOURCE_ARCHIVE_OUTPUT_RECOVERY_DIRECTORY";

struct OwnedChild(Child);

impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn run_with_deadline(test_name: &str) {
    let directory = TempDir::new().expect("owned recovery fixture");
    let mut child = OwnedChild(
        Command::new(std::env::current_exe().expect("test executable"))
            .args(["--exact", test_name, "--nocapture"])
            .env(CHILD_MODE, "1")
            .env(CHILD_DIRECTORY, directory.path())
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("owned recovery test process"),
    );
    let started = Instant::now();
    let mut warned = false;
    loop {
        if let Some(status) = child.0.try_wait().expect("recovery process status") {
            assert!(status.success(), "{test_name} failed: {status}");
            return;
        }
        assert!(
            started.elapsed() < Duration::from_secs(120),
            "{test_name} exceeded 120-second test budget"
        );
        if !warned && started.elapsed() >= Duration::from_secs(60) {
            eprintln!("{test_name} passed 60-second slow-test threshold");
            warned = true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn fixture_events() -> Vec<Event> {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    (0..80)
        .map(|index| {
            let mut state = (index as u64 + 1).wrapping_mul(0x9e37_79b9_7f4a_7c15);
            let body = (0..512 * 1024)
                .map(|_| {
                    state ^= state << 13;
                    state ^= state >> 7;
                    state ^= state << 17;
                    ALPHABET[(state & 63) as usize]
                })
                .collect::<Vec<_>>();
            let mut event = Event::from_strings(
                "synthetic.updated".into(),
                format!("synthetic-entity-{}", index / 2),
                TENANT.into(),
                json!({"index": index, "body": String::from_utf8(body).expect("ASCII payload")}),
                Some(json!({"synthetic": true, "ordinal": index})),
            )
            .expect("synthetic event");
            event.id = Uuid::from_u128(index as u128 + 1);
            event.version = 10 + index % 2;
            event.timestamp = "2026-01-01T00:00:00.000000Z"
                .parse()
                .expect("fixture timestamp");
            event.timestamp += chrono::TimeDelta::microseconds(index);
            event
        })
        .collect()
}

fn assert_same_event(actual: &Event, expected: &Event) {
    assert_eq!(actual.id, expected.id);
    assert_eq!(actual.event_type, expected.event_type);
    assert_eq!(actual.entity_id, expected.entity_id);
    assert_eq!(actual.tenant_id_str(), expected.tenant_id_str());
    assert_eq!(actual.version, expected.version);
    assert_eq!(
        actual.timestamp.timestamp_micros(),
        expected.timestamp.timestamp_micros()
    );
    assert_eq!(actual.metadata, expected.metadata);
    assert!(
        actual.payload == expected.payload,
        "payload differs for event {}",
        expected.id
    );
}

fn bounded_files(root: &Path) -> Vec<PathBuf> {
    let files = ParquetStorage::new(root)
        .expect("archive reader")
        .list_parquet_files_for_tenant(TENANT)
        .expect("tenant archive files");
    assert!(
        files.len() > 1,
        "large flush must publish multiple bounded parts"
    );
    assert!(
        files
            .iter()
            .all(|path| fs::metadata(path).expect("finalized file size").len() <= FILE_LIMIT)
    );
    files
}

fn assert_archived_events(root: &Path, expected: &[Event]) {
    let storage = ParquetStorage::new(root).expect("archive reader");
    let by_id = expected
        .iter()
        .map(|event| (event.id, event))
        .collect::<BTreeMap<_, _>>();
    let mut seen = BTreeSet::new();
    for path in bounded_files(root) {
        for event in storage
            .load_events_from_file_path(&path, TENANT)
            .expect("complete archive part")
        {
            assert!(
                seen.insert(event.id),
                "duplicate archived event {}",
                event.id
            );
            assert_same_event(&event, by_id.get(&event.id).expect("original event ID"));
        }
    }
    assert_eq!(seen.len(), expected.len());
}

fn reopened_store(root: &Path) -> EventStore {
    let mut config = EventStoreConfig::with_persistence(root);
    config.compaction_config.auto_compact = false;
    EventStore::with_config(config)
}

fn assert_strict_histories(store: &EventStore, expected: &[Event]) {
    let entities = expected
        .iter()
        .map(Event::entity_id_str)
        .collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    for entity in entities {
        let originals = expected
            .iter()
            .filter(|event| event.entity_id_str() == entity)
            .map(|event| (event.id, event))
            .collect::<BTreeMap<_, _>>();
        let (events, total) = store
            .query_retained_entity(TENANT, entity, 10, &ReadScope::unrestricted())
            .expect("complete strict retained history");
        assert_eq!(total, originals.len());
        assert_eq!(events.len(), originals.len());
        for event in events {
            assert!(seen.insert(event.id));
            assert_same_event(
                &event,
                originals.get(&event.id).expect("retained original ID"),
            );
        }
        let highest = originals
            .values()
            .map(|event| event.version)
            .max()
            .expect("entity history");
        assert_eq!(store.get_entity_version(entity), highest as u64);
    }
    assert_eq!(seen.len(), expected.len());
}

fn fingerprint(path: &Path) -> String {
    let mut file = File::open(path).expect("fingerprint input");
    let mut hash = Sha256::new();
    let mut buffer = vec![0; 64 * 1024].into_boxed_slice();
    loop {
        let count = file.read(&mut buffer).expect("fingerprint bytes");
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    hex::encode(hash.finalize())
}

fn seed_legacy_snapshot(root: &Path, bounded_sources: &[PathBuf]) -> PathBuf {
    let mut batches = Vec::new();
    for path in bounded_sources {
        let reader =
            ParquetRecordBatchReaderBuilder::try_new(File::open(path).expect("real event schema"))
                .expect("source parquet metadata")
                .with_batch_size(256)
                .build()
                .expect("source batch reader");
        for batch in reader {
            batches.push(batch.expect("source batch"));
        }
    }
    let schema = batches[0].schema();
    let combined = concat_batches(&schema, &batches).expect("legacy full snapshot batch");
    drop(batches);
    let partition = root.join(TENANT).join("2026-01");
    fs::create_dir_all(&partition).expect("legacy tenant partition");
    let source = partition.join("snapshot.synthetic.legacy.parquet");
    let properties = WriterProperties::builder()
        .set_compression(ParquetStorageConfig::default().compression)
        .set_statistics_truncate_length(None)
        .build();
    let mut writer = ArrowWriter::try_new(
        File::create(&source).expect("legacy snapshot"),
        schema,
        Some(properties),
    )
    .expect("legacy writer");
    writer.write(&combined).expect("legacy source rows");
    let metadata = writer.close().expect("legacy source footer");
    let decoded = metadata
        .row_groups()
        .iter()
        .map(parquet::file::metadata::RowGroupMetaData::total_byte_size)
        .sum::<i64>();
    assert!(decoded > 0 && decoded <= 128 * 1024 * 1024);
    assert!(fs::metadata(&source).expect("legacy physical size").len() > FILE_LIMIT);
    source
}

fn recovery_flow(directory: &Path) {
    assert!(directory.is_dir());
    assert_eq!(
        fs::read_dir(directory)
            .expect("owned fixture directory")
            .count(),
        0
    );
    let ordinary = directory.join("ordinary");
    let legacy = directory.join("legacy");
    let mut expected = fixture_events();
    let storage = ParquetStorage::new(&ordinary).expect("ordinary storage");
    for event in &expected {
        storage
            .append_event(event.clone())
            .expect("buffer synthetic event");
    }
    storage.flush().expect("ordinary bounded flush");
    drop(storage);
    let ordinary_files = bounded_files(&ordinary);
    assert_archived_events(&ordinary, &expected);
    let restarted = reopened_store(&ordinary);
    assert_strict_histories(&restarted, &expected);
    drop(restarted);

    let source = seed_legacy_snapshot(&legacy, &ordinary_files);
    let source_hash = fingerprint(&source);
    let before_repair = reopened_store(&legacy);
    let refused = before_repair
        .query_retained_entity(TENANT, "synthetic-entity-0", 10, &ReadScope::unrestricted())
        .expect_err("oversized legacy snapshot must refuse strict read");
    assert!(refused.to_string().contains("file bytes"));
    drop(before_repair);

    let manager = CompactionManager::new(
        &legacy,
        CompactionConfig {
            min_files_to_compact: 100,
            small_file_threshold: 1,
            max_pass_seconds: 120,
            auto_compact: false,
            ..Default::default()
        },
    );
    manager
        .compact_tenant(TENANT)
        .expect("normalize oversized singleton snapshot");
    assert!(
        !source.exists(),
        "oversized source must leave current archive namespace"
    );
    assert_archived_events(&legacy, &expected);
    let receipts = fs::read_dir(source.parent().expect("legacy partition"))
        .expect("repair namespace")
        .map(|entry| entry.expect("repair entry").path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect::<Vec<_>>();
    assert_eq!(receipts.len(), 1);
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipts[0]).expect("repair receipt"))
            .expect("receipt JSON");
    assert_eq!(receipt["rows"].as_u64(), Some(expected.len() as u64));
    let backup = source
        .parent()
        .expect("legacy partition")
        .join(receipt["backup"].as_str().expect("backup filename"));
    assert_eq!(fingerprint(&backup), source_hash);

    let restarted = reopened_store(&legacy);
    assert!(!restarted.is_tenant_loaded(TENANT));
    let mut appended = Event::from_strings(
        "synthetic.updated".into(),
        "synthetic-entity-0".into(),
        TENANT.into(),
        json!({"synthetic": true, "stage": "after-repair"}),
        None,
    )
    .expect("conditional append input");
    appended.id = Uuid::from_u128(1_000_000);
    assert_eq!(
        restarted
            .ingest_with_expected_version(&appended, Some(11))
            .expect("cold conditional append"),
        12
    );
    assert_eq!(
        restarted.total_events(),
        1,
        "cold CAS must resolve versions without hydrating payloads"
    );
    assert!(matches!(
        restarted.ingest_with_expected_version(&appended, Some(11)),
        Err(AllSourceError::VersionConflict {
            expected: 11,
            current: 12
        })
    ));
    appended.version = 12;
    expected.push(appended);
    assert_strict_histories(&restarted, &expected);
    restarted.flush_storage().expect("persist conditional tail");
    drop(restarted);
    assert_archived_events(&legacy, &expected);
    let restarted_again = reopened_store(&legacy);
    assert_strict_histories(&restarted_again, &expected);
    assert_eq!(restarted_again.get_entity_version("synthetic-entity-0"), 12);
    assert_eq!(fingerprint(&backup), source_hash);
}

#[test]
fn bounded_flush_and_legacy_recovery_preserve_strict_reads_and_cas() {
    if std::env::var_os(CHILD_MODE).is_some_and(|value| value == "1") {
        recovery_flow(&PathBuf::from(
            std::env::var_os(CHILD_DIRECTORY).expect("owned child fixture path"),
        ));
    } else {
        run_with_deadline(TEST_NAME);
    }
}

fn failed_repair_flow(directory: &Path) {
    let storage = ParquetStorage::new(directory).expect("healthy storage");
    let expected = (0..2)
        .map(|index| {
            let mut event = Event::from_strings(
                "synthetic.updated".into(),
                format!("healthy-{index}"),
                TENANT.into(),
                json!({"synthetic": true, "index": index}),
                None,
            )
            .expect("healthy event");
            event.id = Uuid::from_u128(700 + index);
            event.version = 41 + index as i64;
            event
        })
        .collect::<Vec<_>>();
    let first = storage
        .write_atomic_parquet(TENANT, "events-first", &expected[..1])
        .expect("first healthy raw archive");
    let second = storage
        .write_atomic_parquet(TENANT, "events-second", &expected[1..])
        .expect("second healthy raw archive");
    let corrupt = first
        .parent()
        .expect("tenant partition")
        .join("snapshot.corrupt.parquet");
    fs::write(&corrupt, vec![0xa5; FILE_LIMIT as usize + 1]).expect("oversized corrupt snapshot");
    let corrupt_hash = fingerprint(&corrupt);
    let manager = CompactionManager::new(
        directory,
        CompactionConfig {
            min_files_to_compact: 2,
            small_file_threshold: 10 * 1024 * 1024,
            auto_compact: false,
            ..Default::default()
        },
    );

    manager
        .compact_tenant(TENANT)
        .expect_err("corrupt repair remains reported");
    assert!(
        !first.exists() && !second.exists(),
        "healthy raw pair must compact despite repair failure"
    );
    assert_eq!(
        fs::metadata(&corrupt)
            .expect("retained corrupt snapshot")
            .len(),
        FILE_LIMIT + 1
    );
    assert_eq!(fingerprint(&corrupt), corrupt_hash);
    let completed = storage
        .list_parquet_files_for_tenant(TENANT)
        .expect("active archives")
        .into_iter()
        .filter(|path| path != &corrupt)
        .collect::<Vec<_>>();
    assert_eq!(completed.len(), 1);
    assert!(
        completed[0]
            .file_name()
            .expect("snapshot name")
            .to_string_lossy()
            .starts_with("snapshot.")
    );
    assert!(
        fs::metadata(&completed[0])
            .expect("healthy snapshot metadata")
            .len()
            <= FILE_LIMIT
    );
    let actual = storage
        .load_events_from_file_path(&completed[0], TENANT)
        .expect("healthy compacted rows");
    assert_eq!(actual.len(), expected.len());
    let by_id = actual
        .iter()
        .map(|event| (event.id, event))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(by_id.len(), expected.len());
    for event in &expected {
        assert_same_event(by_id.get(&event.id).expect("healthy event retained"), event);
    }
}

#[test]
fn failed_oversized_repair_still_compacts_healthy_raw_files() {
    if std::env::var_os(CHILD_MODE).is_some_and(|value| value == "1") {
        failed_repair_flow(&PathBuf::from(
            std::env::var_os(CHILD_DIRECTORY).expect("owned child fixture path"),
        ));
    } else {
        run_with_deadline(FAILED_REPAIR_TEST);
    }
}

#[path = "archive_output_recovery/archive_output_retention_tests.rs"]
mod retention_tests;
