//! HybridSearchEngine - Combined semantic and keyword search orchestrator
//!
//! This module provides:
//! - Unified search interface combining VectorSearchEngine (semantic) and KeywordSearchEngine (keyword)
//! - Score combination and re-ranking using Reciprocal Rank Fusion (RRF)
//! - Metadata filtering (event_type, entity_id, time range)
//! - Configurable search modes (semantic-only, keyword-only, hybrid)

use crate::error::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use uuid::Uuid;

use super::{
    KeywordQuery, KeywordSearchEngine, KeywordSearchResult, SimilarityQuery, SimilarityResult,
    VectorSearchEngine,
};

/// Configuration for the HybridSearchEngine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridSearchEngineConfig {
    /// Weight for semantic search scores (0.0 to 1.0)
    pub semantic_weight: f32,
    /// Weight for keyword search scores (0.0 to 1.0)
    pub keyword_weight: f32,
    /// RRF constant k (typically 60)
    pub rrf_k: f32,
    /// Default number of results to return
    pub default_limit: usize,
    /// Minimum score threshold for final results (0.0 to 1.0)
    pub min_score_threshold: f32,
    /// Over-fetch multiplier for filtering (fetch N * multiplier before filtering)
    pub over_fetch_multiplier: usize,
}

impl Default for HybridSearchEngineConfig {
    fn default() -> Self {
        Self {
            semantic_weight: 0.5,
            keyword_weight: 0.5,
            rrf_k: 60.0,
            default_limit: 10,
            min_score_threshold: 0.0,
            over_fetch_multiplier: 3,
        }
    }
}

/// Search mode for the hybrid search
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SearchMode {
    /// Use only semantic (vector) search
    SemanticOnly,
    /// Use only keyword (BM25) search
    KeywordOnly,
    /// Combine both semantic and keyword search (default)
    #[default]
    Hybrid,
}

/// Metadata filters for search queries
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetadataFilters {
    /// Filter by event type (exact match)
    pub event_type: Option<String>,
    /// Filter by entity ID (exact match)
    pub entity_id: Option<String>,
    /// Filter by start time (events on or after this time)
    pub time_from: Option<DateTime<Utc>>,
    /// Filter by end time (events on or before this time)
    pub time_to: Option<DateTime<Utc>>,
}

impl MetadataFilters {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_event_type(mut self, event_type: impl Into<String>) -> Self {
        self.event_type = Some(event_type.into());
        self
    }

    pub fn with_entity_id(mut self, entity_id: impl Into<String>) -> Self {
        self.entity_id = Some(entity_id.into());
        self
    }

    pub fn with_time_range(mut self, from: DateTime<Utc>, to: DateTime<Utc>) -> Self {
        self.time_from = Some(from);
        self.time_to = Some(to);
        self
    }

    pub fn with_time_from(mut self, from: DateTime<Utc>) -> Self {
        self.time_from = Some(from);
        self
    }

    pub fn with_time_to(mut self, to: DateTime<Utc>) -> Self {
        self.time_to = Some(to);
        self
    }

    /// Check if any filters are set
    pub fn has_filters(&self) -> bool {
        self.event_type.is_some()
            || self.entity_id.is_some()
            || self.time_from.is_some()
            || self.time_to.is_some()
    }
}

/// Unified search query combining all search options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    /// The search query string (natural language or keywords)
    pub query: String,
    /// Maximum number of results to return
    pub limit: usize,
    /// Optional tenant filter for multi-tenancy
    pub tenant_id: Option<String>,
    /// Search mode (semantic, keyword, or hybrid)
    pub mode: SearchMode,
    /// Metadata filters
    pub filters: MetadataFilters,
    /// Minimum similarity threshold for semantic search (0.0 to 1.0)
    pub min_similarity: Option<f32>,
}

impl SearchQuery {
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            limit: 10,
            tenant_id: None,
            mode: SearchMode::Hybrid,
            filters: MetadataFilters::default(),
            min_similarity: None,
        }
    }

    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    pub fn with_tenant(mut self, tenant_id: impl Into<String>) -> Self {
        self.tenant_id = Some(tenant_id.into());
        self
    }

    pub fn with_mode(mut self, mode: SearchMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn with_filters(mut self, filters: MetadataFilters) -> Self {
        self.filters = filters;
        self
    }

    pub fn with_event_type(mut self, event_type: impl Into<String>) -> Self {
        self.filters.event_type = Some(event_type.into());
        self
    }

    pub fn with_entity_id(mut self, entity_id: impl Into<String>) -> Self {
        self.filters.entity_id = Some(entity_id.into());
        self
    }

    pub fn with_time_range(mut self, from: DateTime<Utc>, to: DateTime<Utc>) -> Self {
        self.filters.time_from = Some(from);
        self.filters.time_to = Some(to);
        self
    }

    pub fn with_min_similarity(mut self, threshold: f32) -> Self {
        self.min_similarity = Some(threshold);
        self
    }

    /// Create a semantic-only search query
    pub fn semantic(query: impl Into<String>) -> Self {
        Self::new(query).with_mode(SearchMode::SemanticOnly)
    }

    /// Create a keyword-only search query
    pub fn keyword(query: impl Into<String>) -> Self {
        Self::new(query).with_mode(SearchMode::KeywordOnly)
    }
}

/// Result of a hybrid search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridSearchResult {
    /// Event ID of the matching event
    pub event_id: Uuid,
    /// Combined score (0.0 to 1.0)
    pub score: f32,
    /// Individual semantic similarity score (if available)
    pub semantic_score: Option<f32>,
    /// Individual keyword relevance score (if available)
    pub keyword_score: Option<f32>,
    /// Event type (from keyword search if available)
    pub event_type: Option<String>,
    /// Entity ID (from keyword search if available)
    pub entity_id: Option<String>,
    /// Source text snippet (from vector search if available)
    pub source_text: Option<String>,
}

/// Event metadata for filtering (stored alongside indexed data)
#[derive(Debug, Clone, Default)]
pub struct EventMetadata {
    pub event_type: Option<String>,
    pub entity_id: Option<String>,
    pub timestamp: Option<DateTime<Utc>>,
}

/// HybridSearchEngine - Orchestrates semantic and keyword search
///
/// Features:
/// - Combines VectorSearchEngine (semantic) and KeywordSearchEngine (keyword)
/// - Score fusion using Reciprocal Rank Fusion (RRF)
/// - Metadata filtering for event_type, entity_id, and time range
/// - Configurable search modes and weights
pub struct HybridSearchEngine {
    config: HybridSearchEngineConfig,
    vector_engine: Arc<VectorSearchEngine>,
    keyword_engine: Arc<KeywordSearchEngine>,
    /// Event metadata cache for filtering
    metadata_cache: parking_lot::RwLock<HashMap<Uuid, EventMetadata>>,
    /// Statistics
    stats: parking_lot::RwLock<EngineStats>,
}

#[derive(Debug, Default, Clone)]
struct EngineStats {
    total_searches: u64,
    semantic_searches: u64,
    keyword_searches: u64,
    hybrid_searches: u64,
}

impl HybridSearchEngine {
    /// Create a new HybridSearchEngine with existing engines
    pub fn new(
        vector_engine: Arc<VectorSearchEngine>,
        keyword_engine: Arc<KeywordSearchEngine>,
    ) -> Self {
        Self::with_config(
            vector_engine,
            keyword_engine,
            HybridSearchEngineConfig::default(),
        )
    }

    /// Create a new HybridSearchEngine with custom configuration
    pub fn with_config(
        vector_engine: Arc<VectorSearchEngine>,
        keyword_engine: Arc<KeywordSearchEngine>,
        config: HybridSearchEngineConfig,
    ) -> Self {
        Self {
            config,
            vector_engine,
            keyword_engine,
            metadata_cache: parking_lot::RwLock::new(HashMap::new()),
            stats: parking_lot::RwLock::new(EngineStats::default()),
        }
    }

    /// Get the engine configuration
    pub fn config(&self) -> &HybridSearchEngineConfig {
        &self.config
    }

    /// Store metadata for an event (for post-search filtering)
    pub fn store_metadata(&self, event_id: Uuid, metadata: EventMetadata) {
        let mut cache = self.metadata_cache.write();
        cache.insert(event_id, metadata);
    }

    /// Index an event in both engines
    ///
    /// This is a convenience method that indexes the event in both
    /// the vector and keyword engines, and stores metadata for filtering.
    #[cfg(any(feature = "vector-search", feature = "keyword-search"))]
    pub async fn index_event(
        &self,
        event_id: Uuid,
        tenant_id: &str,
        event_type: &str,
        entity_id: Option<&str>,
        payload: &serde_json::Value,
        timestamp: DateTime<Utc>,
    ) -> Result<()> {
        // Store metadata for filtering
        self.store_metadata(
            event_id,
            EventMetadata {
                event_type: Some(event_type.to_string()),
                entity_id: entity_id.map(std::string::ToString::to_string),
                timestamp: Some(timestamp),
            },
        );

        // Index in keyword engine
        #[cfg(feature = "keyword-search")]
        self.keyword_engine
            .index_event(event_id, tenant_id, event_type, entity_id, payload)?;

        // Index in vector engine (requires embedding generation)
        #[cfg(feature = "vector-search")]
        {
            let embedding = self.vector_engine.embed_event(payload)?;
            let source_text = extract_source_text(payload);
            self.vector_engine
                .index_event(event_id, tenant_id, embedding, source_text)
                .await?;
        }

        Ok(())
    }

    /// Stub for when features are not enabled
    #[cfg(not(any(feature = "vector-search", feature = "keyword-search")))]
    pub fn index_event<'a>(
        &'a self,
        event_id: Uuid,
        _tenant_id: &str,
        event_type: &str,
        entity_id: Option<&str>,
        _payload: &serde_json::Value,
        timestamp: DateTime<Utc>,
    ) -> impl std::future::Future<Output = Result<()>> + 'a {
        let metadata = EventMetadata {
            event_type: Some(event_type.to_string()),
            entity_id: entity_id.map(str::to_string),
            timestamp: Some(timestamp),
        };
        futures::future::lazy(move |_| {
            self.store_metadata(event_id, metadata);
            Ok(())
        })
    }

    /// Commit changes to both engines
    #[cfg(feature = "keyword-search")]
    pub fn commit(&self) -> Result<()> {
        self.keyword_engine.commit()
    }

    #[cfg(not(feature = "keyword-search"))]
    pub fn commit(&self) -> Result<()> {
        Ok(())
    }

    /// Perform a search using the configured mode
    ///
    /// This is the main entry point for searching. It:
    /// 1. Runs semantic search (if natural language query and mode allows)
    /// 2. Runs keyword search (if keywords present and mode allows)
    /// 3. Combines and re-ranks scores using RRF
    /// 4. Applies metadata filters
    /// 5. Returns top-k results
    pub fn search(&self, query: &SearchQuery) -> Result<Vec<HybridSearchResult>> {
        // Update stats
        {
            let mut stats = self.stats.write();
            stats.total_searches += 1;
            match query.mode {
                SearchMode::SemanticOnly => stats.semantic_searches += 1,
                SearchMode::KeywordOnly => stats.keyword_searches += 1,
                SearchMode::Hybrid => stats.hybrid_searches += 1,
            }
        }

        // Calculate how many results to fetch (over-fetch for filtering)
        let fetch_limit = if query.filters.has_filters() {
            query.limit * self.config.over_fetch_multiplier
        } else {
            query.limit
        };

        match query.mode {
            SearchMode::SemanticOnly => self.search_semantic_only(query, fetch_limit),
            SearchMode::KeywordOnly => self.search_keyword_only(query, fetch_limit),
            SearchMode::Hybrid => self.search_hybrid(query, fetch_limit),
        }
    }

    /// Search using only semantic (vector) search
    fn search_semantic_only(
        &self,
        query: &SearchQuery,
        fetch_limit: usize,
    ) -> Result<Vec<HybridSearchResult>> {
        let semantic_results = self.run_semantic_search(query, fetch_limit)?;

        let mut results: Vec<HybridSearchResult> = semantic_results
            .into_iter()
            .map(|r| HybridSearchResult {
                event_id: r.event_id,
                score: r.score,
                semantic_score: Some(r.score),
                keyword_score: None,
                event_type: self.get_cached_event_type(r.event_id),
                entity_id: self.get_cached_entity_id(r.event_id),
                source_text: r.source_text,
            })
            .collect();

        // Apply metadata filters
        self.apply_filters(&mut results, &query.filters);

        // Apply minimum score threshold
        results.retain(|r| r.score >= self.config.min_score_threshold);

        // Truncate to requested limit
        results.truncate(query.limit);

        Ok(results)
    }

    /// Search using only keyword (BM25) search
    fn search_keyword_only(
        &self,
        query: &SearchQuery,
        fetch_limit: usize,
    ) -> Result<Vec<HybridSearchResult>> {
        let keyword_results = self.run_keyword_search(query, fetch_limit)?;

        // Normalize keyword scores to 0-1 range
        let max_score = keyword_results
            .iter()
            .map(|r| r.score)
            .fold(0.0_f32, f32::max);

        let mut results: Vec<HybridSearchResult> = keyword_results
            .into_iter()
            .map(|r| {
                let normalized_score = if max_score > 0.0 {
                    r.score / max_score
                } else {
                    0.0
                };
                HybridSearchResult {
                    event_id: r.event_id,
                    score: normalized_score,
                    semantic_score: None,
                    keyword_score: Some(r.score),
                    event_type: Some(r.event_type),
                    entity_id: r.entity_id,
                    source_text: None,
                }
            })
            .collect();

        // Apply metadata filters
        self.apply_filters(&mut results, &query.filters);

        // Apply minimum score threshold
        results.retain(|r| r.score >= self.config.min_score_threshold);

        // Truncate to requested limit
        results.truncate(query.limit);

        Ok(results)
    }

    /// Search using hybrid (combined) search with RRF score fusion
    fn search_hybrid(
        &self,
        query: &SearchQuery,
        fetch_limit: usize,
    ) -> Result<Vec<HybridSearchResult>> {
        // Run both searches
        let semantic_results = self.run_semantic_search(query, fetch_limit)?;
        let keyword_results = self.run_keyword_search(query, fetch_limit)?;

        // Build score maps with ranks
        let mut combined_scores: HashMap<Uuid, HybridSearchResult> = HashMap::new();

        // Process semantic results
        for (rank, result) in semantic_results.into_iter().enumerate() {
            let rrf_score = 1.0 / (self.config.rrf_k + rank as f32 + 1.0);
            let weighted_score = rrf_score * self.config.semantic_weight;

            combined_scores.insert(
                result.event_id,
                HybridSearchResult {
                    event_id: result.event_id,
                    score: weighted_score,
                    semantic_score: Some(result.score),
                    keyword_score: None,
                    event_type: self.get_cached_event_type(result.event_id),
                    entity_id: self.get_cached_entity_id(result.event_id),
                    source_text: result.source_text,
                },
            );
        }

        // Process keyword results and combine with semantic
        for (rank, result) in keyword_results.into_iter().enumerate() {
            let rrf_score = 1.0 / (self.config.rrf_k + rank as f32 + 1.0);
            let weighted_score = rrf_score * self.config.keyword_weight;

            if let Some(existing) = combined_scores.get_mut(&result.event_id) {
                // Combine scores
                existing.score += weighted_score;
                existing.keyword_score = Some(result.score);
                // Prefer keyword engine's event_type and entity_id as they're stored
                existing.event_type = Some(result.event_type);
                existing.entity_id = result.entity_id;
            } else {
                combined_scores.insert(
                    result.event_id,
                    HybridSearchResult {
                        event_id: result.event_id,
                        score: weighted_score,
                        semantic_score: None,
                        keyword_score: Some(result.score),
                        event_type: Some(result.event_type),
                        entity_id: result.entity_id,
                        source_text: None,
                    },
                );
            }
        }

        // Convert to sorted vector
        let mut results: Vec<HybridSearchResult> = combined_scores.into_values().collect();
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Apply metadata filters
        self.apply_filters(&mut results, &query.filters);

        // Apply minimum score threshold
        results.retain(|r| r.score >= self.config.min_score_threshold);

        // Truncate to requested limit
        results.truncate(query.limit);

        Ok(results)
    }

    /// Run semantic search
    fn run_semantic_search(
        &self,
        query: &SearchQuery,
        limit: usize,
    ) -> Result<Vec<SimilarityResult>> {
        // Generate query embedding
        let query_embedding = self.vector_engine.embed_text(&query.query)?;

        let similarity_query = SimilarityQuery::new(query_embedding, limit)
            .with_min_similarity(query.min_similarity.unwrap_or(0.0));

        let similarity_query = if let Some(ref tenant_id) = query.tenant_id {
            similarity_query.with_tenant(tenant_id.clone())
        } else {
            similarity_query
        };

        self.vector_engine.search_similar(&similarity_query)
    }

    /// Run keyword search
    fn run_keyword_search(
        &self,
        query: &SearchQuery,
        limit: usize,
    ) -> Result<Vec<KeywordSearchResult>> {
        let keyword_query = KeywordQuery::new(&query.query).with_limit(limit);

        let keyword_query = if let Some(ref tenant_id) = query.tenant_id {
            keyword_query.with_tenant(tenant_id)
        } else {
            keyword_query
        };

        self.keyword_engine.search_keywords(&keyword_query)
    }

    /// Apply metadata filters to results
    fn apply_filters(&self, results: &mut Vec<HybridSearchResult>, filters: &MetadataFilters) {
        if !filters.has_filters() {
            return;
        }

        let metadata_cache = self.metadata_cache.read();

        results.retain(|result| {
            // Get cached metadata
            let cached = metadata_cache.get(&result.event_id);

            // Filter by event_type
            if let Some(ref filter_type) = filters.event_type {
                let event_type = result
                    .event_type
                    .as_ref()
                    .or_else(|| cached.and_then(|m| m.event_type.as_ref()));
                match event_type {
                    Some(t) if t == filter_type => {}
                    _ => return false,
                }
            }

            // Filter by entity_id
            if let Some(ref filter_entity) = filters.entity_id {
                let entity_id = result
                    .entity_id
                    .as_ref()
                    .or_else(|| cached.and_then(|m| m.entity_id.as_ref()));
                match entity_id {
                    Some(e) if e == filter_entity => {}
                    _ => return false,
                }
            }

            // Filter by time range
            if filters.time_from.is_some() || filters.time_to.is_some() {
                if let Some(timestamp) = cached.and_then(|m| m.timestamp) {
                    if let Some(from) = filters.time_from
                        && timestamp < from
                    {
                        return false;
                    }
                    if let Some(to) = filters.time_to
                        && timestamp > to
                    {
                        return false;
                    }
                } else {
                    // No timestamp available, exclude if time filter is required
                    return false;
                }
            }

            true
        });
    }

    /// Get cached event type for an event
    fn get_cached_event_type(&self, event_id: Uuid) -> Option<String> {
        self.metadata_cache
            .read()
            .get(&event_id)
            .and_then(|m| m.event_type.clone())
    }

    /// Get cached entity ID for an event
    fn get_cached_entity_id(&self, event_id: Uuid) -> Option<String> {
        self.metadata_cache
            .read()
            .get(&event_id)
            .and_then(|m| m.entity_id.clone())
    }

    /// Get engine statistics
    pub fn stats(&self) -> (u64, u64, u64, u64) {
        let stats = self.stats.read();
        (
            stats.total_searches,
            stats.semantic_searches,
            stats.keyword_searches,
            stats.hybrid_searches,
        )
    }

    /// Health check for both engines
    pub fn health_check(&self) -> Result<()> {
        self.vector_engine.health_check()?;
        self.keyword_engine.health_check()?;
        Ok(())
    }

    /// Get the number of events in the metadata cache
    pub fn cached_metadata_count(&self) -> usize {
        self.metadata_cache.read().len()
    }

    /// Clear the metadata cache
    pub fn clear_metadata_cache(&self) {
        self.metadata_cache.write().clear();
    }
}

/// Extract source text from a payload for the vector search engine
#[cfg(feature = "vector-search")]
fn extract_source_text(payload: &serde_json::Value) -> Option<String> {
    let priority_fields = [
        "content",
        "text",
        "body",
        "message",
        "description",
        "title",
        "name",
    ];

    if let serde_json::Value::Object(map) = payload {
        for field in priority_fields {
            if let Some(serde_json::Value::String(s)) = map.get(field)
                && !s.is_empty()
            {
                return Some(s.clone());
            }
        }
    }

    // Fallback: convert entire payload to string
    Some(payload.to_string())
}

#[cfg(all(test, not(any(feature = "vector-search", feature = "keyword-search"))))]
mod deferred_tests;

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> HybridSearchEngineConfig {
        HybridSearchEngineConfig {
            semantic_weight: 0.5,
            keyword_weight: 0.5,
            rrf_k: 60.0,
            default_limit: 10,
            min_score_threshold: 0.0,
            over_fetch_multiplier: 3,
        }
    }

    #[test]
    fn test_config_default() {
        let config = HybridSearchEngineConfig::default();
        assert_eq!(config.semantic_weight, 0.5);
        assert_eq!(config.keyword_weight, 0.5);
        assert_eq!(config.rrf_k, 60.0);
        assert_eq!(config.default_limit, 10);
    }

    #[test]
    fn test_search_query_builder() {
        let query = SearchQuery::new("test query")
            .with_limit(20)
            .with_tenant("tenant-1")
            .with_mode(SearchMode::Hybrid)
            .with_event_type("UserCreated")
            .with_entity_id("user-123")
            .with_min_similarity(0.7);

        assert_eq!(query.query, "test query");
        assert_eq!(query.limit, 20);
        assert_eq!(query.tenant_id, Some("tenant-1".to_string()));
        assert_eq!(query.mode, SearchMode::Hybrid);
        assert_eq!(query.filters.event_type, Some("UserCreated".to_string()));
        assert_eq!(query.filters.entity_id, Some("user-123".to_string()));
        assert_eq!(query.min_similarity, Some(0.7));
    }

    #[test]
    fn test_search_query_semantic_constructor() {
        let query = SearchQuery::semantic("natural language query");
        assert_eq!(query.mode, SearchMode::SemanticOnly);
    }

    #[test]
    fn test_search_query_keyword_constructor() {
        let query = SearchQuery::keyword("keyword search");
        assert_eq!(query.mode, SearchMode::KeywordOnly);
    }

    #[test]
    fn test_metadata_filters_builder() {
        let now = Utc::now();
        let past = now - chrono::Duration::hours(1);

        let filters = MetadataFilters::new()
            .with_event_type("OrderPlaced")
            .with_entity_id("order-456")
            .with_time_range(past, now);

        assert_eq!(filters.event_type, Some("OrderPlaced".to_string()));
        assert_eq!(filters.entity_id, Some("order-456".to_string()));
        assert!(filters.time_from.is_some());
        assert!(filters.time_to.is_some());
        assert!(filters.has_filters());
    }

    #[test]
    fn test_metadata_filters_has_filters() {
        let empty = MetadataFilters::new();
        assert!(!empty.has_filters());

        let with_type = MetadataFilters::new().with_event_type("Test");
        assert!(with_type.has_filters());

        let with_entity = MetadataFilters::new().with_entity_id("e1");
        assert!(with_entity.has_filters());

        let with_time = MetadataFilters::new().with_time_from(Utc::now());
        assert!(with_time.has_filters());
    }

    #[test]
    fn test_search_mode_default() {
        let mode: SearchMode = Default::default();
        assert_eq!(mode, SearchMode::Hybrid);
    }

    #[test]
    fn test_hybrid_search_result_structure() {
        let result = HybridSearchResult {
            event_id: Uuid::new_v4(),
            score: 0.85,
            semantic_score: Some(0.9),
            keyword_score: Some(0.8),
            event_type: Some("UserCreated".to_string()),
            entity_id: Some("user-123".to_string()),
            source_text: Some("Test content".to_string()),
        };

        assert!(result.score > 0.0);
        assert!(result.semantic_score.is_some());
        assert!(result.keyword_score.is_some());
    }

    #[test]
    fn test_event_metadata_default() {
        let metadata = EventMetadata::default();
        assert!(metadata.event_type.is_none());
        assert!(metadata.entity_id.is_none());
        assert!(metadata.timestamp.is_none());
    }

    #[test]
    fn test_rrf_score_calculation() {
        // Verify RRF formula: 1 / (k + rank + 1)
        let k = 60.0_f32;

        // Rank 0 (first result)
        let score_rank_0 = 1.0 / (k + 0.0 + 1.0);
        assert!((score_rank_0 - 0.01639344).abs() < 0.0001);

        // Rank 1 (second result)
        let score_rank_1 = 1.0 / (k + 1.0 + 1.0);
        assert!((score_rank_1 - 0.01612903).abs() < 0.0001);

        // Verify rank 0 > rank 1
        assert!(score_rank_0 > score_rank_1);
    }

    #[test]
    fn test_config_weights_validation() {
        let config = create_test_config();

        // Weights should sum to 1.0 for balanced hybrid search
        assert!((config.semantic_weight + config.keyword_weight - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_search_query_with_time_range() {
        let now = Utc::now();
        let past = now - chrono::Duration::days(7);

        let query = SearchQuery::new("test").with_time_range(past, now);

        assert_eq!(query.filters.time_from, Some(past));
        assert_eq!(query.filters.time_to, Some(now));
    }

    #[test]
    fn test_search_query_serialization() {
        let query = SearchQuery::new("test query")
            .with_limit(5)
            .with_tenant("tenant-1")
            .with_mode(SearchMode::Hybrid);

        // Should serialize without error
        let json = serde_json::to_string(&query);
        assert!(json.is_ok());

        // Should deserialize without error
        let deserialized: std::result::Result<SearchQuery, _> =
            serde_json::from_str(&json.unwrap());
        assert!(deserialized.is_ok());

        let deserialized = deserialized.unwrap();
        assert_eq!(deserialized.query, "test query");
        assert_eq!(deserialized.limit, 5);
    }

    #[test]
    fn test_hybrid_search_result_serialization() {
        let result = HybridSearchResult {
            event_id: Uuid::new_v4(),
            score: 0.85,
            semantic_score: Some(0.9),
            keyword_score: Some(0.8),
            event_type: Some("Test".to_string()),
            entity_id: None,
            source_text: None,
        };

        let json = serde_json::to_string(&result);
        assert!(json.is_ok());
    }
}

/// Integration tests that require both search features enabled
#[cfg(test)]
#[cfg(all(feature = "vector-search", feature = "keyword-search"))]
mod integration_tests {
    use super::*;
    use crate::infrastructure::search::{KeywordSearchEngineConfig, VectorSearchEngineConfig};

    // VectorSearchEngine::with_config eagerly calls TextEmbedding::try_new, which
    // retrieves the ONNX model. cargo runs these tests in parallel, so on a cold
    // cache three concurrent retrievals race and one loses with
    // "Failed to retrieve model.onnx" — reproducibly the third test. Serialize
    // construction so the first call populates the cache and the rest read it.
    static MODEL_INIT: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn create_test_engines() -> (Arc<VectorSearchEngine>, Arc<KeywordSearchEngine>) {
        let vector_engine = {
            // A poisoned lock only means another test panicked; the cache is still
            // usable, so recover rather than cascading the failure.
            let _guard = MODEL_INIT
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            Arc::new(
                VectorSearchEngine::with_config(VectorSearchEngineConfig {
                    default_similarity_threshold: 0.0,
                    ..Default::default()
                })
                .unwrap(),
            )
        };

        let keyword_engine = Arc::new(
            KeywordSearchEngine::with_config(KeywordSearchEngineConfig {
                auto_commit: true,
                ..Default::default()
            })
            .unwrap(),
        );

        (vector_engine, keyword_engine)
    }

    #[tokio::test]
    async fn test_hybrid_search_integration() {
        let (vector_engine, keyword_engine) = create_test_engines();
        let hybrid_engine = HybridSearchEngine::new(vector_engine, keyword_engine);

        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        // Index events
        hybrid_engine
            .index_event(
                id1,
                "tenant-1",
                "UserCreated",
                Some("user-123"),
                &serde_json::json!({"name": "Alice", "email": "alice@example.com"}),
                Utc::now(),
            )
            .await
            .unwrap();

        hybrid_engine
            .index_event(
                id2,
                "tenant-1",
                "OrderPlaced",
                Some("order-456"),
                &serde_json::json!({"product": "Widget", "quantity": 5}),
                Utc::now(),
            )
            .await
            .unwrap();

        // Hybrid search
        let query = SearchQuery::new("Alice user").with_tenant("tenant-1");
        let results = hybrid_engine.search(&query).unwrap();

        assert!(!results.is_empty());
        // Alice result should be first
        assert_eq!(results[0].event_id, id1);
    }

    #[tokio::test]
    async fn test_search_with_event_type_filter() {
        let (vector_engine, keyword_engine) = create_test_engines();
        let hybrid_engine = HybridSearchEngine::new(vector_engine, keyword_engine);

        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        hybrid_engine
            .index_event(
                id1,
                "tenant-1",
                "UserCreated",
                None,
                &serde_json::json!({"data": "test user"}),
                Utc::now(),
            )
            .await
            .unwrap();

        hybrid_engine
            .index_event(
                id2,
                "tenant-1",
                "OrderPlaced",
                None,
                &serde_json::json!({"data": "test order"}),
                Utc::now(),
            )
            .await
            .unwrap();

        // Search with event_type filter
        let query = SearchQuery::new("test")
            .with_tenant("tenant-1")
            .with_event_type("UserCreated");
        let results = hybrid_engine.search(&query).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].event_type, Some("UserCreated".to_string()));
    }

    #[tokio::test]
    async fn test_search_with_time_range_filter() {
        let (vector_engine, keyword_engine) = create_test_engines();
        let hybrid_engine = HybridSearchEngine::new(vector_engine, keyword_engine);

        let now = Utc::now();
        let past = now - chrono::Duration::hours(2);
        let recent = now - chrono::Duration::minutes(30);

        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        // Old event
        hybrid_engine
            .index_event(
                id1,
                "tenant-1",
                "Event",
                None,
                &serde_json::json!({"data": "old event"}),
                past,
            )
            .await
            .unwrap();

        // Recent event
        hybrid_engine
            .index_event(
                id2,
                "tenant-1",
                "Event",
                None,
                &serde_json::json!({"data": "recent event"}),
                recent,
            )
            .await
            .unwrap();

        // Search for events in the last hour
        let one_hour_ago = now - chrono::Duration::hours(1);
        let query = SearchQuery::new("event")
            .with_tenant("tenant-1")
            .with_time_range(one_hour_ago, now);
        let results = hybrid_engine.search(&query).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].event_id, id2);
    }

    #[test]
    fn test_health_check() {
        let (vector_engine, keyword_engine) = create_test_engines();
        let hybrid_engine = HybridSearchEngine::new(vector_engine, keyword_engine);
        assert!(hybrid_engine.health_check().is_ok());
    }

    #[test]
    fn test_stats() {
        let (vector_engine, keyword_engine) = create_test_engines();
        let hybrid_engine = HybridSearchEngine::new(vector_engine, keyword_engine);

        let (total, semantic, keyword, hybrid) = hybrid_engine.stats();
        assert_eq!(total, 0);
        assert_eq!(semantic, 0);
        assert_eq!(keyword, 0);
        assert_eq!(hybrid, 0);
    }
}
