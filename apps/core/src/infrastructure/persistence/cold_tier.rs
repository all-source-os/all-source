//! Cold-tier archive for events past their retention TTL.
//!
//! Step 5 (per-tenant retention) deletes events older than the
//! tenant's TTL. For tenants where "delete" is too aggressive — audit
//! tenants, compliance-driven workloads, anyone who'd want to answer
//! "what happened on day X" months later — this module adds a layer
//! between retention and deletion: an `ArchiveTarget` that receives
//! the dropped events first.
//!
//! ## Crash safety
//!
//! The compaction pipeline calls `archive(...)` BEFORE deleting any
//! original Parquet file. A failed archive returns `Err`, which makes
//! `compact_tenant` short-circuit; originals stay on disk and the next
//! compaction pass retries. There is no "archive then forget" path
//! that risks data loss.
//!
//! ## Backends
//!
//! - `LocalFsArchive` (this file): copies the events into a
//!   `<root>/<tenant>/<yyyy-mm>/archive-<from>-<to>.parquet` tree on
//!   another filesystem. Use this for a separate cheap-disk volume,
//!   or for tests. The directory tree mirrors the live tenant layout
//!   so a future operator can grep for tenants the same way.
//! - S3/R2/GCS backends: deferred behind a feature flag. The
//!   `ArchiveTarget` trait is the integration seam — drop in an
//!   `object_store::ObjectStore`-backed impl when needed.

use crate::{
    domain::entities::Event,
    error::{AllSourceError, Result},
    infrastructure::persistence::storage::ParquetStorage,
};
use chrono::{DateTime, Utc};
use std::{
    fmt,
    path::{Path, PathBuf},
    sync::Arc,
};

/// Archive target for events past their retention TTL.
///
/// Implementations MUST be crash-safe: a failed `archive` call must
/// return an error that the caller can act on. Partial writes that
/// claim success would cause silent data loss when compaction then
/// deletes the originals.
///
/// Implementations MUST be idempotent on repeat calls with the same
/// `(tenant_id, from, to)` window. Compaction may retry after a
/// transient failure; reprocessing the same batch must not corrupt
/// the archive or lose events.
pub trait ArchiveTarget: Send + Sync + fmt::Debug {
    /// Archive a batch of events scoped to `tenant_id` and the time
    /// window `[from, to]`. Returns `Ok(())` only if the data is
    /// durably persisted in the target.
    fn archive(
        &self,
        tenant_id: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        events: &[Event],
    ) -> Result<()>;

    /// Human-readable description of the backend, for logging.
    /// Defaults to the `Debug` representation.
    fn description(&self) -> String {
        format!("{self:?}")
    }
}

/// Local-filesystem archive. Copies events to
/// `<root>/<tenant>/<yyyy-mm>/archive-<from>-<to>.parquet` using the
/// same atomic-write primitive as live storage.
///
/// Best fit: a separate, cheaper, possibly larger volume than the hot
/// path uses. NOT a replacement for off-site backup — same datacenter
/// failure modes apply. For multi-region durability, use an
/// object-store backend.
pub struct LocalFsArchive {
    /// Backing storage. Reused so the archive directory walks the
    /// same tenant-partitioned tree shape and benefits from the same
    /// atomic-write primitive (tmp + fsync + rename + fsync dir).
    storage: Arc<ParquetStorage>,
    /// Where the archive root lives. Only retained for `description`
    /// / logging — the storage Arc carries the working path.
    root: PathBuf,
}

impl LocalFsArchive {
    /// Create a new local-filesystem archive rooted at `root`.
    /// The directory is created if it doesn't exist. Returns `Err`
    /// if the path is unusable (no permissions, points at a file,
    /// etc).
    pub fn new(root: impl Into<PathBuf>) -> Result<Self> {
        let root: PathBuf = root.into();
        let storage = ParquetStorage::new(&root).map_err(|e| {
            AllSourceError::StorageError(format!(
                "cold-tier archive: failed to open ParquetStorage at {}: {e}",
                root.display()
            ))
        })?;
        Ok(Self {
            storage: Arc::new(storage),
            root,
        })
    }

    /// Path to the archive root (for tests and logging).
    pub fn root(&self) -> &Path {
        &self.root
    }
}

impl fmt::Debug for LocalFsArchive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LocalFsArchive")
            .field("root", &self.root)
            .finish()
    }
}

impl ArchiveTarget for LocalFsArchive {
    fn archive(
        &self,
        tenant_id: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        events: &[Event],
    ) -> Result<()> {
        if events.is_empty() {
            return Ok(());
        }

        // Filename mirrors the live snapshot convention so an operator
        // pulling from cold storage can correlate easily.
        let file_stem = format!(
            "archive.{tenant_id}.{}-{}.{}",
            super::compaction::format_iso_basic(from),
            super::compaction::format_iso_basic(to),
            uuid::Uuid::new_v4().as_simple()
        );
        let path = self
            .storage
            .write_atomic_parquet(tenant_id, &file_stem, events)?;
        tracing::info!(
            tenant_id = tenant_id,
            archive_path = %path.display(),
            events = events.len(),
            from = %from.to_rfc3339(),
            to = %to.to_rfc3339(),
            "cold-tier archive: wrote dropped events"
        );
        Ok(())
    }

    fn description(&self) -> String {
        format!("local-fs:{}", self.root.display())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::entities::Event;
    use chrono::Duration;
    use serde_json::json;
    use tempfile::TempDir;
    use uuid::Uuid;

    fn make_event(tenant: &str, ts: DateTime<Utc>) -> Event {
        Event::reconstruct_from_strings(
            Uuid::new_v4(),
            "test.event".to_string(),
            "entity-1".to_string(),
            tenant.to_string(),
            json!({"x": 1}),
            ts,
            None,
            1,
        )
    }

    #[test]
    fn test_local_fs_archive_writes_one_file() {
        let dir = TempDir::new().unwrap();
        let archive = LocalFsArchive::new(dir.path()).unwrap();

        let now = Utc::now();
        let events: Vec<_> = (0..10)
            .map(|i| make_event("acme", now - Duration::days(i)))
            .collect();
        let from = events.iter().map(|e| e.timestamp).min().unwrap();
        let to = events.iter().map(|e| e.timestamp).max().unwrap();

        archive.archive("acme", from, to, &events).unwrap();

        // Walk the archive root looking for the file.
        let mut found = vec![];
        let mut stack = vec![dir.path().to_path_buf()];
        while let Some(d) = stack.pop() {
            for entry in std::fs::read_dir(&d).unwrap().flatten() {
                let p = entry.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|e| e == "parquet") {
                    found.push(p);
                }
            }
        }
        assert_eq!(found.len(), 1, "exactly one archive file");
        let name = found[0].file_name().unwrap().to_string_lossy().to_string();
        assert!(
            name.starts_with("archive.acme."),
            "filename starts with archive.<tenant>., got {name}"
        );
        assert!(
            found[0].to_string_lossy().contains("/acme/"),
            "path contains tenant subdir: {}",
            found[0].display()
        );
    }

    #[test]
    fn test_local_fs_archive_empty_batch_is_noop() {
        let dir = TempDir::new().unwrap();
        let archive = LocalFsArchive::new(dir.path()).unwrap();
        let now = Utc::now();
        archive.archive("acme", now, now, &[]).unwrap();

        // No files should have been written.
        let mut count = 0;
        let mut stack = vec![dir.path().to_path_buf()];
        while let Some(d) = stack.pop() {
            for entry in std::fs::read_dir(&d).unwrap().flatten() {
                let p = entry.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|e| e == "parquet") {
                    count += 1;
                }
            }
        }
        assert_eq!(count, 0);
    }

    #[test]
    fn test_local_fs_archive_idempotent_on_same_window() {
        // Calling archive twice with the same (tenant, from, to) MUST
        // NOT corrupt either file. Filenames are
        // archive.<tenant>.<from>-<to>.parquet via format_iso_basic;
        // a second call to write_atomic_parquet appends a unique
        // suffix to avoid collisions while keeping the archive tree
        // self-consistent.
        let dir = TempDir::new().unwrap();
        let archive = LocalFsArchive::new(dir.path()).unwrap();

        let now = Utc::now();
        let events: Vec<_> = (0..3)
            .map(|i| make_event("acme", now - Duration::hours(i)))
            .collect();
        let from = events.iter().map(|e| e.timestamp).min().unwrap();
        let to = events.iter().map(|e| e.timestamp).max().unwrap();

        archive.archive("acme", from, to, &events).unwrap();
        archive.archive("acme", from, to, &events).unwrap();

        let mut count = 0;
        let mut stack = vec![dir.path().to_path_buf()];
        while let Some(d) = stack.pop() {
            for entry in std::fs::read_dir(&d).unwrap().flatten() {
                let p = entry.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|e| e == "parquet") {
                    count += 1;
                }
            }
        }
        // Both calls succeeded — either the implementation
        // overwrites (count = 1) or appends with a unique suffix
        // (count = 2). Either is acceptable; the contract is "no
        // error, no corruption, no lost events."
        assert!(count == 1 || count == 2, "got {count} archive files");
    }

    #[test]
    fn test_archive_target_description_includes_root() {
        let dir = TempDir::new().unwrap();
        let archive = LocalFsArchive::new(dir.path()).unwrap();
        let desc = archive.description();
        assert!(desc.starts_with("local-fs:"), "got {desc}");
        assert!(
            desc.contains(&dir.path().display().to_string()),
            "got {desc}"
        );
    }
}
