//! Find store data directories under a root, and open one read-only.
//!
//! One application often keeps several stores (per profile, per workspace), and
//! reading the wrong one returns an empty answer that looks exactly like "no
//! such data". `stores` lists them so the caller picks deliberately.

use std::fs;
use std::path::{Path, PathBuf};

use allsource_core::embedded::{Config, EmbeddedCore};
use anyhow::{Context, Result};
use serde_json::{Value, json};

/// A directory holding `storage/` or `wal/`.
pub fn is_store(dir: &Path) -> bool {
    dir.join("storage").is_dir() || dir.join("wal").is_dir()
}

/// Every store under `root`, depth-first, without descending into a store.
pub fn find(root: &Path, max_depth: usize) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    walk(root, max_depth, &mut found)?;
    found.sort();
    Ok(found)
}

fn walk(dir: &Path, depth_left: usize, found: &mut Vec<PathBuf>) -> Result<()> {
    if is_store(dir) {
        found.push(dir.to_path_buf());
        return Ok(());
    }
    if depth_left == 0 {
        return Ok(());
    }
    let entries = fs::read_dir(dir).with_context(|| format!("cannot list {}", dir.display()))?;
    for entry in entries {
        let entry = entry.with_context(|| format!("cannot read an entry of {}", dir.display()))?;
        let path = entry.path();
        // Skipping an unreadable entry would drop a store from the listing, and
        // the caller reads a short list as "that store does not exist".
        let kind = entry
            .file_type()
            .with_context(|| format!("cannot stat {}", path.display()))?;
        if kind.is_dir() {
            walk(&path, depth_left - 1, found)?;
        }
    }
    Ok(())
}

/// File counts only — no store is opened to list it.
pub fn describe(root: &Path, dir: &Path) -> Result<Value> {
    Ok(json!({
        "dir": dir,
        "relative": dir.strip_prefix(root).unwrap_or(dir),
        "parquet_files": count_files(&dir.join("storage"), "parquet")?,
        "wal_files": count_files(&dir.join("wal"), "log")?,
    }))
}

/// Parquet is partitioned into subdirectories, so this recurses.
///
/// `storage/` and `wal/` are each optional, so a missing directory is a real
/// zero. Every other failure propagates: reporting an unreadable store as
/// having zero files reads as an empty store, which is the one answer this
/// tool exists to make impossible to get by accident.
fn count_files(dir: &Path, extension: &str) -> Result<usize> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(anyhow::Error::new(e).context(format!("cannot list {}", dir.display()))),
    };
    let mut total = 0;
    for entry in entries {
        let entry = entry.with_context(|| format!("cannot read an entry of {}", dir.display()))?;
        let path = entry.path();
        let kind = entry
            .file_type()
            .with_context(|| format!("cannot stat {}", path.display()))?;
        if kind.is_dir() {
            total += count_files(&path, extension)?;
        } else if path.extension().is_some_and(|x| x == extension) {
            total += 1;
        }
    }
    Ok(total)
}

/// Renders what `describe` produced, for `--format table`.
pub fn print_table(stores: &[Value]) {
    if stores.is_empty() {
        println!("No stores found.");
        return;
    }
    let mut table = comfy_table::Table::new();
    table.set_header(vec!["Store", "Parquet files", "WAL files"]);
    for store in stores {
        let text = |key: &str| {
            store
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let count = |key: &str| {
            store
                .get(key)
                .and_then(Value::as_u64)
                .map_or_else(String::new, |n| n.to_string())
        };
        let label = match text("relative").as_str() {
            "" => text("dir"),
            relative => relative.to_string(),
        };
        table.add_row(vec![label, count("parquet_files"), count("wal_files")]);
    }
    println!("{table}");
}

/// Read-only replica: replays the WAL for reads, never truncates it, rejects
/// writes. Never call `shutdown` on it — that syncs the WAL and flushes storage
/// beside whichever process owns the directory (#201).
pub async fn open_read_only(dir: &Path) -> Result<EmbeddedCore> {
    let config = Config::builder()
        .data_dir(dir)
        .single_tenant(true)
        .read_only(true)
        .build()?;
    EmbeddedCore::open(config)
        .await
        .with_context(|| format!("cannot open {} read-only", dir.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use allsource_core::embedded::{IngestEvent, Query};
    use std::time::{SystemTime, UNIX_EPOCH};

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(label: &str) -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default();
            let dir = std::env::temp_dir().join(format!(
                "allsource-inspect-{label}-{}-{nanos}",
                std::process::id()
            ));
            fs::create_dir_all(&dir).expect("scratch dir");
            Self(dir)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn wal_bytes(store: &Path) -> u64 {
        fs::read_dir(store.join("wal"))
            .expect("wal dir")
            .filter_map(std::result::Result::ok)
            .filter_map(|e| e.metadata().ok())
            .map(|m| m.len())
            .sum()
    }

    #[test]
    fn find_lists_nested_stores_without_descending_into_one() {
        let root = Scratch::new("find");
        let profile = root.0.join("profiles/p1/allsource");
        let workspace = root.0.join("profiles/p1/workspaces/org1/allsource");
        fs::create_dir_all(profile.join("wal")).expect("profile store");
        fs::create_dir_all(workspace.join("storage/nested/allsource/wal"))
            .expect("workspace store");
        fs::create_dir_all(root.0.join("profiles/p1/logs")).expect("not a store");

        let found = find(&root.0, 8).expect("walks");
        assert_eq!(found, [profile, workspace]);
        assert!(
            find(&root.0, 1).expect("walks").is_empty(),
            "depth bounds the walk"
        );
    }

    #[tokio::test]
    async fn a_read_only_open_sees_the_unfolded_wal_and_leaves_it_in_place() {
        let scratch = Scratch::new("wal");
        let store = scratch.0.join("allsource");
        let writer = EmbeddedCore::open(
            Config::builder()
                .data_dir(&store)
                .single_tenant(true)
                .build()
                .expect("writer config"),
        )
        .await
        .expect("writer opens");
        for n in 0..3 {
            writer
                .ingest(IngestEvent {
                    entity_id: "run-1",
                    event_type: "run.started",
                    payload: json!({ "n": n }),
                    metadata: None,
                    tenant_id: None,
                })
                .await
                .expect("ingest");
        }
        let before = wal_bytes(&store);
        assert!(before > 0, "the events must still be in the WAL, unfolded");

        let reader = open_read_only(&store)
            .await
            .expect("read-only open beside the writer");
        let seen = reader
            .query(Query::new().event_type_prefix("run."))
            .await
            .expect("query");
        assert_eq!(
            seen.len(),
            3,
            "a reader that skipped the WAL would see nothing"
        );
        drop(reader);

        assert_eq!(
            wal_bytes(&store),
            before,
            "the reader must not truncate the writer's WAL"
        );
        drop(writer);
    }
}
