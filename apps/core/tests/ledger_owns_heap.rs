//! The resident ledger tracks live heap within 10% in both directions (I2)
//! after each path that charges an owner: ingest on all four paths, flush,
//! hydration, eviction and rehydration, WAL recovery, token compaction and
//! webhook dispatch.
//!
//! The ledger is process-wide, so this file holds one test and each scenario
//! drops its store before the next starts. Dropping a store releases every
//! byte it charged, so the ledger returns to zero between scenarios.
use allsource_core::{
    domain::entities::Event,
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

fn config(storage: &std::path::Path, wal: &std::path::Path) -> EventStoreConfig {
    EventStoreConfig::production(
        storage,
        wal,
        WALConfig::default(),
        CompactionConfig::default(),
    )
}

struct Audit {
    live_start: usize,
}

impl Audit {
    fn start() -> Self {
        assert_eq!(resident().total_bytes(), 0, "ledger starts empty");
        Self {
            live_start: COUNTED.allocated(),
        }
    }

    /// `|live − ledger| ≤ 0.10 × live`, with live measured from process start.
    fn check(&self, step: &str) {
        let live = COUNTED.allocated().saturating_sub(self.live_start) as i64;
        let ledger = resident().total_bytes();
        let gap = (live - ledger).unsigned_abs();
        assert!(
            gap * 10 <= live as u64,
            "{step}: live {live} B, ledger {ledger} B, gap {gap} B is over 10%"
        );
    }

    fn released(&self, step: &str) {
        assert_eq!(
            resident().total_bytes(),
            0,
            "{step}: ledger holds bytes after drop"
        );
    }
}

#[test]
fn ledger_owns_heap() {
    let audit = Audit::start();
    let dir = tempfile::tempdir().unwrap();
    let storage = dir.path().join("parquet");
    let wal = dir.path().join("wal");

    {
        let store = EventStore::with_config(config(&storage, &wal));

        for i in 0..EVENTS {
            store.ingest(&event(i % 40, i)).unwrap();
        }
        audit.check("ingest");

        for i in 0..EVENTS {
            store
                .ingest_with_expected_version(&event(i % 40, EVENTS + i), None)
                .unwrap();
        }
        audit.check("ingest_with_expected_version");

        let batch: Vec<Event> = (0..EVENTS).map(|i| event(i % 40, 2 * EVENTS + i)).collect();
        store.ingest_batch(batch).unwrap();
        audit.check("ingest_batch");

        for i in 0..EVENTS {
            store
                .ingest_replicated(&event(i % 40, 3 * EVENTS + i))
                .unwrap();
        }
        audit.check("ingest_replicated (follower apply)");

        store.flush_storage().unwrap();
        audit.check("flush");

        for i in 0..40 {
            let token = Event::from_strings(
                "auth.token.issued".into(),
                format!("session-{i:02}"),
                TENANT.into(),
                json!({"n": i}),
                None,
            )
            .unwrap();
            store.ingest(&token).unwrap();
        }
        let merged = Event::from_strings(
            "auth.token.merged".into(),
            "session-00".into(),
            TENANT.into(),
            json!({"merged": true}),
            None,
        )
        .unwrap();
        store
            .compact_entity_tokens("session-00", "auth.token.issued", merged)
            .unwrap();
        audit.check("token compaction");

        #[cfg(feature = "server")]
        {
            use allsource_core::application::services::webhook::RegisterWebhookRequest;
            store.webhook_registry().register(RegisterWebhookRequest {
                tenant_id: TENANT.into(),
                url: "http://127.0.0.1:9/hook".into(),
                event_types: vec!["order.placed".into()],
                entity_ids: Vec::new(),
                secret: None,
                description: None,
            });
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            store.set_webhook_tx(tx);
            for i in 0..200 {
                store.ingest(&event(i % 40, 4 * EVENTS + i)).unwrap();
            }
            audit.check("webhook dispatch queued");
            while let Ok(task) = rx.try_recv() {
                drop(task);
            }
            audit.check("webhook delivery ended");
        }
    }
    audit.released("first store dropped");

    {
        let store = EventStore::with_config(config(&storage, &wal));
        store.ensure_tenant_loaded(TENANT).unwrap();
        audit.check("hydration");

        store.evict_tenant(TENANT);
        audit.check("eviction");

        store.ensure_tenant_loaded(TENANT).unwrap();
        audit.check("rehydration after eviction");
    }
    audit.released("hydrated store dropped");

    {
        let unflushed_storage = dir.path().join("parquet-unflushed");
        let unflushed_wal = dir.path().join("wal-unflushed");
        let store = EventStore::with_config(config(&unflushed_storage, &unflushed_wal));
        for i in 0..200 {
            store.ingest(&event(i % 40, i)).unwrap();
        }
        drop(store);

        let recovered = EventStore::with_config(config(&unflushed_storage, &unflushed_wal));
        audit.check("WAL recovery");
        drop(recovered);
    }
    audit.released("recovered store dropped");
}
