use super::{
    archive_budget::{ArchiveReadBudget, ArchiveReadLimits},
    archive_output::{ArchiveOutput, clear_pending_output},
    storage::decoded_text_bytes,
};
use crate::error::{AllSourceError, Result};
use arrow::{compute::concat_batches, record_batch::RecordBatch};
use parquet::{arrow::arrow_reader::ParquetRecordBatchReaderBuilder, basic::Compression};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, Metadata},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct RepackReceipt {
    source: String,
    backup: String,
    pub(crate) outputs: Vec<String>,
    output_sha256: Vec<String>,
    original_sha256: String,
    pub rows: u64,
    pub original_bytes: u64,
    pub output_bytes: u64,
    #[serde(default)]
    pub original_created_at: Option<std::time::SystemTime>,
}

pub(crate) fn archive_mutation_guard(
    deadline: Option<Instant>,
) -> Option<parking_lot::MutexGuard<'static, ()>> {
    static LOCK: parking_lot::Mutex<()> = parking_lot::const_mutex(());
    LOCK.try_lock_until(deadline.unwrap_or_else(|| Instant::now() + Duration::from_secs(240)))
}

pub(crate) fn archive_directory_guard(directory: &Path) -> Result<File> {
    let path = directory.join(".archive-mutation.lock");
    if path.exists() {
        regular_file(&path)?;
    }
    let file = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .map_err(io_error)?;
    file.try_lock()
        .map_err(|error| failure(&format!("directory already under maintenance: {error}")))?;
    Ok(file)
}

pub fn read_repack_receipt(path: &Path) -> Result<RepackReceipt> {
    let retained = path.with_extension("json.rolled-back");
    let path = if path.exists() { path } else { &retained };
    if regular_file(path)?.len() > 1024 * 1024 {
        return Err(failure("receipt exceeds byte limit"));
    }
    Ok(serde_json::from_reader(
        File::open(path).map_err(io_error)?,
    )?)
}

/// Stream one oversized archive into bounded replacements and retain its original.
pub fn repack_archive_file(path: &Path, deadline: Option<Instant>) -> Result<PathBuf> {
    repack_with_limit(path, deadline, ArchiveReadLimits::default().max_file_bytes)
}

fn repack_with_limit(path: &Path, deadline: Option<Instant>, max_bytes: u64) -> Result<PathBuf> {
    let deadline = Some(deadline.unwrap_or_else(|| Instant::now() + Duration::from_secs(240)));
    let _mutation = archive_mutation_guard(deadline)
        .ok_or_else(|| failure("mutation lock deadline exceeded"))?;
    repack_locked(path, deadline, max_bytes)
}

pub(crate) fn repack_archive_file_in_sweep(
    path: &Path,
    deadline: Option<Instant>,
) -> Result<PathBuf> {
    repack_locked(path, deadline, ArchiveReadLimits::default().max_file_bytes)
}

fn repack_locked(path: &Path, deadline: Option<Instant>, max_bytes: u64) -> Result<PathBuf> {
    let started = Instant::now();
    let deadline = deadline.unwrap_or_else(|| started + Duration::from_secs(240));
    let directory = path
        .parent()
        .ok_or_else(|| failure("source directory missing"))?;
    let _directory = archive_directory_guard(directory)?;
    let source = file_name(path)?;
    let output_kind = if source.starts_with("snapshot.") {
        "snapshot"
    } else {
        "events"
    };
    if let Some(receipt_path) = resume_pending(path, &source, directory, deadline, max_bytes)? {
        return Ok(receipt_path);
    }
    let metadata = regular_file(path)?;
    if metadata.len() <= max_bytes {
        return Err(failure("source does not exceed file byte limit"));
    }
    let timeout = deadline
        .checked_duration_since(Instant::now())
        .filter(|value| !value.is_zero())
        .ok_or_else(|| failure("deadline exceeded"))?;
    let id = uuid::Uuid::new_v4().as_simple().to_string();
    let backup = format!("{source}.original.{id}");
    let receipt_path = directory.join(format!("{source}.repair.{id}.json"));
    let limits = ArchiveReadLimits {
        timeout,
        max_file_bytes: ArchiveReadLimits::default().max_compressed_bytes,
        ..Default::default()
    };
    let mut budget = ArchiveReadBudget::new(limits);
    budget.compressed(metadata.len())?;
    let file = File::open(path).map_err(io_error)?;
    if !same_file(&metadata, &file.metadata().map_err(io_error)?) {
        return Err(failure("source changed before reading"));
    }
    let builder = ParquetRecordBatchReaderBuilder::try_new(file)?;
    for group in builder.metadata().row_groups() {
        budget.rows(group.num_rows())?;
    }
    let schema = builder.schema().clone();
    let mut output = ArchiveOutput::new(
        directory,
        &format!("{output_kind}.repacked.{id}"),
        Compression::SNAPPY,
        max_bytes,
        true,
    )?;
    let mut buffered = Vec::<RecordBatch>::new();
    let mut buffered_bytes = 0usize;
    let target = usize::try_from(max_bytes / 2)
        .unwrap_or(usize::MAX)
        .clamp(1, 16 * 1024 * 1024);
    let mut rows = 0u64;
    for batch in builder.with_batch_size(64).build()? {
        budget.check()?;
        let batch = batch?;
        let bytes = batch.get_array_memory_size();
        if bytes as u64 > ArchiveReadLimits::default().max_uncompressed_bytes {
            return Err(failure("decoded batch exceeds memory byte limit"));
        }
        budget.decoded_payload(decoded_text_bytes(&batch))?;
        if !buffered.is_empty() && buffered_bytes.saturating_add(bytes) > target {
            output.stage(&concat_batches(&schema, &buffered)?)?;
            buffered.clear();
            buffered_bytes = 0;
        }
        rows = rows
            .checked_add(batch.num_rows() as u64)
            .ok_or_else(|| failure("row count overflow"))?;
        buffered_bytes = buffered_bytes.saturating_add(bytes);
        buffered.push(batch);
        if buffered_bytes >= target {
            output.stage(&concat_batches(&schema, &buffered)?)?;
            buffered.clear();
            buffered_bytes = 0;
        }
    }
    if !buffered.is_empty() {
        output.stage(&concat_batches(&schema, &buffered)?)?;
    }
    budget.check()?;
    if rows == 0 || !same_file(&metadata, &regular_file(path)?) {
        return Err(failure("source changed or contains no rows"));
    }
    let original_sha256 = hash_file(path, Some(deadline))?;
    if !same_file(&metadata, &regular_file(path)?) {
        return Err(failure("source changed while verifying"));
    }
    let staged = output.staged_files();
    let output_bytes = staged.iter().try_fold(0u64, |sum, (temporary, _)| {
        sum.checked_add(regular_file(temporary)?.len())
            .ok_or_else(|| failure("byte count overflow"))
    })?;
    let receipt = RepackReceipt {
        source,
        backup,
        outputs: staged
            .iter()
            .map(|(_, published)| file_name(published))
            .collect::<Result<_>>()?,
        output_sha256: staged
            .iter()
            .map(|(temporary, _)| hash_file(temporary, Some(deadline)))
            .collect::<Result<_>>()?,
        original_sha256,
        rows,
        original_bytes: metadata.len(),
        output_bytes,
        original_created_at: metadata.created().ok().or_else(|| metadata.modified().ok()),
    };
    let temporary = receipt_path.with_extension("json.tmp");
    let mut file = File::create(&temporary).map_err(io_error)?;
    file.write_all(&serde_json::to_vec(&receipt)?)
        .map_err(io_error)?;
    file.sync_all().map_err(io_error)?;
    fs::rename(&temporary, &receipt_path).map_err(io_error)?;
    File::open(directory)
        .map_err(io_error)?
        .sync_all()
        .map_err(io_error)?;
    output.publish()?;
    if !same_file(&metadata, &regular_file(path)?) {
        return Err(failure("source changed before retirement"));
    }
    // The receipt is durable before the original leaves the reader's namespace.
    fs::rename(path, directory.join(&receipt.backup)).map_err(io_error)?;
    File::open(directory)
        .map_err(io_error)?
        .sync_all()
        .map_err(io_error)?;
    Ok(receipt_path)
}

fn resume_pending(
    path: &Path,
    source: &str,
    directory: &Path,
    deadline: Instant,
    max_bytes: u64,
) -> Result<Option<PathBuf>> {
    let prefix = format!("{source}.repair.");
    for entry in fs::read_dir(directory).map_err(io_error)? {
        if Instant::now() >= deadline {
            return Err(failure("deadline exceeded"));
        }
        let receipt_path = entry.map_err(io_error)?.path();
        let Some(name) = receipt_path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !name.starts_with(&prefix)
            || receipt_path.extension() != Some(std::ffi::OsStr::new("json"))
        {
            continue;
        }
        let receipt = read_repack_receipt(&receipt_path)?;
        validate_receipt(&receipt, &receipt_path)?;
        if receipt.source != source {
            return Err(failure("invalid pending receipt"));
        }
        let complete = receipt
            .outputs
            .iter()
            .zip(&receipt.output_sha256)
            .try_fold(true, |complete, (name, hash)| {
                let output = directory.join(safe_name(name)?);
                Ok::<_, AllSourceError>(
                    complete
                        && output.is_file()
                        && regular_file(&output)?.len() <= max_bytes
                        && hash_file(&output, Some(deadline))? == *hash,
                )
            })?;
        let backup = directory.join(safe_name(&receipt.backup)?);
        let original = if path.exists() { path } else { &backup };
        if hash_file(original, Some(deadline))? != receipt.original_sha256 {
            return Err(failure("original fingerprint differs from pending receipt"));
        }
        if complete {
            clear_pending_output(directory, &receipt.outputs)?;
            if !path.exists() {
                return Ok(Some(receipt_path));
            }
            if backup.exists() {
                return Err(failure("backup already exists"));
            }
            fs::rename(path, backup).map_err(io_error)?;
            File::open(directory)
                .map_err(io_error)?
                .sync_all()
                .map_err(io_error)?;
            return Ok(Some(receipt_path));
        }
        rollback_locked(&receipt_path, deadline)?;
    }
    Ok(None)
}

/// Restore an original retained by repacking; keep replacement files recoverable.
pub fn rollback_archive_repack(receipt_path: &Path) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(240);
    let _mutation = archive_mutation_guard(Some(deadline))
        .ok_or_else(|| failure("mutation lock deadline exceeded"))?;
    let directory = receipt_path
        .parent()
        .ok_or_else(|| failure("receipt directory missing"))?;
    let _directory = archive_directory_guard(directory)?;
    rollback_locked(receipt_path, deadline)
}

fn validate_receipt(receipt: &RepackReceipt, receipt_path: &Path) -> Result<()> {
    let expected = format!("{}.repair.", receipt.source);
    let name = file_name(receipt_path)?;
    let id = name
        .strip_prefix(&expected)
        .and_then(|rest| rest.strip_suffix(".json"))
        .ok_or_else(|| failure("invalid archive receipt name"))?;
    uuid::Uuid::parse_str(id).map_err(|_| failure("invalid repair generation"))?;
    let output_kind = if receipt.source.starts_with("snapshot.") {
        "snapshot"
    } else {
        "events"
    };
    if receipt.outputs.is_empty()
        || receipt.outputs.len() != receipt.output_sha256.len()
        || !receipt.source.ends_with(".parquet")
        || receipt.backup != format!("{}.original.{id}", receipt.source)
        || receipt.outputs.iter().any(|name| {
            !name.starts_with(&format!("{output_kind}.repacked.{id}."))
                || !name.ends_with(".parquet")
        })
        || receipt
            .output_sha256
            .iter()
            .chain(std::iter::once(&receipt.original_sha256))
            .any(|hash| hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
        || receipt
            .outputs
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            != receipt.outputs.len()
    {
        return Err(failure("invalid archive receipt names"));
    }
    safe_name(&receipt.source)?;
    safe_name(&receipt.backup)?;
    for output in &receipt.outputs {
        safe_name(output)?;
    }
    Ok(())
}

fn rollback_locked(receipt_path: &Path, deadline: Instant) -> Result<()> {
    let receipt = read_repack_receipt(receipt_path)?;
    validate_receipt(&receipt, receipt_path)?;
    let directory = receipt_path
        .parent()
        .ok_or_else(|| failure("receipt directory missing"))?;
    let source = directory.join(safe_name(&receipt.source)?);
    let backup = directory.join(safe_name(&receipt.backup)?);
    let outputs = receipt
        .outputs
        .iter()
        .map(|name| {
            let path = directory.join(safe_name(name)?);
            if path == source || path == backup || path == receipt_path {
                return Err(failure("receipt paths overlap"));
            }
            Ok(path)
        })
        .collect::<Result<Vec<_>>>()?;
    for (path, expected) in outputs.iter().zip(&receipt.output_sha256) {
        let retained = path.with_extension("parquet.rolled-back");
        if path.exists() {
            if retained.exists() {
                return Err(failure("rollback destination already exists"));
            }
            if hash_file(path, Some(deadline))? != *expected {
                return Err(failure("replacement fingerprint differs"));
            }
        } else if retained.exists() {
            if hash_file(&retained, Some(deadline))? != *expected {
                return Err(failure("retained replacement fingerprint differs"));
            }
        } else if backup.exists() {
            return Err(failure(
                "replacement was consumed or removed; refusing rollback",
            ));
        }
    }
    if backup.exists() {
        if source.exists() {
            return Err(failure("source already exists; refusing replacement"));
        }
        if hash_file(&backup, Some(deadline))? != receipt.original_sha256 {
            return Err(failure("backup fingerprint differs"));
        }
        fs::rename(&backup, &source).map_err(io_error)?;
    } else if hash_file(&source, Some(deadline))? != receipt.original_sha256 {
        return Err(failure("original fingerprint differs"));
    }
    File::open(directory)
        .map_err(io_error)?
        .sync_all()
        .map_err(io_error)?;
    for path in outputs {
        if Instant::now() >= deadline {
            return Err(failure("deadline exceeded"));
        }
        let retained = path.with_extension("parquet.rolled-back");
        if path.exists() {
            if retained.exists() {
                return Err(failure("rollback destination already exists"));
            }
            regular_file(&path)?;
            fs::rename(&path, &retained).map_err(io_error)?;
        }
    }
    File::open(directory)
        .map_err(io_error)?
        .sync_all()
        .map_err(io_error)?;
    clear_pending_output(directory, &receipt.outputs)?;
    if receipt_path.exists() {
        fs::rename(
            receipt_path,
            receipt_path.with_extension("json.rolled-back"),
        )
        .map_err(io_error)?;
        File::open(directory)
            .map_err(io_error)?
            .sync_all()
            .map_err(io_error)?;
    }
    Ok(())
}

fn safe_name(name: &str) -> Result<&str> {
    let mut parts = Path::new(name).components();
    if !matches!(parts.next(), Some(Component::Normal(_))) || parts.next().is_some() {
        return Err(failure("invalid receipt filename"));
    }
    Ok(name)
}

fn file_name(path: &Path) -> Result<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .ok_or_else(|| failure("invalid archive filename"))
}

fn regular_file(path: &Path) -> Result<Metadata> {
    let metadata = fs::symlink_metadata(path).map_err(io_error)?;
    if !metadata.is_file() {
        return Err(failure("archive is not a regular file"));
    }
    Ok(metadata)
}

fn same_file(first: &Metadata, second: &Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if first.ino() != second.ino() || first.dev() != second.dev() {
            return false;
        }
    }
    first.len() == second.len() && first.modified().ok() == second.modified().ok()
}

fn hash_file(path: &Path, deadline: Option<Instant>) -> Result<String> {
    regular_file(path)?;
    let deadline = deadline.unwrap_or_else(|| Instant::now() + Duration::from_secs(240));
    let mut file = File::open(path).map_err(io_error)?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024].into_boxed_slice();
    loop {
        if Instant::now() >= deadline {
            return Err(failure("deadline exceeded"));
        }
        let bytes = file.read(&mut buffer).map_err(io_error)?;
        if bytes == 0 {
            break;
        }
        hash.update(&buffer[..bytes]);
    }
    Ok(hex::encode(hash.finalize()))
}

fn io_error(error: impl std::error::Error) -> AllSourceError {
    failure(&format!("I/O failed: {error}"))
}
fn failure(message: &str) -> AllSourceError {
    AllSourceError::StorageError(format!("Archive repack: {message}"))
}

#[cfg(test)]
#[path = "archive_repack_tests.rs"]
mod tests;
