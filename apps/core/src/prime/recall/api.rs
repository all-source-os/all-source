//! Recall API — `index()` and `context()` methods for the Prime facade.
//!
//! Provides the user-facing agent memory API that combines compressed index,
//! vector search results, and graph context into a single retrieval call.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::{
    compressor::IndexCompressor,
    index_builder::{build_heuristic_index, build_raw_summary},
    types::{CompressedIndex, ContextTier, IndexConfig, LlmBackend, RankedMemory, RecallContext},
};
use crate::{
    application::services::projection::Projection,
    prime::{
        error::{PrimeError, PrimeResult},
        event_store::EventStore,
        projections::{
            AdjacencyListProjection, CrossDomainProjection, DomainIndexProjection,
            GraphStatsProjection, NodeStateProjection,
        },
        temporal,
        types::{AsOf, Edge, EdgeId, Node, NodeId, PrimeStats, node_entity_id},
    },
};

// =============================================================================
// Query types
// =============================================================================

/// Query parameters for `recall.context()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecallContextQuery {
    /// Natural language query string.
    pub query: String,
    /// Agent ID for scoping.
    pub agent_id: Option<String>,
    /// Max number of vector results (default: 5).
    pub top_k: usize,
    /// Time-travel. L1 candidates, peers and edges are read at this point on
    /// the given axes. L0 stats and the L2 compressed index are current state;
    /// the L2 vector and graph arms follow it when the caller uses
    /// [`Prime::recall_at`](crate::prime::Prime::recall_at).
    pub as_of: Option<AsOf>,
    /// Whether to include the compressed index excerpt in the response.
    pub include_index: bool,
    /// Max total tokens in the response (truncate if exceeded).
    pub max_tokens: Option<usize>,
    /// Retrieval tier (default: L2 — full hybrid recall).
    pub tier: ContextTier,
    /// Conversation ID for L1 scoping. Ignored for L0/L2.
    pub conversation_id: Option<String>,
}

impl Default for RecallContextQuery {
    fn default() -> Self {
        Self {
            query: String::new(),
            agent_id: None,
            top_k: 5,
            as_of: None,
            include_index: true,
            max_tokens: None,
            tier: ContextTier::default(),
            conversation_id: None,
        }
    }
}

// =============================================================================
// Recall Dependencies
// =============================================================================

/// Bundled projection dependencies for the recall engine.
///
/// Groups the projections needed for tiered context retrieval into a single
/// struct, keeping the `RecallEngine` constructor clean as dependencies grow.
pub struct RecallDeps {
    pub domain_index: Arc<DomainIndexProjection>,
    pub cross_domain: Arc<CrossDomainProjection>,
    pub node_state: Option<Arc<NodeStateProjection>>,
    pub adjacency: Option<Arc<AdjacencyListProjection>>,
    pub graph_stats: Option<Arc<GraphStatsProjection>>,
    /// Event store for reads at a point in time. Without it, `as_of` on a
    /// context query is an error rather than a silent current-state answer.
    pub core: Option<Arc<dyn EventStore>>,
}

// =============================================================================
// Recall Engine
// =============================================================================

/// Agent memory engine that wraps Prime's projections to provide
/// `index()` and `context()` retrieval methods.
///
/// Supports tiered retrieval via [`ContextTier`]:
/// - **L0**: stats only (~100–200 tokens)
/// - **L1**: recent conversation nodes + 1-hop edges (~500–1500 tokens)
/// - **L2**: full hybrid recall with compressed index (~2000–5000 tokens)
pub struct RecallEngine {
    domain_index: Arc<DomainIndexProjection>,
    cross_domain: Arc<CrossDomainProjection>,
    node_state: Option<Arc<NodeStateProjection>>,
    adjacency: Option<Arc<AdjacencyListProjection>>,
    graph_stats: Option<Arc<GraphStatsProjection>>,
    core: Option<Arc<dyn EventStore>>,
    compressor: IndexCompressor,
}

impl RecallEngine {
    /// Create a new Recall engine with the given index configuration.
    ///
    /// Convenience constructor that creates default projections and an
    /// `IndexCompressor` configured from `IndexConfig`. If `IndexConfig`
    /// has an `llm_endpoint`, an `OllamaBackend` is created automatically.
    ///
    /// Note: This constructor creates standalone projections. For L0/L1 tier
    /// support with shared projections from Prime, use [`RecallEngine::with_deps`].
    pub fn new(config: &IndexConfig) -> Self {
        let llm_backend: Option<Box<dyn LlmBackend>> =
            if let Some(ref endpoint) = config.llm_endpoint {
                let model = config
                    .llm_model
                    .clone()
                    .unwrap_or_else(|| "mistral".to_string());
                Some(Box::new(super::ollama::OllamaBackend::new(
                    endpoint.clone(),
                    model,
                )))
            } else {
                None
            };

        Self {
            domain_index: Arc::new(DomainIndexProjection::new()),
            cross_domain: Arc::new(CrossDomainProjection::new()),
            node_state: None,
            adjacency: None,
            graph_stats: None,
            core: None,
            compressor: IndexCompressor::new(
                llm_backend,
                config.refresh_interval_events,
                config.refresh_interval_seconds,
            ),
        }
    }

    /// Create a Recall engine with shared projection dependencies from Prime.
    ///
    /// This is the preferred constructor when Prime owns the projections.
    /// Enables L0 (stats) and L1 (conversation context) tiers.
    pub fn with_deps(deps: RecallDeps, config: &IndexConfig) -> Self {
        let llm_backend: Option<Box<dyn LlmBackend>> =
            if let Some(ref endpoint) = config.llm_endpoint {
                let model = config
                    .llm_model
                    .clone()
                    .unwrap_or_else(|| "mistral".to_string());
                Some(Box::new(super::ollama::OllamaBackend::new(
                    endpoint.clone(),
                    model,
                )))
            } else {
                None
            };

        Self {
            domain_index: deps.domain_index,
            cross_domain: deps.cross_domain,
            node_state: deps.node_state,
            adjacency: deps.adjacency,
            graph_stats: deps.graph_stats,
            core: deps.core,
            compressor: IndexCompressor::new(
                llm_backend,
                config.refresh_interval_events,
                config.refresh_interval_seconds,
            ),
        }
    }

    /// Create with injected dependencies (for testing and flexibility).
    pub fn with_dependencies(
        domain_index: Arc<DomainIndexProjection>,
        cross_domain: Arc<CrossDomainProjection>,
        compressor: IndexCompressor,
    ) -> Self {
        Self {
            domain_index,
            cross_domain,
            node_state: None,
            adjacency: None,
            graph_stats: None,
            core: None,
            compressor,
        }
    }

    /// Get the current compressed index.
    ///
    /// Generates on first call, cached thereafter (respecting refresh thresholds).
    pub async fn index(&self) -> CompressedIndex {
        let summary = build_raw_summary(&self.domain_index, &self.cross_domain);
        let heuristic = build_heuristic_index(&summary);
        let event_count = summary.total_nodes as u64 + summary.total_edges as u64;

        self.compressor
            .compress(&summary, event_count, &heuristic)
            .await
    }

    /// Combined retrieval dispatched by tier.
    ///
    /// - `L0`: stats only — no index generation, no vector search.
    /// - `L1`: stats + recent conversation nodes + 1-hop edges.
    /// - `L2`: full hybrid recall (compressed index + vectors + graph expansion).
    pub async fn context(&self, query: RecallContextQuery) -> PrimeResult<RecallContext> {
        Ok(match query.tier {
            ContextTier::L0 => self.context_l0(&query),
            ContextTier::L1 => match query.as_of {
                None => self.context_l1(&query),
                Some(at) => self.context_l1_at(&query, at).await?,
            },
            ContextTier::L2 => self.context_l2(&query).await,
        })
    }

    /// L0: stats-only retrieval. No I/O, no compression.
    fn context_l0(&self, _query: &RecallContextQuery) -> RecallContext {
        let stats = self.build_stats();
        let stats_json = serde_json::to_string(&stats).unwrap_or_default();
        let token_count = super::types::estimate_tokens(&stats_json);

        RecallContext {
            index: String::new(),
            vectors: Vec::new(),
            nodes: Vec::new(),
            edges: Vec::new(),
            stats: Some(stats),
            tier: ContextTier::L0,
            token_count,
        }
    }

    /// L1: conversation-scoped recent context.
    ///
    /// Returns L0 stats + recent nodes + their 1-hop edges.
    /// If `conversation_id` is set, filters to nodes from that conversation.
    /// Without `conversation_id`, returns the most recent nodes across all data.
    fn context_l1(&self, query: &RecallContextQuery) -> RecallContext {
        let stats = self.build_stats();
        let mut nodes: Vec<Node> = Vec::new();
        let mut edges: Vec<crate::prime::types::Edge> = Vec::new();

        // Get recent nodes from node_state projection
        if let Some(ref node_state) = self.node_state {
            let all_nodes = node_state.all_nodes();

            // If conversation_id is provided, filter nodes that have matching
            // conversation metadata. Otherwise, take the most recent N nodes.
            let mut candidate_nodes: Vec<Node> = if let Some(ref conv_id) = query.conversation_id {
                all_nodes
                    .into_iter()
                    .filter(|n| {
                        n.properties
                            .get("conversation_id")
                            .and_then(|v| v.as_str())
                            .is_some_and(|id| id == conv_id)
                    })
                    .collect()
            } else {
                all_nodes
            };

            // Sort by updated_at descending, take most recent 20
            candidate_nodes.sort_by_key(|a| std::cmp::Reverse(a.updated_at));
            candidate_nodes.truncate(20);

            // Expand 1-hop outgoing edges from these nodes
            if let Some(ref adjacency) = self.adjacency {
                for node in &candidate_nodes {
                    // entity_id format is "node:{type}:{id}", but adjacency
                    // keys use the raw source/target from edge events (node IDs).
                    let adj_entries = adjacency.outgoing(node.id.as_str());
                    for adj in &adj_entries {
                        // Resolve peer node if available
                        // Try both raw id and entity_id formats
                        let peer_node = node_state.get_node(&adj.peer);
                        if let Some(target) = peer_node
                            && !nodes.iter().any(|n| n.id == target.id)
                            && !candidate_nodes.iter().any(|n| n.id == target.id)
                        {
                            nodes.push(target);
                        }
                        edges.push(crate::prime::types::Edge {
                            id: crate::prime::types::EdgeId::new(&adj.edge_id),
                            source: node.id.clone(),
                            target: crate::prime::types::NodeId::new(&adj.peer),
                            relation: adj.relation.clone(),
                            properties: None,
                            weight: adj.weight,
                            deleted: false,
                            created_at: Utc::now(),
                        });
                    }
                }
            }

            // Add the candidate nodes themselves
            nodes.splice(0..0, candidate_nodes);
        }

        Self::finish_l1(query, stats, nodes, edges)
    }

    /// L1 read at `at`. Candidates, peers and edges are folded from the store,
    /// so the nodes carry their values at `at`. The projections hold only
    /// current state and are not consulted.
    async fn context_l1_at(
        &self,
        query: &RecallContextQuery,
        at: AsOf,
    ) -> PrimeResult<RecallContext> {
        let core: &dyn EventStore = self.core.as_deref().ok_or_else(|| {
            PrimeError::ValidationFailed(
                "as_of needs a recall engine built from Prime's deps".to_string(),
            )
        })?;
        let stats = self.build_stats();
        let mut nodes: Vec<Node> = Vec::new();
        let mut edges: Vec<Edge> = Vec::new();

        let mut candidates: Vec<Node> = Vec::new();
        if let Some(ref node_state) = self.node_state {
            for entity_id in node_state.all_entity_ids() {
                if let Some(node) = temporal::node_at(core, &entity_id, at).await? {
                    candidates.push(node);
                }
            }
        }
        if let Some(conv_id) = query.conversation_id.as_deref() {
            candidates.retain(|n| {
                n.properties
                    .get("conversation_id")
                    .and_then(|v| v.as_str())
                    .is_some_and(|id| id == conv_id)
            });
        }
        candidates.sort_by_key(|n| std::cmp::Reverse(n.recency_instant()));
        candidates.truncate(20);

        let live = temporal::live_edges_at(core, at).await?;
        for node in &candidates {
            let entity_id = node_entity_id(&node.node_type, node.id.as_str());
            for edge in live.iter().filter(|e| e.source == entity_id) {
                if let Some(target) = temporal::node_at(core, &edge.target, at).await?
                    && !nodes.iter().any(|n| n.id == target.id)
                    && !candidates.iter().any(|n| n.id == target.id)
                {
                    nodes.push(target);
                }
                edges.push(Edge {
                    id: EdgeId::new(&edge.id),
                    source: node.id.clone(),
                    target: NodeId::new(&edge.target),
                    relation: edge.relation.clone(),
                    properties: None,
                    weight: None,
                    deleted: false,
                    created_at: chrono::Utc::now(),
                });
            }
        }
        nodes.splice(0..0, candidates);

        Ok(Self::finish_l1(query, stats, nodes, edges))
    }

    /// Size the L1 answer and enforce `max_tokens` by dropping the oldest nodes.
    fn finish_l1(
        query: &RecallContextQuery,
        stats: PrimeStats,
        mut nodes: Vec<Node>,
        edges: Vec<Edge>,
    ) -> RecallContext {
        let mut token_count = 0usize;
        // Estimate token count
        let stats_json = serde_json::to_string(&stats).unwrap_or_default();
        token_count += super::types::estimate_tokens(&stats_json);
        for node in &nodes {
            let node_json = serde_json::to_string(&node).unwrap_or_default();
            token_count += super::types::estimate_tokens(&node_json);
        }

        // Enforce max_tokens: drop oldest nodes first
        if let Some(max) = query.max_tokens {
            while token_count > max && nodes.len() > 1 {
                nodes.pop(); // remove oldest (they're sorted recent-first)
                token_count = super::types::estimate_tokens(&stats_json);
                for node in &nodes {
                    let nj = serde_json::to_string(&node).unwrap_or_default();
                    token_count += super::types::estimate_tokens(&nj);
                }
            }
        }

        RecallContext {
            index: String::new(),
            vectors: Vec::new(),
            nodes,
            edges,
            stats: Some(stats),
            tier: ContextTier::L1,
            token_count,
        }
    }

    /// L2: full hybrid recall (existing behavior).
    async fn context_l2(&self, query: &RecallContextQuery) -> RecallContext {
        let mut index_text = String::new();
        let mut token_count = 0usize;

        // Include compressed index if requested
        if query.include_index {
            let idx = self.index().await;
            index_text = idx.markdown;
            token_count += idx.token_count;
        }

        // Truncate if max_tokens is set and exceeded
        if let Some(max) = query.max_tokens
            && token_count > max
        {
            let target_words = max * 10 / 13; // inverse of words * 1.3
            let truncated: String = index_text
                .split_whitespace()
                .take(target_words)
                .collect::<Vec<_>>()
                .join(" ");
            index_text = truncated + "\n...(truncated)";
            token_count = max;
        }

        // Vector + graph arms: the RecallEngine owns the compressed index but
        // not the vector store (that lives on the Prime facade, behind the
        // optional `prime-vectors` feature). Callers that have both — e.g.
        // prime-mcp's `call_context` — fill these arms from `Prime::recall()`,
        // which embeds the query in-process and runs HNSW + graph expansion.
        // The engine itself returns the index here; vectors/nodes stay empty.
        let vectors: Vec<RankedMemory> = Vec::new();
        let nodes: Vec<Node> = Vec::new();

        RecallContext {
            index: index_text,
            vectors,
            nodes,
            edges: Vec::new(),
            stats: None,
            tier: ContextTier::L2,
            token_count,
        }
    }

    /// Build stats from graph_stats projection or fall back to domain counts.
    fn build_stats(&self) -> PrimeStats {
        if let Some(ref gs) = self.graph_stats {
            gs.stats()
        } else {
            // Fallback: derive minimal stats from domain_index
            let domain_counts = self.domain_index.domain_counts();
            let total_nodes: usize = domain_counts.iter().map(|(_, c)| c).sum();
            PrimeStats {
                total_nodes,
                total_edges: 0,
                nodes_by_type: std::collections::HashMap::new(),
                edges_by_relation: std::collections::HashMap::new(),
                deleted_nodes: 0,
                deleted_edges: 0,
                event_count: 0,
            }
        }
    }

    /// Get all known domains.
    pub fn domains(&self) -> Vec<String> {
        self.domain_index.domains()
    }

    /// Get cross-domain links.
    pub fn cross_domain_links(
        &self,
    ) -> Vec<crate::prime::projections::cross_domain::CrossDomainLink> {
        self.cross_domain.cross_domain_links()
    }

    /// Force regeneration of the compressed index.
    pub async fn refresh_index(&self) -> CompressedIndex {
        self.compressor.invalidate_cache();
        self.index().await
    }

    /// Return all projections as trait objects for registration with a Prime/Core instance.
    pub fn projections(&self) -> Vec<Arc<dyn Projection>> {
        vec![
            Arc::clone(&self.domain_index) as Arc<dyn Projection>,
            Arc::clone(&self.cross_domain) as Arc<dyn Projection>,
        ]
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::services::projection::Projection,
        domain::entities::Event,
        prime::{projections::GraphStatsProjection, types::event_types},
    };
    use uuid::Uuid;

    // Canonical add_node/add_edge shape: node id under `id`, domain under
    // `properties.domain`; edges carry `id` and reference nodes by entity_id
    // (node_domains/domain_index key on these).
    fn make_node(node_id: &str, node_type: &str, domain: &str, name: &str) -> Event {
        Event::reconstruct_from_strings(
            Uuid::new_v4(),
            event_types::NODE_CREATED.to_string(),
            format!("node:{node_type}:{node_id}"),
            "default".to_string(),
            serde_json::json!({
                "id": node_id,
                "node_type": node_type,
                "properties": {"name": name, "domain": domain}
            }),
            Utc::now(),
            None,
            1,
        )
    }

    fn make_edge(edge_id: &str, source: &str, target: &str, relation: &str) -> Event {
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

    fn seed_engine() -> RecallEngine {
        let engine = RecallEngine::new(&IndexConfig::default());

        let events = vec![
            make_node("n1", "metric", "revenue", "Q3 Revenue"),
            make_node("n2", "metric", "revenue", "Churn Rate"),
            make_node("n3", "service", "engineering", "Core API"),
            make_node("n4", "feature", "product", "Dark Mode"),
            // source/target are node entity_ids (matching add_edge)
            make_edge("e1", "node:metric:n1", "node:service:n3", "impacts"),
            make_edge("e2", "node:feature:n4", "node:service:n3", "depends_on"),
        ];

        // Process events through the Arc'd projections
        let projections = engine.projections();
        for event in &events {
            for proj in &projections {
                proj.process(event).unwrap();
            }
        }

        engine
    }

    #[tokio::test]
    async fn test_index_returns_compressed_index() {
        let engine = seed_engine();
        let index = engine.index().await;

        assert!(index.markdown.contains("revenue"));
        assert!(index.markdown.contains("engineering"));
        assert!(index.token_count > 0);
        assert!(!index.domains.is_empty());
    }

    #[tokio::test]
    async fn test_context_with_include_index() {
        let engine = seed_engine();
        let query = RecallContextQuery {
            query: "How does revenue relate to engineering?".to_string(),
            include_index: true,
            ..Default::default()
        };

        let ctx = engine.context(query).await.unwrap();

        assert!(!ctx.index.is_empty());
        assert!(ctx.token_count > 0);
        assert_eq!(ctx.tier, ContextTier::L2);
    }

    #[tokio::test]
    async fn test_context_without_index() {
        let engine = seed_engine();
        let query = RecallContextQuery {
            query: "test".to_string(),
            include_index: false,
            ..Default::default()
        };

        let ctx = engine.context(query).await.unwrap();

        assert!(ctx.index.is_empty());
        assert_eq!(ctx.token_count, 0);
        assert_eq!(ctx.tier, ContextTier::L2);
    }

    #[tokio::test]
    async fn test_context_with_max_tokens_truncates() {
        let engine = seed_engine();
        let query = RecallContextQuery {
            query: "test".to_string(),
            include_index: true,
            max_tokens: Some(10),
            ..Default::default()
        };

        let ctx = engine.context(query).await.unwrap();

        assert!(ctx.token_count <= 10);
        assert!(ctx.index.contains("truncated"));
    }

    // ─── Tier-specific tests ────────────────────────────────────────

    #[tokio::test]
    async fn test_l0_returns_stats_only() {
        let engine = seed_engine();
        let query = RecallContextQuery {
            query: String::new(),
            tier: ContextTier::L0,
            ..Default::default()
        };

        let ctx = engine.context(query).await.unwrap();

        assert_eq!(ctx.tier, ContextTier::L0);
        assert!(ctx.index.is_empty(), "L0 should not generate index");
        assert!(ctx.vectors.is_empty(), "L0 should not run vector search");
        assert!(ctx.nodes.is_empty(), "L0 should not return graph nodes");
        assert!(ctx.stats.is_some(), "L0 should return stats");
        assert!(ctx.token_count > 0, "L0 should have non-zero token count");
        assert!(ctx.token_count < 300, "L0 should be under 300 tokens");
    }

    #[tokio::test]
    async fn test_l1_returns_recent_nodes() {
        // Build engine with full deps for L1 support
        let node_state = Arc::new(NodeStateProjection::new("prime.node_state"));
        let adjacency = Arc::new(AdjacencyListProjection::forward("prime.adjacency"));
        let graph_stats = Arc::new(GraphStatsProjection::new("prime.graph_stats"));
        let domain_index = Arc::new(DomainIndexProjection::new());
        let cross_domain = Arc::new(CrossDomainProjection::new());
        let compressor = IndexCompressor::new(None, 100, 300);

        let engine = RecallEngine {
            domain_index: domain_index.clone(),
            cross_domain: cross_domain.clone(),
            node_state: Some(node_state.clone()),
            adjacency: Some(adjacency.clone()),
            graph_stats: Some(graph_stats.clone()),
            core: None,
            compressor,
        };

        // Seed data through all projections
        let events = vec![
            make_node("n1", "metric", "revenue", "Q3 Revenue"),
            make_node("n2", "metric", "revenue", "Churn Rate"),
            make_node("n3", "service", "engineering", "Core API"),
        ];
        let all_projections: Vec<Arc<dyn Projection>> = vec![
            node_state.clone(),
            adjacency.clone(),
            graph_stats.clone(),
            domain_index.clone(),
            cross_domain.clone(),
        ];
        for event in &events {
            for proj in &all_projections {
                proj.process(event).unwrap();
            }
        }

        let query = RecallContextQuery {
            query: "test".to_string(),
            tier: ContextTier::L1,
            ..Default::default()
        };

        let ctx = engine.context(query).await.unwrap();

        assert_eq!(ctx.tier, ContextTier::L1);
        assert!(ctx.index.is_empty(), "L1 should not generate index");
        assert!(ctx.vectors.is_empty(), "L1 should not run vector search");
        assert!(ctx.stats.is_some(), "L1 should return stats");
        assert!(!ctx.nodes.is_empty(), "L1 should return recent nodes");
    }

    #[tokio::test]
    async fn test_l0_cheaper_than_l2() {
        let engine = seed_engine();

        let l0_ctx = engine
            .context(RecallContextQuery {
                tier: ContextTier::L0,
                ..Default::default()
            })
            .await
            .unwrap();
        let l2_ctx = engine
            .context(RecallContextQuery {
                include_index: true,
                ..Default::default()
            })
            .await
            .unwrap();

        assert!(
            l0_ctx.token_count < l2_ctx.token_count,
            "L0 ({}) should cost fewer tokens than L2 ({})",
            l0_ctx.token_count,
            l2_ctx.token_count
        );
    }

    #[tokio::test]
    async fn test_refresh_index_regenerates() {
        let engine = seed_engine();

        let idx1 = engine.index().await;
        let idx2 = engine.index().await;
        // Should be cached (same event count)
        assert_eq!(idx1.markdown, idx2.markdown);

        // Force refresh
        let idx3 = engine.refresh_index().await;
        // Should have been regenerated (content may be same but last_updated differs)
        assert!(idx3.last_updated >= idx1.last_updated);
    }

    #[tokio::test]
    async fn test_domains_returns_known_domains() {
        let engine = seed_engine();
        let domains = engine.domains();

        assert!(domains.contains(&"revenue".to_string()));
        assert!(domains.contains(&"engineering".to_string()));
        assert!(domains.contains(&"product".to_string()));
    }

    #[tokio::test]
    async fn test_cross_domain_links_detected() {
        let engine = seed_engine();
        let links = engine.cross_domain_links();

        // revenue->engineering and product->engineering
        assert_eq!(links.len(), 2);
    }

    #[tokio::test]
    async fn test_projections_returns_arc_projections() {
        let engine = RecallEngine::new(&IndexConfig::default());
        let projections = engine.projections();

        assert_eq!(projections.len(), 2);
        // Verify they implement Projection (names should be set)
        let names: Vec<&str> = projections.iter().map(|p| p.name()).collect();
        assert!(names.contains(&"prime.domain_index"));
        assert!(names.contains(&"prime.cross_domain"));
    }

    #[tokio::test]
    async fn test_with_dependencies_constructor() {
        let domain_index = Arc::new(DomainIndexProjection::new());
        let cross_domain = Arc::new(CrossDomainProjection::new());
        let compressor = IndexCompressor::new(None, 100, 300);

        let engine =
            RecallEngine::with_dependencies(domain_index.clone(), cross_domain.clone(), compressor);

        // Process an event through the shared Arc
        let event = make_node("n1", "metric", "revenue", "Q3 Revenue");
        domain_index.process(&event).unwrap();
        cross_domain.process(&event).unwrap();

        let domains = engine.domains();
        assert!(domains.contains(&"revenue".to_string()));
    }

    #[tokio::test]
    async fn test_with_dependencies_custom_llm_backend() {
        use std::{future::Future, pin::Pin};

        struct MockBackend;

        impl LlmBackend for MockBackend {
            fn generate(
                &self,
                _prompt: &str,
            ) -> Pin<Box<dyn Future<Output = std::result::Result<String, String>> + Send + '_>>
            {
                Box::pin(async { Ok("# Mock Index".to_string()) })
            }
        }

        let domain_index = Arc::new(DomainIndexProjection::new());
        let cross_domain = Arc::new(CrossDomainProjection::new());

        // Inject a custom LLM backend via the compressor
        let compressor = IndexCompressor::new(Some(Box::new(MockBackend)), 100, 300);
        let engine =
            RecallEngine::with_dependencies(domain_index.clone(), cross_domain.clone(), compressor);

        // Seed some data
        let event = make_node("n1", "metric", "revenue", "Q3 Revenue");
        domain_index.process(&event).unwrap();
        cross_domain.process(&event).unwrap();

        let index = engine.index().await;
        assert_eq!(index.markdown, "# Mock Index");
    }
}
