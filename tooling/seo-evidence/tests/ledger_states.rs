//! The ledger's job is to keep "measured nothing" and "could not measure"
//! apart. These assert the distinction survives serialization, because the
//! JSON is what CI and the drift check actually read.

use seo_evidence::{
    drift,
    ledger::{Entry, Evidence, Ledger},
    pct,
    sources::render,
};

fn entry(source: &str, evidence: Evidence) -> Entry {
    Entry {
        source: source.to_owned(),
        measures: "test".to_owned(),
        evidence,
    }
}

#[test]
fn a_url_prefix_property_is_fully_percent_encoded() {
    assert_eq!(
        pct::encode("https://www.all-source.xyz/"),
        "https%3A%2F%2Fwww.all-source.xyz%2F"
    );
}

#[test]
fn an_empty_observation_is_not_counted_as_a_setup_gap() {
    let ledger = Ledger::new(
        "https://example.test",
        vec![
            entry("a", Evidence::observed_empty("zero rows")),
            entry(
                "b",
                Evidence::setup_required(vec!["KEY".to_owned()], "set KEY", "runbook.md"),
            ),
        ],
    );

    assert_eq!(ledger.coverage.observed, 1);
    assert_eq!(ledger.coverage.setup_required, 1);
    assert_eq!(ledger.coverage.failed, 0);
}

#[test]
fn a_setup_required_entry_carries_no_value_field() {
    let ledger = Ledger::new(
        "https://example.test",
        vec![entry(
            "a",
            Evidence::setup_required(vec!["KEY".to_owned()], "set KEY", "runbook.md"),
        )],
    );
    let json = serde_json::to_value(&ledger).expect("ledger serializes");
    let first = &json["entries"][0];

    assert_eq!(first["state"], "setup_required");
    assert!(
        first.get("value").is_none(),
        "a blocked entry must not expose a value a reader could quote as a measurement"
    );
    assert_eq!(first["unblock"], "set KEY");
}

#[test]
fn losing_an_observation_is_a_regression_that_fails_the_check() {
    let before = Ledger::new(
        "https://example.test",
        vec![entry(
            "gsc.queries",
            Evidence::observed(serde_json::json!(1)),
        )],
    );
    let after = Ledger::new(
        "https://example.test",
        vec![entry(
            "gsc.queries",
            Evidence::setup_required(
                vec!["GSC_SERVICE_ACCOUNT_JSON".to_owned()],
                "set it",
                "r.md",
            ),
        )],
    );

    let result = drift::compare(
        &serde_json::to_string(&before).unwrap(),
        &serde_json::to_string(&after).unwrap(),
    )
    .expect("both ledgers parse");

    assert!(result.has_regressions());
    assert_eq!(result.regressions.len(), 1);
    assert_eq!(result.regressions[0].before, "observed");
    assert_eq!(result.regressions[0].after, "setup_required");
}

#[test]
fn gaining_an_observation_is_not_a_regression() {
    let before = Ledger::new(
        "https://example.test",
        vec![entry(
            "gsc.queries",
            Evidence::setup_required(vec!["K".to_owned()], "set it", "r.md"),
        )],
    );
    let after = Ledger::new(
        "https://example.test",
        vec![entry(
            "gsc.queries",
            Evidence::observed(serde_json::json!(1)),
        )],
    );

    let result = drift::compare(
        &serde_json::to_string(&before).unwrap(),
        &serde_json::to_string(&after).unwrap(),
    )
    .expect("both ledgers parse");

    assert!(!result.has_regressions());
    assert_eq!(result.improvements.len(), 1);
}

#[test]
fn schema_types_are_deduplicated_from_a_rendered_dom() {
    let dom = r#"<script type="application/ld+json">
        {"@type":"Organization","sub":{"@type":"ImageObject"}}</script>
        <script type="application/ld+json">{"@type":"Organization"}</script>"#;

    assert_eq!(
        render::schema_types(dom),
        vec!["ImageObject".to_owned(), "Organization".to_owned()]
    );
}
