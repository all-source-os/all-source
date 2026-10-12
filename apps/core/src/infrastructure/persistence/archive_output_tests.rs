use super::ArchiveOutput;
use crate::{
    domain::entities::Event,
    infrastructure::persistence::{
        ParquetStorage,
        archive_budget::{ArchiveReadBudget, ArchiveReadLimits},
    },
    store::{EventStore, EventStoreConfig, ReadScope},
};
use arrow::{
    array::{StringArray, UInt64Array},
    datatypes::{DataType, Field, Schema},
    record_batch::RecordBatch,
};
use parquet::{
    arrow::{ArrowWriter, arrow_reader::ParquetRecordBatchReaderBuilder},
    basic::Compression,
    file::properties::WriterProperties,
};
use std::{
    fs::{self, File},
    mem::size_of,
    path::Path,
    sync::Arc,
};
use tempfile::{NamedTempFile, TempDir};

#[derive(Debug, PartialEq, Eq)]
struct Row {
    id: String,
    payload: String,
    revision: u64,
}

fn rows(count: usize, payload_bytes: usize) -> Vec<Row> {
    (0..count)
        .map(|index| {
            let mut state = (index as u64 + 1).wrapping_mul(0x9e37_79b9_7f4a_7c15);
            let payload = (0..payload_bytes)
                .map(|_| {
                    state ^= state << 13;
                    state ^= state >> 7;
                    state ^= state << 17;
                    char::from(32 + (state % 95) as u8)
                })
                .collect();
            Row {
                id: format!("event-{index}"),
                payload,
                revision: 9_007_199_254_740_991 + index as u64,
            }
        })
        .collect()
}

fn batch(rows: &[Row]) -> RecordBatch {
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("payload", DataType::Utf8, false),
        Field::new("revision", DataType::UInt64, false),
    ]));
    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|row| &row.id),
            )),
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|row| &row.payload),
            )),
            Arc::new(UInt64Array::from_iter_values(
                rows.iter().map(|row| row.revision),
            )),
        ],
    )
    .expect("valid archive batch")
}

fn write_parquet(file: File, batch: &RecordBatch) {
    let properties = WriterProperties::builder()
        .set_compression(Compression::SNAPPY)
        .set_statistics_truncate_length(None)
        .build();
    let mut writer =
        ArrowWriter::try_new(file, batch.schema(), Some(properties)).expect("parquet writer");
    writer.write(batch).expect("write measurement batch");
    writer.close().expect("finalize measurement footer");
}

fn finalized_bytes(batch: &RecordBatch) -> u64 {
    let file = NamedTempFile::new().expect("measurement file");
    write_parquet(file.reopen().expect("measurement writer"), batch);
    file.as_file().metadata().expect("physical file size").len()
}

fn read_rows(path: &Path) -> Vec<Row> {
    let reader =
        ParquetRecordBatchReaderBuilder::try_new(File::open(path).expect("published file"))
            .expect("published parquet metadata")
            .with_batch_size(2)
            .build()
            .expect("published parquet reader");
    reader
        .flat_map(|batch| {
            let batch = batch.expect("published rows");
            let ids = batch
                .column(0)
                .as_any()
                .downcast_ref::<StringArray>()
                .expect("id strings");
            let payloads = batch
                .column(1)
                .as_any()
                .downcast_ref::<StringArray>()
                .expect("payload strings");
            let revisions = batch
                .column(2)
                .as_any()
                .downcast_ref::<UInt64Array>()
                .expect("revision integers");
            (0..batch.num_rows())
                .map(|index| Row {
                    id: ids.value(index).to_owned(),
                    payload: payloads.value(index).to_owned(),
                    revision: revisions.value(index),
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

fn entries(directory: &Path) -> Vec<std::path::PathBuf> {
    let mut paths = fs::read_dir(directory)
        .expect("archive directory")
        .map(|entry| entry.expect("archive entry").path())
        .filter(|path| {
            path.file_name()
                .is_none_or(|name| name != ".archive-mutation.lock")
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

fn interrupted_publication(directory: &Path) -> std::path::PathBuf {
    let expected = rows(2, 128);
    let mut output = ArchiveOutput::new(
        directory,
        "replacement",
        Compression::SNAPPY,
        finalized_bytes(&batch(&expected)),
        true,
    )
    .expect("interrupted replacement output");
    output.stage(&batch(&expected)).expect("first staged part");
    output.stage(&batch(&expected)).expect("second staged part");
    let pending = directory.join(format!(".archive-output.{}.pending", output.stem));
    let names = output
        .parts
        .iter()
        .map(|part| {
            part.published
                .path
                .file_name()
                .expect("published filename")
                .to_string_lossy()
                .into_owned()
        })
        .collect::<Vec<_>>();
    fs::write(
        &pending,
        serde_json::to_vec(&names).expect("pending generation"),
    )
    .expect("durable pending marker");
    File::open(&pending)
        .expect("pending marker")
        .sync_all()
        .expect("pending marker sync");
    output.pending = Some(pending.clone());
    let first = &output.parts[0];
    fs::rename(&first.temporary, &first.published.path).expect("first renamed part");
    output.renamed.push(first.published.path.clone());
    File::open(directory)
        .expect("publication directory")
        .sync_all()
        .expect("partial generation sync");
    std::mem::forget(output);
    pending
}

#[test]
fn split_parts_obey_physical_byte_cap_and_preserve_values_and_revisions() {
    let directory = TempDir::new().expect("archive directory");
    let expected = rows(24, 2_048);
    let input = batch(&expected);
    let limit = finalized_bytes(&batch(&expected[..3]));
    assert!(finalized_bytes(&input) > limit);

    let mut output = ArchiveOutput::new(
        directory.path(),
        "snapshot",
        Compression::SNAPPY,
        limit,
        true,
    )
    .expect("split archive output");
    output.stage(&input).expect("stage split batch");
    let parts = output.publish().expect("publish bounded parts");

    assert!(parts.len() > 1);
    let mut restored = Vec::new();
    for part in &parts {
        assert!(fs::metadata(&part.path).expect("published size").len() <= limit);
        let recovered = read_rows(&part.path);
        assert_eq!(
            part.metadata.file_metadata().num_rows(),
            recovered.len() as i64
        );
        restored.extend(recovered);
    }
    assert_eq!(restored, expected);
    assert_eq!(entries(directory.path()).len(), parts.len());
}

#[test]
fn finalized_footer_can_trigger_splitting_when_values_fit_byte_cap() {
    let directory = TempDir::new().expect("archive directory");
    let expected = rows(4, 96);
    let input = batch(&expected);
    let complete_size = finalized_bytes(&input);
    let limit = complete_size - 1;
    let value_bytes = expected
        .iter()
        .map(|row| row.id.len() + row.payload.len() + size_of::<u64>())
        .sum::<usize>() as u64;
    assert!(value_bytes < limit);
    for row in &expected {
        assert!(finalized_bytes(&batch(std::slice::from_ref(row))) <= limit);
    }

    let mut output = ArchiveOutput::new(
        directory.path(),
        "snapshot",
        Compression::SNAPPY,
        limit,
        true,
    )
    .expect("split archive output");
    output.stage(&input).expect("stage footer-bounded batch");
    let parts = output.publish().expect("publish footer-bounded parts");

    assert!(parts.len() > 1);
    assert!(
        parts
            .iter()
            .all(|part| { fs::metadata(&part.path).expect("published size").len() <= limit })
    );
    let restored = parts
        .iter()
        .flat_map(|part| read_rows(&part.path))
        .collect::<Vec<_>>();
    assert_eq!(restored, expected);
}

#[test]
fn failed_stage_refuses_publication_and_removes_partial_prefix() {
    let directory = TempDir::new().expect("archive directory");
    let accepted = rows(2, 128);
    let oversized = rows(1, 32_768);
    let limit = finalized_bytes(&batch(&accepted));
    assert!(finalized_bytes(&batch(&oversized)) > limit);

    {
        let mut output = ArchiveOutput::new(
            directory.path(),
            "snapshot",
            Compression::SNAPPY,
            limit,
            true,
        )
        .expect("split archive output");
        output
            .stage(&batch(&accepted))
            .expect("stage accepted rows");
        let mut later = rows(1, 128);
        later.extend(oversized);
        assert!(output.stage(&batch(&later)).is_err());
        assert!(
            entries(directory.path())
                .iter()
                .all(|path| { path.extension().is_some_and(|extension| extension == "tmp") })
        );
        assert!(output.publish().is_err());
    }

    assert!(entries(directory.path()).is_empty());
}

#[test]
fn singular_output_refuses_oversized_finalized_file() {
    let directory = TempDir::new().expect("archive directory");
    let input = batch(&rows(8, 1_024));
    let limit = finalized_bytes(&input) - 1;
    let mut output = ArchiveOutput::new(
        directory.path(),
        "export",
        Compression::SNAPPY,
        limit,
        false,
    )
    .expect("singular archive output");

    assert!(output.stage(&input).is_err());
    drop(output);
    assert!(entries(directory.path()).is_empty());
    assert!(!directory.path().join("export.parquet").exists());
}

#[test]
fn failed_publication_preserves_source_archive() {
    let directory = TempDir::new().expect("archive directory");
    let source = directory.path().join("source.parquet");
    let source_rows = rows(3, 96);
    write_parquet(
        File::create(&source).expect("source archive"),
        &batch(&source_rows),
    );
    let source_bytes = fs::read(&source).expect("original source archive");
    let expected = rows(2, 128);
    let limit = finalized_bytes(&batch(&expected));
    let mut output = ArchiveOutput::new(
        directory.path(),
        "replacement",
        Compression::SNAPPY,
        limit,
        true,
    )
    .expect("replacement archive output");
    output
        .stage(&batch(&expected))
        .expect("stage first replacement");
    output
        .stage(&batch(&expected))
        .expect("stage second replacement");
    let temporary = entries(directory.path())
        .into_iter()
        .rfind(|path| path.extension().is_some_and(|extension| extension == "tmp"))
        .expect("staged replacement");
    let blocked_target = temporary.with_extension("");
    fs::create_dir(&blocked_target).expect("blocked rename target");

    assert!(output.publish().is_err());
    assert_eq!(
        fs::read(&source).expect("preserved source archive"),
        source_bytes
    );
    assert_eq!(read_rows(&source), source_rows);
    assert!(blocked_target.is_dir());
    let mut retained = vec![source, blocked_target];
    retained.sort();
    assert_eq!(entries(directory.path()), retained);
}

#[test]
fn boot_recovery_removes_interrupted_owned_prefix_and_preserves_original() {
    let directory = TempDir::new().expect("archive directory");
    let source = directory.path().join("source.parquet");
    let source_rows = rows(3, 96);
    write_parquet(
        File::create(&source).expect("source archive"),
        &batch(&source_rows),
    );
    let original = fs::read(&source).expect("source bytes");
    let pending = interrupted_publication(directory.path());
    assert!(pending.is_file());
    assert_eq!(
        entries(directory.path())
            .iter()
            .filter(|path| path
                .extension()
                .is_some_and(|extension| extension == "parquet"))
            .count(),
        2
    );

    let restarted = ParquetStorage::new(directory.path()).expect("storage boot recovery");
    assert_eq!(
        restarted.list_parquet_files().expect("recovered namespace"),
        vec![source.clone()]
    );
    assert_eq!(entries(directory.path()), vec![source.clone()]);
    assert_eq!(fs::read(&source).expect("preserved source"), original);
    assert_eq!(read_rows(&source), source_rows);
    assert!(!pending.exists());
}

#[test]
fn strict_reads_and_version_resolution_refuse_pending_publication() {
    let directory = TempDir::new().expect("archive directory");
    let storage = ParquetStorage::new(directory.path()).expect("live storage");
    let tenant = "synthetic-pending-output";
    let mut event = Event::from_strings(
        "synthetic.updated".into(),
        "source-entity".into(),
        tenant.into(),
        serde_json::json!({"original": true}),
        None,
    )
    .expect("source event");
    event.version = 7;
    let source = storage
        .write_atomic_parquet(tenant, "events-original", &[event.clone()])
        .expect("retained source event");
    let original = fs::read(&source).expect("source bytes");
    let pending = interrupted_publication(source.parent().expect("tenant partition"));
    let tolerant = storage
        .load_events_for_tenant(tenant)
        .expect("tolerant source read");
    assert_eq!(tolerant.len(), 1);
    assert_eq!(tolerant[0].id, event.id);

    let mut budget = ArchiveReadBudget::new(ArchiveReadLimits::default());
    let error = storage
        .load_events_for_tenant_with_integrity(tenant, Some(&mut budget))
        .expect_err("pending generation cannot prove archive completeness");
    assert!(
        error
            .to_string()
            .contains("Archive output publication incomplete")
    );
    let mut budget = ArchiveReadBudget::new(ArchiveReadLimits::default());
    match storage.resolve_entity_version(tenant, "source-entity", &mut budget) {
        Err(error) => assert!(
            error
                .to_string()
                .contains("Archive output publication incomplete")
        ),
        Ok(_) => panic!("pending generation cannot authorize a conditional version"),
    }
    assert!(pending.exists());
    assert_eq!(fs::read(&source).expect("source remains intact"), original);

    storage
        .cleanup_partial_writes()
        .expect("recover interrupted generation");
    let mut budget = ArchiveReadBudget::new(ArchiveReadLimits::default());
    let (recovered, complete) = storage
        .load_events_for_tenant_with_integrity(tenant, Some(&mut budget))
        .expect("strict read after recovery");
    assert!(complete);
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].id, event.id);
    assert_eq!(recovered[0].version, 7);
}

#[test]
fn read_only_attachment_preserves_active_staged_and_pending_output() {
    let directory = TempDir::new().expect("archive directory");
    let tenant = "synthetic-read-only-attach";
    let partition = directory.path().join(tenant).join("2026-01");
    fs::create_dir_all(&partition).expect("tenant partition");
    let expected = rows(2, 128);
    let mut output = ArchiveOutput::new(&partition, "active", Compression::SNAPPY, 1_000_000, true)
        .expect("active output");
    output.stage(&batch(&expected)).expect("active staged rows");
    let staged = output
        .staged_files()
        .into_iter()
        .map(|(path, _)| path.to_owned())
        .collect::<Vec<_>>();
    let attach = || {
        let mut config = EventStoreConfig::with_persistence(directory.path());
        config.read_only = true;
        config.compaction_config.auto_compact = false;
        EventStore::with_config(config)
    };
    let reader = attach();
    assert!(reader.is_read_only());
    assert!(!reader.is_tenant_loaded(tenant));
    assert!(staged.iter().all(|path| path.is_file()));
    let published = output
        .publish()
        .expect("active publication after reader attachment");
    assert_eq!(read_rows(&published[0].path), expected);
    drop(reader);
    let pending = interrupted_publication(&partition);
    let before = entries(&partition);
    let reader = attach();
    assert!(before.contains(&pending));
    assert_eq!(entries(&partition), before);
    let error = reader
        .query_retained_entity(tenant, "source-entity", 10, &ReadScope::unrestricted())
        .expect_err("pending output cannot prove completeness");
    assert!(
        error
            .to_string()
            .contains("Archive output publication incomplete")
    );
}
