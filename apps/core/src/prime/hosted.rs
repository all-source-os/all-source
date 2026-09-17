//! `HostedPrime` — a stateless, tenant-scoped Prime engine over a remote Core.
//!
//! This is the composition the hosted `allsource-prime` app uses instead of the
//! embedded [`Prime`](super::facade::Prime): it owns no durable store. Reads
//! resolve a tenant's warm [`GraphProjections`] from the
//! [`TenantProjectionCache`] (hydrated on demand from the remote Core's
//! `prime.*` events); writes ingest an event to the remote Core via an
//! [`HttpCore`] scoped to the tenant and then update the warm bundle so reads
//! stay current.
//!
//! Tenant identity is always an explicit argument supplied by the trusted
//! caller (the gateway), never inferred — so one `HostedPrime` serves every
//! tenant with strict isolation (each tenant's events are queried and folded
//! separately; see the cross-tenant test in [`super::tenant_cache`]).
//!
//! Deliberately a distinct type from the embedded `Prime` (not a new variant of
//! it) so the embedded local-first path and `facade.rs` are untouched. The
//! MCP/HTTP surface is adapted onto this in a later slice.
//!
//! Feature-gated behind `prime-recall`. See bead t-10f876 /
//! `docs/proposals/PRIME_STATELESS_OVER_CORE.md`.

use std::{sync::Arc, time::Duration};

use serde_json::json;

use crate::{
    embedded::{EventView, IngestEvent, Query},
    error::Result,
    prime::{
        event_store::EventStore,
        http_core::HttpCore,
        projection_bundle::GraphProjections,
        tenant_cache::TenantProjectionCache,
        types::{
            Direction, EdgeId, Node, NodeId, PrimeStats, edge_entity_id, event_types,
            node_entity_id,
        },
    },
};

/// A stateless Prime engine backed by a remote Core, serving many tenants.
pub struct HostedPrime {
    base_url: String,
    api_key: Option<String>,
    cache: TenantProjectionCache,
    /// Lazily-initialized in-process text embedder, shared across all tenants
    /// (the model is stateless — only the per-tenant vectors are isolated).
    /// Mirrors the embedded `Prime::embedder` OnceLock (facade.rs).
    #[cfg(feature = "prime-vectors")]
    embedder: std::sync::OnceLock<Arc<crate::prime::vectors::TextEmbedder>>,
}

impl HostedPrime {
    /// Connect to the Core at `base_url`, keeping up to `capacity` tenants warm,
    /// re-hydrating bundles older than `ttl`.
    pub fn connect(
        base_url: impl Into<String>,
        api_key: Option<String>,
        capacity: usize,
        ttl: Duration,
    ) -> Self {
        let base_url = base_url.into();
        let cache = TenantProjectionCache::new(base_url.clone(), api_key.clone(), capacity, ttl);
        Self {
            base_url,
            api_key,
            cache,
            #[cfg(feature = "prime-vectors")]
            embedder: std::sync::OnceLock::new(),
        }
    }

    /// An [`HttpCore`] scoped to `tenant`, for writes.
    fn core_for(&self, tenant: &str) -> HttpCore {
        HttpCore::new(
            self.base_url.clone(),
            self.api_key.clone(),
            Some(tenant.to_string()),
        )
    }

    // ── Reads ────────────────────────────────────────────────────────────

    /// The tenant's warm graph-projection bundle (hydrated on demand).
    pub async fn tenant_graph(&self, tenant: &str) -> Result<Arc<GraphProjections>> {
        self.cache.get_or_hydrate(tenant).await
    }

    /// Get a node by entity_id within a tenant's graph. `None` if absent or
    /// soft-deleted.
    pub async fn get_node(&self, tenant: &str, entity_id: &str) -> Result<Option<Node>> {
        let graph = self.cache.get_or_hydrate(tenant).await?;
        Ok(graph.node_state.get_node(entity_id).filter(|n| !n.deleted))
    }

    // ── Writes ───────────────────────────────────────────────────────────

    /// Create a node: ingest a `prime.node.created` event to the remote Core
    /// (tenant-stamped) and apply it to the tenant's warm bundle so subsequent
    /// reads reflect it without a re-query.
    pub async fn add_node(
        &self,
        tenant: &str,
        node_type: &str,
        properties: serde_json::Value,
    ) -> Result<NodeId> {
        let id = uuid::Uuid::new_v4().to_string();
        let entity_id = node_entity_id(node_type, &id);
        let payload = json!({
            "id": id,
            "node_type": node_type,
            "properties": properties,
        });

        self.core_for(tenant)
            .ingest(IngestEvent {
                entity_id: &entity_id,
                event_type: event_types::NODE_CREATED,
                payload: payload.clone(),
                metadata: None,
                tenant_id: Some(tenant),
            })
            .await?;

        // Update the tenant's warm bundle in place IF it is cached, so an
        // in-process read reflects the write without a re-query. If the tenant
        // is cold, cache.apply no-ops and the next read hydrates from Core
        // (which now has this event). We deliberately do NOT hydrate-then-apply:
        // that would fold the just-ingested event in twice, inflating
        // count-based projections like graph_stats.
        self.cache.apply(
            tenant,
            &self.synth_view_typed(tenant, event_types::NODE_CREATED, &entity_id, payload),
        );

        Ok(NodeId::new(id))
    }

    /// Create a directed edge: ingest a `prime.edge.created` event (tenant-stamped)
    /// and apply it to the tenant's warm bundle so `neighbors` reflects it without
    /// a re-query. Mirrors the embedded `Prime::add_edge_inner` payload exactly.
    pub async fn add_edge(
        &self,
        tenant: &str,
        source: &str,
        target: &str,
        relation: &str,
        properties: Option<serde_json::Value>,
    ) -> Result<EdgeId> {
        self.add_edge_inner(tenant, source, target, relation, None, properties)
            .await
    }

    /// Create a weighted directed edge. See [`add_edge`](Self::add_edge).
    pub async fn add_edge_weighted(
        &self,
        tenant: &str,
        source: &str,
        target: &str,
        relation: &str,
        weight: f64,
        properties: Option<serde_json::Value>,
    ) -> Result<EdgeId> {
        self.add_edge_inner(tenant, source, target, relation, Some(weight), properties)
            .await
    }

    async fn add_edge_inner(
        &self,
        tenant: &str,
        source: &str,
        target: &str,
        relation: &str,
        weight: Option<f64>,
        properties: Option<serde_json::Value>,
    ) -> Result<EdgeId> {
        let id = uuid::Uuid::new_v4().to_string();
        let entity_id = edge_entity_id(&id);

        let mut payload = json!({
            "id": id,
            "source": source,
            "target": target,
            "relation": relation,
        });
        if let Some(w) = weight {
            payload["weight"] = json!(w);
        }
        if let Some(props) = properties {
            payload["properties"] = props;
        }

        self.core_for(tenant)
            .ingest(IngestEvent {
                entity_id: &entity_id,
                event_type: event_types::EDGE_CREATED,
                payload: payload.clone(),
                metadata: None,
                tenant_id: Some(tenant),
            })
            .await?;

        self.cache.apply(
            tenant,
            &self.synth_view_typed(tenant, event_types::EDGE_CREATED, &entity_id, payload),
        );

        Ok(EdgeId::new(id))
    }

    /// Soft-delete a node: ingest a `prime.node.deleted` event (tenant-stamped)
    /// and apply it to the tenant's warm bundle so `get_node` returns `None`
    /// without a re-query. Mirrors the embedded `Prime::delete_node` node-delete
    /// event shape (connected edges are reconciled on the next hydrate).
    pub async fn delete_node(&self, tenant: &str, entity_id: &str) -> Result<()> {
        self.core_for(tenant)
            .ingest(IngestEvent {
                entity_id,
                event_type: event_types::NODE_DELETED,
                payload: json!({}),
                metadata: None,
                tenant_id: Some(tenant),
            })
            .await?;

        self.cache.apply(
            tenant,
            &self.synth_view_typed(tenant, event_types::NODE_DELETED, entity_id, json!({})),
        );

        Ok(())
    }

    /// Get 1-hop neighbors of a node, optionally filtered by relation and
    /// direction. Returns full [`Node`]s with deleted nodes excluded. Mirrors
    /// the embedded `Prime::neighbors` traversal over `adjacency`/`reverse_index`.
    pub async fn neighbors(
        &self,
        tenant: &str,
        entity_id: &str,
        relation: Option<&str>,
        direction: Direction,
    ) -> Result<Vec<Node>> {
        let g = self.cache.get_or_hydrate(tenant).await?;

        let mut peer_ids: Vec<String> = Vec::new();
        let matches = |entry_relation: &str| relation.is_none_or(|r| r == entry_relation);

        match direction {
            Direction::Outgoing => {
                for entry in g.adjacency.outgoing(entity_id) {
                    if matches(&entry.relation) {
                        peer_ids.push(entry.peer.clone());
                    }
                }
            }
            Direction::Incoming => {
                for entry in g.reverse_index.incoming(entity_id) {
                    if matches(&entry.relation) {
                        peer_ids.push(entry.peer.clone());
                    }
                }
            }
            Direction::Both => {
                let mut seen = std::collections::HashSet::new();
                for entry in g.adjacency.outgoing(entity_id) {
                    if matches(&entry.relation) && seen.insert(entry.peer.clone()) {
                        peer_ids.push(entry.peer.clone());
                    }
                }
                for entry in g.reverse_index.incoming(entity_id) {
                    if matches(&entry.relation) && seen.insert(entry.peer.clone()) {
                        peer_ids.push(entry.peer.clone());
                    }
                }
            }
        }

        Ok(peer_ids
            .iter()
            .filter_map(|id| g.node_state.get_node(id).filter(|n| !n.deleted))
            .collect())
    }

    /// All live nodes of a given type. Mirrors the embedded `Prime::nodes_by_type`
    /// (reads `node_type_index` + `node_state`).
    pub async fn search(&self, tenant: &str, node_type: &str) -> Result<Vec<Node>> {
        let g = self.cache.get_or_hydrate(tenant).await?;
        Ok(g.node_type_index
            .nodes_by_type(node_type)
            .iter()
            .filter_map(|entity_id| g.node_state.get_node(entity_id).filter(|n| !n.deleted))
            .collect())
    }

    /// Statistics about the tenant's graph (O(1) via the `graph_stats` projection).
    pub async fn stats(&self, tenant: &str) -> Result<PrimeStats> {
        let g = self.cache.get_or_hydrate(tenant).await?;
        Ok(g.graph_stats.stats())
    }

    /// The full audit trail for an entity, read straight from the remote Core.
    /// Returns raw events (no projection); empty if the entity never existed.
    pub async fn history(&self, tenant: &str, entity_id: &str) -> Result<Vec<EventView>> {
        self.core_for(tenant)
            .query(Query::new().entity_id(entity_id))
            .await
    }

    /// Update a node's properties (deep-merged by the projection): ingest a
    /// `prime.node.updated` event (tenant-stamped) and apply it to the warm
    /// bundle. Mirrors the embedded `Prime::update_node` payload — the raw update
    /// `{properties}` is emitted; node_state merges on `process`. (Schema
    /// validation, which the embedded path does pre-ingest, is deferred here.)
    pub async fn update_node(
        &self,
        tenant: &str,
        entity_id: &str,
        properties: serde_json::Value,
    ) -> Result<()> {
        let payload = json!({ "properties": properties });
        self.core_for(tenant)
            .ingest(IngestEvent {
                entity_id,
                event_type: event_types::NODE_UPDATED,
                payload: payload.clone(),
                metadata: None,
                tenant_id: Some(tenant),
            })
            .await?;
        self.cache.get_or_hydrate(tenant).await?;
        self.cache.apply(
            tenant,
            &self.synth_view_typed(tenant, event_types::NODE_UPDATED, entity_id, payload),
        );
        Ok(())
    }

    /// Delete an edge: ingest a `prime.edge.deleted` event (tenant-stamped) and
    /// apply it so `neighbors` stops returning it without a re-query. Mirrors the
    /// embedded `Prime::delete_edge` shape (`edge:{id}` entity, payload `{id}`).
    pub async fn delete_edge(&self, tenant: &str, edge_id: &str) -> Result<()> {
        let entity_id = edge_entity_id(edge_id);
        let payload = json!({ "id": edge_id });
        self.core_for(tenant)
            .ingest(IngestEvent {
                entity_id: &entity_id,
                event_type: event_types::EDGE_DELETED,
                payload: payload.clone(),
                metadata: None,
                tenant_id: Some(tenant),
            })
            .await?;
        self.cache.get_or_hydrate(tenant).await?;
        self.cache.apply(
            tenant,
            &self.synth_view_typed(tenant, event_types::EDGE_DELETED, &entity_id, payload),
        );
        Ok(())
    }

    /// Shortest path between two nodes (BFS over outgoing edges, optional relation
    /// filter). Mirrors the embedded `Prime::shortest_path`, but runs the BFS over
    /// the tenant's warm bundle (resolved once) instead of the embedded store.
    pub async fn shortest_path(
        &self,
        tenant: &str,
        from: &str,
        to: &str,
        relation: Option<&str>,
    ) -> Result<Option<Vec<Node>>> {
        use std::collections::{HashMap, VecDeque};

        let g = self.cache.get_or_hydrate(tenant).await?;
        let get_node = |id: &str| g.node_state.get_node(id).filter(|n| !n.deleted);

        if from == to {
            return Ok(get_node(from).map(|n| vec![n]));
        }

        let mut visited: HashMap<String, String> = HashMap::new(); // child -> parent
        let mut queue: VecDeque<String> = VecDeque::new();
        visited.insert(from.to_string(), String::new());
        queue.push_back(from.to_string());

        while let Some(current) = queue.pop_front() {
            for entry in g.adjacency.outgoing(&current) {
                if relation.is_some() && relation != Some(entry.relation.as_str()) {
                    continue;
                }
                let peer = entry.peer;
                if visited.contains_key(&peer) {
                    continue;
                }
                visited.insert(peer.clone(), current.clone());
                if peer == to {
                    let mut path_ids = vec![to.to_string()];
                    let mut cursor = to.to_string();
                    while let Some(parent) = visited.get(&cursor) {
                        if parent.is_empty() {
                            break;
                        }
                        path_ids.push(parent.clone());
                        cursor = parent.clone();
                    }
                    path_ids.reverse();
                    return Ok(Some(
                        path_ids.iter().filter_map(|id| get_node(id)).collect(),
                    ));
                }
                queue.push_back(peer);
            }
        }
        Ok(None)
    }

    // ── Vectors & Recall ─────────────────────────────────────────────────

    /// Store a vector embedding for a graph node: ingest a `prime.vector.stored`
    /// event (tenant-stamped) and apply it to the tenant's warm bundle so recall
    /// reflects it without a re-query.
    ///
    /// `node_entity_id` is the graph entity id (e.g. `node:contact:abc`). The
    /// vector is stored under `vec:{node_entity_id}` so [`recall`](Self::recall)
    /// can strip the `vec:` prefix and hydrate the matched node from `node_state`.
    /// Mirrors the embedded `Prime::embed_with_metadata` event shape exactly: the
    /// vector lives in event metadata under `embedding`; text/dimensions/metadata
    /// in the payload.
    #[cfg(feature = "prime-vectors")]
    pub async fn embed(
        &self,
        tenant: &str,
        node_entity_id: &str,
        vector: Vec<f32>,
        metadata: Option<serde_json::Value>,
    ) -> Result<()> {
        use crate::prime::vectors::{event_types as vec_events, vector_entity_id};

        let entity_id = vector_entity_id(node_entity_id);
        let dimensions = vector.len();

        // Dimension guard: a store holds one dimension. Reject a mismatch before
        // it corrupts the HNSW index (mirrors facade's embed_with_metadata).
        let graph = self.cache.get_or_hydrate(tenant).await?;
        if let Some(established) = graph.vector_index.dimension()
            && established != dimensions
        {
            return Err(crate::error::AllSourceError::ValidationError(format!(
                "embedding dimension mismatch for `{node_entity_id}`: this tenant already \
                 holds {established}-dim vectors, but got a {dimensions}-dim vector. \
                 Mixing dimensions corrupts similarity search."
            )));
        }

        let payload = json!({
            "text": serde_json::Value::Null,
            "dimensions": dimensions,
            "metadata": metadata,
        });

        self.core_for(tenant)
            .ingest(IngestEvent {
                entity_id: &entity_id,
                event_type: vec_events::VECTOR_STORED,
                payload: payload.clone(),
                metadata: Some(json!({ "embedding": vector })),
                tenant_id: Some(tenant),
            })
            .await?;

        // Fold into the warm bundle. The vector index reads the embedding from
        // event metadata, so the synthesized view must carry it there too.
        self.cache.apply(
            tenant,
            &self.synth_vector_view(tenant, &entity_id, payload, &vector),
        );

        Ok(())
    }

    /// Lazily-initialized in-process text embedder (mirrors facade `embedder()`).
    /// First call downloads the model — never exercise this in tests.
    #[cfg(feature = "prime-vectors")]
    fn embedder(&self) -> Result<&Arc<crate::prime::vectors::TextEmbedder>> {
        if let Some(e) = self.embedder.get() {
            return Ok(e);
        }
        let new = Arc::new(
            crate::prime::vectors::TextEmbedder::new()
                .map_err(|e| crate::error::AllSourceError::InternalError(e.to_string()))?,
        );
        let _ = self.embedder.set(new);
        Ok(self
            .embedder
            .get()
            .expect("embedder was just set or already initialized"))
    }

    /// Embed `text` with the in-process model, then store the resulting vector
    /// via [`embed`](Self::embed). Convenience for callers that only have text.
    /// First use downloads the embedding model.
    #[cfg(feature = "prime-vectors")]
    pub async fn embed_text(&self, tenant: &str, node_entity_id: &str, text: &str) -> Result<()> {
        let vector = self
            .embedder()?
            .embed(text)
            .map_err(|e| crate::error::AllSourceError::InternalError(e.to_string()))?;
        self.embed(tenant, node_entity_id, vector, None).await
    }

    /// Semantic recall over the tenant's warm bundle: run a vector similarity
    /// search and hydrate the matched graph nodes from `node_state`.
    ///
    /// Returns `(Node, similarity)` pairs ordered by descending similarity,
    /// `similarity = 1.0 - cosine_distance` (matching facade's `vector_search`).
    /// Implemented as direct `vector_index.search` + node hydration rather than
    /// `RecallEngine::with_deps`: the embedded `Prime::recall` itself does not use
    /// the engine — it composes `vector_search` + `get_node` + graph expansion
    /// against its own projections — so the engine path would not mirror facade,
    /// while this path replicates facade's vector→node hydration exactly.
    #[cfg(feature = "prime-recall")]
    #[cfg(feature = "prime-vectors")]
    // Signature (owned `Vec<f32>` query) is fixed by the bead spec to mirror the
    // facade's recall entry point; the index search borrows it.
    #[allow(clippy::needless_pass_by_value)]
    pub async fn recall(
        &self,
        tenant: &str,
        query_vector: Vec<f32>,
        top_k: usize,
    ) -> Result<Vec<(Node, f64)>> {
        let g = self.cache.get_or_hydrate(tenant).await?;

        let hits = g.vector_index.search(&query_vector, top_k);
        let mut out = Vec::with_capacity(hits.len());
        for hit in hits {
            // Vector entity_id is `vec:{graph_entity_id}` — strip the prefix to
            // recover the node's entity id (mirrors facade recall).
            let graph_id = hit.entity_id.strip_prefix("vec:").unwrap_or(&hit.entity_id);
            if let Some(node) = g.node_state.get_node(graph_id).filter(|n| !n.deleted) {
                let similarity = 1.0 - f64::from(hit.distance);
                out.push((node, similarity));
            }
        }
        Ok(out)
    }

    /// Synthesize a local [`EventView`] for a vector-stored event, carrying the
    /// embedding in metadata so the warm bundle's `VectorIndexProjection` folds
    /// it (it reads the vector from `metadata.embedding`, not the payload).
    #[cfg(feature = "prime-vectors")]
    fn synth_vector_view(
        &self,
        tenant: &str,
        entity_id: &str,
        payload: serde_json::Value,
        vector: &[f32],
    ) -> EventView {
        EventView {
            id: uuid::Uuid::new_v4(),
            event_type: crate::prime::vectors::event_types::VECTOR_STORED.to_string(),
            entity_id: entity_id.to_string(),
            tenant_id: tenant.to_string(),
            payload,
            metadata: Some(json!({ "embedding": vector })),
            timestamp: chrono::Utc::now(),
            version: 0,
        }
    }

    /// Delete a stored vector: ingest a `prime.vector.deleted` event
    /// (tenant-stamped) and apply it so recall no longer returns it. Mirrors the
    /// embedded `Prime::delete_vector` (`vec:{node_entity_id}` entity, `{}` payload).
    #[cfg(feature = "prime-vectors")]
    pub async fn delete_vector(&self, tenant: &str, node_entity_id: &str) -> Result<()> {
        let entity_id = crate::prime::vectors::vector_entity_id(node_entity_id);
        self.core_for(tenant)
            .ingest(IngestEvent {
                entity_id: &entity_id,
                event_type: crate::prime::vectors::event_types::VECTOR_DELETED,
                payload: json!({}),
                metadata: None,
                tenant_id: Some(tenant),
            })
            .await?;
        self.cache.apply(
            tenant,
            &self.synth_view_typed(
                tenant,
                crate::prime::vectors::event_types::VECTOR_DELETED,
                &entity_id,
                json!({}),
            ),
        );
        Ok(())
    }

    /// Materialize the tenant's complete graph (nodes + edges + stats), mirroring
    /// the embedded `Prime::full_graph`. The bundle is already single-tenant
    /// (events were queried tenant-scoped), so no per-node `tenant_id` filter is
    /// needed here. `node_type` filters to one type; `limit` caps nodes and sets
    /// `has_more`; edges are restricted to the returned node set.
    pub async fn full_graph(
        &self,
        tenant: &str,
        node_type: Option<&str>,
        limit: Option<usize>,
    ) -> Result<crate::prime::types::FullGraph> {
        use crate::prime::types::{EntityId, FullGraph, GraphEdge, GraphNode, GraphStats};
        use std::collections::{BTreeMap, HashSet};

        let g = self.cache.get_or_hydrate(tenant).await?;

        let mut live: Vec<Node> = g
            .node_state
            .all_nodes()
            .into_iter()
            .filter(|n| node_type.is_none_or(|nt| n.node_type == nt))
            .collect();
        live.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));

        let mut nodes_by_type: BTreeMap<String, usize> = BTreeMap::new();
        for n in &live {
            *nodes_by_type.entry(n.node_type.clone()).or_default() += 1;
        }
        let has_more = limit.is_some_and(|l| live.len() > l);
        if let Some(l) = limit {
            live.truncate(l);
        }

        let included: HashSet<String> = live
            .iter()
            .map(|n| EntityId::node(&n.node_type, n.id.as_str()).to_wire())
            .collect();

        let mut vector_count = 0usize;
        let nodes: Vec<GraphNode> = live
            .iter()
            .map(|n| {
                let wire = EntityId::node(&n.node_type, n.id.as_str()).to_wire();
                let (has_vector, vector_dim) = self.vector_presence(&g, &wire);
                if has_vector {
                    vector_count += 1;
                }
                GraphNode {
                    id: wire,
                    node_type: n.node_type.clone(),
                    properties: n.properties.clone(),
                    has_vector,
                    vector_dim,
                    created_at: n.created_at,
                    updated_at: n.updated_at,
                }
            })
            .collect();

        let edges: Vec<GraphEdge> = g
            .adjacency
            .all_edges()
            .into_iter()
            .filter(|(source, adj)| included.contains(source) && included.contains(&adj.peer))
            .map(|(source, adj)| GraphEdge {
                source,
                target: adj.peer,
                relation: adj.relation,
                properties: None,
                weight: adj.weight,
                created_at: chrono::Utc::now(),
            })
            .collect();

        let stats = GraphStats {
            node_count: nodes.len(),
            edge_count: edges.len(),
            vector_count,
            nodes_by_type,
        };
        Ok(FullGraph {
            nodes,
            edges,
            stats,
            has_more,
        })
    }

    /// `(has_vector, dimension)` for a node wire-id over the warm bundle's vector
    /// index. Always `(false, None)` without the `prime-vectors` feature.
    #[allow(unused_variables)]
    fn vector_presence(&self, g: &GraphProjections, node_wire_id: &str) -> (bool, Option<usize>) {
        #[cfg(feature = "prime-vectors")]
        {
            use crate::application::services::projection::Projection;
            let vec_entity = crate::prime::vectors::vector_entity_id(node_wire_id);
            if let Some(state) = g.vector_index.get_state(&vec_entity) {
                let dim = state
                    .get("vector")
                    .and_then(serde_json::Value::as_array)
                    .map(Vec::len)
                    .or_else(|| {
                        state
                            .get("dimensions")
                            .and_then(serde_json::Value::as_u64)
                            .map(|d| d as usize)
                    });
                return (true, dim);
            }
        }
        (false, None)
    }

    /// Synthesize a local [`EventView`] for cache application. The remote Core
    /// assigns the authoritative id/version/timestamp; the projections only key
    /// on entity_id/event_type/payload, so a local stand-in is sufficient until
    /// the next hydrate reconciles from Core.
    fn synth_view_typed(
        &self,
        tenant: &str,
        event_type: &str,
        entity_id: &str,
        payload: serde_json::Value,
    ) -> EventView {
        EventView {
            id: uuid::Uuid::new_v4(),
            event_type: event_type.to_string(),
            entity_id: entity_id.to_string(),
            tenant_id: tenant.to_string(),
            payload,
            metadata: None,
            timestamp: chrono::Utc::now(),
            version: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    #[tokio::test]
    async fn add_node_then_get_node_round_trips_through_warm_cache() {
        let server = MockServer::start().await;
        // Core has no prior events for this tenant…
        Mock::given(method("GET"))
            .and(path("/api/v1/events/query"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"events": []})),
            )
            .mount(&server)
            .await;
        // …and accepts the ingest.
        Mock::given(method("POST"))
            .and(path("/api/v1/events"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
            .mount(&server)
            .await;

        let hosted = HostedPrime::connect(server.uri(), None, 8, Duration::from_mins(1));
        // Pre-warm the tenant so post-write apply() lands on a warm bundle
        // (cold writes no-op apply; reads then hydrate from Core). See bug t-d90426.
        hosted.tenant_graph("tenant-a").await.unwrap();
        let id = hosted
            .add_node("tenant-a", "contact", serde_json::json!({"name": "Alice"}))
            .await
            .unwrap();

        // The node is visible from the warm bundle even though Core's query
        // returned empty — proving the write updated the cache, not just Core.
        let entity_id = node_entity_id("contact", &id.0);
        let node = hosted.get_node("tenant-a", &entity_id).await.unwrap();
        assert!(node.is_some(), "node should be readable after add_node");
        assert_eq!(node.unwrap().properties["name"], "Alice");
    }

    /// Mount the standard pair: GET query → empty, POST ingest → 200. Proving a
    /// read after a write succeeds therefore proves the warm cache was updated,
    /// not that Core re-served the event.
    async fn mount_empty_core(server: &MockServer) {
        Mock::given(method("GET"))
            .and(path("/api/v1/events/query"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"events": []})),
            )
            .mount(server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/v1/events"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn add_edge_then_neighbors_returns_target() {
        let server = MockServer::start().await;
        mount_empty_core(&server).await;

        let hosted = HostedPrime::connect(server.uri(), None, 8, Duration::from_mins(1));
        // Pre-warm the tenant so post-write apply() lands on a warm bundle
        // (cold writes no-op apply; reads then hydrate from Core). See bug t-d90426.
        hosted.tenant_graph("tenant-a").await.unwrap();
        let alice = hosted
            .add_node("tenant-a", "contact", serde_json::json!({"name": "Alice"}))
            .await
            .unwrap();
        let bob = hosted
            .add_node("tenant-a", "contact", serde_json::json!({"name": "Bob"}))
            .await
            .unwrap();
        let alice_eid = node_entity_id("contact", &alice.0);
        let bob_eid = node_entity_id("contact", &bob.0);

        hosted
            .add_edge("tenant-a", &alice_eid, &bob_eid, "knows", None)
            .await
            .unwrap();

        // Outgoing neighbors of Alice should be Bob, served entirely from the
        // warm cache (Core's query is mocked empty).
        let out = hosted
            .neighbors("tenant-a", &alice_eid, None, Direction::Outgoing)
            .await
            .unwrap();
        assert_eq!(out.len(), 1, "alice should have one outgoing neighbor");
        assert_eq!(out[0].properties["name"], "Bob");

        // And Bob's incoming neighbor is Alice.
        let inc = hosted
            .neighbors("tenant-a", &bob_eid, Some("knows"), Direction::Incoming)
            .await
            .unwrap();
        assert_eq!(inc.len(), 1);
        assert_eq!(inc[0].properties["name"], "Alice");
    }

    #[tokio::test]
    async fn add_nodes_then_search_returns_all_of_type() {
        let server = MockServer::start().await;
        mount_empty_core(&server).await;

        let hosted = HostedPrime::connect(server.uri(), None, 8, Duration::from_mins(1));
        // Pre-warm the tenant so post-write apply() lands on a warm bundle
        // (cold writes no-op apply; reads then hydrate from Core). See bug t-d90426.
        hosted.tenant_graph("tenant-a").await.unwrap();
        hosted
            .add_node("tenant-a", "contact", serde_json::json!({"name": "Alice"}))
            .await
            .unwrap();
        hosted
            .add_node("tenant-a", "contact", serde_json::json!({"name": "Bob"}))
            .await
            .unwrap();
        // A node of a different type must not show up in the search.
        hosted
            .add_node("tenant-a", "company", serde_json::json!({"name": "Acme"}))
            .await
            .unwrap();

        let contacts = hosted.search("tenant-a", "contact").await.unwrap();
        assert_eq!(contacts.len(), 2, "both contacts should be returned");
        let names: std::collections::HashSet<&str> = contacts
            .iter()
            .map(|n| n.properties["name"].as_str().unwrap())
            .collect();
        assert!(names.contains("Alice"));
        assert!(names.contains("Bob"));
    }

    #[tokio::test]
    async fn stats_reflects_added_nodes_and_edges() {
        let server = MockServer::start().await;
        mount_empty_core(&server).await;

        let hosted = HostedPrime::connect(server.uri(), None, 8, Duration::from_mins(1));
        // Pre-warm the tenant so post-write apply() lands on a warm bundle
        // (cold writes no-op apply; reads then hydrate from Core). See bug t-d90426.
        hosted.tenant_graph("tenant-a").await.unwrap();
        let a = hosted
            .add_node("tenant-a", "contact", serde_json::json!({"name": "Alice"}))
            .await
            .unwrap();
        let b = hosted
            .add_node("tenant-a", "contact", serde_json::json!({"name": "Bob"}))
            .await
            .unwrap();
        hosted
            .add_edge(
                "tenant-a",
                &node_entity_id("contact", &a.0),
                &node_entity_id("contact", &b.0),
                "knows",
                None,
            )
            .await
            .unwrap();

        let stats = hosted.stats("tenant-a").await.unwrap();
        assert_eq!(stats.total_nodes, 2);
        assert_eq!(stats.total_edges, 1);
    }

    #[tokio::test]
    async fn delete_node_then_get_node_returns_none() {
        let server = MockServer::start().await;
        mount_empty_core(&server).await;

        let hosted = HostedPrime::connect(server.uri(), None, 8, Duration::from_mins(1));
        // Pre-warm the tenant so post-write apply() lands on a warm bundle
        // (cold writes no-op apply; reads then hydrate from Core). See bug t-d90426.
        hosted.tenant_graph("tenant-a").await.unwrap();
        let id = hosted
            .add_node("tenant-a", "contact", serde_json::json!({"name": "Alice"}))
            .await
            .unwrap();
        let entity_id = node_entity_id("contact", &id.0);

        assert!(
            hosted
                .get_node("tenant-a", &entity_id)
                .await
                .unwrap()
                .is_some()
        );

        hosted.delete_node("tenant-a", &entity_id).await.unwrap();

        // The delete folded into the warm bundle — get_node returns None even
        // though Core's query is mocked empty.
        let node = hosted.get_node("tenant-a", &entity_id).await.unwrap();
        assert!(node.is_none(), "node should be gone after delete_node");
    }

    #[cfg(feature = "prime-vectors")]
    #[tokio::test]
    async fn embed_then_recall_returns_the_embedded_node() {
        let server = MockServer::start().await;
        mount_empty_core(&server).await;

        let hosted = HostedPrime::connect(server.uri(), None, 8, Duration::from_mins(1));
        // Pre-warm the tenant so post-write apply() lands on a warm bundle
        // (cold writes no-op apply; reads then hydrate from Core). See bug t-d90426.
        hosted.tenant_graph("tenant-a").await.unwrap();
        // A real graph node so recall can hydrate it from node_state.
        let id = hosted
            .add_node("tenant-a", "contact", serde_json::json!({"name": "Alice"}))
            .await
            .unwrap();
        let entity_id = node_entity_id("contact", &id.0);

        // Explicit 4-dim vector — never call TextEmbedder in tests.
        hosted
            .embed("tenant-a", &entity_id, vec![1.0, 0.0, 0.0, 0.0], None)
            .await
            .unwrap();

        // A near-identical query vector should return Alice from the warm bundle
        // (Core's query is mocked empty — proving the vector folded into the cache).
        let results = hosted
            .recall("tenant-a", vec![1.0, 0.0, 0.0, 0.0], 5)
            .await
            .unwrap();
        assert_eq!(results.len(), 1, "the embedded node should be recalled");
        assert_eq!(results[0].0.properties["name"], "Alice");
        assert!(
            results[0].1 > 0.99,
            "identical vectors should score ~1.0, got {}",
            results[0].1
        );
    }

    #[cfg(feature = "prime-vectors")]
    #[tokio::test]
    async fn recall_is_isolated_per_tenant() {
        let server = MockServer::start().await;
        mount_empty_core(&server).await;

        let hosted = HostedPrime::connect(server.uri(), None, 8, Duration::from_mins(1));
        // Pre-warm the tenant so post-write apply() lands on a warm bundle
        // (cold writes no-op apply; reads then hydrate from Core). See bug t-d90426.
        hosted.tenant_graph("tenant-a").await.unwrap();

        // tenant-a embeds a node…
        let a_id = hosted
            .add_node("tenant-a", "contact", serde_json::json!({"name": "Alice"}))
            .await
            .unwrap();
        let a_eid = node_entity_id("contact", &a_id.0);
        hosted
            .embed("tenant-a", &a_eid, vec![1.0, 0.0, 0.0, 0.0], None)
            .await
            .unwrap();

        // …and tenant-b recalls with the same vector but has no embeddings.
        let b_results = hosted
            .recall("tenant-b", vec![1.0, 0.0, 0.0, 0.0], 5)
            .await
            .unwrap();
        assert!(
            b_results.is_empty(),
            "tenant-b must not see tenant-a's embedded vector"
        );

        // tenant-a still sees its own.
        let a_results = hosted
            .recall("tenant-a", vec![1.0, 0.0, 0.0, 0.0], 5)
            .await
            .unwrap();
        assert_eq!(a_results.len(), 1);
        assert_eq!(a_results[0].0.properties["name"], "Alice");
    }

    #[tokio::test]
    async fn get_node_absent_returns_none() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/events/query"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"events": []})),
            )
            .mount(&server)
            .await;

        let hosted = HostedPrime::connect(server.uri(), None, 8, Duration::from_mins(1));
        // Pre-warm the tenant so post-write apply() lands on a warm bundle
        // (cold writes no-op apply; reads then hydrate from Core). See bug t-d90426.
        hosted.tenant_graph("t").await.unwrap();
        let node = hosted
            .get_node("tenant-a", "node:contact:ghost")
            .await
            .unwrap();
        assert!(node.is_none());
    }

    /// A wiremock Core that has no prior events and accepts ingests — the
    /// standard fixture proving warm-cache round-trips (Core query is empty, so
    /// any read result came from the cache the write updated).
    async fn empty_core() -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/events/query"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"events": []})),
            )
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/v1/events"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
            .mount(&server)
            .await;
        server
    }

    #[tokio::test]
    async fn update_node_merges_properties_in_warm_bundle() {
        let server = empty_core().await;
        let hosted = HostedPrime::connect(server.uri(), None, 8, Duration::from_mins(1));
        // Pre-warm the tenant so post-write apply() lands on a warm bundle
        // (cold writes no-op apply; reads then hydrate from Core). See bug t-d90426.
        hosted.tenant_graph("t").await.unwrap();
        let id = hosted
            .add_node("t", "contact", serde_json::json!({"name": "Alice"}))
            .await
            .unwrap();
        let eid = node_entity_id("contact", &id.0);
        hosted
            .update_node("t", &eid, serde_json::json!({"role": "eng"}))
            .await
            .unwrap();
        let node = hosted.get_node("t", &eid).await.unwrap().unwrap();
        assert_eq!(node.properties["name"], "Alice"); // preserved (deep merge)
        assert_eq!(node.properties["role"], "eng"); // added
    }

    #[tokio::test]
    async fn delete_edge_removes_it_from_neighbors() {
        let server = empty_core().await;
        let hosted = HostedPrime::connect(server.uri(), None, 8, Duration::from_mins(1));
        // Pre-warm the tenant so post-write apply() lands on a warm bundle
        // (cold writes no-op apply; reads then hydrate from Core). See bug t-d90426.
        hosted.tenant_graph("t").await.unwrap();
        let a = node_entity_id(
            "contact",
            &hosted
                .add_node("t", "contact", serde_json::json!({}))
                .await
                .unwrap()
                .0,
        );
        let b = node_entity_id(
            "contact",
            &hosted
                .add_node("t", "contact", serde_json::json!({}))
                .await
                .unwrap()
                .0,
        );
        let edge = hosted.add_edge("t", &a, &b, "knows", None).await.unwrap();
        assert_eq!(
            hosted
                .neighbors("t", &a, None, Direction::Outgoing)
                .await
                .unwrap()
                .len(),
            1
        );
        hosted.delete_edge("t", &edge.0).await.unwrap();
        assert!(
            hosted
                .neighbors("t", &a, None, Direction::Outgoing)
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn shortest_path_finds_the_chain() {
        let server = empty_core().await;
        let hosted = HostedPrime::connect(server.uri(), None, 8, Duration::from_mins(1));
        // Pre-warm the tenant so post-write apply() lands on a warm bundle
        // (cold writes no-op apply; reads then hydrate from Core). See bug t-d90426.
        hosted.tenant_graph("t").await.unwrap();
        let na = hosted
            .add_node("t", "n", serde_json::json!({}))
            .await
            .unwrap()
            .0;
        let nb = hosted
            .add_node("t", "n", serde_json::json!({}))
            .await
            .unwrap()
            .0;
        let nc = hosted
            .add_node("t", "n", serde_json::json!({}))
            .await
            .unwrap()
            .0;
        let (a, b, c) = (
            node_entity_id("n", &na),
            node_entity_id("n", &nb),
            node_entity_id("n", &nc),
        );
        hosted.add_edge("t", &a, &b, "to", None).await.unwrap();
        hosted.add_edge("t", &b, &c, "to", None).await.unwrap();
        let path = hosted
            .shortest_path("t", &a, &c, None)
            .await
            .unwrap()
            .unwrap();
        // Node.id is the short id; assert the BFS returned the a→b→c chain in order.
        let ids: Vec<&str> = path.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(ids, vec![na.as_str(), nb.as_str(), nc.as_str()]);
    }

    #[tokio::test]
    async fn full_graph_returns_tenant_nodes_and_edges() {
        let server = empty_core().await;
        let hosted = HostedPrime::connect(server.uri(), None, 8, Duration::from_mins(1));
        // Pre-warm the tenant so post-write apply() lands on a warm bundle
        // (cold writes no-op apply; reads then hydrate from Core). See bug t-d90426.
        hosted.tenant_graph("t").await.unwrap();
        let a = node_entity_id(
            "contact",
            &hosted
                .add_node("t", "contact", serde_json::json!({}))
                .await
                .unwrap()
                .0,
        );
        let b = node_entity_id(
            "contact",
            &hosted
                .add_node("t", "contact", serde_json::json!({}))
                .await
                .unwrap()
                .0,
        );
        hosted.add_edge("t", &a, &b, "knows", None).await.unwrap();
        let graph = hosted.full_graph("t", None, None).await.unwrap();
        assert_eq!(graph.stats.node_count, 2);
        assert_eq!(graph.stats.edge_count, 1);
        assert!(!graph.has_more);
    }

    #[cfg(feature = "prime-vectors")]
    #[tokio::test]
    async fn delete_vector_removes_it_from_recall() {
        let server = empty_core().await;
        let hosted = HostedPrime::connect(server.uri(), None, 8, Duration::from_mins(1));
        // Pre-warm the tenant so post-write apply() lands on a warm bundle
        // (cold writes no-op apply; reads then hydrate from Core). See bug t-d90426.
        hosted.tenant_graph("t").await.unwrap();
        let nid = hosted
            .add_node("t", "n", serde_json::json!({}))
            .await
            .unwrap()
            .0;
        let eid = node_entity_id("n", &nid);
        hosted
            .embed("t", &eid, vec![1.0, 0.0, 0.0, 0.0], None)
            .await
            .unwrap();
        assert!(
            !hosted
                .recall("t", vec![1.0, 0.0, 0.0, 0.0], 5)
                .await
                .unwrap()
                .is_empty()
        );
        hosted.delete_vector("t", &eid).await.unwrap();
        assert!(
            hosted
                .recall("t", vec![1.0, 0.0, 0.0, 0.0], 5)
                .await
                .unwrap()
                .is_empty()
        );
    }
}
