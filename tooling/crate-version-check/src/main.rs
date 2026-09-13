//! Intra-repo version-requirement gate.
//!
//! A dependency on a sibling crate carries two facts that must agree:
//!
//! ```toml
//! allsource-core = { version = "0.23.1", path = "../../apps/core" }
//! ```
//!
//! `path` decides what cargo compiles locally; `version` decides what the
//! registry serves after `cargo publish` strips the path. When they disagree the
//! crate does not resolve at all:
//!
//! ```text
//! error: failed to select a version for the requirement `allsource-core = "^0.23.1"`
//! candidate versions found which didn't match: 0.24.0
//! ```
//!
//! That is not hypothetical. The v0.24.0 release bumped `apps/core` to 0.24.0
//! across sixteen files and left `tooling/allsource-mcp` requiring `^0.23.1`, so
//! that crate could not build on main until the next commit. It was caught by a
//! `cargo publish` that aborted, not by a gate — and had the publish used
//! `--no-verify`, a broken requirement would be permanently on crates.io, where
//! a version can be yanked but never corrected.
//!
//! `scripts/check-versions.sh` does not cover this: it compares TOOLCHAIN
//! versions (Rust, Go, Node) across Dockerfiles, CI and manifests. Nothing
//! compared one crate's requirement against a sibling's actual version.
//!
//! Seven manifests in this repo declare `allsource-core`, so the defect has that
//! many doors. This gate closes all of them at once.
//!
//! Run from the repo root:
//! `cargo run --manifest-path tooling/crate-version-check/Cargo.toml`

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

/// Directory names never worth descending into.
const SKIP_DIRS: &[&str] = &["target", "node_modules", ".git", "_build", "deps", ".next"];

struct Mismatch {
    manifest: String,
    dependency: String,
    required: String,
    actual: String,
    sibling: String,
}

/// A requirement this gate deliberately does not judge, surfaced so that an
/// unchecked dependency is visible rather than silently assumed fine.
struct Skipped {
    manifest: String,
    dependency: String,
    required: String,
    reason: &'static str,
}

fn main() -> ExitCode {
    let root = match repo_root() {
        Some(root) => root,
        None => {
            eprintln!("crate-version-check: run from inside the repository");
            return ExitCode::FAILURE;
        }
    };

    let manifests = find_manifests(&root);
    let versions = declared_versions(&manifests);

    let mut mismatches = Vec::new();
    let mut skipped = Vec::new();

    for manifest in &manifests {
        let Ok(text) = fs::read_to_string(manifest) else {
            continue;
        };
        let dir = manifest.parent().unwrap_or(&root);

        for dep in path_dependencies(&text) {
            let Some(required) = dep.version else {
                continue;
            };

            let sibling = normalize(&dir.join(&dep.path).join("Cargo.toml"));
            let Some(actual) = versions.get(&sibling) else {
                continue;
            };

            match caret_matches(&required, actual) {
                Some(true) => {}
                Some(false) => mismatches.push(Mismatch {
                    manifest: relative(manifest, &root),
                    dependency: dep.name.clone(),
                    required: required.clone(),
                    actual: actual.clone(),
                    sibling: relative(Path::new(&sibling), &root),
                }),
                None => skipped.push(Skipped {
                    manifest: relative(manifest, &root),
                    dependency: dep.name.clone(),
                    required: required.clone(),
                    reason: "requirement is not a plain caret version",
                }),
            }
        }
    }

    for skip in &skipped {
        println!(
            "note: {} — `{} = \"{}\"` not checked ({})",
            skip.manifest, skip.dependency, skip.required, skip.reason
        );
    }

    if mismatches.is_empty() {
        println!(
            "crate-version-check: OK — every intra-repo path dependency matches its sibling's version"
        );
        return ExitCode::SUCCESS;
    }

    eprintln!(
        "\ncrate-version-check: {} version requirement(s) do not match the crate beside them\n",
        mismatches.len()
    );
    for m in &mismatches {
        eprintln!("  {}", m.manifest);
        eprintln!(
            "    {} = {{ version = \"{}\", path = ... }}",
            m.dependency, m.required
        );
        eprintln!("    but {} declares version = \"{}\"", m.sibling, m.actual);
        eprintln!(
            "    fix: set the requirement to \"{}\", or bump {}\n",
            m.actual, m.sibling
        );
    }
    eprintln!("A requirement that does not match its path crate cannot resolve, and");
    eprintln!("publishing it would put an unsatisfiable dependency on crates.io.");
    ExitCode::FAILURE
}

fn repo_root() -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        if dir.join(".git").exists() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

fn find_manifests(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };

            if kind.is_dir() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if SKIP_DIRS.contains(&name.as_ref()) {
                    continue;
                }
                stack.push(path);
            } else if path.file_name().is_some_and(|n| n == "Cargo.toml") {
                found.push(path);
            }
        }
    }
    found
}

/// Map each manifest to the version its own `[package]` declares.
fn declared_versions(manifests: &[PathBuf]) -> HashMap<String, String> {
    let mut versions = HashMap::new();
    for manifest in manifests {
        let Ok(text) = fs::read_to_string(manifest) else {
            continue;
        };
        if let Some(version) = package_version(&text) {
            versions.insert(normalize(manifest), version);
        }
    }
    versions
}

fn package_version(text: &str) -> Option<String> {
    let mut in_package = false;
    for line in text.lines() {
        let line = strip_comment(line);
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_package = trimmed == "[package]";
            continue;
        }
        if in_package {
            if let Some(value) = key_value(trimmed, "version") {
                return Some(value);
            }
        }
    }
    None
}

struct PathDep {
    name: String,
    path: String,
    version: Option<String>,
}

/// Dependency entries that declare BOTH a `path` and a `version`.
///
/// Handles the inline form (`foo = { path = "..", version = ".." }`, which may
/// wrap across lines) and the sub-table form (`[dependencies.foo]`).
fn path_dependencies(text: &str) -> Vec<PathDep> {
    let mut deps = Vec::new();
    let mut section = String::new();
    let mut subtable: Option<(String, String, Option<String>, Option<String>)> = None;
    let mut pending: Option<(String, String)> = None;

    for line in text.lines() {
        let line = strip_comment(line);
        let trimmed = line.trim();

        if trimmed.starts_with('[') && pending.is_none() {
            if let Some((name, _, Some(path), version)) = subtable.take() {
                deps.push(PathDep {
                    name,
                    path,
                    version,
                });
            }
            section = trimmed.trim_matches(['[', ']']).to_string();
            if let Some(name) = dependency_subtable(&section) {
                subtable = Some((name, section.clone(), None, None));
            }
            continue;
        }

        if let Some((_, _, path, version)) = subtable.as_mut() {
            if let Some(value) = key_value(trimmed, "path") {
                *path = Some(value);
            }
            if let Some(value) = key_value(trimmed, "version") {
                *version = Some(value);
            }
            continue;
        }

        if !is_dependency_section(&section) {
            continue;
        }

        // Accumulate an inline table until its braces balance.
        if let Some((name, mut buf)) = pending.take() {
            buf.push(' ');
            buf.push_str(trimmed);
            if balanced(&buf) {
                push_inline(&mut deps, &name, &buf);
            } else {
                pending = Some((name, buf));
            }
            continue;
        }

        let Some((name, rest)) = trimmed.split_once('=') else {
            continue;
        };
        let name = name.trim().trim_matches('"').to_string();
        let rest = rest.trim().to_string();
        if !rest.starts_with('{') {
            continue;
        }
        if balanced(&rest) {
            push_inline(&mut deps, &name, &rest);
        } else {
            pending = Some((name, rest));
        }
    }

    if let Some((name, _, Some(path), version)) = subtable {
        deps.push(PathDep {
            name,
            path,
            version,
        });
    }
    deps
}

fn push_inline(deps: &mut Vec<PathDep>, name: &str, body: &str) {
    let Some(path) = inline_value(body, "path") else {
        return;
    };
    deps.push(PathDep {
        name: name.to_string(),
        path,
        version: inline_value(body, "version"),
    });
}

fn is_dependency_section(section: &str) -> bool {
    matches!(
        section.rsplit('.').next(),
        Some("dependencies") | Some("dev-dependencies") | Some("build-dependencies")
    )
}

/// `[dependencies.foo]` → `foo`; also the dev/build and `target.*` variants.
fn dependency_subtable(section: &str) -> Option<String> {
    let (head, name) = section.rsplit_once('.')?;
    if is_dependency_section(head) {
        Some(name.trim_matches('"').to_string())
    } else {
        None
    }
}

fn balanced(text: &str) -> bool {
    let mut depth = 0i32;
    let mut in_string = false;
    for ch in text.chars() {
        match ch {
            '"' => in_string = !in_string,
            '{' if !in_string => depth += 1,
            '}' if !in_string => depth -= 1,
            _ => {}
        }
    }
    depth <= 0
}

/// `key = "value"` at the start of a trimmed line.
fn key_value(line: &str, key: &str) -> Option<String> {
    let rest = line.strip_prefix(key)?;
    let rest = rest.trim_start();
    let rest = rest.strip_prefix('=')?;
    quoted(rest.trim_start())
}

/// `key = "value"` anywhere inside an inline table body.
fn inline_value(body: &str, key: &str) -> Option<String> {
    let mut rest = body;
    while let Some(idx) = rest.find(key) {
        let before_ok = idx == 0
            || !rest[..idx]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || c == '-' || c == '_');
        let after = rest[idx + key.len()..].trim_start();
        if before_ok {
            if let Some(after) = after.strip_prefix('=') {
                if let Some(value) = quoted(after.trim_start()) {
                    return Some(value);
                }
            }
        }
        rest = &rest[idx + key.len()..];
    }
    None
}

fn quoted(text: &str) -> Option<String> {
    let rest = text.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn strip_comment(line: &str) -> &str {
    let mut in_string = false;
    for (idx, ch) in line.char_indices() {
        match ch {
            '"' => in_string = !in_string,
            '#' if !in_string => return &line[..idx],
            _ => {}
        }
    }
    line
}

fn normalize(path: &Path) -> String {
    fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_string()
}

fn relative(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}

/// Does `actual` satisfy a bare/caret requirement, per cargo's rules?
///
/// `None` when the requirement uses any other operator — this gate reports those
/// rather than guessing at semantics it does not implement.
fn caret_matches(required: &str, actual: &str) -> Option<bool> {
    let req = required.trim();
    if req.contains(',') || req.contains('*') {
        return None;
    }
    let req = match req.strip_prefix('^') {
        Some(rest) => rest,
        None => {
            if req.starts_with(['=', '~', '>', '<']) {
                return None;
            }
            req
        }
    };

    let (r_major, r_minor, r_patch, minor_given, patch_given) = parse_req(req)?;
    let actual = parse_exact(actual)?;
    let lower = (r_major, r_minor, r_patch);

    let upper = if r_major > 0 {
        (r_major + 1, 0, 0)
    } else if !minor_given {
        (1, 0, 0)
    } else if r_minor > 0 {
        (0, r_minor + 1, 0)
    } else if !patch_given {
        (0, 1, 0)
    } else {
        (0, 0, r_patch + 1)
    };

    Some(actual >= lower && actual < upper)
}

type Version = (u64, u64, u64);

fn parse_req(text: &str) -> Option<(u64, u64, u64, bool, bool)> {
    let mut parts = text.split('.');
    let major = parts.next()?.trim().parse().ok()?;
    let minor_raw = parts.next();
    let patch_raw = parts.next();
    let minor = minor_raw
        .map(|p| p.trim().parse().ok())
        .unwrap_or(Some(0))?;
    let patch = patch_raw
        .map(|p| numeric_prefix(p.trim()))
        .unwrap_or(Some(0))?;
    Some((
        major,
        minor,
        patch,
        minor_raw.is_some(),
        patch_raw.is_some(),
    ))
}

fn parse_exact(text: &str) -> Option<Version> {
    let mut parts = text.split('.');
    let major = parts.next()?.trim().parse().ok()?;
    let minor = parts
        .next()
        .map(|p| p.trim().parse().ok())
        .unwrap_or(Some(0))?;
    let patch = parts
        .next()
        .map(|p| numeric_prefix(p.trim()))
        .unwrap_or(Some(0))?;
    Some((major, minor, patch))
}

/// Trailing pre-release/build metadata is ignored; only the numeric core orders.
fn numeric_prefix(text: &str) -> Option<u64> {
    let digits: String = text.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}
