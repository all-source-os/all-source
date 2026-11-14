//! tie_break_orderings: outputs over hashed collections have a total order, so
//! the heap replay's response digests are reproducible.

use super::{
    feed::{Candidate, Locator, MODEL_EVENT},
    *,
};
use std::path::PathBuf;
use tempfile::TempDir;
use uuid::Uuid;

const TENANT: &str = "synthetic-tie-break";
const ENTITY: &str = "synthetic-model";

fn commit(id: Uuid, version: i64) -> Event {
    let mut event = Event::from_strings(
        MODEL_EVENT.into(),
        ENTITY.into(),
        TENANT.into(),
        serde_json::json!({"synthetic": true}),
        None,
    )
    .unwrap();
    event.id = id;
    event.version = version;
    event
}

#[test]
fn read_model_journal_orders_equal_versions_by_id() {
    let store = EventStore::new();
    let low = Uuid::from_u128(1);
    let high = Uuid::from_u128(2);
    {
        let mut events = store.events.write();
        events.push(commit(high, 1));
        events.push(commit(low, 1));
        events.push(commit(Uuid::from_u128(0), 2));
    }
    let ids: Vec<Uuid> = store
        .read_model_journal(TENANT, ENTITY)
        .unwrap()
        .into_iter()
        .map(|event| event.id)
        .collect();
    assert_eq!(ids, vec![low, high, Uuid::from_u128(0)]);
}

#[test]
fn file_namer_names_flushed_archive_files() {
    let directory = TempDir::new().unwrap();
    let store = EventStore::with_config(EventStoreConfig {
        parquet_file_namer: Some(FileNamer::new(|tenant, _| format!("replay-{tenant}-0001"))),
        ..EventStoreConfig::with_persistence(directory.path())
    });
    let event = Event::from_strings(
        "synthetic.updated".into(),
        ENTITY.into(),
        TENANT.into(),
        serde_json::json!({"synthetic": true}),
        None,
    )
    .unwrap();
    store.ingest_with_expected_version(&event, Some(0)).unwrap();
    store.flush_storage().unwrap();

    let mut names = Vec::new();
    let mut stack = vec![directory.path().join(TENANT)];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "parquet") {
                names.push(path.file_name().unwrap().to_string_lossy().into_owned());
            }
        }
    }
    assert!(!names.is_empty());
    for name in names {
        assert!(
            name.starts_with(&format!("replay-{TENANT}-0001.")),
            "{name}"
        );
    }
}

#[test]
fn feed_payload_read_reports_the_first_failing_file_by_path() {
    let directory = TempDir::new().unwrap();
    let store = EventStore::with_config(EventStoreConfig::with_persistence(directory.path()));
    // Different corruptions give different errors, so the reported one names
    // its file.
    let corrupt = |name: &str, bytes: &[u8]| {
        let path: PathBuf = directory.path().join(name);
        std::fs::write(&path, bytes).unwrap();
        Arc::new(path)
    };
    let later = corrupt("z-events.parquet", b"unreadable archived history");
    let earlier = corrupt("a-events.parquet", b"");
    let candidate = |position: usize, file: &Arc<PathBuf>| Candidate {
        position,
        id: Uuid::from_u128(10 + position as u128),
        at: Locator::Archive {
            file: Arc::clone(file),
            row: 0,
        },
    };
    let error = |candidates: &[Candidate]| {
        store
            .read_feed_payloads(TENANT, candidates)
            .unwrap_err()
            .to_string()
    };
    let earlier_alone = error(&[candidate(0, &earlier)]);
    assert_ne!(earlier_alone, error(&[candidate(0, &later)]));
    for _ in 0..8 {
        assert_eq!(
            error(&[candidate(0, &later), candidate(1, &earlier)]),
            earlier_alone
        );
    }
}
