use allsource_core::{
    QueryEventsRequest,
    domain::entities::Event,
    infrastructure::persistence::{ArchiveReadLimits, ParquetStorage},
    store::{EventStore, EventStoreConfig},
};
use serde_json::json;
use std::time::Duration;
use tempfile::TempDir;

const TENANT: &str = "synthetic-budget";

fn event(version: i64) -> Event {
    let mut event = Event::from_strings(
        "synthetic.updated".into(),
        "synthetic-entity".into(),
        TENANT.into(),
        json!({"synthetic": true}),
        None,
    )
    .unwrap();
    event.version = version;
    event
}

fn seeded(limits: ArchiveReadLimits) -> (TempDir, EventStore) {
    let directory = TempDir::new().unwrap();
    let storage = ParquetStorage::new(directory.path()).unwrap();
    storage
        .write_atomic_parquet(TENANT, "events-seed", &[event(1), event(2)])
        .unwrap();
    drop(storage);
    let store = EventStore::with_config(EventStoreConfig {
        strict_archive_limits: limits,
        ..EventStoreConfig::with_persistence(directory.path())
    });
    (directory, store)
}

fn assert_refused(limits: ArchiveReadLimits, dimension: &str) {
    let (_directory, store) = seeded(limits);
    let mut subscriber = store.subscribe_events();
    let error = store
        .ingest_with_expected_version(&event(0), Some(2))
        .unwrap_err();
    assert!(error.to_string().contains(dimension), "{error}");
    assert!(!store.is_tenant_loaded(TENANT));
    assert!(subscriber.try_recv().is_err());

    let query = QueryEventsRequest {
        tenant_id: Some(TENANT.into()),
        ..Default::default()
    };
    assert_eq!(store.query(&query).unwrap().len(), 2);
    assert!(
        store
            .ingest_with_expected_version(&event(0), Some(2))
            .is_err()
    );
    assert!(subscriber.try_recv().is_err());
    assert_eq!(
        store.ingest_with_expected_version(&event(0), None).unwrap(),
        3
    );
    assert_eq!(store.query(&query).unwrap().len(), 3);
}

#[test]
fn directory_entry_budget_refuses_conditional_writes() {
    assert_refused(
        ArchiveReadLimits {
            max_entries: 0,
            ..Default::default()
        },
        "entries",
    );
}

#[test]
fn file_count_budget_refuses_conditional_writes() {
    assert_refused(
        ArchiveReadLimits {
            max_files: 0,
            ..Default::default()
        },
        "files",
    );
}

#[test]
fn file_size_budget_refuses_conditional_writes() {
    assert_refused(
        ArchiveReadLimits {
            max_file_bytes: 1,
            ..Default::default()
        },
        "file bytes",
    );
}

#[test]
fn compressed_byte_budget_refuses_conditional_writes() {
    assert_refused(
        ArchiveReadLimits {
            max_compressed_bytes: 1,
            ..Default::default()
        },
        "compressed bytes",
    );
}

/// Rows and decoded bytes bound a hydration, not a version resolve.
///
/// A resolve folds one batch at a time and keeps two columns, so its peak heap
/// is a batch however many files it reads. Charging it a cumulative row or
/// decoded-byte ceiling measured nothing it could exhaust, and it barred a file
/// holding more than `max_rows` from ever being folded — a permanent refusal of
/// every conditional write to that tenant rather than a budget (gh#321).
fn assert_resolves_regardless(limits: ArchiveReadLimits, dimension: &str) {
    let (_directory, store) = seeded(limits);
    assert_eq!(
        store
            .ingest_with_expected_version(&event(0), Some(2))
            .unwrap(),
        3,
        "{dimension} must not gate a version resolve"
    );
    assert!(
        matches!(
            store.ingest_with_expected_version(&event(0), Some(2)),
            Err(allsource_core::error::AllSourceError::VersionConflict {
                expected: 2,
                current: 3
            })
        ),
        "the resolved version must still fence a stale expectation"
    );
}

#[test]
fn decoded_byte_budget_does_not_gate_a_version_resolve() {
    assert_resolves_regardless(
        ArchiveReadLimits {
            max_uncompressed_bytes: 1,
            ..Default::default()
        },
        "uncompressed bytes",
    );
}

#[test]
fn decoded_row_budget_does_not_gate_a_version_resolve() {
    assert_resolves_regardless(
        ArchiveReadLimits {
            max_rows: 1,
            ..Default::default()
        },
        "rows",
    );
}

#[test]
fn elapsed_time_budget_refuses_conditional_writes() {
    assert_refused(
        ArchiveReadLimits {
            timeout: Duration::ZERO,
            ..Default::default()
        },
        "elapsed time",
    );
}

#[test]
fn admitted_complete_history_still_allows_conditional_writes() {
    let (_directory, store) = seeded(ArchiveReadLimits::default());
    assert_eq!(
        store
            .ingest_with_expected_version(&event(0), Some(2))
            .unwrap(),
        3
    );
}
