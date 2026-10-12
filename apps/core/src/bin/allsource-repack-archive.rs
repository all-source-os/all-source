//! Inspect, repack, or roll back one local archive through Core's persistence engine.

use allsource_core::{
    error::{AllSourceError, Result},
    infrastructure::persistence::{
        archive_budget::ArchiveReadLimits,
        archive_repack::{read_repack_receipt, repack_archive_file, rollback_archive_repack},
    },
};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use std::{
    fs::{self, File},
    path::Path,
    process::ExitCode,
    time::{Duration, Instant},
};

fn inspect_archive_file(path: &Path) -> Result<serde_json::Value> {
    let io_error = |error: std::io::Error| AllSourceError::StorageError(error.to_string());
    let metadata = fs::symlink_metadata(path).map_err(io_error)?;
    if !metadata.is_file() {
        return Err(AllSourceError::StorageError(
            "Archive is not a regular file".into(),
        ));
    }
    let file = File::open(path).map_err(io_error)?;
    let opened = file.metadata().map_err(io_error)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.ino() != opened.ino() || metadata.dev() != opened.dev() {
            return Err(AllSourceError::StorageError(
                "Archive changed before inspection".into(),
            ));
        }
    }
    if metadata.len() != opened.len()
        || metadata.modified().map_err(io_error)? != opened.modified().map_err(io_error)?
    {
        return Err(AllSourceError::StorageError(
            "Archive changed before inspection".into(),
        ));
    }
    let reader = ParquetRecordBatchReaderBuilder::try_new(file)?;
    let rows = u64::try_from(reader.metadata().file_metadata().num_rows())
        .map_err(|_| AllSourceError::StorageError("Invalid archive row count".into()))?;
    let declared = reader
        .metadata()
        .row_groups()
        .iter()
        .try_fold(0u64, |sum, group| {
            let bytes = u64::try_from(group.total_byte_size())
                .map_err(|_| AllSourceError::StorageError("Invalid row-group byte count".into()))?;
            sum.checked_add(bytes)
                .ok_or_else(|| AllSourceError::StorageError("Row-group byte count overflow".into()))
        })?;
    Ok(serde_json::json!({
        "file_bytes": metadata.len(), "rows": rows,
        "declared_uncompressed_bytes": declared,
        "exceeds_file_limit": metadata.len() > ArchiveReadLimits::default().max_file_bytes,
    }))
}

fn usage() {
    eprintln!(
        "Usage: allsource-repack-archive inspect <PARQUET>\n\
              allsource-repack-archive repack <PARQUET>\n\
              allsource-repack-archive rollback <RECEIPT>\n\n\
              Inspect is read-only. Repack retains the original and emits a receipt.\n\
              Stop Core and retain a volume backup before operator maintenance.\n\
              Rollback refuses changed originals or consumed replacements."
    );
}

fn main() -> ExitCode {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() == 1 && matches!(args[0].as_str(), "-h" | "--help") {
        usage();
        return ExitCode::SUCCESS;
    }
    if args.len() != 2 || !matches!(args[0].as_str(), "inspect" | "repack" | "rollback") {
        usage();
        return ExitCode::from(2);
    }
    let path = Path::new(&args[1]);
    let result = match args[0].as_str() {
        "inspect" => inspect_archive_file(path),
        "repack" => repack_archive_file(path, Some(Instant::now() + Duration::from_secs(240)))
            .and_then(|receipt| {
                Ok(serde_json::json!({
                    "receipt": receipt,
                    "archive": read_repack_receipt(&receipt)?,
                }))
            }),
        "rollback" => rollback_archive_repack(path)
            .map(|()| serde_json::json!({"restored": true, "receipt": path})),
        _ => unreachable!(),
    };
    match result {
        Ok(value) => {
            println!("{value}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(1)
        }
    }
}
