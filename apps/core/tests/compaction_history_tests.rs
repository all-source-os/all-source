//! Token compaction reclaims memory; it never rewrites history.
//!
//! The merged event used to be appended to the WAL, which made compaction look
//! durable while it was not: the merged event never reached Parquet, so a
//! checkpoint deleted it and a restart brought the original tokens back. The
//! merge is now explicitly a derived view, and these tests pin both halves of
//! that — what memory shows, and what the durable log still holds.

use allsource_core::{
    QueryEventsRequest,
    domain::entities::Event,
    infrastructure::persistence::{
        compaction::CompactionConfig, snapshot::SnapshotConfig, wal::WALConfig,
    },
    store::{EventStore, EventStoreConfig},
};
use std::path::Path;
use tempfile::TempDir;

const ENTITY: &str = "wf-1";
const TOKEN_TYPE: &str = "workflow.token";

fn config(data_dir: &Path) -> EventStoreConfig {
    EventStoreConfig::production(
        data_dir.join("storage"),
        data_dir.join("wal"),
        SnapshotConfig::default(),
        WALConfig::default(),
        CompactionConfig::default(),
    )
}

fn event(event_type: &str, payload: serde_json::Value) -> Event {
    Event::from_strings(
        event_type.to_string(),
        ENTITY.to_string(),
        "default".to_string(),
        payload,
        None,
    )
    .expect("event")
}

fn event_types(store: &EventStore) -> Vec<String> {
    store
        .query(&QueryEventsRequest {
            entity_id: Some(ENTITY.to_string()),
            tenant_id: Some("default".to_string()),
            limit: Some(1000),
            ..Default::default()
        })
        .expect("query")
        .iter()
        .map(|e| e.event_type_str().to_string())
        .collect()
}

fn merged() -> Event {
    event(
        "workflow.output.complete",
        serde_json::json!({ "text": "w0w1w2", "token_count": 3 }),
    )
}

fn store_with_three_tokens(data_dir: &Path) -> EventStore {
    let store = EventStore::with_config(config(data_dir));
    for i in 0..3 {
        store
            .ingest(&event(
                TOKEN_TYPE,
                serde_json::json!({ "token": format!("w{i}"), "index": i }),
            ))
            .expect("ingest");
    }
    store
}

#[test]
fn compaction_replaces_the_tokens_in_memory() {
    let tmp = TempDir::new().unwrap();
    let store = store_with_three_tokens(tmp.path());

    let compacted = store
        .compact_entity_tokens(ENTITY, TOKEN_TYPE, merged())
        .expect("compact");

    assert!(compacted);
    assert_eq!(event_types(&store), ["workflow.output.complete"]);
}

/// Replayed from the WAL, with no checkpoint in between — the crash shape. This
/// is where a merged event written to the log used to show up, giving a reader
/// the merge AND the tokens it was supposed to replace.
#[test]
fn compaction_leaves_the_durable_token_history_intact() {
    let tmp = TempDir::new().unwrap();
    {
        let store = store_with_three_tokens(tmp.path());
        store
            .compact_entity_tokens(ENTITY, TOKEN_TYPE, merged())
            .expect("compact");
        if let Some(wal) = store.wal() {
            wal.sync().expect("sync");
        }
    }

    let reopened = EventStore::with_config(config(tmp.path()));

    let types = event_types(&reopened);
    assert_eq!(
        types,
        [TOKEN_TYPE, TOKEN_TYPE, TOKEN_TYPE],
        "the original tokens are the durable history and must survive compaction"
    );
    assert!(
        !types.iter().any(|t| t == "workflow.output.complete"),
        "the merged event is derived and must not be written to the log: {types:?}"
    );
}
