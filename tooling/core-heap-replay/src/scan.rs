//! Source scans over the replay path.
//!
//! The replay is reproducible only if every map on its path uses the fixed
//! hasher. These scans fail on a hash constructor that bypasses `HashState`,
//! and fail if a scanned path disappears (a scan of nothing is a pass that
//! never ran).

use std::path::{Path, PathBuf};

/// Replay-path sources, relative to the repository root. Entries ending in
/// `/` are directories and are scanned recursively for `.rs` files.
pub const SCANNED_PATHS: &[&str] = &[
    "apps/core/src/store.rs",
    "apps/core/src/store_feed.rs",
    "apps/core/src/store_read_model.rs",
    "apps/core/src/store_resident.rs",
    "apps/core/src/infrastructure/persistence/index.rs",
    "apps/core/src/infrastructure/persistence/storage_feed.rs",
    "apps/core/src/application/services/projection.rs",
    "apps/core/src/infrastructure/memory/",
    "apps/core/src/infrastructure/security/auth.rs",
    "apps/core/src/infrastructure/security/rate_limit.rs",
];

/// Constructors that must be routed through `HashState` on the replay path.
pub const BANNED: &[&str] = &[
    "HashMap::new(",
    "HashMap::with_capacity(",
    "HashMap::default(",
    "HashSet::new(",
    "HashSet::with_capacity(",
    "HashSet::default(",
    "DashMap::new(",
    "DashMap::with_capacity(",
    "DashMap::default(",
    "RandomState",
    "with_hasher(",
    "with_capacity_and_hasher(",
];

/// Lines the scan allows: the read-model map used for lookup only, and the
/// tenant stats and census paths, which no replay request reaches. Line numbers
/// are of the file as it stands; a moved line needs a new entry.
pub const EXEMPT: &[(&str, usize)] = &[
    ("apps/core/src/store.rs", 445),
    ("apps/core/src/store.rs", 3133),
    ("apps/core/src/store.rs", 3161),
    ("apps/core/src/store.rs", 3197),
    ("apps/core/src/store.rs", 3198),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub file: String,
    pub line: usize,
    pub pattern: &'static str,
}

/// The repository root: this crate lives at `tooling/core-heap-replay`.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root")
}

/// Files a `SCANNED_PATHS` entry names. Empty when the path is missing.
pub fn resolve_scanned_files(root: &Path, entry: &str) -> Vec<PathBuf> {
    let path = root.join(entry);
    if let Some(dir) = entry.strip_suffix('/') {
        let mut files = Vec::new();
        collect_rs(&root.join(dir), &mut files);
        files
    } else if path.is_file() {
        vec![path]
    } else {
        Vec::new()
    }
}

fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Scan every `SCANNED_PATHS` entry under `root`. A missing path is reported as
/// a violation on that path, so the scan cannot pass on an empty set.
pub fn find_violations(root: &Path) -> Vec<Violation> {
    let mut violations = Vec::new();
    for entry in SCANNED_PATHS {
        let files = resolve_scanned_files(root, entry);
        if files.is_empty() {
            violations.push(Violation {
                file: (*entry).to_string(),
                line: 0,
                pattern: "scanned path missing",
            });
        }
        for file in files {
            let rel = file
                .strip_prefix(root)
                .unwrap_or(&file)
                .to_string_lossy()
                .replace('\\', "/");
            let Ok(text) = std::fs::read_to_string(&file) else {
                continue;
            };
            for hit in find_violations_in_text(&rel, &text) {
                if !EXEMPT.contains(&(rel.as_str(), hit.line)) {
                    violations.push(hit);
                }
            }
        }
    }
    violations
}

/// Banned constructors in one file's text. Comment lines are skipped.
pub fn find_violations_in_text(file: &str, text: &str) -> Vec<Violation> {
    let mut out = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim_start().starts_with("//") {
            continue;
        }
        for pattern in BANNED {
            if contains_token(line, pattern) {
                out.push(Violation {
                    file: file.to_string(),
                    line: index + 1,
                    pattern,
                });
            }
        }
    }
    out
}

/// A banned pattern matches when it is not the tail of a longer identifier, so
/// `fast_set_with_capacity(` does not match `with_capacity(`.
fn contains_token(line: &str, pattern: &str) -> bool {
    line.match_indices(pattern).any(|(at, _)| {
        line[..at]
            .chars()
            .next_back()
            .is_none_or(|c| !(c.is_alphanumeric() || c == '_'))
    })
}
