//! Offline copy-set erasure with preview fingerprints and an interrupted-operation barrier.

use super::{
    erasure::{
        ErasureScope, check_deadline, erase_parquet_file, erase_wal_segment, failure, io_error,
        regular_file,
    },
    erasure_backup,
    erasure_inventory::{self, Item, Kind},
};
use crate::error::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    time::Instant,
};

const MARKER: &str = ".erasure-in-progress.json";

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LocalCopy {
    pub wal_dir: PathBuf,
    pub storage_dir: PathBuf,
    #[serde(deserialize_with = "explicit_backup_dir")]
    pub backup_dir: Option<PathBuf>,
}

fn explicit_backup_dir<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<PathBuf>, D::Error> {
    Option::<PathBuf>::deserialize(deserializer)
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ErasureManifest {
    pub version: u32,
    pub tenant_id: String,
    pub entity_ids: Vec<String>,
    pub copies: Vec<LocalCopy>,
    pub external_copies: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct MaintenanceReport {
    pub phase: &'static str,
    pub manifest_sha256: String,
    pub inventory_sha256: String,
    pub files: usize,
    pub records: u64,
    pub matches: u64,
    pub local_copies_verified: bool,
}

impl ErasureManifest {
    pub fn read(path: &Path) -> Result<Self> {
        if regular_file(path)?.len() > 1024 * 1024 {
            return Err(failure("manifest exceeds byte limit"));
        }
        Ok(serde_json::from_reader(
            File::open(path).map_err(io_error)?,
        )?)
    }

    fn scope(&self) -> Result<ErasureScope> {
        if self.version != 1 || self.copies.is_empty() || self.copies.len() > 32 {
            return Err(failure(
                "manifest version 1 and 1..32 explicit copy sets required",
            ));
        }
        if !self.external_copies.is_empty() {
            return Err(failure(
                "external copies remain: restore or retire them before erasure",
            ));
        }
        ErasureScope::new(self.tenant_id.clone(), self.entity_ids.clone())
    }

    fn fingerprint(&self) -> Result<String> {
        Ok(hex::encode(Sha256::digest(serde_json::to_vec(self)?)))
    }
}

fn pending_matches(roots: &[PathBuf], expected: &str) -> Result<()> {
    for root in roots {
        let path = root.join(MARKER);
        if path.exists() {
            if regular_file(&path)?.len() > 1024 {
                return Err(failure("invalid maintenance barrier"));
            }
            let value: String = serde_json::from_reader(File::open(path).map_err(io_error)?)?;
            if value != expected {
                return Err(failure("a different erasure is unfinished"));
            }
        }
    }
    Ok(())
}

fn plan(
    manifest: &ErasureManifest,
    deadline: Instant,
) -> Result<(MaintenanceReport, Vec<Item>, Vec<PathBuf>)> {
    check_deadline(deadline)?;
    let scope = manifest.scope()?;
    let roots = erasure_inventory::roots(&manifest.copies)?;
    let manifest_sha256 = manifest.fingerprint()?;
    pending_matches(&roots, &manifest_sha256)?;
    let items = erasure_inventory::inventory(&manifest.copies, &scope, deadline)?;
    if items.is_empty() {
        return Err(failure("empty copy inventory cannot prove erasure"));
    }
    let mut hash = Sha256::new();
    hash.update(manifest_sha256.as_bytes());
    let mut records = 0;
    let mut matches = 0;
    for item in &items {
        let (report, digest) =
            erasure_inventory::inspect(item, &scope, &manifest_sha256, deadline)?;
        hash.update(serde_json::to_vec(&(&item.path, digest))?);
        records += report.examined;
        matches += report.removed;
    }
    Ok((
        MaintenanceReport {
            phase: "preview",
            manifest_sha256,
            inventory_sha256: hex::encode(hash.finalize()),
            files: items.len(),
            records,
            matches,
            local_copies_verified: false,
        },
        items,
        roots,
    ))
}

pub fn preview(manifest: &ErasureManifest, deadline: Instant) -> Result<MaintenanceReport> {
    Ok(plan(manifest, deadline)?.0)
}

fn lock_roots(roots: &[PathBuf]) -> Result<Vec<File>> {
    roots
        .iter()
        .map(|root| {
            let path = root.join(".erasure-maintenance.lock");
            if path.exists() {
                regular_file(&path)?;
            }
            let mut options = OpenOptions::new();
            options.read(true).write(true).create(true).truncate(false);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let file = options.open(path).map_err(io_error)?;
            file.try_lock()
                .map_err(|_| failure("another maintenance command owns this copy"))?;
            Ok(file)
        })
        .collect()
}

fn remove_synced(path: &Path) -> Result<()> {
    regular_file(path)?;
    fs::remove_file(path).map_err(io_error)?;
    File::open(path.parent().ok_or_else(|| failure("parent missing"))?)
        .and_then(|f| f.sync_all())
        .map_err(io_error)
}

pub fn apply(
    manifest: &ErasureManifest,
    expected_inventory: &str,
    writers_stopped: bool,
    deadline: Instant,
) -> Result<MaintenanceReport> {
    if !writers_stopped {
        return Err(failure("explicit stopped-writer confirmation required"));
    }
    manifest.scope()?;
    let roots = erasure_inventory::roots(&manifest.copies)?;
    let _locks = lock_roots(&roots)?;
    let (before, items, roots) = plan(manifest, deadline)?;
    if before.inventory_sha256 != expected_inventory {
        return Err(failure("preview is stale; inspect again before confirming"));
    }
    let scope = manifest.scope()?;
    for root in &roots {
        erasure_backup::publish_json(&root.join(MARKER), &before.manifest_sha256)?;
    }
    for item in items {
        check_deadline(deadline)?;
        match item.kind {
            Kind::Wal => {
                erase_wal_segment(&item.path, &scope, deadline)?;
            }
            Kind::Archive(root) => {
                erase_parquet_file(&root, &item.path, &scope, deadline)?;
            }
            Kind::Backup => {
                erasure_backup::erase(&item.path, &scope, &before.manifest_sha256, deadline)?;
            }
            Kind::Derived => {
                remove_synced(&item.path)?;
            }
            Kind::Sidecar => {}
        }
    }
    let (mut after, _, _) = plan(manifest, deadline)?;
    if after.matches != 0 {
        return Err(failure(
            "verification found retained matches; keep writers stopped",
        ));
    }
    for root in &roots {
        remove_synced(&root.join(MARKER))?;
    }
    after.phase = "verified";
    after.local_copies_verified = true;
    Ok(after)
}

pub(crate) fn require_no_pending(storage: Option<&Path>, wal: Option<&Path>) -> Result<()> {
    for root in storage.into_iter().chain(wal) {
        match fs::symlink_metadata(root.join(MARKER)) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            _ => {
                return Err(failure(
                    "unfinished erasure; finish maintenance before starting Core",
                ));
            }
        }
    }
    Ok(())
}
