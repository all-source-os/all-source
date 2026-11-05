//! Contradiction detection projection — surfaces conflicting exclusive edges.
//!
//! Some relations are "exclusive" (e.g. `is_ceo_of` — a person can only be CEO
//! of one company). When a second edge with the same source + exclusive relation
//! but a different target is created, a contradiction is recorded.

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashSet,
    sync::{Arc, RwLock},
};

use crate::{
    application::services::projection::Projection, domain::entities::Event, error::Result,
    prime::types::event_types,
};

/// A detected contradiction between two edges.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contradiction {
    /// Unique ID for this contradiction.
    pub id: String,
    /// The source entity that has conflicting edges.
    pub entity_id: String,
    /// The exclusive relation type.
    pub relation: String,
    /// The first (existing) edge ID.
    pub existing_edge: String,
    /// The second (conflicting) edge ID.
    pub conflicting_edge: String,
    /// Whether this contradiction has been resolved.
    pub resolved: bool,
}

/// Snapshot format for persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ContradictionSnapshot {
    exclusive_relations: Vec<String>,
    /// (source, relation) -> (target, edge_id) for exclusive edges
    exclusive_edges: Vec<(String, String, String, String)>,
    contradictions: Vec<Contradiction>,
}

/// Projection that detects contradicting exclusive edges.
pub struct ContradictionDetectionProjection {
    name: String,
    /// Set of relation types that are exclusive (only one target per source).
    exclusive_relations: Arc<RwLock<HashSet<String>>>,
    /// Tracks current exclusive edges: (source, relation) -> (target, edge_id)
    exclusive_edges: Arc<DashMap<(String, String), (String, String)>>,
    /// Detected contradictions.
    contradictions: Arc<DashMap<String, Contradiction>>,
    /// Counter for generating contradiction IDs.
    counter: std::sync::atomic::AtomicU64,
}

impl ContradictionDetectionProjection {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            exclusive_relations: Arc::new(RwLock::new(HashSet::new())),
            exclusive_edges: Arc::new(DashMap::new()),
            contradictions: Arc::new(DashMap::new()),
            counter: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Register a relation as exclusive (only one target per source).
    pub fn configure_exclusive(&self, relation: &str) {
        self.exclusive_relations
            .write()
            .unwrap()
            .insert(relation.to_string());
    }

    pub fn is_exclusive(&self, relation: &str) -> bool {
        self.exclusive_relations
            .read()
            .is_ok_and(|set| set.contains(relation))
    }

    /// Backfill contradiction detection for a newly-configured exclusive relation.
    ///
    /// Scans the adjacency projection for all edges with the given relation,
    /// groups by source entity, and creates Contradiction entries for sources
    /// that have more than one target.
    pub fn backfill_exclusive(
        &self,
        relation: &str,
        adjacency: &super::adjacency::AdjacencyListProjection,
        entity_ids: &[String],
    ) {
        // Group edges by source for this relation
        let mut source_edges: std::collections::HashMap<String, Vec<(String, String)>> =
            std::collections::HashMap::new();

        for entity_id in entity_ids {
            for entry in adjacency.outgoing(entity_id) {
                if entry.relation == relation {
                    source_edges
                        .entry(entity_id.clone())
                        .or_default()
                        .push((entry.peer.clone(), entry.edge_id.clone()));
                }
            }
        }

        // Create contradictions for sources with >1 target
        for (source, targets) in &source_edges {
            if targets.len() > 1 {
                // Record the first edge as the "existing" one
                let key = (source.clone(), relation.to_string());
                if let Some(first) = targets.first() {
                    self.exclusive_edges
                        .insert(key, (first.0.clone(), first.1.clone()));
                }

                // Each additional target is a contradiction against the first
                for target in targets.iter().skip(1) {
                    let id = format!(
                        "contradiction-{}",
                        self.counter
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                    );
                    self.contradictions.insert(
                        id.clone(),
                        Contradiction {
                            id,
                            entity_id: source.clone(),
                            relation: relation.to_string(),
                            existing_edge: targets[0].1.clone(),
                            conflicting_edge: target.1.clone(),
                            resolved: false,
                        },
                    );
                }
            } else if let Some(only) = targets.first() {
                let key = (source.clone(), relation.to_string());
                self.exclusive_edges
                    .insert(key, (only.0.clone(), only.1.clone()));
            }
        }
    }

    /// List all unresolved contradictions.
    pub fn contradictions(&self) -> Vec<Contradiction> {
        self.contradictions
            .iter()
            .filter(|entry| !entry.value().resolved)
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Resolve a contradiction by keeping the specified edge and deleting the other.
    /// Returns the edge ID that should be deleted.
    pub fn resolve(&self, contradiction_id: &str, keep_edge: &str) -> Option<String> {
        if let Some(mut entry) = self.contradictions.get_mut(contradiction_id) {
            entry.resolved = true;

            let delete_edge = if entry.existing_edge == keep_edge {
                entry.conflicting_edge.clone()
            } else {
                entry.existing_edge.clone()
            };

            // Update the exclusive edge to point to the kept edge's target
            let key = (entry.entity_id.clone(), entry.relation.clone());
            if let Some(mut edge_entry) = self.exclusive_edges.get_mut(&key) {
                *edge_entry = (String::new(), keep_edge.to_string());
            }

            Some(delete_edge)
        } else {
            None
        }
    }
}

impl Projection for ContradictionDetectionProjection {
    fn name(&self) -> &str {
        &self.name
    }

    fn process(&self, event: &Event) -> Result<()> {
        let event_type = event.event_type_str();

        if event_type != event_types::EDGE_CREATED {
            return Ok(());
        }

        let payload = &event.payload;
        let Some(relation) = payload.get("relation").and_then(|v| v.as_str()) else {
            return Ok(());
        };

        // Only check exclusive relations
        let is_exclusive = self.exclusive_relations.read().unwrap().contains(relation);
        if !is_exclusive {
            return Ok(());
        }

        let source = match payload.get("source").and_then(|v| v.as_str()) {
            Some(s) => s.to_string(),
            None => return Ok(()),
        };
        let target = match payload.get("target").and_then(|v| v.as_str()) {
            Some(t) => t.to_string(),
            None => return Ok(()),
        };
        let edge_id = match payload.get("id").and_then(|v| v.as_str()) {
            Some(id) => id.to_string(),
            None => return Ok(()),
        };

        let key = (source.clone(), relation.to_string());

        if let Some(existing) = self.exclusive_edges.get(&key) {
            let (existing_target, existing_edge_id) = existing.value().clone();
            if existing_target != target {
                // Contradiction detected!
                let id = format!(
                    "contradiction-{}",
                    self.counter
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                );
                self.contradictions.insert(
                    id.clone(),
                    Contradiction {
                        id,
                        entity_id: source,
                        relation: relation.to_string(),
                        existing_edge: existing_edge_id,
                        conflicting_edge: edge_id,
                        resolved: false,
                    },
                );
            }
        } else {
            self.exclusive_edges.insert(key, (target, edge_id));
        }

        Ok(())
    }

    fn get_state(&self, _entity_id: &str) -> Option<Value> {
        let contradictions = self.contradictions();
        serde_json::to_value(contradictions).ok()
    }

    fn clear(&self) {
        self.exclusive_edges.clear();
        self.contradictions.clear();
        self.counter.store(0, std::sync::atomic::Ordering::Relaxed);
    }

    fn snapshot(&self) -> Option<Value> {
        let exclusive_relations: Vec<String> = self
            .exclusive_relations
            .read()
            .unwrap()
            .iter()
            .cloned()
            .collect();

        let exclusive_edges: Vec<(String, String, String, String)> = self
            .exclusive_edges
            .iter()
            .map(|entry| {
                let (source, relation) = entry.key().clone();
                let (target, edge_id) = entry.value().clone();
                (source, relation, target, edge_id)
            })
            .collect();

        let contradictions: Vec<Contradiction> = self
            .contradictions
            .iter()
            .map(|entry| entry.value().clone())
            .collect();

        let snap = ContradictionSnapshot {
            exclusive_relations,
            exclusive_edges,
            contradictions,
        };
        serde_json::to_value(snap).ok()
    }

    fn restore(&self, snapshot: &Value) -> Result<()> {
        let snap: ContradictionSnapshot = serde_json::from_value(snapshot.clone())
            .map_err(|e| crate::error::AllSourceError::StorageError(e.to_string()))?;

        {
            let mut relations = self.exclusive_relations.write().unwrap();
            relations.clear();
            for r in snap.exclusive_relations {
                relations.insert(r);
            }
        }

        self.exclusive_edges.clear();
        for (source, relation, target, edge_id) in snap.exclusive_edges {
            self.exclusive_edges
                .insert((source, relation), (target, edge_id));
        }

        self.contradictions.clear();
        for c in snap.contradictions {
            self.contradictions.insert(c.id.clone(), c);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn make_edge_event(edge_id: &str, source: &str, target: &str, relation: &str) -> Event {
        Event::reconstruct_from_strings(
            Uuid::new_v4(),
            event_types::EDGE_CREATED.to_string(),
            format!("edge:{edge_id}"),
            "default".to_string(),
            serde_json::json!({
                "id": edge_id,
                "source": source,
                "target": target,
                "relation": relation,
            }),
            Utc::now(),
            None,
            1,
        )
    }

    #[test]
    fn test_contradiction_detected_for_exclusive_relation() {
        let proj = ContradictionDetectionProjection::new("contradiction");
        proj.configure_exclusive("is_ceo_of");

        // Alice is_ceo_of CompanyA
        proj.process(&make_edge_event("e-1", "alice", "companyA", "is_ceo_of"))
            .unwrap();

        // Alice is_ceo_of CompanyB — contradiction!
        proj.process(&make_edge_event("e-2", "alice", "companyB", "is_ceo_of"))
            .unwrap();

        let contradictions = proj.contradictions();
        assert_eq!(contradictions.len(), 1);
        assert_eq!(contradictions[0].entity_id, "alice");
        assert_eq!(contradictions[0].relation, "is_ceo_of");
        assert_eq!(contradictions[0].existing_edge, "e-1");
        assert_eq!(contradictions[0].conflicting_edge, "e-2");
        assert!(!contradictions[0].resolved);
    }

    #[test]
    fn test_no_contradiction_for_non_exclusive_relation() {
        let proj = ContradictionDetectionProjection::new("contradiction");
        proj.configure_exclusive("is_ceo_of");

        // "knows" is not exclusive — multiple targets are fine
        proj.process(&make_edge_event("e-1", "alice", "bob", "knows"))
            .unwrap();
        proj.process(&make_edge_event("e-2", "alice", "carol", "knows"))
            .unwrap();

        assert!(proj.contradictions().is_empty());
    }

    #[test]
    fn test_no_contradiction_for_same_target() {
        let proj = ContradictionDetectionProjection::new("contradiction");
        proj.configure_exclusive("is_ceo_of");

        // Same source, same relation, same target — not a contradiction
        proj.process(&make_edge_event("e-1", "alice", "companyA", "is_ceo_of"))
            .unwrap();
        proj.process(&make_edge_event("e-2", "alice", "companyA", "is_ceo_of"))
            .unwrap();

        assert!(proj.contradictions().is_empty());
    }

    #[test]
    fn test_resolve_contradiction() {
        let proj = ContradictionDetectionProjection::new("contradiction");
        proj.configure_exclusive("is_ceo_of");

        proj.process(&make_edge_event("e-1", "alice", "companyA", "is_ceo_of"))
            .unwrap();
        proj.process(&make_edge_event("e-2", "alice", "companyB", "is_ceo_of"))
            .unwrap();

        let contradictions = proj.contradictions();
        assert_eq!(contradictions.len(), 1);
        let c_id = &contradictions[0].id;

        // Resolve: keep e-2, delete e-1
        let to_delete = proj.resolve(c_id, "e-2").unwrap();
        assert_eq!(to_delete, "e-1");

        // No more unresolved contradictions
        assert!(proj.contradictions().is_empty());
    }

    #[test]
    fn test_different_sources_no_contradiction() {
        let proj = ContradictionDetectionProjection::new("contradiction");
        proj.configure_exclusive("is_ceo_of");

        // Different sources — each can be CEO of a different company
        proj.process(&make_edge_event("e-1", "alice", "companyA", "is_ceo_of"))
            .unwrap();
        proj.process(&make_edge_event("e-2", "bob", "companyB", "is_ceo_of"))
            .unwrap();

        assert!(proj.contradictions().is_empty());
    }

    #[test]
    fn test_snapshot_and_restore() {
        let proj = ContradictionDetectionProjection::new("contradiction");
        proj.configure_exclusive("is_ceo_of");

        proj.process(&make_edge_event("e-1", "alice", "companyA", "is_ceo_of"))
            .unwrap();
        proj.process(&make_edge_event("e-2", "alice", "companyB", "is_ceo_of"))
            .unwrap();

        assert_eq!(proj.contradictions().len(), 1);

        // Snapshot
        let snap = proj.snapshot().expect("snapshot should be Some");

        // Clear
        proj.clear();
        assert!(proj.contradictions().is_empty());

        // Restore
        proj.restore(&snap).unwrap();
        assert_eq!(proj.contradictions().len(), 1);
        assert_eq!(proj.contradictions()[0].relation, "is_ceo_of");
    }

    #[test]
    fn test_ignores_non_edge_events() {
        let proj = ContradictionDetectionProjection::new("contradiction");
        proj.configure_exclusive("is_ceo_of");

        let event = Event::reconstruct_from_strings(
            Uuid::new_v4(),
            event_types::NODE_CREATED.to_string(),
            "node:person:alice".to_string(),
            "default".to_string(),
            serde_json::json!({"node_type": "person"}),
            Utc::now(),
            None,
            1,
        );
        proj.process(&event).unwrap();

        assert!(proj.contradictions().is_empty());
    }

    #[test]
    fn test_multiple_exclusive_relations() {
        let proj = ContradictionDetectionProjection::new("contradiction");
        proj.configure_exclusive("is_ceo_of");
        proj.configure_exclusive("capital_of");

        proj.process(&make_edge_event("e-1", "alice", "companyA", "is_ceo_of"))
            .unwrap();
        proj.process(&make_edge_event("e-2", "alice", "companyB", "is_ceo_of"))
            .unwrap();
        proj.process(&make_edge_event("e-3", "france", "paris", "capital_of"))
            .unwrap();
        proj.process(&make_edge_event("e-4", "france", "lyon", "capital_of"))
            .unwrap();

        assert_eq!(proj.contradictions().len(), 2);
    }
}
