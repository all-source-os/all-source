use super::{
    RepackReceipt, archive_directory_guard, repack_archive_file, repack_with_limit,
    rollback_archive_repack,
};
use crate::infrastructure::persistence::ParquetStorage;
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
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
};
use tempfile::{NamedTempFile, TempDir};

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
    RecordBatch::try_new(
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("payload", DataType::Utf8, false),
            Field::new("revision", DataType::UInt64, false),
        ])),
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
    .expect("valid source batch")
}

fn write_parquet(file: File, rows: &[Row]) {
    let batch = batch(rows);
    let properties = WriterProperties::builder()
        .set_compression(Compression::SNAPPY)
        .set_statistics_truncate_length(None)
        .build();
    let mut writer = ArrowWriter::try_new(file, batch.schema(), Some(properties))
        .expect("source parquet writer");
    writer.write(&batch).expect("source rows");
    writer.close().expect("source parquet footer");
}

fn finalized_bytes(rows: &[Row]) -> u64 {
    let file = NamedTempFile::new().expect("measurement file");
    write_parquet(file.reopen().expect("measurement writer"), rows);
    file.as_file().metadata().expect("finalized size").len()
}

fn entries(directory: &Path) -> Vec<PathBuf> {
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

fn snapshot(directory: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    entries(directory)
        .into_iter()
        .map(|path| {
            let bytes = fs::read(&path).expect("archive state");
            (path, bytes)
        })
        .collect()
}

fn receipt(path: &Path) -> RepackReceipt {
    serde_json::from_slice(&fs::read(path).expect("repair receipt")).expect("valid repair receipt")
}

fn fingerprint(path: &Path) -> String {
    let mut file = File::open(path).expect("fingerprint input");
    let mut digest = Sha256::new();
    let mut buffer = vec![0; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).expect("fingerprint bytes");
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    hex::encode(digest.finalize())
}

fn assert_repaired_rows(directory: &Path, receipt: &RepackReceipt, expected: &[Row], limit: u64) {
    let mut current = entries(directory)
        .into_iter()
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "parquet")
        })
        .collect::<Vec<_>>();
    let ordered = receipt
        .outputs
        .iter()
        .map(|name| directory.join(name))
        .collect::<Vec<_>>();
    let mut recorded = ordered.clone();
    current.sort();
    recorded.sort();
    assert_eq!(current, recorded);
    assert!(current.len() > 1);
    let mut index = 0;
    let mut output_bytes = 0;
    assert_eq!(ordered.len(), receipt.output_sha256.len());
    for (path, hash) in ordered.into_iter().zip(&receipt.output_sha256) {
        let bytes = fs::metadata(&path).expect("replacement metadata").len();
        assert!(bytes <= limit);
        assert_eq!(fingerprint(&path), *hash);
        output_bytes += bytes;
        let reader =
            ParquetRecordBatchReaderBuilder::try_new(File::open(path).expect("replacement"))
                .expect("replacement metadata")
                .with_batch_size(4)
                .build()
                .expect("replacement reader");
        for batch in reader {
            let batch = batch.expect("replacement rows");
            let ids = batch
                .column(0)
                .as_any()
                .downcast_ref::<StringArray>()
                .expect("ids");
            let payloads = batch
                .column(1)
                .as_any()
                .downcast_ref::<StringArray>()
                .expect("payloads");
            let revisions = batch
                .column(2)
                .as_any()
                .downcast_ref::<UInt64Array>()
                .expect("revisions");
            for row in 0..batch.num_rows() {
                assert_eq!(ids.value(row), expected[index].id);
                assert!(
                    payloads.value(row) == expected[index].payload,
                    "payload differs at row {index}"
                );
                assert_eq!(revisions.value(row), expected[index].revision);
                index += 1;
            }
        }
    }
    assert_eq!(index, expected.len());
    assert_eq!(receipt.rows, expected.len() as u64);
    assert_eq!(receipt.output_bytes, output_bytes);
}

#[test]
fn repack_preserves_rows_and_original_with_byte_identical_idempotent_rollback() {
    let directory = TempDir::new().expect("archive directory");
    let source = directory.path().join("snapshot.parquet");
    let expected = rows(12, 2_048);
    let limit = finalized_bytes(&expected[..3]);
    write_parquet(File::create(&source).expect("source archive"), &expected);
    let original = fs::read(&source).expect("original bytes");
    assert!(original.len() as u64 > limit);

    let receipt_path = repack_with_limit(&source, None, limit).expect("archive repair");
    let receipt = receipt(&receipt_path);
    let backup = directory.path().join(&receipt.backup);
    assert!(!source.exists());
    assert_eq!(receipt.source, "snapshot.parquet");
    assert_eq!(receipt.original_bytes, original.len() as u64);
    assert_eq!(fs::read(&backup).expect("retained source"), original);
    assert_eq!(fingerprint(&backup), receipt.original_sha256);
    assert_repaired_rows(directory.path(), &receipt, &expected, limit);

    rollback_archive_repack(&receipt_path).expect("restore original");
    assert_eq!(fs::read(&source).expect("restored bytes"), original);
    assert!(!backup.exists());
    assert!(
        receipt
            .outputs
            .iter()
            .all(|name| !directory.path().join(name).exists())
    );
    assert!(receipt.outputs.iter().all(|name| {
        directory
            .path()
            .join(name)
            .with_extension("parquet.rolled-back")
            .is_file()
    }));
    let after_rollback = entries(directory.path());
    rollback_archive_repack(&receipt_path).expect("repeat rollback");
    assert_eq!(entries(directory.path()), after_rollback);
    assert_eq!(fs::read(&source).expect("unchanged original"), original);
}

#[test]
fn corrupt_archive_refuses_repair_and_preserves_source_bytes() {
    let directory = TempDir::new().expect("archive directory");
    let source = directory.path().join("corrupt.parquet");
    write_parquet(
        File::create(&source).expect("source archive"),
        &rows(4, 128),
    );
    let mut corrupted = fs::read(&source).expect("source bytes");
    corrupted.truncate(corrupted.len() - 16);
    fs::write(&source, &corrupted).expect("corrupt source footer");
    let limit = corrupted.len() as u64 - 1;

    assert!(repack_with_limit(&source, None, limit).is_err());
    assert_eq!(
        fs::read(&source).expect("preserved corrupt source"),
        corrupted
    );
    assert_eq!(entries(directory.path()), vec![source]);
}

#[test]
fn oversized_single_row_refuses_repair_without_receipt_or_replacements() {
    let directory = TempDir::new().expect("archive directory");
    let source = directory.path().join("oversized-row.parquet");
    let limit = finalized_bytes(&rows(1, 128));
    write_parquet(
        File::create(&source).expect("source archive"),
        &rows(1, 32_768),
    );
    let original = fs::read(&source).expect("source bytes");
    assert!(original.len() as u64 > limit);

    assert!(repack_with_limit(&source, None, limit).is_err());
    assert_eq!(fs::read(&source).expect("preserved source"), original);
    assert_eq!(entries(directory.path()), vec![source]);
}

#[test]
fn tampered_backup_fingerprint_refuses_rollback_and_preserves_replacements() {
    let directory = TempDir::new().expect("archive directory");
    let source = directory.path().join("snapshot.parquet");
    let expected = rows(8, 1_024);
    let limit = finalized_bytes(&expected[..2]);
    write_parquet(File::create(&source).expect("source archive"), &expected);
    let receipt_path = repack_with_limit(&source, None, limit).expect("archive repair");
    let receipt = receipt(&receipt_path);
    let backup = directory.path().join(&receipt.backup);
    let mut changed = fs::read(&backup).expect("backup bytes");
    let index = changed.len() / 2;
    changed[index] ^= 0x80;
    fs::write(&backup, &changed).expect("tampered backup");
    let before = snapshot(directory.path());

    let error = rollback_archive_repack(&receipt_path).expect_err("fingerprint mismatch");
    assert!(error.to_string().contains("backup fingerprint differs"));
    assert!(!source.exists());
    assert_eq!(snapshot(directory.path()), before);
    assert_repaired_rows(directory.path(), &receipt, &expected, limit);
}

#[test]
fn repack_completed_retry_returns_same_receipt_after_source_retirement() {
    let directory = TempDir::new().expect("archive directory");
    let source = directory.path().join("snapshot.parquet");
    let expected = rows(12, 2_048);
    let limit = finalized_bytes(&expected[..3]);
    write_parquet(File::create(&source).expect("source archive"), &expected);
    let original = fs::read(&source).expect("source bytes");
    let receipt_path = repack_with_limit(&source, None, limit).expect("archive repair");
    let receipt = receipt(&receipt_path);
    let before = snapshot(directory.path());
    assert!(!source.exists());

    let repeated = repack_with_limit(&source, None, limit).expect("completed repair retry");
    assert_eq!(repeated, receipt_path);
    assert_eq!(snapshot(directory.path()), before);
    assert!(!source.exists());
    assert_eq!(
        fs::read(directory.path().join(&receipt.backup)).expect("original backup"),
        original
    );
    assert_repaired_rows(directory.path(), &receipt, &expected, limit);
}

#[test]
fn pending_repair_retry_preserves_rows_through_boot_cleanup() {
    let directory = TempDir::new().expect("archive directory");
    let source = directory.path().join("snapshot.parquet");
    let expected = rows(12, 2_048);
    let limit = finalized_bytes(&expected[..3]);
    write_parquet(File::create(&source).expect("source archive"), &expected);
    let original = fs::read(&source).expect("source bytes");
    let first_path = repack_with_limit(&source, None, limit).expect("initial repair");
    let first = receipt(&first_path);
    fs::rename(directory.path().join(&first.backup), &source).expect("pre-retirement source");
    let stem = first.outputs[0]
        .rsplit_once(".part-")
        .expect("output part name")
        .0;
    let pending = directory
        .path()
        .join(format!(".archive-output.{stem}.pending"));
    fs::write(
        &pending,
        serde_json::to_vec(&first.outputs).expect("pending output list"),
    )
    .expect("pending publication marker");
    File::open(&pending)
        .expect("pending marker")
        .sync_all()
        .expect("durable marker");
    File::open(directory.path())
        .expect("archive directory")
        .sync_all()
        .expect("durable generation");

    let retried_path = repack_with_limit(&source, None, limit).expect("recover pending repair");
    let repaired = receipt(&retried_path);
    assert!(!pending.exists());
    drop(ParquetStorage::new(directory.path()).expect("boot cleanup after receipt recovery"));
    assert!(!source.exists());
    assert_eq!(
        fs::read(directory.path().join(&repaired.backup)).expect("retained original"),
        original
    );
    assert_repaired_rows(directory.path(), &repaired, &expected, limit);
    assert!(
        entries(directory.path())
            .iter()
            .all(|path| path.extension().is_none_or(|ext| ext != "pending"))
    );
}

#[test]
fn tampered_replacement_refuses_rollback_without_mutation() {
    let directory = TempDir::new().expect("archive directory");
    let source = directory.path().join("snapshot.parquet");
    let expected = rows(8, 1_024);
    let limit = finalized_bytes(&expected[..2]);
    write_parquet(File::create(&source).expect("source archive"), &expected);
    let receipt_path = repack_with_limit(&source, None, limit).expect("archive repair");
    let receipt = receipt(&receipt_path);
    let output = directory.path().join(&receipt.outputs[0]);
    let mut bytes = fs::read(&output).expect("replacement bytes");
    let middle = bytes.len() / 2;
    bytes[middle] ^= 0x80;
    fs::write(&output, bytes).expect("tampered replacement");
    let before = snapshot(directory.path());

    let error =
        rollback_archive_repack(&receipt_path).expect_err("replacement fingerprint mismatch");
    assert!(
        error
            .to_string()
            .contains("replacement fingerprint differs")
    );
    assert!(!source.exists());
    assert_eq!(snapshot(directory.path()), before);
    assert_eq!(
        fingerprint(&directory.path().join(&receipt.backup)),
        receipt.original_sha256
    );
}

#[test]
fn consumed_replacement_refuses_rollback_without_mutation() {
    let directory = TempDir::new().expect("archive directory");
    let source = directory.path().join("snapshot.parquet");
    let expected = rows(8, 1_024);
    let limit = finalized_bytes(&expected[..2]);
    write_parquet(File::create(&source).expect("source archive"), &expected);
    let receipt_path = repack_with_limit(&source, None, limit).expect("archive repair");
    let receipt = receipt(&receipt_path);
    let output = directory.path().join(&receipt.outputs[0]);
    let retained = output.with_extension("parquet.consumed");
    fs::rename(&output, &retained).expect("replacement consumed by another generation");
    let before = snapshot(directory.path());

    let error = rollback_archive_repack(&receipt_path).expect_err("consumed replacement");
    assert!(
        error
            .to_string()
            .contains("replacement was consumed or removed; refusing rollback")
    );
    assert!(!source.exists());
    assert_eq!(snapshot(directory.path()), before);
    assert_eq!(fingerprint(&retained), receipt.output_sha256[0]);
    assert_eq!(
        fingerprint(&directory.path().join(&receipt.backup)),
        receipt.original_sha256
    );
}

#[test]
fn competing_directory_lock_handles_refuse_concurrent_maintenance() {
    let directory = TempDir::new().expect("archive directory");
    let owner = archive_directory_guard(directory.path()).expect("first maintenance lock");
    let contender = File::options()
        .read(true)
        .write(true)
        .open(directory.path().join(".archive-mutation.lock"))
        .expect("competing lock handle");
    assert!(contender.try_lock().is_err());
    let error = archive_directory_guard(directory.path()).expect_err("competing maintenance");
    assert!(
        error
            .to_string()
            .contains("directory already under maintenance")
    );
    drop(owner);
    contender
        .try_lock()
        .expect("maintenance lock released by owner");
    drop(contender);
    let next = archive_directory_guard(directory.path()).expect("subsequent maintenance");
    drop(next);
    assert!(entries(directory.path()).is_empty());
}

#[test]
fn default_32_mib_boundary_repack_preserves_rows_and_versions() {
    const FILE_LIMIT: u64 = 32 * 1024 * 1024;
    const REPAIR_DECODED_LIMIT: i64 = 128 * 1024 * 1024;
    let directory = TempDir::new().expect("archive directory");
    let source = directory.path().join("snapshot.parquet");
    let expected = rows(40, 1024 * 1024);
    write_parquet(File::create(&source).expect("source archive"), &expected);
    let original_bytes = fs::metadata(&source).expect("source metadata").len();
    assert!(original_bytes > FILE_LIMIT);
    let metadata = ParquetRecordBatchReaderBuilder::try_new(File::open(&source).expect("source"))
        .expect("source parquet metadata");
    let decoded_bytes = metadata
        .metadata()
        .row_groups()
        .iter()
        .map(parquet::file::metadata::RowGroupMetaData::total_byte_size)
        .sum::<i64>();
    assert!(decoded_bytes > 0 && decoded_bytes <= REPAIR_DECODED_LIMIT);
    drop(metadata);
    let original_hash = fingerprint(&source);

    let receipt_path = repack_archive_file(&source, None).expect("default archive repair");
    let receipt = receipt(&receipt_path);
    assert_eq!(receipt.original_bytes, original_bytes);
    assert_eq!(receipt.original_sha256, original_hash);
    assert_eq!(
        fingerprint(&directory.path().join(&receipt.backup)),
        original_hash
    );
    assert!(!source.exists());
    assert_repaired_rows(directory.path(), &receipt, &expected, FILE_LIMIT);

    rollback_archive_repack(&receipt_path).expect("restore default-sized original");
    assert_eq!(fingerprint(&source), original_hash);
}

#[path = "archive_repack_pending_tests.rs"]
mod pending_tests;
