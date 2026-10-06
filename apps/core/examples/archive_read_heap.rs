//! Measure what an archive read actually holds, against what the budget charges.
//!
//! `ArchiveReadLimits` is expressed in bytes, but until this was measured the
//! number it compared against was Parquet's declared uncompressed size. The
//! memory that runs out is the decoded `serde_json::Value` tree, and this
//! prints the ratio between the two so the ceiling can be set against the
//! quantity that actually exhausts.
//!
//! Usage:
//!   cargo run --profile release-debug --example archive_read_heap --features dhat-heap
//!
//! Without the feature it still reports the byte counts, just not peak heap.

#[cfg(not(feature = "dhat-heap"))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

use allsource_core::{domain::entities::Event, infrastructure::persistence::ParquetStorage};
use serde_json::json;

const TENANT: &str = "archive-read-heap";
const EVENTS: usize = 20_000;

/// A payload shaped like the ones that actually sit in the archive: a handful
/// of short string fields and a nested object, not a single scalar.
fn payload(i: usize) -> serde_json::Value {
    json!({
        "index": i,
        "actor": format!("user-{}", i % 997),
        "action": "record.updated",
        "source": "archive-read-heap",
        "detail": {
            "field": format!("field-{}", i % 31),
            "previous": format!("value-{}", i.wrapping_mul(7) % 10_000),
            "current": format!("value-{}", i.wrapping_mul(13) % 10_000),
            "note": "measured against the strict archive read path",
        },
        "tags": ["alpha", "beta", "gamma"],
    })
}

fn main() {
    let directory = tempfile::TempDir::new().expect("temp dir");
    let storage = ParquetStorage::new(directory.path()).expect("storage");

    let events: Vec<Event> = (0..EVENTS)
        .map(|i| {
            Event::from_strings(
                "archive.read".to_string(),
                format!("entity-{}", i % 512),
                TENANT.to_string(),
                payload(i),
                None,
            )
            .expect("event")
        })
        .collect();

    let text_bytes: u64 = events
        .iter()
        .map(|event| serde_json::to_vec(&event.payload).map_or(0, |v| v.len() as u64))
        .sum();

    storage
        .write_atomic_parquet(TENANT, "events-heap", &events)
        .expect("write");
    drop(events);

    let parquet_bytes: u64 = storage
        .list_parquet_files_for_tenant(TENANT)
        .expect("list")
        .iter()
        .map(|path| std::fs::metadata(path).map_or(0, |m| m.len()))
        .sum();

    #[cfg(feature = "dhat-heap")]
    let profiler = dhat::Profiler::new_heap();

    let loaded = storage.load_events_for_tenant(TENANT).expect("load");
    let count = loaded.len();

    #[cfg(feature = "dhat-heap")]
    let peak = {
        let stats = dhat::HeapStats::get();
        drop(profiler);
        stats.max_bytes as u64
    };

    std::hint::black_box(&loaded);

    println!("events decoded          : {count}");
    println!("payload JSON text bytes : {text_bytes}");
    println!("parquet file bytes      : {parquet_bytes}");

    #[cfg(feature = "dhat-heap")]
    {
        println!("peak heap bytes         : {peak}");
        println!(
            "heap / payload text     : {:.2}x",
            peak as f64 / text_bytes.max(1) as f64
        );
        println!(
            "heap / parquet file     : {:.2}x",
            peak as f64 / parquet_bytes.max(1) as f64
        );
    }

    #[cfg(not(feature = "dhat-heap"))]
    println!("peak heap               : rerun with --features dhat-heap");
}
