//! The search owner charges every event it indexes: the event's resident bytes
//! and its embedding, and releases them when the search service is dropped.
#![cfg(feature = "server")]

use allsource_core::{
    domain::entities::Event,
    infrastructure::{
        memory::ledger::{Owner, resident},
        search::{EventSearch, QueryEmbedder},
    },
    store::EventStore,
};
use serde_json::json;
use std::{alloc::System, sync::Arc};

#[global_allocator]
static COUNTED: cap::Cap<System> = cap::Cap::new(System, usize::MAX);

const EVENTS: usize = 64;

struct FixedEmbedder;

impl QueryEmbedder for FixedEmbedder {
    fn embed(&self, text: &str) -> allsource_core::error::Result<Vec<f32>> {
        let len = text.len() as f32;
        Ok(vec![len, 1.0, 0.5, 0.25])
    }
}

fn event(seq: usize) -> Event {
    Event::from_strings(
        "order.placed".into(),
        format!("order-{seq:04}"),
        "tenant-a".into(),
        json!({"seq": seq, "note": format!("order {seq} shipped to the warehouse")}),
        None,
    )
    .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn search_charges_indexed_events_and_releases_on_drop() {
    assert_eq!(
        resident().owner_bytes(Owner::Search),
        0,
        "search owner starts empty"
    );

    let store = Arc::new(EventStore::new());
    let search = EventSearch::new(Arc::clone(&store)).unwrap();
    search.set_embedder(Arc::new(FixedEmbedder));

    let events: Vec<Event> = (0..EVENTS).map(event).collect();
    let expected: u64 = events.iter().map(Event::resident_bytes).sum();
    for event in &events {
        store.ingest(event).unwrap();
    }

    let indexed = search.catch_up().await.unwrap();
    assert_eq!(indexed, EVENTS, "catch_up indexes every ingested event");

    let charged = resident().owner_bytes(Owner::Search) as u64;
    assert!(
        charged >= expected,
        "search owner holds {charged} B, below the {expected} B of indexed events"
    );

    drop(search);
    assert_eq!(
        resident().owner_bytes(Owner::Search),
        0,
        "search owner keeps bytes after the service is dropped"
    );
    drop(store);
}
