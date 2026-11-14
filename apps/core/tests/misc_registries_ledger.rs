//! The `misc` owner charges the store's registries at insert and releases what
//! they hold when the store is dropped.
#![cfg(feature = "server")]

use allsource_core::{
    application::services::webhook::RegisterWebhookRequest,
    infrastructure::memory::ledger::{Owner, resident},
    store::EventStore,
};
use serde_json::json;
use uuid::Uuid;

#[test]
fn misc_registries_charge_inserts_and_release_on_drop() {
    assert_eq!(
        resident().owner_bytes(Owner::Misc),
        0,
        "misc owner starts empty"
    );

    let store = EventStore::new();
    let empty = resident().owner_bytes(Owner::Misc);

    store.webhook_registry().register(RegisterWebhookRequest {
        tenant_id: "tenant-a".into(),
        url: "http://127.0.0.1:9/hook".into(),
        event_types: vec!["order.*".into()],
        entity_ids: Vec::new(),
        secret: None,
        description: Some("orders".into()),
    });
    let after_webhook = resident().owner_bytes(Owner::Misc);
    assert!(after_webhook > empty, "webhook registration is not charged");

    store
        .consumer_registry()
        .register("consumer-a", &["order.*".to_string()]);
    let after_consumer = resident().owner_bytes(Owner::Misc);
    assert!(
        after_consumer > after_webhook,
        "consumer registration is not charged"
    );

    store.set_projection_status("orders".into(), "paused".into());
    let after_status = resident().owner_bytes(Owner::Misc);
    assert!(
        after_status > after_consumer,
        "projection status is not charged"
    );

    let exactly_once = store.exactly_once();
    exactly_once.check_idempotency("key-1", Uuid::new_v4());
    exactly_once.commit_offset("pipeline-a", "stream-a", 7, Uuid::new_v4());
    let event_id = Uuid::new_v4();
    exactly_once.record_processing("pipeline-a", event_id);
    let after_exactly_once = resident().owner_bytes(Owner::Misc);
    assert!(
        after_exactly_once > after_status,
        "exactly-once registries are not charged"
    );

    store
        .schema_registry()
        .register_schema(
            "order.placed".into(),
            json!({"type": "object"}),
            Some("order placed".into()),
            Some(vec!["orders".into()]),
        )
        .unwrap();
    assert!(
        resident().owner_bytes(Owner::Misc) > after_exactly_once,
        "schema registration is not charged"
    );

    drop(exactly_once);
    drop(store);
    assert_eq!(
        resident().owner_bytes(Owner::Misc),
        0,
        "misc owner keeps bytes after the store is dropped"
    );
}
