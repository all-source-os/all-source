//! Resident system events are charged to `system_store` at their
//! `resident_bytes()`. A checkpoint that folds the Tenant domain releases those
//! events; the other domains stay charged for the life of the store.
//!
//! The ledger is process-wide, so this file holds one test.
use allsource_core::{
    domain::value_objects::system_stream::{SystemDomain, system_entity_id_value, tenant_events},
    infrastructure::{
        memory::ledger::{Owner, resident},
        persistence::SystemMetadataStore,
    },
};
use serde_json::json;

#[test]
fn system_store_charges_resident_events_and_releases_folded_domain() {
    assert_eq!(resident().owner_bytes(Owner::SystemStore), 0);

    let dir = tempfile::tempdir().unwrap();
    let store = SystemMetadataStore::new(dir.path().join("__system")).unwrap();

    // `resident_bytes()` counts capacity, and the store holds a clone, so the
    // expected value is measured on a clone too.
    let mut tenant_bytes = 0i64;
    for i in 0..50 {
        let event = store
            .append_system_event(
                tenant_events::CREATED,
                system_entity_id_value(SystemDomain::Tenant, &format!("tenant-{i:02}")),
                json!({"name": "x".repeat(200)}),
                None,
            )
            .unwrap();
        tenant_bytes += event.clone().resident_bytes() as i64;
    }

    let mut other_bytes = 0i64;
    for i in 0..20 {
        let event = store
            .append_system_event(
                tenant_events::CREATED,
                system_entity_id_value(SystemDomain::Audit, &format!("audit-{i:02}")),
                json!({"note": "y".repeat(120)}),
                None,
            )
            .unwrap();
        other_bytes += event.clone().resident_bytes() as i64;
    }

    assert_eq!(
        resident().owner_bytes(Owner::SystemStore),
        tenant_bytes + other_bytes
    );

    store.release_folded(u64::MAX);
    assert_eq!(resident().owner_bytes(Owner::SystemStore), other_bytes);

    drop(store);
    assert_eq!(resident().owner_bytes(Owner::SystemStore), 0);
}
