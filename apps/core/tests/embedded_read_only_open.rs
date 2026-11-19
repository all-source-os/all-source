#![cfg(feature = "embedded")]

use allsource_core::{
    embedded::{Config, EmbeddedCore, IngestEvent},
    infrastructure::persistence::{WALConfig, WriteAheadLog},
};
use serde_json::json;
use std::{fs, io::Write, path::Path};
use tempfile::TempDir;

fn snapshot(path: &Path) -> Vec<(String, Vec<u8>, std::time::SystemTime, bool)> {
    fn walk(
        root: &Path,
        path: &Path,
        output: &mut Vec<(String, Vec<u8>, std::time::SystemTime, bool)>,
    ) {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            let metadata = fs::metadata(&path).unwrap();
            let name = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            if path.is_dir() {
                output.push((
                    name,
                    Vec::new(),
                    metadata.modified().unwrap(),
                    metadata.permissions().readonly(),
                ));
                walk(root, &path, output);
            } else {
                output.push((
                    name,
                    fs::read(path).unwrap(),
                    metadata.modified().unwrap(),
                    metadata.permissions().readonly(),
                ));
            }
        }
    }
    let mut result = Vec::new();
    walk(path, path, &mut result);
    result.sort_by(|left, right| left.0.cmp(&right.0));
    result
}

#[tokio::test]
async fn read_only_open_refuses_missing_store_without_creating_it() {
    let tmp = TempDir::new().unwrap();
    let missing = tmp.path().join("missing");
    let result = EmbeddedCore::open(
        Config::builder()
            .data_dir(&missing)
            .read_only(true)
            .build()
            .unwrap(),
    )
    .await;
    assert!(result.is_err());
    assert!(!missing.exists());
}

#[tokio::test]
async fn read_only_open_replays_wal_without_mutating_torn_segment() {
    let tmp = TempDir::new().unwrap();
    let writer = EmbeddedCore::open(Config::builder().data_dir(tmp.path()).build().unwrap())
        .await
        .unwrap();
    writer
        .ingest(IngestEvent {
            entity_id: "fixture",
            event_type: "fixture.created",
            payload: json!({"value": 1}),
            metadata: None,
            tenant_id: None,
        })
        .await
        .unwrap();
    drop(writer);
    let wal = tmp.path().join("wal/wal-0000000000000000.log");
    fs::OpenOptions::new()
        .append(true)
        .open(&wal)
        .unwrap()
        .write_all(b"{torn")
        .unwrap();
    let before = snapshot(tmp.path());
    let reader = EmbeddedCore::open(
        Config::builder()
            .data_dir(tmp.path())
            .read_only(true)
            .wal_fsync_interval_ms(1)
            .build()
            .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(reader.stats().total_events, 1);
    reader.shutdown().await.unwrap();
    drop(reader);
    assert_eq!(snapshot(tmp.path()), before);
}

#[tokio::test]
async fn read_only_open_leaves_live_writer_usable() {
    let tmp = TempDir::new().unwrap();
    let writer = EmbeddedCore::open(Config::builder().data_dir(tmp.path()).build().unwrap())
        .await
        .unwrap();
    writer
        .ingest(IngestEvent {
            entity_id: "fixture",
            event_type: "fixture.created",
            payload: json!({"value": 1}),
            metadata: None,
            tenant_id: None,
        })
        .await
        .unwrap();
    let before = snapshot(tmp.path());
    let reader = EmbeddedCore::open(
        Config::builder()
            .data_dir(tmp.path())
            .read_only(true)
            .build()
            .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(reader.stats().total_events, 1);
    reader.shutdown().await.unwrap();
    drop(reader);
    assert_eq!(snapshot(tmp.path()), before);
    writer
        .ingest(IngestEvent {
            entity_id: "fixture",
            event_type: "fixture.updated",
            payload: json!({"value": 2}),
            metadata: None,
            tenant_id: None,
        })
        .await
        .unwrap();
    assert_eq!(writer.stats().total_events, 2);
}

#[tokio::test]
async fn read_only_wal_only_store_does_not_create_storage_directory() {
    let tmp = TempDir::new().unwrap();
    fs::create_dir(tmp.path().join("wal")).unwrap();
    let before = snapshot(tmp.path());
    let reader = EmbeddedCore::open(
        Config::builder()
            .data_dir(tmp.path())
            .read_only(true)
            .build()
            .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(reader.stats().total_events, 0);
    reader.shutdown().await.unwrap();
    drop(reader);
    assert_eq!(snapshot(tmp.path()), before);
    assert!(!tmp.path().join("storage").exists());
}

#[cfg(unix)]
#[tokio::test]
async fn read_only_open_refuses_an_unreadable_wal_segment() {
    use std::os::unix::fs::symlink;

    let tmp = TempDir::new().unwrap();
    fs::create_dir(tmp.path().join("wal")).unwrap();
    let segment = tmp.path().join("wal/wal-0000000000000000.log");
    symlink("missing", &segment).unwrap();
    let result = EmbeddedCore::open(
        Config::builder()
            .data_dir(tmp.path())
            .read_only(true)
            .build()
            .unwrap(),
    )
    .await;
    assert!(result.is_err());
    assert!(
        fs::symlink_metadata(segment)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(!tmp.path().join("storage").exists());
}

#[tokio::test]
async fn read_only_open_refuses_invalid_storage_path() {
    let tmp = TempDir::new().unwrap();
    fs::create_dir(tmp.path().join("wal")).unwrap();
    fs::write(tmp.path().join("storage"), b"not a directory").unwrap();
    let before = snapshot(tmp.path());
    let result = EmbeddedCore::open(
        Config::builder()
            .data_dir(tmp.path())
            .read_only(true)
            .build()
            .unwrap(),
    )
    .await;
    assert!(result.is_err());
    assert_eq!(snapshot(tmp.path()), before);
}

#[cfg(unix)]
#[tokio::test]
async fn read_only_open_refuses_dangling_storage_symlink() {
    use std::os::unix::fs::symlink;

    let tmp = TempDir::new().unwrap();
    fs::create_dir(tmp.path().join("wal")).unwrap();
    let storage = tmp.path().join("storage");
    symlink("missing", &storage).unwrap();
    let result = EmbeddedCore::open(
        Config::builder()
            .data_dir(tmp.path())
            .read_only(true)
            .build()
            .unwrap(),
    )
    .await;
    assert!(result.is_err());
    assert!(
        fs::symlink_metadata(storage)
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[test]
fn read_only_wal_rejects_writer_methods_without_touching_files() {
    let tmp = TempDir::new().unwrap();
    let segment = tmp.path().join("wal-0000000000000000.log");
    fs::write(&segment, b"{torn").unwrap();
    let before = snapshot(tmp.path());
    let wal = WriteAheadLog::open_read_only(tmp.path(), WALConfig::default()).unwrap();
    assert!(wal.flush().is_err());
    assert!(wal.sync().is_err());
    assert!(wal.seal().is_err());
    assert!(wal.truncate().is_err());
    assert!(wal.remove_sealed(&segment).is_err());
    assert!(wal.recover_for_append().is_err());
    assert!(wal.recover_for_append_after(0, &[]).is_err());
    assert!(wal.recover_entries_for_append(0).is_err());
    assert_eq!(snapshot(tmp.path()), before);
}
