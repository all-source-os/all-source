//! VectorSearchEngine - High-performance vector search with HNSW indexing
//!
//! This module provides:
//! - Embedding generation using fastembed (pure Rust, no Python)
//! - HNSW index for efficient approximate nearest neighbor search
//! - Background indexing with async/non-blocking operations
//! - Configurable similarity thresholds

#[cfg(feature = "vector-search")]
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
#[cfg(feature = "vector-search")]
use instant_distance::{Builder, HnswMap, Search};

#[cfg(not(feature = "vector-search"))]
use crate::domain::value_objects::EmbeddingVector;
use crate::{
    domain::value_objects::DistanceMetric,
    error::{AllSourceError, Result},
};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::mpsc;
use uuid::Uuid;

/// Configuration for the VectorSearchEngine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorSearchEngineConfig {
    /// Model to use for embeddings (default: AllMiniLmL6V2)
    pub model_name: String,
    /// Dimensions of the embedding vectors (depends on model)
    pub embedding_dimensions: usize,
    /// HNSW ef_construction parameter (higher = more accurate, slower build)
    pub hnsw_ef_construction: usize,
    /// HNSW M parameter (max connections per node)
    pub hnsw_m: usize,
    /// HNSW ef_search parameter (higher = more accurate search)
    pub hnsw_ef_search: usize,
    /// Default similarity threshold for search
    pub default_similarity_threshold: f32,
    /// Maximum batch size for background indexing
    pub batch_size: usize,
    /// Channel buffer size for background indexing
    pub channel_buffer_size: usize,
}

impl Default for VectorSearchEngineConfig {
    fn default() -> Self {
        Self {
            model_name: "AllMiniLmL6V2".to_string(),
            embedding_dimensions: 384, // all-MiniLM-L6-v2 dimensions
            hnsw_ef_construction: 100,
            hnsw_m: 16,
            hnsw_ef_search: 50,
            default_similarity_threshold: 0.5,
            batch_size: 100,
            channel_buffer_size: 1000,
        }
    }
}

/// A vector entry stored in the index
#[derive(Debug, Clone)]
pub struct IndexedVector {
    pub event_id: Uuid,
    pub tenant_id: String,
    pub embedding: Vec<f32>,
    pub source_text: Option<String>,
}

/// Request for background indexing
#[derive(Debug, Clone)]
pub struct IndexRequest {
    pub event_id: Uuid,
    pub tenant_id: String,
    pub payload: serde_json::Value,
    pub source_text: Option<String>,
}

/// Result of a similarity search
#[derive(Debug, Clone)]
pub struct SimilarityResult {
    pub event_id: Uuid,
    pub score: f32,
    pub source_text: Option<String>,
}

/// Query parameters for similarity search
#[derive(Debug, Clone)]
pub struct SimilarityQuery {
    /// Query embedding vector
    pub query_vector: Vec<f32>,
    /// Number of results to return
    pub k: usize,
    /// Optional tenant filter
    pub tenant_id: Option<String>,
    /// Minimum similarity threshold (0.0 to 1.0)
    pub min_similarity: Option<f32>,
    /// Distance metric to use
    pub metric: DistanceMetric,
}

impl SimilarityQuery {
    pub fn new(query_vector: Vec<f32>, k: usize) -> Self {
        Self {
            query_vector,
            k,
            tenant_id: None,
            min_similarity: None,
            metric: DistanceMetric::Cosine,
        }
    }

    pub fn with_tenant(mut self, tenant_id: String) -> Self {
        self.tenant_id = Some(tenant_id);
        self
    }

    pub fn with_min_similarity(mut self, threshold: f32) -> Self {
        self.min_similarity = Some(threshold);
        self
    }

    pub fn with_metric(mut self, metric: DistanceMetric) -> Self {
        self.metric = metric;
        self
    }
}

/// Point type for HNSW index
#[cfg(feature = "vector-search")]
#[derive(Clone)]
struct VectorPoint {
    values: Vec<f32>,
}

#[cfg(feature = "vector-search")]
impl instant_distance::Point for VectorPoint {
    fn distance(&self, other: &Self) -> f32 {
        // Cosine distance = 1 - cosine_similarity
        let dot: f32 = self
            .values
            .iter()
            .zip(other.values.iter())
            .map(|(a, b)| a * b)
            .sum();
        let norm_a: f32 = self.values.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = other.values.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm_a < 1e-10 || norm_b < 1e-10 {
            return 1.0;
        }
        1.0 - (dot / (norm_a * norm_b))
    }
}

/// VectorSearchEngine with HNSW indexing and fastembed embeddings
///
/// Features:
/// - Pure Rust embeddings via fastembed (no Python dependency)
/// - HNSW index for O(log n) approximate nearest neighbor search
/// - Background indexing with async channel-based processing
/// - Multi-tenant support with tenant isolation
/// - Configurable similarity thresholds
pub struct VectorSearchEngine {
    config: VectorSearchEngineConfig,
    #[cfg(feature = "vector-search")]
    embedding_model: Option<Arc<parking_lot::Mutex<TextEmbedding>>>,
    #[cfg(feature = "vector-search")]
    hnsw_index: Arc<RwLock<Option<HnswMap<VectorPoint, Uuid>>>>,
    /// Metadata storage: event_id -> IndexedVector
    vectors: Arc<RwLock<HashMap<Uuid, IndexedVector>>>,
    /// Tenant index: tenant_id -> list of event_ids
    tenant_index: Arc<RwLock<HashMap<String, Vec<Uuid>>>>,
    /// Channel for background indexing
    index_sender: Option<mpsc::Sender<IndexRequest>>,
    /// Statistics
    stats: Arc<RwLock<EngineStats>>,
}

#[derive(Debug, Default, Clone)]
struct EngineStats {
    total_indexed: u64,
    total_searches: u64,
    total_embeddings_generated: u64,
}

impl VectorSearchEngine {
    /// Create a new VectorSearchEngine with default configuration
    #[cfg(feature = "vector-search")]
    pub fn new() -> Result<Self> {
        Self::with_config(VectorSearchEngineConfig::default())
    }

    /// Create a new VectorSearchEngine with custom configuration
    #[cfg(feature = "vector-search")]
    pub fn with_config(config: VectorSearchEngineConfig) -> Result<Self> {
        // Initialize the embedding model
        let model = TextEmbedding::try_new(
            InitOptions::new(EmbeddingModel::AllMiniLML6V2).with_show_download_progress(false),
        )
        .map_err(|e| {
            AllSourceError::InternalError(format!("Failed to load embedding model: {e}"))
        })?;

        let mut engine = Self::with_external_embeddings(config);
        engine.embedding_model = Some(Arc::new(parking_lot::Mutex::new(model)));
        Ok(engine)
    }

    /// An engine that never loads a model: callers index and query with
    /// vectors they computed themselves, and `embed_text` refuses.
    #[cfg(feature = "vector-search")]
    pub fn with_external_embeddings(config: VectorSearchEngineConfig) -> Self {
        Self {
            config,
            embedding_model: None,
            hnsw_index: Arc::new(RwLock::new(None)),
            vectors: Arc::new(RwLock::new(HashMap::new())),
            tenant_index: Arc::new(RwLock::new(HashMap::new())),
            index_sender: None,
            stats: Arc::new(RwLock::new(EngineStats::default())),
        }
    }

    #[cfg(not(feature = "vector-search"))]
    pub fn with_external_embeddings(config: VectorSearchEngineConfig) -> Self {
        Self {
            config,
            vectors: Arc::new(RwLock::new(HashMap::new())),
            tenant_index: Arc::new(RwLock::new(HashMap::new())),
            index_sender: None,
            stats: Arc::new(RwLock::new(EngineStats::default())),
        }
    }

    /// Create a VectorSearchEngine without the embedding model (for testing)
    #[cfg(not(feature = "vector-search"))]
    pub fn new() -> Result<Self> {
        Self::with_config(VectorSearchEngineConfig::default())
    }

    #[cfg(not(feature = "vector-search"))]
    pub fn with_config(config: VectorSearchEngineConfig) -> Result<Self> {
        Ok(Self::with_external_embeddings(config))
    }

    /// Get the engine configuration
    pub fn config(&self) -> &VectorSearchEngineConfig {
        &self.config
    }

    /// Generate embedding from text using fastembed
    #[cfg(feature = "vector-search")]
    pub fn embed_text(&self, text: &str) -> Result<Vec<f32>> {
        let model = self.embedding_model.as_ref().ok_or_else(|| {
            AllSourceError::InternalError(
                "This vector engine has no embedding model; supply embeddings".to_string(),
            )
        })?;
        let embeddings = model.lock().embed(vec![text], None).map_err(|e| {
            AllSourceError::InternalError(format!("Embedding generation failed: {e}"))
        })?;

        let embedding = embeddings
            .into_iter()
            .next()
            .ok_or_else(|| AllSourceError::InternalError("No embedding generated".to_string()))?;

        {
            let mut stats = self.stats.write();
            stats.total_embeddings_generated += 1;
        }

        Ok(embedding)
    }

    #[cfg(not(feature = "vector-search"))]
    pub fn embed_text(&self, _text: &str) -> Result<Vec<f32>> {
        Err(AllSourceError::InternalError(
            "Vector search feature not enabled. Enable 'vector-search' feature in Cargo.toml"
                .to_string(),
        ))
    }

    /// Generate embedding from an event payload
    ///
    /// Extracts text content from the event payload and generates an embedding.
    /// Handles various payload structures by extracting relevant text fields.
    pub fn embed_event(&self, payload: &serde_json::Value) -> Result<Vec<f32>> {
        let text = Self::extract_text_from_payload(payload);
        if text.is_empty() {
            return Err(AllSourceError::InvalidInput(
                "Event payload contains no text content for embedding".to_string(),
            ));
        }
        self.embed_text(&text)
    }

    /// Extract text content from a JSON payload for embedding
    pub(crate) fn extract_text_from_payload(payload: &serde_json::Value) -> String {
        let mut text_parts = Vec::new();

        match payload {
            serde_json::Value::String(s) => {
                text_parts.push(s.clone());
            }
            serde_json::Value::Object(map) => {
                // Priority fields that typically contain meaningful text
                let priority_fields = [
                    "content",
                    "text",
                    "body",
                    "message",
                    "description",
                    "title",
                    "name",
                    "summary",
                ];

                for field in priority_fields {
                    if let Some(serde_json::Value::String(s)) = map.get(field) {
                        text_parts.push(s.clone());
                    }
                }

                // If no priority fields found, collect all string values
                if text_parts.is_empty() {
                    for (key, value) in map {
                        // Skip internal/metadata fields
                        if key.starts_with('_') || key == "id" || key == "timestamp" {
                            continue;
                        }
                        if let serde_json::Value::String(s) = value {
                            text_parts.push(s.clone());
                        }
                    }
                }
            }
            serde_json::Value::Array(arr) => {
                for item in arr {
                    let item_text = Self::extract_text_from_payload(item);
                    if !item_text.is_empty() {
                        text_parts.push(item_text);
                    }
                }
            }
            _ => {}
        }

        text_parts.join(" ")
    }

    /// Index an event with its embedding
    // Public async API. Dropping `async` would change the returned type from a
    // named future to `impl Future` and make the body run eagerly at call time,
    // which is a breaking change for every downstream caller. The body is also
    // cfg-dependent (`vector-search` pulls in the HNSW rebuild), so "no `.await`
    // today" is a property of one feature combination, not of the API.
    #[allow(unknown_lints, clippy::unused_async_trait_impl)]
    pub async fn index_event(
        &self,
        event_id: Uuid,
        tenant_id: &str,
        embedding: Vec<f32>,
        source_text: Option<String>,
    ) -> Result<()> {
        // Store the vector metadata
        let indexed = IndexedVector {
            event_id,
            tenant_id: tenant_id.to_string(),
            embedding: embedding.clone(),
            source_text,
        };

        {
            let mut vectors = self.vectors.write();
            vectors.insert(event_id, indexed);
        }

        {
            let mut tenant_idx = self.tenant_index.write();
            tenant_idx
                .entry(tenant_id.to_string())
                .or_default()
                .push(event_id);
        }

        // Rebuild HNSW index (in production, this would be batched)
        #[cfg(feature = "vector-search")]
        self.rebuild_hnsw_index()?;

        {
            let mut stats = self.stats.write();
            stats.total_indexed += 1;
        }

        Ok(())
    }

    /// Index many vectors and rebuild the HNSW index once for the whole batch.
    pub fn index_batch(&self, entries: Vec<IndexedVector>) -> Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        let added = entries.len() as u64;
        {
            let mut vectors = self.vectors.write();
            let mut tenant_idx = self.tenant_index.write();
            for entry in entries {
                tenant_idx
                    .entry(entry.tenant_id.clone())
                    .or_default()
                    .push(entry.event_id);
                vectors.insert(entry.event_id, entry);
            }
        }

        #[cfg(feature = "vector-search")]
        self.rebuild_hnsw_index()?;

        self.stats.write().total_indexed += added;
        Ok(())
    }

    /// Rebuild the HNSW index from current vectors
    #[cfg(feature = "vector-search")]
    fn rebuild_hnsw_index(&self) -> Result<()> {
        let vectors = self.vectors.read();
        if vectors.is_empty() {
            let mut index = self.hnsw_index.write();
            *index = None;
            return Ok(());
        }

        let points: Vec<VectorPoint> = vectors
            .values()
            .map(|v| VectorPoint {
                values: v.embedding.clone(),
            })
            .collect();

        let values: Vec<Uuid> = vectors.keys().copied().collect();

        let hnsw = Builder::default()
            .ef_construction(self.config.hnsw_ef_construction)
            .build(points, values);

        let mut index = self.hnsw_index.write();
        *index = Some(hnsw);

        Ok(())
    }

    /// Search for similar vectors using HNSW index
    #[cfg(feature = "vector-search")]
    pub fn search_similar(&self, query: &SimilarityQuery) -> Result<Vec<SimilarityResult>> {
        {
            let mut stats = self.stats.write();
            stats.total_searches += 1;
        }

        let index_guard = self.hnsw_index.read();
        let Some(index) = index_guard.as_ref() else {
            return Ok(vec![]);
        };

        let query_point = VectorPoint {
            values: query.query_vector.clone(),
        };

        let mut search = Search::default();
        let results = index.search(&query_point, &mut search);

        let vectors = self.vectors.read();
        let min_sim = query
            .min_similarity
            .unwrap_or(self.config.default_similarity_threshold);

        let mut similarity_results = Vec::new();

        for result in results.take(query.k * 2) {
            // Take extra to account for filtering
            let event_id = *result.value;

            // Convert cosine distance to similarity
            let similarity = 1.0 - result.distance;

            // Apply similarity threshold
            if similarity < min_sim {
                continue;
            }

            // Apply tenant filter if specified
            if let Some(ref tenant_filter) = query.tenant_id
                && let Some(vec) = vectors.get(&event_id)
                && vec.tenant_id != *tenant_filter
            {
                continue;
            }

            let source_text = vectors.get(&event_id).and_then(|v| v.source_text.clone());

            similarity_results.push(SimilarityResult {
                event_id,
                score: similarity,
                source_text,
            });

            if similarity_results.len() >= query.k {
                break;
            }
        }

        Ok(similarity_results)
    }

    /// Fallback search using brute force (when HNSW not available or for testing)
    #[cfg(not(feature = "vector-search"))]
    pub fn search_similar(&self, query: &SimilarityQuery) -> Result<Vec<SimilarityResult>> {
        {
            let mut stats = self.stats.write();
            stats.total_searches += 1;
        }

        let vectors = self.vectors.read();
        let min_sim = query
            .min_similarity
            .unwrap_or(self.config.default_similarity_threshold);

        let query_embedding = EmbeddingVector::new(query.query_vector.clone())?;
        let mut scored: Vec<(Uuid, f32, Option<String>)> = Vec::new();

        for (event_id, indexed) in vectors.iter() {
            // Apply tenant filter
            if let Some(ref tenant_filter) = query.tenant_id
                && indexed.tenant_id != *tenant_filter
            {
                continue;
            }

            let vec_embedding = EmbeddingVector::new(indexed.embedding.clone())?;
            let similarity = match query.metric {
                DistanceMetric::Cosine => query_embedding.cosine_similarity(&vec_embedding)?,
                DistanceMetric::Euclidean => {
                    let dist = query_embedding.euclidean_distance(&vec_embedding)?;
                    1.0 / (1.0 + dist) // Convert to similarity
                }
                DistanceMetric::DotProduct => query_embedding.dot_product(&vec_embedding)?,
            };

            if similarity >= min_sim {
                scored.push((*event_id, similarity, indexed.source_text.clone()));
            }
        }

        // Sort by similarity (highest first)
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        Ok(scored
            .into_iter()
            .take(query.k)
            .map(|(event_id, score, source_text)| SimilarityResult {
                event_id,
                score,
                source_text,
            })
            .collect())
    }

    /// Start background indexing worker
    ///
    /// Returns a sender that can be used to submit index requests.
    /// The worker runs asynchronously and processes requests in batches.
    pub fn start_background_indexer(&mut self) -> mpsc::Sender<IndexRequest> {
        let (tx, mut rx) = mpsc::channel::<IndexRequest>(self.config.channel_buffer_size);
        self.index_sender = Some(tx.clone());

        let engine = Self {
            config: self.config.clone(),
            #[cfg(feature = "vector-search")]
            embedding_model: self.embedding_model.clone(),
            #[cfg(feature = "vector-search")]
            hnsw_index: self.hnsw_index.clone(),
            vectors: self.vectors.clone(),
            tenant_index: self.tenant_index.clone(),
            index_sender: None,
            stats: self.stats.clone(),
        };

        let batch_size = self.config.batch_size;

        tokio::spawn(async move {
            let mut batch: Vec<IndexRequest> = Vec::with_capacity(batch_size);

            loop {
                // Try to receive with a timeout for batching
                match tokio::time::timeout(std::time::Duration::from_millis(100), rx.recv()).await {
                    Ok(Some(request)) => {
                        batch.push(request);

                        // If batch is full, process it
                        if batch.len() >= batch_size {
                            engine.process_batch(&mut batch).await;
                        }
                    }
                    Ok(None) => {
                        // Channel closed, process remaining and exit
                        if !batch.is_empty() {
                            engine.process_batch(&mut batch).await;
                        }
                        break;
                    }
                    Err(_) => {
                        // Timeout - process any pending items
                        if !batch.is_empty() {
                            engine.process_batch(&mut batch).await;
                        }
                    }
                }
            }

            tracing::info!("Background indexer stopped");
        });

        tx
    }

    /// Process a batch of index requests
    async fn process_batch(&self, batch: &mut Vec<IndexRequest>) {
        for request in batch.drain(..) {
            match self.embed_event(&request.payload) {
                Ok(embedding) => {
                    if let Err(e) = self
                        .index_event(
                            request.event_id,
                            &request.tenant_id,
                            embedding,
                            request.source_text,
                        )
                        .await
                    {
                        tracing::error!("Failed to index event {}: {}", request.event_id, e);
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to generate embedding for event {}: {}",
                        request.event_id,
                        e
                    );
                }
            }
        }

        // Rebuild HNSW index after batch
        #[cfg(feature = "vector-search")]
        if let Err(e) = self.rebuild_hnsw_index() {
            tracing::error!("Failed to rebuild HNSW index: {}", e);
        }
    }

    /// Get the number of indexed vectors
    pub fn count(&self, tenant_id: Option<&str>) -> usize {
        if let Some(tid) = tenant_id {
            self.tenant_index
                .read()
                .get(tid)
                .map_or(0, std::vec::Vec::len)
        } else {
            self.vectors.read().len()
        }
    }

    /// Get engine statistics
    pub fn stats(&self) -> (u64, u64, u64) {
        let stats = self.stats.read();
        (
            stats.total_indexed,
            stats.total_searches,
            stats.total_embeddings_generated,
        )
    }

    /// Delete a vector by event ID
    pub fn delete(&self, event_id: Uuid) -> Result<bool> {
        let removed = {
            let mut vectors = self.vectors.write();
            vectors.remove(&event_id)
        };

        if let Some(indexed) = removed {
            let mut tenant_idx = self.tenant_index.write();
            if let Some(ids) = tenant_idx.get_mut(&indexed.tenant_id) {
                ids.retain(|id| *id != event_id);
            }

            #[cfg(feature = "vector-search")]
            self.rebuild_hnsw_index()?;

            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Delete all vectors for a tenant
    pub fn delete_by_tenant(&self, tenant_id: &str) -> Result<usize> {
        let event_ids = {
            let mut tenant_idx = self.tenant_index.write();
            tenant_idx.remove(tenant_id).unwrap_or_default()
        };

        let deleted = event_ids.len();
        {
            let mut vectors = self.vectors.write();
            for id in &event_ids {
                vectors.remove(id);
            }
        }

        if deleted > 0 {
            #[cfg(feature = "vector-search")]
            self.rebuild_hnsw_index()?;
        }

        Ok(deleted)
    }

    /// Health check
    pub fn health_check(&self) -> Result<()> {
        let vec_count = self.vectors.read().len();
        let idx_count: usize = self
            .tenant_index
            .read()
            .values()
            .map(std::vec::Vec::len)
            .sum();

        // Allow some discrepancy due to concurrent operations
        if vec_count > 0 && idx_count == 0 {
            return Err(AllSourceError::InternalError(
                "Vector index inconsistency detected".to_string(),
            ));
        }

        Ok(())
    }
}

/// Unit tests for VectorSearchEngine
///
/// These tests run without the `vector-search` feature enabled to avoid
/// network dependencies on model downloads. The HNSW and fastembed
/// integration is tested via integration tests when the feature is enabled.
#[cfg(test)]
#[cfg(not(feature = "vector-search"))]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_test_engine() -> VectorSearchEngine {
        VectorSearchEngine::with_config(VectorSearchEngineConfig {
            default_similarity_threshold: 0.0, // Allow all for testing
            ..Default::default()
        })
        .unwrap()
    }

    fn create_test_embedding(dims: usize, seed: f32) -> Vec<f32> {
        (0..dims).map(|i| (i as f32 + seed) / dims as f32).collect()
    }

    #[test]
    fn test_extract_text_from_string_payload() {
        let _engine = create_test_engine();
        let payload = json!("Hello world");
        let text = VectorSearchEngine::extract_text_from_payload(&payload);
        assert_eq!(text, "Hello world");
    }

    #[test]
    fn test_extract_text_from_object_payload() {
        let _engine = create_test_engine();
        let payload = json!({
            "title": "Test Title",
            "content": "Test content here",
            "id": "123"
        });
        let text = VectorSearchEngine::extract_text_from_payload(&payload);
        assert!(text.contains("Test content here"));
        assert!(text.contains("Test Title"));
    }

    #[test]
    fn test_extract_text_priority_fields() {
        let _engine = create_test_engine();
        let payload = json!({
            "content": "Priority content",
            "random_field": "Should not appear first"
        });
        let text = VectorSearchEngine::extract_text_from_payload(&payload);
        assert!(text.starts_with("Priority content"));
    }

    #[tokio::test]
    async fn test_index_and_search() {
        let engine = create_test_engine();

        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let id3 = Uuid::new_v4();

        // Create normalized vectors for cosine similarity testing
        let embedding1 = vec![1.0, 0.0, 0.0];
        let embedding2 = vec![0.9, 0.436, 0.0]; // Similar to embedding1
        let embedding3 = vec![0.0, 1.0, 0.0]; // Orthogonal

        engine
            .index_event(
                id1,
                "tenant-1",
                embedding1.clone(),
                Some("first".to_string()),
            )
            .await
            .unwrap();
        engine
            .index_event(
                id2,
                "tenant-1",
                embedding2.clone(),
                Some("second".to_string()),
            )
            .await
            .unwrap();
        engine
            .index_event(
                id3,
                "tenant-1",
                embedding3.clone(),
                Some("third".to_string()),
            )
            .await
            .unwrap();

        let query =
            SimilarityQuery::new(vec![1.0, 0.0, 0.0], 2).with_tenant("tenant-1".to_string());

        let results = engine.search_similar(&query).unwrap();

        assert_eq!(results.len(), 2);
        // First result should be exact match
        assert_eq!(results[0].event_id, id1);
        assert!((results[0].score - 1.0).abs() < 1e-5);
    }

    #[tokio::test]
    async fn test_search_with_similarity_threshold() {
        let engine = create_test_engine();

        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        engine
            .index_event(id1, "tenant-1", vec![1.0, 0.0, 0.0], None)
            .await
            .unwrap();
        engine
            .index_event(id2, "tenant-1", vec![0.0, 1.0, 0.0], None)
            .await
            .unwrap();

        let query = SimilarityQuery::new(vec![1.0, 0.0, 0.0], 10)
            .with_tenant("tenant-1".to_string())
            .with_min_similarity(0.5);

        let results = engine.search_similar(&query).unwrap();

        // Only id1 should match (similarity = 1.0)
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].event_id, id1);
    }

    #[tokio::test]
    async fn test_tenant_isolation() {
        let engine = create_test_engine();

        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        engine
            .index_event(id1, "tenant-1", vec![1.0, 0.0, 0.0], None)
            .await
            .unwrap();
        engine
            .index_event(id2, "tenant-2", vec![1.0, 0.0, 0.0], None)
            .await
            .unwrap();

        let query =
            SimilarityQuery::new(vec![1.0, 0.0, 0.0], 10).with_tenant("tenant-1".to_string());

        let results = engine.search_similar(&query).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].event_id, id1);
    }

    #[tokio::test]
    async fn test_delete() {
        let engine = create_test_engine();

        let id = Uuid::new_v4();
        engine
            .index_event(id, "tenant-1", vec![1.0, 0.0, 0.0], None)
            .await
            .unwrap();

        assert_eq!(engine.count(None), 1);

        let deleted = engine.delete(id).unwrap();
        assert!(deleted);
        assert_eq!(engine.count(None), 0);
    }

    #[tokio::test]
    async fn test_delete_by_tenant() {
        let engine = create_test_engine();

        for i in 0..5 {
            let tenant = if i < 3 { "tenant-1" } else { "tenant-2" };
            engine
                .index_event(
                    Uuid::new_v4(),
                    tenant,
                    create_test_embedding(3, i as f32),
                    None,
                )
                .await
                .unwrap();
        }

        assert_eq!(engine.count(Some("tenant-1")), 3);
        assert_eq!(engine.count(Some("tenant-2")), 2);

        let deleted = engine.delete_by_tenant("tenant-1").unwrap();
        assert_eq!(deleted, 3);

        assert_eq!(engine.count(Some("tenant-1")), 0);
        assert_eq!(engine.count(Some("tenant-2")), 2);
    }

    #[tokio::test]
    async fn test_stats() {
        let engine = create_test_engine();

        for i in 0..3 {
            engine
                .index_event(
                    Uuid::new_v4(),
                    "tenant-1",
                    create_test_embedding(3, i as f32),
                    None,
                )
                .await
                .unwrap();
        }

        let query =
            SimilarityQuery::new(vec![1.0, 0.0, 0.0], 2).with_tenant("tenant-1".to_string());
        engine.search_similar(&query).unwrap();
        engine.search_similar(&query).unwrap();

        let (indexed, searches, _) = engine.stats();
        assert_eq!(indexed, 3);
        assert_eq!(searches, 2);
    }

    #[test]
    fn test_health_check() {
        let engine = create_test_engine();
        assert!(engine.health_check().is_ok());
    }

    #[test]
    fn test_config_default() {
        let config = VectorSearchEngineConfig::default();
        assert_eq!(config.embedding_dimensions, 384);
        assert_eq!(config.model_name, "AllMiniLmL6V2");
        assert!(config.default_similarity_threshold > 0.0);
    }
}

/// Integration tests that require the vector-search feature
/// These tests verify HNSW and fastembed integration
/// Run with: cargo test --features vector-search infrastructure::search::integration_tests
#[cfg(test)]
#[cfg(feature = "vector-search")]
mod integration_tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = VectorSearchEngineConfig::default();
        assert_eq!(config.embedding_dimensions, 384);
        assert_eq!(config.model_name, "AllMiniLmL6V2");
        assert!(config.default_similarity_threshold > 0.0);
    }

    #[test]
    fn test_similarity_query_builder() {
        let query = SimilarityQuery::new(vec![1.0, 0.0, 0.0], 10)
            .with_tenant("tenant-1".to_string())
            .with_min_similarity(0.8)
            .with_metric(DistanceMetric::Cosine);

        assert_eq!(query.k, 10);
        assert_eq!(query.tenant_id, Some("tenant-1".to_string()));
        assert_eq!(query.min_similarity, Some(0.8));
        assert_eq!(query.metric, DistanceMetric::Cosine);
    }

    #[test]
    fn test_indexed_vector_struct() {
        let vec = IndexedVector {
            event_id: Uuid::new_v4(),
            tenant_id: "tenant-1".to_string(),
            embedding: vec![1.0, 2.0, 3.0],
            source_text: Some("test".to_string()),
        };
        assert_eq!(vec.embedding.len(), 3);
        assert_eq!(vec.source_text, Some("test".to_string()));
    }
}
