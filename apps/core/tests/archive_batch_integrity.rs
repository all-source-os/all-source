use allsource_core::{
    QueryEventsRequest,
    domain::entities::Event,
    error::AllSourceError,
    infrastructure::persistence::ParquetStorage,
    store::{EventStore, EventStoreConfig},
};
use parquet::{
    arrow::{ArrowWriter, arrow_reader::ParquetRecordBatchReaderBuilder},
    file::properties::WriterProperties,
};
use serde_json::json;
use std::{fs::File, path::PathBuf};
use tempfile::TempDir;

const TENANT: &str = "synthetic-batch-integrity";
const ENTITY: &str = "synthetic-entity";
const BATCH_ROWS: usize = 1024;

fn event(version: i64) -> Event {
    let mut event = Event::from_strings(
        "synthetic.updated".into(),
        ENTITY.into(),
        TENANT.into(),
        json!({"synthetic": true}),
        None,
    )
    .unwrap();
    event.version = version;
    event
}

/// Preserve the footer and first row group while making the second unreadable.
/// This distinguishes decoder failure from a file-open or footer failure.
fn corrupt_second_row_group(storage: &ParquetStorage) -> PathBuf {
    let events = (1..=2 * BATCH_ROWS)
        .map(|version| event(version as i64))
        .collect::<Vec<_>>();
    let path = storage
        .write_atomic_parquet(TENANT, "events-batch-corrupt", &events)
        .unwrap();
    let builder = ParquetRecordBatchReaderBuilder::try_new(File::open(&path).unwrap()).unwrap();
    let schema = builder.schema().clone();
    let batches = builder
        .build()
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let properties = WriterProperties::builder()
        .set_max_row_group_row_count(Some(BATCH_ROWS))
        .build();
    let mut writer =
        ArrowWriter::try_new(File::create(&path).unwrap(), schema, Some(properties)).unwrap();
    for batch in batches {
        writer.write(&batch).unwrap();
    }
    writer.close().unwrap();

    let builder = ParquetRecordBatchReaderBuilder::try_new(File::open(&path).unwrap()).unwrap();
    assert_eq!(builder.metadata().num_row_groups(), 2);
    let (offset, length) = builder.metadata().row_group(1).column(0).byte_range();
    drop(builder);
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[offset as usize..(offset + length) as usize].fill(0);
    std::fs::write(&path, bytes).unwrap();

    let mut reader = ParquetRecordBatchReaderBuilder::try_new(File::open(&path).unwrap())
        .expect("the footer remains valid")
        .with_batch_size(BATCH_ROWS)
        .build()
        .unwrap();
    assert_eq!(reader.next().unwrap().unwrap().num_rows(), BATCH_ROWS);
    assert!(
        reader.next().unwrap().is_err(),
        "the second batch must fail"
    );
    path
}

#[test]
fn file_decode_failure_never_returns_a_successful_prefix() {
    let directory = TempDir::new().unwrap();
    let storage = ParquetStorage::new(directory.path()).unwrap();
    let corrupt = corrupt_second_row_group(&storage);
    assert!(
        storage
            .load_events_from_file_path(&corrupt, TENANT)
            .is_err()
    );

    // Tolerant reads still return healthy files, skipping the whole bad file.
    let healthy = event(2049);
    storage
        .write_atomic_parquet(TENANT, "events-healthy", std::slice::from_ref(&healthy))
        .unwrap();
    for events in [
        storage.load_events_for_tenant(TENANT).unwrap(),
        storage.load_all_events().unwrap(),
    ] {
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, healthy.id);
    }
}

#[test]
fn later_batch_failure_cannot_authorize_a_conditional_append() {
    for warm_query in [false, true] {
        let directory = TempDir::new().unwrap();
        let storage = ParquetStorage::new(directory.path()).unwrap();
        let corrupt = corrupt_second_row_group(&storage);
        let original = std::fs::read(&corrupt).unwrap();
        drop(storage);
        let store = EventStore::with_config(EventStoreConfig::with_persistence(directory.path()));
        let query = QueryEventsRequest {
            entity_id: Some(ENTITY.into()),
            tenant_id: Some(TENANT.into()),
            ..Default::default()
        };
        if warm_query {
            store.query(&query).unwrap();
        }
        let mut subscriber = store.subscribe_events();
        assert!(matches!(
            store.ingest_with_expected_version(&event(0), Some(BATCH_ROWS as u64)),
            Err(AllSourceError::StorageError(_))
        ));
        assert!(subscriber.try_recv().is_err());
        assert!(store.query(&query).unwrap().is_empty());
        assert_eq!(std::fs::read(&corrupt).unwrap(), original);
    }
}
