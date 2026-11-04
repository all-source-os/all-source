//! Time-travel folds over Core's two axes.
//!
//! Record time is `EventView.timestamp`, when Core filed the event. Valid time
//! is `valid_from` (unset reads as the record time) and `valid_to` (unset is
//! open-ended). A record-time read folds in filing order. A valid-time read
//! folds in `(valid_from, filed, version)` order, so a correction filed later
//! with the same `valid_from` as the fact it corrects applies after it.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use chrono::{DateTime, Utc};
use serde_json::{Map, Value};

use super::{
    error::PrimeResult,
    event_store::EventStore,
    types::{AsOf, Direction, HistoryEntry, Node, NodeId, event_types},
};
use crate::embedded::{EventView, Query};

/// A directed edge that is live at the requested time.
pub(crate) struct LiveEdge {
    pub(crate) id: String,
    pub(crate) source: String,
    pub(crate) target: String,
    pub(crate) relation: String,
}

/// When the fact became true: `valid_from`, or the record time when unset.
pub(crate) fn valid_from(event: &EventView) -> DateTime<Utc> {
    event.valid_from.unwrap_or(event.timestamp)
}

fn axis_time(event: &EventView, valid: bool) -> DateTime<Utc> {
    if valid {
        valid_from(event)
    } else {
        event.timestamp
    }
}

/// Audit-trail row for one event, carrying both of its instants.
pub(crate) fn history_entry(event: &EventView) -> HistoryEntry {
    HistoryEntry {
        event_type: event.event_type.clone(),
        timestamp: event.timestamp,
        valid_at: valid_from(event),
        recorded_at: event.timestamp,
        payload: event.payload.clone(),
        source: event
            .metadata
            .as_ref()
            .and_then(|m| m.get("source"))
            .and_then(Value::as_str)
            .map(String::from),
    }
}

/// Events matching `query` that are visible at `at`, in fold order.
pub(crate) async fn events_at(
    core: &dyn EventStore,
    query: Query,
    at: AsOf,
) -> PrimeResult<Vec<EventView>> {
    let mut query = query.until(at.recorded.unwrap_or_else(Utc::now));
    if let Some(valid) = at.valid {
        query = query.valid_as_of(valid);
    }
    let mut events = core.query(query).await?;
    if at.valid.is_some() {
        events.sort_by_key(|e| (valid_from(e), e.timestamp, e.version));
    }
    Ok(events)
}

/// Fold a node's events into its state. `valid` selects which axis
/// `created_at` and `updated_at` are reported on.
fn fold_node(entity_id: &str, events: &[EventView], valid: bool) -> Option<Node> {
    let mut node_type = String::new();
    let mut properties = Map::new();
    let mut domain: Option<String> = None;
    let mut labels: Vec<String> = Vec::new();
    let mut deleted = false;
    let mut created_at = None;

    for event in events {
        match event.event_type.as_str() {
            event_types::NODE_CREATED => {
                node_type = event
                    .payload
                    .get("node_type")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string();
                if let Some(Value::Object(props)) = event.payload.get("properties") {
                    properties = props.clone();
                }
                domain = event
                    .payload
                    .get("domain")
                    .and_then(Value::as_str)
                    .map(String::from);
                labels = event
                    .payload
                    .get("labels")
                    .and_then(Value::as_array)
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                deleted = false;
                created_at = Some(axis_time(event, valid));
            }
            event_types::NODE_UPDATED => {
                if let Some(Value::Object(updates)) = event.payload.get("properties") {
                    for (key, value) in updates {
                        properties.insert(key.clone(), value.clone());
                    }
                }
                if let Some(d) = event.payload.get("domain") {
                    domain = d.as_str().map(String::from);
                }
                if let Some(l) = event.payload.get("labels").and_then(Value::as_array) {
                    labels = l
                        .iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect();
                }
            }
            event_types::NODE_DELETED => deleted = true,
            _ => {}
        }
    }

    if deleted || node_type.is_empty() {
        return None;
    }
    let created_at = created_at?;
    let id = entity_id.split(':').nth(2).unwrap_or(entity_id).to_string();

    Some(Node {
        id: NodeId::new(id),
        node_type,
        properties: Value::Object(properties),
        domain,
        labels,
        deleted: false,
        created_at,
        updated_at: events.last().map_or(created_at, |e| axis_time(e, valid)),
    })
}

/// The state of one node at `at`, or `None` if it did not exist or was
/// deleted by then.
pub(crate) async fn node_at(
    core: &dyn EventStore,
    entity_id: &str,
    at: AsOf,
) -> PrimeResult<Option<Node>> {
    let events = events_at(core, Query::new().entity_id(entity_id), at).await?;
    Ok(fold_node(entity_id, &events, at.valid.is_some()))
}

/// Every edge live at `at`. Edges deleted by `at` are dropped.
pub(crate) async fn live_edges_at(core: &dyn EventStore, at: AsOf) -> PrimeResult<Vec<LiveEdge>> {
    let events = events_at(core, Query::new().event_type_prefix("prime.edge."), at).await?;

    let mut live: BTreeMap<String, LiveEdge> = BTreeMap::new();
    let mut deleted: HashSet<String> = HashSet::new();
    for event in &events {
        match event.event_type.as_str() {
            event_types::EDGE_CREATED => {
                let Some(id) = event.payload.get("id").and_then(Value::as_str) else {
                    continue;
                };
                let text = |key: &str| {
                    event
                        .payload
                        .get(key)
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string()
                };
                live.insert(
                    id.to_string(),
                    LiveEdge {
                        id: id.to_string(),
                        source: text("source"),
                        target: text("target"),
                        relation: text("relation"),
                    },
                );
            }
            event_types::EDGE_DELETED => {
                if let Some(id) = event.payload.get("id").and_then(Value::as_str) {
                    deleted.insert(id.to_string());
                }
            }
            _ => {}
        }
    }

    Ok(live
        .into_values()
        .filter(|edge| !deleted.contains(&edge.id))
        .collect())
}

/// Distinct outgoing 1-hop neighbours of `entity_id` at `at`.
pub(crate) async fn neighbors_at(
    core: &dyn EventStore,
    entity_id: &str,
    relation: Option<&str>,
    at: AsOf,
) -> PrimeResult<Vec<Node>> {
    let mut neighbors = Vec::new();
    let mut seen = HashSet::new();
    for edge in live_edges_at(core, at).await? {
        if edge.source != entity_id || relation.is_some_and(|r| r != edge.relation) {
            continue;
        }
        if seen.insert(edge.target.clone())
            && let Some(node) = node_at(core, &edge.target, at).await?
        {
            neighbors.push(node);
        }
    }
    Ok(neighbors)
}

/// Breadth-first walk from `entity_id` over edges live at `at`, as
/// [`super::facade::Prime::neighbors_within`] does over the live graph. The
/// start node is included at depth 0.
pub(crate) async fn neighbors_within_at(
    core: &dyn EventStore,
    entity_id: &str,
    depth: usize,
    relation: Option<&str>,
    direction: Direction,
    at: AsOf,
) -> PrimeResult<Vec<(Node, usize)>> {
    let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();
    let edges = live_edges_at(core, at).await?;
    for edge in edges
        .iter()
        .filter(|e| relation.is_none_or(|r| r == e.relation))
    {
        if matches!(direction, Direction::Outgoing | Direction::Both) {
            adjacency
                .entry(edge.source.as_str())
                .or_default()
                .push(edge.target.as_str());
        }
        if matches!(direction, Direction::Incoming | Direction::Both) {
            adjacency
                .entry(edge.target.as_str())
                .or_default()
                .push(edge.source.as_str());
        }
    }

    let mut visited: HashSet<&str> = HashSet::from([entity_id]);
    let mut queue: VecDeque<(&str, usize)> = VecDeque::from([(entity_id, 0)]);
    let mut result = Vec::new();
    while let Some((current, d)) = queue.pop_front() {
        if let Some(node) = node_at(core, current, at).await? {
            result.push((node, d));
        }
        if d < depth {
            for peer in adjacency.get(current).into_iter().flatten() {
                if visited.insert(peer) {
                    queue.push_back((peer, d + 1));
                }
            }
        }
    }
    Ok(result)
}
