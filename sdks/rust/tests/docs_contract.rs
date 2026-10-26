//! Guards the SDK against calling routes the latest-state contract retired
//! (`docs/contracts/latest-state/README.md`, section 5). Core, the Query
//! Service and every SDK dropped them together because their stores carry no
//! tenant key, so a reference reappearing here would point at a route that
//! no longer exists.

use std::{fs, path::PathBuf};

/// Route fragments removed by the contract.
const RETIRED_ROUTES: &[&str] = &[
    "/api/v1/snapshots",
    "/projections/{}/state",
    "/{}/{}/state",
    "/bulk/save",
    "/projections/{}/bulk",
    "/api/query/projected",
];

/// Methods removed with those routes.
const RETIRED_METHODS: &[&str] = &[
    "fn get_projection_state",
    "fn get_projection_state_summary",
    "fn bulk_get_projection_states",
    "fn put_projection_state",
    "fn bulk_put_projection_state",
];

fn sources() -> Vec<(PathBuf, String)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files: Vec<_> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
        .map(|path| {
            let text = fs::read_to_string(&path).unwrap();
            (path, text)
        })
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no sources under {}", dir.display());
    files
}

#[test]
fn no_source_calls_a_retired_route_or_keeps_a_retired_method() {
    for (path, text) in sources() {
        for needle in RETIRED_ROUTES.iter().chain(RETIRED_METHODS) {
            assert!(
                !text.contains(*needle),
                "{} still contains `{needle}`, retired by the latest-state contract",
                path.display()
            );
        }
    }
}

#[test]
fn projection_handle_get_state_reads_the_worker_read_model() {
    let source = include_str!("../src/projection_worker.rs");
    let start = source
        .find("pub async fn get_state<T: DeserializeOwned>")
        .expect("ProjectionHandle::get_state");
    let body: String = source[start..].chars().take(300).collect();
    assert!(
        body.contains("read_model_row"),
        "get_state must read the worker's read model: {body}"
    );
}
