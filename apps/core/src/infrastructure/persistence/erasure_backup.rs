//! Backup data and metadata publish separately; a synced receipt makes that gap retryable.

use super::{
    backup::BackupMetadata,
    erasure::{
        ErasureReport, ErasureScope, check_deadline, digest, failure, io_error, regular_file,
        rewrite,
    },
};
use crate::{domain::entities::Event, error::Result};
use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Instant,
};

const MAX_DECODED: u64 = 128 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
struct Pending {
    scope: String,
    before: String,
    after: BackupMetadata,
}

fn paths(path: &Path) -> Result<(PathBuf, PathBuf)> {
    let id = path
        .file_name()
        .and_then(|s| s.to_str())
        .and_then(|s| s.strip_suffix(".backup.gz"))
        .ok_or_else(|| failure("invalid backup name"))?;
    if id.is_empty()
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
    {
        return Err(failure("invalid backup identity"));
    }
    Ok((
        path.with_file_name(format!("{id}_metadata.json")),
        path.with_file_name(format!(".erasure-{id}.json")),
    ))
}

fn hash(path: &Path, deadline: Instant) -> Result<String> {
    regular_file(path)?;
    Ok(hex::encode(digest(
        &mut File::open(path).map_err(io_error)?,
        deadline,
    )?))
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    if regular_file(path)?.len() > 1024 * 1024 {
        return Err(failure("backup metadata exceeds limit"));
    }
    Ok(serde_json::from_reader(
        File::open(path).map_err(io_error)?,
    )?)
}

pub(super) fn publish_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| failure("metadata parent missing"))?;
    if parent.canonicalize().map_err(io_error)? != parent {
        return Err(failure("metadata parent must be canonical"));
    }
    if path.exists() {
        regular_file(path)?;
    }
    let temporary = path.with_file_name(format!(".erasure-meta-{}.pending", uuid::Uuid::new_v4()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary).map_err(io_error)?;
    let result = (|| {
        serde_json::to_writer(&mut file, value)?;
        file.sync_all().map_err(io_error)?;
        fs::rename(&temporary, path).map_err(io_error)?;
        File::open(parent)
            .and_then(|file| file.sync_all())
            .map_err(io_error)
    })();
    drop(file);
    if temporary.exists() {
        fs::remove_file(&temporary).map_err(io_error)?;
    }
    result
}

fn effective_metadata(path: &Path, scope_hash: &str, deadline: Instant) -> Result<BackupMetadata> {
    let (metadata_path, pending_path) = paths(path)?;
    let mut metadata: BackupMetadata = read_json(&metadata_path)?;
    let checksum = hash(path, deadline)?;
    if pending_path.exists() {
        let pending: Pending = read_json(&pending_path)?;
        if pending.scope != scope_hash {
            return Err(failure("backup has a different pending erasure"));
        }
        if checksum == pending.after.checksum {
            metadata = pending.after;
        } else if checksum != pending.before {
            return Err(failure("backup changed during interrupted erasure"));
        }
    }
    if metadata.checksum != checksum
        || !metadata.compressed
        || metadata.size_bytes != regular_file(path)?.len()
        || path.file_name().and_then(|s| s.to_str())
            != Some(format!("{}.backup.gz", metadata.backup_id).as_str())
    {
        return Err(failure("backup metadata or checksum mismatch"));
    }
    Ok(metadata)
}

fn events(path: &Path, metadata: &BackupMetadata, deadline: Instant) -> Result<Vec<Event>> {
    check_deadline(deadline)?;
    let mut decoded = Vec::new();
    GzDecoder::new(File::open(path).map_err(io_error)?)
        .take(MAX_DECODED + 1)
        .read_to_end(&mut decoded)
        .map_err(io_error)?;
    check_deadline(deadline)?;
    if decoded.len() as u64 > MAX_DECODED {
        return Err(failure("backup decoded bytes exceed limit"));
    }
    let events: Vec<Event> = serde_json::from_slice(&decoded)?;
    if events.len() as u64 != metadata.event_count {
        return Err(failure("backup event count mismatch"));
    }
    Ok(events)
}

pub(super) fn inspect(
    path: &Path,
    scope: &ErasureScope,
    scope_hash: &str,
    deadline: Instant,
) -> Result<ErasureReport> {
    let metadata = effective_metadata(path, scope_hash, deadline)?;
    let events = events(path, &metadata, deadline)?;
    Ok(ErasureReport {
        examined: events.len() as u64,
        removed: events
            .iter()
            .filter(|e| scope.matches(e.tenant_id_str(), e.entity_id_str()))
            .count() as u64,
    })
}

pub(super) fn erase(
    path: &Path,
    scope: &ErasureScope,
    scope_hash: &str,
    deadline: Instant,
) -> Result<ErasureReport> {
    let (metadata_path, pending_path) = paths(path)?;
    let metadata = effective_metadata(path, scope_hash, deadline)?;
    let all = events(path, &metadata, deadline)?;
    let examined = all.len() as u64;
    let kept: Vec<_> = all
        .into_iter()
        .filter(|e| !scope.matches(e.tenant_id_str(), e.entity_id_str()))
        .collect();
    let removed = examined - kept.len() as u64;
    if removed > 0 {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        serde_json::to_writer(&mut encoder, &kept)?;
        let encoded = encoder.finish().map_err(io_error)?;
        if encoded.len() > 64 * 1024 * 1024 {
            return Err(failure("replacement backup exceeds file limit"));
        }
        use sha2::{Digest, Sha256};
        let mut after = metadata.clone();
        after.checksum = hex::encode(Sha256::digest(&encoded));
        after.event_count = kept.len() as u64;
        after.size_bytes = encoded.len() as u64;
        rewrite(path, deadline, |_source, output| {
            output.write_all(&encoded).map_err(io_error)?;
            publish_json(
                &pending_path,
                &Pending {
                    scope: scope_hash.into(),
                    before: metadata.checksum.clone(),
                    after: after.clone(),
                },
            )?;
            Ok(ErasureReport { examined, removed })
        })?;
        publish_json(&metadata_path, &after)?;
    } else if pending_path.exists() {
        publish_json(&metadata_path, &metadata)?;
    }
    if pending_path.exists() {
        fs::remove_file(&pending_path).map_err(io_error)?;
        File::open(
            path.parent()
                .ok_or_else(|| failure("backup parent missing"))?,
        )
        .and_then(|file| file.sync_all())
        .map_err(io_error)?;
    }
    Ok(ErasureReport { examined, removed })
}
