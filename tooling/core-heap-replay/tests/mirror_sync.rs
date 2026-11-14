//! The build mirror. Evaluators build from one stable directory synced from the
//! candidate tree, so cargo fingerprints survive a change of worktree path.

use core_heap_replay::steps::{Budget, sync_mirror};
use std::{
    fs::FileTimes,
    path::PathBuf,
    time::{Duration, SystemTime},
};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mirror-sync-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn write(path: &PathBuf, text: &str) {
    std::fs::create_dir_all(path.parent().expect("parent")).expect("parent dir");
    std::fs::write(path, text).expect("write file");
}

fn mtime(path: &PathBuf) -> SystemTime {
    std::fs::metadata(path)
        .expect("metadata")
        .modified()
        .expect("mtime")
}

fn set_old_mtime(path: &PathBuf) {
    let old = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000);
    let file = std::fs::File::options()
        .write(true)
        .open(path)
        .expect("open for mtime");
    file.set_times(FileTimes::new().set_modified(old))
        .expect("set mtime");
}

#[test]
fn mirror_copies_new_files_and_removes_deleted_ones() {
    let root = scratch("copy");
    let (source, dest) = (root.join("source"), root.join("mirror"));
    write(&source.join("apps/core/src/lib.rs"), "one");
    write(&source.join("Cargo.toml"), "[workspace]");
    write(&dest.join("stale.rs"), "gone");

    sync_mirror(&root, &Budget::new(), &source, &dest).expect("sync");

    assert_eq!(
        std::fs::read_to_string(dest.join("apps/core/src/lib.rs")).unwrap(),
        "one"
    );
    assert!(dest.join("Cargo.toml").is_file());
    assert!(
        !dest.join("stale.rs").exists(),
        "a file absent from the source is removed"
    );
}

#[test]
fn mirror_keeps_the_mtime_of_an_unchanged_file_and_bumps_a_changed_one() {
    let root = scratch("mtime");
    let (source, dest) = (root.join("source"), root.join("mirror"));
    write(&source.join("same.rs"), "same");
    write(&source.join("changed.rs"), "before");
    sync_mirror(&root, &Budget::new(), &source, &dest).expect("first sync");
    set_old_mtime(&dest.join("same.rs"));
    set_old_mtime(&dest.join("changed.rs"));
    let old = mtime(&dest.join("same.rs"));

    write(&source.join("changed.rs"), "after");
    sync_mirror(&root, &Budget::new(), &source, &dest).expect("second sync");

    assert_eq!(
        mtime(&dest.join("same.rs")),
        old,
        "unchanged file keeps its mtime"
    );
    assert!(
        mtime(&dest.join("changed.rs")) > old,
        "changed file gets a new mtime"
    );
}

#[test]
fn mirror_does_not_copy_git_autoresearch_or_target_trees() {
    let root = scratch("excludes");
    let (source, dest) = (root.join("source"), root.join("mirror"));
    write(&source.join(".git"), "gitdir: elsewhere");
    write(
        &source.join(".autoresearch/worktrees/run/baseline/x.rs"),
        "nested",
    );
    write(
        &source.join("tooling/core-heap-replay/target/debug/x"),
        "build",
    );
    write(&source.join("apps/core/src/lib.rs"), "kept");

    sync_mirror(&root, &Budget::new(), &source, &dest).expect("sync");

    assert!(!dest.join(".git").exists());
    assert!(!dest.join(".autoresearch").exists());
    assert!(!dest.join("tooling/core-heap-replay/target").exists());
    assert!(dest.join("apps/core/src/lib.rs").is_file());
}
