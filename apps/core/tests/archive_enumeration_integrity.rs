#![cfg(unix)]

use allsource_core::{
    QueryEventsRequest,
    domain::entities::Event,
    error::AllSourceError,
    infrastructure::persistence::ParquetStorage,
    store::{EventStore, EventStoreConfig},
};
use serde_json::json;
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};
use tempfile::TempDir;

const TENANT: &str = "synthetic-enumeration-integrity";

fn event() -> Event {
    Event::from_strings(
        "synthetic.updated".into(),
        "synthetic-entity".into(),
        TENANT.into(),
        json!({"synthetic": true}),
        None,
    )
    .unwrap()
}

struct RestorePermissions(PathBuf, fs::Permissions);

impl Drop for RestorePermissions {
    fn drop(&mut self) {
        fs::set_permissions(&self.0, self.1.clone()).unwrap();
    }
}

#[test]
fn unreadable_partition_cannot_authorize_append_even_after_tolerant_query() {
    for warm_query in [false, true] {
        let directory = TempDir::new().unwrap();
        let storage = ParquetStorage::new(directory.path()).unwrap();
        let archive = storage
            .write_atomic_parquet(TENANT, "events-hidden", &[event()])
            .unwrap();
        drop(storage);
        let partition = archive.parent().unwrap().to_path_buf();
        let _restore = RestorePermissions(
            partition.clone(),
            fs::metadata(&partition).unwrap().permissions(),
        );
        fs::set_permissions(&partition, fs::Permissions::from_mode(0o000)).unwrap();
        assert_eq!(
            fs::read_dir(&partition).unwrap_err().kind(),
            std::io::ErrorKind::PermissionDenied,
            "this fixture requires an unprivileged test process"
        );
        let store = EventStore::with_config(EventStoreConfig::with_persistence(directory.path()));
        if warm_query {
            assert!(
                store
                    .query(&QueryEventsRequest {
                        tenant_id: Some(TENANT.into()),
                        ..Default::default()
                    })
                    .unwrap()
                    .is_empty()
            );
        }
        let mut subscriber = store.subscribe_events();
        assert!(matches!(
            store.ingest_with_expected_version(&event(), Some(0)),
            Err(AllSourceError::StorageError(_))
        ));
        assert!(subscriber.try_recv().is_err());
        if !warm_query {
            assert!(!store.is_tenant_loaded(TENANT));
        }
    }
}

#[test]
fn a_non_directory_tenant_path_is_not_an_empty_archive() {
    let directory = TempDir::new().unwrap();
    fs::write(directory.path().join(TENANT), b"unavailable tenant archive").unwrap();
    let store = EventStore::with_config(EventStoreConfig::with_persistence(directory.path()));
    assert!(matches!(
        store.ingest_with_expected_version(&event(), Some(0)),
        Err(AllSourceError::StorageError(_))
    ));
}
