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
    // The resolve is bounded by compressed bytes, not rows: its peak memory is
    // one batch, and a row ceiling would bar a large file from ever folding.
    for failure in ["corrupt", "byte cap", "elapsed", "unrepresentable version"] {
        let (directory, mut store) = seeded();
        match failure {
            "corrupt" => std::fs::write(
                directory.path().join(TENANT).join("events-corrupt.parquet"),
                b"unreadable archived history",
            )
            .unwrap(),
            "byte cap" => store.strict_archive_limits.max_compressed_bytes = 1,
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
        // "unrepresentable version" is the one case where the resolve itself
        // succeeds; the write is then refused by the version check, not by the
        // index, so certifying the entity is correct there.
        if failure != "unrepresentable version" {
            assert!(
                !store.version_index_entities.contains_key(ENTITY),
                "a resolve that could not finish must not certify the entity: {failure}"
            );
        }
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
/// A fold that runs out of time must leave the files it did fold recorded, so
/// the attempt after it does strictly less work. Without that, a tenant whose
/// archive cannot be folded inside one budget is refused conditional writes
/// for good, which is what locked a production tenant out.
#[test]
fn an_exhausted_resolve_persists_progress_and_a_retry_completes() {
    let directory = TempDir::new().unwrap();
    let storage = ParquetStorage::new(directory.path()).unwrap();
    for i in 0..12 {
        storage
            .write_atomic_parquet(
                TENANT,
                &format!("events-crowd-{i}"),
                &[event(TENANT, &format!("crowd-entity-{i}"), 2)],
            )
            .unwrap();
    }
    storage
        .write_atomic_parquet(TENANT, "events-target", &[event(TENANT, ENTITY, 5)])
        .unwrap();

    let mut store = EventStore::with_config(EventStoreConfig::with_persistence(directory.path()));
    // Room for at least one of these files per attempt, never all thirteen.
    store.strict_archive_limits.max_compressed_bytes = 4096;

    let mut refusals = 0;
    let mut resolved = None;
    for _ in 0..400 {
        match store.ingest_with_expected_version(&event(TENANT, ENTITY, 0), Some(5)) {
            Err(AllSourceError::ArchiveIndexIncomplete { remaining }) => {
                assert!(remaining > 0);
                refusals += 1;
            }
            other => {
                resolved = Some(other);
                break;
            }
        }
    }

    assert!(
        refusals > 0,
        "the first attempts should have run out of time"
    );
    assert_eq!(
        resolved
            .expect("a retry must eventually complete the fold")
            .unwrap(),
        6,
        "after {refusals} resumed attempts"
    );
}

/// A refusal while the manifest is incomplete is a retry, not a failure: the
/// caller cannot tell "not ready yet" from "broken" otherwise, and the two
/// want opposite responses.
#[test]
fn an_incomplete_resolve_is_reported_as_retryable() {
    let (_directory, mut store) = seeded();
    store.strict_archive_limits.timeout = Duration::ZERO;
    let error = store
        .ingest_with_expected_version(&event(TENANT, ENTITY, 0), Some(3))
        .unwrap_err();
    assert!(error.is_retryable(), "{error}");
}

/// A flush already knows the versions it wrote, so a conditional write that
/// follows it must not read those files back.
///
/// Proven by making every archive file unreadable after the flush: a resolve
/// that folds them would fail, so succeeding means the manifest answered. The
/// directory is still enumerated — that is how a resolve learns whether any
/// file is unaccounted for — so this pins which work the manifest removes.
#[test]
fn a_flushed_version_is_recorded_without_re_reading_the_archive() {
    let directory = TempDir::new().unwrap();
    let store = EventStore::with_config(EventStoreConfig::with_persistence(directory.path()));
    store
        .ingest_with_expected_version(&event(TENANT, ENTITY, 0), Some(0))
        .unwrap();
    store.flush_storage().unwrap();

    let mut archived = 0;
    for entry in walk_parquet(&directory.path().join(TENANT)) {
        std::fs::write(&entry, b"unreadable archived history").unwrap();
        archived += 1;
    }
    assert!(
        archived > 0,
        "the flush should have written an archive file"
    );

    let reopened = EventStore::with_config(EventStoreConfig::with_persistence(directory.path()));
    assert_eq!(
        reopened
            .ingest_with_expected_version(&event(TENANT, ENTITY, 0), Some(1))
            .unwrap(),
        2,
        "the manifest written at flush should answer without folding {archived} file(s)"
    );
}

fn walk_parquet(root: &Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "parquet") {
                found.push(path);
            }
        }
    }
    found
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
