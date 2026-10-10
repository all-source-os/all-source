//! Per-tenant record of the highest archived version of each entity.
//!
//! A conditional write must not assign a version that already exists on disk,
//! so it needs one entity's archived high-water mark. Deriving that by reading
//! the tenant's event files makes the cost of a single append scale with the
//! number of files the tenant has ever flushed. One production tenant reached
//! 20,359 files for 148 MB of events, where that derivation cannot finish
//! inside any budget an HTTP request can wait for (gh#321).
//!
//! The manifest answers the question in one file read instead. It names the
//! files it was folded from, so a file it does not name is one it cannot speak
//! for: a resolve consults the manifest and then only the files missing from
//! it. That list is what makes partial progress durable — a resolve that runs
//! out of budget persists the files it managed to fold, and the next attempt
//! starts from there rather than from nothing.

use crate::{
    domain::entities::Event,
    error::{AllSourceError, Result},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, HashMap},
    fs,
    path::Path,
};

/// File name of the manifest inside a tenant's archive directory.
pub(crate) const MANIFEST_FILE_NAME: &str = "_version_manifest.json";

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub(crate) struct VersionManifest {
    /// Highest version folded so far, per entity.
    pub(crate) entities: HashMap<String, u64>,
    /// Archive files these versions were folded from, relative to the
    /// tenant directory. A file absent here has not been accounted for.
    pub(crate) covered: BTreeSet<String>,
}

impl VersionManifest {
    /// Read the manifest for a tenant directory, or `None` when none exists.
    ///
    /// A manifest that cannot be parsed is treated as absent rather than as an
    /// error: it is a derived cache, and rebuilding it costs time where
    /// refusing every conditional write for the tenant costs availability.
    /// Corruption of the event files it was built from is still caught, by the
    /// fold that rebuilds it.
    pub(crate) fn load(tenant_dir: &Path) -> Option<Self> {
        let bytes = fs::read(tenant_dir.join(MANIFEST_FILE_NAME)).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// Replace the tenant's manifest atomically.
    ///
    /// Writes a sibling temporary file and renames it over the target, so a
    /// crash leaves either the previous manifest or this one, never a torn
    /// read. The rename is same-directory, which is what makes it atomic.
    pub(crate) fn store(&self, tenant_dir: &Path) -> Result<()> {
        let target = tenant_dir.join(MANIFEST_FILE_NAME);
        let temporary = tenant_dir.join(format!("{MANIFEST_FILE_NAME}.{}.tmp", std::process::id()));
        let encoded = serde_json::to_vec(self).map_err(|error| {
            AllSourceError::StorageError(format!("Cannot encode version manifest: {error}"))
        })?;
        fs::write(&temporary, &encoded).map_err(|error| {
            AllSourceError::StorageError(format!("Cannot write version manifest: {error}"))
        })?;
        fs::rename(&temporary, &target).map_err(|error| {
            let _ = fs::remove_file(&temporary);
            AllSourceError::StorageError(format!("Cannot publish version manifest: {error}"))
        })
    }

    /// Raise an entity's recorded version. Folding is a max, so folding the
    /// same file twice changes nothing.
    pub(crate) fn observe(&mut self, entity_id: &str, version: u64) {
        self.entities
            .entry(entity_id.to_string())
            .and_modify(|current| *current = (*current).max(version))
            .or_insert(version);
    }

    /// Fold the versions of events that have just been written to `file`.
    ///
    /// Callers that already hold the events do not have to read them back.
    pub(crate) fn observe_file(&mut self, file: &str, events: &[Event]) {
        for event in events {
            if let Ok(version) = u64::try_from(event.version) {
                self.observe(event.entity_id_str(), version);
            }
        }
        self.covered.insert(file.to_string());
    }

    pub(crate) fn version_of(&self, entity_id: &str) -> Option<u64> {
        self.entities.get(entity_id).copied()
    }
}

/// What a resolve could establish about an entity's archived version.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ResolvedVersion {
    /// Every archive file has been accounted for; this is the answer.
    Complete(Option<u64>),
    /// Files remain unaccounted for, so no version can be trusted yet.
    /// Progress was persisted, so a retry resumes rather than restarts.
    Incomplete { remaining: usize },
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn a_stored_manifest_round_trips() {
        let directory = TempDir::new().unwrap();
        let mut manifest = VersionManifest::default();
        manifest.observe("entity-a", 3);
        manifest.observe("entity-a", 1);
        manifest.covered.insert("events-1.parquet".into());
        manifest.store(directory.path()).unwrap();

        let loaded = VersionManifest::load(directory.path()).unwrap();
        assert_eq!(loaded.version_of("entity-a"), Some(3));
        assert_eq!(loaded.version_of("entity-b"), None);
        assert!(loaded.covered.contains("events-1.parquet"));
    }

    #[test]
    fn storing_twice_leaves_no_temporary_behind() {
        let directory = TempDir::new().unwrap();
        VersionManifest::default().store(directory.path()).unwrap();
        VersionManifest::default().store(directory.path()).unwrap();
        let strays: Vec<_> = fs::read_dir(directory.path())
            .unwrap()
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(strays.is_empty(), "{strays:?}");
    }

    /// A manifest is a cache of what the event files say. Unreadable JSON must
    /// read as "nothing folded yet", so the tenant rebuilds instead of being
    /// refused every conditional write.
    #[test]
    fn unparseable_manifest_reads_as_absent() {
        let directory = TempDir::new().unwrap();
        fs::write(directory.path().join(MANIFEST_FILE_NAME), b"{not json").unwrap();
        assert!(VersionManifest::load(directory.path()).is_none());
    }

    #[test]
    fn missing_manifest_reads_as_absent() {
        let directory = TempDir::new().unwrap();
        assert!(VersionManifest::load(directory.path()).is_none());
    }
}
