use super::*;
use crate::infrastructure::persistence::ParquetStorage;
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use tempfile::TempDir;

const TENANT: &str = "synthetic-verified-versions";
const ENTITY: &str = "synthetic-entity";

fn event(tenant: &str, entity: &str, version: i64) -> Event {
    let mut event = Event::from_strings(
        "synthetic.updated".into(),
        entity.into(),
        tenant.into(),
        serde_json::json!({"synthetic": true}),
        None,
    )
    .unwrap();
    event.version = version;
    event
}

fn archive(directory: &Path, tenant: &str, events: &[Event]) {
    let storage = ParquetStorage::new(directory).unwrap();
    storage
        .write_atomic_parquet(tenant, "events-seed", events)
        .unwrap();
}

fn seeded() -> (TempDir, EventStore) {
    let directory = TempDir::new().unwrap();
    archive(
        directory.path(),
        TENANT,
        &[event(TENANT, ENTITY, 1), event(TENANT, ENTITY, 3)],
    );
    let store = EventStore::with_config(EventStoreConfig::with_persistence(directory.path()));
    (directory, store)
}

fn verify(store: &EventStore, tenant: &str, entity: &str) -> Result<(Vec<Event>, usize)> {
    store.query_retained_entity(tenant, entity, 1, &ReadScope::unrestricted())
}

#[test]
fn verified_archive_allows_cas_without_another_archive_scan() {
    let (_directory, mut store) = seeded();
    let (_, total) = verify(&store, TENANT, ENTITY).unwrap();
    assert_eq!(total, 2);
    store.strict_archive_limits.timeout = Duration::ZERO;
    assert!(matches!(
        store.ingest_with_expected_version(&event(TENANT, ENTITY, 0), Some(1)),
        Err(AllSourceError::VersionConflict {
            expected: 1,
            current: 3
        })
    ));
    assert_eq!(
        store
            .ingest_with_expected_version(&event(TENANT, ENTITY, 0), Some(3))
            .unwrap(),
        4
    );
}

#[test]
fn verified_archive_observes_versions_before_event_id_deduplication() {
    let directory = TempDir::new().unwrap();
    let first = event(TENANT, ENTITY, 1);
    let mut duplicate = first.clone();
    duplicate.version = 7;
    archive(directory.path(), TENANT, &[first, duplicate]);
    let mut store = EventStore::with_config(EventStoreConfig::with_persistence(directory.path()));
    let (_, total) = verify(&store, TENANT, ENTITY).unwrap();
    assert_eq!(total, 1);
    store.strict_archive_limits.timeout = Duration::ZERO;
    assert_eq!(store.get_entity_version(ENTITY), 7);
    assert_eq!(
        store
            .ingest_with_expected_version(&event(TENANT, ENTITY, 0), Some(7))
            .unwrap(),
        8
    );
}

#[test]
fn verified_archive_does_not_certify_another_tenant() {
    let (directory, mut store) = seeded();
    let other = "synthetic-other-tenant";
    archive(directory.path(), other, &[event(other, "other-entity", 9)]);
    verify(&store, TENANT, ENTITY).unwrap();
    store.strict_archive_limits.timeout = Duration::ZERO;
    let mut subscriber = store.subscribe_events();
    assert!(matches!(
        store.ingest_with_expected_version(&event(other, "other-entity", 0), Some(9)),
        Err(AllSourceError::StorageError(_))
    ));
    assert!(!store.version_index_tenants.contains_key(other));
    assert!(subscriber.try_recv().is_err());
}

#[test]
fn tolerant_hydration_does_not_certify_the_version_index() {
    let (_directory, mut store) = seeded();
    store.ensure_tenant_loaded(TENANT).unwrap();
    assert!(!store.version_index_tenants.contains_key(TENANT));
    store.strict_archive_limits.timeout = Duration::ZERO;
    assert!(matches!(
        store.ingest_with_expected_version(&event(TENANT, ENTITY, 0), Some(3)),
        Err(AllSourceError::StorageError(_))
    ));
}

#[test]
fn failed_strict_hydration_does_not_certify_the_version_index() {
    for failure in ["corrupt", "row cap", "elapsed", "unrepresentable version"] {
        let (directory, mut store) = seeded();
        match failure {
            "corrupt" => std::fs::write(
                directory.path().join(TENANT).join("events-corrupt.parquet"),
                b"unreadable archived history",
            )
            .unwrap(),
            "row cap" => store.strict_archive_limits.max_rows = 1,
            "elapsed" => store.strict_archive_limits.timeout = Duration::ZERO,
            "unrepresentable version" => {
                archive(directory.path(), TENANT, &[event(TENANT, ENTITY, -1)]);
            }
            _ => unreachable!(),
        }
        assert!(verify(&store, TENANT, ENTITY).is_err(), "{failure}");
        assert!(!store.tenant_loader.is_complete(TENANT), "{failure}");
        assert!(
            !store.version_index_tenants.contains_key(TENANT),
            "{failure}"
        );
        let mut subscriber = store.subscribe_events();
        assert!(
            store
                .ingest_with_expected_version(&event(TENANT, ENTITY, 0), Some(3))
                .is_err(),
            "{failure}"
        );
        assert!(subscriber.try_recv().is_err(), "{failure}");
    }
}

#[test]
fn cancelled_strict_hydration_does_not_certify_or_append() {
    let (_directory, store) = seeded();
    let cancellation = Arc::new(AtomicBool::new(true));
    assert!(
        store
            .query_retained_entity_cancellable(
                TENANT,
                ENTITY,
                1,
                &ReadScope::unrestricted(),
                Some(&cancellation),
            )
            .is_err()
    );
    assert!(!store.version_index_tenants.contains_key(TENANT));
    assert!(!store.tenant_loader.is_complete(TENANT));
    assert_eq!(store.total_events(), 0);
    cancellation.store(false, Ordering::Release);
    verify(&store, TENANT, ENTITY).unwrap();
    cancellation.store(true, Ordering::Release);
    let mut subscriber = store.subscribe_events();
    assert!(
        store
            .ingest_with_expected_version_cancellable(
                &event(TENANT, ENTITY, 0),
                Some(3),
                Some(&cancellation),
            )
            .is_err()
    );
    assert_eq!(store.total_events(), 2);
    assert!(subscriber.try_recv().is_err());
}

/// gh#321: a conditional append must not be priced by the tenant's history.
///
/// This archive holds more rows than the row budget admits, so a fold of every
/// entity in the tenant cannot complete. The targeted entity's own row groups
/// still fit, so the write must succeed — and must still refuse a stale
/// expectation, which is the only reason to read the archive at all.
#[test]
fn conditional_write_succeeds_on_a_tenant_too_large_to_fold_whole() {
    let directory = TempDir::new().unwrap();
    let crowd: Vec<Event> = (0..400)
        .map(|i| event(TENANT, &format!("crowd-entity-{i}"), i64::from(i % 7) + 1))
        .collect();
    let storage = ParquetStorage::new(directory.path()).unwrap();
    storage
        .write_atomic_parquet(TENANT, "events-crowd", &crowd)
        .unwrap();
    storage
        .write_atomic_parquet(TENANT, "events-target", &[event(TENANT, ENTITY, 5)])
        .unwrap();

    let mut store = EventStore::with_config(EventStoreConfig::with_persistence(directory.path()));
    store.strict_archive_limits.max_rows = 50;

    assert!(matches!(
        store.ingest_with_expected_version(&event(TENANT, ENTITY, 0), Some(4)),
        Err(AllSourceError::VersionConflict {
            expected: 4,
            current: 5
        })
    ));
    assert_eq!(
        store
            .ingest_with_expected_version(&event(TENANT, ENTITY, 0), Some(5))
            .unwrap(),
        6
    );
    assert!(
        !store.version_index_tenants.contains_key(TENANT),
        "resolving one entity must not certify the tenant"
    );
}

#[test]
fn verified_version_index_survives_event_cache_eviction() {
    let (_directory, mut store) = seeded();
    verify(&store, TENANT, ENTITY).unwrap();
    store.evict_tenant(TENANT);
    assert!(!store.is_tenant_loaded(TENANT));
    store.strict_archive_limits.timeout = Duration::ZERO;
    assert_eq!(store.get_entity_version(ENTITY), 3);
    assert_eq!(
        store
            .ingest_with_expected_version(&event(TENANT, ENTITY, 0), Some(3))
            .unwrap(),
        4
    );
}
