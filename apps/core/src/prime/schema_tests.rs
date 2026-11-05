use super::*;

#[allow(clippy::needless_pass_by_value)]
fn kind(v: Value) -> KindDef {
    KindDef::parse(&v).unwrap()
}

#[test]
fn widening_a_kind_is_not_breaking() {
    let old = kind(json!({
        "required": ["name"],
        "properties": {
            "age": {"kind": "integer", "min": 0, "max": 100},
            "status": {"kind": "enum", "values": ["a"]},
            "boss": {"kind": "node_ref", "node_types": ["person"]}
        },
        "to_types": ["company"],
        "cardinality": "one"
    }));
    let new = kind(json!({
        "required": ["name"],
        "properties": {
            "age": {"kind": "integer", "min": -1, "max": 200},
            "status": {"kind": "enum", "values": ["a", "b"]},
            "boss": {"kind": "node_ref"},
            "nick": {"kind": "string"}
        },
        "cardinality": "many"
    }));
    assert!(old.breaking_changes(&new).is_empty());
}

#[test]
fn each_restriction_is_reported_as_breaking() {
    let old = kind(json!({
        "properties": {
            "age": {"kind": "integer"},
            "status": {"kind": "enum", "values": ["a", "b"]},
            "name": {"kind": "string"}
        }
    }));
    let new = kind(json!({
        "required": ["name"],
        "properties": {
            "age": {"kind": "number", "max": 10},
            "status": {"kind": "enum", "values": ["a"]},
            "name": {"kind": "string", "max_length": 5, "required": true,
                     "requires_assertion": ["confidence"]},
            "extra": {"kind": "string", "required": true}
        },
        "from_types": ["person"],
        "cardinality": "one"
    }));
    let changes = old.breaking_changes(&new).join("\n");
    for expected in [
        "required field name added",
        "property age kind changed",
        "property age max lowered",
        "property status enum values removed",
        "property name max_length lowered",
        "property name became required",
        "property name requires_assertion extended",
        "required property extra added",
        "from_types narrowed",
        "cardinality changed from many to one",
    ] {
        assert!(
            changes.contains(expected),
            "missing {expected:?} in:\n{changes}"
        );
    }
}

#[test]
fn malformed_kind_definitions_are_refused_at_registration() {
    let proj = SchemaProjection::new("schema");
    for bad in [
        json!({"properties": {"s": {"kind": "enum"}}}),
        json!({"properties": {"n": {"kind": "number", "min": 5, "max": 1}}}),
        json!({"properties": {"n": {"kind": "colour"}}}),
        json!({"properties": {"n": {"kind": "string", "requires_assertion": ["mood"]}}}),
    ] {
        assert!(
            proj.plan_registration(DEFAULT_TENANT, SchemaKind::Node, "x", &bad)
                .is_err(),
            "{bad}"
        );
    }
}

#[test]
fn a_legacy_json_schema_style_registration_is_still_accepted_and_stays_untyped() {
    let proj = SchemaProjection::new("schema");
    let legacy = json!({
        "required": ["name"],
        "properties": {"name": {"type": "string"}, "age": {"kind": "integer"}}
    });
    assert_eq!(
        proj.plan_registration(DEFAULT_TENANT, SchemaKind::Node, "person", &legacy)
            .unwrap(),
        1
    );
    let def = kind(legacy);
    assert_eq!(def.required, vec!["name".to_string()]);
    assert!(!def.properties.contains_key("name"));
    assert_eq!(def.properties["age"].kind, PropertyKind::Integer);
}

#[test]
fn enforcement_replay_keeps_the_latest_event_per_tenant() {
    let proj = SchemaProjection::new("schema");
    let at = |secs: i64| chrono::DateTime::from_timestamp(secs, 0).unwrap();
    let set = |tenant: &str, mode: &str, ts| {
        Event::reconstruct_from_strings(
            uuid::Uuid::new_v4(),
            ENFORCEMENT_SET.to_string(),
            "prime:enforcement".to_string(),
            tenant.to_string(),
            json!({"mode": mode}),
            ts,
            None,
            1,
        )
    };
    proj.process(&set("a", "strict", at(200))).unwrap();
    proj.process(&set("a", "warn", at(100))).unwrap();
    proj.process(&set("b", "warn", at(50))).unwrap();
    assert_eq!(proj.enforcement("a"), EnforcementMode::Strict);
    assert_eq!(proj.enforcement("b"), EnforcementMode::Warn);
    assert_eq!(proj.enforcement("c"), EnforcementMode::Open);

    let snap = proj.snapshot().unwrap();
    proj.clear();
    proj.restore(&snap).unwrap();
    assert_eq!(proj.enforcement("a"), EnforcementMode::Strict);
}

#[test]
fn legacy_array_snapshot_still_restores() {
    let proj = SchemaProjection::new("schema");
    proj.restore(&json!([{
        "type_name": "person", "kind": "Node", "schema": {"required": ["name"]}
    }]))
    .unwrap();
    assert!(proj.validate_node("person", &json!({})).is_err());
    assert_eq!(proj.schemas()[0].version, 1);
}
