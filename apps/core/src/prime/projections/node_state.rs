//! NodeState projection — maintains the current merged state of every node.
//!
//! Keyed by the event's `entity_id` (format `node:{type}:{id}`). Processes
//! `prime.node.created`, `prime.node.updated`, and `prime.node.deleted` events,
//! supporting deep property merges and soft deletes.

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

use crate::{
    application::services::projection::Projection, domain::entities::Event, error::Result,
    prime::types::event_types,
};

/// Serializable snapshot entry for a single node.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct NodeEntry {
    node_type: String,
    properties: Value,
    domain: Option<String>,
    labels: Vec<String>,
    deleted: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

/// Projection that maintains the current merged state of each node.
///
/// Provides O(1) lookups via [`DashMap`]. Supports snapshot/restore for
/// checkpoint-accelerated startup.
pub struct NodeStateProjection {
    name: String,
    /// entity_id -> NodeEntry
    nodes: Arc<DashMap<String, NodeEntry>>,
}

impl NodeStateProjection {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            nodes: Arc::new(DashMap::new()),
        }
    }

    /// Get the total number of nodes (including soft-deleted).
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Check if the projection is empty.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Get a node as a typed [`crate::prime::types::Node`] directly, bypassing JSON serialization.
    ///
    /// Returns `None` if not found. Includes soft-deleted nodes (check `node.deleted`).
    pub fn get_node(&self, entity_id: &str) -> Option<crate::prime::types::Node> {
        let entry = self.nodes.get(entity_id)?;
        let e = entry.value();

        // Extract short ID from entity_id "node:{type}:{id}"
        let id = crate::prime::types::EntityId::parse(entity_id)
            .map_or_else(|| entity_id.to_string(), |eid| eid.short_id().to_string());

        Some(crate::prime::types::Node {
            id: crate::prime::types::NodeId::new(id),
            node_type: e.node_type.clone(),
            properties: e.properties.clone(),
            domain: e.domain.clone(),
            labels: e.labels.clone(),
            deleted: e.deleted,
            created_at: e.created_at,
            updated_at: e.updated_at,
        })
    }

    /// Return all live (non-deleted) nodes.
    ///
    /// Constructs directly from internal state — no redundant DashMap lookup.
    /// Entity ids of every node the projection has seen, tombstones included.
    pub fn all_entity_ids(&self) -> Vec<String> {
        self.nodes.iter().map(|entry| entry.key().clone()).collect()
    }

    pub fn all_nodes(&self) -> Vec<crate::prime::types::Node> {
        self.nodes
            .iter()
            .filter(|entry| !entry.value().deleted)
            .map(|entry| {
                let e = entry.value();
                let id = crate::prime::types::EntityId::parse(entry.key())
                    .map_or_else(|| entry.key().clone(), |eid| eid.short_id().to_string());
                crate::prime::types::Node {
                    id: crate::prime::types::NodeId::new(id),
                    node_type: e.node_type.clone(),
                    properties: e.properties.clone(),
                    domain: e.domain.clone(),
                    labels: e.labels.clone(),
                    deleted: false,
                    created_at: e.created_at,
                    updated_at: e.updated_at,
                }
            })
            .collect()
    }

    /// Check if a node exists and is not deleted.
    pub fn is_live(&self, entity_id: &str) -> bool {
        self.nodes.get(entity_id).is_some_and(|e| !e.deleted)
    }

    /// Check if a node exists and is deleted.
    pub fn is_deleted(&self, entity_id: &str) -> bool {
        self.nodes.get(entity_id).is_some_and(|e| e.deleted)
    }
}

impl Projection for NodeStateProjection {
    fn name(&self) -> &str {
        &self.name
    }

    fn process(&self, event: &Event) -> Result<()> {
        let event_type = event.event_type_str();
        let entity_id = event.entity_id_str().to_string();
        let payload = &event.payload;
        let timestamp = event.timestamp;

        match event_type {
            event_types::NODE_CREATED => {
                let node_type = payload
                    .get("node_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();

                let properties = payload
                    .get("properties")
                    .cloned()
                    .unwrap_or(Value::Object(Default::default()));

                let domain = payload
                    .get("domain")
                    .and_then(|v| v.as_str())
                    .map(String::from);

                let labels = payload
                    .get("labels")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();

                self.nodes.insert(
                    entity_id,
                    NodeEntry {
                        node_type,
                        properties,
                        domain,
                        labels,
                        deleted: false,
                        created_at: timestamp,
                        updated_at: timestamp,
                    },
                );
            }
            event_types::NODE_UPDATED => {
                if let Some(mut entry) = self.nodes.get_mut(&entity_id) {
                    // Deep merge properties
                    if let (Some(Value::Object(updates)), Value::Object(existing)) =
                        (payload.get("properties"), &mut entry.properties)
                    {
                        for (key, value) in updates {
                            existing.insert(key.clone(), value.clone());
                        }
                    }

                    // Update optional fields if present
                    if let Some(domain) = payload.get("domain") {
                        entry.domain = domain.as_str().map(String::from);
                    }
                    if let Some(labels) =
                        payload.get("labels").and_then(serde_json::Value::as_array)
                    {
                        entry.labels = labels
                            .iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect();
                    }

                    entry.updated_at = timestamp;
                }
            }
            event_types::NODE_DELETED => {
                if let Some(mut entry) = self.nodes.get_mut(&entity_id) {
                    entry.deleted = true;
                    entry.updated_at = timestamp;
                }
            }
            _ => {} // Ignore non-node events
        }

        Ok(())
    }

    fn get_state(&self, entity_id: &str) -> Option<Value> {
        self.nodes
            .get(entity_id)
            .map(|entry| serde_json::to_value(entry.value()).unwrap_or(Value::Null))
    }

    fn clear(&self) {
        self.nodes.clear();
    }

    fn snapshot(&self) -> Option<Value> {
        let entries: Vec<(String, NodeEntry)> = self
            .nodes
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect();
        serde_json::to_value(entries).ok()
    }

    fn restore(&self, snapshot: &Value) -> Result<()> {
        let entries: Vec<(String, NodeEntry)> = serde_json::from_value(snapshot.clone())
            .map_err(|e| crate::error::AllSourceError::StorageError(e.to_string()))?;
        self.nodes.clear();
        for (key, entry) in entries {
            self.nodes.insert(key, entry);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn test_create_node_and_get_state() {
        let proj = NodeStateProjection::new("node_state");

        let event = make_event(
            "node:person:alice",
            event_types::NODE_CREATED,
            serde_json::json!({
                "node_type": "person",
                "properties": {"name": "Alice", "role": "engineer"},
                "domain": "engineering",
                "labels": ["active"],
            }),
        );
        proj.process(&event).unwrap();

        let state = proj.get_state("node:person:alice").unwrap();
        assert_eq!(state["node_type"], "person");
        assert_eq!(state["properties"]["name"], "Alice");
        assert_eq!(state["properties"]["role"], "engineer");
        assert_eq!(state["domain"], "engineering");
        assert_eq!(state["deleted"], false);
        assert_eq!(state["labels"][0], "active");
    }

    #[test]
    fn test_update_node_deep_merges_properties() {
        let proj = NodeStateProjection::new("node_state");

        // Create
        proj.process(&make_event(
            "node:person:bob",
            event_types::NODE_CREATED,
            serde_json::json!({
                "node_type": "person",
                "properties": {"name": "Bob", "age": 30},
            }),
        ))
        .unwrap();

        // Update — add "role", keep "name" and "age"
        proj.process(&make_event(
            "node:person:bob",
            event_types::NODE_UPDATED,
            serde_json::json!({
                "properties": {"role": "manager", "age": 31},
            }),
        ))
        .unwrap();

        let state = proj.get_state("node:person:bob").unwrap();
        assert_eq!(state["properties"]["name"], "Bob"); // preserved
        assert_eq!(state["properties"]["role"], "manager"); // added
        assert_eq!(state["properties"]["age"], 31); // updated
    }

    #[test]
    fn test_delete_node_soft_deletes() {
        let proj = NodeStateProjection::new("node_state");

        proj.process(&make_event(
            "node:person:carol",
            event_types::NODE_CREATED,
            serde_json::json!({
                "node_type": "person",
                "properties": {"name": "Carol"},
            }),
        ))
        .unwrap();

        proj.process(&make_event(
            "node:person:carol",
            event_types::NODE_DELETED,
            serde_json::json!({}),
        ))
        .unwrap();

        let state = proj.get_state("node:person:carol").unwrap();
        assert_eq!(state["deleted"], true);
        // Properties are still accessible
        assert_eq!(state["properties"]["name"], "Carol");
    }

    #[test]
    fn test_snapshot_and_restore() {
        let proj = NodeStateProjection::new("node_state");

        proj.process(&make_event(
            "node:person:dave",
            event_types::NODE_CREATED,
            serde_json::json!({
                "node_type": "person",
                "properties": {"name": "Dave"},
                "domain": "sales",
            }),
        ))
        .unwrap();

        // Snapshot
        let snap = proj.snapshot().expect("snapshot should be Some");

        // Clear and verify empty
        proj.clear();
        assert!(proj.get_state("node:person:dave").is_none());

        // Restore and verify
        proj.restore(&snap).unwrap();
        let state = proj.get_state("node:person:dave").unwrap();
        assert_eq!(state["node_type"], "person");
        assert_eq!(state["properties"]["name"], "Dave");
        assert_eq!(state["domain"], "sales");
    }

    #[test]
    fn test_ignores_non_node_events() {
        let proj = NodeStateProjection::new("node_state");

        proj.process(&make_event(
            "edge:e-1",
            event_types::EDGE_CREATED,
            serde_json::json!({"source": "a", "target": "b"}),
        ))
        .unwrap();

        assert!(proj.is_empty());
    }

    #[test]
    fn test_update_nonexistent_node_is_noop() {
        let proj = NodeStateProjection::new("node_state");

        proj.process(&make_event(
            "node:person:ghost",
            event_types::NODE_UPDATED,
            serde_json::json!({"properties": {"name": "Ghost"}}),
        ))
        .unwrap();

        assert!(proj.get_state("node:person:ghost").is_none());
    }
}
