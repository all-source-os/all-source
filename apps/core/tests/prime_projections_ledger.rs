//! Prime projections charge `projection.permanent` for the state they hold and
//! release it when they are cleared. Synthetic events only.
#![cfg(feature = "prime")]

use allsource_core::{
    application::services::projection::Projection,
    domain::entities::Event,
    infrastructure::memory::ledger::{Owner, resident},
    prime::{
        projections::{
            AdjacencyListProjection, ContradictionDetectionProjection, CrossDomainProjection,
            DomainIndexProjection, GraphStatsProjection, NodeStateProjection,
            NodeTypeIndexProjection, RelevanceDecayProjection, ReverseIndexProjection,
        },
        schema::SchemaProjection,
    },
};
use chrono::Utc;
use serde_json::{Value, json};
use uuid::Uuid;

fn event(entity_id: &str, event_type: &str, payload: Value) -> Event {
    Event::reconstruct_from_strings(
        Uuid::new_v4(),
        event_type.to_string(),
        entity_id.to_string(),
        "default".to_string(),
        payload,
        Utc::now(),
        None,
        1,
    )
}

fn workload() -> Vec<Event> {
    let mut events = Vec::new();
    for (id, domain) in [
        ("alice", "engineering"),
        ("bob", "sales"),
        ("carol", "sales"),
    ] {
        events.push(event(
            &format!("node:person:{id}"),
            "prime.node.created",
            json!({
                "id": id,
                "node_type": "person",
                "properties": {"name": id, "role": "engineer", "domain": domain},
                "labels": ["active"],
            }),
        ));
    }
    for (edge, source, target) in [
        ("e1", "node:person:alice", "node:person:bob"),
        ("e2", "node:person:bob", "node:person:carol"),
        ("e3", "node:person:alice", "node:person:carol"),
    ] {
        events.push(event(
            &format!("edge:{edge}"),
            "prime.edge.created",
            json!({
                "id": edge,
                "source": source,
                "target": target,
                "relation": "reports_to",
                "weight": 1.0,
            }),
        ));
    }
    events.push(event(
        "schema:person",
        "prime.schema.registered",
        json!({
            "type_name": "person",
            "kind": "node",
            "schema": {"required": ["name"]},
        }),
    ));
    events
}

/// Feed `events` to `projection` and return how far the owner grew.
fn grow(projection: &dyn Projection, events: &[Event]) -> i64 {
    let before = resident().owner_bytes(Owner::ProjectionPermanent);
    for event in events {
        projection.process(event).expect("process");
    }
    resident().owner_bytes(Owner::ProjectionPermanent) - before
}

#[test]
fn prime_projections_charge_inserts_and_release_on_clear() {
    let events = workload();
    let node_state = NodeStateProjection::new("node_state");
    let node_type_index = NodeTypeIndexProjection::new("node_type_index");
    let adjacency = AdjacencyListProjection::new_forward("adjacency");
    let reverse_index = ReverseIndexProjection::new_reverse("reverse_index");
    let graph_stats = GraphStatsProjection::new("graph_stats");
    let domain_index = DomainIndexProjection::new();
    let cross_domain = CrossDomainProjection::new();
    let relevance = RelevanceDecayProjection::new("relevance");
    let contradiction = ContradictionDetectionProjection::new("contradiction");
    contradiction.configure_exclusive("reports_to");
    let schema = SchemaProjection::new("schema");

    let projections: Vec<(&str, &dyn Projection)> = vec![
        ("node_state", &node_state),
        ("node_type_index", &node_type_index),
        ("adjacency", &adjacency),
        ("reverse_index", &reverse_index),
        ("graph_stats", &graph_stats),
        ("domain_index", &domain_index),
        ("cross_domain", &cross_domain),
        ("relevance", &relevance),
        ("contradiction", &contradiction),
        ("schema", &schema),
    ];

    for (label, projection) in &projections {
        let grew = grow(*projection, &events);
        assert!(grew > 0, "{label} holds state but charged nothing");
    }

    let held = resident().owner_bytes(Owner::ProjectionPermanent);
    for (label, projection) in &projections {
        projection.clear();
        assert!(
            resident().owner_bytes(Owner::ProjectionPermanent) <= held,
            "{label} clear grew the owner"
        );
    }
    let cleared = resident().owner_bytes(Owner::ProjectionPermanent);
    assert!(
        cleared < held,
        "clearing every projection released nothing: held {held}, after clear {cleared}"
    );
}
