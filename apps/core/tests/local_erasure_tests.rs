use allsource_core::{
    QueryEventsRequest,
    domain::entities::Event,
    infrastructure::persistence::{
        ParquetStorage, WALConfig, WriteAheadLog,
        erasure::{ErasureScope, erase_parquet_file, erase_wal_segment},
    },
    store::{EventStore, EventStoreConfig},
};
use std::{
    fs,
    io::Write,
    path::Path,
    time::{Duration, Instant},
};

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}

fn scope() -> ErasureScope {
    ErasureScope::new("alpha".into(), vec!["trainee-private".into()]).unwrap()
}

fn event(tenant: &str, entity: &str) -> Event {
    Event::from_strings(
        "workout.logged".into(),
        entity.into(),
        tenant.into(),
        serde_json::json!({"notes": format!("synthetic {tenant}/{entity}")}),
        None,
    )
    .unwrap()
}

fn ids_after_restart(root: &Path, tenant: &str) -> Vec<uuid::Uuid> {
    let store = EventStore::with_config(EventStoreConfig {
        storage_dir: Some(root.join("storage")),
        wal_dir: Some(root.join("wal")),
        ..Default::default()
    });
    let mut ids = store
        .query(&QueryEventsRequest {
            tenant_id: Some(tenant.into()),
            limit: Some(100),
            ..Default::default()
        })
        .unwrap()
        .into_iter()
        .map(|event| event.id)
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    ids
}

#[test]
fn wal_archives_and_snapshots_stay_erased_across_repeated_restarts() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().canonicalize().unwrap();
    let storage_root = root.join("storage");
    let storage = ParquetStorage::new(&storage_root).unwrap();
    let wal = WriteAheadLog::new(root.join("wal"), WALConfig::default()).unwrap();
    let owned = event("alpha", "trainee-private");
    let keep = event("alpha", "trainee-other");
    let other_tenant = event("beta", "trainee-private");
    let mut owned_tail = event("alpha", "trainee-private");
    owned_tail.version = 2;
    for event in [&owned, &keep, &other_tenant, &owned_tail] {
        storage.append_event(event.clone()).unwrap();
        wal.append(event.clone()).unwrap();
    }
    storage.flush().unwrap();
    storage
        .write_atomic_parquet(
            "alpha",
            "snapshot.alpha.synthetic",
            &[owned, keep.clone(), owned_tail],
        )
        .unwrap();
    wal.sync().unwrap();
    let segments = wal.segment_stamps().unwrap();
    drop(wal);
    let files = storage.list_parquet_files_for_tenant("alpha").unwrap();
    drop(storage);

    for path in &files {
        assert!(
            erase_parquet_file(&storage_root, path, &scope(), deadline())
                .unwrap()
                .removed
                > 0
        );
        assert_eq!(
            erase_parquet_file(&storage_root, path, &scope(), deadline())
                .unwrap()
                .removed,
            0
        );
    }
    for segment in segments {
        assert_eq!(
            erase_wal_segment(&segment.path, &scope(), deadline())
                .unwrap()
                .removed,
            2
        );
        assert_eq!(
            erase_wal_segment(&segment.path, &scope(), deadline())
                .unwrap()
                .removed,
            0
        );
    }
    for _ in 0..2 {
        assert_eq!(ids_after_restart(&root, "alpha"), vec![keep.id]);
        assert_eq!(ids_after_restart(&root, "beta"), vec![other_tenant.id]);
    }
}

#[test]
fn erasing_every_wal_entry_preserves_the_next_sequence() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().canonicalize().unwrap();
    let wal = WriteAheadLog::new(&root, WALConfig::default()).unwrap();
    assert_eq!(wal.append(event("alpha", "trainee-private")).unwrap(), 1);
    assert_eq!(wal.append(event("alpha", "trainee-private")).unwrap(), 2);
    wal.sync().unwrap();
    let path = wal.segment_stamps().unwrap()[0].path.clone();
    drop(wal);
    assert_eq!(
        erase_wal_segment(&path, &scope(), deadline())
            .unwrap()
            .removed,
        2
    );
    let reopened = WriteAheadLog::new(&root, WALConfig::default()).unwrap();
    assert!(reopened.recover_for_append().unwrap().is_empty());
    assert_eq!(reopened.append(event("alpha", "trainee-other")).unwrap(), 3);
}

#[test]
fn corrupt_wal_refuses_erasure_without_losing_valid_entries() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().canonicalize().unwrap();
    let wal = WriteAheadLog::new(&root, WALConfig::default()).unwrap();
    wal.append(event("alpha", "trainee-private")).unwrap();
    wal.sync().unwrap();
    let path = wal.segment_stamps().unwrap()[0].path.clone();
    drop(wal);
    let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
    writeln!(file, "unreadable retained record").unwrap();
    file.sync_all().unwrap();
    drop(file);
    let original = fs::read(&path).unwrap();
    assert!(erase_wal_segment(&path, &scope(), deadline()).is_err());
    assert_eq!(fs::read(&path).unwrap(), original);
    assert!(fs::read_dir(root).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".pending")
    }));
}

#[test]
fn startup_checkpoint_preserves_the_erasure_sequence_floor() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().canonicalize().unwrap();
    let wal = WriteAheadLog::new(root.join("wal"), WALConfig::default()).unwrap();
    wal.append(event("alpha", "trainee-other")).unwrap();
    wal.append(event("alpha", "trainee-private")).unwrap();
    wal.sync().unwrap();
    let path = wal.segment_stamps().unwrap()[0].path.clone();
    drop(wal);
    erase_wal_segment(&path, &scope(), deadline()).unwrap();
    let reopened = EventStore::with_config(EventStoreConfig {
        storage_dir: Some(root.join("storage")),
        wal_dir: Some(root.join("wal")),
        ..Default::default()
    });
    assert_eq!(reopened.wal().unwrap().current_sequence(), 2);
    assert_eq!(
        reopened
            .wal()
            .unwrap()
            .append(event("alpha", "trainee-new"))
            .unwrap(),
        3
    );
}

#[test]
fn expired_deadline_and_wrong_tenant_leave_archive_unchanged() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().canonicalize().unwrap();
    let storage = ParquetStorage::new(&root).unwrap();
    let path = storage
        .write_atomic_parquet(
            "beta",
            "events.synthetic",
            &[event("beta", "trainee-private")],
        )
        .unwrap();
    drop(storage);
    let original = fs::read(&path).unwrap();
    assert!(erase_parquet_file(&root, &path, &scope(), deadline()).is_err());
    let beta_scope = ErasureScope::new("beta".into(), vec!["trainee-private".into()]).unwrap();
    assert!(erase_parquet_file(&root, &path, &beta_scope, Instant::now()).is_err());
    assert_eq!(fs::read(&path).unwrap(), original);
}

#[test]
fn scope_rejects_empty_and_unsafe_identities() {
    assert!(ErasureScope::new("alpha".into(), vec![]).is_err());
    assert!(ErasureScope::new("../alpha".into(), vec!["owned".into()]).is_err());
    assert!(ErasureScope::new("alpha".into(), vec![String::new()]).is_err());
}

#[test]
fn an_entirely_erased_snapshot_remains_a_readable_empty_archive() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().canonicalize().unwrap();
    let storage = ParquetStorage::new(&root).unwrap();
    let path = storage
        .write_atomic_parquet(
            "alpha",
            "snapshot.alpha.synthetic",
            &[event("alpha", "trainee-private")],
        )
        .unwrap();
    drop(storage);
    assert_eq!(
        erase_parquet_file(&root, &path, &scope(), deadline())
            .unwrap()
            .removed,
        1
    );
    let reopened = ParquetStorage::new(&root).unwrap();
    assert!(
        reopened
            .load_events_from_file_path(&path, "alpha")
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        erase_parquet_file(&root, &path, &scope(), deadline())
            .unwrap()
            .removed,
        0
    );
}

#[test]
fn corrupt_archive_and_sequence_floor_fail_closed() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().canonicalize().unwrap();
    let partition = root.join("alpha/2026-11");
    fs::create_dir_all(&partition).unwrap();
    let path = partition.join("events.synthetic.parquet");
    fs::write(&path, b"corrupt synthetic archive").unwrap();
    assert!(erase_parquet_file(&root, &path, &scope(), deadline()).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"corrupt synthetic archive");

    let wal = WriteAheadLog::new(root.join("wal"), WALConfig::default()).unwrap();
    wal.append(event("alpha", "trainee-other")).unwrap();
    wal.sync().unwrap();
    drop(wal);
    fs::write(root.join("wal/.erasure-sequence-floor"), b"invalid").unwrap();
    let reopened = WriteAheadLog::new(root.join("wal"), WALConfig::default()).unwrap();
    assert!(reopened.recover_for_append().is_err());
    assert!(reopened.append(event("alpha", "trainee-new")).is_err());
    assert_eq!(reopened.read_all_entries().unwrap().len(), 1);
    drop(reopened);
    let store = EventStore::with_config(EventStoreConfig {
        storage_dir: Some(root.join("storage")),
        wal_dir: Some(root.join("wal")),
        ..Default::default()
    });
    assert!(store.ingest(&event("alpha", "trainee-new")).is_err());
}

#[cfg(unix)]
#[test]
fn symlink_and_hardlink_sources_are_rejected() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().canonicalize().unwrap();
    let wal = WriteAheadLog::new(&root, WALConfig::default()).unwrap();
    wal.append(event("alpha", "trainee-private")).unwrap();
    wal.sync().unwrap();
    let path = wal.segment_stamps().unwrap()[0].path.clone();
    drop(wal);
    let link = root.join("linked.log");
    std::os::unix::fs::symlink(&path, &link).unwrap();
    assert!(erase_wal_segment(&link, &scope(), deadline()).is_err());
    fs::hard_link(&path, root.join("other-copy.log")).unwrap();
    assert!(erase_wal_segment(&path, &scope(), deadline()).is_err());
}
