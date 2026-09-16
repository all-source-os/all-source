//! Entity-prefix read scoping (#265).
//!
//! The incident: auth events carry session tokens whose value IS the
//! `entity_id`, so a read key that could see the tenant could see live bearer
//! credentials, and a routine query rendered several into an agent's context.
//! These tests pin the property that closes it — and in particular that the
//! obvious bypasses do not work.

use allsource_core::{
    QueryEventsRequest,
    domain::entities::Event,
    store::{EventStore, ReadScope},
};

const SENSITIVE: &str = "auth-session:session_a1b2c3";
const ORDINARY: &str = "workflow-run:run-1";

fn store_with_both() -> EventStore {
    let store = EventStore::new();
    for (entity, event_type) in [
        (SENSITIVE, "auth.session.created"),
        (ORDINARY, "workflow.started"),
    ] {
        let event = Event::from_strings(
            event_type.to_string(),
            entity.to_string(),
            "tenant1".to_string(),
            serde_json::json!({ "token": "live-bearer-value" }),
            None,
        )
        .expect("event");
        store.ingest(&event).expect("ingest");
    }
    store
}

fn query(entity_id: Option<&str>) -> QueryEventsRequest {
    QueryEventsRequest {
        entity_id: entity_id.map(str::to_string),
        tenant_id: Some("tenant1".to_string()),
        ..Default::default()
    }
}

#[test]
fn an_unscoped_read_still_sees_everything() {
    let store = store_with_both();
    let seen = store.query(&query(None)).expect("query");
    assert_eq!(seen.len(), 2, "scoping must be opt-in, not a silent change");
}

#[test]
fn a_scoped_read_cannot_see_a_stream_outside_its_prefixes() {
    let store = store_with_both();
    let scope = ReadScope::allow_entity_prefixes(["workflow-run:"]);

    let seen = store.query_scoped(&query(None), &scope).expect("query");

    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].entity_id().as_str(), ORDINARY);
}

/// The bypass that matters: asking for the forbidden entity by name. A scope
/// applied only to broad scans would hand the whole event over here.
#[test]
fn naming_the_forbidden_entity_directly_does_not_bypass_the_scope() {
    let store = store_with_both();
    let scope = ReadScope::allow_entity_prefixes(["workflow-run:"]);

    let seen = store
        .query_scoped(&query(Some(SENSITIVE)), &scope)
        .expect("query");

    assert!(
        seen.is_empty(),
        "an exact entity_id must not reach past the scope: {seen:?}"
    );
}

/// `total` is the pre-window match count, so counting before the scope would
/// tell a caller how many events it is not allowed to see.
#[test]
fn the_total_does_not_leak_the_existence_of_hidden_events() {
    let store = store_with_both();
    let scope = ReadScope::allow_entity_prefixes(["workflow-run:"]);

    let (_, total) = store
        .query_window_scoped(&query(None), 0, false, &scope)
        .expect("query");

    assert_eq!(total, 1, "total must count only what the scope permits");
}

#[test]
fn several_prefixes_are_all_honoured() {
    let store = store_with_both();
    let scope = ReadScope::allow_entity_prefixes(["workflow-run:", "auth-session:"]);

    let seen = store.query_scoped(&query(None), &scope).expect("query");

    assert_eq!(seen.len(), 2);
}

/// An empty allow-list is "scoped to nothing", not "unscoped". Reading it the
/// other way would turn a misconfiguration into full access.
#[test]
fn an_empty_allow_list_denies_everything() {
    let store = store_with_both();
    let scope = ReadScope::allow_entity_prefixes(Vec::<String>::new());

    let seen = store.query_scoped(&query(None), &scope).expect("query");

    assert!(
        seen.is_empty(),
        "an empty allow-list must not mean 'allow all'"
    );
    assert!(!scope.is_unrestricted());
}

#[test]
fn a_prefix_matches_only_at_the_start() {
    let scope = ReadScope::allow_entity_prefixes(["workflow-run:"]);
    assert!(scope.permits("workflow-run:run-1"));
    assert!(
        !scope.permits("shadow-workflow-run:run-1"),
        "a prefix rule must not match mid-string"
    );
}
