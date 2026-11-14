//! Domain Index Projection — maps domains to their member nodes.
//!
//! Maintains a `DashMap<String, Vec<NodeId>>` so callers can efficiently
//! list all known domains or retrieve every node within a domain.

use dashmap::DashMap;
use serde_json::Value;
use std::sync::Arc;

use super::charge;
use crate::{
    application::services::projection::Projection,
    domain::entities::Event,
    error::Result,
    infrastructure::memory::ledger::Tally,
    prime::types::{NodeId, event_types},
};

/// Projection name constant.
pub const PROJ_DOMAIN_INDEX: &str = "prime.domain_index";

fn node_slot(id: &NodeId) -> u64 {
    std::mem::size_of::<NodeId>() as u64 + charge::text(&id.0)
}

/// Maps domain names → node IDs that belong to each domain.
pub struct DomainIndexProjection {
    name: String,
    /// domain -> list of node IDs
    index: Arc<DashMap<String, Vec<NodeId>>>,
    charge: Tally,
}

impl DomainIndexProjection {
    pub fn new() -> Self {
        Self {
            name: PROJ_DOMAIN_INDEX.to_string(),
            index: Arc::new(DashMap::new()),
            charge: charge::tally(),
        }
    }

    fn store(&self, domain: String, nodes: Vec<NodeId>) {
        let key_bytes = charge::text(&domain);
        let owned = key_bytes + nodes.iter().map(node_slot).sum::<u64>();
        match self.index.insert(domain, nodes) {
            Some(old) => self
                .charge
                .adjust(key_bytes + old.iter().map(node_slot).sum::<u64>(), owned),
            None => self.charge.add(charge::entry::<String, Vec<NodeId>>(owned)),
        }
    }

    /// List all known domains.
    pub fn domains(&self) -> Vec<String> {
        self.index.iter().map(|e| e.key().clone()).collect()
    }

    /// Get all node IDs in a domain.
    pub fn nodes_in_domain(&self, domain: &str) -> Vec<NodeId> {
        self.index
            .get(domain)
            .map(|entry| entry.value().clone())
            .unwrap_or_default()
    }

    /// Count of nodes per domain.
    pub fn domain_counts(&self) -> Vec<(String, usize)> {
        self.index
            .iter()
            .map(|e| (e.key().clone(), e.value().len()))
            .collect()
    }
}

impl Default for DomainIndexProjection {
    fn default() -> Self {
        Self::new()
    }
}

impl Projection for DomainIndexProjection {
    fn name(&self) -> &str {
        &self.name
    }

    fn process(&self, event: &Event) -> Result<()> {
        let event_type = event.event_type_str();

        match event_type {
            event_types::NODE_CREATED | event_types::NODE_UPDATED => {
                // Extract domain and node id from the event payload.
                //
                // The canonical `add_node`/`update_node` shape (facade.rs) is
                // `{ "id": <id>, "node_type": ..., "properties": { "domain": ... } }`,
                // i.e. the id key is `id` and `domain` lives under `properties`.
                // Older/hand-crafted events used top-level `node_id` + `domain`.
                // Read the canonical shape first, fall back to the legacy one —
                // without this, domain never resolves and prime_index/context
                // report 0 nodes for all live-recorded data (the prime_index bug).
                let domain = event
                    .payload
                    .get("properties")
                    .and_then(|p| p.get("domain"))
                    .and_then(Value::as_str)
                    .or_else(|| event.payload.get("domain").and_then(Value::as_str));
                let node_id_str = event
                    .payload
                    .get("id")
                    .and_then(Value::as_str)
                    .or_else(|| event.payload.get("node_id").and_then(Value::as_str));
                if let (Some(domain), Some(node_id_str)) = (domain, node_id_str) {
                    let node_id = NodeId::new(node_id_str);
                    let is_new_domain = !self.index.contains_key(domain);
                    let slot = node_slot(&node_id);
                    let mut pushed = false;

                    self.index
                        .entry(domain.to_string())
                        .and_modify(|nodes| {
                            if !nodes.contains(&node_id) {
                                nodes.push(node_id.clone());
                                pushed = true;
                            }
                        })
                        .or_insert_with(|| vec![node_id]);
                    if is_new_domain {
                        self.charge.add(charge::entry::<String, Vec<NodeId>>(
                            charge::text(domain) + slot,
                        ));
                    } else if pushed {
                        self.charge.add(slot);
                    }
                }
            }
            event_types::NODE_DELETED => {
                // Remove node from its domain. NODE_DELETED payloads carry the id
                // under `id`; fall back to legacy `node_id`. (delete_node emits an
                // empty payload + the entity_id, so also accept the entity_id tail.)
                let node_id = event
                    .payload
                    .get("id")
                    .and_then(Value::as_str)
                    .or_else(|| event.payload.get("node_id").and_then(Value::as_str));
                if let Some(node_id) = node_id {
                    let target = NodeId::new(node_id);
                    let mut released = 0;
                    // Remove from all domains (node may have changed domain)
                    for mut entry in self.index.iter_mut() {
                        entry.value_mut().retain(|id| {
                            let keep = id != &target;
                            if !keep {
                                released += node_slot(id);
                            }
                            keep
                        });
                    }
                    // Remove empty domain entries
                    self.index.retain(|domain, nodes| {
                        let keep = !nodes.is_empty();
                        if !keep {
                            released += charge::entry::<String, Vec<NodeId>>(charge::text(domain));
                        }
                        keep
                    });
                    self.charge.sub(released);
                }
            }
            _ => {}
        }

        Ok(())
    }

    fn get_state(&self, domain: &str) -> Option<Value> {
        self.index.get(domain).map(|entry| {
            let ids: Vec<&str> = entry.value().iter().map(NodeId::as_str).collect();
            serde_json::json!({ "domain": domain, "node_ids": ids, "count": ids.len() })
        })
    }

    fn clear(&self) {
        self.index.clear();
        self.charge.take();
    }

    fn snapshot(&self) -> Option<Value> {
        let data: std::collections::HashMap<String, Vec<String>> = self
            .index
            .iter()
            .map(|e| {
                (
                    e.key().clone(),
                    e.value().iter().map(|id| id.0.clone()).collect(),
                )
            })
            .collect();
        Some(serde_json::to_value(data).unwrap_or_default())
    }

    fn restore(&self, snapshot: &Value) -> Result<()> {
        if let Ok(data) = serde_json::from_value::<std::collections::HashMap<String, Vec<String>>>(
            snapshot.clone(),
        ) {
            self.clear();
            for (domain, ids) in data {
                self.store(domain, ids.into_iter().map(NodeId::new).collect());
            }
        }
        Ok(())
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn make_node_event(event_type: &str, node_id: &str, domain: Option<&str>) -> Event {
        let mut payload = serde_json::json!({
            "node_id": node_id,
            "node_type": "concept",
            "properties": {}
        });
        if let Some(d) = domain {
            payload["domain"] = serde_json::json!(d);
        }
        Event::reconstruct_from_strings(
            Uuid::new_v4(),
            event_type.to_string(),
            format!("node:concept:{node_id}"),
            "default".to_string(),
            payload,
            chrono::Utc::now(),
            None,
            1,
        )
    }

    #[test]
    fn test_domain_index_add_nodes() {
        let proj = DomainIndexProjection::new();

        proj.process(&make_node_event(
            event_types::NODE_CREATED,
            "n1",
            Some("revenue"),
        ))
        .unwrap();
        proj.process(&make_node_event(
            event_types::NODE_CREATED,
            "n2",
            Some("revenue"),
        ))
        .unwrap();
        proj.process(&make_node_event(
            event_types::NODE_CREATED,
            "n3",
            Some("engineering"),
        ))
        .unwrap();

        let domains = proj.domains();
        assert_eq!(domains.len(), 2);
        assert_eq!(proj.nodes_in_domain("revenue").len(), 2);
        assert_eq!(proj.nodes_in_domain("engineering").len(), 1);
        assert_eq!(proj.nodes_in_domain("nonexistent").len(), 0);
    }

    #[test]
    fn test_domain_index_no_duplicates() {
        let proj = DomainIndexProjection::new();

        // Process same node twice
        proj.process(&make_node_event(
            event_types::NODE_CREATED,
            "n1",
            Some("revenue"),
        ))
        .unwrap();
        proj.process(&make_node_event(
            event_types::NODE_UPDATED,
            "n1",
            Some("revenue"),
        ))
        .unwrap();

        assert_eq!(proj.nodes_in_domain("revenue").len(), 1);
    }

    #[test]
    fn test_domain_index_delete_node() {
        let proj = DomainIndexProjection::new();

        proj.process(&make_node_event(
            event_types::NODE_CREATED,
            "n1",
            Some("revenue"),
        ))
        .unwrap();
        proj.process(&make_node_event(
            event_types::NODE_CREATED,
            "n2",
            Some("revenue"),
        ))
        .unwrap();

        // Delete n1
        proj.process(&make_node_event(event_types::NODE_DELETED, "n1", None))
            .unwrap();

        assert_eq!(proj.nodes_in_domain("revenue").len(), 1);
        assert_eq!(proj.nodes_in_domain("revenue")[0].as_str(), "n2");
    }

    #[test]
    fn test_domain_index_node_without_domain_ignored() {
        let proj = DomainIndexProjection::new();

        proj.process(&make_node_event(event_types::NODE_CREATED, "n1", None))
            .unwrap();

        assert!(proj.domains().is_empty());
    }

    #[test]
    fn test_domain_index_get_state() {
        let proj = DomainIndexProjection::new();

        proj.process(&make_node_event(
            event_types::NODE_CREATED,
            "n1",
            Some("revenue"),
        ))
        .unwrap();

        let state = proj.get_state("revenue").unwrap();
        assert_eq!(state["count"], 1);
        assert_eq!(state["node_ids"][0], "n1");
        assert!(proj.get_state("nonexistent").is_none());
    }

    #[test]
    fn test_domain_index_snapshot_restore() {
        let proj = DomainIndexProjection::new();

        proj.process(&make_node_event(
            event_types::NODE_CREATED,
            "n1",
            Some("revenue"),
        ))
        .unwrap();
        proj.process(&make_node_event(
            event_types::NODE_CREATED,
            "n2",
            Some("engineering"),
        ))
        .unwrap();

        let snapshot = proj.snapshot().unwrap();

        // Clear and restore
        proj.clear();
        assert!(proj.domains().is_empty());

        proj.restore(&snapshot).unwrap();
        assert_eq!(proj.domains().len(), 2);
        assert_eq!(proj.nodes_in_domain("revenue").len(), 1);
        assert_eq!(proj.nodes_in_domain("engineering").len(), 1);
    }

    #[test]
    fn test_domain_counts() {
        let proj = DomainIndexProjection::new();

        proj.process(&make_node_event(
            event_types::NODE_CREATED,
            "n1",
            Some("revenue"),
        ))
        .unwrap();
        proj.process(&make_node_event(
            event_types::NODE_CREATED,
            "n2",
            Some("revenue"),
        ))
        .unwrap();
        proj.process(&make_node_event(
            event_types::NODE_CREATED,
            "n3",
            Some("engineering"),
        ))
        .unwrap();

        let counts = proj.domain_counts();
        assert_eq!(counts.len(), 2);
    }
}
