//! The ledger counts and never refuses: a budget-driven eviction produces the
//! same evicted-tenant sequence as before the ledger existed, and every owner
//! counter stays non-negative while tenants are loaded and evicted.
#![cfg(feature = "server")]

use allsource_core::{
    domain::entities::Event,
    infrastructure::memory::ledger::{Owner, resident},
    store::{EventStore, EventStoreConfig},
};
use serde_json::json;

const TENANTS: [&str; 4] = ["tenant-00", "tenant-01", "tenant-02", "tenant-03"];
const EVENTS_PER_TENANT: usize = 40;
const PAD_BYTES: usize = 2_000;

fn event(tenant: &str, seq: usize) -> Event {
    Event::from_strings(
        "order.placed".into(),
        format!("order-{:03}", seq % 20),
        tenant.into(),
        json!({"seq": seq, "pad": "x".repeat(PAD_BYTES)}),
        None,
    )
    .unwrap()
}

fn config(dir: &std::path::Path, budget: Option<u64>) -> EventStoreConfig {
    let mut config = EventStoreConfig::with_persistence(dir.join("parquet"));
    config.cache_byte_budget = budget;
    config
}

fn assert_counts_non_negative(step: &str) {
    for owner in Owner::ALL {
        assert!(
            resident().owner_bytes(owner) >= 0,
            "{step}: {} went negative",
            owner.label()
        );
    }
}

#[test]
fn ledger_counts_only() {
    let dir = tempfile::tempdir().unwrap();
    {
        let store = EventStore::with_config(config(dir.path(), None));
        for tenant in TENANTS {
            for seq in 0..EVENTS_PER_TENANT {
                store.ingest(&event(tenant, seq)).unwrap();
            }
        }
        store.flush_storage().unwrap();
    }

    let store = EventStore::with_config(config(dir.path(), Some(200_000)));
    let mut evicted = Vec::new();
    for tenant in TENANTS {
        let loaded_before: Vec<&str> = TENANTS
            .iter()
            .copied()
            .filter(|t| store.is_tenant_loaded(t))
            .collect();
        store.ensure_tenant_loaded(tenant).unwrap();
        store.enforce_cache_budget_now();
        for previous in loaded_before {
            if !store.is_tenant_loaded(previous) && !evicted.contains(&previous) {
                evicted.push(previous);
            }
        }
        assert_counts_non_negative(tenant);
    }

    assert_eq!(evicted, vec!["tenant-00", "tenant-01"]);
    drop(store);
}
