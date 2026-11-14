//! Hash constructors on the replay path must go through `HashState`.

use core_heap_replay::scan::{find_violations, find_violations_in_text, repo_root};

#[test]
fn replay_path_has_no_unrouted_hash_constructors() {
    let violations = find_violations(&repo_root());
    assert!(
        violations.is_empty(),
        "unrouted hash constructors:\n{}",
        violations
            .iter()
            .map(|v| format!("{}:{} {}", v.file, v.line, v.pattern))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn negative_control_detects_each_banned_constructor() {
    for pattern in [
        "HashMap::new()",
        "HashSet::with_capacity(4)",
        "DashMap::default()",
        "RandomState::new()",
        "HashMap::with_hasher(x)",
    ] {
        let text = format!("let m = {pattern};\n");
        let hits = find_violations_in_text("control.rs", &text);
        assert_eq!(hits.len(), 1, "control not detected: {pattern}");
    }
}

#[test]
fn routed_constructors_are_not_flagged() {
    let text = "let m = fast_map(); let s = fast_set_with_capacity(4);\n";
    assert!(find_violations_in_text("control.rs", text).is_empty());
}
