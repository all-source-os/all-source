//! The memory gauges split RSS into live heap and allocator-retained bytes,
//! and attribute the live part to the store's in-memory structures.
use allsource_core::{
    domain::entities::Event,
    infrastructure::observability::{
        MetricsRegistry,
        heap::{StructureSize, json_heap_bytes},
    },
    store::EventStore,
};
use serde_json::json;

#[global_allocator]
static COUNTED: cap::Cap<std::alloc::System> = cap::Cap::new(std::alloc::System, usize::MAX);

#[test]
fn global_allocator_count_follows_a_live_allocation() {
    let before = COUNTED.allocated();
    let block = vec![0u8; 8 << 20];
    assert!(COUNTED.allocated() >= before + (8 << 20) - (1 << 20));
    drop(block);
    assert!(COUNTED.allocated() < before + (1 << 20));
}

#[test]
fn json_estimate_grows_with_content() {
    let small = json_heap_bytes(&json!({"a": "x"}));
    let large = json_heap_bytes(&json!({"a": "x".repeat(10_000)}));
    assert!(large >= small + 9_999);
    assert_eq!(json_heap_bytes(&json!(42)), 0);
}

fn event(entity: &str, body: &str) -> Event {
    Event::from_strings(
        "heap.v1.sampled".into(),
        entity.into(),
        "synthetic-heap-tenant".into(),
        json!({"body": body}),
        None,
    )
    .unwrap()
}

fn find<'a>(sizes: &'a [StructureSize], name: &str) -> &'a StructureSize {
    sizes
        .iter()
        .find(|size| size.name == name)
        .unwrap_or_else(|| panic!("no {name} in {sizes:?}"))
}

#[test]
fn store_reports_events_and_indexes_by_size() {
    let store = EventStore::new();
    let body = "y".repeat(2_000);
    for i in 0..50 {
        store
            .ingest(&event(&format!("entity-{}", i % 5), &body))
            .unwrap();
    }
    let sizes = store.resident_structures();

    let events = find(&sizes, "events");
    assert_eq!(events.entries, 50);
    assert!(events.bytes.unwrap() >= 50 * 2_000);

    assert_eq!(find(&sizes, "entity_index").entries, 5);
    assert_eq!(find(&sizes, "type_index").entries, 1);
    assert_eq!(find(&sizes, "id_index").entries, 50);
    assert!(find(&sizes, "entity_index").bytes.unwrap() > 0);
    assert_eq!(find(&sizes, "geo_index").bytes, None);
}

#[test]
fn heap_gauges_split_rss_and_skip_missing_estimates() {
    let metrics = MetricsRegistry::new();
    let structures = [
        StructureSize {
            name: "events",
            entries: 3,
            bytes: Some(900),
        },
        StructureSize {
            name: "geo_index",
            entries: 2,
            bytes: None,
        },
    ];
    metrics.record_heap(4_000, Some(10_000), &structures);
    assert_eq!(metrics.heap_live_bytes.get(), 4_000);
    assert_eq!(metrics.allocator_retained_bytes.get(), 6_000);
    assert_eq!(
        metrics
            .resident_structure_entries
            .with_label_values(&["geo_index"])
            .get(),
        2
    );
    assert_eq!(
        metrics
            .resident_structure_bytes
            .with_label_values(&["events"])
            .get(),
        900
    );
    let encoded = metrics.encode().unwrap();
    assert!(!encoded.contains("allsource_resident_structure_bytes{structure=\"geo_index\"}"));

    metrics.record_heap(4_000, None, &[]);
    assert_eq!(metrics.allocator_retained_bytes.get(), 0);
}
