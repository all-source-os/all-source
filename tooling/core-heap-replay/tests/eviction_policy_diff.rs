//! `eviction_policy_diff_allowed`: against the oracle commit, the eviction-policy
//! items gain only lines that call the resident ledger, and lose none.

use core_heap_replay::{
    policy::{ITEMS, ORACLE_COMMIT, diff_items, diff_violations},
    scan::repo_root,
};
use std::{collections::BTreeSet, path::PathBuf, process::Command};

const STORE: &str = "apps/core/src/store.rs";

/// The repository whose object store holds the oracle commit. The evaluator
/// builds from a mirror without `.git` and names the candidate worktree here.
fn history_root() -> PathBuf {
    std::env::var_os("CORE_SOURCE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(repo_root)
}

fn git_show(file: &str) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(history_root())
        .arg("show")
        .arg(format!("{ORACLE_COMMIT}:{file}"))
        .output()
        .expect("git runs");
    assert!(output.status.success(), "git show failed for {file}");
    String::from_utf8(output.stdout).expect("utf-8 source")
}

#[test]
fn eviction_policy_diff_allowed() {
    let root = repo_root();
    let mut files: Vec<&str> = Vec::new();
    for (file, _) in ITEMS {
        if !files.contains(file) {
            files.push(file);
        }
    }
    let mut violations = Vec::new();
    for file in files {
        let new = std::fs::read_to_string(root.join(file)).expect("current source");
        violations
            .extend(diff_violations(file, &git_show(file), &new).expect("both versions parse"));
    }
    assert!(
        violations.is_empty(),
        "eviction policy changed outside ledger calls:\n{}",
        violations.join("\n")
    );
}

const OLD: &str = "\
struct EventStore { events_resident: Tally, map: Vec<u8> }
impl EventStore {
    fn try_evict_tenant(&self, t: &str) -> bool {
        let n = self.map.len();
        n > 0
    }
}
";

fn synthetic_diff(old: &str, new: &str) -> Result<Vec<String>, String> {
    diff_items(STORE, &["try_evict_tenant"], old, new)
}

fn with_added_before_tail(line: &str) -> String {
    OLD.replace(
        "        n > 0\n",
        &format!("        {line}\n        n > 0\n"),
    )
}

#[test]
fn ledger_call_on_a_ledger_field_is_allowed() {
    let new =
        with_added_before_tail("self.events_resident.release_matching(&events, |e| e.keep());");
    assert_eq!(
        synthetic_diff(OLD, &new).expect("parses"),
        Vec::<String>::new()
    );
}

#[test]
fn ledger_constructor_call_is_allowed() {
    let new = with_added_before_tail("let guard = Lease::new(Owner::WebhookQueue, 8);");
    assert_eq!(
        synthetic_diff(OLD, &new).expect("parses"),
        Vec::<String>::new()
    );
}

#[test]
fn plain_added_line_is_rejected() {
    let new = with_added_before_tail("let extra = self.map.len();");
    let violations = synthetic_diff(OLD, &new).expect("parses");
    assert_eq!(violations.len(), 1, "{violations:?}");
    assert!(violations[0].contains("added outside a ledger call"));
}

#[test]
fn method_call_on_a_non_ledger_field_is_rejected() {
    let new = with_added_before_tail("self.map.clear();");
    assert_eq!(synthetic_diff(OLD, &new).expect("parses").len(), 1);
}

#[test]
fn removed_line_is_rejected() {
    let new = OLD.replace("        let n = self.map.len();\n", "");
    let violations = synthetic_diff(OLD, &new).expect("parses");
    assert!(
        violations.iter().any(|v| v.contains("removed or changed")),
        "{violations:?}"
    );
}

#[test]
fn ledger_call_wrapped_in_a_new_condition_is_rejected() {
    let new = OLD.replace(
        "        n > 0\n",
        "        if n > 0 {\n            self.events_resident.release_matching(&events, |_| true);\n        }\n        n > 0\n",
    );
    let violations = synthetic_diff(OLD, &new).expect("parses");
    assert!(
        violations
            .iter()
            .any(|v| v.contains("added outside a ledger call")),
        "a new condition around a ledger call must be rejected: {violations:?}"
    );
}

#[test]
fn ledger_fields_are_only_the_ledger_typed_fields() {
    let ast = syn::parse_file(OLD).expect("parses");
    let fields: BTreeSet<String> = core_heap_replay::policy::ledger_fields(&ast);
    assert_eq!(
        fields.into_iter().collect::<Vec<_>>(),
        vec!["events_resident".to_string()]
    );
}
