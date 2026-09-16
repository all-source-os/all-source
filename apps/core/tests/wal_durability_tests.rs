//! A store opened by a newer binary reads every byte the older binary wrote
//! (#287). The WAL deletes a segment only when every line in it is durable
//! elsewhere; each test below pins one way that used to fail.

use allsource_core::{
    QueryEventsRequest,
    domain::entities::Event,
    infrastructure::persistence::{
        compaction::CompactionConfig,
        snapshot::SnapshotConfig,
        wal::{QUARANTINE_DIR, WALConfig, WriteAheadLog},
    },
    store::{EventStore, EventStoreConfig},
};
use std::{
    collections::HashSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use tempfile::TempDir;

/// Written by allsource-core 0.19 (chronis, March 2026). The checksum covers
/// only `sequence`, `wal_timestamp` and `id`, so the entity and payload here
/// are synthetic while the checksum is the one that release computed.
const LEGACY_0_19_LINE: &str = r#"{"sequence":1,"wal_timestamp":"2026-03-07T16:52:51.859568Z","event":{"id":"a4e1f320-4716-4542-9313-4d72e12291c4","event_type":"task.created","entity_id":"task:legacy-1","tenant_id":"default","payload":{"title":"written by 0.19"},"timestamp":"2026-03-07T16:52:51.859370Z","metadata":null,"version":1},"checksum":1822986689}"#;

const UNREADABLE_LINE: &str =
    r#"{"sequence":2,"format":"from a release this binary does not know"}"#;

fn event(entity: &str) -> Event {
    Event::from_strings(
        "thing.happened".to_string(),
        entity.to_string(),
        "default".to_string(),
        serde_json::json!({ "entity": entity }),
        None,
    )
    .expect("event")
}

fn wal_segments(dir: &Path) -> Vec<PathBuf> {
    let mut segments: Vec<PathBuf> = fs::read_dir(dir)
        .expect("read wal dir")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "log"))
        .collect();
    segments.sort();
    segments
}

fn quarantined_bytes(wal_dir: &Path) -> String {
    let dir = wal_dir.join(QUARANTINE_DIR);
    if !dir.is_dir() {
        return String::new();
    }
    fs::read_dir(dir)
        .expect("read quarantine")
        .flatten()
        .map(|e| fs::read_to_string(e.path()).expect("read quarantined segment"))
        .collect()
}

fn write_segment(wal_dir: &Path, lines: &[&str]) {
    fs::create_dir_all(wal_dir).expect("wal dir");
    let mut file = fs::File::create(wal_dir.join("wal-0000000000000000.log")).expect("segment");
    for line in lines {
        writeln!(file, "{line}").expect("write line");
    }
}

#[test]
fn a_line_this_binary_cannot_read_survives_truncation() {
    let tmp = TempDir::new().unwrap();
    write_segment(tmp.path(), &[LEGACY_0_19_LINE, UNREADABLE_LINE]);

    let wal = WriteAheadLog::new(tmp.path(), WALConfig::default()).unwrap();
    assert_eq!(wal.recover().unwrap().len(), 1);
    wal.truncate().unwrap();

    assert!(
        quarantined_bytes(tmp.path()).contains(UNREADABLE_LINE),
        "an unreadable line must be kept, not deleted"
    );
}

#[test]
fn a_fully_readable_segment_is_deleted_not_quarantined() {
    let tmp = TempDir::new().unwrap();
    let wal = WriteAheadLog::new(tmp.path(), WALConfig::default()).unwrap();
    wal.append(event("e-1")).unwrap();
    wal.recover().unwrap();

    wal.truncate().unwrap();

    assert!(quarantined_bytes(tmp.path()).is_empty());
    assert!(wal.recover().unwrap().is_empty());
}

#[test]
fn rotation_never_deletes_a_segment() {
    let tmp = TempDir::new().unwrap();
    let config = WALConfig {
        max_file_size: 512,
        max_wal_files: 2,
        ..WALConfig::default()
    };
    let wal = WriteAheadLog::new(tmp.path(), config).unwrap();
    for i in 0..60 {
        wal.append(event(&format!("e-{i}"))).unwrap();
    }
    wal.flush().unwrap();

    assert!(wal.stats().files_rotated > 2, "test must rotate");
    assert_eq!(
        wal.recover().unwrap().len(),
        60,
        "segments past max_wal_files may hold unflushed events"
    );
}

#[test]
fn a_torn_final_line_does_not_swallow_the_next_entry() {
    let tmp = TempDir::new().unwrap();
    {
        let wal = WriteAheadLog::new(tmp.path(), WALConfig::default()).unwrap();
        wal.append(event("before-crash")).unwrap();
        wal.flush().unwrap();
    }
    let segment = tmp.path().join("wal-0000000000000000.log");
    let mut file = fs::OpenOptions::new().append(true).open(&segment).unwrap();
    file.write_all(br#"{"sequence":2,"wal_times"#).unwrap();
    drop(file);

    let wal = WriteAheadLog::new(tmp.path(), WALConfig::default()).unwrap();
    wal.append(event("after-restart")).unwrap();
    wal.flush().unwrap();

    let entities: Vec<String> = wal
        .recover()
        .unwrap()
        .iter()
        .map(|e| e.entity_id_str().to_string())
        .collect();
    assert_eq!(entities, ["before-crash", "after-restart"]);
}

#[test]
fn entries_appended_after_a_seal_survive_removal_of_sealed_segments() {
    let tmp = TempDir::new().unwrap();
    let wal = WriteAheadLog::new(tmp.path(), WALConfig::default()).unwrap();
    for i in 0..3 {
        wal.append(event(&format!("sealed-{i}"))).unwrap();
    }

    let active = wal.seal().unwrap();
    wal.append(event("after-seal")).unwrap();
    wal.remove_sealed(&active).unwrap();
    wal.flush().unwrap();

    let entities: Vec<String> = wal
        .recover()
        .unwrap()
        .iter()
        .map(|e| e.entity_id_str().to_string())
        .collect();
    assert_eq!(entities, ["after-seal"]);
}

fn production_config(data_dir: &Path) -> EventStoreConfig {
    EventStoreConfig::production(
        data_dir.join("storage"),
        data_dir.join("wal"),
        SnapshotConfig::default(),
        WALConfig {
            sync_on_write: false,
            ..WALConfig::default()
        },
        CompactionConfig::default(),
    )
}

fn distinct_ids_after_reopen(data_dir: &Path) -> HashSet<uuid::Uuid> {
    let store = EventStore::with_config(production_config(data_dir));
    store.hydrate_all_from_storage().expect("hydrate");
    store
        .query(&QueryEventsRequest {
            tenant_id: Some("default".to_string()),
            limit: Some(100_000),
            ..Default::default()
        })
        .expect("query")
        .iter()
        .map(|e| e.id)
        .collect()
}

#[test]
fn a_legacy_wal_is_fully_read_and_reaches_parquet_before_it_is_retired() {
    let tmp = TempDir::new().unwrap();
    write_segment(
        &tmp.path().join("wal"),
        &[LEGACY_0_19_LINE, UNREADABLE_LINE],
    );

    let first_boot = distinct_ids_after_reopen(tmp.path());
    assert_eq!(first_boot.len(), 1, "the 0.19 entry must be read");

    // Boot checkpointed and retired the legacy segment, so a second boot can
    // only see the event if it reached Parquet first.
    let second_boot = distinct_ids_after_reopen(tmp.path());
    assert_eq!(second_boot, first_boot);

    assert!(quarantined_bytes(&tmp.path().join("wal")).contains(UNREADABLE_LINE));
}

#[test]
fn the_wal_and_storage_paths_derive_from_the_data_dir() {
    let tmp = TempDir::new().unwrap();
    let store = EventStore::with_config(production_config(tmp.path()));
    store.ingest(&event("e-1")).unwrap();
    store.checkpoint().unwrap();

    assert!(
        !wal_segments(&tmp.path().join("wal")).is_empty(),
        "the WAL lives at <data_dir>/wal; a release that moves it must also read it here"
    );
    assert!(tmp.path().join("storage").join("default").is_dir());
}
