//! Offline, exact-stream erasure of local durable event files. Core writers,
//! replicas and backup jobs must be stopped before calling these operations.
//! Each replacement is atomic; interrupted multi-file work must be retried and
//! verified across every retained copy before restarting consumers.

use super::{
    archive_repack::{archive_directory_guard, archive_mutation_guard},
    wal::WALEntry,
};
use crate::error::{AllSourceError, Result};
use arrow::{
    array::{Array, BooleanArray, StringArray},
    compute::filter_record_batch,
};
use parquet::arrow::{ArrowWriter, arrow_reader::ParquetRecordBatchReaderBuilder};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::{self, File, Metadata, OpenOptions},
    io::{BufRead, BufReader, Read, Seek, SeekFrom, Write},
    path::{Component, Path},
    time::Instant,
};

const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_DECODED_BYTES: usize = 128 * 1024 * 1024;

pub struct ErasureScope {
    pub(super) tenant: String,
    pub(super) entities: HashSet<String>,
}

impl ErasureScope {
    pub fn new(tenant: String, entities: Vec<String>) -> Result<Self> {
        if tenant.is_empty()
            || tenant.len() > 256
            || !tenant
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
            || entities.is_empty()
            || entities.len() > 10_000
            || entities.iter().any(|id| id.is_empty() || id.len() > 1024)
        {
            return Err(failure(
                "an exact tenant and nonempty, bounded entity list are required",
            ));
        }
        Ok(Self {
            tenant,
            entities: entities.into_iter().collect(),
        })
    }

    pub(super) fn matches(&self, tenant: &str, entity: &str) -> bool {
        tenant == self.tenant && self.entities.contains(entity)
    }
}

#[derive(Debug, Default, PartialEq, Eq, Serialize)]
pub struct ErasureReport {
    pub examined: u64,
    pub removed: u64,
}

pub(super) fn failure(message: &str) -> AllSourceError {
    AllSourceError::StorageError(format!("Local event erasure: {message}"))
}

pub(super) fn io_error(error: impl std::fmt::Display) -> AllSourceError {
    failure(&error.to_string())
}

pub(super) fn check_deadline(deadline: Instant) -> Result<()> {
    if Instant::now() >= deadline {
        return Err(failure("deadline exceeded"));
    }
    Ok(())
}

pub(super) fn regular_file(path: &Path) -> Result<Metadata> {
    let meta = fs::symlink_metadata(path).map_err(io_error)?;
    if !meta.is_file() || meta.len() > MAX_FILE_BYTES {
        return Err(failure(
            "source must be a regular file within the byte limit",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.nlink() != 1 {
            return Err(failure(
                "hard-linked sources require separate copy inventory",
            ));
        }
    }
    Ok(meta)
}

fn unchanged(before: &Metadata, after: &Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if before.ino() != after.ino() || before.dev() != after.dev() {
            return false;
        }
    }
    before.len() == after.len() && before.modified().ok() == after.modified().ok()
}

pub(super) fn digest(file: &mut File, deadline: Instant) -> Result<Vec<u8>> {
    file.seek(SeekFrom::Start(0)).map_err(io_error)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 16 * 1024];
    loop {
        check_deadline(deadline)?;
        let count = file.read(&mut buffer).map_err(io_error)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    file.seek(SeekFrom::Start(0)).map_err(io_error)?;
    Ok(hash.finalize().to_vec())
}

pub(super) fn rewrite(
    path: &Path,
    deadline: Instant,
    transform: impl FnOnce(&mut File, &mut File) -> Result<ErasureReport>,
) -> Result<ErasureReport> {
    check_deadline(deadline)?;
    let _mutation = archive_mutation_guard(Some(deadline))
        .ok_or_else(|| failure("maintenance lock deadline exceeded"))?;
    let parent = path
        .parent()
        .ok_or_else(|| failure("source parent missing"))?;
    if parent.canonicalize().map_err(io_error)? != parent {
        return Err(failure(
            "use canonical absolute paths without symlink ancestors",
        ));
    }
    let _directory = archive_directory_guard(parent)?;
    let before = regular_file(path)?;
    let mut source = File::open(path).map_err(io_error)?;
    if !unchanged(&before, &source.metadata().map_err(io_error)?) {
        return Err(failure("source changed before opening"));
    }
    let original_hash = digest(&mut source, deadline)?;
    let temporary = parent.join(format!(".erasure-{}.pending", uuid::Uuid::new_v4()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut output = options.open(&temporary).map_err(io_error)?;
    let result = (|| {
        let report = transform(&mut source, &mut output)?;
        check_deadline(deadline)?;
        if report.removed == 0 {
            return Ok(report);
        }
        output.sync_all().map_err(io_error)?;
        let current = regular_file(path)?;
        if !unchanged(&before, &current) || digest(&mut source, deadline)? != original_hash {
            return Err(failure("source changed during maintenance"));
        }
        fs::rename(&temporary, path).map_err(io_error)?;
        File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(io_error)?;
        Ok(report)
    })();
    drop(output);
    match fs::remove_file(&temporary) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(io_error(error)),
    }
    result
}

/// Erase exact tenant/entity matches; retain original sequence numbers and bytes
/// for all other WAL entries. Any unreadable or invalid entry rejects the rewrite.
pub fn erase_wal_segment(
    path: &Path,
    scope: &ErasureScope,
    deadline: Instant,
) -> Result<ErasureReport> {
    rewrite(path, deadline, |source, output| {
        let (report, sequence) = scan_wal(source, output, scope, deadline)?;
        if report.removed > 0 {
            super::wal_sequence::preserve_floor(
                path.parent().ok_or_else(|| failure("WAL parent missing"))?,
                sequence,
            )?;
        }
        Ok(report)
    })
}

pub(super) fn scan_wal(
    source: &mut File,
    output: &mut dyn Write,
    scope: &ErasureScope,
    deadline: Instant,
) -> Result<(ErasureReport, u64)> {
    let mut report = ErasureReport::default();
    let mut sequence = 0;
    for line in BufReader::new(source).lines() {
        check_deadline(deadline)?;
        let line = line.map_err(io_error)?;
        if line.trim().is_empty() {
            continue;
        }
        let entry: WALEntry = serde_json::from_str(&line)?;
        if !entry.verify() {
            return Err(failure("invalid WAL checksum"));
        }
        sequence = sequence.max(entry.sequence);
        report.examined += 1;
        if scope.matches(entry.event.tenant_id_str(), entry.event.entity_id_str()) {
            report.removed += 1;
        } else {
            writeln!(output, "{line}").map_err(io_error)?;
        }
    }
    Ok((report, sequence))
}

/// Rewrite one partitioned event archive or compaction snapshot. Tenant identity
/// comes from the storage layout; row filtering preserves every existing column.
pub fn erase_parquet_file(
    root: &Path,
    path: &Path,
    scope: &ErasureScope,
    deadline: Instant,
) -> Result<ErasureReport> {
    if root.canonicalize().map_err(io_error)? != root {
        return Err(failure("storage root must be canonical"));
    }
    let relative = path
        .strip_prefix(root)
        .map_err(|_| failure("archive outside storage root"))?;
    let parts = relative.components().collect::<Vec<_>>();
    if parts.len() != 3
        || parts
            .iter()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(failure("partitioned tenant/month/file layout required"));
    }
    let tenant = parts[0]
        .as_os_str()
        .to_str()
        .ok_or_else(|| failure("invalid tenant path"))?;
    if tenant != scope.tenant {
        return Err(failure("archive belongs to a different tenant"));
    }
    if !path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.ends_with(".parquet")
                || name.contains(".parquet.")
                || name.starts_with(".erasure-") && name.ends_with(".pending")
        })
    {
        return Err(failure("expected a Parquet archive"));
    }
    rewrite(path, deadline, |source, output| {
        scan_parquet(source, Some(output), scope, deadline)
    })
}

pub(super) fn scan_parquet(
    source: &mut File,
    output: Option<&mut File>,
    scope: &ErasureScope,
    deadline: Instant,
) -> Result<ErasureReport> {
    let reader = ParquetRecordBatchReaderBuilder::try_new(source.try_clone().map_err(io_error)?)?;
    let declared = reader
        .metadata()
        .row_groups()
        .iter()
        .try_fold(0u64, |sum, group| {
            let bytes = u64::try_from(group.total_byte_size())
                .map_err(|_| failure("invalid row-group size"))?;
            sum.checked_add(bytes)
                .ok_or_else(|| failure("row-group size overflow"))
        })?;
    if declared > MAX_DECODED_BYTES as u64 {
        return Err(failure("archive exceeds decoded byte limit"));
    }
    let mut writer = output
        .map(|output| ArrowWriter::try_new(output, reader.schema().clone(), None))
        .transpose()?;
    let mut report = ErasureReport::default();
    for batch in reader.with_batch_size(64).build()? {
        check_deadline(deadline)?;
        let batch = batch?;
        if batch.get_array_memory_size() > MAX_DECODED_BYTES {
            return Err(failure("batch exceeds decoded byte limit"));
        }
        let ids = batch
            .column_by_name("entity_id")
            .and_then(|column| column.as_any().downcast_ref::<StringArray>())
            .ok_or_else(|| failure("invalid entity_id column"))?;
        if ids.null_count() > 0 {
            return Err(failure("null entity identity"));
        }
        let keep = BooleanArray::from(
            ids.iter()
                .map(|id| !scope.entities.contains(id.unwrap_or_default()))
                .collect::<Vec<_>>(),
        );
        let filtered = filter_record_batch(&batch, &keep)?;
        report.examined += batch.num_rows() as u64;
        report.removed += (batch.num_rows() - filtered.num_rows()) as u64;
        if let Some(writer) = writer.as_mut() {
            writer.write(&filtered)?;
        }
    }
    if let Some(writer) = writer {
        writer.close()?;
    }
    Ok(report)
}
