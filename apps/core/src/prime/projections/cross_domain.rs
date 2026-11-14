//! Cross-Domain Relationship Projection — tracks edges between different domains.
//!
//! When a `prime.edge.created` event connects nodes in different domains,
//! the projection records the cross-domain link. This powers the compressed
//! index's cross-reference section.

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{mem::size_of, sync::Arc};

use super::charge;
use crate::{
    application::services::projection::Projection, domain::entities::Event, error::Result,
    infrastructure::memory::ledger::Tally, prime::types::event_types,
};

/// Projection name constant.
pub const PROJ_CROSS_DOMAIN: &str = "prime.cross_domain";

/// A link between two domains with supporting evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossDomainLink {
    pub domain_a: String,
    pub domain_b: String,
    pub edge_count: usize,
    /// Sample relation types connecting these domains (up to 5).
    pub sample_relations: Vec<String>,
}

/// Canonical key for a domain pair (alphabetically ordered to avoid (A,B) vs (B,A) duplication).
fn domain_pair_key(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    }
}

/// Internal entry tracking edges for a domain pair.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CrossDomainEntry {
    edges: Vec<String>,     // edge IDs
    relations: Vec<String>, // relation types (deduped, max 5)
}

fn string_slot(text: &str) -> u64 {
    size_of::<String>() as u64 + charge::text(text)
}

fn strings_bytes(values: &[String]) -> u64 {
    values.iter().map(|value| string_slot(value)).sum()
}

fn pair_bytes(pair: &(String, String)) -> u64 {
    charge::text(&pair.0) + charge::text(&pair.1)
}

fn entry_bytes(entry: &CrossDomainEntry) -> u64 {
    strings_bytes(&entry.edges) + strings_bytes(&entry.relations)
}

/// Tracks cross-domain edges and the relationships between domains.
pub struct CrossDomainProjection {
    name: String,
    /// (domain_a, domain_b) -> entry (alphabetically ordered keys)
    links: Arc<DashMap<(String, String), CrossDomainEntry>>,
    /// node_id -> domain (lightweight cache built from node events)
    node_domains: Arc<DashMap<String, String>>,
    charge: Tally,
}

impl CrossDomainProjection {
    pub fn new() -> Self {
        Self {
            name: PROJ_CROSS_DOMAIN.to_string(),
            links: Arc::new(DashMap::new()),
            node_domains: Arc::new(DashMap::new()),
            charge: charge::tally(),
        }
    }

    fn put_node_domain(&self, node_id: String, domain: String) {
        let key_bytes = charge::text(&node_id);
        let owned = key_bytes + charge::text(&domain);
        match self.node_domains.insert(node_id, domain) {
            Some(old) => self.charge.adjust(key_bytes + charge::text(&old), owned),
            None => self.charge.add(charge::entry::<String, String>(owned)),
        }
    }

    fn store_link(&self, key: (String, String), entry: CrossDomainEntry) {
        let key_bytes = pair_bytes(&key);
        let owned = key_bytes + entry_bytes(&entry);
        match self.links.insert(key, entry) {
            Some(old) => self.charge.adjust(key_bytes + entry_bytes(&old), owned),
            None => self
                .charge
                .add(charge::entry::<(String, String), CrossDomainEntry>(owned)),
        }
    }

    /// Get all cross-domain links.
    pub fn cross_domain_links(&self) -> Vec<CrossDomainLink> {
        self.links
            .iter()
            .map(|entry| {
                let (domain_a, domain_b) = entry.key();
                let val = entry.value();
                CrossDomainLink {
                    domain_a: domain_a.clone(),
                    domain_b: domain_b.clone(),
                    edge_count: val.edges.len(),
                    sample_relations: val.relations.clone(),
                }
            })
            .collect()
    }

    /// Get the link between two specific domains, if any.
    pub fn link_between(&self, domain_a: &str, domain_b: &str) -> Option<CrossDomainLink> {
        let key = domain_pair_key(domain_a, domain_b);
        self.links.get(&key).map(|entry| {
            let val = entry.value();
            CrossDomainLink {
                domain_a: key.0.clone(),
                domain_b: key.1.clone(),
                edge_count: val.edges.len(),
                sample_relations: val.relations.clone(),
            }
        })
    }

    /// List all domains that have cross-domain connections.
    pub fn connected_domains(&self) -> Vec<String> {
        let mut domains = std::collections::HashSet::new();
        for entry in self.links.iter() {
            domains.insert(entry.key().0.clone());
            domains.insert(entry.key().1.clone());
        }
        let mut result: Vec<String> = domains.into_iter().collect();
        result.sort();
        result
    }
}

impl Default for CrossDomainProjection {
    fn default() -> Self {
        Self::new()
    }
}

impl Projection for CrossDomainProjection {
    fn name(&self) -> &str {
        &self.name
    }

    fn process(&self, event: &Event) -> Result<()> {
        let event_type = event.event_type_str();

        match event_type {
            // Track node domains from node events
            event_types::NODE_CREATED | event_types::NODE_UPDATED => {
                // Canonical add_node shape: id under `id`, domain under
                // `properties.domain`. Fall back to legacy top-level keys. Key
                // node_domains by the full entity_id (`node:{type}:{id}`) so it
                // matches the source/target on edge events (which carry
                // entity_ids), not the bare uuid. Without this the projection
                // silently tracks nothing and reports 0 cross-domain links.
                let domain = event
                    .payload
                    .get("properties")
                    .and_then(|p| p.get("domain"))
                    .and_then(Value::as_str)
                    .or_else(|| event.payload.get("domain").and_then(Value::as_str));
                let entity_id = event.entity_id_str();
                let key = if entity_id.is_empty() {
                    event
                        .payload
                        .get("id")
                        .and_then(Value::as_str)
                        .or_else(|| event.payload.get("node_id").and_then(Value::as_str))
                        .map(str::to_string)
                } else {
                    Some(entity_id.to_string())
                };
                if let (Some(key), Some(domain)) = (key, domain) {
                    self.put_node_domain(key, domain.to_string());
                }
            }

            event_types::EDGE_CREATED => {
                if let (Some(edge_id), Some(source_id), Some(target_id)) = (
                    event
                        .payload
                        .get("id")
                        .and_then(Value::as_str)
                        .or_else(|| event.payload.get("edge_id").and_then(Value::as_str)),
                    event.payload.get("source").and_then(Value::as_str),
                    event.payload.get("target").and_then(Value::as_str),
                ) {
                    // Look up domains of source and target
                    let source_domain = self.node_domains.get(source_id).map(|d| d.clone());
                    let target_domain = self.node_domains.get(target_id).map(|d| d.clone());

                    if let (Some(sd), Some(td)) = (source_domain, target_domain) {
                        // Only record cross-domain links (different domains)
                        if sd != td {
                            let key = domain_pair_key(&sd, &td);
                            let relation = event
                                .payload
                                .get("relation")
                                .and_then(Value::as_str)
                                .unwrap_or("unknown")
                                .to_string();

                            let key_bytes = pair_bytes(&key);
                            let is_new = !self.links.contains_key(&key);
                            let mut grown = 0;
                            self.links
                                .entry(key)
                                .and_modify(|entry| {
                                    if !entry.edges.contains(&edge_id.to_string()) {
                                        entry.edges.push(edge_id.to_string());
                                        grown += string_slot(edge_id);
                                    }
                                    if entry.relations.len() < 5
                                        && !entry.relations.contains(&relation)
                                    {
                                        entry.relations.push(relation.clone());
                                        grown += string_slot(&relation);
                                    }
                                })
                                .or_insert_with(|| CrossDomainEntry {
                                    edges: vec![edge_id.to_string()],
                                    relations: vec![relation.clone()],
                                });
                            if is_new {
                                self.charge.add(
                                    charge::entry::<(String, String), CrossDomainEntry>(
                                        key_bytes + string_slot(edge_id) + string_slot(&relation),
                                    ),
                                );
                            } else {
                                self.charge.add(grown);
                            }
                        }
                    }
                }
            }

            event_types::EDGE_DELETED => {
                if let Some(edge_id) = event
                    .payload
                    .get("id")
                    .and_then(Value::as_str)
                    .or_else(|| event.payload.get("edge_id").and_then(Value::as_str))
                {
                    // Remove edge from all cross-domain entries
                    let edge_str = edge_id.to_string();
                    let mut empty_keys = Vec::new();
                    let mut released = 0;

                    for mut entry in self.links.iter_mut() {
                        entry.value_mut().edges.retain(|e| {
                            let keep = e != &edge_str;
                            if !keep {
                                released += string_slot(e);
                            }
                            keep
                        });
                        if entry.value().edges.is_empty() {
                            empty_keys.push(entry.key().clone());
                        }
                    }

                    for key in empty_keys {
                        if let Some((removed_key, removed)) = self.links.remove(&key) {
                            released += charge::entry::<(String, String), CrossDomainEntry>(
                                pair_bytes(&removed_key) + entry_bytes(&removed),
                            );
                        }
                    }
                    self.charge.sub(released);
                }
            }

            _ => {}
        }

        Ok(())
    }

    fn get_state(&self, domain_pair: &str) -> Option<Value> {
        // Accept "domainA:domainB" format
        let parts: Vec<&str> = domain_pair.split(':').collect();
        if parts.len() == 2 {
            self.link_between(parts[0], parts[1])
                .map(|link| serde_json::to_value(link).unwrap_or_default())
        } else {
            None
        }
    }

    fn clear(&self) {
        self.links.clear();
        self.node_domains.clear();
        self.charge.take();
    }

    fn snapshot(&self) -> Option<Value> {
        let links: std::collections::HashMap<String, CrossDomainEntry> = self
            .links
            .iter()
            .map(|e| {
                let key = format!("{}:{}", e.key().0, e.key().1);
                (key, e.value().clone())
            })
            .collect();
        let domains: std::collections::HashMap<String, String> = self
            .node_domains
            .iter()
            .map(|e| (e.key().clone(), e.value().clone()))
            .collect();
        Some(serde_json::json!({
            "links": links,
            "node_domains": domains,
        }))
    }

    fn restore(&self, snapshot: &Value) -> Result<()> {
        self.clear();

        if let Some(Ok(data)) = snapshot.get("links").map(|v| {
            serde_json::from_value::<std::collections::HashMap<String, CrossDomainEntry>>(v.clone())
        }) {
            for (key_str, entry) in data {
                let parts: Vec<&str> = key_str.split(':').collect();
                if parts.len() == 2 {
                    self.store_link((parts[0].to_string(), parts[1].to_string()), entry);
                }
            }
        }

        if let Some(Ok(data)) = snapshot
            .get("node_domains")
            .map(|v| serde_json::from_value::<std::collections::HashMap<String, String>>(v.clone()))
        {
            for (node_id, domain) in data {
                self.put_node_domain(node_id, domain);
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

    // Emit events in the canonical add_node/add_edge shape: node entity_id is
    // `node:concept:{node_id}`, domain lives under `properties.domain`, the id
    // key is `id`; edges carry `id` and reference nodes by their entity_id in
    // `source`/`target` (matching facade.rs). node_domains is keyed by entity_id,
    // so edges must reference entity_ids — `node_entity_id` builds them here.
    fn node_entity(node_id: &str) -> String {
        format!("node:concept:{node_id}")
    }

    fn make_node_event(node_id: &str, domain: &str) -> Event {
        Event::reconstruct_from_strings(
            Uuid::new_v4(),
            event_types::NODE_CREATED.to_string(),
            node_entity(node_id),
            "default".to_string(),
            serde_json::json!({
                "id": node_id,
                "node_type": "concept",
                "properties": { "domain": domain }
            }),
            chrono::Utc::now(),
            None,
            1,
        )
    }

    fn make_edge_event(
        event_type: &str,
        edge_id: &str,
        source: &str,
        target: &str,
        relation: &str,
    ) -> Event {
        Event::reconstruct_from_strings(
            Uuid::new_v4(),
            event_type.to_string(),
            format!("edge:{edge_id}"),
            "default".to_string(),
            serde_json::json!({
                "id": edge_id,
                "source": node_entity(source),
                "target": node_entity(target),
                "relation": relation,
            }),
            chrono::Utc::now(),
            None,
            1,
        )
    }

    #[test]
    fn test_cross_domain_link_recorded() {
        let proj = CrossDomainProjection::new();

        // Create nodes in different domains
        proj.process(&make_node_event("n1", "revenue")).unwrap();
        proj.process(&make_node_event("n2", "engineering")).unwrap();

        // Create edge between them
        proj.process(&make_edge_event(
            event_types::EDGE_CREATED,
            "e1",
            "n1",
            "n2",
            "impacts",
        ))
        .unwrap();

        let links = proj.cross_domain_links();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].edge_count, 1);
        assert!(links[0].sample_relations.contains(&"impacts".to_string()));
    }

    #[test]
    fn test_same_domain_not_recorded() {
        let proj = CrossDomainProjection::new();

        // Both nodes in same domain
        proj.process(&make_node_event("n1", "engineering")).unwrap();
        proj.process(&make_node_event("n2", "engineering")).unwrap();

        proj.process(&make_edge_event(
            event_types::EDGE_CREATED,
            "e1",
            "n1",
            "n2",
            "collaborates",
        ))
        .unwrap();

        assert!(proj.cross_domain_links().is_empty());
    }

    #[test]
    fn test_domain_pair_key_canonical() {
        // Regardless of order, same pair produces same key
        let k1 = domain_pair_key("revenue", "engineering");
        let k2 = domain_pair_key("engineering", "revenue");
        assert_eq!(k1, k2);
        assert_eq!(k1.0, "engineering"); // alphabetically first
    }

    #[test]
    fn test_multiple_edges_between_domains() {
        let proj = CrossDomainProjection::new();

        proj.process(&make_node_event("n1", "revenue")).unwrap();
        proj.process(&make_node_event("n2", "engineering")).unwrap();
        proj.process(&make_node_event("n3", "engineering")).unwrap();

        proj.process(&make_edge_event(
            event_types::EDGE_CREATED,
            "e1",
            "n1",
            "n2",
            "impacts",
        ))
        .unwrap();
        proj.process(&make_edge_event(
            event_types::EDGE_CREATED,
            "e2",
            "n1",
            "n3",
            "depends_on",
        ))
        .unwrap();

        let links = proj.cross_domain_links();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].edge_count, 2);
        assert_eq!(links[0].sample_relations.len(), 2);
    }

    #[test]
    fn test_edge_deleted_removes_link() {
        let proj = CrossDomainProjection::new();

        proj.process(&make_node_event("n1", "revenue")).unwrap();
        proj.process(&make_node_event("n2", "engineering")).unwrap();

        proj.process(&make_edge_event(
            event_types::EDGE_CREATED,
            "e1",
            "n1",
            "n2",
            "impacts",
        ))
        .unwrap();
        assert_eq!(proj.cross_domain_links().len(), 1);

        // Delete the edge
        proj.process(&make_edge_event(
            event_types::EDGE_DELETED,
            "e1",
            "n1",
            "n2",
            "impacts",
        ))
        .unwrap();

        // Link should be removed (no edges left)
        assert!(proj.cross_domain_links().is_empty());
    }

    #[test]
    fn test_link_between() {
        let proj = CrossDomainProjection::new();

        proj.process(&make_node_event("n1", "revenue")).unwrap();
        proj.process(&make_node_event("n2", "engineering")).unwrap();
        proj.process(&make_edge_event(
            event_types::EDGE_CREATED,
            "e1",
            "n1",
            "n2",
            "impacts",
        ))
        .unwrap();

        // Both orderings should work
        assert!(proj.link_between("revenue", "engineering").is_some());
        assert!(proj.link_between("engineering", "revenue").is_some());
        assert!(proj.link_between("revenue", "sales").is_none());
    }

    #[test]
    fn test_connected_domains() {
        let proj = CrossDomainProjection::new();

        proj.process(&make_node_event("n1", "revenue")).unwrap();
        proj.process(&make_node_event("n2", "engineering")).unwrap();
        proj.process(&make_node_event("n3", "sales")).unwrap();
        proj.process(&make_edge_event(
            event_types::EDGE_CREATED,
            "e1",
            "n1",
            "n2",
            "impacts",
        ))
        .unwrap();

        let connected = proj.connected_domains();
        assert_eq!(connected.len(), 2);
        assert!(connected.contains(&"engineering".to_string()));
        assert!(connected.contains(&"revenue".to_string()));
        // sales not connected
        assert!(!connected.contains(&"sales".to_string()));
    }

    #[test]
    fn test_snapshot_restore() {
        let proj = CrossDomainProjection::new();

        proj.process(&make_node_event("n1", "revenue")).unwrap();
        proj.process(&make_node_event("n2", "engineering")).unwrap();
        proj.process(&make_edge_event(
            event_types::EDGE_CREATED,
            "e1",
            "n1",
            "n2",
            "impacts",
        ))
        .unwrap();

        let snapshot = proj.snapshot().unwrap();

        // Clear and restore
        proj.clear();
        assert!(proj.cross_domain_links().is_empty());

        proj.restore(&snapshot).unwrap();
        assert_eq!(proj.cross_domain_links().len(), 1);
        // Node domains should also be restored
        assert!(proj.link_between("revenue", "engineering").is_some());
    }

    #[test]
    fn test_sample_relations_capped_at_5() {
        let proj = CrossDomainProjection::new();

        proj.process(&make_node_event("n1", "revenue")).unwrap();
        proj.process(&make_node_event("n2", "engineering")).unwrap();

        for i in 0..8 {
            proj.process(&make_edge_event(
                event_types::EDGE_CREATED,
                &format!("e{i}"),
                "n1",
                "n2",
                &format!("relation_{i}"),
            ))
            .unwrap();
        }

        let links = proj.cross_domain_links();
        assert_eq!(links[0].edge_count, 8);
        assert_eq!(links[0].sample_relations.len(), 5); // capped
    }
}
