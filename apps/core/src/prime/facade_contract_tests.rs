use super::*;
use crate::prime::{
    assertion::{AssertionSource, WARNINGS_KEY},
    projections::declarative::{MergePolicy, ProjectionDef, provenance_for_field},
    schema::Violation,
};

fn def(field: &str, policy: MergePolicy) -> ProjectionDef {
    ProjectionDef::new("person", vec![(field.to_string(), policy)])
}

async fn event_count(prime: &Prime) -> usize {
    prime.core().query(Query::new()).await.unwrap().len()
}

fn violation(err: PrimeError) -> Violation {
    match err {
        PrimeError::SchemaViolation(v) => *v,
        other => panic!("expected a schema violation, got {other}"),
    }
}

/// person: age integer 0..=150, status enum, born datetime, employer node_ref
/// company; works_at person → company, cardinality one.
async fn typed_store(mode: EnforcementMode) -> Prime {
    let prime = Prime::open_in_memory().await.unwrap();
    prime
        .register_kind(
            SchemaKind::Node,
            "person",
            json!({
                "required": ["name"],
                "properties": {
                    "name": {"kind": "string", "max_length": 40},
                    "age": {"kind": "integer", "min": 0, "max": 150},
                    "status": {"kind": "enum", "values": ["lead", "customer"]},
                    "born": {"kind": "datetime"},
                    "employer": {"kind": "node_ref", "node_types": ["company"]}
                }
            }),
        )
        .await
        .unwrap();
    prime
        .register_kind(SchemaKind::Node, "company", json!({}))
        .await
        .unwrap();
    prime
        .register_kind(
            SchemaKind::Edge,
            "works_at",
            json!({"from_types": ["person"], "to_types": ["company"], "cardinality": "one"}),
        )
        .await
        .unwrap();
    prime.set_enforcement(mode).await.unwrap();
    prime
}

#[tokio::test]
async fn user_correction_written_before_a_newer_extraction_wins_under_highest_priority() {
    let prime = Prime::open_in_memory().await.unwrap();
    let id = prime
        .add_node_asserted(
            "person",
            json!({"name": "Alice Chen"}),
            Some(Assertion::user().with_actor("alice@example.com")),
        )
        .await
        .unwrap();
    let eid = node_entity_id("person", id.as_str());
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    prime
        .update_node_asserted(
            &eid,
            json!({"name": "alice chen"}),
            Some(
                Assertion::extraction()
                    .with_model("claude-x")
                    .with_run_id("run-1")
                    .with_confidence(0.4),
            ),
        )
        .await
        .unwrap();

    let obs = prime.observations(&eid).await.unwrap();
    let p = provenance_for_field(&obs, &def("name", MergePolicy::HighestPriority), "name").unwrap();
    assert_eq!(p.value, json!("Alice Chen"));
    assert_eq!(p.assertion.source, AssertionSource::User);
    assert_eq!(p.assertion.actor.as_deref(), Some("alice@example.com"));
    assert_eq!(p.source_priority, 1000);

    let newest = provenance_for_field(&obs, &def("name", MergePolicy::LastWrite), "name").unwrap();
    assert_eq!(
        newest.value,
        json!("alice chen"),
        "the extraction is the newer write"
    );
}

#[tokio::test]
async fn extracted_field_provenance_returns_model_run_derivation_confidence_and_event() {
    let prime = Prime::open_in_memory().await.unwrap();
    let id = prime
        .add_node_asserted(
            "person",
            json!({"title": "CTO"}),
            Some(
                Assertion::extraction()
                    .with_model("claude-x")
                    .with_run_id("run-42")
                    .with_derived_from(vec!["doc:deck.pdf".to_string()])
                    .with_confidence(0.82),
            ),
        )
        .await
        .unwrap();
    let eid = node_entity_id("person", id.as_str());
    let obs = prime.observations(&eid).await.unwrap();
    let p = provenance_for_field(&obs, &def("title", MergePolicy::LastWrite), "title").unwrap();

    assert_eq!(p.assertion.source, AssertionSource::Extraction);
    assert_eq!(p.assertion.model.as_deref(), Some("claude-x"));
    assert_eq!(p.assertion.run_id.as_deref(), Some("run-42"));
    assert_eq!(p.assertion.derived_from, vec!["doc:deck.pdf".to_string()]);
    assert_eq!(p.assertion.confidence, Some(0.82));
    let events = prime
        .core()
        .query(Query::new().entity_id(&eid))
        .await
        .unwrap();
    assert_eq!(p.source_event_id, events[0].id.to_string());
}

#[tokio::test]
async fn legacy_event_without_an_assertion_reports_source_unknown() {
    let prime = Prime::open_in_memory().await.unwrap();
    prime
        .core()
        .ingest(IngestEvent {
            entity_id: "node:person:legacy",
            event_type: event_types::NODE_CREATED,
            payload: json!({"id": "legacy", "node_type": "person", "properties": {"name": "Old"}}),
            metadata: None,
            tenant_id: None,
        })
        .await
        .unwrap();
    let obs = prime.observations("node:person:legacy").await.unwrap();
    assert_eq!(obs[0].assertion.source, AssertionSource::Unknown);
    assert_eq!(obs[0].source_priority, 0);
}

#[tokio::test]
async fn unasserted_write_api_records_an_agent_assertion() {
    let prime = Prime::open_in_memory().await.unwrap();
    let id = prime
        .add_node("person", json!({"name": "A"}))
        .await
        .unwrap();
    let obs = prime
        .observations(&node_entity_id("person", id.as_str()))
        .await
        .unwrap();
    assert_eq!(obs[0].assertion.source, AssertionSource::Agent);
    assert_eq!(obs[0].source_priority, 100);
}

#[tokio::test]
async fn invalid_confidence_is_refused_on_every_write_and_appends_no_event() {
    let prime = Prime::open_in_memory().await.unwrap();
    let a = prime.add_node("person", json!({})).await.unwrap();
    let b = prime.add_node("person", json!({})).await.unwrap();
    let (a, b) = (
        node_entity_id("person", a.as_str()),
        node_entity_id("person", b.as_str()),
    );
    let before = event_count(&prime).await;

    for bad in [1.5, -0.1, f64::NAN] {
        let assertion = || Some(Assertion::agent().with_confidence(bad));
        let err = prime
            .add_node_asserted("person", json!({}), assertion())
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("assertion.confidence"),
            "{bad}: {err}"
        );
        assert!(
            prime
                .update_node_asserted(&a, json!({"x": 1}), assertion())
                .await
                .is_err()
        );
        assert!(
            prime
                .add_edge_asserted(&a, &b, "knows", None, None, assertion())
                .await
                .is_err()
        );
    }
    assert_eq!(event_count(&prime).await, before);
}

#[tokio::test]
async fn explicit_priority_overrides_the_derived_one_and_import_derives_500() {
    let prime = Prime::open_in_memory().await.unwrap();
    let id = prime
        .add_node_asserted("person", json!({"n": 1}), Some(Assertion::import()))
        .await
        .unwrap();
    let eid = node_entity_id("person", id.as_str());
    prime
        .update_node_asserted(
            &eid,
            json!({"n": 2}),
            Some(Assertion::extraction().with_source_priority(2000)),
        )
        .await
        .unwrap();
    let obs = prime.observations(&eid).await.unwrap();
    let priorities: Vec<i32> = obs.iter().map(|o| o.source_priority).collect();
    assert_eq!(priorities, vec![500, 2000]);
}

#[tokio::test]
async fn strict_refuses_each_typed_violation_naming_field_expected_and_received() {
    let prime = typed_store(EnforcementMode::Strict).await;
    let p1 = node_entity_id(
        "person",
        prime
            .add_node("person", json!({"name": "Ann"}))
            .await
            .unwrap()
            .as_str(),
    );
    let p2 = node_entity_id(
        "person",
        prime
            .add_node("person", json!({"name": "Bob"}))
            .await
            .unwrap()
            .as_str(),
    );
    let c1 = node_entity_id(
        "company",
        prime.add_node("company", json!({})).await.unwrap().as_str(),
    );
    let c2 = node_entity_id(
        "company",
        prime.add_node("company", json!({})).await.unwrap().as_str(),
    );
    let before = event_count(&prime).await;

    let cases = [
        (json!({"name": "A", "age": "old"}), "age", json!("old")),
        (json!({"name": "A", "age": 200}), "age", json!(200)),
        (
            json!({"name": "A", "status": "vip"}),
            "status",
            json!("vip"),
        ),
        (
            json!({"name": "A", "born": "yesterday"}),
            "born",
            json!("yesterday"),
        ),
        (json!({"name": "A", "employer": p2}), "employer", json!(p2)),
    ];
    for (props, field, received) in cases {
        let v = violation(prime.add_node("person", props).await.unwrap_err());
        assert_eq!(v.type_name, "person");
        assert_eq!(v.field, field);
        assert_eq!(v.received, received);
        assert!(!v.expected.is_empty());
    }

    let v = violation(
        prime
            .add_edge(&p1, &p2, "works_at", None)
            .await
            .unwrap_err(),
    );
    assert_eq!(
        (v.field.as_str(), v.received.clone()),
        ("target", json!("person"))
    );

    prime.add_edge(&p1, &c1, "works_at", None).await.unwrap();
    let v = violation(
        prime
            .add_edge(&p1, &c2, "works_at", None)
            .await
            .unwrap_err(),
    );
    assert_eq!(v.field, "target");
    assert_eq!(v.received, json!(c2));
    assert!(v.expected.contains("cardinality one"), "{}", v.expected);

    assert_eq!(event_count(&prime).await, before + 1, "only the valid edge");
}

#[tokio::test]
async fn open_accepts_the_writes_strict_refuses() {
    let prime = typed_store(EnforcementMode::Open).await;
    let p1 = node_entity_id(
        "person",
        prime
            .add_node(
                "person",
                json!({"name": "Ann", "age": "old", "status": "vip"}),
            )
            .await
            .unwrap()
            .as_str(),
    );
    let p2 = node_entity_id(
        "person",
        prime
            .add_node("person", json!({"name": "B", "age": 200, "born": "x"}))
            .await
            .unwrap()
            .as_str(),
    );
    prime.add_edge(&p1, &p2, "works_at", None).await.unwrap();
    let c = node_entity_id(
        "company",
        prime.add_node("company", json!({})).await.unwrap().as_str(),
    );
    prime.add_edge(&p1, &c, "works_at", None).await.unwrap();
    prime
        .add_node("unregistered", json!({}))
        .await
        .expect("open accepts unknown types");
    assert_eq!(prime.enforcement(), EnforcementMode::Open);
}

#[tokio::test]
async fn strict_refuses_an_unregistered_type() {
    let prime = typed_store(EnforcementMode::Strict).await;
    let v = violation(prime.add_node("gadget", json!({})).await.unwrap_err());
    assert_eq!(v.field, "node_type");
    assert_eq!(v.received, json!("gadget"));
}

#[tokio::test]
async fn warn_accepts_and_records_the_violation_on_the_same_event() {
    let prime = typed_store(EnforcementMode::Warn).await;
    let id = prime
        .add_node("person", json!({"name": "Ann", "age": -3}))
        .await
        .unwrap();
    let eid = node_entity_id("person", id.as_str());
    let events = prime
        .core()
        .query(Query::new().entity_id(&eid))
        .await
        .unwrap();
    let warnings = &events[0].metadata.as_ref().unwrap()[WARNINGS_KEY];
    assert_eq!(warnings[0]["field"], "age");
    assert_eq!(warnings[0]["received"], -3);
    assert_eq!(events[0].metadata.as_ref().unwrap()["kind_version"], 1);
}

#[tokio::test]
async fn property_requiring_an_assertion_refuses_a_write_without_it() {
    let prime = Prime::open_in_memory().await.unwrap();
    prime
        .register_kind(
            SchemaKind::Node,
            "fact",
            json!({"properties": {"claim": {"kind": "string", "requires_assertion": ["source", "confidence"]}}}),
        )
        .await
        .unwrap();
    prime
        .set_enforcement(EnforcementMode::Strict)
        .await
        .unwrap();

    let v = violation(
        prime
            .add_node("fact", json!({"claim": "x"}))
            .await
            .unwrap_err(),
    );
    assert_eq!(v.field, "claim");
    assert!(v.expected.contains("source"), "{}", v.expected);
    prime
        .add_node_asserted(
            "fact",
            json!({"claim": "x"}),
            Some(Assertion::user().with_confidence(0.9)),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn legacy_required_schema_validates_unchanged_in_every_mode() {
    for mode in [
        EnforcementMode::Open,
        EnforcementMode::Warn,
        EnforcementMode::Strict,
    ] {
        let prime = Prime::open_in_memory().await.unwrap();
        prime
            .register_schema("person", SchemaKind::Node, json!({"required": ["name"]}))
            .await
            .unwrap();
        prime.set_enforcement(mode).await.unwrap();
        let err = prime.add_node("person", json!({})).await.unwrap_err();
        assert_eq!(
            err.to_string(),
            "validation failed: validation failed for person: missing required fields: name"
        );
        prime
            .add_node("person", json!({"name": "A", "anything": [1, 2]}))
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn compatible_reregistration_is_accepted_breaking_is_refused_and_replay_never_revalidates() {
    let dir = tempfile::tempdir().unwrap();
    let eid;
    {
        let prime = Prime::open(dir.path()).await.unwrap();
        let v1 = prime
            .register_kind(
                SchemaKind::Node,
                "person",
                json!({"properties": {"age": {"kind": "integer", "max": 150}}}),
            )
            .await
            .unwrap();
        assert_eq!(v1.version, 1);
        let id = prime
            .add_node("person", json!({"age": "not a number"}))
            .await
            .unwrap();
        eid = node_entity_id("person", id.as_str());

        let v2 = prime
            .register_kind(
                SchemaKind::Node,
                "person",
                json!({"properties": {
                    "age": {"kind": "integer", "max": 200},
                    "nick": {"kind": "string"}
                }}),
            )
            .await
            .unwrap();
        assert_eq!(v2.version, 2);

        let err = prime
            .register_kind(
                SchemaKind::Node,
                "person",
                json!({"properties": {"age": {"kind": "integer", "max": 100}}}),
            )
            .await
            .unwrap_err();
        assert!(err.to_string().contains("max lowered"), "{err}");

        prime.set_enforcement(EnforcementMode::Warn).await.unwrap();
        prime
            .set_enforcement(EnforcementMode::Strict)
            .await
            .unwrap();
        prime.shutdown().await.unwrap();
    }

    let prime = Prime::open(dir.path()).await.unwrap();
    assert!(
        prime.get_node(&eid).is_some(),
        "a node written under open replays under strict"
    );
    assert_eq!(prime.enforcement(), EnforcementMode::Strict);
    let kinds = prime.schemas();
    assert_eq!(kinds.len(), 1);
    assert_eq!(kinds[0].version, 2);
}

#[tokio::test]
async fn cardinality_one_edge_kind_drives_the_contradiction_projection() {
    let prime = Prime::open_in_memory().await.unwrap();
    let a = node_entity_id(
        "person",
        prime.add_node("person", json!({})).await.unwrap().as_str(),
    );
    let b = node_entity_id(
        "company",
        prime.add_node("company", json!({})).await.unwrap().as_str(),
    );
    let c = node_entity_id(
        "company",
        prime.add_node("company", json!({})).await.unwrap().as_str(),
    );
    prime
        .register_kind(SchemaKind::Edge, "works_at", json!({"cardinality": "one"}))
        .await
        .unwrap();
    prime.add_edge(&a, &b, "works_at", None).await.unwrap();
    prime.add_edge(&a, &c, "works_at", None).await.unwrap();
    assert_eq!(
        prime.contradictions().len(),
        1,
        "open accepts, contradiction flags"
    );
}

#[tokio::test]
async fn conversation_scoped_writes_carry_the_assertion_and_the_conversation() {
    let prime = Prime::open_in_memory().await.unwrap();
    let id = prime
        .with_conversation("conv-1")
        .add_node_asserted("note", json!({"t": 1}), Some(Assertion::user()))
        .await
        .unwrap();
    let eid = node_entity_id("note", id.as_str());
    let events = prime
        .core()
        .query(Query::new().entity_id(&eid))
        .await
        .unwrap();
    let meta = events[0].metadata.as_ref().unwrap();
    assert_eq!(meta["conversation_id"], "conv-1");
    assert_eq!(meta["assertion"]["source"], "user");
}
