use allsource_core::{
    domain::entities::Event,
    infrastructure::persistence::{CompactionConfig, CompactionManager, CompactionStrategy},
    store::{EventStore, EventStoreConfig, read_model::ModelCommit},
};
use serde_json::json;
use std::{collections::BTreeMap, path::Path};

fn event(entity: &str, blob: usize) -> Event {
    Event::from_strings(
        "item.changed".into(),
        entity.into(),
        "alice".into(),
        json!({"blob": "x".repeat(blob)}),
        None,
    )
    .unwrap()
}

/// One archive file per call, so compaction has several files to merge.
fn seed(dir: &Path, files: usize, per_file: usize, blob: usize) -> Vec<uuid::Uuid> {
    let store = EventStore::with_config(EventStoreConfig::with_persistence(dir));
    let mut ids = Vec::new();
    for file in 0..files {
        for index in 0..per_file {
            let event = event(&format!("entity-{file}-{index}"), blob);
            ids.push(event.id);
            store.ingest(&event).unwrap();
        }
        store.flush_storage().unwrap();
    }
    ids
}

fn cold_store(dir: &Path, max_decoded_bytes: u64) -> EventStore {
    let mut config = EventStoreConfig::with_persistence(dir);
    config.strict_archive_limits.max_uncompressed_bytes = max_decoded_bytes;
    EventStore::with_config(config)
}

#[test]
fn head_pages_and_commits_serve_a_tenant_over_the_strict_budget() {
    let dir = tempfile::tempdir().unwrap();
    let ids = seed(dir.path(), 4, 10, 2048);
    let store = cold_store(dir.path(), 16 * 1024);

    let head = store.tenant_feed("alice", None, 0).unwrap().head;
    assert_eq!(head.position, 40);

    let first = store.tenant_feed("alice", None, 5).unwrap();
    assert_eq!(first.positions, vec![1, 2, 3, 4, 5]);
    let returned: Vec<_> = first.events.iter().map(|event| event.id).collect();
    assert_eq!(returned, ids[..5]);
    let second = store.tenant_feed("alice", Some(first.cursor), 5).unwrap();
    assert_eq!(second.positions, vec![6, 7, 8, 9, 10]);

    assert!(
        store.tenant_feed("alice", None, 40).is_err(),
        "a page that must decode every payload still exceeds the budget"
    );

    let revision = store
        .commit_read_model(
            "alice",
            "board",
            ModelCommit {
                expected_revision: 0,
                checkpoint: head,
                ready: true,
                reset: false,
                changes: BTreeMap::from([("q/one".into(), Some(json!(1)))]),
            },
        )
        .unwrap();
    assert_eq!(revision, 1);
}

#[test]
fn compaction_after_the_index_is_built_keeps_positions_and_epoch() {
    let dir = tempfile::tempdir().unwrap();
    let ids = seed(dir.path(), 4, 3, 64);
    let store = cold_store(dir.path(), 1024 * 1024);
    let before = store.tenant_feed("alice", None, 0).unwrap().head;
    assert_eq!(before.position, 12);

    let manager = CompactionManager::new(
        dir.path(),
        CompactionConfig {
            min_files_to_compact: 2,
            small_file_threshold: 100 * 1024 * 1024,
            strategy: CompactionStrategy::SizeBased,
            ..Default::default()
        },
    );
    assert_eq!(manager.compact_tenant("alice").unwrap().files_compacted, 4);

    let page = store.tenant_feed("alice", None, 100).unwrap();
    assert_eq!(page.head, before);
    assert_eq!(page.positions, (1..=12).collect::<Vec<_>>());
    let returned: Vec<_> = page.events.iter().map(|event| event.id).collect();
    assert_eq!(returned, ids);
}

#[test]
fn events_ingested_after_the_index_is_built_follow_the_archive() {
    let dir = tempfile::tempdir().unwrap();
    let ids = seed(dir.path(), 2, 2, 64);
    let store = cold_store(dir.path(), 1024 * 1024);
    let head = store.tenant_feed("alice", None, 0).unwrap().head;
    assert_eq!(head.position, 4);

    let live = event("live", 64);
    store.ingest(&live).unwrap();
    let page = store.tenant_feed("alice", Some(head), 10).unwrap();
    assert_eq!(page.positions, vec![5]);
    assert_eq!(page.events[0].id, live.id);
    assert!(page.caught_up);

    let all = store.tenant_feed("alice", None, 10).unwrap();
    let returned: Vec<_> = all.events.iter().map(|event| event.id).collect();
    assert_eq!(returned[..4], ids[..]);
    assert_eq!(returned[4], live.id);
}
