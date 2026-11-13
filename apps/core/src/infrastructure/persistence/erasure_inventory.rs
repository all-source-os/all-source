//! Closed inventory of local Core files; unknown retained copies prevent mutation.

use super::{
    erasure::{
        ErasureReport, ErasureScope, check_deadline, digest, failure, io_error, regular_file,
        scan_parquet, scan_wal,
    },
    erasure_backup,
    erasure_maintenance::LocalCopy,
};
use crate::error::Result;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    time::Instant,
};

#[derive(Clone)]
pub(super) enum Kind {
    Wal,
    Archive(PathBuf),
    Backup,
    Derived,
    Sidecar,
}

pub(super) struct Item {
    pub path: PathBuf,
    pub kind: Kind,
}

pub(super) fn roots(copies: &[LocalCopy]) -> Result<Vec<PathBuf>> {
    let mut roots = Vec::new();
    for copy in copies {
        for path in [&copy.wal_dir, &copy.storage_dir]
            .into_iter()
            .chain(copy.backup_dir.iter())
        {
            if !path.is_absolute()
                || path.parent().is_none()
                || !fs::symlink_metadata(path).map_err(io_error)?.is_dir()
                || path.canonicalize().map_err(io_error)? != *path
            {
                return Err(failure(
                    "copy roots must be canonical, existing directories",
                ));
            }
            if roots
                .iter()
                .any(|prior: &PathBuf| prior.starts_with(path) || path.starts_with(prior))
            {
                return Err(failure("copy roots overlap"));
            }
            roots.push(path.clone());
        }
    }
    roots.sort();
    Ok(roots)
}

fn entries(path: &Path, deadline: Instant) -> Result<Vec<PathBuf>> {
    check_deadline(deadline)?;
    let mut paths = Vec::new();
    for entry in fs::read_dir(path).map_err(io_error)? {
        check_deadline(deadline)?;
        if paths.len() >= 10_000 {
            return Err(failure("directory inventory exceeds limit"));
        }
        let path = entry.map_err(io_error)?.path();
        if fs::symlink_metadata(&path)
            .map_err(io_error)?
            .file_type()
            .is_symlink()
        {
            return Err(failure("inventory contains a symbolic link"));
        }
        paths.push(path);
    }
    paths.sort();
    Ok(paths)
}

fn name(path: &Path) -> Result<&str> {
    path.file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| failure("non-UTF8 inventory name"))
}

fn marker(name: &str) -> bool {
    matches!(
        name,
        ".erasure-maintenance.lock" | ".erasure-in-progress.json" | ".archive-mutation.lock"
    )
}

fn check_inventory_size(items: &[Item]) -> Result<()> {
    if items.len() >= 100_000 {
        return Err(failure("inventory exceeds file limit"));
    }
    Ok(())
}

fn interrupted_temporary(name: &str, root: &Path) -> bool {
    if !root.join(".erasure-in-progress.json").exists() {
        return false;
    }
    [".erasure-meta-", ".erasure-", ".sequence-"]
        .iter()
        .any(|prefix| {
            name.strip_prefix(prefix)
                .and_then(|s| s.strip_suffix(".pending"))
                .is_some_and(|id| uuid::Uuid::parse_str(id).is_ok())
        })
}

pub(super) fn inventory(
    copies: &[LocalCopy],
    scope: &ErasureScope,
    deadline: Instant,
) -> Result<Vec<Item>> {
    let mut items = Vec::new();
    for copy in copies {
        for path in entries(&copy.wal_dir, deadline)? {
            check_inventory_size(&items)?;
            let n = name(&path)?;
            if marker(n) {
                regular_file(&path)?;
                continue;
            }
            if interrupted_temporary(n, &copy.wal_dir) {
                items.push(Item {
                    path,
                    kind: Kind::Derived,
                });
            } else if n == "feed-log" {
                let own = hex::encode(Sha256::digest(scope.tenant.as_bytes()));
                for feed in entries(&path, deadline)? {
                    check_inventory_size(&items)?;
                    let n = name(&feed)?;
                    regular_file(&feed)?;
                    let prefix = n
                        .get(..32)
                        .filter(|s| s.bytes().all(|c| c.is_ascii_hexdigit()));
                    if prefix.is_none()
                        || !matches!(n.get(32..), Some(".feed" | ".feed.tmp" | ".feed.corrupt"))
                    {
                        return Err(failure("unknown durable-feed copy"));
                    }
                    if n.starts_with(&own[..32]) {
                        items.push(Item {
                            path: feed,
                            kind: Kind::Derived,
                        });
                    }
                }
            } else if n == "quarantine" {
                for retained in entries(&path, deadline)? {
                    check_inventory_size(&items)?;
                    regular_file(&retained)?;
                    if marker(name(&retained)?) {
                        continue;
                    }
                    let kind = if interrupted_temporary(name(&retained)?, &copy.wal_dir) {
                        Kind::Derived
                    } else if name(&retained)? == ".erasure-sequence-floor" {
                        Kind::Sidecar
                    } else {
                        Kind::Wal
                    };
                    items.push(Item {
                        path: retained,
                        kind,
                    });
                }
            } else if n.starts_with("wal-")
                && (path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("log"))
                    || n.ends_with(".sealed"))
                || n.starts_with(".erasure-") && n.ends_with(".pending")
            {
                items.push(Item {
                    path,
                    kind: Kind::Wal,
                });
            } else if n == ".erasure-sequence-floor" {
                super::wal_sequence::read_floor(&copy.wal_dir)?;
                items.push(Item {
                    path,
                    kind: Kind::Sidecar,
                });
            } else {
                return Err(failure("unknown retained WAL copy"));
            }
        }
        for tenant in entries(&copy.storage_dir, deadline)? {
            let n = name(&tenant)?;
            if marker(n) || n == "_compaction_state.json" {
                regular_file(&tenant)?;
                continue;
            }
            if interrupted_temporary(n, &copy.storage_dir) {
                items.push(Item {
                    path: tenant,
                    kind: Kind::Derived,
                });
                continue;
            }
            if !fs::symlink_metadata(&tenant).map_err(io_error)?.is_dir() {
                return Err(failure("unpartitioned or unknown storage copy"));
            }
            if n != scope.tenant {
                continue;
            }
            for partition in entries(&tenant, deadline)? {
                check_inventory_size(&items)?;
                let n = name(&partition)?;
                if n.starts_with("_version_manifest.json") {
                    regular_file(&partition)?;
                    items.push(Item {
                        path: partition,
                        kind: Kind::Derived,
                    });
                    continue;
                }
                if !fs::symlink_metadata(&partition).map_err(io_error)?.is_dir() {
                    return Err(failure("unknown tenant storage copy"));
                }
                for path in entries(&partition, deadline)? {
                    check_inventory_size(&items)?;
                    let n = name(&path)?;
                    regular_file(&path)?;
                    if marker(n) {
                        continue;
                    }
                    let kind = if interrupted_temporary(n, &copy.storage_dir) {
                        Kind::Derived
                    } else if n.contains(".repair.")
                        && (path
                            .extension()
                            .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
                            || n.ends_with(".json.rolled-back"))
                    {
                        Kind::Sidecar
                    } else if n.ends_with(".parquet")
                        || n.contains(".parquet.")
                        || n.starts_with(".erasure-") && n.ends_with(".pending")
                    {
                        Kind::Archive(copy.storage_dir.clone())
                    } else {
                        return Err(failure("unknown retained archive copy"));
                    };
                    items.push(Item { path, kind });
                }
            }
        }
        if let Some(root) = &copy.backup_dir {
            for path in entries(root, deadline)? {
                check_inventory_size(&items)?;
                let n = name(&path)?;
                regular_file(&path)?;
                if marker(n) {
                    continue;
                }
                let kind = if interrupted_temporary(n, root) {
                    Kind::Derived
                } else if n.ends_with(".backup.gz") {
                    Kind::Backup
                } else if let Some(id) = n.strip_suffix("_metadata.json") {
                    regular_file(&root.join(format!("{id}.backup.gz")))?;
                    Kind::Sidecar
                } else if let Some(id) = n
                    .strip_prefix(".erasure-")
                    .and_then(|s| s.strip_suffix(".json"))
                {
                    regular_file(&root.join(format!("{id}.backup.gz")))?;
                    Kind::Sidecar
                } else {
                    return Err(failure("unknown retained backup copy"));
                };
                items.push(Item { path, kind });
            }
        }
    }
    if items.len() > 100_000 {
        return Err(failure("inventory exceeds file limit"));
    }
    items.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(items)
}

pub(super) fn inspect(
    item: &Item,
    scope: &ErasureScope,
    scope_hash: &str,
    deadline: Instant,
) -> Result<(ErasureReport, String)> {
    regular_file(&item.path)?;
    let mut file = File::open(&item.path).map_err(io_error)?;
    let before = digest(&mut file, deadline)?;
    let report = match &item.kind {
        Kind::Wal => scan_wal(&mut file, &mut std::io::sink(), scope, deadline)?.0,
        Kind::Archive(_) => scan_parquet(&mut file, None, scope, deadline)?,
        Kind::Backup => erasure_backup::inspect(&item.path, scope, scope_hash, deadline)?,
        Kind::Derived | Kind::Sidecar => ErasureReport::default(),
    };
    let after = digest(&mut File::open(&item.path).map_err(io_error)?, deadline)?;
    if before != after {
        return Err(failure("copy changed during inspection; stop all writers"));
    }
    Ok((report, hex::encode(before)))
}
