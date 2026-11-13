use allsource_core::{
    QueryEventsRequest,
    domain::entities::Event,
    infrastructure::persistence::{
        ParquetStorage, WALConfig, WriteAheadLog,
        backup::{BackupConfig, BackupManager},
        erasure::{ErasureScope, erase_wal_segment},
        erasure_maintenance::{ErasureManifest, LocalCopy, apply, preview},
        feed_log::{FeedLog, FeedLogRecord, FeedLogScope},
    },
    store::{EventStore, EventStoreConfig},
};
use std::{
    fs,
    path::PathBuf,
    process::{Child, Command, Output, Stdio},
    time::{Duration, Instant},
};

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(20)
}

fn event(tenant: &str, entity: &str) -> Event {
    Event::from_strings(
        "workout.logged".into(),
        entity.into(),
        tenant.into(),
        serde_json::json!({"note":"synthetic private history"}),
        None,
    )
    .unwrap()
}

struct Fixture {
    _temporary: tempfile::TempDir,
    root: PathBuf,
    manifest: ErasureManifest,
    kept: Event,
    other_tenant: Event,
    backups: Vec<String>,
}

impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().canonicalize().unwrap();
        let owned = event("alpha", "private");
        let kept = event("alpha", "other-owner");
        let other_tenant = event("beta", "private");
        let mut copies = Vec::new();
        let mut backups = Vec::new();
        for copy_name in ["primary", "restored-backup"] {
            let base = root.join(copy_name);
            let wal_dir = base.join("wal");
            let storage_dir = base.join("storage");
            let backup_dir = base.join("backups");
            let wal = WriteAheadLog::new(&wal_dir, WALConfig::default()).unwrap();
            let storage = ParquetStorage::new(&storage_dir).unwrap();
            for e in [&owned, &kept, &other_tenant] {
                wal.append(e.clone()).unwrap();
                storage.append_event(e.clone()).unwrap();
            }
            wal.sync().unwrap();
            storage.flush().unwrap();
            let snapshot = storage
                .write_atomic_parquet(
                    "alpha",
                    "snapshot.synthetic",
                    &[owned.clone(), kept.clone()],
                )
                .unwrap();
            fs::copy(
                &snapshot,
                snapshot.with_extension("parquet.original.retained"),
            )
            .unwrap();
            let manager = BackupManager::new(BackupConfig {
                backup_dir: backup_dir.clone(),
                ..Default::default()
            })
            .unwrap();
            backups.push(
                manager
                    .create_backup(&[owned.clone(), kept.clone(), other_tenant.clone()])
                    .unwrap()
                    .backup_id,
            );
            let feed = FeedLog::open(wal_dir.join("feed-log"), FeedLogScope::All).unwrap();
            feed.create(
                "alpha",
                &[
                    FeedLogRecord::new(owned.id, "workout.logged", "private"),
                    FeedLogRecord::new(kept.id, "workout.logged", "other-owner"),
                ],
            )
            .unwrap();
            feed.create(
                "beta",
                &[FeedLogRecord::new(
                    other_tenant.id,
                    "workout.logged",
                    "private",
                )],
            )
            .unwrap();
            copies.push(LocalCopy {
                wal_dir,
                storage_dir,
                backup_dir: Some(backup_dir),
            });
        }
        Self {
            _temporary: temporary,
            root,
            manifest: ErasureManifest {
                version: 1,
                tenant_id: "alpha".into(),
                entity_ids: vec!["private".into()],
                copies,
                external_copies: vec![],
            },
            kept,
            other_tenant,
            backups,
        }
    }

    fn assert_preserved(&self) {
        for (copy, backup_id) in self.manifest.copies.iter().zip(&self.backups) {
            for _ in 0..2 {
                let store = EventStore::try_with_config(EventStoreConfig {
                    storage_dir: Some(copy.storage_dir.clone()),
                    wal_dir: Some(copy.wal_dir.clone()),
                    ..Default::default()
                })
                .unwrap();
                for (tenant, expected) in [("alpha", self.kept.id), ("beta", self.other_tenant.id)]
                {
                    let mut ids: Vec<_> = store
                        .query(&QueryEventsRequest {
                            tenant_id: Some(tenant.into()),
                            limit: Some(100),
                            ..Default::default()
                        })
                        .unwrap()
                        .iter()
                        .map(|e| e.id)
                        .collect();
                    ids.sort();
                    ids.dedup();
                    assert_eq!(ids, vec![expected]);
                }
            }
            let manager = BackupManager::new(BackupConfig {
                backup_dir: copy.backup_dir.clone().unwrap(),
                ..Default::default()
            })
            .unwrap();
            let events = manager.restore_from_backup(backup_id).unwrap();
            assert_eq!(events.len(), 2);
            assert!(events.iter().any(|e| e.id == self.kept.id));
            assert!(events.iter().any(|e| e.id == self.other_tenant.id));
            let feed = FeedLog::open(copy.wal_dir.join("feed-log"), FeedLogScope::All).unwrap();
            assert!(!feed.is_open("alpha"));
            assert_eq!(feed.records("beta").unwrap().1[0].id, self.other_tenant.id);
        }
    }
}

struct OwnedChild(Option<Child>);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if let Some(child) = self.0.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn command(args: &[&str]) -> Output {
    let mut child = OwnedChild(Some(
        Command::new(env!("CARGO_BIN_EXE_allsource-erase"))
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    ));
    let limit = deadline();
    loop {
        if child.0.as_mut().unwrap().try_wait().unwrap().is_some() {
            return child.0.take().unwrap().wait_with_output().unwrap();
        }
        assert!(
            Instant::now() < limit,
            "erasure command exceeded test deadline"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn command_erases_all_declared_copies_and_backups_without_exposing_payloads() {
    let f = Fixture::new();
    let path = f.root.join("manifest.json");
    fs::write(&path, serde_json::to_vec(&f.manifest).unwrap()).unwrap();
    let output = command(&["preview", path.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let preview: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(preview["matches"].as_u64().unwrap() >= 8);
    assert_eq!(preview["local_copies_verified"], false);
    assert!(!String::from_utf8_lossy(&output.stdout).contains("private"));
    let output = command(&[
        "apply",
        path.to_str().unwrap(),
        preview["inventory_sha256"].as_str().unwrap(),
        "--writers-stopped",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(receipt["matches"], 0);
    assert_eq!(receipt["local_copies_verified"], true);
    f.assert_preserved();
    let retry = preview_manifest(&f.manifest);
    assert_eq!(retry.matches, 0);
    assert!(
        apply(&f.manifest, &retry.inventory_sha256, true, deadline())
            .unwrap()
            .local_copies_verified
    );
}

fn preview_manifest(
    manifest: &ErasureManifest,
) -> allsource_core::infrastructure::persistence::erasure_maintenance::MaintenanceReport {
    preview(manifest, deadline()).unwrap()
}

#[test]
fn confirmation_and_stale_preview_fail_before_erasure() {
    let f = Fixture::new();
    let p = preview_manifest(&f.manifest);
    assert!(apply(&f.manifest, &p.inventory_sha256, false, deadline()).is_err());
    assert!(apply(&f.manifest, "incorrect", true, deadline()).is_err());
    assert_eq!(preview_manifest(&f.manifest).matches, p.matches);
    let wal = WriteAheadLog::new(&f.manifest.copies[0].wal_dir, WALConfig::default()).unwrap();
    wal.recover_for_append().unwrap();
    wal.append(event("alpha", "another")).unwrap();
    wal.sync().unwrap();
    drop(wal);
    assert!(apply(&f.manifest, &p.inventory_sha256, true, deadline()).is_err());
    assert!(
        !f.manifest.copies[0]
            .wal_dir
            .join(".erasure-in-progress.json")
            .exists()
    );
}

#[test]
fn interrupted_copy_set_blocks_restart_and_resumes_same_manifest() {
    let f = Fixture::new();
    let p = preview_manifest(&f.manifest);
    let copy = &f.manifest.copies[0];
    let wal = WriteAheadLog::new(&copy.wal_dir, WALConfig::default()).unwrap();
    let paths = wal.segment_stamps().unwrap();
    drop(wal);
    erase_wal_segment(
        &paths[0].path,
        &ErasureScope::new("alpha".into(), vec!["private".into()]).unwrap(),
        deadline(),
    )
    .unwrap();
    fs::write(
        copy.wal_dir.join(".erasure-in-progress.json"),
        serde_json::to_vec(&p.manifest_sha256).unwrap(),
    )
    .unwrap();
    assert!(
        EventStore::try_with_config(EventStoreConfig {
            storage_dir: Some(copy.storage_dir.clone()),
            wal_dir: Some(copy.wal_dir.clone()),
            ..Default::default()
        })
        .is_err()
    );
    let p = preview_manifest(&f.manifest);
    apply(&f.manifest, &p.inventory_sha256, true, deadline()).unwrap();
    f.assert_preserved();
}

#[test]
fn unknown_copy_and_corrupt_backup_refuse_before_mutating_other_files() {
    let f = Fixture::new();
    let before = preview_manifest(&f.manifest);
    let unknown = f.manifest.copies[0].wal_dir.join("unknown-copy.bin");
    fs::write(&unknown, b"unclassified").unwrap();
    assert!(preview(&f.manifest, deadline()).is_err());
    fs::remove_file(unknown).unwrap();
    assert_eq!(
        preview_manifest(&f.manifest).inventory_sha256,
        before.inventory_sha256
    );
    let backup = f.manifest.copies[0]
        .backup_dir
        .as_ref()
        .unwrap()
        .join(format!("{}.backup.gz", f.backups[0]));
    fs::write(backup, b"corrupt backup").unwrap();
    assert!(apply(&f.manifest, &before.inventory_sha256, true, deadline()).is_err());
    assert!(
        !f.manifest.copies[0]
            .wal_dir
            .join(".erasure-in-progress.json")
            .exists()
    );
}

#[test]
fn external_copies_and_overlapping_roots_cannot_be_declared_complete() {
    let mut f = Fixture::new();
    f.manifest
        .external_copies
        .push("unmounted volume snapshot".into());
    assert!(preview(&f.manifest, deadline()).is_err());
    f.manifest.external_copies.clear();
    f.manifest.copies[0].backup_dir = Some(f.manifest.copies[0].wal_dir.clone());
    assert!(preview(&f.manifest, deadline()).is_err());
}

#[test]
fn cli_requires_explicit_mutation_arguments() {
    assert!(command(&["--help"]).status.success());
    assert!(
        !command(&["apply", "unused", "fingerprint"])
            .status
            .success()
    );
    assert!(
        !command(&["apply", "unused", "fingerprint", "--force"])
            .status
            .success()
    );
}

#[test]
fn backup_data_published_before_metadata_recovers_from_synced_receipt() {
    let f = Fixture::new();
    let p = preview_manifest(&f.manifest);
    let root = f.manifest.copies[0].backup_dir.as_ref().unwrap();
    let id = &f.backups[0];
    let metadata_path = root.join(format!("{id}_metadata.json"));
    let before: allsource_core::infrastructure::persistence::backup::BackupMetadata =
        serde_json::from_slice(&fs::read(&metadata_path).unwrap()).unwrap();
    let stage = tempfile::tempdir().unwrap();
    let manager = BackupManager::new(BackupConfig {
        backup_dir: stage.path().into(),
        ..Default::default()
    })
    .unwrap();
    let mut after = manager
        .create_backup(&[f.kept.clone(), f.other_tenant.clone()])
        .unwrap();
    fs::copy(
        stage.path().join(format!("{}.backup.gz", after.backup_id)),
        root.join(format!("{id}.backup.gz")),
    )
    .unwrap();
    after.backup_id = id.clone();
    after.to_sequence = before.to_sequence;
    fs::write(
        root.join(format!(".erasure-{id}.json")),
        serde_json::to_vec(&serde_json::json!({
            "scope":p.manifest_sha256,"before":before.checksum,"after":after
        }))
        .unwrap(),
    )
    .unwrap();
    fs::write(
        root.join(".erasure-in-progress.json"),
        serde_json::to_vec(&p.manifest_sha256).unwrap(),
    )
    .unwrap();
    let interrupted = root.join(format!(".erasure-meta-{}.pending", uuid::Uuid::new_v4()));
    fs::write(&interrupted, b"{unfinished metadata").unwrap();
    let retry = preview_manifest(&f.manifest);
    apply(&f.manifest, &retry.inventory_sha256, true, deadline()).unwrap();
    assert!(!interrupted.exists());
    assert!(!root.join(format!(".erasure-{id}.json")).exists());
    f.assert_preserved();
}

#[test]
fn malformed_manifest_does_not_echo_contents_or_default_backup_scope() {
    let f = Fixture::new();
    let path = f.root.join("manifest.json");
    let mut value = serde_json::to_value(&f.manifest).unwrap();
    value["copies"][0]
        .as_object_mut()
        .unwrap()
        .remove("backup_dir");
    fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(ErasureManifest::read(&path).is_err());
    value = serde_json::to_value(&f.manifest).unwrap();
    value["entity_ids"] = "synthetic-sensitive-token".into();
    fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    let output = command(&["preview", path.to_str().unwrap()]);
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("synthetic-sensitive-token"));
}

#[test]
fn interrupted_filtered_replacements_are_discarded_only_for_matching_manifest() {
    let f = Fixture::new();
    let p = preview_manifest(&f.manifest);
    let copy = &f.manifest.copies[0];
    let interrupted = copy
        .wal_dir
        .join(format!(".erasure-{}.pending", uuid::Uuid::new_v4()));
    fs::write(&interrupted, b"unfinished filtered WAL line").unwrap();
    assert!(preview(&f.manifest, deadline()).is_err());
    fs::write(
        copy.wal_dir.join(".erasure-in-progress.json"),
        serde_json::to_vec(&p.manifest_sha256).unwrap(),
    )
    .unwrap();
    let retry = preview_manifest(&f.manifest);
    apply(&f.manifest, &retry.inventory_sha256, true, deadline()).unwrap();
    assert!(!interrupted.exists());
    f.assert_preserved();
}
