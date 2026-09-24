//! Prime facade — high-level entry point wrapping [`EmbeddedCore`].
//!
//! Configures merge strategies for graph events, registers Prime projections,
//! and exposes graph-aware convenience methods on top of the event store.

#[cfg(feature = "prime-vectors")]
use std::sync::OnceLock;
use std::{
    fs::{File, OpenOptions},
    path::Path,
    sync::Arc,
};

use fs4::{FileExt, TryLockError};

use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use crate::{
    application::services::projection::Projection as _,
    embedded::{Config, EmbeddedCore, IngestEvent, Query},
    error::Result,
    infrastructure::cluster::crdt::MergeStrategy,
};

use super::{
    error::{PrimeError, PrimeResult},
    event_store::EventStore,
    projections::{
        AdjacencyListProjection, Contradiction, ContradictionDetectionProjection,
        CrossDomainProjection, DomainIndexProjection, GraphStatsProjection, NodeStateProjection,
        NodeTypeIndexProjection, ReverseIndexProjection,
    },
    schema::SchemaProjection,
    types::{
        Direction, Edge, EdgeId, Node, NodeId, PrimeStats, edge_entity_id, event_types,
        node_entity_id,
    },
};

// Well-known projection names used by the facade.
const PROJ_NODE_STATE: &str = "prime.node_state";
const PROJ_NODE_TYPE_INDEX: &str = "prime.node_type_index";
const PROJ_ADJACENCY: &str = "prime.adjacency";
const PROJ_REVERSE_INDEX: &str = "prime.reverse_index";
const PROJ_GRAPH_STATS: &str = "prime.graph_stats";
const PROJ_SCHEMA: &str = "prime.schema";
const PROJ_CONTRADICTION: &str = "prime.contradiction";
const PROJ_CROSS_DOMAIN: &str = "prime.cross_domain";
#[cfg(feature = "prime-vectors")]
const PROJ_VECTOR_INDEX: &str = "prime.vector_index";

/// High-level Prime engine wrapping [`EmbeddedCore`] with graph-aware
/// merge strategies and projections.
pub struct Prime {
    /// Event I/O backend, held behind the [`EventStore`] trait so a remote
    /// implementation can slot in without touching the facade. Backed by an
    /// [`EmbeddedCore`] today.
    core: Arc<dyn EventStore>,
    node_state: Arc<NodeStateProjection>,
    node_type_index: Arc<NodeTypeIndexProjection>,
    adjacency: Arc<AdjacencyListProjection>,
    reverse_index: Arc<ReverseIndexProjection>,
    graph_stats: Arc<GraphStatsProjection>,
    schema: Arc<SchemaProjection>,
    contradiction: Arc<ContradictionDetectionProjection>,
    cross_domain: Arc<CrossDomainProjection>,
    domain_index: Arc<DomainIndexProjection>,
    #[cfg(feature = "prime-vectors")]
    vector_index: Arc<super::vectors::VectorIndexProjection>,
    /// Lazily-initialized text → vector embedder. Built on the first
    /// `embed_text` call so graph-only callers don't download the model.
    #[cfg(feature = "prime-vectors")]
    embedder: OnceLock<Arc<super::vectors::TextEmbedder>>,
    /// Exclusive advisory lock on the data-dir, held for the process lifetime
    /// when this instance is the writer. `None` for in-memory instances and for
    /// read-only replicas (another process owns the lock). Dropping it releases
    /// the lock; the OS also releases it automatically on process exit/crash.
    _data_dir_lock: Option<DataDirLock>,
    /// True when another process owns the data-dir and this instance rejects
    /// every write. Kept as its own flag rather than derived from the lock:
    /// an in-memory instance also has no lock and is perfectly writable.
    read_only: bool,
}

/// RAII guard holding an exclusive advisory lock on a Prime data-dir.
///
/// While held, no other process can become the writer for the same data-dir,
/// which prevents the concurrent boot-time `wal.truncate()` that unlinked the
/// active WAL inode and silently lost the running writer's data (issue #201).
struct DataDirLock {
    _file: File,
}

/// Result of trying to claim write ownership of a data-dir.
enum DataDirAccess {
    /// This process owns the data-dir and may write. Hold the lock for life.
    Writer(DataDirLock),
    /// Another live process owns the data-dir; open a read-only replica.
    Replica,
}

/// Try to acquire the exclusive writer lock for `data_dir`.
///
/// Returns [`DataDirAccess::Writer`] (holding the lock) when this process is the
/// sole owner, or [`DataDirAccess::Replica`] when another live process already
/// holds it. On platforms/filesystems where advisory locking is unavailable we
/// log and fall back to `Writer` so a lone process is never bricked — the lock
/// is a safety improvement, not a hard requirement.
fn acquire_data_dir_lock(data_dir: &Path) -> Result<DataDirAccess> {
    std::fs::create_dir_all(data_dir).map_err(|e| {
        crate::error::AllSourceError::StorageError(format!(
            "failed to create Prime data dir {}: {e}",
            data_dir.display()
        ))
    })?;

    let lock_path = data_dir.join("prime.lock");
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(|e| {
            crate::error::AllSourceError::StorageError(format!(
                "failed to open Prime lock file {}: {e}",
                lock_path.display()
            ))
        })?;

    // fs4 1.x mirrors std's stabilised `File::try_lock`: success is `Ok(())`,
    // and contention is `Err(WouldBlock)` rather than the old `Ok(false)`.
    // Keep the two failure modes apart — collapsing them would either brick a
    // lone process on a lock-less filesystem or admit a second writer.
    match FileExt::try_lock(&file) {
        Ok(()) => Ok(DataDirAccess::Writer(DataDirLock { _file: file })),
        // Someone else holds the lock: this instance is a read-only replica.
        Err(TryLockError::WouldBlock) => Ok(DataDirAccess::Replica),
        Err(TryLockError::Error(e)) => {
            tracing::warn!(
                error = %e,
                path = %lock_path.display(),
                "advisory file locking unavailable for Prime data dir — proceeding as writer \
                 without a lock (do NOT run a second writer on this dir)"
            );
            // Synthesize a Writer without a real lock: better to keep a lone
            // process fully functional than to brick it on an FS without flock.
            Ok(DataDirAccess::Writer(DataDirLock { _file: file }))
        }
    }
}

impl Prime {
    /// Build an `EmbeddedConfig` with Prime merge strategies.
    fn config_builder() -> crate::embedded::ConfigBuilder {
        Config::builder()
            .merge_strategy(event_types::NODE_CREATED, MergeStrategy::FirstWriteWins)
            .merge_strategy(event_types::NODE_UPDATED, MergeStrategy::LastWriteWins)
            .merge_strategy(event_types::EDGE_CREATED, MergeStrategy::AppendOnly)
            .merge_strategy(event_types::EDGE_DELETED, MergeStrategy::LastWriteWins)
            .merge_strategy(event_types::NODE_DELETED, MergeStrategy::LastWriteWins)
    }

    /// Register all graph projections on the underlying store.
    #[allow(clippy::type_complexity)]
    fn register_graph_projections(
        store: &Arc<crate::store::EventStore>,
    ) -> (
        Arc<NodeStateProjection>,
        Arc<NodeTypeIndexProjection>,
        Arc<AdjacencyListProjection>,
        Arc<ReverseIndexProjection>,
        Arc<GraphStatsProjection>,
        Arc<SchemaProjection>,
        Arc<ContradictionDetectionProjection>,
        Arc<CrossDomainProjection>,
        Arc<DomainIndexProjection>,
    ) {
        let node_state = Arc::new(NodeStateProjection::new(PROJ_NODE_STATE));
        let node_type_index = Arc::new(NodeTypeIndexProjection::new(PROJ_NODE_TYPE_INDEX));
        let adjacency = Arc::new(AdjacencyListProjection::new_forward(PROJ_ADJACENCY));
        let reverse_index = Arc::new(ReverseIndexProjection::new_reverse(PROJ_REVERSE_INDEX));
        let graph_stats = Arc::new(GraphStatsProjection::new(PROJ_GRAPH_STATS));
        let schema = Arc::new(SchemaProjection::new(PROJ_SCHEMA));
        let contradiction = Arc::new(ContradictionDetectionProjection::new(PROJ_CONTRADICTION));
        let cross_domain = Arc::new(CrossDomainProjection::new());
        // The domain index feeds the Recall engine's compressed index
        // (prime_index / prime_context). It MUST be registered + backfilled here
        // like every other projection — otherwise a fresh, empty instance handed
        // to RecallEngine reports 0 nodes for all live-recorded data (the
        // prime_index 0-node bug). It is shared with RecallEngine via
        // `recall_deps()` so live writes and the compressed index stay in sync.
        let domain_index = Arc::new(DomainIndexProjection::new());

        type DynProj = Arc<dyn crate::application::services::projection::Projection>;

        let _ = store.register_projection_with_backfill(&(Arc::clone(&node_state) as DynProj));
        let _ = store.register_projection_with_backfill(&(Arc::clone(&node_type_index) as DynProj));
        let _ = store.register_projection_with_backfill(&(Arc::clone(&adjacency) as DynProj));
        let _ = store.register_projection_with_backfill(&(Arc::clone(&reverse_index) as DynProj));
        let _ = store.register_projection_with_backfill(&(Arc::clone(&graph_stats) as DynProj));
        let _ = store.register_projection_with_backfill(&(Arc::clone(&schema) as DynProj));
        let _ = store.register_projection_with_backfill(&(Arc::clone(&contradiction) as DynProj));
        let _ = store.register_projection_with_backfill(&(Arc::clone(&cross_domain) as DynProj));
        let _ = store.register_projection_with_backfill(&(Arc::clone(&domain_index) as DynProj));

        (
            node_state,
            node_type_index,
            adjacency,
            reverse_index,
            graph_stats,
            schema,
            contradiction,
            cross_domain,
            domain_index,
        )
    }

    /// Register vector index projection (only when prime-vectors feature is enabled).
    #[cfg(feature = "prime-vectors")]
    fn register_vector_projection(
        store: &Arc<crate::store::EventStore>,
    ) -> Arc<super::vectors::VectorIndexProjection> {
        type DynProj = Arc<dyn crate::application::services::projection::Projection>;
        let vi = Arc::new(super::vectors::VectorIndexProjection::new(
            PROJ_VECTOR_INDEX,
        ));
        let _ = store.register_projection_with_backfill(&(Arc::clone(&vi) as DynProj));
        vi
    }

    /// Build a `Prime` instance from a configured `EmbeddedCore`.
    fn from_core(core: EmbeddedCore, data_dir_lock: Option<DataDirLock>, read_only: bool) -> Self {
        let store = core.inner();

        // `core` must come from `EmbeddedCore::open`, which has already
        // hydrated the full Parquet archive; the backfill below depends on it
        // (issue #180).
        let (
            node_state,
            node_type_index,
            adjacency,
            reverse_index,
            graph_stats,
            schema,
            contradiction,
            cross_domain,
            domain_index,
        ) = Self::register_graph_projections(&store);
        #[cfg(feature = "prime-vectors")]
        let vector_index = Self::register_vector_projection(&store);

        Self {
            core: Arc::new(core) as Arc<dyn EventStore>,
            node_state,
            node_type_index,
            adjacency,
            reverse_index,
            graph_stats,
            schema,
            contradiction,
            cross_domain,
            domain_index,
            #[cfg(feature = "prime-vectors")]
            vector_index,
            #[cfg(feature = "prime-vectors")]
            embedder: OnceLock::new(),
            _data_dir_lock: data_dir_lock,
            read_only,
        }
    }

    /// True when another process owns the data-dir, so this instance serves
    /// reads of the shared memory and rejects every write.
    ///
    /// Losing the writer lock is silent by design — a replica is a correct,
    /// useful thing to be — which is exactly why a server embedding Prime must
    /// report this on its health surface. A replica whose health check says
    /// `ok` looks identical to a working writer right up until someone notices
    /// that nothing has been ingested.
    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    /// Open a durable Prime instance at `path`.
    ///
    /// Acquires an exclusive advisory lock on the data dir. If another live
    /// process already owns it, this instance opens as a **read-only replica**:
    /// it replays the WAL + Parquet so it can serve reads of the shared memory,
    /// but it never truncates the WAL (which would corrupt the owner's log —
    /// issue #201) and rejects writes. To share a writable memory across
    /// projects, run one Prime in `--mode http` (or use `--sync-to`) and point
    /// clients at it rather than opening the same data dir N times.
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let (lock, read_only) = match acquire_data_dir_lock(path)? {
            DataDirAccess::Writer(lock) => (Some(lock), false),
            DataDirAccess::Replica => {
                tracing::warn!(
                    path = %path.display(),
                    "Prime data dir is owned by another running process — opening READ-ONLY \
                     replica. This instance serves reads of the shared memory but rejects writes. \
                     For a writable shared memory, run one Prime in --mode http and connect to it."
                );
                (None, true)
            }
        };
        let config = Self::config_builder()
            .data_dir(path)
            .read_only(read_only)
            .build()?;
        let core = EmbeddedCore::open(config).await?;
        Ok(Self::from_core(core, lock, read_only))
    }

    /// Open an in-memory Prime instance (no persistence). Useful for testing.
    pub async fn open_in_memory() -> Result<Self> {
        let config = Self::config_builder().build()?;
        let core = EmbeddedCore::open(config).await?;
        Ok(Self::from_core(core, None, false))
    }

    /// Shut down the Prime engine, flushing all pending writes.
    pub async fn shutdown(self) -> Result<()> {
        self.core.shutdown().await
    }

    /// Return statistics about the Prime graph (O(1) via projection).
    pub fn stats(&self) -> PrimeStats {
        self.graph_stats.stats()
    }

    /// Access the underlying event store for direct event operations.
    ///
    /// Returns the backend behind the [`EventStore`] trait — `ingest`,
    /// `ingest_batch`, `query`, and `shutdown` are available. Callers that
    /// previously reached for `EmbeddedCore`-specific methods do not exist
    /// (the facade is the surface for everything else).
    pub fn core(&self) -> &dyn EventStore {
        &*self.core
    }

    /// Return shared projection dependencies for the Recall engine.
    ///
    /// Shares Prime's node_state, adjacency, graph_stats, cross_domain, and
    /// domain_index projections with RecallEngine, enabling L0/L1/L2 tiers.
    /// `domain_index` is the live, registered+backfilled projection (not a
    /// fresh empty one) so the Recall engine's compressed index reflects all
    /// recorded nodes — the fix for the prime_index 0-node bug.
    #[cfg(feature = "prime-recall")]
    pub fn recall_deps(&self) -> super::recall::RecallDeps {
        super::recall::RecallDeps {
            domain_index: Arc::clone(&self.domain_index),
            cross_domain: Arc::clone(&self.cross_domain),
            node_state: Some(Arc::clone(&self.node_state)),
            adjacency: Some(Arc::clone(&self.adjacency)),
            graph_stats: Some(Arc::clone(&self.graph_stats)),
        }
    }

    // =========================================================================
    // Schema Enforcement
    // =========================================================================

    /// Register a JSON schema for a node type. Properties will be validated
    /// on `add_node()` and `update_node()`. Stored as an event for durability.
    pub async fn register_schema(
        &self,
        type_name: &str,
        kind: super::schema::SchemaKind,
        schema: Value,
    ) -> PrimeResult<()> {
        let entity_id = format!("schema:{type_name}");
        self.core
            .ingest(IngestEvent {
                entity_id: &entity_id,
                event_type: super::schema::SCHEMA_REGISTERED,
                payload: json!({
                    "type_name": type_name,
                    "kind": match kind {
                        super::schema::SchemaKind::Node => "node",
                        super::schema::SchemaKind::Edge => "edge",
                    },
                    "schema": schema,
                }),
                metadata: None,
                tenant_id: None,
            })
            .await?;
        Ok(())
    }

    /// List all registered schemas.
    pub fn schemas(&self) -> Vec<super::schema::SchemaEntry> {
        self.schema.schemas()
    }

    // =========================================================================
    // Contradiction Detection
    // =========================================================================

    /// Mark a relation as exclusive (only one target per source) and backfill
    /// existing edges to detect pre-existing contradictions.
    pub fn configure_exclusive(&self, relation: &str) {
        let all_entity_ids = self.node_type_index.all_entity_ids();
        self.contradiction.configure_exclusive(relation);
        self.contradiction
            .backfill_exclusive(relation, &self.adjacency, &all_entity_ids);
    }

    /// List all unresolved contradictions.
    pub fn contradictions(&self) -> Vec<Contradiction> {
        self.contradiction.contradictions()
    }

    /// Resolve a contradiction by keeping the specified edge. Deletes the other.
    pub async fn resolve_contradiction(
        &self,
        contradiction_id: &str,
        keep_edge: &str,
    ) -> PrimeResult<()> {
        if let Some(delete_edge) = self.contradiction.resolve(contradiction_id, keep_edge) {
            self.delete_edge(&delete_edge).await?;
        }
        Ok(())
    }

    // =========================================================================
    // Node CRUD
    // =========================================================================

    /// Create a new graph node. Returns the generated [`NodeId`].
    ///
    /// If a schema is registered for `node_type`, properties are validated first.
    pub async fn add_node(
        &self,
        node_type: &str,
        properties: serde_json::Value,
    ) -> PrimeResult<NodeId> {
        self.schema.validate_node(node_type, &properties)?;

        let id = uuid::Uuid::new_v4().to_string();
        let entity_id = node_entity_id(node_type, &id);

        self.core
            .ingest(IngestEvent {
                entity_id: &entity_id,
                event_type: event_types::NODE_CREATED,
                payload: json!({
                    "id": id,
                    "node_type": node_type,
                    "properties": properties,
                }),
                metadata: None,
                tenant_id: None,
            })
            .await?;

        Ok(NodeId::new(id))
    }

    /// Get a node by its entity_id. Returns `None` if deleted or not found.
    pub fn get_node(&self, entity_id: &str) -> Option<Node> {
        let node = self.node_state.get_node(entity_id)?;
        if node.deleted {
            return None;
        }
        Some(node)
    }

    /// Update a node's properties (deep merge).
    ///
    /// Validates the merged result against any registered schema.
    ///
    /// **Eventual consistency:** Between the existence check and the ingest,
    /// another task could delete this node. The update event will be a no-op
    /// in the projection (see ADR-016).
    pub async fn update_node(
        &self,
        entity_id: &str,
        properties: serde_json::Value,
    ) -> PrimeResult<()> {
        let node = self
            .node_state
            .get_node(entity_id)
            .filter(|n| !n.deleted)
            .ok_or_else(|| PrimeError::NodeNotFound(entity_id.to_string()))?;

        // Validate the merged properties against the schema
        let mut merged = node.properties.clone();
        if let (Value::Object(base), Value::Object(update)) = (&mut merged, &properties) {
            for (k, v) in update {
                base.insert(k.clone(), v.clone());
            }
        }
        self.schema.validate_node(&node.node_type, &merged)?;

        self.core
            .ingest(IngestEvent {
                entity_id,
                event_type: event_types::NODE_UPDATED,
                payload: json!({ "properties": properties }),
                metadata: None,
                tenant_id: None,
            })
            .await?;

        Ok(())
    }

    /// Delete a node and all its connected edges atomically.
    ///
    /// Uses `ingest_batch` to emit all edge-deletion + node-deletion events
    /// in a single WAL write (ADR-013). Includes source/target in edge-delete
    /// payloads for O(1) adjacency index updates (ADR-012).
    pub async fn delete_node(&self, entity_id: &str) -> PrimeResult<()> {
        if !self.node_state.is_live(entity_id) {
            return Err(PrimeError::NodeNotFound(entity_id.to_string()));
        }

        // Collect all edge entity_ids and payloads for batch
        let outgoing = self.adjacency.outgoing(entity_id);
        let incoming = self.reverse_index.incoming(entity_id);

        // Build owned data for the batch (IngestEvent borrows strings)
        struct EdgeDel {
            entity_id: String,
            payload: serde_json::Value,
        }

        let mut edge_dels: Vec<EdgeDel> = Vec::with_capacity(outgoing.len() + incoming.len());
        let mut seen_edge_ids = std::collections::HashSet::new();

        for adj in &outgoing {
            if seen_edge_ids.insert(adj.edge_id.clone()) {
                edge_dels.push(EdgeDel {
                    entity_id: format!("edge:{}", adj.edge_id),
                    payload: json!({"id": adj.edge_id, "source": entity_id, "target": adj.peer}),
                });
            }
        }
        for adj in &incoming {
            if seen_edge_ids.insert(adj.edge_id.clone()) {
                edge_dels.push(EdgeDel {
                    entity_id: format!("edge:{}", adj.edge_id),
                    payload: json!({"id": adj.edge_id, "source": adj.peer, "target": entity_id}),
                });
            }
        }

        // Build batch: edge deletes + node delete
        let mut batch: Vec<IngestEvent<'_>> = edge_dels
            .iter()
            .map(|ed| IngestEvent {
                entity_id: &ed.entity_id,
                event_type: event_types::EDGE_DELETED,
                payload: ed.payload.clone(),
                metadata: None,
                tenant_id: None,
            })
            .collect();

        batch.push(IngestEvent {
            entity_id,
            event_type: event_types::NODE_DELETED,
            payload: json!({}),
            metadata: None,
            tenant_id: None,
        });

        self.core.ingest_batch(batch).await?;
        Ok(())
    }

    /// Get all nodes of a given type.
    pub fn nodes_by_type(&self, node_type: &str) -> Vec<Node> {
        self.node_type_index
            .nodes_by_type(node_type)
            .iter()
            .filter_map(|entity_id| self.get_node(entity_id))
            .collect()
    }

    /// Whether a node belongs to `tenant`, the single predicate every
    /// tenant-scoped read in this file shares.
    ///
    /// Prime is a single-store engine — its projections are keyed by
    /// `entity_id` alone and every Prime event is ingested with
    /// `tenant_id: None` — so the only tenant marker available at query time
    /// is the one the writer stamped into `properties`. `None` matches
    /// everything, which is correct only where the deployment itself is the
    /// tenant boundary.
    ///
    /// Two readers filtering by tenant in two slightly different ways is how a
    /// leak gets introduced, so `full_graph` and `recall` both call this.
    fn node_in_tenant(node: &Node, tenant: Option<&str>) -> bool {
        match tenant {
            None => true,
            Some(t) => node
                .properties
                .get("tenant_id")
                .and_then(|v| v.as_str())
                .is_some_and(|nt| nt == t),
        }
    }

    /// Materialize the COMPLETE knowledge graph from the live projections —
    /// every node (full properties + vector presence), every edge (relation +
    /// properties), and summary stats — reading the `node_state` and
    /// `adjacency` projections directly rather than reconstructing from an
    /// events window.
    ///
    /// ## Tenant scoping (security)
    ///
    /// Prime is a **single-store** engine: its projections are keyed only by
    /// `entity_id` (`node:{type}:{id}`), never by tenant — every Prime event
    /// is ingested with `tenant_id: None` (see `add_node`/`add_edge`). There
    /// is exactly one Prime graph per process/data-dir.
    ///
    /// To make this endpoint safe behind a multi-tenant gateway, `tenant`
    /// filters on a `tenant_id` marker carried in each node's `properties`.
    /// When `Some`, only nodes whose `properties.tenant_id` equals it are
    /// returned, and edges are restricted to that node set — so tenant A can
    /// never see tenant B's nodes even though they share one store. When
    /// `None` (local prime-mcp, single-store, no tenant), the whole graph is
    /// returned. Callers that own the tenant boundary at the *deployment*
    /// level (one Prime per tenant) simply pass `None`.
    ///
    /// `node_type` further filters to a single type. `limit` caps the node
    /// count (post-filter) and sets `has_more` when it truncates; edges are
    /// always restricted to the returned nodes.
    pub fn full_graph(
        &self,
        tenant: Option<&str>,
        node_type: Option<&str>,
        limit: Option<usize>,
    ) -> super::types::FullGraph {
        use super::types::{FullGraph, GraphEdge, GraphNode, GraphStats};
        use std::collections::{BTreeMap, HashSet};

        // 1. Collect live nodes, applying tenant + node_type filters.
        let mut live: Vec<Node> = self
            .node_state
            .all_nodes()
            .into_iter()
            .filter(|n| node_type.is_none_or(|nt| n.node_type == nt))
            .filter(|n| Self::node_in_tenant(n, tenant))
            .collect();

        // Stable ordering so pagination is deterministic.
        live.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));

        // nodes_by_type counts reflect the *unpaginated* filtered set.
        let mut nodes_by_type: BTreeMap<String, usize> = BTreeMap::new();
        for n in &live {
            *nodes_by_type.entry(n.node_type.clone()).or_default() += 1;
        }
        let total_filtered = live.len();

        let has_more = limit.is_some_and(|l| total_filtered > l);
        if let Some(l) = limit {
            live.truncate(l);
        }

        // Set of returned node wire-ids — edges are restricted to these so a
        // tenant filter can't leak cross-tenant edges.
        let included: HashSet<String> = live
            .iter()
            .map(|n| super::types::EntityId::node(&n.node_type, n.id.as_str()).to_wire())
            .collect();

        // 2. Build GraphNodes with vector presence/dimension.
        let mut vector_count = 0usize;
        let nodes: Vec<GraphNode> = live
            .iter()
            .map(|n| {
                let wire = super::types::EntityId::node(&n.node_type, n.id.as_str()).to_wire();
                let (has_vector, vector_dim) = self.vector_presence(&wire);
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

        // 3. Enumerate every live edge whose source AND target are included.
        let edges: Vec<GraphEdge> = self
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
                created_at: Utc::now(),
            })
            .collect();

        let stats = GraphStats {
            node_count: nodes.len(),
            edge_count: edges.len(),
            vector_count,
            nodes_by_type,
        };

        FullGraph {
            nodes,
            edges,
            stats,
            has_more,
        }
    }

    /// Live nodes carrying no vector, and so unreachable by semantic recall.
    ///
    /// `add_node` does not embed — a node becomes recallable only once
    /// something embeds it. This is both the work list that closes that gap
    /// and the number `prime_stats` reports, so the deficit is never silent.
    pub fn nodes_missing_vectors(&self) -> Vec<Node> {
        self.node_state
            .all_nodes()
            .into_iter()
            .filter(|n| {
                let wire = super::types::EntityId::node(&n.node_type, n.id.as_str()).to_wire();
                !self.vector_presence(&wire).0
            })
            .collect()
    }

    /// Count of [`Self::nodes_missing_vectors`], without building the list.
    pub fn count_nodes_missing_vectors(&self) -> usize {
        self.node_state
            .all_nodes()
            .into_iter()
            .filter(|n| {
                let wire = super::types::EntityId::node(&n.node_type, n.id.as_str()).to_wire();
                !self.vector_presence(&wire).0
            })
            .count()
    }

    /// Report `(has_vector, dimension)` for a node wire-id, without returning
    /// the raw embedding. Always `(false, None)` when the `prime-vectors`
    /// feature is disabled.
    fn vector_presence(&self, node_wire_id: &str) -> (bool, Option<usize>) {
        #[cfg(feature = "prime-vectors")]
        {
            let vec_entity = super::vectors::vector_entity_id(node_wire_id);
            if let Some(state) = self.vector_index.get_state(&vec_entity) {
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
            (false, None)
        }
        #[cfg(not(feature = "prime-vectors"))]
        {
            let _ = node_wire_id;
            (false, None)
        }
    }

    // =========================================================================
    // Edge CRUD
    // =========================================================================

    /// Create a directed edge between two nodes. Returns the generated [`EdgeId`].
    pub async fn add_edge(
        &self,
        source: &str,
        target: &str,
        relation: &str,
        properties: Option<serde_json::Value>,
    ) -> PrimeResult<EdgeId> {
        self.add_edge_inner(source, target, relation, None, properties)
            .await
    }

    /// Create a weighted directed edge between two nodes.
    pub async fn add_edge_weighted(
        &self,
        source: &str,
        target: &str,
        relation: &str,
        weight: f64,
        properties: Option<serde_json::Value>,
    ) -> PrimeResult<EdgeId> {
        self.add_edge_inner(source, target, relation, Some(weight), properties)
            .await
    }

    async fn add_edge_inner(
        &self,
        source: &str,
        target: &str,
        relation: &str,
        weight: Option<f64>,
        properties: Option<serde_json::Value>,
    ) -> PrimeResult<EdgeId> {
        // Validate source and target exist and are not deleted.
        // Note: eventual consistency — node could be deleted between check and
        // ingest. The edge event is harmless in that case (ADR-016).
        if !self.node_state.is_live(source) {
            return Err(PrimeError::NodeNotFound(source.to_string()));
        }
        if !self.node_state.is_live(target) {
            return Err(PrimeError::NodeNotFound(target.to_string()));
        }

        // Validate edge properties against registered schema
        self.schema.validate_edge(relation, properties.as_ref())?;

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

        self.core
            .ingest(IngestEvent {
                entity_id: &entity_id,
                event_type: event_types::EDGE_CREATED,
                payload,
                metadata: None,
                tenant_id: None,
            })
            .await?;

        Ok(EdgeId::new(id))
    }

    /// Get an edge by its ID. Queries the event store for the edge creation event.
    pub async fn get_edge(&self, edge_id: &str) -> PrimeResult<Option<Edge>> {
        let entity_id = edge_entity_id(edge_id);
        let events = self.core.query(Query::new().entity_id(&entity_id)).await?;

        // Check if deleted
        let is_deleted = events
            .iter()
            .any(|e| e.event_type == event_types::EDGE_DELETED);
        if is_deleted {
            return Ok(None);
        }

        // Find creation event
        let created = events
            .iter()
            .find(|e| e.event_type == event_types::EDGE_CREATED);

        Ok(created.and_then(edge_from_event))
    }

    /// Delete an edge.
    pub async fn delete_edge(&self, edge_id: &str) -> PrimeResult<()> {
        let entity_id = edge_entity_id(edge_id);

        self.core
            .ingest(IngestEvent {
                entity_id: &entity_id,
                event_type: event_types::EDGE_DELETED,
                payload: json!({"id": edge_id}),
                metadata: None,
                tenant_id: None,
            })
            .await?;

        Ok(())
    }

    // =========================================================================
    // Memory Compaction
    // =========================================================================

    /// Merge source nodes into the target node.
    ///
    /// - Properties are merged (target values take precedence on conflict)
    /// - All edges pointing to/from source nodes are redirected to target
    /// - Source nodes are soft-deleted
    /// - A `prime.memory.compacted` event is emitted for the audit trail
    ///
    /// The original state is preserved in event history and can be reconstructed
    /// via `as_of` time-travel queries.
    pub async fn compact(
        &self,
        target_entity_id: &str,
        source_entity_ids: &[&str],
    ) -> PrimeResult<()> {
        // Verify target exists
        let target_node = self
            .get_node(target_entity_id)
            .ok_or_else(|| PrimeError::NodeNotFound(target_entity_id.to_string()))?;

        // Collect properties from all sources for merging
        let mut merged_properties = target_node.properties.clone();
        let mut merged_source_ids = Vec::new();

        for &source_id in source_entity_ids {
            let source_node = self
                .get_node(source_id)
                .ok_or_else(|| PrimeError::NodeNotFound(source_id.to_string()))?;

            // Merge properties: source values fill gaps, target values win on conflict
            if let (Value::Object(target_map), Value::Object(source_map)) =
                (&mut merged_properties, &source_node.properties)
            {
                for (key, value) in source_map {
                    target_map
                        .entry(key.clone())
                        .or_insert_with(|| value.clone());
                }
            }

            merged_source_ids.push(source_id.to_string());

            // Redirect outgoing edges from source to target
            for entry in self.adjacency.outgoing(source_id) {
                // Don't create self-loops (if source pointed to target)
                if entry.peer == target_entity_id {
                    continue;
                }
                // Don't redirect if it would create a duplicate edge
                // (target already has same relation to same peer)
                let already_exists = self
                    .adjacency
                    .outgoing(target_entity_id)
                    .iter()
                    .any(|e| e.peer == entry.peer && e.relation == entry.relation);

                if !already_exists {
                    self.core
                        .ingest(IngestEvent {
                            entity_id: &edge_entity_id(&uuid::Uuid::new_v4().to_string()),
                            event_type: event_types::EDGE_CREATED,
                            payload: json!({
                                "id": uuid::Uuid::new_v4().to_string(),
                                "source": target_entity_id,
                                "target": entry.peer,
                                "relation": entry.relation,
                            }),
                            metadata: None,
                            tenant_id: None,
                        })
                        .await?;
                }

                // Delete the old edge from source
                self.core
                    .ingest(IngestEvent {
                        entity_id: &edge_entity_id(&entry.edge_id),
                        event_type: event_types::EDGE_DELETED,
                        payload: json!({"id": entry.edge_id}),
                        metadata: None,
                        tenant_id: None,
                    })
                    .await?;
            }

            // Redirect incoming edges to source → point to target instead
            for entry in self.reverse_index.incoming(source_id) {
                // Don't create self-loops
                if entry.peer == target_entity_id {
                    continue;
                }
                let already_exists = self
                    .reverse_index
                    .incoming(target_entity_id)
                    .iter()
                    .any(|e| e.peer == entry.peer && e.relation == entry.relation);

                if !already_exists {
                    self.core
                        .ingest(IngestEvent {
                            entity_id: &edge_entity_id(&uuid::Uuid::new_v4().to_string()),
                            event_type: event_types::EDGE_CREATED,
                            payload: json!({
                                "id": uuid::Uuid::new_v4().to_string(),
                                "source": entry.peer,
                                "target": target_entity_id,
                                "relation": entry.relation,
                            }),
                            metadata: None,
                            tenant_id: None,
                        })
                        .await?;
                }

                // Delete the old edge
                self.core
                    .ingest(IngestEvent {
                        entity_id: &edge_entity_id(&entry.edge_id),
                        event_type: event_types::EDGE_DELETED,
                        payload: json!({"id": entry.edge_id}),
                        metadata: None,
                        tenant_id: None,
                    })
                    .await?;
            }

            // Soft-delete the source node
            self.core
                .ingest(IngestEvent {
                    entity_id: source_id,
                    event_type: event_types::NODE_DELETED,
                    payload: json!({}),
                    metadata: None,
                    tenant_id: None,
                })
                .await?;
        }

        // Update target with merged properties
        self.core
            .ingest(IngestEvent {
                entity_id: target_entity_id,
                event_type: event_types::NODE_UPDATED,
                payload: json!({
                    "properties": merged_properties,
                }),
                metadata: None,
                tenant_id: None,
            })
            .await?;

        // Emit compaction event for audit trail
        self.core
            .ingest(IngestEvent {
                entity_id: target_entity_id,
                event_type: "prime.memory.compacted",
                payload: json!({
                    "target": target_entity_id,
                    "merged_from": merged_source_ids,
                }),
                metadata: None,
                tenant_id: None,
            })
            .await?;

        // Clean up vectors associated with merged source nodes
        #[cfg(feature = "prime-vectors")]
        {
            for source_id in &merged_source_ids {
                let vec_entity_id = super::vectors::vector_entity_id(source_id);
                // Check if source had a stored vector
                if self.vector_index.get_state(&vec_entity_id).is_some() {
                    self.core
                        .ingest(IngestEvent {
                            entity_id: &vec_entity_id,
                            event_type: super::vectors::event_types::VECTOR_DELETED,
                            payload: json!({}),
                            metadata: None,
                            tenant_id: None,
                        })
                        .await?;
                }
            }
        }

        Ok(())
    }

    // =========================================================================
    // Vector Operations (requires `prime-vectors` feature)
    // =========================================================================

    /// Lazily initialize and return the in-process text embedder.
    ///
    /// First call downloads the AllMiniLML6V2 model into the fastembed cache
    /// (~25 MB); subsequent calls reuse the cached model. Embedding latency
    /// on AllMiniLML6V2 is ~1–3 ms for short strings on modern CPUs.
    #[cfg(feature = "prime-vectors")]
    pub fn embedder(&self) -> PrimeResult<&Arc<super::vectors::TextEmbedder>> {
        if let Some(e) = self.embedder.get() {
            return Ok(e);
        }
        let new = Arc::new(super::vectors::TextEmbedder::new()?);
        // If another thread won the race, our `new` is dropped and we read
        // theirs back via `get` below.
        let _ = self.embedder.set(new);
        Ok(self
            .embedder
            .get()
            .expect("embedder was just set or already initialized"))
    }

    /// Convert text to an embedding vector using the in-process embedder.
    ///
    /// Lets MCP/HTTP callers that only have text supply vector-based tools
    /// like [`Prime::embed`], [`Prime::vector_search`], and [`Prime::recall`]
    /// without standing up a separate embedding service.
    #[cfg(feature = "prime-vectors")]
    pub fn embed_text(&self, text: &str) -> PrimeResult<Vec<f32>> {
        self.embedder()?.embed(text)
    }

    /// Store a vector embedding associated with an entity.
    #[cfg(feature = "prime-vectors")]
    pub async fn embed(&self, id: &str, text: Option<&str>, vector: Vec<f32>) -> PrimeResult<()> {
        self.embed_with_metadata(id, text, vector, None).await
    }

    /// Store a vector embedding with additional metadata.
    #[cfg(feature = "prime-vectors")]
    pub async fn embed_with_metadata(
        &self,
        id: &str,
        text: Option<&str>,
        vector: Vec<f32>,
        metadata: Option<serde_json::Value>,
    ) -> PrimeResult<()> {
        let entity_id = super::vectors::vector_entity_id(id);
        let dimensions = vector.len();

        // Dimension guard: every vector in a data-dir must share one dimension
        // (the embedding model's output size). Switching embedders against a
        // populated data-dir — e.g. fastembed's 384 dims vs OpenAI's 1536 — would
        // silently mix incompatible vectors and corrupt similarity search (the
        // HNSW index computes distances assuming equal length). Reject the
        // mismatch with an actionable error instead. The index is hydrated from
        // the event log on open, so `established` reflects persisted vectors too.
        if let Some(established) = self.vector_index.dimension()
            && established != dimensions
        {
            return Err(PrimeError::CoreError(anyhow::anyhow!(
                "embedding dimension mismatch for `{id}`: this data-dir already holds \
                 {established}-dim vectors, but got a {dimensions}-dim vector. Mixing \
                 dimensions corrupts similarity search.\n\
                 To fix one of:\n\
                 • Use the same embedder/model that produced the existing {established}-dim \
                 vectors (check PRIME_EMBED_ENDPOINT / PRIME_EMBED_MODEL / the bundled model).\n\
                 • Start a fresh --data-dir for the new model.\n\
                 • Re-embed every node with the new model (clear and rebuild this data-dir).",
            )));
        }

        self.core
            .ingest(IngestEvent {
                entity_id: &entity_id,
                event_type: super::vectors::event_types::VECTOR_STORED,
                payload: json!({
                    "text": text,
                    "dimensions": dimensions,
                    "metadata": metadata,
                }),
                metadata: Some(json!({ "embedding": vector })),
                tenant_id: None,
            })
            .await?;

        Ok(())
    }

    /// Find the `top_k` most similar vectors to the one stored at `id`.
    #[cfg(feature = "prime-vectors")]
    pub fn similar(
        &self,
        id: &str,
        top_k: usize,
    ) -> PrimeResult<Vec<super::vectors::VectorSearchResult>> {
        let entity_id = super::vectors::vector_entity_id(id);
        let state = self
            .vector_index
            .get_state(&entity_id)
            .ok_or_else(|| PrimeError::NodeNotFound(id.to_string()))?;

        let vector: Vec<f32> = state
            .get("vector")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        Ok(self.vector_search(&vector, top_k))
    }

    /// Direct vector search with a raw query vector.
    #[cfg(feature = "prime-vectors")]
    pub fn vector_search(
        &self,
        query: &[f32],
        top_k: usize,
    ) -> Vec<super::vectors::VectorSearchResult> {
        self.vector_index
            .search(query, top_k)
            .into_iter()
            .map(|hit| super::vectors::VectorSearchResult {
                id: hit.entity_id,
                score: 1.0 - f64::from(hit.distance), // Convert distance to similarity
                text: hit.text,
                metadata: hit.metadata,
            })
            .collect()
    }

    /// Domain-aware vector search that ensures results span multiple domains.
    ///
    /// Over-fetches from HNSW, groups by domain, then round-robin interleaves
    /// so that cross-domain queries return results from all relevant domains
    /// instead of clustering in the single most similar domain.
    #[cfg(feature = "prime-vectors")]
    pub fn vector_search_cross_domain(
        &self,
        query: &[f32],
        top_k: usize,
    ) -> Vec<super::vectors::VectorSearchResult> {
        use std::collections::HashMap;

        // Over-fetch to get candidates across domains
        let all_results = self.vector_search(query, top_k * 3);

        if all_results.is_empty() {
            return all_results;
        }

        // Group results by domain
        let mut by_domain: HashMap<String, Vec<super::vectors::VectorSearchResult>> =
            HashMap::new();
        for result in all_results {
            let domain = self
                .domain_of(&result.id)
                .unwrap_or_else(|| "unknown".to_string());
            by_domain.entry(domain).or_default().push(result);
        }

        // If only one domain, no cross-domain benefit — return as-is
        if by_domain.len() <= 1 {
            let mut flat: Vec<_> = by_domain.into_values().flatten().collect();
            flat.truncate(top_k);
            return flat;
        }

        // Round-robin interleave across domains
        let mut merged = Vec::with_capacity(top_k);
        let domain_count = by_domain.len();
        let per_domain = (top_k / domain_count).max(1);

        for results in by_domain.values() {
            merged.extend(results.iter().take(per_domain).cloned());
        }

        // Sort by score and truncate
        merged.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        merged.truncate(top_k);
        merged
    }

    /// Look up the domain of a node by its entity ID.
    ///
    /// Checks top-level `domain` field first, then `properties.domain`,
    /// then falls back to `node_type` (since node_type often IS the domain).
    #[cfg(feature = "prime-vectors")]
    fn domain_of(&self, entity_id: &str) -> Option<String> {
        // entity_id from vector search is "vec:{node_entity_id}" — strip prefix
        let node_id = entity_id.strip_prefix("vec:").unwrap_or(entity_id);
        let state = self.node_state.get_state(node_id)?;

        // Try top-level domain field
        if let Some(domain) = state.get("domain").and_then(|v| v.as_str()) {
            return Some(domain.to_string());
        }
        // Try properties.domain
        if let Some(domain) = state
            .get("properties")
            .and_then(|p| p.get("domain"))
            .and_then(|v| v.as_str())
        {
            return Some(domain.to_string());
        }
        // Fall back to node_type as domain
        state
            .get("node_type")
            .and_then(|v| v.as_str())
            .map(String::from)
    }

    /// Delete a stored vector embedding.
    #[cfg(feature = "prime-vectors")]
    pub async fn delete_vector(&self, id: &str) -> PrimeResult<()> {
        let entity_id = super::vectors::vector_entity_id(id);

        self.core
            .ingest(IngestEvent {
                entity_id: &entity_id,
                event_type: super::vectors::event_types::VECTOR_DELETED,
                payload: json!({}),
                metadata: None,
                tenant_id: None,
            })
            .await?;

        Ok(())
    }

    /// Get a stored vector entry.
    #[cfg(feature = "prime-vectors")]
    pub fn get_vector(&self, id: &str) -> Option<super::vectors::VectorEntry> {
        let entity_id = super::vectors::vector_entity_id(id);
        let state = self.vector_index.get_state(&entity_id)?;

        Some(super::vectors::VectorEntry {
            id: id.to_string(),
            text: state.get("text").and_then(|v| v.as_str()).map(String::from),
            dimensions: state
                .get("vector")
                .and_then(serde_json::Value::as_array)
                .map_or(0, Vec::len),
            metadata: state.get("metadata").cloned(),
        })
    }

    // =========================================================================
    // Remember / Forget Convenience Methods (requires `prime-vectors`)
    // =========================================================================

    /// High-level "remember" — creates a node, stores its embedding, and creates
    /// edges to related entities, all in one call.
    ///
    /// Returns the entity_id of the created node.
    #[cfg(feature = "prime-vectors")]
    pub async fn remember(
        &self,
        text: &str,
        vector: Vec<f32>,
        node_type: &str,
        properties: serde_json::Value,
        relations: &[(&str, &str)], // (target_entity_id, relation)
    ) -> PrimeResult<String> {
        // 1. Create node
        let node_id = self.add_node(node_type, properties).await?;
        let entity_id = node_entity_id(node_type, node_id.as_str());

        // 2. Store embedding
        self.embed(&entity_id, Some(text), vector).await?;

        // 3. Create edges
        for (target, relation) in relations {
            self.add_edge(&entity_id, target, relation, None).await?;
        }

        Ok(entity_id)
    }

    /// High-level "forget" — soft-deletes a node, its embedding, and all connected edges.
    #[cfg(feature = "prime-vectors")]
    pub async fn forget(&self, entity_id: &str) -> PrimeResult<()> {
        // Delete vector (strip "node:" prefix to get vec entity_id)
        self.delete_vector(entity_id).await?;

        // Delete node (cascades edges)
        self.delete_node(entity_id).await?;

        Ok(())
    }

    // =========================================================================
    // Hybrid Recall
    // =========================================================================

    /// Hybrid recall combining vector similarity, graph proximity, and temporal recency.
    ///
    /// Scoring: `similarity_weight * cosine + proximity_weight * 1/(1+depth) + recency_weight * exp_decay`
    ///
    /// ## Tenant scoping (security)
    ///
    /// `query.tenant` filters on `properties.tenant_id` exactly as
    /// [`Self::full_graph`] does, through the same [`Self::node_in_tenant`]
    /// predicate. It applies to vector seeds, lexical and type-scan seeds, and
    /// graph expansion alike — see the note in the body for why expansion
    /// cannot be left out. `None` searches the whole store and is correct only
    /// where the deployment is the tenant boundary.
    #[cfg(feature = "prime-vectors")]
    // Public async API, and one of a family of `pub async fn` methods on the
    // Prime facade that the MCP/HTTP layers call uniformly. Dropping `async`
    // here alone would break that symmetry and change the public signature;
    // this variant reads from already-hydrated in-memory projections, so it
    // happens to have no `.await` today.
    #[allow(unknown_lints, clippy::unused_async_trait_impl)]
    pub async fn recall(
        &self,
        query: super::types::RecallQuery,
    ) -> PrimeResult<super::types::RecallResult> {
        use super::types::{RecallResult, Retrieval, ScoreComponents, ScoredNode};
        use std::collections::HashMap;

        let now = chrono::Utc::now();
        let mut scored: HashMap<String, ScoredNode> = HashMap::new();
        let mut vector_results = Vec::new();
        let mut retrieval = Retrieval::Empty;
        // Applied to all three arms below. Filtering only the seeds would
        // still leak: graph expansion walks edges, and an edge can cross into
        // another tenant's node.
        let tenant = query.tenant.as_deref();
        // Every seeding arm is capped at this, so graph expansion runs over a
        // bounded frontier no matter which arm produced the seeds.
        let seed_cap = query.top_k * 2;

        // Normalize weights so they sum to 1.0
        let total_weight = query.similarity_weight + query.proximity_weight + query.recency_weight;
        let (sw, pw, rw) = if total_weight > 0.0 {
            (
                query.similarity_weight / total_weight,
                query.proximity_weight / total_weight,
                query.recency_weight / total_weight,
            )
        } else {
            (1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0)
        };

        // Step 1: Vector similarity (if query vector provided)
        if let Some(ref qvec) = query.vector {
            retrieval = Retrieval::Semantic;
            let hits = self.vector_search(qvec, seed_cap); // Over-fetch for graph expansion
            vector_results = hits
                .iter()
                .map(|h| super::vectors::VectorSearchResult {
                    id: h.id.clone(),
                    score: h.score,
                    text: h.text.clone(),
                    metadata: h.metadata.clone(),
                })
                .collect();

            // Seed scored nodes from vector hits (depth=0)
            for hit in &hits {
                // Vector entity_id is "vec:{graph_entity_id}" — strip prefix
                let graph_id = hit.id.strip_prefix("vec:").unwrap_or(&hit.id);
                if let Some(node) = self
                    .get_node(graph_id)
                    .filter(|n| Self::node_in_tenant(n, tenant))
                {
                    let recency = recency_score(node.updated_at, now);
                    let components = ScoreComponents {
                        similarity: hit.score,
                        proximity: 1.0, // depth 0 = max proximity
                        recency,
                    };
                    let score = sw * hit.score + pw * 1.0 + rw * recency;
                    scored.insert(
                        graph_id.to_string(),
                        ScoredNode {
                            node,
                            score,
                            depth: 0,
                            components,
                        },
                    );
                }
            }
        }

        // Step 1b: No vector — rank by how well node text matches the query.
        // Must run before Step 2, which seeds from `scored`: an arm that fills
        // `scored` after it gets no graph expansion and `depth` is silently dead.
        if query.vector.is_none() {
            let terms = query
                .text
                .as_deref()
                .map(super::lexical::terms)
                .unwrap_or_default();

            let candidates: Vec<Node> = match query.node_type {
                Some(ref nt) => self.nodes_by_type(nt),
                None => self.node_state.all_nodes(),
            }
            .into_iter()
            .filter(|n| Self::node_in_tenant(n, tenant))
            .collect();

            let mut hits: Vec<(f64, Node)> = if terms.is_empty() {
                Vec::new()
            } else {
                candidates
                    .iter()
                    .filter_map(|n| {
                        let s = super::lexical::score(n, &terms);
                        (s > 0.0).then(|| (s, n.clone()))
                    })
                    .collect()
            };

            if hits.is_empty() {
                // Gated on node_type: "every node in the graph, by recency" is
                // not an answer to an unmatched query.
                if query.node_type.is_some() {
                    retrieval = Retrieval::TypeScan;
                    hits = candidates.into_iter().map(|n| (0.0, n)).collect();
                    hits.sort_by_key(|hit| std::cmp::Reverse(hit.1.updated_at));
                }
            } else {
                retrieval = Retrieval::Lexical;
                hits.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
            }

            for (similarity, node) in hits.into_iter().take(seed_cap) {
                let entity_id = node_entity_id(&node.node_type, node.id.as_str());
                let recency = recency_score(node.updated_at, now);
                let components = ScoreComponents {
                    similarity,
                    proximity: 1.0,
                    recency,
                };
                let score = sw * similarity + pw * 1.0 + rw * recency;
                scored.insert(
                    entity_id,
                    ScoredNode {
                        node,
                        score,
                        depth: 0,
                        components,
                    },
                );
            }
        }

        // Step 2: Graph expansion from seed nodes
        if query.depth > 0 {
            let seeds: Vec<String> = scored.keys().cloned().collect();
            for seed_id in &seeds {
                let bfs_results =
                    self.neighbors_within(seed_id, query.depth, None, Direction::Both);
                for (node, depth) in bfs_results {
                    if depth == 0 {
                        continue; // Already scored
                    }
                    let entity_id = node_entity_id(&node.node_type, node.id.as_str());
                    if scored.contains_key(&entity_id) {
                        continue;
                    }
                    // Filter by node_type if specified
                    if query
                        .node_type
                        .as_ref()
                        .is_some_and(|nt| node.node_type != *nt)
                    {
                        continue;
                    }
                    if !Self::node_in_tenant(&node, tenant) {
                        continue;
                    }
                    let proximity = 1.0 / (1.0 + depth as f64);
                    let recency = recency_score(node.updated_at, now);
                    let components = ScoreComponents {
                        similarity: 0.0, // No direct vector match
                        proximity,
                        recency,
                    };
                    let score = pw * proximity + rw * recency;
                    scored.insert(
                        entity_id,
                        ScoredNode {
                            node,
                            score,
                            depth,
                            components,
                        },
                    );
                }
            }
        }

        // Sort by score descending
        let mut nodes: Vec<ScoredNode> = scored.into_values().collect();
        nodes.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // MMR re-ranking: boost diversity across domains/types
        mmr_rerank(&mut nodes, query.top_k, 0.7);

        if nodes.is_empty() {
            retrieval = Retrieval::Empty;
        }

        Ok(RecallResult {
            nodes,
            vectors: vector_results,
            edges: Vec::new(),
            retrieval,
        })
    }

    // =========================================================================
    // Traversal
    // =========================================================================

    /// Get 1-hop neighbors of a node, optionally filtered by relation and direction.
    ///
    /// Returns full [`Node`] objects. Deleted nodes are excluded.
    pub fn neighbors(
        &self,
        entity_id: &str,
        relation: Option<&str>,
        direction: Direction,
    ) -> Vec<Node> {
        let mut peer_ids: Vec<String> = Vec::new();

        match direction {
            Direction::Outgoing => {
                for entry in self.adjacency.outgoing(entity_id) {
                    if relation.is_none() || relation == Some(entry.relation.as_str()) {
                        peer_ids.push(entry.peer.clone());
                    }
                }
            }
            Direction::Incoming => {
                for entry in self.reverse_index.incoming(entity_id) {
                    if relation.is_none() || relation == Some(entry.relation.as_str()) {
                        peer_ids.push(entry.peer.clone());
                    }
                }
            }
            Direction::Both => {
                let mut seen = std::collections::HashSet::new();
                for entry in self.adjacency.outgoing(entity_id) {
                    if (relation.is_none() || relation == Some(entry.relation.as_str()))
                        && seen.insert(entry.peer.clone())
                    {
                        peer_ids.push(entry.peer.clone());
                    }
                }
                for entry in self.reverse_index.incoming(entity_id) {
                    if (relation.is_none() || relation == Some(entry.relation.as_str()))
                        && seen.insert(entry.peer.clone())
                    {
                        peer_ids.push(entry.peer.clone());
                    }
                }
            }
        }

        peer_ids.iter().filter_map(|id| self.get_node(id)).collect()
    }

    /// BFS traversal up to `depth` hops. Returns nodes with their depth level.
    ///
    /// `depth = 0` returns just the center node. `depth = 1` returns center + immediate neighbors.
    pub fn neighbors_within(
        &self,
        entity_id: &str,
        depth: usize,
        relation: Option<&str>,
        direction: Direction,
    ) -> Vec<(Node, usize)> {
        use std::collections::{HashSet, VecDeque};

        let mut visited = HashSet::new();
        let mut result = Vec::new();
        let mut queue = VecDeque::new();

        visited.insert(entity_id.to_string());
        queue.push_back((entity_id.to_string(), 0usize));

        while let Some((current, d)) = queue.pop_front() {
            if let Some(node) = self.get_node(&current) {
                result.push((node, d));
            }

            if d < depth {
                let peers = self.neighbor_ids(&current, relation, direction);
                for peer in peers {
                    if visited.insert(peer.clone()) {
                        queue.push_back((peer, d + 1));
                    }
                }
            }
        }

        result
    }

    /// Extract an ego-network subgraph around `center` up to `depth` hops.
    pub fn subgraph(&self, center: &str, depth: usize) -> super::types::SubGraph {
        use std::collections::HashSet;

        let bfs = self.neighbors_within(center, depth, None, Direction::Both);
        let node_ids: HashSet<String> = bfs
            .iter()
            .map(|(n, _)| {
                // Reconstruct entity_id from node
                node_entity_id(&n.node_type, n.id.as_str())
            })
            .collect();

        let nodes: Vec<Node> = bfs.into_iter().map(|(n, _)| n).collect();

        // Collect all edges between nodes in the subgraph
        let mut edges = Vec::new();
        let mut seen_edges = HashSet::new();
        for entity_id in &node_ids {
            for entry in self.adjacency.outgoing(entity_id) {
                if node_ids.contains(&entry.peer) && seen_edges.insert(entry.edge_id.clone()) {
                    edges.push(Edge {
                        id: EdgeId::new(&entry.edge_id),
                        source: NodeId::new(entity_id.clone()),
                        target: NodeId::new(&entry.peer),
                        relation: entry.relation.clone(),
                        properties: None,
                        weight: entry.weight,
                        deleted: false,
                        created_at: chrono::Utc::now(),
                    });
                }
            }
        }

        super::types::SubGraph { nodes, edges }
    }

    /// BFS shortest path (unweighted). Returns ordered path including start and end.
    pub fn shortest_path(&self, from: &str, to: &str, relation: Option<&str>) -> Option<Vec<Node>> {
        use std::collections::{HashMap, VecDeque};

        if from == to {
            return self.get_node(from).map(|n| vec![n]);
        }

        let mut visited: HashMap<String, String> = HashMap::new(); // child -> parent
        let mut queue = VecDeque::new();

        visited.insert(from.to_string(), String::new());
        queue.push_back(from.to_string());

        while let Some(current) = queue.pop_front() {
            let peers = self.neighbor_ids(&current, relation, Direction::Outgoing);
            for peer in peers {
                if visited.contains_key(&peer) {
                    continue;
                }
                visited.insert(peer.clone(), current.clone());
                if peer == to {
                    // Reconstruct path
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
                    return Some(path_ids.iter().filter_map(|id| self.get_node(id)).collect());
                }
                queue.push_back(peer);
            }
        }

        None // No path
    }

    /// Dijkstra shortest path (weighted). Returns path + total weight.
    pub fn shortest_path_weighted(
        &self,
        from: &str,
        to: &str,
        relation: Option<&str>,
    ) -> Option<(Vec<Node>, f64)> {
        use std::{
            cmp::Ordering,
            collections::{BinaryHeap, HashMap},
        };

        if from == to {
            return self.get_node(from).map(|n| (vec![n], 0.0));
        }

        #[derive(PartialEq)]
        struct State {
            cost: f64,
            node: String,
        }

        impl Eq for State {}
        impl PartialOrd for State {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                Some(self.cmp(other))
            }
        }
        impl Ord for State {
            fn cmp(&self, other: &Self) -> Ordering {
                // Reverse for min-heap
                other
                    .cost
                    .partial_cmp(&self.cost)
                    .unwrap_or(Ordering::Equal)
            }
        }

        let mut dist: HashMap<String, f64> = HashMap::new();
        let mut prev: HashMap<String, String> = HashMap::new();
        let mut heap = BinaryHeap::new();

        dist.insert(from.to_string(), 0.0);
        heap.push(State {
            cost: 0.0,
            node: from.to_string(),
        });

        while let Some(State { cost, node }) = heap.pop() {
            if node == to {
                // Reconstruct path
                let mut path_ids = vec![to.to_string()];
                let mut cursor = to.to_string();
                while let Some(parent) = prev.get(&cursor) {
                    path_ids.push(parent.clone());
                    cursor = parent.clone();
                }
                path_ids.reverse();
                let nodes: Vec<Node> = path_ids.iter().filter_map(|id| self.get_node(id)).collect();
                return Some((nodes, cost));
            }

            if cost > *dist.get(&node).unwrap_or(&f64::INFINITY) {
                continue;
            }

            for entry in self.adjacency.outgoing(&node) {
                if relation.is_some() && relation != Some(entry.relation.as_str()) {
                    continue;
                }
                // Default weight 1.0 for unweighted edges
                let edge_weight = entry.weight.unwrap_or(1.0);
                let next_cost = cost + edge_weight;
                if next_cost < *dist.get(&entry.peer).unwrap_or(&f64::INFINITY) {
                    dist.insert(entry.peer.clone(), next_cost);
                    prev.insert(entry.peer.clone(), node.clone());
                    heap.push(State {
                        cost: next_cost,
                        node: entry.peer.clone(),
                    });
                }
            }
        }

        None
    }

    // =========================================================================
    // Temporal Queries
    // =========================================================================

    /// Get the full audit trail for any entity (node, edge, or vector).
    ///
    /// Returns all events in chronological order. Returns an empty vec
    /// (not an error) if the entity has never existed.
    pub async fn history(&self, entity_id: &str) -> PrimeResult<Vec<super::types::HistoryEntry>> {
        let events = self.core.query(Query::new().entity_id(entity_id)).await?;

        let entries = events
            .iter()
            .map(|e| super::types::HistoryEntry {
                event_type: e.event_type.clone(),
                timestamp: e.timestamp,
                payload: e.payload.clone(),
                source: e
                    .metadata
                    .as_ref()
                    .and_then(|m| m.get("source"))
                    .and_then(|v| v.as_str())
                    .map(String::from),
            })
            .collect();

        Ok(entries)
    }

    /// Project a node's event history into a list of
    /// [`super::projections::declarative::Observation`]s
    /// suitable for [`super::projections::declarative::fold`] and the
    /// provenance API.
    ///
    /// Only `prime.node.created` and `prime.node.updated` events contribute
    /// observations — deletes are out of scope for the fold (the snapshot
    /// represents the live state of a node). Each observation carries:
    /// - `observed_at` from the event's timestamp
    /// - `source_event_id` from the event's id (load-bearing for provenance)
    /// - `source_priority` and `specificity_score` from the event metadata,
    ///   defaulting to 0 when absent — writers are expected to set these
    ///   when they care about projection precedence (see the convention
    ///   in `MergePolicy::HighestPriority`)
    /// - `fields` from `payload.properties` (the canonical node-property
    ///   bag emitted by `add_node` / `update_node`)
    pub async fn observations(
        &self,
        entity_id: &str,
    ) -> PrimeResult<Vec<super::projections::declarative::Observation>> {
        use super::projections::declarative::Observation;
        use std::collections::BTreeMap;

        let events = self.core.query(Query::new().entity_id(entity_id)).await?;

        let observations = events
            .iter()
            .filter(|e| {
                e.event_type.as_str() == event_types::NODE_CREATED
                    || e.event_type.as_str() == event_types::NODE_UPDATED
            })
            .map(|e| {
                let fields = e
                    .payload
                    .get("properties")
                    .and_then(|p| p.as_object())
                    .map(|obj| {
                        obj.iter()
                            .map(|(k, v)| (k.clone(), v.clone()))
                            .collect::<BTreeMap<_, _>>()
                    })
                    .unwrap_or_default();
                let metadata = e.metadata.as_ref();
                let source_priority = metadata
                    .and_then(|m| m.get("source_priority"))
                    .and_then(serde_json::Value::as_i64)
                    .map_or(0, |i| i as i32);
                let specificity_score = metadata
                    .and_then(|m| m.get("specificity_score"))
                    .and_then(serde_json::Value::as_i64)
                    .map_or(0, |i| i as i32);
                Observation {
                    observed_at: e.timestamp,
                    source_priority,
                    specificity_score,
                    source_event_id: e.id.to_string(),
                    fields,
                }
            })
            .collect();

        Ok(observations)
    }

    /// Persist a declarative projection definition as a
    /// `prime.projection.defined` event in Core. The event log is the
    /// source of truth for projection defs — in-memory registries are
    /// caches that hydrate from `load_projection_defs` on startup.
    ///
    /// Calling this multiple times for the same `entity_type` appends new
    /// events; replay picks the latest by timestamp (last-write-wins),
    /// so agents can iterate on a definition without losing prior
    /// versions from the audit trail.
    ///
    /// Entity ID convention: `projection:{entity_type}`. One stream per
    /// entity_type, so `history(entity_id)` gives the full evolution.
    pub async fn define_projection(
        &self,
        def: &super::projections::declarative::ProjectionDef,
    ) -> PrimeResult<()> {
        let entity_id = format!("projection:{}", def.entity_type);
        let payload = serde_json::to_value(def).map_err(|e| {
            PrimeError::ValidationFailed(format!("failed to serialize ProjectionDef: {e}"))
        })?;
        self.core
            .ingest(IngestEvent {
                entity_id: &entity_id,
                event_type: event_types::PROJECTION_DEFINED,
                payload,
                metadata: None,
                tenant_id: None,
            })
            .await?;
        Ok(())
    }

    /// Replay every `prime.projection.defined` event and return the
    /// latest definition per entity_type. Called at startup to hydrate
    /// the in-memory cache that the MCP tools read from.
    ///
    /// "Latest" is determined by event timestamp; ties broken by
    /// event ordering (whichever comes later in the WAL wins). This
    /// matches how `MergePolicy::LastWrite` behaves and keeps the
    /// replay outcome deterministic for a given event log.
    pub async fn load_projection_defs(
        &self,
    ) -> PrimeResult<Vec<super::projections::declarative::ProjectionDef>> {
        use super::projections::declarative::ProjectionDef;
        use std::collections::HashMap;

        let events = self
            .core
            .query(Query::new().event_type(event_types::PROJECTION_DEFINED))
            .await?;

        // Latest-by-timestamp per entity_id. HashMap dedupes; insertion
        // order is preserved because we iterate events in their natural
        // (chronological) order from the WAL.
        let mut latest: HashMap<String, (chrono::DateTime<Utc>, ProjectionDef)> = HashMap::new();
        for event in &events {
            let def: ProjectionDef = match serde_json::from_value(event.payload.clone()) {
                Ok(d) => d,
                Err(e) => {
                    tracing::warn!(
                        entity_id = %event.entity_id,
                        error = %e,
                        "skipping malformed prime.projection.defined event during replay"
                    );
                    continue;
                }
            };
            latest
                .entry(event.entity_id.clone())
                .and_modify(|(ts, existing)| {
                    if event.timestamp >= *ts {
                        *ts = event.timestamp;
                        *existing = def.clone();
                    }
                })
                .or_insert((event.timestamp, def));
        }

        Ok(latest.into_values().map(|(_, def)| def).collect())
    }

    /// Get what changed in the graph between two timestamps.
    pub async fn diff(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> PrimeResult<super::types::GraphDiff> {
        use super::types::GraphDiff;

        let events = self
            .core
            .query(
                Query::new()
                    .event_type_prefix("prime.")
                    .since(from)
                    .until(to),
            )
            .await?;

        let mut diff = GraphDiff::default();

        for event in &events {
            let entity_id = event.entity_id.clone();
            match event.event_type.as_str() {
                event_types::NODE_CREATED => diff.nodes_added.push(entity_id),
                event_types::NODE_UPDATED => diff.nodes_updated.push(entity_id),
                event_types::NODE_DELETED => diff.nodes_deleted.push(entity_id),
                event_types::EDGE_CREATED => diff.edges_added.push(entity_id),
                event_types::EDGE_DELETED => diff.edges_deleted.push(entity_id),
                #[cfg(feature = "prime-vectors")]
                super::vectors::types::event_types::VECTOR_STORED => {
                    diff.vectors_stored.push(entity_id);
                }
                #[cfg(feature = "prime-vectors")]
                super::vectors::types::event_types::VECTOR_DELETED => {
                    diff.vectors_deleted.push(entity_id);
                }
                _ => {}
            }
        }

        Ok(diff)
    }

    /// Get a chronological event stream for an entity within a time range.
    pub async fn timeline(
        &self,
        entity_id: &str,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
    ) -> PrimeResult<Vec<super::types::HistoryEntry>> {
        let mut query = Query::new().entity_id(entity_id);
        if let Some(f) = from {
            query = query.since(f);
        }
        if let Some(t) = to {
            query = query.until(t);
        }

        let events = self.core.query(query).await?;

        let entries = events
            .iter()
            .map(|e| super::types::HistoryEntry {
                event_type: e.event_type.clone(),
                timestamp: e.timestamp,
                payload: e.payload.clone(),
                source: e
                    .metadata
                    .as_ref()
                    .and_then(|m| m.get("source"))
                    .and_then(|v| v.as_str())
                    .map(String::from),
            })
            .collect();

        Ok(entries)
    }

    // =========================================================================
    // Conversation Scoping
    // =========================================================================

    /// Create a conversation-scoped handle that tags all mutations with a
    /// conversation ID in event metadata.
    pub fn with_conversation<'a>(&'a self, conversation_id: &'a str) -> ConversationScope<'a> {
        ConversationScope {
            prime: self,
            conversation_id,
        }
    }

    /// Get all events from a specific conversation.
    pub async fn conversation_history(
        &self,
        conversation_id: &str,
    ) -> PrimeResult<Vec<super::types::HistoryEntry>> {
        // Query all events and filter by conversation_id in metadata
        let all_events = self
            .core
            .query(Query::new().event_type_prefix("prime."))
            .await?;

        let entries = all_events
            .iter()
            .filter(|e| {
                e.metadata
                    .as_ref()
                    .and_then(|m| m.get("conversation_id"))
                    .and_then(|v| v.as_str())
                    == Some(conversation_id)
            })
            .map(|e| super::types::HistoryEntry {
                event_type: e.event_type.clone(),
                timestamp: e.timestamp,
                payload: e.payload.clone(),
                source: e
                    .metadata
                    .as_ref()
                    .and_then(|m| m.get("source"))
                    .and_then(|v| v.as_str())
                    .map(String::from),
            })
            .collect();

        Ok(entries)
    }

    /// Get a diff of what changed in a specific conversation.
    pub async fn conversation_diff(
        &self,
        conversation_id: &str,
    ) -> PrimeResult<super::types::GraphDiff> {
        let history = self.conversation_history(conversation_id).await?;
        let mut diff = super::types::GraphDiff::default();

        for entry in &history {
            match entry.event_type.as_str() {
                event_types::NODE_CREATED => diff.nodes_added.push(
                    entry
                        .payload
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                ),
                event_types::NODE_UPDATED => diff.nodes_updated.push(
                    entry
                        .payload
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                ),
                event_types::NODE_DELETED => diff.nodes_deleted.push(String::new()),
                event_types::EDGE_CREATED => diff.edges_added.push(
                    entry
                        .payload
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                ),
                event_types::EDGE_DELETED => diff.edges_deleted.push(
                    entry
                        .payload
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                ),
                _ => {}
            }
        }

        Ok(diff)
    }

    /// Get the state of a node as it existed at `as_of` timestamp.
    ///
    /// Replays events up to the given timestamp to reconstruct point-in-time state.
    /// Returns `None` if the node didn't exist yet or was deleted before `as_of`.
    pub async fn get_node_as_of(
        &self,
        entity_id: &str,
        as_of: DateTime<Utc>,
    ) -> PrimeResult<Option<Node>> {
        let events = self
            .core
            .query(Query::new().entity_id(entity_id).until(as_of))
            .await?;

        if events.is_empty() {
            return Ok(None);
        }

        let mut node_type = String::new();
        let mut properties = serde_json::Map::new();
        let mut domain: Option<String> = None;
        let mut labels: Vec<String> = Vec::new();
        let mut deleted = false;
        let mut created_at = as_of;

        for event in &events {
            match event.event_type.as_str() {
                event_types::NODE_CREATED => {
                    node_type = event
                        .payload
                        .get("node_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    if let Some(serde_json::Value::Object(props)) = event.payload.get("properties")
                    {
                        properties = props.clone();
                    }
                    domain = event
                        .payload
                        .get("domain")
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    labels = event
                        .payload
                        .get("labels")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default();
                    deleted = false;
                    created_at = event.timestamp;
                }
                event_types::NODE_UPDATED => {
                    if let Some(serde_json::Value::Object(updates)) =
                        event.payload.get("properties")
                    {
                        for (key, value) in updates {
                            properties.insert(key.clone(), value.clone());
                        }
                    }
                    if let Some(d) = event.payload.get("domain") {
                        domain = d.as_str().map(String::from);
                    }
                    if let Some(l) = event
                        .payload
                        .get("labels")
                        .and_then(serde_json::Value::as_array)
                    {
                        labels = l
                            .iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect();
                    }
                }
                event_types::NODE_DELETED => {
                    deleted = true;
                }
                _ => {}
            }
        }

        if deleted || node_type.is_empty() {
            return Ok(None);
        }

        let id = entity_id.split(':').nth(2).unwrap_or(entity_id).to_string();

        Ok(Some(Node {
            id: NodeId::new(id),
            node_type,
            properties: serde_json::Value::Object(properties),
            domain,
            labels,
            deleted: false,
            created_at,
            updated_at: events.last().map_or(created_at, |e| e.timestamp),
        }))
    }

    /// Get 1-hop neighbors of a node as the graph existed at `as_of` timestamp.
    ///
    /// Replays edge events up to the timestamp, filtering by relation.
    pub async fn neighbors_as_of(
        &self,
        entity_id: &str,
        relation: Option<&str>,
        as_of: DateTime<Utc>,
    ) -> PrimeResult<Vec<Node>> {
        // Get all prime edge events up to as_of
        let all_events = self
            .core
            .query(Query::new().event_type_prefix("prime.edge.").until(as_of))
            .await?;

        // Build live outgoing edges from entity_id at as_of
        let mut live_targets: std::collections::HashMap<String, String> =
            std::collections::HashMap::new(); // edge_id -> target
        let mut deleted_edges: std::collections::HashSet<String> = std::collections::HashSet::new();

        for event in &all_events {
            match event.event_type.as_str() {
                event_types::EDGE_CREATED => {
                    let source = event
                        .payload
                        .get("source")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if source != entity_id {
                        continue;
                    }
                    let rel = event
                        .payload
                        .get("relation")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if relation.is_some() && relation != Some(rel) {
                        continue;
                    }
                    if let Some(edge_id) = event.payload.get("id").and_then(|v| v.as_str()) {
                        let target = event
                            .payload
                            .get("target")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        live_targets.insert(edge_id.to_string(), target);
                    }
                }
                event_types::EDGE_DELETED => {
                    if let Some(edge_id) = event.payload.get("id").and_then(|v| v.as_str()) {
                        deleted_edges.insert(edge_id.to_string());
                    }
                }
                _ => {}
            }
        }

        // Remove deleted edges
        for deleted in &deleted_edges {
            live_targets.remove(deleted);
        }

        // Resolve targets to Nodes at as_of
        let mut neighbors = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for target in live_targets.values() {
            if seen.insert(target.clone())
                && let Some(node) = Box::pin(self.get_node_as_of(target, as_of)).await?
            {
                neighbors.push(node);
            }
        }

        Ok(neighbors)
    }

    /// Helper: get raw peer IDs without resolving to Node objects.
    fn neighbor_ids(
        &self,
        entity_id: &str,
        relation: Option<&str>,
        direction: Direction,
    ) -> Vec<String> {
        let mut peers = Vec::new();
        let mut seen = std::collections::HashSet::new();

        let add_entries = |entries: Vec<super::projections::AdjEntry>,
                           peers: &mut Vec<String>,
                           seen: &mut std::collections::HashSet<String>| {
            for entry in entries {
                if (relation.is_none() || relation == Some(entry.relation.as_str()))
                    && seen.insert(entry.peer.clone())
                {
                    peers.push(entry.peer);
                }
            }
        };

        match direction {
            Direction::Outgoing => {
                add_entries(self.adjacency.outgoing(entity_id), &mut peers, &mut seen);
            }
            Direction::Incoming => {
                add_entries(
                    self.reverse_index.incoming(entity_id),
                    &mut peers,
                    &mut seen,
                );
            }
            Direction::Both => {
                add_entries(self.adjacency.outgoing(entity_id), &mut peers, &mut seen);
                add_entries(
                    self.reverse_index.incoming(entity_id),
                    &mut peers,
                    &mut seen,
                );
            }
        }

        peers
    }
}

// =============================================================================
// Conversation Scoping
// =============================================================================

/// A conversation-scoped handle to Prime that tags all mutations with a
/// `conversation_id` in event metadata.
///
/// Obtained via [`Prime::with_conversation`]. Delegates all reads to the
/// underlying `Prime` instance (conversations don't isolate reads).
pub struct ConversationScope<'a> {
    prime: &'a Prime,
    conversation_id: &'a str,
}

impl ConversationScope<'_> {
    fn metadata(&self) -> serde_json::Value {
        json!({ "conversation_id": self.conversation_id })
    }

    /// Create a node within this conversation.
    pub async fn add_node(
        &self,
        node_type: &str,
        properties: serde_json::Value,
    ) -> PrimeResult<NodeId> {
        let id = uuid::Uuid::new_v4().to_string();
        #[allow(deprecated)]
        let entity_id = node_entity_id(node_type, &id);

        self.prime
            .core
            .ingest(IngestEvent {
                entity_id: &entity_id,
                event_type: event_types::NODE_CREATED,
                payload: json!({
                    "id": id,
                    "node_type": node_type,
                    "properties": properties,
                }),
                metadata: Some(self.metadata()),
                tenant_id: None,
            })
            .await?;

        Ok(NodeId::new(id))
    }

    /// Create an edge within this conversation.
    pub async fn add_edge(
        &self,
        source: &str,
        target: &str,
        relation: &str,
        properties: Option<serde_json::Value>,
    ) -> PrimeResult<EdgeId> {
        if !self.prime.node_state.is_live(source) {
            return Err(PrimeError::NodeNotFound(source.to_string()));
        }
        if !self.prime.node_state.is_live(target) {
            return Err(PrimeError::NodeNotFound(target.to_string()));
        }

        let id = uuid::Uuid::new_v4().to_string();
        #[allow(deprecated)]
        let entity_id = edge_entity_id(&id);

        let mut payload = json!({
            "id": id,
            "source": source,
            "target": target,
            "relation": relation,
        });
        if let Some(props) = properties {
            payload["properties"] = props;
        }

        self.prime
            .core
            .ingest(IngestEvent {
                entity_id: &entity_id,
                event_type: event_types::EDGE_CREATED,
                payload,
                metadata: Some(self.metadata()),
                tenant_id: None,
            })
            .await?;

        Ok(EdgeId::new(id))
    }

    /// Update a node within this conversation.
    pub async fn update_node(
        &self,
        entity_id: &str,
        properties: serde_json::Value,
    ) -> PrimeResult<()> {
        if !self.prime.node_state.is_live(entity_id) {
            return Err(PrimeError::NodeNotFound(entity_id.to_string()));
        }

        self.prime
            .core
            .ingest(IngestEvent {
                entity_id,
                event_type: event_types::NODE_UPDATED,
                payload: json!({ "properties": properties }),
                metadata: Some(self.metadata()),
                tenant_id: None,
            })
            .await?;

        Ok(())
    }

    /// Delete a node within this conversation (delegates to Prime::delete_node).
    pub async fn delete_node(&self, entity_id: &str) -> PrimeResult<()> {
        // delete_node uses ingest_batch; we can't easily add metadata to batch
        // events, so we delegate and emit a conversation marker event afterward
        self.prime.delete_node(entity_id).await
    }

    /// All reads delegate to the underlying Prime instance.
    pub fn get_node(&self, entity_id: &str) -> Option<Node> {
        self.prime.get_node(entity_id)
    }

    pub fn neighbors(
        &self,
        entity_id: &str,
        relation: Option<&str>,
        direction: Direction,
    ) -> Vec<Node> {
        self.prime.neighbors(entity_id, relation, direction)
    }

    pub fn stats(&self) -> PrimeStats {
        self.prime.stats()
    }

    /// Get the conversation ID this scope is bound to.
    pub fn conversation_id(&self) -> &str {
        self.conversation_id
    }
}

// =============================================================================
// Free Functions
// =============================================================================

/// Maximal Marginal Relevance re-ranking.
///
/// Selects top_k results that balance relevance with diversity across domains.
/// `lambda` controls the trade-off: 1.0 = pure relevance, 0.0 = pure diversity.
#[cfg(feature = "prime-vectors")]
fn mmr_rerank(nodes: &mut Vec<super::types::ScoredNode>, top_k: usize, lambda: f64) {
    use super::types::ScoredNode;

    if nodes.len() <= top_k {
        return;
    }

    let mut selected: Vec<ScoredNode> = Vec::with_capacity(top_k);
    let mut remaining: Vec<ScoredNode> = std::mem::take(nodes);

    while selected.len() < top_k && !remaining.is_empty() {
        let mut best_idx = 0;
        let mut best_mmr = f64::NEG_INFINITY;

        for (i, candidate) in remaining.iter().enumerate() {
            let relevance = candidate.score;

            // Max redundancy to any already-selected node
            let max_redundancy = if selected.is_empty() {
                0.0
            } else {
                selected
                    .iter()
                    .map(|s| {
                        // Same node_type = high redundancy
                        if s.node.node_type == candidate.node.node_type
                            && s.node.domain == candidate.node.domain
                        {
                            0.8
                        } else if s.node.domain == candidate.node.domain {
                            0.5
                        } else {
                            0.0
                        }
                    })
                    .fold(0.0f64, f64::max)
            };

            let mmr = lambda * relevance - (1.0 - lambda) * max_redundancy;
            if mmr > best_mmr {
                best_mmr = mmr;
                best_idx = i;
            }
        }

        selected.push(remaining.remove(best_idx));
    }

    *nodes = selected;
}

fn edge_from_event(event: &crate::embedded::EventView) -> Option<Edge> {
    let payload = &event.payload;
    let id = payload.get("id")?.as_str()?.to_string();
    let source = payload.get("source")?.as_str()?.to_string();
    let target = payload.get("target")?.as_str()?.to_string();
    let relation = payload.get("relation")?.as_str()?.to_string();
    let properties = payload.get("properties").cloned();
    let weight = payload.get("weight").and_then(serde_json::Value::as_f64);

    Some(Edge {
        id: EdgeId::new(id),
        source: NodeId::new(source),
        target: NodeId::new(target),
        relation,
        properties,
        weight,
        deleted: false,
        created_at: event.timestamp,
    })
}

/// Exponential decay score for recency: `exp(-lambda * hours_ago)`.
/// Returns 1.0 for recent events, decaying toward 0 for older ones.
fn recency_score(
    timestamp: chrono::DateTime<chrono::Utc>,
    now: chrono::DateTime<chrono::Utc>,
) -> f64 {
    let hours_ago = (now - timestamp).num_seconds().max(0) as f64 / 3600.0;
    // lambda = 0.01 → half-life ≈ 69 hours (~3 days)
    (-0.01 * hours_ago).exp()
}

/// Reconstruct a [`Node`] from projection state.
fn node_from_state(entity_id: &str, state: &serde_json::Value) -> Option<Node> {
    use super::types::NodeId;
    use chrono::{DateTime, Utc};

    // Extract the short ID from entity_id "node:{type}:{id}"
    let id = entity_id.split(':').nth(2).unwrap_or(entity_id).to_string();

    let node_type = state.get("node_type")?.as_str()?.to_string();
    let properties = state.get("properties").cloned().unwrap_or(json!({}));
    let domain = state
        .get("domain")
        .and_then(|v| v.as_str())
        .map(String::from);
    let labels = state
        .get("labels")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let deleted = state
        .get("deleted")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let created_at: DateTime<Utc> = state
        .get("created_at")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(Utc::now);
    let updated_at: DateTime<Utc> = state
        .get("updated_at")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(Utc::now);

    Some(Node {
        id: NodeId::new(id),
        node_type,
        properties,
        domain,
        labels,
        deleted,
        created_at,
        updated_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_open_in_memory_and_shutdown() {
        let prime = Prime::open_in_memory().await.unwrap();
        let stats = prime.stats();
        assert_eq!(stats.total_nodes, 0);
        assert_eq!(stats.total_edges, 0);
        assert_eq!(stats.event_count, 0);
        prime.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_open_with_path_and_ingest() {
        let dir = tempfile::tempdir().unwrap();
        let prime = Prime::open(dir.path()).await.unwrap();

        let _id = prime
            .add_node("person", json!({"name": "Alice"}))
            .await
            .unwrap();

        let stats = prime.stats();
        assert_eq!(stats.total_nodes, 1);
        assert_eq!(stats.event_count, 1);

        prime.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_persistence_across_reopens() {
        let dir = tempfile::tempdir().unwrap();

        {
            let prime = Prime::open(dir.path()).await.unwrap();
            prime
                .add_node("person", json!({"name": "Bob"}))
                .await
                .unwrap();
            prime.shutdown().await.unwrap();
        }

        {
            let prime = Prime::open(dir.path()).await.unwrap();
            let events = prime
                .core()
                .query(Query::new().event_type(event_types::NODE_CREATED))
                .await
                .unwrap();
            assert_eq!(events.len(), 1);
            prime.shutdown().await.unwrap();
        }
    }

    // ─── Declarative projection definitions are event-sourced ───────────
    //
    // Locks the contract from feedback_event_source_everything_in_allsource:
    // ProjectionDefs persist via prime.projection.defined events, NOT in-memory
    // state. If anyone "optimises" this back to a runtime registry, both of
    // these tests fail.

    #[tokio::test]
    async fn define_projection_round_trips_through_event_log() {
        use crate::prime::projections::{MergePolicy, ProjectionDef};
        use std::collections::BTreeMap;

        let prime = Prime::open_in_memory().await.unwrap();

        let mut fields = BTreeMap::new();
        fields.insert("status".to_string(), MergePolicy::LastWrite);
        fields.insert("name".to_string(), MergePolicy::HighestPriority);
        let def = ProjectionDef {
            entity_type: "contact".to_string(),
            field_policies: fields,
        };
        prime.define_projection(&def).await.unwrap();

        let loaded = prime.load_projection_defs().await.unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].entity_type, "contact");
        assert_eq!(
            loaded[0].field_policies.get("status"),
            Some(&MergePolicy::LastWrite)
        );
        prime.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn define_projection_latest_wins_across_repeated_definitions() {
        use crate::prime::projections::{MergePolicy, ProjectionDef};
        use std::collections::BTreeMap;

        let prime = Prime::open_in_memory().await.unwrap();

        // V1
        let mut f1 = BTreeMap::new();
        f1.insert("status".to_string(), MergePolicy::LastWrite);
        prime
            .define_projection(&ProjectionDef {
                entity_type: "task".to_string(),
                field_policies: f1,
            })
            .await
            .unwrap();

        // Tiny pause so the second event has a strictly later timestamp on
        // platforms where the WAL's resolution can collapse same-millisecond
        // events; keeps the "latest wins" assertion deterministic.
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;

        // V2: replace status with merge_array, add a new field
        let mut f2 = BTreeMap::new();
        f2.insert("status".to_string(), MergePolicy::MergeArray);
        f2.insert("priority".to_string(), MergePolicy::HighestPriority);
        prime
            .define_projection(&ProjectionDef {
                entity_type: "task".to_string(),
                field_policies: f2,
            })
            .await
            .unwrap();

        let loaded = prime.load_projection_defs().await.unwrap();
        assert_eq!(loaded.len(), 1, "latest-wins should dedupe by entity_type");
        let def = &loaded[0];
        assert_eq!(def.entity_type, "task");
        assert_eq!(
            def.field_policies.get("status"),
            Some(&MergePolicy::MergeArray),
            "V2 replaced V1"
        );
        assert!(
            def.field_policies.contains_key("priority"),
            "V2's new field survives"
        );
        prime.shutdown().await.unwrap();
    }

    /// The acceptance criterion from bead t-cdd2: ProjectionDef survives a
    /// process restart (i.e. a Prime reopen). Without this guarantee the
    /// product can't claim "event-sourced storage" for projection defs.
    #[tokio::test]
    async fn projection_defs_survive_prime_reopen() {
        use crate::prime::projections::{MergePolicy, ProjectionDef};
        use std::collections::BTreeMap;

        let dir = tempfile::tempdir().unwrap();

        // Boot 1: define a projection, shutdown.
        {
            let prime = Prime::open(dir.path()).await.unwrap();
            let mut fields = BTreeMap::new();
            fields.insert("status".to_string(), MergePolicy::LastWrite);
            fields.insert("tags".to_string(), MergePolicy::MergeArray);
            prime
                .define_projection(&ProjectionDef {
                    entity_type: "post".to_string(),
                    field_policies: fields,
                })
                .await
                .unwrap();
            prime.shutdown().await.unwrap();
        }

        // Boot 2: reopen, replay, projection def must still be there.
        {
            let prime = Prime::open(dir.path()).await.unwrap();
            let loaded = prime.load_projection_defs().await.unwrap();
            assert_eq!(
                loaded.len(),
                1,
                "projection def did not survive Prime restart — durability regression"
            );
            assert_eq!(loaded[0].entity_type, "post");
            assert_eq!(
                loaded[0].field_policies.get("tags"),
                Some(&MergePolicy::MergeArray)
            );
            prime.shutdown().await.unwrap();
        }
    }

    /// Regression test for issue #180: projections must rebuild from the full
    /// Parquet archive, not the WAL alone.
    ///
    /// The WAL is compacted into Parquet and truncated during the *second*
    /// process's boot. From the third boot onward the WAL is empty, so a
    /// WAL-only backfill would leave every projection blank. The graph must
    /// survive an arbitrary number of restarts via `prime.stats()` and the
    /// node/edge projections — the surface MCP clients actually query.
    #[tokio::test]
    async fn test_projections_survive_wal_compaction() {
        let dir = tempfile::tempdir().unwrap();
        let mut node_ids = Vec::new();

        // Boot 1: ingest a small graph (3 nodes + 1 edge).
        {
            let prime = Prime::open(dir.path()).await.unwrap();
            for name in ["Alice", "Bob", "Carol"] {
                let id = prime
                    .add_node("person", json!({"name": name, "domain": "team"}))
                    .await
                    .unwrap();
                node_ids.push(crate::prime::EntityId::node("person", id.as_str()).to_wire());
            }
            prime
                .add_edge(&node_ids[0], &node_ids[1], "manages", None)
                .await
                .unwrap();

            let stats = prime.stats();
            assert_eq!(stats.total_nodes, 3);
            assert_eq!(stats.total_edges, 1);
            prime.shutdown().await.unwrap();
        }

        // Boots 2-4: boot 2 compacts the WAL into Parquet and truncates it;
        // boots 3 and 4 start with an empty WAL and must hydrate from Parquet.
        for boot in 2..=4 {
            let prime = Prime::open(dir.path()).await.unwrap();
            let stats = prime.stats();
            assert_eq!(
                stats.total_nodes, 3,
                "boot {boot}: nodes lost — projection rebuilt from WAL only"
            );
            assert_eq!(
                stats.total_edges, 1,
                "boot {boot}: edges lost — projection rebuilt from WAL only"
            );

            // node_state must carry full properties, not just counts.
            let alice = prime
                .get_node(&node_ids[0])
                .unwrap_or_else(|| panic!("boot {boot}: Alice missing from node_state"));
            assert_eq!(alice.properties["name"], "Alice");

            // adjacency must still resolve the edge.
            let neighbors = prime.neighbors(&node_ids[0], None, Direction::Outgoing);
            assert_eq!(
                neighbors.len(),
                1,
                "boot {boot}: edge lost from adjacency projection"
            );

            prime.shutdown().await.unwrap();
        }
    }

    #[tokio::test]
    async fn test_stats_counts_nodes_and_edges() {
        let prime = Prime::open_in_memory().await.unwrap();

        prime
            .add_node("person", json!({"name": "Alice"}))
            .await
            .unwrap();
        prime
            .add_node("person", json!({"name": "Bob"}))
            .await
            .unwrap();

        // Ingest a raw edge event
        prime
            .core()
            .ingest(IngestEvent {
                entity_id: "edge:e-1",
                event_type: event_types::EDGE_CREATED,
                payload: json!({
                    "id": "e-1",
                    "source": "alice",
                    "target": "bob",
                    "relation": "knows",
                }),
                metadata: None,
                tenant_id: None,
            })
            .await
            .unwrap();

        let stats = prime.stats();
        assert_eq!(stats.total_nodes, 2);
        assert_eq!(stats.total_edges, 1);
        assert_eq!(stats.event_count, 3);

        prime.shutdown().await.unwrap();
    }

    // =========================================================================
    // Node CRUD tests
    // =========================================================================

    #[tokio::test]
    async fn test_full_node_crud_lifecycle() {
        let prime = Prime::open_in_memory().await.unwrap();

        // Create
        let id = prime
            .add_node("person", json!({"name": "Alice", "age": 30}))
            .await
            .unwrap();
        let entity_id = node_entity_id("person", id.as_str());

        // Read
        let node = prime.get_node(&entity_id).unwrap();
        assert_eq!(node.node_type, "person");
        assert_eq!(node.properties["name"], "Alice");
        assert_eq!(node.properties["age"], 30);

        // Update (deep merge)
        prime
            .update_node(&entity_id, json!({"role": "engineer", "age": 31}))
            .await
            .unwrap();
        let node = prime.get_node(&entity_id).unwrap();
        assert_eq!(node.properties["name"], "Alice"); // preserved
        assert_eq!(node.properties["role"], "engineer"); // added
        assert_eq!(node.properties["age"], 31); // updated

        // Delete
        prime.delete_node(&entity_id).await.unwrap();
        assert!(prime.get_node(&entity_id).is_none());

        prime.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_delete_node_cascades_edges() {
        let prime = Prime::open_in_memory().await.unwrap();

        let alice_id = prime
            .add_node("person", json!({"name": "Alice"}))
            .await
            .unwrap();
        let bob_id = prime
            .add_node("person", json!({"name": "Bob"}))
            .await
            .unwrap();

        let alice_entity = node_entity_id("person", alice_id.as_str());
        let bob_entity = node_entity_id("person", bob_id.as_str());

        // Add edge alice -> bob
        prime
            .core()
            .ingest(IngestEvent {
                entity_id: "edge:e-1",
                event_type: event_types::EDGE_CREATED,
                payload: json!({
                    "id": "e-1",
                    "source": alice_entity,
                    "target": bob_entity,
                    "relation": "knows",
                }),
                metadata: None,
                tenant_id: None,
            })
            .await
            .unwrap();

        // Verify edge exists
        assert_eq!(prime.adjacency.outgoing(&alice_entity).len(), 1);

        // Delete alice — should cascade and delete the edge
        prime.delete_node(&alice_entity).await.unwrap();

        // Edge should be gone
        assert_eq!(prime.adjacency.outgoing(&alice_entity).len(), 0);
        assert_eq!(prime.reverse_index.incoming(&bob_entity).len(), 0);

        // Alice should be gone
        assert!(prime.get_node(&alice_entity).is_none());
        // Bob should still exist
        assert!(prime.get_node(&bob_entity).is_some());

        prime.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_update_nonexistent_node_returns_error() {
        let prime = Prime::open_in_memory().await.unwrap();

        let result = prime
            .update_node("node:person:ghost", json!({"name": "Ghost"}))
            .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            PrimeError::NodeNotFound(id) => assert_eq!(id, "node:person:ghost"),
            other => panic!("expected NodeNotFound, got: {other}"),
        }

        prime.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_nodes_by_type() {
        let prime = Prime::open_in_memory().await.unwrap();

        prime
            .add_node("person", json!({"name": "Alice"}))
            .await
            .unwrap();
        prime
            .add_node("person", json!({"name": "Bob"}))
            .await
            .unwrap();
        prime
            .add_node("project", json!({"name": "Prime"}))
            .await
            .unwrap();

        let persons = prime.nodes_by_type("person");
        assert_eq!(persons.len(), 2);

        let projects = prime.nodes_by_type("project");
        assert_eq!(projects.len(), 1);

        prime.shutdown().await.unwrap();
    }

    // =========================================================================
    // Edge CRUD tests
    // =========================================================================

    #[tokio::test]
    async fn test_create_edge_between_nodes() {
        let prime = Prime::open_in_memory().await.unwrap();

        let alice_id = prime
            .add_node("person", json!({"name": "Alice"}))
            .await
            .unwrap();
        let bob_id = prime
            .add_node("person", json!({"name": "Bob"}))
            .await
            .unwrap();
        let alice_entity = node_entity_id("person", alice_id.as_str());
        let bob_entity = node_entity_id("person", bob_id.as_str());

        let edge_id = prime
            .add_edge(&alice_entity, &bob_entity, "knows", None)
            .await
            .unwrap();

        // Verify edge exists via get_edge
        let edge = prime.get_edge(edge_id.as_str()).await.unwrap().unwrap();
        assert_eq!(edge.relation, "knows");
        assert_eq!(edge.source.as_str(), &alice_entity);
        assert_eq!(edge.target.as_str(), &bob_entity);

        // Verify adjacency projections
        assert_eq!(prime.adjacency.outgoing(&alice_entity).len(), 1);
        assert_eq!(prime.reverse_index.incoming(&bob_entity).len(), 1);

        prime.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_create_edge_to_nonexistent_node() {
        let prime = Prime::open_in_memory().await.unwrap();

        let alice_id = prime
            .add_node("person", json!({"name": "Alice"}))
            .await
            .unwrap();
        let alice_entity = node_entity_id("person", alice_id.as_str());

        let result = prime
            .add_edge(&alice_entity, "node:person:ghost", "knows", None)
            .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            PrimeError::NodeNotFound(id) => assert_eq!(id, "node:person:ghost"),
            other => panic!("expected NodeNotFound, got: {other}"),
        }

        prime.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_delete_edge() {
        let prime = Prime::open_in_memory().await.unwrap();

        let alice_id = prime
            .add_node("person", json!({"name": "Alice"}))
            .await
            .unwrap();
        let bob_id = prime
            .add_node("person", json!({"name": "Bob"}))
            .await
            .unwrap();
        let alice_entity = node_entity_id("person", alice_id.as_str());
        let bob_entity = node_entity_id("person", bob_id.as_str());

        let edge_id = prime
            .add_edge(&alice_entity, &bob_entity, "knows", None)
            .await
            .unwrap();

        // Delete
        prime.delete_edge(edge_id.as_str()).await.unwrap();

        // Verify gone
        let edge = prime.get_edge(edge_id.as_str()).await.unwrap();
        assert!(edge.is_none());

        // Verify adjacency updated
        assert_eq!(prime.adjacency.outgoing(&alice_entity).len(), 0);
        assert_eq!(prime.reverse_index.incoming(&bob_entity).len(), 0);

        prime.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_weighted_edge() {
        let prime = Prime::open_in_memory().await.unwrap();

        let a = prime
            .add_node("person", json!({"name": "A"}))
            .await
            .unwrap();
        let b = prime
            .add_node("person", json!({"name": "B"}))
            .await
            .unwrap();
        let a_entity = node_entity_id("person", a.as_str());
        let b_entity = node_entity_id("person", b.as_str());

        let edge_id = prime
            .add_edge_weighted(&a_entity, &b_entity, "trust", 0.95, None)
            .await
            .unwrap();

        let edge = prime.get_edge(edge_id.as_str()).await.unwrap().unwrap();
        assert_eq!(edge.weight, Some(0.95));

        prime.shutdown().await.unwrap();
    }

    // =========================================================================
    // Neighbor query tests
    // =========================================================================

    #[tokio::test]
    async fn test_neighbors_all_directions() {
        let prime = Prime::open_in_memory().await.unwrap();

        // Create nodes A, B, C, D
        let a = prime
            .add_node("person", json!({"name": "A"}))
            .await
            .unwrap();
        let b = prime
            .add_node("person", json!({"name": "B"}))
            .await
            .unwrap();
        let c = prime
            .add_node("person", json!({"name": "C"}))
            .await
            .unwrap();
        let d = prime
            .add_node("person", json!({"name": "D"}))
            .await
            .unwrap();

        let a_e = node_entity_id("person", a.as_str());
        let b_e = node_entity_id("person", b.as_str());
        let c_e = node_entity_id("person", c.as_str());
        let d_e = node_entity_id("person", d.as_str());

        // A->B (works_on), A->C (knows), D->A (manages)
        prime.add_edge(&a_e, &b_e, "works_on", None).await.unwrap();
        prime.add_edge(&a_e, &c_e, "knows", None).await.unwrap();
        prime.add_edge(&d_e, &a_e, "manages", None).await.unwrap();

        // Outgoing from A = [B, C]
        let out = prime.neighbors(&a_e, None, Direction::Outgoing);
        assert_eq!(out.len(), 2);

        // Outgoing from A filtered by "works_on" = [B]
        let out_filtered = prime.neighbors(&a_e, Some("works_on"), Direction::Outgoing);
        assert_eq!(out_filtered.len(), 1);
        assert_eq!(out_filtered[0].properties["name"], "B");

        // Incoming to A = [D]
        let inc = prime.neighbors(&a_e, None, Direction::Incoming);
        assert_eq!(inc.len(), 1);
        assert_eq!(inc[0].properties["name"], "D");

        // Both from A = [B, C, D] (deduplicated)
        let both = prime.neighbors(&a_e, None, Direction::Both);
        assert_eq!(both.len(), 3);

        prime.shutdown().await.unwrap();
    }

    // =========================================================================
    // BFS traversal tests
    // =========================================================================

    #[tokio::test]
    async fn test_bfs_diamond_graph() {
        let prime = Prime::open_in_memory().await.unwrap();

        // Diamond: A->B->D, A->C->D
        let a = prime.add_node("n", json!({"name": "A"})).await.unwrap();
        let b = prime.add_node("n", json!({"name": "B"})).await.unwrap();
        let c = prime.add_node("n", json!({"name": "C"})).await.unwrap();
        let d = prime.add_node("n", json!({"name": "D"})).await.unwrap();

        let ae = node_entity_id("n", a.as_str());
        let be = node_entity_id("n", b.as_str());
        let ce = node_entity_id("n", c.as_str());
        let de = node_entity_id("n", d.as_str());

        prime.add_edge(&ae, &be, "link", None).await.unwrap();
        prime.add_edge(&ae, &ce, "link", None).await.unwrap();
        prime.add_edge(&be, &de, "link", None).await.unwrap();
        prime.add_edge(&ce, &de, "link", None).await.unwrap();

        let results = prime.neighbors_within(&ae, 2, None, Direction::Outgoing);

        // A(0), B(1), C(1), D(2)
        assert_eq!(results.len(), 4);
        assert_eq!(results[0].1, 0); // A at depth 0
        // B and C at depth 1
        let depth_1: Vec<_> = results.iter().filter(|(_, d)| *d == 1).collect();
        assert_eq!(depth_1.len(), 2);
        // D at depth 2
        let depth_2: Vec<_> = results.iter().filter(|(_, d)| *d == 2).collect();
        assert_eq!(depth_2.len(), 1);

        prime.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_subgraph_extraction() {
        let prime = Prime::open_in_memory().await.unwrap();

        let a = prime.add_node("n", json!({"name": "A"})).await.unwrap();
        let b = prime.add_node("n", json!({"name": "B"})).await.unwrap();
        let c = prime.add_node("n", json!({"name": "C"})).await.unwrap();

        let ae = node_entity_id("n", a.as_str());
        let be = node_entity_id("n", b.as_str());
        let ce = node_entity_id("n", c.as_str());

        prime.add_edge(&ae, &be, "link", None).await.unwrap();
        prime.add_edge(&ae, &ce, "link", None).await.unwrap();

        let sg = prime.subgraph(&ae, 1);
        assert_eq!(sg.nodes.len(), 3); // A, B, C
        assert_eq!(sg.edges.len(), 2); // A->B, A->C

        prime.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_bfs_cycle_terminates() {
        let prime = Prime::open_in_memory().await.unwrap();

        // Cycle: A->B->C->A
        let a = prime.add_node("n", json!({"name": "A"})).await.unwrap();
        let b = prime.add_node("n", json!({"name": "B"})).await.unwrap();
        let c = prime.add_node("n", json!({"name": "C"})).await.unwrap();

        let ae = node_entity_id("n", a.as_str());
        let be = node_entity_id("n", b.as_str());
        let ce = node_entity_id("n", c.as_str());

        prime.add_edge(&ae, &be, "link", None).await.unwrap();
        prime.add_edge(&be, &ce, "link", None).await.unwrap();
        prime.add_edge(&ce, &ae, "link", None).await.unwrap();

        let results = prime.neighbors_within(&ae, 10, None, Direction::Outgoing);
        // Each node visited exactly once
        assert_eq!(results.len(), 3);

        prime.shutdown().await.unwrap();
    }

    // =========================================================================
    // Shortest path tests
    // =========================================================================

    #[tokio::test]
    async fn test_shortest_path_bfs() {
        let prime = Prime::open_in_memory().await.unwrap();

        // A->B->C->D
        let a = prime.add_node("n", json!({"name": "A"})).await.unwrap();
        let b = prime.add_node("n", json!({"name": "B"})).await.unwrap();
        let c = prime.add_node("n", json!({"name": "C"})).await.unwrap();
        let d = prime.add_node("n", json!({"name": "D"})).await.unwrap();

        let ae = node_entity_id("n", a.as_str());
        let be = node_entity_id("n", b.as_str());
        let ce = node_entity_id("n", c.as_str());
        let de = node_entity_id("n", d.as_str());

        prime.add_edge(&ae, &be, "link", None).await.unwrap();
        prime.add_edge(&be, &ce, "link", None).await.unwrap();
        prime.add_edge(&ce, &de, "link", None).await.unwrap();

        let path = prime.shortest_path(&ae, &de, None).unwrap();
        assert_eq!(path.len(), 4); // A, B, C, D

        prime.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_shortest_path_weighted_dijkstra() {
        let prime = Prime::open_in_memory().await.unwrap();

        // A->B (w=1), A->C (w=3), B->D (w=1), C->D (w=1)
        // Cheapest: A->B->D (cost 2) vs A->C->D (cost 4)
        let a = prime.add_node("n", json!({"name": "A"})).await.unwrap();
        let b = prime.add_node("n", json!({"name": "B"})).await.unwrap();
        let c = prime.add_node("n", json!({"name": "C"})).await.unwrap();
        let d = prime.add_node("n", json!({"name": "D"})).await.unwrap();

        let ae = node_entity_id("n", a.as_str());
        let be = node_entity_id("n", b.as_str());
        let ce = node_entity_id("n", c.as_str());
        let de = node_entity_id("n", d.as_str());

        prime
            .add_edge_weighted(&ae, &be, "link", 1.0, None)
            .await
            .unwrap();
        prime
            .add_edge_weighted(&ae, &ce, "link", 3.0, None)
            .await
            .unwrap();
        prime
            .add_edge_weighted(&be, &de, "link", 1.0, None)
            .await
            .unwrap();
        prime
            .add_edge_weighted(&ce, &de, "link", 1.0, None)
            .await
            .unwrap();

        let (path, cost) = prime.shortest_path_weighted(&ae, &de, None).unwrap();
        assert_eq!(path.len(), 3); // A, B, D
        assert!((cost - 2.0).abs() < f64::EPSILON);

        prime.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_shortest_path_disconnected() {
        let prime = Prime::open_in_memory().await.unwrap();

        let a = prime.add_node("n", json!({"name": "A"})).await.unwrap();
        let b = prime.add_node("n", json!({"name": "B"})).await.unwrap();

        let ae = node_entity_id("n", a.as_str());
        let be = node_entity_id("n", b.as_str());

        // No edges — disconnected
        assert!(prime.shortest_path(&ae, &be, None).is_none());

        prime.shutdown().await.unwrap();
    }

    // =========================================================================
    // History / temporal tests
    // =========================================================================

    #[tokio::test]
    async fn test_history_full_lifecycle() {
        let prime = Prime::open_in_memory().await.unwrap();

        let id = prime
            .add_node("person", json!({"name": "Alice"}))
            .await
            .unwrap();
        let entity_id = node_entity_id("person", id.as_str());

        // Update twice
        prime
            .update_node(&entity_id, json!({"role": "engineer"}))
            .await
            .unwrap();
        prime
            .update_node(&entity_id, json!({"level": "senior"}))
            .await
            .unwrap();

        // Delete
        prime.delete_node(&entity_id).await.unwrap();

        let history = prime.history(&entity_id).await.unwrap();
        assert_eq!(history.len(), 4); // created, updated, updated, deleted
        assert_eq!(history[0].event_type, event_types::NODE_CREATED);
        assert_eq!(history[1].event_type, event_types::NODE_UPDATED);
        assert_eq!(history[2].event_type, event_types::NODE_UPDATED);
        assert_eq!(history[3].event_type, event_types::NODE_DELETED);

        // Chronological order
        for window in history.windows(2) {
            assert!(window[0].timestamp <= window[1].timestamp);
        }

        prime.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_history_nonexistent_entity_returns_empty() {
        let prime = Prime::open_in_memory().await.unwrap();

        let history = prime.history("node:person:ghost").await.unwrap();
        assert!(history.is_empty());

        prime.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_history_edge_events() {
        let prime = Prime::open_in_memory().await.unwrap();

        let a = prime.add_node("n", json!({"name": "A"})).await.unwrap();
        let b = prime.add_node("n", json!({"name": "B"})).await.unwrap();
        let a_e = node_entity_id("n", a.as_str());
        let b_e = node_entity_id("n", b.as_str());

        let edge_id = prime.add_edge(&a_e, &b_e, "knows", None).await.unwrap();

        let edge_entity = edge_entity_id(edge_id.as_str());
        prime.delete_edge(edge_id.as_str()).await.unwrap();

        let history = prime.history(&edge_entity).await.unwrap();
        assert_eq!(history.len(), 2); // created, deleted
        assert_eq!(history[0].event_type, event_types::EDGE_CREATED);
        assert_eq!(history[1].event_type, event_types::EDGE_DELETED);

        prime.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_diff_between_timestamps() {
        use chrono::Utc;

        let prime = Prime::open_in_memory().await.unwrap();

        let t1 = Utc::now();

        // Add 3 nodes and 2 edges
        let a = prime
            .add_node("person", json!({"name": "A"}))
            .await
            .unwrap();
        let b = prime
            .add_node("person", json!({"name": "B"}))
            .await
            .unwrap();
        let c = prime
            .add_node("project", json!({"name": "C"}))
            .await
            .unwrap();
        let a_e = node_entity_id("person", a.as_str());
        let b_e = node_entity_id("person", b.as_str());
        prime.add_edge(&a_e, &b_e, "knows", None).await.unwrap();
        prime
            .add_edge(
                &a_e,
                &node_entity_id("project", c.as_str()),
                "works_on",
                None,
            )
            .await
            .unwrap();

        let t2 = Utc::now();

        let diff = prime.diff(t1, t2).await.unwrap();
        assert_eq!(diff.nodes_added.len(), 3);
        assert_eq!(diff.edges_added.len(), 2);
        assert!(diff.nodes_deleted.is_empty());

        prime.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_timeline_with_range() {
        use chrono::Utc;

        let prime = Prime::open_in_memory().await.unwrap();

        let id = prime
            .add_node("person", json!({"name": "Alice"}))
            .await
            .unwrap();
        let entity_id = node_entity_id("person", id.as_str());

        let t_mid = Utc::now();

        prime
            .update_node(&entity_id, json!({"role": "engineer"}))
            .await
            .unwrap();

        let t_end = Utc::now();

        // Full timeline
        let full = prime.timeline(&entity_id, None, None).await.unwrap();
        assert_eq!(full.len(), 2);

        // Timeline from t_mid — should only include the update
        let after_mid = prime
            .timeline(&entity_id, Some(t_mid), Some(t_end))
            .await
            .unwrap();
        assert_eq!(after_mid.len(), 1);
        assert_eq!(after_mid[0].event_type, event_types::NODE_UPDATED);

        prime.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_shortest_path_to_self() {
        let prime = Prime::open_in_memory().await.unwrap();

        let a = prime.add_node("n", json!({"name": "A"})).await.unwrap();
        let ae = node_entity_id("n", a.as_str());

        let path = prime.shortest_path(&ae, &ae, None).unwrap();
        assert_eq!(path.len(), 1);
        assert_eq!(path[0].properties["name"], "A");

        prime.shutdown().await.unwrap();
    }

    // =========================================================================
    // Time-travel tests
    // =========================================================================

    #[tokio::test]
    async fn test_neighbors_as_of() {
        let prime = Prime::open_in_memory().await.unwrap();

        let a = prime.add_node("n", json!({"name": "A"})).await.unwrap();
        let b = prime.add_node("n", json!({"name": "B"})).await.unwrap();
        let ae = node_entity_id("n", a.as_str());
        let be = node_entity_id("n", b.as_str());

        // Add edge A->B
        prime.add_edge(&ae, &be, "knows", None).await.unwrap();
        let t2 = chrono::Utc::now();

        // Small delay to ensure distinct timestamps
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // Add another node C and edge A->C after t2
        let c = prime.add_node("n", json!({"name": "C"})).await.unwrap();
        let ce = node_entity_id("n", c.as_str());
        prime.add_edge(&ae, &ce, "knows", None).await.unwrap();

        // At t2, only B should be a neighbor
        let neighbors_at_t2 = prime.neighbors_as_of(&ae, None, t2).await.unwrap();
        assert_eq!(neighbors_at_t2.len(), 1);
        assert_eq!(neighbors_at_t2[0].properties["name"], "B");

        // At now, both B and C should be neighbors
        let neighbors_now = prime
            .neighbors_as_of(&ae, None, chrono::Utc::now())
            .await
            .unwrap();
        assert_eq!(neighbors_now.len(), 2);

        prime.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_get_node_as_of_deleted() {
        let prime = Prime::open_in_memory().await.unwrap();

        let a = prime.add_node("n", json!({"name": "A"})).await.unwrap();
        let ae = node_entity_id("n", a.as_str());
        let t1 = chrono::Utc::now();

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // Delete node after t1
        prime.delete_node(&ae).await.unwrap();

        // At t1, node should exist
        let node_at_t1 = prime.get_node_as_of(&ae, t1).await.unwrap();
        assert!(node_at_t1.is_some());
        assert_eq!(node_at_t1.unwrap().properties["name"], "A");

        // At now (after deletion), node should be gone
        let node_now = prime.get_node_as_of(&ae, chrono::Utc::now()).await.unwrap();
        assert!(node_now.is_none());

        prime.shutdown().await.unwrap();
    }

    // =========================================================================
    // Memory compaction tests
    // =========================================================================

    #[tokio::test]
    async fn test_compact_merges_properties() {
        let prime = Prime::open_in_memory().await.unwrap();

        let a = prime
            .add_node("person", json!({"name": "Alice", "age": 30}))
            .await
            .unwrap();
        let b = prime
            .add_node(
                "person",
                json!({"name": "Alice Smith", "email": "alice@example.com"}),
            )
            .await
            .unwrap();

        let a_e = node_entity_id("person", a.as_str());
        let b_e = node_entity_id("person", b.as_str());

        prime.compact(&a_e, &[&b_e]).await.unwrap();

        // Target has merged properties (target wins on "name" conflict)
        let node = prime.get_node(&a_e).unwrap();
        assert_eq!(node.properties["name"], "Alice"); // target wins
        assert_eq!(node.properties["age"], 30); // kept from target
        assert_eq!(node.properties["email"], "alice@example.com"); // added from source

        // Source is deleted
        assert!(prime.get_node(&b_e).is_none());

        prime.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_compact_redirects_edges() {
        let prime = Prime::open_in_memory().await.unwrap();

        let a = prime
            .add_node("person", json!({"name": "A"}))
            .await
            .unwrap();
        let b = prime
            .add_node("person", json!({"name": "B"}))
            .await
            .unwrap();
        let c = prime
            .add_node("project", json!({"name": "C"}))
            .await
            .unwrap();

        let a_e = node_entity_id("person", a.as_str());
        let b_e = node_entity_id("person", b.as_str());
        let c_e = node_entity_id("project", c.as_str());

        // B -> C (works_on)
        prime.add_edge(&b_e, &c_e, "works_on", None).await.unwrap();

        // Compact B into A
        prime.compact(&a_e, &[&b_e]).await.unwrap();

        // A should now have an edge to C
        let neighbors = prime.neighbors(&a_e, Some("works_on"), Direction::Outgoing);
        assert_eq!(neighbors.len(), 1);
        assert_eq!(neighbors[0].properties["name"], "C");

        // B should be deleted
        assert!(prime.get_node(&b_e).is_none());

        prime.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_compact_emits_compacted_event() {
        let prime = Prime::open_in_memory().await.unwrap();

        let a = prime.add_node("n", json!({"name": "A"})).await.unwrap();
        let b = prime.add_node("n", json!({"name": "B"})).await.unwrap();

        let a_e = node_entity_id("n", a.as_str());
        let b_e = node_entity_id("n", b.as_str());

        prime.compact(&a_e, &[&b_e]).await.unwrap();

        // Check for compaction event in history
        let history = prime.history(&a_e).await.unwrap();
        let compacted = history
            .iter()
            .find(|h| h.event_type == "prime.memory.compacted");
        assert!(compacted.is_some());
        let payload = &compacted.unwrap().payload;
        assert_eq!(payload["target"], a_e);
        assert!(
            payload["merged_from"]
                .as_array()
                .unwrap()
                .contains(&json!(b_e))
        );

        prime.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_compact_nonexistent_target_fails() {
        let prime = Prime::open_in_memory().await.unwrap();

        let b = prime.add_node("n", json!({"name": "B"})).await.unwrap();
        let b_e = node_entity_id("n", b.as_str());

        let result = prime.compact("node:n:ghost", &[&b_e]).await;
        assert!(result.is_err());

        prime.shutdown().await.unwrap();
    }

    // =========================================================================
    // Vector operation tests (prime-vectors feature)
    // =========================================================================

    #[cfg(feature = "prime-vectors")]
    #[tokio::test]
    async fn test_embed_and_vector_search() {
        let prime = Prime::open_in_memory().await.unwrap();

        // Embed 3 documents with distinct vectors
        prime
            .embed("doc-1", Some("Rust is fast"), vec![1.0, 0.0, 0.0, 0.0])
            .await
            .unwrap();
        prime
            .embed("doc-2", Some("Rust is safe"), vec![0.9, 0.1, 0.0, 0.0])
            .await
            .unwrap();
        prime
            .embed("doc-3", Some("Python is dynamic"), vec![0.0, 0.0, 1.0, 0.0])
            .await
            .unwrap();

        // Search for something close to doc-1
        let results = prime.vector_search(&[1.0, 0.0, 0.0, 0.0], 3);
        assert_eq!(results.len(), 3);
        // Highest similarity should be doc-1
        assert!(results[0].id.contains("doc-1"));
        assert!(results[0].score > 0.99);
        // doc-3 should be least similar
        assert!(results[2].id.contains("doc-3"));

        prime.shutdown().await.unwrap();
    }

    #[cfg(feature = "prime-vectors")]
    #[tokio::test]
    async fn test_embed_delete_excludes_from_search() {
        let prime = Prime::open_in_memory().await.unwrap();

        prime
            .embed("doc-a", Some("Hello"), vec![1.0, 0.0])
            .await
            .unwrap();
        prime
            .embed("doc-b", Some("World"), vec![0.9, 0.1])
            .await
            .unwrap();

        // Delete doc-a
        prime.delete_vector("doc-a").await.unwrap();

        // Search should only return doc-b
        let results = prime.vector_search(&[1.0, 0.0], 10);
        assert_eq!(results.len(), 1);
        assert!(results[0].id.contains("doc-b"));

        // get_vector should return None for deleted
        assert!(prime.get_vector("doc-a").is_none());
        assert!(prime.get_vector("doc-b").is_some());

        prime.shutdown().await.unwrap();
    }

    #[cfg(feature = "prime-vectors")]
    #[tokio::test]
    async fn test_similar_finds_close_vectors() {
        let prime = Prime::open_in_memory().await.unwrap();

        prime
            .embed("ref", Some("Reference"), vec![1.0, 0.0, 0.0])
            .await
            .unwrap();
        prime
            .embed("close", Some("Close"), vec![0.95, 0.05, 0.0])
            .await
            .unwrap();
        prime
            .embed("far", Some("Far"), vec![0.0, 0.0, 1.0])
            .await
            .unwrap();

        let similar = prime.similar("ref", 3).unwrap();
        assert_eq!(similar.len(), 3);
        // First result is self (highest similarity), second is "close"
        assert!(similar[0].id.contains("ref"));
        assert!(similar[1].id.contains("close"));

        prime.shutdown().await.unwrap();
    }

    // =========================================================================
    // Hybrid recall tests
    // =========================================================================

    #[cfg(feature = "prime-vectors")]
    #[tokio::test]
    async fn test_recall_vector_plus_graph() {
        use crate::prime::types::RecallQuery;

        let prime = Prime::open_in_memory().await.unwrap();

        // Create graph nodes
        let a = prime
            .add_node("concept", json!({"name": "Rust"}))
            .await
            .unwrap();
        let b = prime
            .add_node("concept", json!({"name": "Safety"}))
            .await
            .unwrap();
        let c = prime
            .add_node("concept", json!({"name": "Speed"}))
            .await
            .unwrap();
        let ae = node_entity_id("concept", a.as_str());
        let be = node_entity_id("concept", b.as_str());
        let ce = node_entity_id("concept", c.as_str());

        prime.add_edge(&ae, &be, "related", None).await.unwrap();
        prime.add_edge(&ae, &ce, "related", None).await.unwrap();

        // Embed vectors for the nodes
        prime
            .embed(&ae, Some("Rust"), vec![1.0, 0.0, 0.0, 0.0])
            .await
            .unwrap();
        prime
            .embed(&be, Some("Safety"), vec![0.8, 0.2, 0.0, 0.0])
            .await
            .unwrap();
        prime
            .embed(&ce, Some("Speed"), vec![0.0, 0.0, 1.0, 0.0])
            .await
            .unwrap();

        let result = prime
            .recall(RecallQuery {
                vector: Some(vec![1.0, 0.0, 0.0, 0.0]),
                depth: 1,
                top_k: 10,
                similarity_weight: 0.5,
                proximity_weight: 0.3,
                recency_weight: 0.2,
                ..Default::default()
            })
            .await
            .unwrap();

        // Should get vector matches + graph neighbors
        assert!(!result.nodes.is_empty());
        assert!(!result.vectors.is_empty());
        // Highest scored should be the closest vector match
        assert!(result.nodes[0].score > 0.0);

        prime.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn a_replica_reports_itself_as_read_only() {
        let dir = tempfile::tempdir().expect("tempdir");

        let writer = Prime::open(dir.path()).await.expect("writer opens");
        assert!(!writer.is_read_only(), "the lock owner is writable");

        let replica = Prime::open(dir.path()).await.expect("replica opens");
        assert!(
            replica.is_read_only(),
            "a second process on the same data dir must know it is a replica"
        );

        // In-memory has no lock either, so deriving read-only from the absence
        // of a lock would call this one a replica. It is not.
        let memory = Prime::open_in_memory().await.expect("in-memory opens");
        assert!(!memory.is_read_only());

        replica.shutdown().await.unwrap();
        writer.shutdown().await.unwrap();
        memory.shutdown().await.unwrap();
    }

    #[cfg(feature = "prime-vectors")]
    #[tokio::test]
    async fn recall_never_returns_another_tenants_nodes() {
        use crate::prime::types::RecallQuery;

        let prime = Prime::open_in_memory().await.unwrap();

        let mine = prime
            .add_node(
                "interaction",
                json!({"name": "Invoice", "tenant_id": "t-a"}),
            )
            .await
            .unwrap();
        let theirs = prime
            .add_node(
                "interaction",
                json!({"name": "Invoice", "tenant_id": "t-b"}),
            )
            .await
            .unwrap();
        let mine_e = node_entity_id("interaction", mine.as_str());
        let theirs_e = node_entity_id("interaction", theirs.as_str());

        prime
            .embed(&mine_e, Some("Invoice"), vec![1.0, 0.0, 0.0, 0.0])
            .await
            .unwrap();
        prime
            .embed(&theirs_e, Some("Invoice"), vec![1.0, 0.0, 0.0, 0.0])
            .await
            .unwrap();

        let scoped = prime
            .recall(RecallQuery {
                vector: Some(vec![1.0, 0.0, 0.0, 0.0]),
                tenant: Some("t-a".into()),
                top_k: 10,
                ..Default::default()
            })
            .await
            .unwrap();
        assert!(!scoped.nodes.is_empty(), "own node must still be found");
        for n in &scoped.nodes {
            assert_eq!(
                n.node.properties.get("tenant_id").and_then(|v| v.as_str()),
                Some("t-a")
            );
        }

        // The same query with no tenant sees both, which is what makes the
        // assertion above evidence of filtering rather than of an empty store.
        let unscoped = prime
            .recall(RecallQuery {
                vector: Some(vec![1.0, 0.0, 0.0, 0.0]),
                top_k: 10,
                ..Default::default()
            })
            .await
            .unwrap();
        assert!(unscoped.nodes.len() > scoped.nodes.len());

        prime.shutdown().await.unwrap();
    }

    #[cfg(feature = "prime-vectors")]
    #[tokio::test]
    async fn recall_expansion_does_not_cross_a_tenant_edge() {
        use crate::prime::types::RecallQuery;

        let prime = Prime::open_in_memory().await.unwrap();

        let mine = prime
            .add_node("interaction", json!({"name": "Thread", "tenant_id": "t-a"}))
            .await
            .unwrap();
        let theirs = prime
            .add_node("interaction", json!({"name": "Secret", "tenant_id": "t-b"}))
            .await
            .unwrap();
        let mine_e = node_entity_id("interaction", mine.as_str());
        let theirs_e = node_entity_id("interaction", theirs.as_str());

        // An edge that crosses the tenant boundary: seeding on t-a's node and
        // expanding one hop would reach t-b's node if expansion were unfiltered.
        prime
            .add_edge(&mine_e, &theirs_e, "mentions", None)
            .await
            .unwrap();
        prime
            .embed(&mine_e, Some("Thread"), vec![1.0, 0.0, 0.0, 0.0])
            .await
            .unwrap();

        let result = prime
            .recall(RecallQuery {
                vector: Some(vec![1.0, 0.0, 0.0, 0.0]),
                tenant: Some("t-a".into()),
                depth: 2,
                top_k: 10,
                ..Default::default()
            })
            .await
            .unwrap();

        assert!(!result.nodes.is_empty());
        assert!(
            !result
                .nodes
                .iter()
                .any(|n| n.node.id.as_str() == theirs.as_str()),
            "expansion crossed into another tenant"
        );

        prime.shutdown().await.unwrap();
    }

    #[cfg(feature = "prime-vectors")]
    #[tokio::test]
    async fn lexical_recall_is_tenant_scoped_too() {
        use crate::prime::types::RecallQuery;

        let prime = Prime::open_in_memory().await.unwrap();
        prime
            .add_node(
                "interaction",
                json!({"name": "Renewal", "tenant_id": "t-a"}),
            )
            .await
            .unwrap();
        prime
            .add_node(
                "interaction",
                json!({"name": "Renewal", "tenant_id": "t-b"}),
            )
            .await
            .unwrap();

        // No vector on the query, so this exercises the lexical arm rather
        // than the vector one.
        let result = prime
            .recall(RecallQuery {
                text: Some("Renewal".into()),
                tenant: Some("t-b".into()),
                top_k: 10,
                ..Default::default()
            })
            .await
            .unwrap();

        assert!(!result.nodes.is_empty());
        for n in &result.nodes {
            assert_eq!(
                n.node.properties.get("tenant_id").and_then(|v| v.as_str()),
                Some("t-b")
            );
        }

        prime.shutdown().await.unwrap();
    }

    #[cfg(feature = "prime-vectors")]
    #[tokio::test]
    async fn test_recall_recency_weight() {
        use crate::prime::types::RecallQuery;

        let prime = Prime::open_in_memory().await.unwrap();

        // All nodes created "now" so recency is ~1.0 for all
        prime
            .add_node("fact", json!({"name": "New"}))
            .await
            .unwrap();
        prime
            .add_node("fact", json!({"name": "Also New"}))
            .await
            .unwrap();

        let result = prime
            .recall(RecallQuery {
                node_type: Some("fact".to_string()),
                recency_weight: 1.0,
                similarity_weight: 0.0,
                proximity_weight: 0.0,
                top_k: 10,
                ..Default::default()
            })
            .await
            .unwrap();

        // Graph-only (no vector), ranked by recency
        assert_eq!(result.nodes.len(), 2);
        // Both should have high recency scores (created just now)
        for node in &result.nodes {
            assert!(node.components.recency > 0.99);
        }

        prime.shutdown().await.unwrap();
    }

    #[cfg(feature = "prime-vectors")]
    #[tokio::test]
    async fn test_recall_depth_zero_only_direct_matches() {
        use crate::prime::types::RecallQuery;

        let prime = Prime::open_in_memory().await.unwrap();

        let a = prime.add_node("n", json!({"name": "A"})).await.unwrap();
        let b = prime.add_node("n", json!({"name": "B"})).await.unwrap();
        let ae = node_entity_id("n", a.as_str());
        let be = node_entity_id("n", b.as_str());

        prime.add_edge(&ae, &be, "link", None).await.unwrap();
        prime.embed(&ae, Some("A"), vec![1.0, 0.0]).await.unwrap();

        let result = prime
            .recall(RecallQuery {
                vector: Some(vec![1.0, 0.0]),
                depth: 0, // No graph expansion
                top_k: 10,
                ..Default::default()
            })
            .await
            .unwrap();

        // Only direct vector matches, no graph expansion
        assert_eq!(result.nodes.len(), 1);
        assert_eq!(result.nodes[0].depth, 0);

        prime.shutdown().await.unwrap();
    }

    // =========================================================================
    // Remember / Forget tests
    // =========================================================================

    #[cfg(feature = "prime-vectors")]
    #[tokio::test]
    async fn test_remember_creates_node_vector_edges() {
        let prime = Prime::open_in_memory().await.unwrap();

        // Create a target node to link to
        let project = prime
            .add_node("project", json!({"name": "Prime"}))
            .await
            .unwrap();
        let project_entity = node_entity_id("project", project.as_str());

        // Remember a concept
        let entity_id = prime
            .remember(
                "Rust is a systems language",
                vec![1.0, 0.0, 0.0],
                "concept",
                json!({"name": "Rust"}),
                &[(&project_entity, "related_to")],
            )
            .await
            .unwrap();

        // Node should exist
        let node = prime.get_node(&entity_id).unwrap();
        assert_eq!(node.node_type, "concept");
        assert_eq!(node.properties["name"], "Rust");

        // Vector should be searchable
        let results = prime.vector_search(&[1.0, 0.0, 0.0], 5);
        assert!(!results.is_empty());

        // Edge should exist
        let neighbors = prime.neighbors(&entity_id, Some("related_to"), Direction::Outgoing);
        assert_eq!(neighbors.len(), 1);
        assert_eq!(neighbors[0].properties["name"], "Prime");

        prime.shutdown().await.unwrap();
    }

    #[cfg(feature = "prime-vectors")]
    #[tokio::test]
    async fn test_forget_removes_node_vector_edges() {
        let prime = Prime::open_in_memory().await.unwrap();

        let entity_id = prime
            .remember(
                "Ephemeral knowledge",
                vec![0.5, 0.5],
                "fact",
                json!({"name": "temp"}),
                &[],
            )
            .await
            .unwrap();

        // Verify it exists
        assert!(prime.get_node(&entity_id).is_some());
        assert!(!prime.vector_search(&[0.5, 0.5], 5).is_empty());

        // Forget
        prime.forget(&entity_id).await.unwrap();

        // Node should be gone
        assert!(prime.get_node(&entity_id).is_none());

        // Vector should be gone from search
        let results = prime.vector_search(&[0.5, 0.5], 5);
        assert!(results.is_empty());

        prime.shutdown().await.unwrap();
    }

    // =========================================================================
    // Conversation scoping tests
    // =========================================================================

    #[tokio::test]
    async fn test_conversation_scoping_isolates_history() {
        let prime = Prime::open_in_memory().await.unwrap();

        // Conversation 1: add node A
        let conv1 = prime.with_conversation("conv-1");
        let a = conv1
            .add_node("person", json!({"name": "Alice"}))
            .await
            .unwrap();
        #[allow(deprecated)]
        let ae = node_entity_id("person", a.as_str());

        // Conversation 2: add node B
        let conv2 = prime.with_conversation("conv-2");
        let b = conv2
            .add_node("person", json!({"name": "Bob"}))
            .await
            .unwrap();
        #[allow(deprecated)]
        let _be = node_entity_id("person", b.as_str());

        // conversation_history(conv-1) should only show conv-1 events
        let h1 = prime.conversation_history("conv-1").await.unwrap();
        assert_eq!(h1.len(), 1);

        let h2 = prime.conversation_history("conv-2").await.unwrap();
        assert_eq!(h2.len(), 1);

        // Both nodes exist globally
        assert!(prime.get_node(&ae).is_some());

        prime.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_conversation_diff() {
        let prime = Prime::open_in_memory().await.unwrap();

        let conv = prime.with_conversation("conv-x");
        conv.add_node("concept", json!({"name": "Rust"}))
            .await
            .unwrap();
        conv.add_node("concept", json!({"name": "Safety"}))
            .await
            .unwrap();

        let diff = prime.conversation_diff("conv-x").await.unwrap();
        assert_eq!(diff.nodes_added.len(), 2);

        prime.shutdown().await.unwrap();
    }
}
