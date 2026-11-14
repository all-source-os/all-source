//! The embedded open path (`EmbeddedCore::open`) hydrates the archive into
//! memory. The resident ledger tracks live heap within 10% (I2) after that
//! open, and returns to zero when the core is dropped.
//!
//! Gated on `embedded`: `cargo test -p allsource-core --features embedded --test ledger_owns_heap_embedded`.
#![cfg(feature = "embedded")]

use allsource_core::{
    domain::entities::Event,
    embedded::{Config, EmbeddedCore},
    infrastructure::{
        memory::ledger::resident,
        persistence::{compaction::CompactionConfig, wal::WALConfig},
    },
    store::{EventStore, EventStoreConfig},
};
use serde_json::json;
use std::alloc::System;

#[global_allocator]
static COUNTED: cap::Cap<System> = cap::Cap::new(System, usize::MAX);

const EVENTS: usize = 800;
const PAD_BYTES: usize = 3_600;
const TENANT: &str = "tenant-a";

fn event(entity: usize, seq: usize) -> Event {
    Event::from_strings(
        "order.placed".into(),
        format!("order-{entity:04}"),
        TENANT.into(),
        json!({"seq": seq, "pad": "x".repeat(PAD_BYTES)}),
        None,
    )
    .unwrap()
}

/// The layout `EmbeddedCore::open` reads: `<data_dir>/storage` and `<data_dir>/wal`.
fn seed(data_dir: &std::path::Path) {
    let store = EventStore::with_config(EventStoreConfig::production(
        data_dir.join("storage"),
        data_dir.join("wal"),
        WALConfig::default(),
        CompactionConfig::default(),
    ));
    for i in 0..EVENTS {
        store.ingest(&event(i % 40, i)).unwrap();
    }
    store.flush_storage().unwrap();
}

/// `|live − ledger| ≤ 0.10 × live`, with live measured from the start of the test.
fn check(live_start: usize, step: &str) {
    let live = COUNTED.allocated().saturating_sub(live_start) as i64;
    let ledger = resident().total_bytes();
    let gap = (live - ledger).unsigned_abs();
    assert!(
        gap * 10 <= live as u64,
        "{step}: live {live} B, ledger {ledger} B, gap {gap} B is over 10%"
    );
}

#[tokio::test]
async fn ledger_owns_heap_across_embedded_open() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path());
    assert_eq!(resident().total_bytes(), 0, "ledger starts empty");
    let live_start = COUNTED.allocated();

    let core = EmbeddedCore::open(Config::builder().data_dir(dir.path()).build().unwrap())
        .await
        .expect("embedded open over a seeded archive");
    assert!(
        resident().total_bytes() > 0,
        "embedded open hydrated nothing into the ledger"
    );
    check(live_start, "embedded open");

    drop(core);
    assert_eq!(
        resident().total_bytes(),
        0,
        "ledger holds bytes after the embedded core is dropped"
    );
}
