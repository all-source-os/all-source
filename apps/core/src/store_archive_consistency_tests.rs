use super::*;
use std::time::Duration;
use tempfile::TempDir;

const TENANT: &str = "synthetic-archive-consistency";

fn event() -> Event {
    Event::from_strings(
        "synthetic.updated".into(),
        "synthetic-entity".into(),
        TENANT.into(),
        serde_json::json!({"synthetic": true}),
        None,
    )
    .unwrap()
}

#[test]
fn eviction_cannot_erase_an_unflushed_conditional_predecessor() {
    let directory = TempDir::new().unwrap();
    let store = EventStore::with_config(EventStoreConfig::with_persistence(directory.path()));
    store
        .ingest_with_expected_version(&event(), Some(0))
        .unwrap();
    store.evict_tenant(TENANT);
    assert_eq!(
        store.total_events(),
        1,
        "pending history must remain resident"
    );
    assert!(
        store
            .ingest_with_expected_version(&event(), Some(0))
            .is_err()
    );
    assert_eq!(
        store
            .query(&QueryEventsRequest {
                tenant_id: Some(TENANT.into()),
                ..Default::default()
            })
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        store
            .ingest_with_expected_version(&event(), Some(1))
            .unwrap(),
        2
    );
    store.flush_storage().unwrap();
    store.evict_tenant(TENANT);
    assert!(!store.is_tenant_loaded(TENANT));
    assert_eq!(
        store
            .ingest_with_expected_version(&event(), Some(2))
            .unwrap(),
        3
    );
}

#[test]
fn in_memory_store_cannot_evict_its_only_history() {
    let store = EventStore::new();
    store
        .ingest_with_expected_version(&event(), Some(0))
        .unwrap();
    store.evict_tenant(TENANT);
    assert_eq!(store.total_events(), 1);
    assert_eq!(
        store
            .ingest_with_expected_version(&event(), Some(1))
            .unwrap(),
        2
    );
}

#[test]
fn replicated_history_reaches_parquet_before_it_can_be_evicted() {
    let directory = TempDir::new().unwrap();
    let store = EventStore::with_config(EventStoreConfig::with_persistence(directory.path()));
    let mut replicated = event();
    replicated.version = 1;
    store.ingest_replicated(&replicated).unwrap();
    store.evict_tenant(TENANT);
    assert_eq!(store.total_events(), 1);
    store.flush_storage().unwrap();
    store.evict_tenant(TENANT);
    assert_eq!(store.total_events(), 0);
    let restored = store
        .query(&QueryEventsRequest {
            tenant_id: Some(TENANT.into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(restored.len(), 1);
    assert_eq!(restored[0].id, replicated.id);
    assert_eq!(restored[0].version, 1);
}

#[test]
fn read_only_replication_never_buffers_archive_writes_or_discards_memory() {
    let directory = TempDir::new().unwrap();
    let store = EventStore::with_config(EventStoreConfig {
        read_only: true,
        ..EventStoreConfig::with_persistence(directory.path())
    });
    store.ingest_replicated(&event()).unwrap();
    assert!(
        !store
            .storage
            .as_ref()
            .unwrap()
            .read()
            .has_pending_tenant_events(TENANT)
    );
    store.evict_tenant(TENANT);
    assert_eq!(store.total_events(), 1);
}

#[test]
fn eviction_does_not_wait_for_in_flight_storage_work() {
    let directory = TempDir::new().unwrap();
    let store = Arc::new(EventStore::with_config(EventStoreConfig::with_persistence(
        directory.path(),
    )));
    store
        .ingest_with_expected_version(&event(), Some(0))
        .unwrap();
    store.flush_storage().unwrap();
    let storage = store.storage.as_ref().unwrap().read();
    let eviction_store = Arc::clone(&store);
    let (finished_tx, finished_rx) = std::sync::mpsc::channel();
    let eviction = std::thread::spawn(move || {
        eviction_store.evict_tenant(TENANT);
        finished_tx.send(()).unwrap();
    });
    // test-hang-allow: release storage even if a regression makes eviction wait for it.
    let finished_before_release = finished_rx.recv_timeout(Duration::from_secs(1));
    drop(storage);
    eviction.join().unwrap();
    assert!(finished_before_release.is_ok());
    assert_eq!(store.total_events(), 1);
    store.evict_tenant(TENANT);
    assert_eq!(store.total_events(), 0);
    assert!(!store.is_tenant_loaded(TENANT));
}

/// Eviction must not lower an entity's version: the archive still holds those
/// events, and a version handed out twice is two different events claiming one
/// place in the entity's history.
#[test]
fn eviction_does_not_lower_an_entity_version() {
    let directory = TempDir::new().unwrap();
    let store = EventStore::with_config(EventStoreConfig::with_persistence(directory.path()));
    store
        .ingest_with_expected_version(&event(), Some(0))
        .unwrap();
    store
        .ingest_with_expected_version(&event(), Some(1))
        .unwrap();
    store.flush_storage().unwrap();
    assert_eq!(store.get_entity_version("synthetic-entity"), 2);

    store.evict_tenant(TENANT);
    assert_eq!(
        store.total_events(),
        0,
        "the cache should have been dropped"
    );
    assert_eq!(
        store.get_entity_version("synthetic-entity"),
        2,
        "eviction lowered a version that is still on disk"
    );

    assert!(
        store
            .ingest_with_expected_version(&event(), Some(0))
            .is_err(),
        "a stale expected_version was accepted after eviction"
    );
    assert_eq!(
        store
            .ingest_with_expected_version(&event(), Some(2))
            .unwrap(),
        3
    );
}

/// A cold store must learn an entity's version from the archive without
/// hydrating the tenant's events.
#[test]
fn conditional_write_resolves_version_without_loading_the_tenant() {
    let directory = TempDir::new().unwrap();
    {
        let store = EventStore::with_config(EventStoreConfig::with_persistence(directory.path()));
        store
            .ingest_with_expected_version(&event(), Some(0))
            .unwrap();
        store
            .ingest_with_expected_version(&event(), Some(1))
            .unwrap();
        store.flush_storage().unwrap();
    }

    let store = EventStore::with_config(EventStoreConfig::with_persistence(directory.path()));
    assert_eq!(store.total_events(), 0);
    assert!(
        store
            .ingest_with_expected_version(&event(), Some(0))
            .is_err()
    );
    assert_eq!(
        store
            .ingest_with_expected_version(&event(), Some(2))
            .unwrap(),
        3
    );
    assert!(
        !store.is_tenant_loaded(TENANT),
        "resolving a version must not hydrate the tenant"
    );
}

#[test]
fn resident_counter_counts_only_the_new_batch() {
    let store = EventStore::new();
    store.ingest_batch(vec![event(), event(), event()]).unwrap();
    store.ingest_batch(vec![event(), event()]).unwrap();
    assert_eq!(store.stats().total_ingested, 5);
    assert_eq!(store.total_events(), 5);
}

#[tokio::test(flavor = "current_thread")]
async fn eviction_between_hydration_and_version_check_cannot_authorize_a_stale_write() {
    let directory = TempDir::new().unwrap();
    {
        let store = EventStore::with_config(EventStoreConfig::with_persistence(directory.path()));
        store
            .ingest_with_expected_version(&event(), Some(0))
            .unwrap();
        store.flush_storage().unwrap();
    }
    let store = Arc::new(EventStore::with_config(EventStoreConfig::with_persistence(
        directory.path(),
    )));
    let gate_store = Arc::clone(&store);
    let (held_tx, held_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let holder = std::thread::spawn(move || {
        let _gate = gate_store.durability_gate.write();
        held_tx.send(()).unwrap();
        // test-hang-allow: synthetic checkpoint pause, always released within three seconds.
        release_rx.recv_timeout(Duration::from_secs(3))
    });
    held_rx.await.unwrap();
    let writer_store = Arc::clone(&store);
    let writer = tokio::task::spawn_blocking(move || {
        writer_store.ingest_with_expected_version(&event(), Some(0))
    });
    // test-hang-allow: wait for the version index to resolve, before releasing the checkpoint gate.
    tokio::time::timeout(Duration::from_secs(1), async {
        while !store.version_index_tenants.contains_key(TENANT) {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
    let eviction_store = Arc::clone(&store);
    let (evicted_tx, mut evicted_rx) = tokio::sync::oneshot::channel();
    let eviction = tokio::task::spawn_blocking(move || {
        eviction_store.evict_tenant(TENANT);
        let _ = evicted_tx.send(());
    });
    // test-hang-allow: the fixed store may pin the cache until the write finishes.
    let _ = tokio::time::timeout(Duration::from_millis(100), &mut evicted_rx).await;
    release_tx.send(()).unwrap();
    let result = writer.await.unwrap();
    eviction.await.unwrap();
    holder.join().unwrap().unwrap();
    assert!(
        result.is_err(),
        "eviction authorized a stale write: {result:?}"
    );
    assert_eq!(
        store
            .query(&QueryEventsRequest {
                tenant_id: Some(TENANT.into()),
                ..Default::default()
            })
            .unwrap()
            .len(),
        1
    );
}
