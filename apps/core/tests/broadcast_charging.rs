//! The broadcast owner stays within the two-sided 10% bound (I2) in four ring
//! configurations: store ring only, websocket ring only, both, and a websocket
//! receiver that connects and drops during the run.
#![cfg(feature = "server")]

use allsource_core::{
    domain::entities::Event, infrastructure::memory::ledger::resident, store::EventStore,
};
use serde_json::json;
use std::alloc::System;

#[global_allocator]
static COUNTED: cap::Cap<System> = cap::Cap::new(System, usize::MAX);

/// Well above both ring capacities (1,024 and 1,000), so the overlap of an
/// `Arc` sent to both rings is a small share of live heap (design I2).
const EVENTS: usize = 20_000;
const PAD_BYTES: usize = 3_600;

#[derive(Clone, Copy, Debug)]
enum Ring {
    StoreOnly,
    WebSocketOnly,
    Both,
    ConnectAndDrop,
}

fn event(seq: usize) -> Event {
    Event::from_strings(
        "order.placed".into(),
        format!("order-{:04}", seq % 40),
        "tenant-a".into(),
        json!({"seq": seq, "pad": "x".repeat(PAD_BYTES)}),
        None,
    )
    .unwrap()
}

/// `|live − ledger| ≤ 0.10 × live`, with live measured from `live_start`.
fn check(ring: Ring, live_start: usize, step: &str) {
    let live = COUNTED.allocated().saturating_sub(live_start) as i64;
    let ledger = resident().total_bytes();
    let gap = (live - ledger).unsigned_abs();
    assert!(
        gap * 10 <= live as u64,
        "{ring:?} {step}: live {live} B, ledger {ledger} B, gap {gap} B is over 10%"
    );
}

#[test]
fn broadcast_charging() {
    assert_eq!(resident().total_bytes(), 0, "ledger starts empty");
    let live_start = COUNTED.allocated();

    for ring in [
        Ring::StoreOnly,
        Ring::WebSocketOnly,
        Ring::Both,
        Ring::ConnectAndDrop,
    ] {
        let store = EventStore::new();
        let mut store_rx = None;
        let mut ws_rx = None;
        match ring {
            Ring::StoreOnly => store_rx = Some(store.subscribe_events()),
            Ring::WebSocketOnly => ws_rx = Some(store.websocket_manager().subscribe_events()),
            Ring::Both => {
                store_rx = Some(store.subscribe_events());
                ws_rx = Some(store.websocket_manager().subscribe_events());
            }
            Ring::ConnectAndDrop => {}
        }

        for seq in 0..EVENTS {
            if matches!(ring, Ring::ConnectAndDrop) {
                if seq == EVENTS / 4 {
                    ws_rx = Some(store.websocket_manager().subscribe_events());
                }
                if seq == EVENTS / 2 {
                    ws_rx = None;
                }
            }
            store.ingest(&event(seq)).unwrap();
        }
        check(ring, live_start, "after ingest");

        drop(store_rx);
        drop(ws_rx);
        store.ingest(&event(EVENTS)).unwrap();
        check(ring, live_start, "after receivers dropped");

        drop(store);
        assert_eq!(
            resident().total_bytes(),
            0,
            "{ring:?}: ledger holds bytes after store drop"
        );
    }
}
