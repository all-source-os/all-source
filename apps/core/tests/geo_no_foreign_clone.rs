//! A geo query over several tenants clones only the caller's events. The resolver
//! runs once per candidate; each clone is a `Some` it returns. The handler's path,
//! `EventStore::geo_query_for_tenant`, reports the resident bytes it cloned.

use allsource_core::{
    Event,
    query::{BoundingBox, GeoQueryRequest, RadiusQuery, execute_geo_query},
    store::{EventStore, EventStoreConfig},
};
use std::cell::Cell;
use uuid::Uuid;

fn located(entity: &str, tenant: &str, lat: f64, lng: f64) -> Event {
    Event::from_strings(
        "location.update".into(),
        entity.into(),
        tenant.into(),
        serde_json::json!({"action": "checkin"}),
        Some(serde_json::json!({"location": {"lat": lat, "lng": lng}})),
    )
    .unwrap()
}

#[test]
fn geo_no_foreign_clone() {
    let (config, _mode) =
        EventStoreConfig::from_env_vars(None, None, None, None, None, None, None, None);
    let store = EventStore::with_config(config);
    let events = vec![
        located("a-1", "tenant-a", 40.70, -74.00),
        located("b-1", "tenant-b", 40.71, -74.01),
        located("a-2", "tenant-a", 40.72, -74.02),
        located("b-2", "tenant-b", 40.73, -74.03),
        located("b-3", "tenant-b", 40.74, -74.04),
        located("a-3", "tenant-a", 40.75, -74.05),
        located("b-4", "tenant-b", 40.76, -74.06),
        located("b-5", "tenant-b", 40.77, -74.07),
    ];
    store.ingest_batch(events.clone()).unwrap();

    let request = GeoQueryRequest {
        bbox: Some(BoundingBox {
            min_lat: 40.0,
            min_lng: -75.0,
            max_lat: 41.0,
            max_lng: -73.0,
        }),
        radius: None,
        event_type: None,
        limit: None,
        sort_by_distance: None,
    };
    let own_bytes: u64 = events
        .iter()
        .filter(|e| e.tenant_id_str() == "tenant-a")
        .map(|e| {
            store
                .get_event_by_id(&e.id())
                .unwrap()
                .unwrap()
                .resident_bytes()
        })
        .sum();
    let all_bytes: u64 = events.iter().map(Event::resident_bytes).sum();
    assert!(own_bytes < all_bytes);

    let (handler_results, cloned_bytes) = store.geo_query_for_tenant(&request, "tenant-a");
    assert_eq!(
        cloned_bytes, own_bytes,
        "the handler path clones exactly the caller's events"
    );
    assert_eq!(handler_results.len(), 3);

    let geo_index = store.geo_index();
    let calls = Cell::new(0usize);
    let clones = Cell::new(0usize);
    let resolve = |id: Uuid| {
        calls.set(calls.get() + 1);
        let event = store.resident_event_for_tenant(&id, "tenant-a");
        if event.is_some() {
            clones.set(clones.get() + 1);
        }
        event
    };

    let results = execute_geo_query(&resolve, &geo_index, &request);

    assert_eq!(calls.get(), 8, "one resolver call per geo candidate");
    assert_eq!(clones.get(), 3, "only tenant-a events are cloned");
    let entities: Vec<&str> = results
        .iter()
        .map(|r| r.event["entity_id"].as_str().unwrap())
        .collect();
    assert_eq!(entities, ["a-1", "a-2", "a-3"], "results keep ingest order");

    distance_sort_clones_at_most_limit();
}

/// A distance-sorted query clones at most `limit` events and returns the nearest,
/// ties kept in ingest order.
fn distance_sort_clones_at_most_limit() {
    let (config, _mode) =
        EventStoreConfig::from_env_vars(None, None, None, None, None, None, None, None);
    let store = EventStore::with_config(config);
    store
        .ingest_batch(vec![
            located("far", "tenant-a", 40.90, -74.00),
            located("near-tie-1", "tenant-a", 40.71, -74.00),
            located("mid", "tenant-a", 40.80, -74.00),
            located("nearest", "tenant-a", 40.7001, -74.00),
            located("near-tie-2", "tenant-a", 40.71, -74.00),
            located("farther", "tenant-a", 40.95, -74.00),
        ])
        .unwrap();
    let request = GeoQueryRequest {
        bbox: None,
        radius: Some(RadiusQuery {
            center_lat: 40.70,
            center_lng: -74.00,
            radius_km: 100.0,
        }),
        event_type: None,
        limit: Some(3),
        sort_by_distance: Some(true),
    };

    let geo_index = store.geo_index();
    let clones = Cell::new(0usize);
    let resolve = |id: Uuid| {
        let event = store.resident_event_for_tenant(&id, "tenant-a");
        if event.is_some() {
            clones.set(clones.get() + 1);
        }
        event
    };
    let results = execute_geo_query(&resolve, &geo_index, &request);

    assert_eq!(clones.get(), 3, "resolution stops at the limit");
    let entities: Vec<&str> = results
        .iter()
        .map(|r| r.event["entity_id"].as_str().unwrap())
        .collect();
    assert_eq!(entities, ["nearest", "near-tie-1", "near-tie-2"]);
}
