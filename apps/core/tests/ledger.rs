//! `ResidentLedger` counts, the signed unattributed gauge, and the rule that
//! owner counters change only through the ledger API (design decision 1, FR-1).
use allsource_core::infrastructure::{
    memory::ledger::{Owner, ResidentLedger},
    observability::MetricsRegistry,
    persistence::index::{EventIndex, IndexEntry, ValidWindow},
};
use chrono::Utc;
use std::{
    fs,
    path::{Path, PathBuf},
};
use uuid::Uuid;

#[test]
fn index_vec_growth() {
    let ledger = ResidentLedger::new();
    let slot = std::mem::size_of::<IndexEntry>();
    let index = EventIndex::new();
    let entity = "entity-synthetic";

    for offset in 0..10_000 {
        let before = index.entity_capacity(entity);
        let now = Utc::now();
        index
            .index_event(
                Uuid::new_v4(),
                entity,
                "order.placed",
                now,
                ValidWindow {
                    from: now,
                    to: None,
                },
                offset,
            )
            .unwrap();
        ledger.track_capacity(
            Owner::IndexEntity,
            before,
            index.entity_capacity(entity),
            slot,
        );
    }

    assert_eq!(
        ledger.owner_bytes(Owner::IndexEntity),
        (index.entity_capacity(entity) * slot) as i64
    );
}

#[test]
fn ledger_counts_and_never_refuses_an_over_release() {
    let ledger = ResidentLedger::new();
    ledger.charge(Owner::Query, 100);
    ledger.release(Owner::Query, 250);
    assert_eq!(ledger.owner_bytes(Owner::Query), -150);
    assert_eq!(ledger.total_bytes(), -150);
}

#[test]
fn permanent_owners_are_summed_separately() {
    let ledger = ResidentLedger::new();
    ledger.charge(Owner::EntityVersions, 400);
    ledger.charge(Owner::Query, 50);
    assert_eq!(ledger.permanent_bytes(), 400);
    assert_eq!(ledger.total_bytes(), 450);
}

#[test]
fn unattributed_gauge_is_signed() {
    let metrics = MetricsRegistry::new();
    let ledger = ResidentLedger::new();
    ledger.charge(Owner::EventsResident, 1_000);

    metrics.record_ledger(1_500, &ledger);
    assert_eq!(metrics.heap_unattributed_bytes.get(), 500);

    metrics.record_ledger(600, &ledger);
    assert_eq!(metrics.heap_unattributed_bytes.get(), -400);

    let encoded = metrics.encode().unwrap();
    assert!(encoded.contains("allsource_heap_unattributed_bytes -400"));
    assert!(encoded.contains("allsource_core_ledger_owner_bytes{owner=\"events.resident\"} 1000"));
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn owner_counters_mutate_only_through_the_ledger_api() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let ledger_file = src.join("infrastructure/memory/ledger.rs");
    assert!(ledger_file.exists(), "missing {}", ledger_file.display());

    let mut files = Vec::new();
    rust_files(&src, &mut files);
    assert!(!files.is_empty());

    let banned = [
        "ResidentLedger {",
        ".owners[",
        ".owners.",
        ".rows_decoded.",
        "RESIDENT.owners",
    ];
    for file in files.iter().filter(|file| **file != ledger_file) {
        let text = fs::read_to_string(file).unwrap();
        for pattern in banned {
            assert!(
                !text.contains(pattern),
                "{} touches ledger internals via `{pattern}`",
                file.display()
            );
        }
    }
}
