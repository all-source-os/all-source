//! The event feed with `ALLSOURCE_FEED_DURABLE_LOG` on. Dropping a store
//! without a feed read or checkpoint is the crash: the feed log's pending
//! records are only in memory, while the WAL already holds the events.
use allsource_core::{
    domain::entities::Event,
    infrastructure::persistence::feed_log::{FeedLog, FeedLogScope},
    store::{EventStore, EventStoreConfig, feed::FeedCursor},
};
use serde_json::json;
use std::{path::Path, sync::Once};
use uuid::Uuid;

static ENABLE: Once = Once::new();

#[allow(unsafe_code)]
fn enable() {
    // SAFETY: every test calls this before it opens a store or reads the
    // environment, and `Once` makes the single write finish before any of them
    // continues.
    ENABLE.call_once(|| unsafe { std::env::set_var("ALLSOURCE_FEED_DURABLE_LOG", "1") });
}

fn config(dir: &Path) -> EventStoreConfig {
    EventStoreConfig {
        storage_dir: Some(dir.join("storage")),
        wal_dir: Some(dir.join("wal")),
        ..EventStoreConfig::default()
    }
}

fn store(dir: &Path) -> EventStore {
    enable();
    EventStore::with_config(config(dir))
}

fn ingest(store: &EventStore, entity: &str) -> Uuid {
    let event = Event::from_strings(
        "item.changed".into(),
        entity.into(),
        "alice".into(),
        json!({"entity": entity}),
        None,
    )
    .unwrap();
    store.ingest(&event).unwrap();
    event.id
}

/// What a fresh process would find in the feed log on disk.
fn logged(dir: &Path) -> Vec<Uuid> {
    let log = FeedLog::open(dir.join("wal").join("feed-log"), FeedLogScope::All).unwrap();
    log.records("alice")
        .map(|(_, records)| records.iter().map(|record| record.id).collect())
        .unwrap_or_default()
}

#[test]
fn restart_keeps_epoch_and_positions() {
    let dir = tempfile::tempdir().unwrap();
    let (ids, epoch) = {
        let store = store(dir.path());
        let mut ids: Vec<Uuid> = ["a", "b", "c"]
            .into_iter()
            .map(|entity| ingest(&store, entity))
            .collect();
        let page = store.tenant_feed("alice", None, 10).unwrap();
        assert_eq!(page.positions, vec![1, 2, 3]);
        ids.push(ingest(&store, "d"));
        let next = store.tenant_feed("alice", Some(page.cursor), 10).unwrap();
        assert_eq!(next.positions, vec![4]);
        (ids, page.head.epoch)
    };

    let store = store(dir.path());
    let page = store
        .tenant_feed("alice", Some(FeedCursor { epoch, position: 2 }), 10)
        .unwrap();
    assert_eq!(page.head, FeedCursor { epoch, position: 4 });
    assert_eq!(page.positions, vec![3, 4]);
    let returned: Vec<_> = page.events.iter().map(|event| event.id).collect();
    assert_eq!(returned, ids[2..]);
}

#[test]
fn crash_before_fsync_never_served() {
    let dir = tempfile::tempdir().unwrap();
    let store = store(dir.path());
    let a = ingest(&store, "a");
    store.tenant_feed("alice", None, 0).unwrap();
    let b = ingest(&store, "b");
    assert_eq!(
        logged(dir.path()),
        vec![a],
        "b is not durable in the feed log yet"
    );

    let page = store.tenant_feed("alice", None, 10).unwrap();
    assert_eq!(page.positions, vec![1, 2]);
    assert_eq!(
        logged(dir.path()),
        vec![a, b],
        "a served position was fsynced first"
    );
}

#[test]
fn boot_truncate_after_recovery_fsyncs_log_first() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b, c) = {
        let store = store(dir.path());
        let a = ingest(&store, "a");
        let b = ingest(&store, "b");
        store.tenant_feed("alice", None, 0).unwrap();
        (a, b, ingest(&store, "c"))
    };
    assert_eq!(logged(dir.path()), vec![a, b]);

    // Boot replays the WAL into the feed log, flushes Parquet and truncates
    // the WAL. Nothing reads the feed before this store is dropped.
    drop(store(dir.path()));

    assert_eq!(logged(dir.path()), vec![a, b, c]);
}

#[test]
fn checkpoint_fsyncs_feed_log_before_retiring_wal_segments() {
    let dir = tempfile::tempdir().unwrap();
    let store = store(dir.path());
    let a = ingest(&store, "a");
    store.tenant_feed("alice", None, 0).unwrap();
    let b = ingest(&store, "b");

    store.checkpoint().unwrap();

    assert_eq!(logged(dir.path()), vec![a, b]);
}

#[test]
fn a_log_missing_archived_events_appends_them_without_moving_served_positions() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b, epoch) = {
        let store = store(dir.path());
        let a = ingest(&store, "a");
        let head = store.tenant_feed("alice", None, 0).unwrap().head;
        let b = ingest(&store, "b");
        store.flush_storage().unwrap();
        (a, b, head.epoch)
    };
    // b's feed-log record died with the process; dropping the WAL segments
    // leaves the archive as its only copy.
    for entry in std::fs::read_dir(dir.path().join("wal")).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            std::fs::remove_file(path).unwrap();
        }
    }
    assert_eq!(logged(dir.path()), vec![a]);

    let store = store(dir.path());
    let page = store.tenant_feed("alice", None, 10).unwrap();
    assert_eq!(page.head, FeedCursor { epoch, position: 2 });
    let returned: Vec<_> = page.events.iter().map(|event| event.id).collect();
    assert_eq!(returned, vec![a, b]);
}
