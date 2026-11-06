use super::*;
use allsource_core::prime::AssertionSource;

#[test]
fn no_assertion_fields_means_no_assertion() {
    let a = assertion_from_args(&json!({ "type": "contact", "properties": {} }), "source").unwrap();
    assert!(a.is_none());
}

#[test]
fn flat_fields_build_a_validated_assertion() {
    let a = assertion_from_args(
        &json!({
            "source": "extraction",
            "actor": "doc-bot",
            "model": "m-1",
            "run_id": "run-7",
            "derived_from": ["README.md", "node:document:1"],
            "confidence": 0.4,
        }),
        "source",
    )
    .unwrap()
    .unwrap();
    assert_eq!(a.source, AssertionSource::Extraction);
    assert_eq!(a.actor.as_deref(), Some("doc-bot"));
    assert_eq!(a.model.as_deref(), Some("m-1"));
    assert_eq!(a.run_id.as_deref(), Some("run-7"));
    assert_eq!(a.derived_from, vec!["README.md", "node:document:1"]);
    assert_eq!(a.confidence, Some(0.4));
    assert_eq!(a.priority(), 0);
}

#[test]
fn edge_writes_read_the_assertion_source_from_assertion_source() {
    let args = json!({ "source": "node:a:1", "target": "node:b:2", "relation": "r", "assertion_source": "user" });
    let a = assertion_from_args(&args, ASSERTION_SOURCE_KEY)
        .unwrap()
        .unwrap();
    assert_eq!(a.source, AssertionSource::User);
}

#[test]
fn out_of_range_confidence_is_refused_with_the_received_value() {
    for bad in [json!(1.5), json!(-0.1)] {
        let err = assertion_from_args(&json!({ "confidence": bad }), "source").unwrap_err();
        assert!(err.contains("assertion.confidence"), "{err}");
        assert!(err.contains(&bad.to_string()), "{err}");
    }
}

#[test]
fn non_numeric_confidence_and_unknown_source_are_refused() {
    let err = assertion_from_args(&json!({ "confidence": "high" }), "source").unwrap_err();
    assert!(err.contains("\"high\""), "{err}");
    let err = assertion_from_args(&json!({ "source": "unknown" }), "source").unwrap_err();
    assert!(err.contains("assertion.source"), "{err}");
}

#[test]
fn a_hosted_write_records_the_principal_and_ignores_a_claimed_one() {
    let a = hosted_assertion(&json!({}), "source", Some("user-42"))
        .unwrap()
        .unwrap();
    assert_eq!(a.source, AssertionSource::Agent);
    assert_eq!(a.recorded_by.as_deref(), Some("user-42"));

    let a = hosted_assertion(
        &json!({ "source": "user", "actor": "ann", "recorded_by": "mallory" }),
        "source",
        Some("user-42"),
    )
    .unwrap()
    .unwrap();
    assert_eq!(a.source, AssertionSource::User);
    assert_eq!(a.actor.as_deref(), Some("ann"));
    assert_eq!(a.recorded_by.as_deref(), Some("user-42"));

    let a = hosted_assertion(&json!({ "recorded_by": "mallory" }), "source", None).unwrap();
    assert!(a.is_none(), "without a principal nothing is stamped");
}

#[test]
fn explicit_source_priority_overrides_the_derived_one() {
    let a = assertion_from_args(
        &json!({ "source": "import", "source_priority": 42 }),
        "source",
    )
    .unwrap()
    .unwrap();
    assert_eq!(a.priority(), 42);
    let a = assertion_from_args(&json!({ "source": "import" }), "source")
        .unwrap()
        .unwrap();
    assert_eq!(a.priority(), 500);
}

#[test]
fn a_matching_or_absent_tenant_is_not_a_conflict() {
    assert!(conflicting_tenant(None, "t-a").is_none());
    assert!(conflicting_tenant(Some(&json!("t-a")), "t-a").is_none());
    assert!(conflicting_tenant(Some(&json!("t-b")), "t-a").is_some());
    assert!(conflicting_tenant(Some(&json!(7)), "t-a").is_some());
}

#[test]
fn provenance_without_a_projection_falls_back_to_last_write() {
    use chrono::{TimeZone, Utc};
    use std::collections::BTreeMap;

    let obs = |id: &str, secs: i64, status: &str| Observation {
        observed_at: Utc.timestamp_opt(secs, 0).unwrap(),
        recorded_at: None,
        source_priority: 100,
        specificity_score: 0,
        source_event_id: id.to_string(),
        fields: BTreeMap::from([("status".to_string(), json!(status))]),
        assertion: Assertion::agent(),
    };
    let observations = vec![obs("e1", 10, "open"), obs("e2", 20, "closed")];
    let v = provenance_json("node:task:1", "status", &observations, None).unwrap();
    assert_eq!(v["value"], "closed");
    assert_eq!(v["source_event_id"], "e2");
    assert_eq!(v["merge_policy_applied"], "last_write");
    assert_eq!(v["policy_source"], "default");
    assert_eq!(v["assertion"]["source"], "agent");

    let err = provenance_json("node:task:1", "missing", &observations, None).unwrap_err();
    assert!(err.contains("no observation sets it"), "{err}");
}
