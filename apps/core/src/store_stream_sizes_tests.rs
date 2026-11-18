//! stream_sizes: a removal between lock chunks restarts the scan once, and a
//! second one marks the result incomplete.

use super::*;

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
    let sizes = store().stream_sizes(Some(TENANT));

    assert!(sizes.complete);
    assert_eq!(sizes.streams[0].event_count, EVENTS);
}

#[test]
fn stream_sizes_restarts_once_when_events_are_removed_mid_scan() {
    let store = store();
    let mut pauses = 0;

    let sizes = store.stream_sizes_between(Some(TENANT), || {
        if pauses == 0 {
            remove_entity(&store, "other-0");
        }
        pauses += 1;
    });

    assert!(sizes.complete);
    assert_eq!(sizes.streams.len(), 1);
    assert_eq!(sizes.streams[0].event_count, EVENTS);
    assert_eq!(pauses, 2);
}

#[test]
fn stream_sizes_marks_the_result_incomplete_when_removals_keep_coming() {
    let store = store();
    let mut pauses = 0;

    let sizes = store.stream_sizes_between(Some(TENANT), || {
        remove_entity(&store, &format!("other-{pauses}"));
        pauses += 1;
    });

    assert!(!sizes.complete);
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
