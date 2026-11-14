//! Directed adjacency projection — O(1) neighbor lookups and O(1) edge deletion.
//!
//! A single parameterized projection replaces the previous `AdjacencyListProjection`
//! and `ReverseIndexProjection` (ADR-012). Direction determines which payload field
//! is the key vs. the peer:
//!
//! - `Forward`: key = `source`, peer = `target` (outgoing edges)
//! - `Reverse`: key = `target`, peer = `source` (incoming edges)
//!
//! A secondary `edge_id → key` index enables O(1) edge deletion, replacing the
//! previous O(E) full-scan approach.

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

use super::charge;
use crate::{
    application::services::projection::Projection, domain::entities::Event, error::Result,
    infrastructure::memory::ledger::Tally, prime::types::event_types,
};

/// A single adjacency entry: (relation, peer_node, edge_id, weight).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AdjEntry {
    pub relation: String,
    pub peer: String,
    pub edge_id: String,
    pub weight: Option<f64>,
}

fn entry_bytes(entry: &AdjEntry) -> u64 {
    std::mem::size_of::<AdjEntry>() as u64
        + charge::text(&entry.relation)
        + charge::text(&entry.peer)
        + charge::text(&entry.edge_id)
}

/// Which end of an edge is the lookup key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdjacencyDirection {
    /// key = source, peer = target (outgoing edges)
    Forward,
    /// key = target, peer = source (incoming edges)
    Reverse,
}

/// Directed adjacency projection with O(1) neighbor lookup and O(1) edge deletion.
///
/// Maintains:
/// - `adj`: key → `Vec<AdjEntry>` for neighbor queries
/// - `edge_index`: edge_id → key for O(1) deletion without full scan
pub struct DirectedAdjacencyProjection {
    name: String,
    direction: AdjacencyDirection,
    /// key (source or target) → adjacency entries
    adj: Arc<DashMap<String, Vec<AdjEntry>>>,
    /// Secondary index: edge_id → key for O(1) deletion
    edge_index: Arc<DashMap<String, String>>,
    charge: Tally,
}

impl DirectedAdjacencyProjection {
    pub fn new(name: impl Into<String>, direction: AdjacencyDirection) -> Self {
        Self {
            name: name.into(),
            direction,
            adj: Arc::new(DashMap::new()),
            edge_index: Arc::new(DashMap::new()),
            charge: charge::tally(),
        }
    }

    fn index_edge(&self, edge_id: &str, key: &str) {
        match self.edge_index.insert(edge_id.to_string(), key.to_string()) {
            Some(old_key) => self
                .charge
                .adjust(charge::text(&old_key), charge::text(key)),
            None => self.charge.add(charge::entry::<String, String>(
                charge::text(edge_id) + charge::text(key),
            )),
        }
    }

    fn store_adj(&self, key: String, entries: Vec<AdjEntry>) {
        let key_bytes = charge::text(&key);
        let owned = key_bytes + entries.iter().map(entry_bytes).sum::<u64>();
        match self.adj.insert(key, entries) {
            Some(old) => {
                let old_owned = key_bytes + old.iter().map(entry_bytes).sum::<u64>();
                self.charge.adjust(old_owned, owned);
            }
            None => self
                .charge
                .add(charge::entry::<String, Vec<AdjEntry>>(owned)),
        }
    }

    fn push_entry(&self, key: String, entry: AdjEntry) {
        let owned = entry_bytes(&entry);
        let is_new = !self.adj.contains_key(&key);
        let key_bytes = charge::text(&key);
        self.adj.entry(key).or_default().push(entry);
        if is_new {
            self.charge
                .add(charge::entry::<String, Vec<AdjEntry>>(key_bytes));
        }
        self.charge.add(owned);
    }

    /// Create a forward (outgoing) adjacency projection.
    pub fn forward(name: impl Into<String>) -> Self {
        Self::new(name, AdjacencyDirection::Forward)
    }

    /// Create a reverse (incoming) adjacency projection.
    pub fn reverse(name: impl Into<String>) -> Self {
        Self::new(name, AdjacencyDirection::Reverse)
    }

    /// Get adjacency entries for a given key.
    pub fn entries(&self, key: &str) -> Vec<AdjEntry> {
        self.adj.get(key).map(|v| v.clone()).unwrap_or_default()
    }

    /// Convenience: get outgoing edges (only meaningful for Forward direction).
    pub fn outgoing(&self, source: &str) -> Vec<AdjEntry> {
        self.entries(source)
    }

    /// Convenience: get incoming edges (only meaningful for Reverse direction).
    pub fn incoming(&self, target: &str) -> Vec<AdjEntry> {
        self.entries(target)
    }

    /// Enumerate every edge in the projection as `(source, AdjEntry)` tuples.
    ///
    /// Only meaningful on a `Forward` projection: the key is the edge's
    /// `source` and `AdjEntry.peer` is the `target`. Used by the full-graph
    /// read endpoint to materialize every edge directly from the live
    /// adjacency index (rather than reconstructing from the events window).
    ///
    /// O(E) — one entry per live edge. Soft-deleted edges have already been
    /// removed from `adj` by the `EDGE_DELETED` handler, so this returns only
    /// live edges.
    pub fn all_edges(&self) -> Vec<(String, AdjEntry)> {
        self.adj
            .iter()
            .flat_map(|entry| {
                let source = entry.key().clone();
                entry
                    .value()
                    .iter()
                    .map(move |adj| (source.clone(), adj.clone()))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// Extract key and peer from event payload based on direction.
    fn extract_key_peer(&self, payload: &Value) -> (String, String) {
        let source = payload
            .get("source")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let target = payload
            .get("target")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        match self.direction {
            AdjacencyDirection::Forward => (source, target),
            AdjacencyDirection::Reverse => (target, source),
        }
    }
}

impl Projection for DirectedAdjacencyProjection {
    fn name(&self) -> &str {
        &self.name
    }

    fn process(&self, event: &Event) -> Result<()> {
        let event_type = event.event_type_str();
        let payload = &event.payload;

        match event_type {
            event_types::EDGE_CREATED => {
                let (key, peer) = self.extract_key_peer(payload);
                let relation = payload
                    .get("relation")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let edge_id = payload
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let weight = payload.get("weight").and_then(serde_json::Value::as_f64);

                // Secondary index for O(1) deletion
                self.index_edge(&edge_id, &key);

                self.push_entry(
                    key,
                    AdjEntry {
                        relation,
                        peer,
                        edge_id,
                        weight,
                    },
                );
            }
            event_types::EDGE_DELETED => {
                let edge_id = payload.get("id").and_then(|v| v.as_str()).unwrap_or("");

                // O(1) lookup via secondary index
                if let Some((_, key)) = self.edge_index.remove(edge_id) {
                    self.charge.sub(charge::entry::<String, String>(
                        charge::text(edge_id) + charge::text(&key),
                    ));
                    if let Some(mut entries) = self.adj.get_mut(&key) {
                        let released: u64 = entries
                            .iter()
                            .filter(|e| e.edge_id == edge_id)
                            .map(entry_bytes)
                            .sum();
                        entries.retain(|e| e.edge_id != edge_id);
                        self.charge.sub(released);
                    }
                }
            }
            _ => {}
        }

        Ok(())
    }

    fn get_state(&self, key: &str) -> Option<Value> {
        self.adj
            .get(key)
            .map(|v| serde_json::to_value(v.value()).unwrap_or(Value::Null))
    }

    fn clear(&self) {
        self.adj.clear();
        self.edge_index.clear();
        self.charge.take();
    }

    fn snapshot(&self) -> Option<Value> {
        let entries: Vec<(String, Vec<AdjEntry>)> = self
            .adj
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect();
        serde_json::to_value(entries).ok()
    }

    fn restore(&self, snapshot: &Value) -> Result<()> {
        let entries: Vec<(String, Vec<AdjEntry>)> = serde_json::from_value(snapshot.clone())
            .map_err(|e| crate::error::AllSourceError::StorageError(e.to_string()))?;
        self.clear();
        for (key, adj_entries) in entries {
            for entry in &adj_entries {
                self.index_edge(&entry.edge_id, &key);
            }
            self.store_adj(key, adj_entries);
        }
        Ok(())
    }
}

/// Backward-compatible type alias for forward adjacency.
pub type AdjacencyListProjection = DirectedAdjacencyProjection;
/// Backward-compatible type alias for reverse adjacency.
pub type ReverseIndexProjection = DirectedAdjacencyProjection;

/// Backward-compatible constructors.
impl AdjacencyListProjection {
    /// Create a forward adjacency projection (backward-compatible constructor).
    pub fn new_forward(name: impl Into<String>) -> Self {
        Self::forward(name)
    }
}

impl ReverseIndexProjection {
    /// Create a reverse adjacency projection (backward-compatible constructor).
    pub fn new_reverse(name: impl Into<String>) -> Self {
        Self::reverse(name)
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

    fn edge_created(id: &str, source: &str, target: &str, relation: &str) -> Event {
        make_event(
            &format!("edge:{id}"),
            event_types::EDGE_CREATED,
            serde_json::json!({
                "id": id,
                "source": source,
                "target": target,
                "relation": relation,
            }),
        )
    }

    fn edge_deleted(id: &str) -> Event {
        make_event(
            &format!("edge:{id}"),
            event_types::EDGE_DELETED,
            serde_json::json!({"id": id}),
        )
    }

    #[test]
    fn test_forward_and_reverse() {
        let fwd = DirectedAdjacencyProjection::forward("fwd");
        let rev = DirectedAdjacencyProjection::reverse("rev");

        let events = vec![
            edge_created("e1", "A", "B", "knows"),
            edge_created("e2", "A", "C", "knows"),
            edge_created("e3", "D", "B", "works_with"),
        ];

        for event in &events {
            fwd.process(event).unwrap();
            rev.process(event).unwrap();
        }

        // Forward: A -> [B, C]
        let a_out = fwd.outgoing("A");
        assert_eq!(a_out.len(), 2);
        let peers: Vec<&str> = a_out.iter().map(|e| e.peer.as_str()).collect();
        assert!(peers.contains(&"B"));
        assert!(peers.contains(&"C"));

        // Reverse: B <- [A, D]
        let b_in = rev.incoming("B");
        assert_eq!(b_in.len(), 2);
        let sources: Vec<&str> = b_in.iter().map(|e| e.peer.as_str()).collect();
        assert!(sources.contains(&"A"));
        assert!(sources.contains(&"D"));
    }

    #[test]
    fn test_o1_edge_deletion() {
        let fwd = DirectedAdjacencyProjection::forward("fwd");
        let rev = DirectedAdjacencyProjection::reverse("rev");

        let create = edge_created("e1", "A", "B", "knows");
        fwd.process(&create).unwrap();
        rev.process(&create).unwrap();

        assert_eq!(fwd.outgoing("A").len(), 1);
        assert_eq!(rev.incoming("B").len(), 1);

        // Delete — uses O(1) secondary index, no full scan
        let delete = edge_deleted("e1");
        fwd.process(&delete).unwrap();
        rev.process(&delete).unwrap();

        assert_eq!(fwd.outgoing("A").len(), 0);
        assert_eq!(rev.incoming("B").len(), 0);
    }

    #[test]
    fn test_snapshot_restore_preserves_edge_index() {
        let fwd = DirectedAdjacencyProjection::forward("fwd");

        fwd.process(&edge_created("e1", "X", "Y", "links")).unwrap();
        fwd.process(&edge_created("e2", "X", "Z", "links")).unwrap();

        let snap = fwd.snapshot().unwrap();
        fwd.clear();
        assert!(fwd.outgoing("X").is_empty());

        fwd.restore(&snap).unwrap();
        assert_eq!(fwd.outgoing("X").len(), 2);

        // Edge index should be rebuilt — deletion should still be O(1)
        fwd.process(&edge_deleted("e1")).unwrap();
        assert_eq!(fwd.outgoing("X").len(), 1);
        assert_eq!(fwd.outgoing("X")[0].peer, "Z");
    }

    #[test]
    fn test_delete_nonexistent_edge_is_noop() {
        let fwd = DirectedAdjacencyProjection::forward("fwd");
        fwd.process(&edge_created("e1", "A", "B", "knows")).unwrap();

        // Delete non-existent edge — should not panic or corrupt state
        fwd.process(&edge_deleted("e-nonexistent")).unwrap();
        assert_eq!(fwd.outgoing("A").len(), 1);
    }

    #[test]
    fn test_backward_compat_type_aliases() {
        // AdjacencyListProjection and ReverseIndexProjection are type aliases
        let adj = AdjacencyListProjection::new_forward("adj");
        let rev = ReverseIndexProjection::new_reverse("rev");

        let create = edge_created("e1", "X", "Y", "links");
        adj.process(&create).unwrap();
        rev.process(&create).unwrap();

        assert_eq!(adj.outgoing("X").len(), 1);
        assert_eq!(rev.incoming("Y").len(), 1);
    }
}
