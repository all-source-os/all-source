use super::{
    ParquetStorage, assert_repaired_rows, entries, finalized_bytes, fingerprint, receipt,
    repack_with_limit, rows, write_parquet,
};
use arrow::compute::concat_batches;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use std::{
    fs::{self, File},
    path::{Path, PathBuf},
};
use tempfile::TempDir;

#[test]
fn expired_sweep_preserves_archives_without_waiting_for_maintenance() {
    use crate::infrastructure::persistence::{
        archive_repack::archive_mutation_guard,
        compaction::{CompactionConfig, CompactionManager},
    };
    use std::time::Instant;

    let directory = TempDir::new().expect("archive directory");
    let tenant = "synthetic-paused";
    let partition = directory.path().join(tenant);
    fs::create_dir(&partition).expect("tenant partition");
    let source = partition.join("events.parquet");
    write_parquet(
        File::create(&source).expect("source archive"),
        &rows(8, 128),
    );
    let original = fs::read(&source).expect("original archive");
    let _maintenance = archive_mutation_guard(None).expect("active maintenance");
    let manager = CompactionManager::new(directory.path(), CompactionConfig::default());

    let result = manager
        .compact_tenant_until(tenant, Some(Instant::now()))
        .expect("expired sweep pauses without a mutation error");

    assert_eq!(result.files_compacted, 0);
    assert_eq!(fs::read(&source).expect("preserved archive"), original);
}

#[test]
fn maintenance_timeout_keeps_sweep_cursor_on_blocked_tenant() {
    use crate::infrastructure::persistence::{
        archive_repack::archive_mutation_guard,
        compaction::{CompactionConfig, CompactionManager},
    };
    use std::time::{Duration, Instant};

    let directory = TempDir::new().expect("archive directory");
    let mut originals = Vec::new();
    for tenant in ["synthetic-first", "synthetic-second"] {
        let partition = directory.path().join(tenant);
        fs::create_dir(&partition).expect("tenant partition");
        let source = partition.join("events.parquet");
        write_parquet(
            File::create(&source).expect("source archive"),
            &rows(8, 128),
        );
        originals.push((source.clone(), fs::read(&source).expect("original archive")));
    }
    let _maintenance = archive_mutation_guard(None).expect("active maintenance");
    let manager = CompactionManager::new(directory.path(), CompactionConfig::default());

    let result = manager
        .compact_until(Some(Instant::now() + Duration::from_millis(50)))
        .expect("contended sweep pauses");

    assert_eq!(result.files_compacted, 0);
    assert_eq!(manager.sweep_cursor().as_deref(), Some("synthetic-first"));
    for (source, original) in originals {
        assert_eq!(fs::read(source).expect("preserved archive"), original);
    }
}

fn pending_generation(directory: &Path, outputs: &[String]) -> PathBuf {
    let (stem, _) = outputs[0]
        .rsplit_once(".part-")
        .expect("output part filename");
    let marker = directory.join(format!(".archive-output.{stem}.pending"));
    fs::write(
        &marker,
        serde_json::to_vec(outputs).expect("pending output list"),
    )
    .expect("pending marker");
    File::open(&marker)
        .expect("pending marker")
        .sync_all()
        .expect("durable marker");
    File::open(directory)
        .expect("archive directory")
        .sync_all()
        .expect("durable generation");
    marker
}

#[test]
fn completed_receipt_retry_clears_own_marker_and_preserves_unrelated_generation() {
    let directory = TempDir::new().expect("archive directory");
    let source = directory.path().join("snapshot.parquet");
    let expected = rows(12, 2_048);
    let limit = finalized_bytes(&expected[..3]);
    write_parquet(File::create(&source).expect("source archive"), &expected);
    let original = fs::read(&source).expect("source bytes");
    let first_path = repack_with_limit(&source, None, limit).expect("initial repair");
    let first = receipt(&first_path);
    let own_marker = pending_generation(directory.path(), &first.outputs);
    let unrelated_name = "unrelated.generation.part-1.parquet".to_owned();
    let unrelated_part = directory.path().join(&unrelated_name);
    write_parquet(
        File::create(&unrelated_part).expect("unrelated part"),
        &rows(2, 128),
    );
    let unrelated_marker = pending_generation(directory.path(), &[unrelated_name]);
    let unrelated_bytes = fs::read(&unrelated_part).expect("unrelated output bytes");
    let marker_bytes = fs::read(&unrelated_marker).expect("unrelated marker bytes");

    let retried_path = repack_with_limit(&source, None, limit).expect("completed receipt retry");
    let repaired = receipt(&retried_path);
    assert!(!own_marker.exists());
    assert!(!source.exists());
    assert_eq!(
        fs::read(&unrelated_part).expect("unrelated part preserved"),
        unrelated_bytes
    );
    assert_eq!(
        fs::read(&unrelated_marker).expect("unrelated marker preserved"),
        marker_bytes
    );
    assert_eq!(
        fs::read(directory.path().join(&repaired.backup)).expect("original backup"),
        original
    );
    assert_eq!(repaired.rows, expected.len() as u64);
    let reader = ParquetStorage::open_existing(directory.path()).expect("attach without recovery");
    let mut visible = reader
        .list_parquet_files()
        .expect("visible completed outputs");
    let mut recorded = repaired
        .outputs
        .iter()
        .map(|name| directory.path().join(name))
        .collect::<Vec<_>>();
    visible.sort();
    recorded.sort();
    assert_eq!(visible, recorded);
    let mut batches = Vec::new();
    for (name, hash) in repaired.outputs.iter().zip(&repaired.output_sha256) {
        let part = directory.path().join(name);
        assert!(
            fs::metadata(&part)
                .expect("completed output metadata")
                .len()
                <= limit
        );
        assert_eq!(fingerprint(&part), *hash);
        let reader =
            ParquetRecordBatchReaderBuilder::try_new(File::open(part).expect("completed part"))
                .expect("completed part metadata")
                .build()
                .expect("completed part reader");
        for batch in reader {
            batches.push(batch.expect("completed part rows"));
        }
    }
    let original_rows = super::batch(&expected);
    let restored =
        concat_batches(&original_rows.schema(), &batches).expect("complete restored rows");
    assert!(restored == original_rows, "owned values or versions differ");
}

#[test]
fn partial_pending_receipt_retry_preserves_original_rows_through_boot_cleanup() {
    let directory = TempDir::new().expect("archive directory");
    let source = directory.path().join("snapshot.parquet");
    let expected = rows(12, 2_048);
    let limit = finalized_bytes(&expected[..3]);
    write_parquet(File::create(&source).expect("source archive"), &expected);
    let original = fs::read(&source).expect("source bytes");
    let first_path = repack_with_limit(&source, None, limit).expect("initial repair");
    let first = receipt(&first_path);
    assert!(first.outputs.len() > 1);
    fs::rename(directory.path().join(&first.backup), &source).expect("pre-retirement source");
    let unfinished = directory
        .path()
        .join(first.outputs.last().expect("last replacement"));
    let temporary = unfinished.with_extension("parquet.tmp");
    fs::rename(&unfinished, &temporary).expect("last part still staged at interruption");
    let pending = pending_generation(directory.path(), &first.outputs);
    assert_eq!(
        fs::read(&source).expect("intact interrupted original"),
        original
    );

    let retried_path = repack_with_limit(&source, None, limit).expect("partial receipt retry");
    let repaired = receipt(&retried_path);
    assert!(!pending.exists());
    drop(ParquetStorage::new(directory.path()).expect("boot cleanup after partial recovery"));
    assert!(!source.exists());
    assert!(!temporary.exists());
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
