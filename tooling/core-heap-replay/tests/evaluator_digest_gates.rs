//! The evaluator's digest gates: every digest the replay emits is a hard gate
//! against a committed expected value, as the design's gate table requires.

use core_heap_replay::scan::repo_root;

const EVALUATOR: &str = "src/bin/heap_replay_eval.rs";
const EXPECTED_DIR: &str = "tooling/core-heap-replay/expected";

#[test]
fn every_digest_has_a_hard_gate_in_the_evaluator() {
    let source =
        std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(EVALUATOR))
            .expect("evaluator source");
    for gate in [
        "query_digest_equal",
        "prod_hasher_digest_equal",
        "codec_digest_equal",
        "geo_digest_equal",
        "evictions_match",
    ] {
        assert!(
            source.contains(&format!("\"{gate}\"")),
            "evaluator has no hard gate named {gate}"
        );
    }
}

#[test]
fn every_expected_digest_file_is_committed() {
    let dir = repo_root().join(EXPECTED_DIR);
    for name in [
        "expected-digest.txt",
        "expected-codec-digest.txt",
        "expected-geo-digest.txt",
    ] {
        let text = std::fs::read_to_string(dir.join(name))
            .unwrap_or_else(|e| panic!("{name} missing: {e}"));
        let value = text.trim();
        assert_eq!(value.len(), 64, "{name} is not a sha256 hex digest");
        assert!(
            value.chars().all(|c| c.is_ascii_hexdigit()),
            "{name} is not hex"
        );
    }
}
