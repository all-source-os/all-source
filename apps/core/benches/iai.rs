//! iai-callgrind deterministic instruction-count benchmarks for merge gating.
//!
//! Mirrors the hot paths from `performance_benchmarks.rs` (criterion) but
//! produces instruction counts via callgrind (deterministic, zero variance)
//! so CI can enforce sub-3% regression gates that criterion's wall-clock
//! variance (~3%) would miss.
//!
//! Requires valgrind — Linux only. macOS contributors should rely on the
//! criterion bench locally; iai-callgrind runs in CI on `ubuntu-latest`.
//!
//! Build (even on macOS): `cargo check --bench iai`
//! Run (Linux + valgrind): `cargo bench --bench iai`

use allsource_core::{QueryEventsRequest, domain::entities::Event, store::EventStore};
use iai_callgrind::{library_benchmark, library_benchmark_group, main};
use serde_json::json;

fn build_event(i: usize) -> Event {
    Event::from_strings(
        "benchmark.event".to_string(),
        format!("entity-{}", i % 100),
        "default".to_string(),
        json!({ "index": i, "data": "payload" }),
        None,
    )
    .unwrap()
}

fn build_events(n: usize) -> Vec<Event> {
    (0..n).map(build_event).collect()
}

#[library_benchmark]
fn bench_ingest_single() {
    let store = EventStore::new();
    let event = build_event(0);
    store.ingest(std::hint::black_box(&event)).unwrap();
    std::hint::black_box(());
}

#[library_benchmark]
#[bench::small(100)]
#[bench::medium(1_000)]
#[bench::large(10_000)]
fn bench_ingest_batch(n: usize) {
    let events = build_events(n);
    let store = EventStore::new();
    for e in &events {
        store.ingest(e).unwrap();
    }
    std::hint::black_box(());
}

#[library_benchmark]
fn bench_query_after_ingest() {
    let store = EventStore::new();
    for e in &build_events(1_000) {
        store.ingest(e).unwrap();
    }
    let req = QueryEventsRequest {
        as_of_valid: None,
        since_valid: None,
        until_valid: None,
        entity_id: Some("entity-0".to_string()),
        ..Default::default()
    };
    std::hint::black_box(store.query(std::hint::black_box(&req)).unwrap());
}

library_benchmark_group!(
    name = ingest_hot_path;
    benchmarks = bench_ingest_single, bench_ingest_batch, bench_query_after_ingest
);

main!(library_benchmark_groups = ingest_hot_path);
