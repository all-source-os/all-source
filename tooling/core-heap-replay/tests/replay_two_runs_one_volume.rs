//! Two replay processes on the same fixture produce identical bytes.
//!
//! Needs a built `heap_replay` example and a generated fixture. Both are named
//! by environment variables that the evaluator sets; the test fails when they
//! are absent so it can never pass by not running.

use core_heap_replay::replay::{ReplayRequest, run_replay_on_clone};
use std::path::PathBuf;

fn required_path(name: &str) -> PathBuf {
    let value = std::env::var(name)
        .unwrap_or_else(|_| panic!("{name} must name the built artifact; run via the evaluator"));
    PathBuf::from(value)
}

#[test]
fn replay_two_runs_one_volume() {
    let replay_bin = required_path("HEAP_REPLAY_BIN");
    let fixture = required_path("HEAP_FIXTURE_DIR");
    let scratch = std::env::temp_dir().join(format!("heap-replay-two-runs-{}", std::process::id()));

    let first = run_replay_on_clone(&ReplayRequest {
        bin: &replay_bin,
        fixture: &fixture,
        scenario: "main",
        scratch: &scratch.join("run-1"),
        envs: &[],
    })
    .expect("first replay");
    let second = run_replay_on_clone(&ReplayRequest {
        bin: &replay_bin,
        fixture: &fixture,
        scenario: "main",
        scratch: &scratch.join("run-2"),
        envs: &[],
    })
    .expect("second replay");

    assert_eq!(
        first.query_digest, second.query_digest,
        "query digest differs"
    );
    assert_eq!(first.geo_digest, second.geo_digest, "geo digest differs");
    assert_eq!(
        first.codec_digest, second.codec_digest,
        "codec digest differs"
    );
    assert_eq!(first.evictions, second.evictions, "evictions differ");
    assert_eq!(
        first.rows_decoded, second.rows_decoded,
        "rows decoded differ"
    );
}
