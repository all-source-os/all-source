//! GraphStats projection — tracks graph statistics incrementally.
//!
//! Maintains counters for nodes, edges, types, and relations so that
//! `prime.stats()` is O(1) without scanning the entire graph.

use dashmap::{DashMap, mapref::entry::Entry};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use super::charge;
use crate::{
    application::services::projection::Projection,
    domain::entities::Event,
    error::Result,
    infrastructure::memory::ledger::Tally,
    prime::types::{PrimeStats, event_types},
};

/// Serializable snapshot of graph statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StatsSnapshot {
    total_nodes: usize,
    total_edges: usize,
    deleted_nodes: usize,
    deleted_edges: usize,
    event_count: usize,
    nodes_by_type: Vec<(String, usize)>,
    edges_by_relation: Vec<(String, usize)>,
}

/// Projection that tracks graph statistics incrementally.
///
/// All counters are updated on each `prime.*` event, so `stats()` is O(1).
pub struct GraphStatsProjection {
    name: String,
    total_nodes: AtomicUsize,
    total_edges: AtomicUsize,
    deleted_nodes: AtomicUsize,
    deleted_edges: AtomicUsize,
    event_count: AtomicUsize,
    /// node_type -> count of live nodes
    nodes_by_type: Arc<DashMap<String, usize>>,
    /// relation -> count of live edges
    edges_by_relation: Arc<DashMap<String, usize>>,
    charge: Tally,
}

impl GraphStatsProjection {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            total_nodes: AtomicUsize::new(0),
            total_edges: AtomicUsize::new(0),
            deleted_nodes: AtomicUsize::new(0),
            deleted_edges: AtomicUsize::new(0),
            event_count: AtomicUsize::new(0),
            nodes_by_type: Arc::new(DashMap::new()),
            edges_by_relation: Arc::new(DashMap::new()),
            charge: charge::tally(),
        }
    }

    /// Counts are never removed, only decremented, so a key is charged once.
    fn increment(&self, counts: &DashMap<String, usize>, key: &str) {
        match counts.entry(key.to_string()) {
            Entry::Occupied(mut count) => *count.get_mut() += 1,
            Entry::Vacant(vacant) => {
                self.charge
                    .add(charge::entry::<String, usize>(charge::text(key)));
                vacant.insert(1);
            }
        }
    }

    fn store(&self, counts: &DashMap<String, usize>, key: String, value: usize) {
        let key_bytes = charge::text(&key);
        if counts.insert(key, value).is_none() {
            self.charge.add(charge::entry::<String, usize>(key_bytes));
        }
    }

    /// Returns current graph statistics.
    pub fn stats(&self) -> PrimeStats {
        PrimeStats {
            total_nodes: self.total_nodes.load(Ordering::Relaxed),
            total_edges: self.total_edges.load(Ordering::Relaxed),
            deleted_nodes: self.deleted_nodes.load(Ordering::Relaxed),
            deleted_edges: self.deleted_edges.load(Ordering::Relaxed),
            event_count: self.event_count.load(Ordering::Relaxed),
            nodes_by_type: self
                .nodes_by_type
                .iter()
                .map(|entry| (entry.key().clone(), *entry.value()))
                .collect(),
            edges_by_relation: self
                .edges_by_relation
                .iter()
                .map(|entry| (entry.key().clone(), *entry.value()))
                .collect(),
        }
    }
}

impl Projection for GraphStatsProjection {
    fn name(&self) -> &str {
        &self.name
    }

    fn process(&self, event: &Event) -> Result<()> {
        let event_type = event.event_type_str();

        // Only process prime.* events
        if !event_type.starts_with("prime.") {
            return Ok(());
        }

        self.event_count.fetch_add(1, Ordering::Relaxed);

        match event_type {
            event_types::NODE_CREATED => {
                self.total_nodes.fetch_add(1, Ordering::Relaxed);

                if let Some(node_type) = event.payload.get("node_type").and_then(|v| v.as_str()) {
                    self.increment(&self.nodes_by_type, node_type);
                }
            }
            event_types::NODE_DELETED => {
                self.deleted_nodes.fetch_add(1, Ordering::Relaxed);

                if let Some(node_type) = event.payload.get("node_type").and_then(|v| v.as_str())
                    && let Some(mut count) = self.nodes_by_type.get_mut(node_type)
                {
                    *count = count.saturating_sub(1);
                }
            }
            event_types::EDGE_CREATED => {
                self.total_edges.fetch_add(1, Ordering::Relaxed);

                if let Some(relation) = event.payload.get("relation").and_then(|v| v.as_str()) {
                    self.increment(&self.edges_by_relation, relation);
                }
            }
            event_types::EDGE_DELETED => {
                self.deleted_edges.fetch_add(1, Ordering::Relaxed);

                if let Some(relation) = event.payload.get("relation").and_then(|v| v.as_str())
                    && let Some(mut count) = self.edges_by_relation.get_mut(relation)
                {
                    *count = count.saturating_sub(1);
                }
            }
            _ => {} // NODE_UPDATED and future events don't affect stats
        }

        Ok(())
    }

    fn get_state(&self, _entity_id: &str) -> Option<Value> {
        // Stats projection doesn't track per-entity state.
        // Use stats() instead.
        serde_json::to_value(self.stats()).ok()
    }

    fn clear(&self) {
        self.total_nodes.store(0, Ordering::Relaxed);
        self.total_edges.store(0, Ordering::Relaxed);
        self.deleted_nodes.store(0, Ordering::Relaxed);
        self.deleted_edges.store(0, Ordering::Relaxed);
        self.event_count.store(0, Ordering::Relaxed);
        self.nodes_by_type.clear();
        self.edges_by_relation.clear();
        self.charge.take();
    }

    fn snapshot(&self) -> Option<Value> {
        let snap = StatsSnapshot {
            total_nodes: self.total_nodes.load(Ordering::Relaxed),
            total_edges: self.total_edges.load(Ordering::Relaxed),
            deleted_nodes: self.deleted_nodes.load(Ordering::Relaxed),
            deleted_edges: self.deleted_edges.load(Ordering::Relaxed),
            event_count: self.event_count.load(Ordering::Relaxed),
            nodes_by_type: self
                .nodes_by_type
                .iter()
                .map(|e| (e.key().clone(), *e.value()))
                .collect(),
            edges_by_relation: self
                .edges_by_relation
                .iter()
                .map(|e| (e.key().clone(), *e.value()))
                .collect(),
        };
        serde_json::to_value(snap).ok()
    }

    fn restore(&self, snapshot: &Value) -> Result<()> {
        let snap: StatsSnapshot = serde_json::from_value(snapshot.clone())
            .map_err(|e| crate::error::AllSourceError::StorageError(e.to_string()))?;

        self.total_nodes.store(snap.total_nodes, Ordering::Relaxed);
        self.total_edges.store(snap.total_edges, Ordering::Relaxed);
        self.deleted_nodes
            .store(snap.deleted_nodes, Ordering::Relaxed);
        self.deleted_edges
            .store(snap.deleted_edges, Ordering::Relaxed);
        self.event_count.store(snap.event_count, Ordering::Relaxed);

        self.nodes_by_type.clear();
        self.edges_by_relation.clear();
        self.charge.take();
        for (k, v) in snap.nodes_by_type {
            self.store(&self.nodes_by_type, k, v);
        }
        for (k, v) in snap.edges_by_relation {
            self.store(&self.edges_by_relation, k, v);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn make_event(entity_id: &str, event_type: &str, payload: Value) -> Event {
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

    #[test]
    fn test_node_created_increments_counts() {
        let proj = GraphStatsProjection::new("graph_stats");

        proj.process(&make_event(
            "node:person:alice",
            event_types::NODE_CREATED,
            serde_json::json!({"node_type": "person", "properties": {"name": "Alice"}}),
        ))
        .unwrap();

        let stats = proj.stats();
        assert_eq!(stats.total_nodes, 1);
        assert_eq!(stats.nodes_by_type.get("person"), Some(&1));
        assert_eq!(stats.event_count, 1);
    }

    #[test]
    fn test_multiple_node_types() {
        let proj = GraphStatsProjection::new("graph_stats");

        // 3 person nodes
        for i in 0..3 {
            proj.process(&make_event(
                &format!("node:person:p{i}"),
                event_types::NODE_CREATED,
                serde_json::json!({"node_type": "person", "properties": {}}),
            ))
            .unwrap();
        }
        // 2 project nodes
        for i in 0..2 {
            proj.process(&make_event(
                &format!("node:project:j{i}"),
                event_types::NODE_CREATED,
                serde_json::json!({"node_type": "project", "properties": {}}),
            ))
            .unwrap();
        }

        let stats = proj.stats();
        assert_eq!(stats.total_nodes, 5);
        assert_eq!(stats.nodes_by_type.get("person"), Some(&3));
        assert_eq!(stats.nodes_by_type.get("project"), Some(&2));
    }

    #[test]
    fn test_edge_created_increments_counts() {
        let proj = GraphStatsProjection::new("graph_stats");

        proj.process(&make_event(
            "edge:e-1",
            event_types::EDGE_CREATED,
            serde_json::json!({"source": "a", "target": "b", "relation": "works_on"}),
        ))
        .unwrap();

        proj.process(&make_event(
            "edge:e-2",
            event_types::EDGE_CREATED,
            serde_json::json!({"source": "a", "target": "c", "relation": "knows"}),
        ))
        .unwrap();

        let stats = proj.stats();
        assert_eq!(stats.total_edges, 2);
        assert_eq!(stats.edges_by_relation.get("works_on"), Some(&1));
        assert_eq!(stats.edges_by_relation.get("knows"), Some(&1));
    }

    #[test]
    fn test_node_deleted_increments_deleted_and_decrements_type() {
        let proj = GraphStatsProjection::new("graph_stats");

        proj.process(&make_event(
            "node:person:alice",
            event_types::NODE_CREATED,
            serde_json::json!({"node_type": "person", "properties": {}}),
        ))
        .unwrap();

        proj.process(&make_event(
            "node:person:alice",
            event_types::NODE_DELETED,
            serde_json::json!({"node_type": "person"}),
        ))
        .unwrap();

        let stats = proj.stats();
        assert_eq!(stats.total_nodes, 1); // total ever created
        assert_eq!(stats.deleted_nodes, 1);
        assert_eq!(stats.nodes_by_type.get("person"), Some(&0)); // live count
    }

    #[test]
    fn test_edge_deleted_increments_deleted_and_decrements_relation() {
        let proj = GraphStatsProjection::new("graph_stats");

        proj.process(&make_event(
            "edge:e-1",
            event_types::EDGE_CREATED,
            serde_json::json!({"relation": "works_on"}),
        ))
        .unwrap();

        proj.process(&make_event(
            "edge:e-1",
            event_types::EDGE_DELETED,
            serde_json::json!({"relation": "works_on"}),
        ))
        .unwrap();

        let stats = proj.stats();
        assert_eq!(stats.total_edges, 1);
        assert_eq!(stats.deleted_edges, 1);
        assert_eq!(stats.edges_by_relation.get("works_on"), Some(&0));
    }

    #[test]
    fn test_ignores_non_prime_events() {
        let proj = GraphStatsProjection::new("graph_stats");

        proj.process(&make_event(
            "user-123",
            "user.created",
            serde_json::json!({"name": "Alice"}),
        ))
        .unwrap();

        let stats = proj.stats();
        assert_eq!(stats.total_nodes, 0);
        assert_eq!(stats.event_count, 0);
    }

    #[test]
    fn test_node_updated_only_increments_event_count() {
        let proj = GraphStatsProjection::new("graph_stats");

        proj.process(&make_event(
            "node:person:alice",
            event_types::NODE_CREATED,
            serde_json::json!({"node_type": "person", "properties": {}}),
        ))
        .unwrap();

        proj.process(&make_event(
            "node:person:alice",
            event_types::NODE_UPDATED,
            serde_json::json!({"properties": {"role": "manager"}}),
        ))
        .unwrap();

        let stats = proj.stats();
        assert_eq!(stats.total_nodes, 1); // no double-count
        assert_eq!(stats.event_count, 2);
    }

    #[test]
    fn test_full_scenario() {
        let proj = GraphStatsProjection::new("graph_stats");

        // Add 5 nodes: 3 person, 2 project
        for i in 0..3 {
            proj.process(&make_event(
                &format!("node:person:p{i}"),
                event_types::NODE_CREATED,
                serde_json::json!({"node_type": "person", "properties": {}}),
            ))
            .unwrap();
        }
        for i in 0..2 {
            proj.process(&make_event(
                &format!("node:project:j{i}"),
                event_types::NODE_CREATED,
                serde_json::json!({"node_type": "project", "properties": {}}),
            ))
            .unwrap();
        }

        // Add 4 edges: 2 works_on, 2 knows
        for i in 0..2 {
            proj.process(&make_event(
                &format!("edge:wo-{i}"),
                event_types::EDGE_CREATED,
                serde_json::json!({"relation": "works_on"}),
            ))
            .unwrap();
        }
        for i in 0..2 {
            proj.process(&make_event(
                &format!("edge:kn-{i}"),
                event_types::EDGE_CREATED,
                serde_json::json!({"relation": "knows"}),
            ))
            .unwrap();
        }

        // Delete 1 node
        proj.process(&make_event(
            "node:person:p0",
            event_types::NODE_DELETED,
            serde_json::json!({"node_type": "person"}),
        ))
        .unwrap();

        let stats = proj.stats();
        assert_eq!(stats.total_nodes, 5);
        assert_eq!(stats.total_edges, 4);
        assert_eq!(stats.deleted_nodes, 1);
        assert_eq!(stats.deleted_edges, 0);
        assert_eq!(stats.nodes_by_type.get("person"), Some(&2)); // 3 - 1 deleted
        assert_eq!(stats.nodes_by_type.get("project"), Some(&2));
        assert_eq!(stats.edges_by_relation.get("works_on"), Some(&2));
        assert_eq!(stats.edges_by_relation.get("knows"), Some(&2));
        assert_eq!(stats.event_count, 10); // 5 nodes + 4 edges + 1 delete
    }

    #[test]
    fn test_snapshot_and_restore() {
        let proj = GraphStatsProjection::new("graph_stats");

        proj.process(&make_event(
            "node:person:alice",
            event_types::NODE_CREATED,
            serde_json::json!({"node_type": "person", "properties": {}}),
        ))
        .unwrap();
        proj.process(&make_event(
            "edge:e-1",
            event_types::EDGE_CREATED,
            serde_json::json!({"relation": "knows"}),
        ))
        .unwrap();

        // Snapshot
        let snap = proj.snapshot().expect("snapshot should be Some");

        // Clear and verify empty
        proj.clear();
        assert_eq!(proj.stats().total_nodes, 0);

        // Restore and verify
        proj.restore(&snap).unwrap();
        let stats = proj.stats();
        assert_eq!(stats.total_nodes, 1);
        assert_eq!(stats.total_edges, 1);
        assert_eq!(stats.nodes_by_type.get("person"), Some(&1));
        assert_eq!(stats.edges_by_relation.get("knows"), Some(&1));
        assert_eq!(stats.event_count, 2);
    }

    #[test]
    fn test_get_state_returns_stats() {
        let proj = GraphStatsProjection::new("graph_stats");

        proj.process(&make_event(
            "node:person:alice",
            event_types::NODE_CREATED,
            serde_json::json!({"node_type": "person", "properties": {}}),
        ))
        .unwrap();

        let state = proj.get_state("anything").unwrap();
        assert_eq!(state["total_nodes"], 1);
    }

    #[test]
    fn test_delete_without_prior_create_saturates() {
        let proj = GraphStatsProjection::new("graph_stats");

        // Delete a node type that was never created — should not underflow
        proj.process(&make_event(
            "node:person:ghost",
            event_types::NODE_DELETED,
            serde_json::json!({"node_type": "person"}),
        ))
        .unwrap();

        let stats = proj.stats();
        assert_eq!(stats.deleted_nodes, 1);
        // nodes_by_type should not contain "person" (never created)
        assert!(!stats.nodes_by_type.contains_key("person"));
    }
}
