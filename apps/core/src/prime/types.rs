//! Core types for AllSource Prime — the unified agent memory engine.
//!
//! All graph entities (nodes, edges) are stored as events in the WAL.
//! These types represent the domain model; projections maintain indexed views.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

// =============================================================================
// Entity IDs
// =============================================================================

/// Unique identifier for a graph node.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub String);

impl NodeId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for NodeId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for NodeId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Unique identifier for a graph edge.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EdgeId(pub String);

impl EdgeId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EdgeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for EdgeId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for EdgeId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

// =============================================================================
// Graph Entities
// =============================================================================

/// A graph node representing an entity in the agent's knowledge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub node_type: String,
    pub properties: Value,
    pub domain: Option<String>,
    pub labels: Vec<String>,
    pub deleted: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A directed edge connecting two nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub id: EdgeId,
    pub source: NodeId,
    pub target: NodeId,
    pub relation: String,
    pub properties: Option<Value>,
    pub weight: Option<f64>,
    pub deleted: bool,
    pub created_at: DateTime<Utc>,
}

/// Direction for graph traversal queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    /// Follow outgoing edges (source → target).
    Outgoing,
    /// Follow incoming edges (target ← source).
    Incoming,
    /// Follow edges in both directions.
    Both,
}

/// A subgraph extracted from a traversal (ego network).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubGraph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

/// Statistics about the Prime graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrimeStats {
    pub total_nodes: usize,
    pub total_edges: usize,
    pub nodes_by_type: std::collections::HashMap<String, usize>,
    pub edges_by_relation: std::collections::HashMap<String, usize>,
    pub deleted_nodes: usize,
    pub deleted_edges: usize,
    pub event_count: usize,
}

/// A single entry in an entity's audit trail.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// The event type (e.g. `prime.node.created`, `prime.edge.deleted`).
    pub event_type: String,
    /// When the event occurred.
    pub timestamp: DateTime<Utc>,
    /// The event payload.
    pub payload: Value,
    /// Optional source/correlation metadata.
    pub source: Option<String>,
}

/// Summary of changes between two points in time.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GraphDiff {
    pub nodes_added: Vec<String>,
    pub nodes_updated: Vec<String>,
    pub nodes_deleted: Vec<String>,
    pub edges_added: Vec<String>,
    pub edges_deleted: Vec<String>,
    pub vectors_stored: Vec<String>,
    pub vectors_deleted: Vec<String>,
}

// =============================================================================
// Full-Graph Read Types
// =============================================================================

/// A single node in a full-graph read, including vector presence.
///
/// Wire shape is the documented full-graph contract (see
/// `infrastructure::web::prime_api::get_full_graph`). `id` is the wire-format
/// entity id `node:{type}:{id}`. `has_vector`/`vector_dim` report embedding
/// presence and dimension only — raw float arrays are never returned by
/// default (payload bloat).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub node_type: String,
    pub properties: Value,
    pub has_vector: bool,
    pub vector_dim: Option<usize>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A single edge in a full-graph read. `source`/`target` are wire-format node
/// entity ids. `properties` is the full edge property bag (may be null).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub relation: String,
    pub properties: Option<Value>,
    pub weight: Option<f64>,
    pub created_at: DateTime<Utc>,
}

/// Summary statistics for a full-graph read. Mirrors the documented
/// `stats` block of the full-graph contract.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GraphStats {
    pub node_count: usize,
    pub edge_count: usize,
    pub vector_count: usize,
    pub nodes_by_type: std::collections::BTreeMap<String, usize>,
}

/// The complete materialized knowledge graph for one store, assembled from
/// the live `node_state` + `adjacency` projections (not an events
/// reconstruction). `has_more` is `true` when a `?limit` truncated the node
/// set; edges are filtered to the returned node set.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FullGraph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub stats: GraphStats,
    pub has_more: bool,
}

// =============================================================================
// Hybrid Recall Types
// =============================================================================

/// Query for hybrid recall combining vector similarity, graph proximity, and temporal recency.
#[derive(Debug, Clone)]
pub struct RecallQuery {
    /// Query text. Used for lexical scoring when `vector` is `None`, so a
    /// caller whose embedder is unavailable still gets a ranked answer.
    pub text: Option<String>,
    /// Query vector for similarity search.
    pub vector: Option<Vec<f32>>,
    /// Filter results to this node type.
    pub node_type: Option<String>,
    /// Graph expansion depth from vector matches.
    pub depth: usize,
    /// Maximum results to return.
    pub top_k: usize,
    /// Weight for temporal recency (0.0–1.0).
    pub recency_weight: f64,
    /// Weight for vector similarity (0.0–1.0).
    pub similarity_weight: f64,
    /// Weight for graph proximity (0.0–1.0).
    pub proximity_weight: f64,
}

impl Default for RecallQuery {
    fn default() -> Self {
        Self {
            text: None,
            vector: None,
            node_type: None,
            depth: 1,
            top_k: 10,
            recency_weight: 0.2,
            similarity_weight: 0.5,
            proximity_weight: 0.3,
        }
    }
}

/// Components of a scored node's ranking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreComponents {
    pub similarity: f64,
    pub proximity: f64,
    pub recency: f64,
}

/// A node with a combined recall score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoredNode {
    pub node: Node,
    pub score: f64,
    pub depth: usize,
    pub components: ScoreComponents,
}

/// What actually drove a recall's ranking.
///
/// A caller cannot tell a lexical answer from a semantic one by looking at the
/// nodes, and the two are not interchangeable — so recall states which it did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Retrieval {
    /// Nothing to search on, or nothing matched.
    #[default]
    Empty,
    /// Vector similarity ranked the results.
    Semantic,
    /// Query text matched against node properties — no query vector was given.
    Lexical,
    /// Every node of a type, ranked by recency. No query text to match.
    TypeScan,
}

impl Retrieval {
    /// Whether this is a fallback for semantic recall rather than the real thing.
    pub fn is_degraded(self) -> bool {
        matches!(self, Self::Lexical | Self::TypeScan)
    }
}

/// Result of a hybrid recall query.
#[derive(Debug, Clone, Default)]
pub struct RecallResult {
    pub nodes: Vec<ScoredNode>,
    #[cfg(feature = "prime-vectors")]
    pub vectors: Vec<super::vectors::VectorSearchResult>,
    pub edges: Vec<Edge>,
    /// How `nodes` were retrieved — see [`Retrieval`].
    pub retrieval: Retrieval,
}

// =============================================================================
// Event Type Constants
// =============================================================================

/// Event type namespace for all Prime events.
pub mod event_types {
    pub const NODE_CREATED: &str = "prime.node.created";
    pub const NODE_UPDATED: &str = "prime.node.updated";
    pub const NODE_DELETED: &str = "prime.node.deleted";
    pub const EDGE_CREATED: &str = "prime.edge.created";
    pub const EDGE_DELETED: &str = "prime.edge.deleted";

    /// Declarative `ProjectionDef` registration. Emitted by
    /// `Prime::define_projection`; replayed by `Prime::load_projection_defs`
    /// on startup. Multiple events for the same entity_id (one per
    /// entity_type) are valid — latest-by-timestamp wins on replay, so
    /// agents can iterate on a projection without orphaning prior defs.
    pub const PROJECTION_DEFINED: &str = "prime.projection.defined";
}

// =============================================================================
// Typed Entity ID
// =============================================================================

/// Typed entity identifier — replaces stringly-typed `node:{type}:{id}` conventions.
///
/// Wire format is preserved for backward compatibility:
/// - `node:{type}:{id}` for graph nodes
/// - `edge:{id}` for edges
/// - `vec:{id}` for vectors
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EntityId {
    Node { node_type: String, id: String },
    Edge { id: String },
    Vector { id: String },
}

impl EntityId {
    /// Parse a wire-format entity ID string.
    ///
    /// Returns `None` for unrecognized formats.
    pub fn parse(s: &str) -> Option<Self> {
        if let Some(rest) = s.strip_prefix("node:") {
            let (node_type, id) = rest.split_once(':')?;
            Some(Self::Node {
                node_type: node_type.to_string(),
                id: id.to_string(),
            })
        } else {
            s.strip_prefix("edge:")
                .map(|id| Self::Edge { id: id.to_string() })
                .or_else(|| {
                    s.strip_prefix("vec:")
                        .map(|id| Self::Vector { id: id.to_string() })
                })
        }
    }

    /// Construct a node entity ID.
    pub fn node(node_type: impl Into<String>, id: impl Into<String>) -> Self {
        Self::Node {
            node_type: node_type.into(),
            id: id.into(),
        }
    }

    /// Construct an edge entity ID.
    pub fn edge(id: impl Into<String>) -> Self {
        Self::Edge { id: id.into() }
    }

    /// Construct a vector entity ID.
    pub fn vector(id: impl Into<String>) -> Self {
        Self::Vector { id: id.into() }
    }

    /// Serialize to wire format string.
    pub fn to_wire(&self) -> String {
        match self {
            Self::Node { node_type, id } => format!("node:{node_type}:{id}"),
            Self::Edge { id } => format!("edge:{id}"),
            Self::Vector { id } => format!("vec:{id}"),
        }
    }

    /// Get the short ID segment (without prefix).
    pub fn short_id(&self) -> &str {
        match self {
            Self::Node { id, .. } | Self::Edge { id } | Self::Vector { id } => id,
        }
    }

    /// Get the node type (only for Node variant).
    pub fn node_type(&self) -> Option<&str> {
        match self {
            Self::Node { node_type, .. } => Some(node_type),
            _ => None,
        }
    }

    /// Strip `vec:` prefix to get the inner graph entity ID, if this is a Vector.
    /// For non-Vector variants, returns the wire format as-is.
    pub fn to_graph_entity_wire(&self) -> String {
        match self {
            Self::Vector { id } => id.clone(),
            other => other.to_wire(),
        }
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_wire())
    }
}

impl Serialize for EntityId {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_wire())
    }
}

impl<'de> Deserialize<'de> for EntityId {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Self::parse(&s).ok_or_else(|| serde::de::Error::custom(format!("invalid entity ID: {s}")))
    }
}

// =============================================================================
// Entity ID Formatting (deprecated — use EntityId constructors)
// =============================================================================

/// Generates the entity ID for a node event: `node:{type}:{id}`
#[deprecated(since = "0.17.0", note = "Use EntityId::node() instead")]
pub fn node_entity_id(node_type: &str, id: &str) -> String {
    EntityId::node(node_type, id).to_wire()
}

/// Generates the entity ID for an edge event: `edge:{id}`
#[deprecated(since = "0.17.0", note = "Use EntityId::edge() instead")]
pub fn edge_entity_id(id: &str) -> String {
    EntityId::edge(id).to_wire()
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_id_creation() {
        let id = NodeId::new("test-123");
        assert_eq!(id.as_str(), "test-123");
        assert_eq!(id.to_string(), "test-123");
    }

    #[test]
    fn test_node_id_from_string() {
        let id: NodeId = "test-456".into();
        assert_eq!(id.as_str(), "test-456");
    }

    #[test]
    fn test_edge_id_creation() {
        let id = EdgeId::new("edge-789");
        assert_eq!(id.as_str(), "edge-789");
    }

    #[allow(deprecated)]
    #[test]
    fn test_node_entity_id_format() {
        let entity_id = node_entity_id("person", "abc-123");
        assert_eq!(entity_id, "node:person:abc-123");
    }

    #[allow(deprecated)]
    #[test]
    fn test_edge_entity_id_format() {
        let entity_id = edge_entity_id("xyz-789");
        assert_eq!(entity_id, "edge:xyz-789");
    }

    #[test]
    fn test_entity_id_parse_node() {
        let eid = EntityId::parse("node:person:abc-123").unwrap();
        assert_eq!(eid, EntityId::node("person", "abc-123"));
        assert_eq!(eid.short_id(), "abc-123");
        assert_eq!(eid.node_type(), Some("person"));
        assert_eq!(eid.to_wire(), "node:person:abc-123");
    }

    #[test]
    fn test_entity_id_parse_edge() {
        let eid = EntityId::parse("edge:xyz-789").unwrap();
        assert_eq!(eid, EntityId::edge("xyz-789"));
        assert_eq!(eid.short_id(), "xyz-789");
        assert_eq!(eid.node_type(), None);
    }

    #[test]
    fn test_entity_id_parse_vector() {
        let eid = EntityId::parse("vec:doc-42").unwrap();
        assert_eq!(eid, EntityId::vector("doc-42"));
        assert_eq!(eid.to_graph_entity_wire(), "doc-42");
    }

    #[test]
    fn test_entity_id_parse_invalid() {
        assert!(EntityId::parse("invalid").is_none());
        assert!(EntityId::parse("").is_none());
        assert!(EntityId::parse("unknown:foo").is_none());
    }

    #[test]
    fn test_entity_id_colon_in_node_type_rejected() {
        // node:type:with:colon:id — split_once stops at first colon
        // so node_type="person", id="abc:def" (preserves colons in id)
        let eid = EntityId::parse("node:person:abc:def").unwrap();
        assert_eq!(eid.node_type(), Some("person"));
        assert_eq!(eid.short_id(), "abc:def"); // id preserves colons
    }

    #[test]
    fn test_entity_id_serialization_roundtrip() {
        let eid = EntityId::node("project", "p-1");
        let json = serde_json::to_string(&eid).unwrap();
        assert_eq!(json, "\"node:project:p-1\"");
        let parsed: EntityId = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, eid);
    }

    #[test]
    fn test_direction_serialization() {
        let json = serde_json::to_string(&Direction::Outgoing).unwrap();
        assert_eq!(json, "\"Outgoing\"");
        let parsed: Direction = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, Direction::Outgoing);
    }

    #[test]
    fn test_node_serialization_roundtrip() {
        let node = Node {
            id: NodeId::new("n-1"),
            node_type: "person".to_string(),
            properties: serde_json::json!({"name": "Alice", "role": "engineer"}),
            domain: Some("engineering".to_string()),
            labels: vec!["active".to_string()],
            deleted: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let json = serde_json::to_string(&node).unwrap();
        let parsed: Node = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, node.id);
        assert_eq!(parsed.node_type, "person");
        assert_eq!(parsed.domain, Some("engineering".to_string()));
    }

    #[test]
    fn test_edge_serialization_roundtrip() {
        let edge = Edge {
            id: EdgeId::new("e-1"),
            source: NodeId::new("n-1"),
            target: NodeId::new("n-2"),
            relation: "works_on".to_string(),
            properties: Some(serde_json::json!({"since": "2026-01"})),
            weight: Some(0.9),
            deleted: false,
            created_at: Utc::now(),
        };
        let json = serde_json::to_string(&edge).unwrap();
        let parsed: Edge = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, edge.id);
        assert_eq!(parsed.relation, "works_on");
        assert_eq!(parsed.weight, Some(0.9));
    }

    #[test]
    fn test_event_type_constants() {
        assert_eq!(event_types::NODE_CREATED, "prime.node.created");
        assert_eq!(event_types::NODE_UPDATED, "prime.node.updated");
        assert_eq!(event_types::NODE_DELETED, "prime.node.deleted");
        assert_eq!(event_types::EDGE_CREATED, "prime.edge.created");
        assert_eq!(event_types::EDGE_DELETED, "prime.edge.deleted");
    }

    #[test]
    fn test_node_id_equality_and_hash() {
        use std::collections::HashSet;
        let id1 = NodeId::new("same");
        let id2 = NodeId::new("same");
        let id3 = NodeId::new("different");
        assert_eq!(id1, id2);
        assert_ne!(id1, id3);

        let mut set = HashSet::new();
        set.insert(id1.clone());
        assert!(set.contains(&id2));
        assert!(!set.contains(&id3));
    }

    #[test]
    fn test_subgraph_empty() {
        let sg = SubGraph {
            nodes: vec![],
            edges: vec![],
        };
        assert!(sg.nodes.is_empty());
        assert!(sg.edges.is_empty());
    }

    #[test]
    fn test_prime_stats_default() {
        let stats = PrimeStats {
            total_nodes: 0,
            total_edges: 0,
            nodes_by_type: std::collections::HashMap::new(),
            edges_by_relation: std::collections::HashMap::new(),
            deleted_nodes: 0,
            deleted_edges: 0,
            event_count: 0,
        };
        assert_eq!(stats.total_nodes, 0);
    }
}
