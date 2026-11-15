use allsource_core::{
    QueryEventsRequest,
    domain::entities::Event,
    infrastructure::persistence::{
        ParquetStorage, WALConfig, WriteAheadLog,
        backup::{BackupConfig, BackupManager},
        erasure::{ErasureScope, erase_wal_segment},
        erasure_maintenance::{ErasureManifest, LocalCopy, apply, preview},
        erasure_redaction::{EventRedaction, content_sha256},
    },
    store::{EventStore, EventStoreConfig},
};
use serde_json::json;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(20)
}

fn event(tenant: &str, entity: &str, payload: serde_json::Value) -> Event {
    let mut event = Event::from_strings(
        "record.updated".into(),
        entity.into(),
        tenant.into(),
        payload,
        None,
    )
    .unwrap();
    // Parquet stores microseconds; full-record equality needs a representable fixture.
    event.timestamp = chrono::DateTime::from_timestamp_micros(1_800_000_000_123_456).unwrap();
    event
}

struct Fixture {
    _temporary: tempfile::TempDir,
    manifest: ErasureManifest,
    target: Event,
    keep: Event,
    other_tenant: Event,
    backup_id: String,
    segment: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().canonicalize().unwrap();
        let copy = LocalCopy {
            wal_dir: root.join("wal"),
            storage_dir: root.join("storage"),
            backup_dir: Some(root.join("backups")),
        };
        let private = event("alpha", "private", json!({"note":"remove personal record"}));
        let mut target = event(
            "alpha",
            "shared",
            json!({"owner":"departing", "note":"private note", "template":"preserved"}),
        );
        target.metadata = Some(json!({"email":"synthetic@example.invalid"}));
        let mut keep = event(
            "alpha",
            "shared",
            json!({"owner":"remaining", "note":"keep this record"}),
        );
        keep.version = 2;
        let other_tenant = event("beta", "shared", target.payload.clone());
        let wal = WriteAheadLog::new(&copy.wal_dir, WALConfig::default()).unwrap();
        let storage = ParquetStorage::new(&copy.storage_dir).unwrap();
        let records = vec![private, target.clone(), keep.clone(), other_tenant.clone()];
        for event in &records {
            wal.append(event.clone()).unwrap();
            storage.append_event(event.clone()).unwrap();
        }
        wal.sync().unwrap();
        storage.flush().unwrap();
        let snapshot = storage
            .write_atomic_parquet("alpha", "snapshot.redaction", &records[..3])
            .unwrap();
        std::fs::copy(
            &snapshot,
            snapshot.with_extension("parquet.original.retained"),
        )
        .unwrap();
        let segment = wal.segment_stamps().unwrap()[0].path.clone();
        drop(wal);
        drop(storage);
        let backups = BackupManager::new(BackupConfig {
            backup_dir: copy.backup_dir.clone().unwrap(),
            ..Default::default()
        })
        .unwrap();
        let backup_id = backups.create_backup(&records).unwrap().backup_id;
        let redaction = EventRedaction {
            event_id: target.id,
            entity_id: "shared".into(),
            expected_content_sha256: content_sha256(&target.payload, target.metadata.as_ref())
                .unwrap(),
            payload: json!({"owner":null, "note":null, "template":"preserved"}),
            metadata: None,
        };
        let manifest = ErasureManifest {
            version: 2,
            tenant_id: "alpha".into(),
            entity_ids: vec!["private".into()],
            redactions: vec![redaction],
            copies: vec![copy],
            external_copies: vec![],
        };
        Self {
            _temporary: temporary,
            manifest,
            target,
            keep,
            other_tenant,
            backup_id,
            segment,
        }
    }

    fn assert_records(&self, records: &[Event]) {
        assert_eq!(records.len(), 3);
        let target = records.iter().find(|e| e.id == self.target.id).unwrap();
        let replacement = &self.manifest.redactions[0];
        assert_eq!(target.payload, replacement.payload);
        assert_eq!(target.metadata, replacement.metadata);
        assert_eq!(target.version, self.target.version);
        assert_eq!(target.timestamp, self.target.timestamp);
        for expected in [&self.keep, &self.other_tenant] {
            let actual = records.iter().find(|e| e.id == expected.id).unwrap();
            assert_eq!(
                serde_json::to_value(actual).unwrap(),
                serde_json::to_value(expected).unwrap()
            );
        }
    }

    fn verify_recovery(&self) {
        let copy = &self.manifest.copies[0];
        for _ in 0..2 {
            let store = EventStore::try_with_config(EventStoreConfig {
                storage_dir: Some(copy.storage_dir.clone()),
                wal_dir: Some(copy.wal_dir.clone()),
                ..Default::default()
            })
            .unwrap();
            let mut records = Vec::new();
            for tenant in ["alpha", "beta"] {
                records.extend(
                    store
                        .query(&QueryEventsRequest {
                            tenant_id: Some(tenant.into()),
                            limit: Some(100),
                            ..Default::default()
                        })
                        .unwrap(),
                );
            }
            records.sort_by_key(|e| e.id);
            records.dedup_by_key(|e| e.id);
            self.assert_records(&records);
        }
        let backups = BackupManager::new(BackupConfig {
            backup_dir: copy.backup_dir.clone().unwrap(),
            ..Default::default()
        })
        .unwrap();
        self.assert_records(&backups.restore_from_backup(&self.backup_id).unwrap());
    }
}

#[test]
fn redaction_preserves_shared_records_across_restart_and_backup_restore() {
    let f = Fixture::new();
    let before = preview(&f.manifest, deadline()).unwrap();
    assert_eq!(before.redactions, 5);
    assert_eq!(before.matches, 10);
    let receipt = apply(&f.manifest, &before.inventory_sha256, true, deadline()).unwrap();
    assert!(receipt.local_copies_verified);
    assert_eq!(receipt.matches, 0);
    let again = preview(&f.manifest, deadline()).unwrap();
    assert_eq!(again.matches, 0);
    apply(&f.manifest, &again.inventory_sha256, true, deadline()).unwrap();
    f.verify_recovery();
}

#[test]
fn already_redacted_wal_can_resume_remaining_copies() {
    let f = Fixture::new();
    let scope =
        ErasureScope::with_redactions("alpha".into(), vec![], f.manifest.redactions.clone())
            .unwrap();
    assert_eq!(
        erase_wal_segment(&f.segment, &scope, deadline())
            .unwrap()
            .redacted,
        1
    );
    let next = preview(&f.manifest, deadline()).unwrap();
    assert_eq!(next.redactions, 4);
    apply(&f.manifest, &next.inventory_sha256, true, deadline()).unwrap();
    f.verify_recovery();
}

#[test]
fn stale_redaction_refuses_before_modifying_any_file() {
    let mut f = Fixture::new();
    let bytes = std::fs::read(&f.segment).unwrap();
    f.manifest.redactions[0].expected_content_sha256 = "0".repeat(64);
    assert!(
        preview(&f.manifest, deadline())
            .unwrap_err()
            .to_string()
            .contains("approved source")
    );
    assert_eq!(bytes, std::fs::read(&f.segment).unwrap());
}

#[test]
fn missing_event_and_wrong_entity_cannot_claim_completion() {
    let mut f = Fixture::new();
    f.manifest.redactions[0].event_id = uuid::Uuid::new_v4();
    assert!(
        preview(&f.manifest, deadline())
            .unwrap_err()
            .to_string()
            .contains("target missing")
    );
    f.manifest.redactions[0].event_id = f.target.id;
    f.manifest.redactions[0].entity_id = "wrong-stream".into();
    assert!(
        preview(&f.manifest, deadline())
            .unwrap_err()
            .to_string()
            .contains("different entity")
    );
}

#[test]
fn redaction_requires_version_two_and_unambiguous_scope() {
    let mut f = Fixture::new();
    f.manifest.version = 1;
    assert!(preview(&f.manifest, deadline()).is_err());
    f.manifest.version = 2;
    f.manifest.redactions.push(f.manifest.redactions[0].clone());
    assert!(preview(&f.manifest, deadline()).is_err());
    f.manifest.redactions.pop();
    f.manifest.entity_ids.push("shared".into());
    assert!(preview(&f.manifest, deadline()).is_err());
}

#[test]
fn wal_redaction_preserves_nanosecond_timestamp() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().canonicalize().unwrap();
    let wal = WriteAheadLog::new(&root, WALConfig::default()).unwrap();
    let mut expected = event("alpha", "shared", json!({"note":"private"}));
    expected.timestamp = chrono::DateTime::from_timestamp(1_800_000_000, 123_456_789).unwrap();
    wal.append(expected.clone()).unwrap();
    wal.sync().unwrap();
    let segment = wal.segment_stamps().unwrap()[0].path.clone();
    drop(wal);
    let redaction = EventRedaction {
        event_id: expected.id,
        entity_id: "shared".into(),
        expected_content_sha256: content_sha256(&expected.payload, expected.metadata.as_ref())
            .unwrap(),
        payload: json!({"note":null}),
        metadata: None,
    };
    expected.payload = redaction.payload.clone();
    let scope = ErasureScope::with_redactions("alpha".into(), vec![], vec![redaction]).unwrap();
    assert_eq!(
        erase_wal_segment(&segment, &scope, deadline())
            .unwrap()
            .redacted,
        1
    );
    let reopened = WriteAheadLog::new(&root, WALConfig::default()).unwrap();
    let recovered = reopened.recover().unwrap();
    assert_eq!(recovered.len(), 1);
    assert_eq!(
        serde_json::to_value(&recovered[0]).unwrap(),
        serde_json::to_value(&expected).unwrap()
    );
}

#[test]
fn content_digest_ignores_json_object_key_order() {
    let left: serde_json::Value = serde_json::from_str(r#"{"a":1,"b":{"c":2,"d":3}}"#).unwrap();
    let right: serde_json::Value = serde_json::from_str(r#"{"b":{"d":3,"c":2},"a":1}"#).unwrap();
    assert_eq!(
        content_sha256(&left, None).unwrap(),
        content_sha256(&right, None).unwrap()
    );
}
