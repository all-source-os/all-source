//! Catch a read-only replica up with what its writer has made durable since boot.
//!
//! A replica is never reopened to refresh: `ParquetStorage::new` deletes
//! `*.parquet.tmp` files, which on a live data dir are the writer's in-flight
//! flushes. Refresh only lists, stats and reads.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::Arc,
};

use chrono::{DateTime, Utc};

use super::EventStore;
use crate::{error::Result, infrastructure::persistence::wal::WalSegmentStamp};

/// What one [`EventStore::refresh_from_disk`] call added.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct RefreshReport {
    /// Events not in memory before this call.
    pub new_events: usize,
    /// Parquet files read for the first time.
    pub new_parquet_files: usize,
    /// Whether any WAL segment changed, which forces a WAL re-read.
    pub wal_changed: bool,
    /// Newest timestamp among `new_events`.
    pub newest_event_at: Option<DateTime<Utc>>,
}

/// The disk a replica has already read, so a refresh reads only the difference.
#[derive(Default)]
pub(crate) struct RefreshState {
    seen_parquet: HashSet<PathBuf>,
    wal_stamps: Option<Vec<WalSegmentStamp>>,
}

impl RefreshState {
    pub(crate) fn mark_parquet_seen(&mut self, files: impl IntoIterator<Item = PathBuf>) {
        self.seen_parquet.extend(files);
    }

    pub(crate) fn set_wal_stamps(&mut self, stamps: Vec<WalSegmentStamp>) {
        self.wal_stamps = Some(stamps);
    }
}

impl EventStore {
    /// Load what a live writer has made durable since this replica last looked.
    ///
    /// Reads Parquet files this replica has not read yet, and re-reads the WAL
    /// when any segment's size or mtime changed. Every event goes through the
    /// id dedup in `append_loaded_event`, so overlap between the WAL, a fresh
    /// checkpoint and a compacted file adds nothing twice. Never writes, never
    /// creates or deletes a file.
    ///
    /// A no-op on a writable store: a writer already holds everything it wrote,
    /// and `WriteAheadLog::recover` would reset its sequence counter.
    pub fn refresh_from_disk(&self) -> Result<RefreshReport> {
        let mut report = RefreshReport::default();
        if !self.read_only {
            return Ok(report);
        }

        let mut state = self.refresh_state.lock();
        let mut loaded = Vec::new();
        let mut tenants = HashSet::new();
        // Marked seen only once every fallible step has passed: a failure below
        // drops `loaded`, and its files must be read again next time.
        let mut newly_seen = Vec::new();

        if let Some(storage) = self.storage.as_ref().map(Arc::clone) {
            let storage = storage.read();
            let files = storage.list_parquet_files()?;
            let present: HashSet<&Path> = files.iter().map(PathBuf::as_path).collect();
            // Compaction removes files whose events are already in memory.
            state
                .seen_parquet
                .retain(|path| present.contains(path.as_path()));
            for path in &files {
                if state.seen_parquet.contains(path) {
                    continue;
                }
                let tenant = storage.tenant_id_for_file(path);
                match storage.load_events_from_file_path(path, &tenant) {
                    Ok(events) => {
                        loaded.extend(events);
                        newly_seen.push(path.clone());
                        tenants.insert(tenant);
                        report.new_parquet_files += 1;
                    }
                    Err(error) => tracing::debug!(
                        file = %path.display(),
                        error = %error,
                        "refresh: Parquet file not readable yet; retrying on the next refresh"
                    ),
                }
            }
        }

        if let Some(wal) = self.wal.as_ref() {
            // Stamped before reading, so an append that lands mid-read changes
            // the next stamp and is picked up then.
            let stamps = wal.segment_stamps()?;
            if state.wal_stamps.as_ref() != Some(&stamps) {
                loaded.extend(wal.recover()?);
                state.wal_stamps = Some(stamps);
                report.wal_changed = true;
            }
        }
        state.seen_parquet.extend(newly_seen);

        let _resident = self.cache_residency_gate.read();
        for event in loaded {
            let at = event.timestamp;
            if self.append_loaded_event(event) {
                report.new_events += 1;
                report.newest_event_at = report.newest_event_at.max(Some(at));
            }
        }
        for tenant in &tenants {
            self.tenant_loader.mark_loaded(tenant);
        }

        Ok(report)
    }
}
