//! stream_sizes: a removal between lock chunks restarts the scan once, and a
//! second one marks the result incomplete.

use super::*;
use crate::store::EventStoreConfig;

const TENANT: &str = "tenant-sized";
const EVENTS: usize = STREAM_SIZE_CHUNK + 104;
const OTHERS: usize = 8;

fn event(entity: &str, tenant: &str) -> Event {
    Event::from_strings(
        "order.placed".into(),
        entity.into(),
        tenant.into(),
        serde_json::json!({ "n": 1 }),
        None,
    )
    .unwrap()
}

/// Other tenants' events sit ahead of the sized tenant's, so removing one
/// shifts every sized event left of the cursor.
fn store() -> EventStore {
    let store = EventStore::new();
    for i in 0..OTHERS {
        store
            .ingest(&event(&format!("other-{i}"), "tenant-other"))
            .unwrap();
    }
    for _ in 0..EVENTS {
        store.ingest(&event("sized", TENANT)).unwrap();
    }
    store
}

fn remove_entity(store: &EventStore, entity: &str) {
    let mut events = store.events.write();
    store.retain_events(&mut events, |e| e.entity_id_str() != entity);
}

#[test]
fn stream_sizes_without_removals_is_complete() {
    let sizes = store().stream_sizes(Some(TENANT)).unwrap();

    assert!(sizes.complete);
    assert_eq!(sizes.streams[0].event_count, EVENTS);
}

#[test]
fn stream_sizes_restarts_once_when_events_are_removed_mid_scan() {
    let store = store();
    let mut pauses = 0;

    let sizes = store
        .stream_sizes_between(Some(TENANT), || {
            if pauses == 0 {
                remove_entity(&store, "other-0");
            }
            pauses += 1;
        })
        .unwrap();

    assert!(sizes.complete);
    assert_eq!(sizes.streams.len(), 1);
    assert_eq!(sizes.streams[0].event_count, EVENTS);
    assert_eq!(pauses, 2);
}

#[test]
fn stream_sizes_marks_the_result_incomplete_when_removals_keep_coming() {
    let store = store();
    let mut pauses = 0;

    let sizes = store
        .stream_sizes_between(Some(TENANT), || {
            remove_entity(&store, &format!("other-{pauses}"));
            pauses += 1;
        })
        .unwrap();

    assert!(!sizes.complete);
}

/// A persisted store whose tenant loads give up after 20 ms waiting for the
/// tenant's load lock, so a held lock stands in for a failing hydration.
fn persisted_store(directory: &std::path::Path) -> EventStore {
    let mut store = EventStore::with_config(EventStoreConfig::with_persistence(directory));
    store.tenant_loader = Arc::new(TenantLoader::with_timeout(
        std::time::Duration::from_millis(20),
    ));
    for i in 0..OTHERS {
        store
            .ingest(&event(&format!("other-{i}"), "tenant-other"))
            .unwrap();
    }
    for _ in 0..EVENTS {
        store.ingest(&event("sized", TENANT)).unwrap();
    }
    store.flush_storage().unwrap();
    store
}

/// Holds `tenant`'s load lock for the rest of the test.
fn block_tenant_loads(store: &EventStore, tenant: &str) {
    let lock = store.tenant_loader.lock_for(tenant);
    std::mem::forget(lock.lock());
    std::mem::forget(lock);
}

#[tokio::test]
async fn stream_sizes_of_a_cold_tenant_that_fails_to_hydrate_is_an_error() {
    let directory = tempfile::TempDir::new().unwrap();
    let store = persisted_store(directory.path());
    block_tenant_loads(&store, "tenant-cold");

    assert!(store.stream_sizes(Some("tenant-cold")).is_err());
}

#[tokio::test]
async fn stream_sizes_retry_reloads_a_tenant_evicted_during_the_first_pass() {
    let directory = tempfile::TempDir::new().unwrap();
    let store = persisted_store(directory.path());
    let mut pauses = 0;

    let sizes = store
        .stream_sizes_between(Some(TENANT), || {
            if pauses == 0 {
                store.evict_tenant(TENANT);
                assert!(!store.is_tenant_loaded(TENANT));
            }
            pauses += 1;
        })
        .unwrap();

    assert!(sizes.complete);
    assert_eq!(sizes.streams.len(), 1);
    assert_eq!(sizes.streams[0].event_count, EVENTS);
}

#[tokio::test]
async fn stream_sizes_is_an_error_when_an_evicted_tenant_cannot_be_reloaded() {
    let directory = tempfile::TempDir::new().unwrap();
    let store = persisted_store(directory.path());
    let mut pauses = 0;

    let sizes = store.stream_sizes_between(Some(TENANT), || {
        if pauses == 0 {
            store.evict_tenant(TENANT);
            block_tenant_loads(&store, TENANT);
        }
        pauses += 1;
    });

    assert!(sizes.is_err(), "an evicted tenant is not an empty tenant");
}

#[tokio::test]
async fn stream_sizes_is_incomplete_when_the_tenant_leaves_during_the_retry() {
    let directory = tempfile::TempDir::new().unwrap();
    let store = persisted_store(directory.path());
    let mut pauses = 0;

    let sizes = store
        .stream_sizes_between(Some(TENANT), || {
            store.evict_tenant(TENANT);
            pauses += 1;
        })
        .unwrap();

    assert!(!sizes.complete);
}

#[tokio::test]
async fn stream_sizes_is_not_complete_from_a_scan_that_raced_an_eviction_after_hydration() {
    let directory = tempfile::TempDir::new().unwrap();
    let store = persisted_store(directory.path());
    let mut evicted = false;
    let mut reloaded = false;

    let sizes = store
        .stream_sizes_racing(
            Some(TENANT),
            || {
                if !evicted {
                    store.evict_tenant(TENANT);
                    assert!(!store.is_tenant_loaded(TENANT));
                    evicted = true;
                }
            },
            || {
                if !reloaded {
                    store.ensure_tenant_loaded(TENANT).unwrap();
                    reloaded = true;
                }
            },
        )
        .unwrap();

    assert!(
        sizes.complete,
        "the retry reads the reloaded tenant: {sizes:?}"
    );
    assert_eq!(sizes.streams.len(), 1, "never zero streams marked complete");
    assert_eq!(sizes.streams[0].event_count, EVENTS);
}

#[tokio::test]
async fn plain_stream_listing_that_raced_an_eviction_after_hydration_is_incomplete() {
    let directory = tempfile::TempDir::new().unwrap();
    let store = persisted_store(directory.path());

    let listed = store
        .list_streams_for_tenant_seamed(TENANT, &mut || store.evict_tenant(TENANT), &mut || {
            store.ensure_tenant_loaded(TENANT).unwrap();
        })
        .unwrap();

    assert!(
        !listed.complete,
        "a read of an evicted tenant is not complete: {listed:?}"
    );
    assert!(store.list_streams_for_tenant(TENANT).unwrap().complete);
}

#[tokio::test]
async fn a_tenant_read_that_raced_an_eviction_after_hydration_is_not_resident() {
    let directory = tempfile::TempDir::new().unwrap();
    let store = persisted_store(directory.path());

    let (_, resident) = store
        .read_tenant(
            TENANT,
            &mut || store.evict_tenant(TENANT),
            &mut || store.ensure_tenant_loaded(TENANT).unwrap(),
            || store.collect_stats_for_tenant(TENANT),
        )
        .unwrap();

    assert!(!resident);
    assert_eq!(store.stats_for_tenant(TENANT).unwrap().total_events, EVENTS);
}

async fn listing_status(
    store: Arc<EventStore>,
    with_bytes: Option<bool>,
) -> axum::http::StatusCode {
    use axum::{
        extract::{Query, State},
        response::IntoResponse,
    };
    crate::infrastructure::web::api::list_streams(
        crate::infrastructure::security::middleware::OptionalAuth(None),
        State(store),
        Query(crate::infrastructure::web::api::ListStreamsParams {
            tenant_id: Some("tenant-cold".to_owned()),
            limit: None,
            offset: None,
            with_bytes,
            sort: None,
        }),
    )
    .await
    .into_response()
    .status()
}

#[tokio::test]
async fn stream_listing_of_a_cold_tenant_that_fails_to_hydrate_is_503() {
    let directory = tempfile::TempDir::new().unwrap();
    let store = Arc::new(persisted_store(directory.path()));
    block_tenant_loads(&store, "tenant-cold");

    assert_eq!(
        listing_status(Arc::clone(&store), Some(true)).await,
        axum::http::StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        listing_status(store, None).await,
        axum::http::StatusCode::SERVICE_UNAVAILABLE
    );
}

#[tokio::test]
async fn stats_of_a_cold_tenant_that_fails_to_hydrate_is_503() {
    use axum::{
        extract::{Query, State},
        response::IntoResponse,
    };
    let directory = tempfile::TempDir::new().unwrap();
    let store = Arc::new(persisted_store(directory.path()));
    block_tenant_loads(&store, "tenant-cold");

    let response = crate::infrastructure::web::api::get_stats(
        State(store),
        Query(crate::infrastructure::web::api::StatsParams {
            tenant_id: Some("tenant-cold".to_owned()),
        }),
    )
    .await
    .into_response();

    assert_eq!(
        response.status(),
        axum::http::StatusCode::SERVICE_UNAVAILABLE,
        "a tenant that could not be read is not a tenant with zero events"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["error"], "store_unavailable");
}

#[tokio::test]
async fn event_types_of_a_cold_tenant_that_fails_to_hydrate_is_503() {
    use axum::{
        extract::{Query, State},
        response::IntoResponse,
    };
    let directory = tempfile::TempDir::new().unwrap();
    let store = Arc::new(persisted_store(directory.path()));
    block_tenant_loads(&store, "tenant-cold");

    let status = crate::infrastructure::web::api::list_event_types(
        crate::infrastructure::security::middleware::OptionalAuth(None),
        State(store),
        Query(crate::infrastructure::web::api::ListEventTypesParams {
            tenant_id: Some("tenant-cold".to_owned()),
            limit: None,
            offset: None,
        }),
    )
    .await
    .into_response()
    .status();

    assert_eq!(status, axum::http::StatusCode::SERVICE_UNAVAILABLE);
}

#[test]
fn stream_sizes_counts_every_stream_when_new_stream_ids_overflow_a_chunk() {
    let store = EventStore::new();
    let streams = STREAM_SIZE_CHUNK + 500;
    let long = "k".repeat(96);
    for n in 0..streams {
        store
            .ingest(&event(&format!("{long}:{n}"), TENANT))
            .unwrap();
    }
    store.ingest(&event(&format!("{long}:0"), TENANT)).unwrap();

    let sizes = store.stream_sizes(Some(TENANT)).unwrap();

    assert!(sizes.complete);
    assert_eq!(sizes.streams.len(), streams);
    let first = sizes
        .streams
        .iter()
        .find(|s| s.stream_id == format!("{long}:0"))
        .unwrap();
    assert_eq!(first.event_count, 2);
    assert!(sizes.streams.iter().all(|s| s.estimated_bytes.is_some()));
}

#[test]
fn retain_events_counts_only_a_removal_that_removed_something() {
    let store = store();

    remove_entity(&store, "no-such-entity");
    assert_eq!(
        store
            .events_removed
            .load(std::sync::atomic::Ordering::Relaxed),
        0
    );
    remove_entity(&store, "other-0");
    assert_eq!(
        store
            .events_removed
            .load(std::sync::atomic::Ordering::Relaxed),
        1
    );
}
