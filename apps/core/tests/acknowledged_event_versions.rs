use allsource_core::{
    QueryEventsRequest,
    domain::entities::Event,
    infrastructure::persistence::WALConfig,
    store::{EventStore, EventStoreConfig},
};
use serde_json::json;
use std::sync::{Arc, Barrier};
use tempfile::TempDir;

fn event(entity: &str) -> Event {
    Event::from_strings(
        "agent_run.v1.evidence".into(),
        entity.into(),
        "synthetic-run-tenant".into(),
        json!({"synthetic": true}),
        None,
    )
    .unwrap()
}

fn versions(store: &EventStore, entity: &str) -> Vec<i64> {
    let mut versions: Vec<_> = store
        .query(&QueryEventsRequest {
            entity_id: Some(entity.into()),
            tenant_id: Some("synthetic-run-tenant".into()),
            ..Default::default()
        })
        .unwrap()
        .iter()
        .map(|event| event.version)
        .collect();
    versions.sort_unstable();
    versions
}

#[test]
fn acknowledged_versions_reach_queries_and_subscribers_without_changing_caller_event() {
    let store = EventStore::new();
    let mut subscriber = store.subscribe_events();
    for previous in 0..3 {
        let mut original = event("run");
        original.version = 987;
        let ack = store
            .ingest_with_expected_version(&original, Some(previous))
            .unwrap();
        assert_eq!(ack, previous + 1);
        assert_eq!(original.version, 987);
        assert_eq!(
            subscriber.try_recv().unwrap().version,
            i64::try_from(ack).unwrap()
        );
    }
    assert!(
        store
            .ingest_with_expected_version(&event("run"), Some(0))
            .is_err()
    );
    assert_eq!(versions(&store, "run"), vec![1, 2, 3]);
    assert!(subscriber.try_recv().is_err());
}

#[test]
fn simultaneous_conditional_writes_have_one_stored_winner() {
    let store = Arc::new(EventStore::new());
    let barrier = Arc::new(Barrier::new(8));
    let mut workers = Vec::new();
    for _ in 0..8 {
        let store = Arc::clone(&store);
        let barrier = Arc::clone(&barrier);
        workers.push(std::thread::spawn(move || {
            barrier.wait();
            store.ingest_with_expected_version(&event("race"), Some(0))
        }));
    }
    let results: Vec<_> = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect();
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(versions(&store, "race"), vec![1]);
}

#[test]
fn wal_recovery_preserves_acknowledged_versions() {
    let directory = TempDir::new().unwrap();
    {
        let store = EventStore::with_config(EventStoreConfig::with_wal(
            directory.path(),
            WALConfig::default(),
        ));
        for previous in 0..3 {
            store
                .ingest_with_expected_version(&event("wal"), Some(previous))
                .unwrap();
        }
    }
    let store = EventStore::with_config(EventStoreConfig::with_wal(
        directory.path(),
        WALConfig::default(),
    ));
    assert_eq!(versions(&store, "wal"), vec![1, 2, 3]);
    assert_eq!(
        store
            .ingest_with_expected_version(&event("wal"), Some(3))
            .unwrap(),
        4
    );
}

#[test]
fn cold_parquet_write_uses_existing_version_without_a_preceding_query() {
    let directory = TempDir::new().unwrap();
    {
        let store = EventStore::with_config(EventStoreConfig::with_persistence(directory.path()));
        for previous in 0..3 {
            store
                .ingest_with_expected_version(&event("cold"), Some(previous))
                .unwrap();
        }
        store.flush_storage().unwrap();
    }
    {
        let store = EventStore::with_config(EventStoreConfig::with_persistence(directory.path()));
        assert!(!store.is_tenant_loaded("synthetic-run-tenant"));
        assert_eq!(
            store
                .ingest_with_expected_version(&event("cold"), Some(3))
                .unwrap(),
            4
        );
        assert_eq!(versions(&store, "cold"), vec![1, 2, 3, 4]);
    }
}
