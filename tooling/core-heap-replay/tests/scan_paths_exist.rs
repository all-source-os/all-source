//! Every path the source scans read must exist and match at least one file.

use core_heap_replay::scan::{SCANNED_PATHS, repo_root, resolve_scanned_files};

#[test]
fn every_scanned_path_exists_and_matches_a_file() {
    let root = repo_root();
    for entry in SCANNED_PATHS {
        let files = resolve_scanned_files(&root, entry);
        assert!(
            !files.is_empty(),
            "scanned path {entry} is missing or matches zero files"
        );
    }
}

#[test]
fn scan_covers_the_replay_set_named_in_the_design() {
    for required in [
        "apps/core/src/store.rs",
        "apps/core/src/store_feed.rs",
        "apps/core/src/store_read_model.rs",
        "apps/core/src/store_resident.rs",
        "apps/core/src/infrastructure/persistence/index.rs",
        "apps/core/src/infrastructure/persistence/storage_feed.rs",
        "apps/core/src/application/services/projection.rs",
        "apps/core/src/infrastructure/memory/",
    ] {
        assert!(
            SCANNED_PATHS.contains(&required),
            "scan set lost {required}"
        );
    }
}
