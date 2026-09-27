use allsource_core::{
    QueryEventsRequest,
    domain::entities::Event,
    error::AllSourceError,
    store::{EventStore, EventStoreConfig},
};
use serde_json::json;
use tempfile::TempDir;

const TENANT: &str = "synthetic-archive-integrity";

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

fn history(store: &EventStore) -> Vec<Event> {
    store
        .query(&QueryEventsRequest {
            entity_id: Some("synthetic-entity".into()),
            tenant_id: Some(TENANT.into()),
            ..Default::default()
        })
        .unwrap()
}

#[derive(Clone, Copy)]
enum Warmup {
    None,
    Query,
    FullArchive,
}

fn assert_incomplete_history_cannot_authorize_append(warmup: Warmup) {
    let directory = TempDir::new().unwrap();
    {
        let store = EventStore::with_config(EventStoreConfig::with_persistence(directory.path()));
        for previous in 0..3 {
            store
                .ingest_with_expected_version(&event(), Some(previous))
                .unwrap();
        }
        store.flush_storage().unwrap();
    }

    let partition = directory.path().join(TENANT).join("2026-09");
    std::fs::create_dir_all(&partition).unwrap();
    let corrupt = partition.join("events-unreadable.parquet");
    std::fs::write(&corrupt, b"unknown archived history").unwrap();
    let store = EventStore::with_config(EventStoreConfig::with_persistence(directory.path()));
    // Existing reads tolerate an unreadable file. Partial history is not
    // evidence of the complete version required for a conditional write.
    match warmup {
        Warmup::None => assert!(!store.is_tenant_loaded(TENANT)),
        Warmup::Query => {
            assert_eq!(history(&store).len(), 3);
            assert!(store.is_tenant_loaded(TENANT));
        }
        Warmup::FullArchive => {
            assert_eq!(store.hydrate_all_from_storage().unwrap(), 3);
            assert!(store.is_tenant_loaded(TENANT));
        }
    }
    let mut subscriber = store.subscribe_events();

    assert!(matches!(
        store.ingest_with_expected_version(&event(), Some(3)),
        Err(AllSourceError::StorageError(_))
    ));
    assert!(subscriber.try_recv().is_err());
    assert_eq!(history(&store).len(), 3);
    assert_eq!(std::fs::read(corrupt).unwrap(), b"unknown archived history");
}

#[test]
fn cold_conditional_append_rejects_incomplete_archive() {
    assert_incomplete_history_cannot_authorize_append(Warmup::None);
}

#[test]
fn tolerant_query_does_not_authorize_conditional_append() {
    assert_incomplete_history_cannot_authorize_append(Warmup::Query);
}

#[test]
fn full_archive_hydration_does_not_authorize_incomplete_conditional_append() {
    assert_incomplete_history_cannot_authorize_append(Warmup::FullArchive);
}

#[test]
fn complete_archive_after_full_hydration_allows_conditional_append() {
    let directory = TempDir::new().unwrap();
    {
        let store = EventStore::with_config(EventStoreConfig::with_persistence(directory.path()));
        store
            .ingest_with_expected_version(&event(), Some(0))
            .unwrap();
        store.flush_storage().unwrap();
    }
    let store = EventStore::with_config(EventStoreConfig::with_persistence(directory.path()));
    assert_eq!(store.hydrate_all_from_storage().unwrap(), 1);
    assert_eq!(
        store
            .ingest_with_expected_version(&event(), Some(1))
            .unwrap(),
        2
    );
    assert_eq!(history(&store).len(), 2);
}
