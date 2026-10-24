use super::*;
use crate::store::EventStoreConfig;
use std::sync::atomic::Ordering;

#[tokio::test]
async fn prefix_reads_merge_cold_archive_and_live_updates_without_hydration() {
    let directory = tempfile::TempDir::new().unwrap();
    let store = EventStore::with_config(EventStoreConfig::with_persistence(directory.path()));
    for (tenant, kind, entity) in [
        ("synthetic", "auth.user.created", "user"),
        ("other", "auth.user.created", "other-user"),
        ("synthetic", "unrelated.created", "noise"),
    ] {
        store
            .ingest(
                &Event::from_strings(
                    kind.into(),
                    entity.into(),
                    tenant.into(),
                    serde_json::json!({"id":entity}),
                    None,
                )
                .unwrap(),
            )
            .unwrap();
    }
    store.flush_storage().unwrap();
    store.evict_tenant("synthetic");
    let update = Event::from_strings(
        "auth.user.updated".into(),
        "user".into(),
        "synthetic".into(),
        serde_json::json!({"_deleted":true}),
        None,
    )
    .unwrap();
    store.ingest(&update).unwrap();
    let request: QueryEventsRequest = serde_json::from_value(
        serde_json::json!({"tenant_id":"synthetic", "event_type_prefix":"auth.user.", "limit":1}),
    )
    .unwrap();
    let resident = store.total_events();
    let (page, count) = store
        .query_retained_prefix(&request, 0, true, Arc::new(AtomicBool::new(false)))
        .unwrap();
    assert_eq!(count, 2);
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].id, update.id);
    let (older, total) = store
        .query_retained_prefix(&request, 1, true, Arc::new(AtomicBool::new(false)))
        .unwrap();
    assert_eq!(total, 2);
    assert_eq!(older[0].event_type_str(), "auth.user.created");
    assert_eq!(store.total_events(), resident);
    assert!(!store.tenant_loader.is_complete("synthetic"));
    let cancel = Arc::new(AtomicBool::new(false));
    cancel.store(true, Ordering::Release);
    assert!(
        store
            .query_retained_prefix(&request, 0, true, cancel)
            .unwrap_err()
            .to_string()
            .contains("cancelled")
    );
}

#[tokio::test]
async fn prefix_read_refuses_corrupt_archive_instead_of_returning_partial_auth_state() {
    let directory = tempfile::TempDir::new().unwrap();
    let store = EventStore::with_config(EventStoreConfig::with_persistence(directory.path()));
    store
        .ingest(
            &Event::from_strings(
                "auth.user.created".into(),
                "user".into(),
                "synthetic".into(),
                serde_json::json!({}),
                None,
            )
            .unwrap(),
        )
        .unwrap();
    store.flush_storage().unwrap();
    store.evict_tenant("synthetic");
    fn corrupt(path: &std::path::Path) -> usize {
        let mut count = 0;
        for entry in std::fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                count += corrupt(&path);
            } else if path.extension().is_some_and(|v| v == "parquet") {
                std::fs::write(path, b"invalid").unwrap();
                count += 1;
            }
        }
        count
    }
    assert!(corrupt(directory.path()) > 0);
    let request = serde_json::from_value(
        serde_json::json!({"tenant_id":"synthetic", "event_type_prefix":"auth.user.", "limit":100}),
    )
    .unwrap();
    assert!(
        store
            .query_retained_prefix(&request, 0, true, Arc::new(AtomicBool::new(false)))
            .is_err()
    );
}
