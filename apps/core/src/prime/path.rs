//! Unweighted shortest path over the adjacency projections, shared by the
//! embedded [`Prime`](super::Prime) and the hosted backend so both answer the
//! same path for the same graph.

use std::collections::{HashMap, VecDeque};

use serde::{Deserialize, Serialize};

use super::{
    projections::AdjEntry,
    types::{Direction, Node},
};

/// Which way a hop crossed its edge, relative to the direction of travel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HopDirection {
    /// Travelled source → target: `from` is the edge's source.
    Outgoing,
    /// Travelled target → source: `from` is the edge's target.
    Incoming,
}

/// One edge crossed by a path, in travel order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PathHop {
    pub from: String,
    pub to: String,
    pub relation: String,
    pub direction: HopDirection,
    pub edge_id: String,
}

/// A path from `from` to `to`: the nodes in order and the edge between each pair.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphPath {
    pub nodes: Vec<Node>,
    pub hops: Vec<PathHop>,
}

/// Breadth-first search from `from` to `to`, following edges in `direction`.
///
/// `outgoing(id)` and `incoming(id)` return the adjacency entries keyed by
/// `id` (peer = target and peer = source respectively). `get_node` resolves a
/// path id to its node; ids it cannot resolve are left out of `nodes`, while
/// `hops` always carries every edge crossed.
pub(crate) fn shortest_path(
    from: &str,
    to: &str,
    relation: Option<&str>,
    direction: Direction,
    outgoing: impl Fn(&str) -> Vec<AdjEntry>,
    incoming: impl Fn(&str) -> Vec<AdjEntry>,
    get_node: impl Fn(&str) -> Option<Node>,
) -> Option<GraphPath> {
    if from == to {
        return get_node(from).map(|n| GraphPath {
            nodes: vec![n],
            hops: Vec::new(),
        });
    }

    let mut parent: HashMap<String, Option<PathHop>> = HashMap::new();
    let mut queue = VecDeque::new();
    parent.insert(from.to_string(), None);
    queue.push_back(from.to_string());

    while let Some(current) = queue.pop_front() {
        let mut steps = Vec::new();
        if matches!(direction, Direction::Outgoing | Direction::Both) {
            steps.extend(
                outgoing(&current)
                    .into_iter()
                    .map(|e| (e, HopDirection::Outgoing)),
            );
        }
        if matches!(direction, Direction::Incoming | Direction::Both) {
            steps.extend(
                incoming(&current)
                    .into_iter()
                    .map(|e| (e, HopDirection::Incoming)),
            );
        }

        for (entry, hop_direction) in steps {
            if relation.is_some_and(|r| r != entry.relation) || parent.contains_key(&entry.peer) {
                continue;
            }
            let hop = PathHop {
                from: current.clone(),
                to: entry.peer.clone(),
                relation: entry.relation,
                direction: hop_direction,
                edge_id: entry.edge_id,
            };
            parent.insert(entry.peer.clone(), Some(hop));
            if entry.peer == to {
                return Some(reconstruct(from, to, &parent, &get_node));
            }
            queue.push_back(entry.peer);
        }
    }

    None
}

fn reconstruct(
    from: &str,
    to: &str,
    parent: &HashMap<String, Option<PathHop>>,
    get_node: &impl Fn(&str) -> Option<Node>,
) -> GraphPath {
    let mut hops = Vec::new();
    let mut cursor = to.to_string();
    while let Some(Some(hop)) = parent.get(&cursor) {
        cursor = hop.from.clone();
        hops.push(hop.clone());
    }
    hops.reverse();

    let ids = std::iter::once(from).chain(hops.iter().map(|h| h.to.as_str()));
    GraphPath {
        nodes: ids.filter_map(get_node).collect(),
        hops,
    }
}
