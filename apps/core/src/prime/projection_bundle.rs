//! `GraphProjections` — a Prime graph-projection set built by folding a list of
//! events, with no backing event store.
//!
//! The embedded Prime path registers its projections against a local
//! `store::EventStore` that backfills + pushes events to them
//! ([`Prime::from_core`](super::facade::Prime)). The hosted, stateless path
//! ([`HttpCore`](super::http_core::HttpCore)) has no such store — it pulls a
//! tenant's `prime.*` events from a remote Core and must materialize the same
//! projections in memory. This module is that fold: create fresh projections,
//! reconstruct each [`EventView`] into a domain [`Event`], and `process` it
//! through every projection.
//!
//! This is the per-tenant building block the warm cache (slice 3) hands out.
//! Vector projections are intentionally excluded here — they require the
//! embedder and are layered on separately.
//!
//! Feature-gated behind `prime-recall` (it pairs with the HTTP-backed path).

use std::sync::Arc;

use crate::{
    application::services::projection::Projection,
    domain::entities::Event,
    embedded::EventView,
    prime::{
        projections::{
            AdjacencyListProjection, ContradictionDetectionProjection, CrossDomainProjection,
            DomainIndexProjection, GraphStatsProjection, NodeStateProjection,
            NodeTypeIndexProjection, ReverseIndexProjection,
        },
        schema::SchemaProjection,
    },
};

#[cfg(feature = "prime-vectors")]
use crate::prime::vectors::VectorIndexProjection;

// Projection identities. They mirror the embedded path's names for parity, but
// since these projections are never registered with a store the names are only
// cosmetic (used in logs / snapshots).
const PROJ_NODE_STATE: &str = "prime.node_state";
const PROJ_NODE_TYPE_INDEX: &str = "prime.node_type_index";
const PROJ_ADJACENCY: &str = "prime.adjacency";
const PROJ_REVERSE_INDEX: &str = "prime.reverse_index";
const PROJ_GRAPH_STATS: &str = "prime.graph_stats";
const PROJ_SCHEMA: &str = "prime.schema";
const PROJ_CONTRADICTION: &str = "prime.contradiction";
#[cfg(feature = "prime-vectors")]
const PROJ_VECTOR_INDEX: &str = "prime.vector_index";

/// A Prime graph-projection set materialized from an event list.
///
/// Holds the same graph projections [`Prime`](super::facade::Prime) registers,
/// each an `Arc` so it can be shared with the recall engine. Fed by
/// [`GraphProjections::from_event_views`] (initial backfill) and kept current
/// by [`GraphProjections::apply`] (per-write updates).
pub struct GraphProjections {
    pub node_state: Arc<NodeStateProjection>,
    pub node_type_index: Arc<NodeTypeIndexProjection>,
    pub adjacency: Arc<AdjacencyListProjection>,
    pub reverse_index: Arc<ReverseIndexProjection>,
    pub graph_stats: Arc<GraphStatsProjection>,
    pub schema: Arc<SchemaProjection>,
    pub contradiction: Arc<ContradictionDetectionProjection>,
    pub cross_domain: Arc<CrossDomainProjection>,
    pub domain_index: Arc<DomainIndexProjection>,
    /// HNSW vector index over the tenant's stored embeddings. Folds
    /// `prime.vector.stored` / `prime.vector.deleted` events so the warm bundle
    /// can serve semantic recall without re-embedding. Mirrors the embedded
    /// `Prime::vector_index` (registered via `register_vector_projection`).
    #[cfg(feature = "prime-vectors")]
    pub vector_index: Arc<VectorIndexProjection>,
}

impl GraphProjections {
    /// Create an empty projection set (no events applied yet).
    pub fn empty() -> Self {
        Self {
            node_state: Arc::new(NodeStateProjection::new(PROJ_NODE_STATE)),
            node_type_index: Arc::new(NodeTypeIndexProjection::new(PROJ_NODE_TYPE_INDEX)),
            adjacency: Arc::new(AdjacencyListProjection::new_forward(PROJ_ADJACENCY)),
            reverse_index: Arc::new(ReverseIndexProjection::new_reverse(PROJ_REVERSE_INDEX)),
            graph_stats: Arc::new(GraphStatsProjection::new(PROJ_GRAPH_STATS)),
            schema: Arc::new(SchemaProjection::new(PROJ_SCHEMA)),
            contradiction: Arc::new(ContradictionDetectionProjection::new(PROJ_CONTRADICTION)),
            cross_domain: Arc::new(CrossDomainProjection::new()),
            domain_index: Arc::new(DomainIndexProjection::new()),
            #[cfg(feature = "prime-vectors")]
            vector_index: Arc::new(VectorIndexProjection::new(PROJ_VECTOR_INDEX)),
        }
    }

    /// Build a projection set by folding `events` (e.g. a tenant's `prime.*`
    /// events queried from a remote Core) in order.
    pub fn from_event_views(events: &[EventView]) -> Self {
        let bundle = Self::empty();
        for view in events {
            bundle.apply(view);
        }
        bundle
    }

    /// Apply a single event view to every projection. Each projection ignores
    /// event types it doesn't care about, so it is safe to fan out to all.
    /// A projection error is logged and skipped — one bad event must not poison
    /// the whole graph (matches the embedded path's per-projection isolation).
    pub fn apply(&self, view: &EventView) {
        let event = view_to_event(view);
        self.process_all(&event);
    }

    fn process_all(&self, event: &Event) {
        let projections: [&dyn Projection; 9] = [
            self.node_state.as_ref(),
            self.node_type_index.as_ref(),
            self.adjacency.as_ref(),
            self.reverse_index.as_ref(),
            self.graph_stats.as_ref(),
            self.schema.as_ref(),
            self.contradiction.as_ref(),
            self.cross_domain.as_ref(),
            self.domain_index.as_ref(),
        ];
        for proj in projections {
            Self::process_one(proj, event);
        }
        // The vector index is folded separately: it's feature-gated, so adding it
        // to the fixed-size array above would change the array length per-feature.
        #[cfg(feature = "prime-vectors")]
        Self::process_one(self.vector_index.as_ref(), event);
    }

    fn process_one(proj: &dyn Projection, event: &Event) {
        if let Err(e) = proj.process(event) {
            tracing::warn!(
                projection = proj.name(),
                event_type = event.event_type_str(),
                error = %e,
                "prime projection failed to process event; skipping"
            );
        }
    }
}

/// Reconstruct a domain [`Event`] from an [`EventView`] DTO so it can be fed to
/// the projections' `process` method.
fn view_to_event(view: &EventView) -> Event {
    Event::reconstruct_from_strings(
        view.id,
        view.event_type.clone(),
        view.entity_id.clone(),
        view.tenant_id.clone(),
        view.payload.clone(),
        view.timestamp,
        view.metadata.clone(),
        view.version,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prime::types::event_types;
    use chrono::Utc;
    use serde_json::json;
    use uuid::Uuid;

    fn node_created(entity_id: &str, node_type: &str, name: &str) -> EventView {
        EventView {
            id: Uuid::new_v4(),
            event_type: event_types::NODE_CREATED.to_string(),
            entity_id: entity_id.to_string(),
            tenant_id: "tenant-a".to_string(),
            payload: json!({
                "id": entity_id,
                "node_type": node_type,
                "properties": { "name": name },
            }),
            metadata: None,
            timestamp: Utc::now(),
            version: 1,
            valid_from: None,
            valid_to: None,
        }
    }

    #[test]
    fn folds_node_created_events_into_a_queryable_graph() {
        let events = vec![
            node_created("node:contact:alice", "contact", "Alice"),
            node_created("node:contact:bob", "contact", "Bob"),
        ];
        let graph = GraphProjections::from_event_views(&events);

        let alice = graph
            .node_state
            .get_node("node:contact:alice")
            .expect("alice should be in the materialized graph");
        assert_eq!(alice.properties["name"], "Alice");
        assert!(graph.node_state.get_node("node:contact:bob").is_some());
        assert!(graph.node_state.get_node("node:contact:ghost").is_none());
    }

    #[test]
    fn empty_bundle_has_no_nodes() {
        let graph = GraphProjections::empty();
        assert!(graph.node_state.get_node("node:contact:alice").is_none());
    }

    #[test]
    fn apply_updates_an_existing_bundle() {
        let graph = GraphProjections::empty();
        graph.apply(&node_created("node:contact:carol", "contact", "Carol"));
        assert_eq!(
            graph
                .node_state
                .get_node("node:contact:carol")
                .unwrap()
                .properties["name"],
            "Carol"
        );
    }
}
