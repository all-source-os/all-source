//! System store checkpoints: the folded state of the high-volume system
//! streams plus every other system event, as of one WAL sequence, so a boot
//! reads one file and the WAL tail instead of the whole system history.
//!
//! A checkpoint is one file, `checkpoint-{covered_seq:016x}.json`: a JSON
//! body, a newline, and a JSON footer carrying the body's length and CRC-32.
//! It is written to a temporary name, fsynced, renamed and the directory
//! fsynced, so a crash leaves either the previous set or the new one. A file
//! that fails its footer, or that this binary cannot read, is skipped and
//! never deleted.

use crate::{
    domain::entities::Event,
    error::{AllSourceError, Result},
    infrastructure::persistence::wal::CoveredSegment,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

/// Subdirectory of the system data dir. Binaries without checkpoints list
/// only `wal/`, so they ignore it and replay the whole WAL.
pub const CHECKPOINT_DIR: &str = "checkpoint";
const FORMAT: u32 = 1;
const MAGIC: &str = "allsource-system-checkpoint";
const PREFIX: &str = "checkpoint-";
const SUFFIX: &str = ".json";
/// The newest checkpoints kept, so a damaged newest file still leaves an
/// older one. The WAL is never pruned, so with none a boot replays it whole.
pub const RETAINED: usize = 2;
const REJECTED_DIR: &str = "rejected";

/// One folded stream, tagged with the folding code's own format number.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FoldedStream {
    pub format: u32,
    pub state: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemCheckpoint {
    pub format: u32,
    /// Every system event with a sequence at or below this is in the
    /// checkpoint, verbatim or folded.
    pub covered_seq: u64,
    pub created_at: DateTime<Utc>,
    /// Each WAL segment and the length of it the checkpoint covers.
    pub segments: Vec<CoveredSegment>,
    /// Events written per system domain up to `covered_seq`.
    pub domain_counts: BTreeMap<String, u64>,
    /// The events of every domain that is not folded, in sequence order.
    pub verbatim: Vec<(u64, Event)>,
    /// Folded domains by name.
    pub folds: BTreeMap<String, FoldedStream>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Footer {
    magic: String,
    format: u32,
    covered_seq: u64,
    body_len: u64,
    body_crc32: u32,
}

fn storage(context: &str, e: impl std::fmt::Display) -> AllSourceError {
    AllSourceError::StorageError(format!("{context}: {e}"))
}

fn file_name(covered_seq: u64) -> String {
    format!("{PREFIX}{covered_seq:016x}{SUFFIX}")
}

fn covered_seq_of(path: &Path) -> Option<u64> {
    let name = path.file_name()?.to_str()?;
    let hex = name.strip_prefix(PREFIX)?.strip_suffix(SUFFIX)?;
    u64::from_str_radix(hex, 16).ok()
}

/// Checkpoint files by name, newest first. Reads no contents.
fn list(dir: &Path) -> Result<Vec<(u64, PathBuf)>> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(storage("Failed to read checkpoint directory", e)),
    };
    let mut found = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|e| storage("Failed to read checkpoint entry", e))?
            .path();
        if let Some(seq) = covered_seq_of(&path) {
            found.push((seq, path));
        }
    }
    found.sort_by_key(|(seq, _)| std::cmp::Reverse(*seq));
    Ok(found)
}

fn sync_dir(dir: &Path) -> Result<()> {
    File::open(dir)
        .and_then(|d| d.sync_all())
        .map_err(|e| storage("Failed to sync checkpoint directory", e))
}

/// Read and verify one checkpoint file.
pub fn read(path: &Path) -> Result<SystemCheckpoint> {
    let bytes = fs::read(path).map_err(|e| storage("Failed to read checkpoint", e))?;
    let body_end = bytes
        .strip_suffix(b"\n")
        .unwrap_or(&bytes)
        .iter()
        .rposition(|b| *b == b'\n')
        .ok_or_else(|| storage("Checkpoint has no footer", path.display()))?;
    let footer: Footer = serde_json::from_slice(&bytes[body_end + 1..])
        .map_err(|e| storage("Checkpoint footer unreadable", e))?;
    let body = &bytes[..body_end];
    if footer.magic != MAGIC || footer.format != FORMAT {
        return Err(storage(
            "Checkpoint written in an unknown format",
            format!("{} format {}", footer.magic, footer.format),
        ));
    }
    if footer.body_len != body.len() as u64 || footer.body_crc32 != crc32fast::hash(body) {
        return Err(storage(
            "Checkpoint body fails its checksum",
            path.display(),
        ));
    }
    let checkpoint: SystemCheckpoint =
        serde_json::from_slice(body).map_err(|e| storage("Checkpoint body unreadable", e))?;
    if checkpoint.covered_seq != footer.covered_seq
        || covered_seq_of(path) != Some(footer.covered_seq)
    {
        return Err(storage(
            "Checkpoint sequence disagrees with its name or footer",
            path.display(),
        ));
    }
    Ok(checkpoint)
}

/// The newest checkpoint that verifies, if any.
pub fn load_latest(dir: &Path) -> Option<SystemCheckpoint> {
    let files = match list(dir) {
        Ok(files) => files,
        Err(e) => {
            tracing::error!("System checkpoints unreadable, replaying the whole WAL: {e}");
            return None;
        }
    };
    for (_, path) in files {
        match read(&path) {
            Ok(checkpoint) => return Some(checkpoint),
            Err(e) => tracing::error!("Skipping system checkpoint {}: {e}", path.display()),
        }
    }
    None
}

/// The highest sequence any checkpoint file name on disk claims, readable or
/// not, rejected ones included. A WAL replayed without a checkpoint starts
/// above it, so no sequence a checkpoint covers is ever handed out again.
pub fn highest_named(dir: &Path) -> u64 {
    let mut highest = 0;
    for directory in [dir.to_path_buf(), dir.join(REJECTED_DIR)] {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let Some(start) = name.find(PREFIX) else {
                continue;
            };
            let hex = name[start + PREFIX.len()..]
                .split(['.', '-'])
                .next()
                .unwrap_or_default();
            if let Ok(seq) = u64::from_str_radix(hex, 16) {
                highest = highest.max(seq);
            }
        }
    }
    highest
}

/// Write `checkpoint` durably, then remove verified checkpoints older than
/// the newest [`RETAINED`]. A checkpoint already on disk at the same
/// sequence is kept as it is.
pub fn write(dir: &Path, checkpoint: &SystemCheckpoint) -> Result<PathBuf> {
    fs::create_dir_all(dir).map_err(|e| storage("Failed to create checkpoint directory", e))?;
    let target = dir.join(file_name(checkpoint.covered_seq));
    if target.exists() {
        return Ok(target);
    }
    let temporary = dir.join(format!(".{}.tmp", file_name(checkpoint.covered_seq)));
    let body = serde_json::to_vec(checkpoint).map_err(|e| storage("Checkpoint encode", e))?;
    let footer = serde_json::to_vec(&Footer {
        magic: MAGIC.into(),
        format: FORMAT,
        covered_seq: checkpoint.covered_seq,
        body_len: body.len() as u64,
        body_crc32: crc32fast::hash(&body),
    })
    .map_err(|e| storage("Checkpoint footer encode", e))?;
    {
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&temporary)
            .map_err(|e| storage("Failed to create checkpoint", e))?;
        file.write_all(&body)
            .and_then(|()| file.write_all(b"\n"))
            .and_then(|()| file.write_all(&footer))
            .and_then(|()| file.write_all(b"\n"))
            .and_then(|()| file.sync_all())
            .map_err(|e| storage("Failed to write checkpoint", e))?;
    }
    fs::rename(&temporary, &target).map_err(|e| storage("Failed to publish checkpoint", e))?;
    sync_dir(dir)?;
    retire_old(dir)?;
    Ok(target)
}

/// Delete checkpoints past the newest [`RETAINED`] that this binary can
/// read, and temporary files left by an interrupted write.
fn retire_old(dir: &Path) -> Result<()> {
    for (_, path) in list(dir)?.into_iter().skip(RETAINED) {
        if read(&path).is_ok() {
            fs::remove_file(&path).map_err(|e| storage("Failed to remove checkpoint", e))?;
        }
    }
    let entries = fs::read_dir(dir).map_err(|e| storage("Failed to read checkpoints", e))?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(&format!(".{PREFIX}")) && name.ends_with(".tmp") {
            let _ = fs::remove_file(entry.path());
        }
    }
    Ok(())
}

/// Move every checkpoint aside so the next boot replays the whole WAL. Used
/// when a checkpoint disagrees with a full replay or with the WAL.
pub fn reject_all(dir: &Path) -> Result<usize> {
    let rejected = dir.join(REJECTED_DIR);
    let mut moved = 0;
    let stamp = Utc::now().format("%Y%m%dT%H%M%S%.6fZ");
    for (_, path) in list(dir)? {
        if let Some(name) = path.file_name() {
            fs::create_dir_all(&rejected)
                .map_err(|e| storage("Failed to create rejected dir", e))?;
            let target = rejected.join(format!("{stamp}-{}", name.to_string_lossy()));
            fs::rename(&path, target).map_err(|e| storage("Failed to reject checkpoint", e))?;
            moved += 1;
        }
    }
    if moved > 0 {
        sync_dir(&rejected)?;
        sync_dir(dir)?;
    }
    Ok(moved)
}

pub const fn format() -> u32 {
    FORMAT
}
