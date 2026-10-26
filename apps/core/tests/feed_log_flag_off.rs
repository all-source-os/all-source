//! Without `ALLSOURCE_FEED_DURABLE_LOG` the feed keeps a random epoch per
//! process and writes no feed log. Its own test binary, so no other test can
//! have set the variable.
use allsource_core::{
    domain::entities::Event,
    store::{EventStore, EventStoreConfig},
};
use serde_json::json;
use std::path::Path;

fn store(dir: &Path) -> EventStore {
    EventStore::with_config(EventStoreConfig {
        storage_dir: Some(dir.join("storage")),
        wal_dir: Some(dir.join("wal")),
        ..EventStoreConfig::default()
    })
}

#[test]
fn flag_off_keeps_random_epoch() {
    assert!(
        std::env::var_os("ALLSOURCE_FEED_DURABLE_LOG").is_none(),
        "this test checks the default; unset ALLSOURCE_FEED_DURABLE_LOG"
    );
    let dir = tempfile::tempdir().unwrap();
    let first = {
        let store = store(dir.path());
        let event = Event::from_strings(
            "item.changed".into(),
            "a".into(),
            "alice".into(),
            json!({}),
            None,
        )
        .unwrap();
        store.ingest(&event).unwrap();
        store.tenant_feed("alice", None, 0).unwrap().head
    };

    let second = store(dir.path())
        .tenant_feed("alice", None, 0)
        .unwrap()
        .head;
    assert_eq!(second.position, first.position);
    assert_ne!(second.epoch, first.epoch);
    assert!(!dir.path().join("wal").join("feed-log").exists());
}
