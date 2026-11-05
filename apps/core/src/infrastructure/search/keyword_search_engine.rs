//! KeywordSearchEngine - BM25-based full-text search using Tantivy
//!
//! This module provides:
//! - Full-text search using BM25 ranking algorithm
//! - Indexing of event_type, payload (JSON), and entity_id fields
//! - Query parser for complex queries (boolean, phrase, wildcard)
//! - Incremental index updates
//! - Multi-tenant support

#[cfg(feature = "keyword-search")]
use tantivy::{
    Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument,
    collector::TopDocs,
    doc,
    query::QueryParser,
    schema::{
        Field, IndexRecordOption, STORED, STRING, Schema, TEXT, TextFieldIndexing, TextOptions,
        Value,
    },
};

#[cfg(feature = "keyword-search")]
use crate::error::AllSourceError;
use crate::error::Result;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use uuid::Uuid;

/// Configuration for the KeywordSearchEngine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeywordSearchEngineConfig {
    /// Heap size for the index writer in bytes (default: 50MB)
    pub writer_heap_size: usize,
    /// Number of results to return by default
    pub default_limit: usize,
    /// Whether to commit after each index operation (slower but safer)
    pub auto_commit: bool,
}

impl Default for KeywordSearchEngineConfig {
    fn default() -> Self {
        Self {
            writer_heap_size: 50_000_000, // 50MB
            default_limit: 10,
            auto_commit: false,
        }
    }
}

/// A document indexed in the keyword search engine
#[derive(Debug, Clone)]
pub struct IndexedDocument {
    pub event_id: Uuid,
    pub tenant_id: String,
    pub event_type: String,
    pub entity_id: Option<String>,
    pub payload_text: String,
}

/// Result of a keyword search
#[derive(Debug, Clone)]
pub struct KeywordSearchResult {
    pub event_id: Uuid,
    pub score: f32,
    pub event_type: String,
    pub entity_id: Option<String>,
    pub highlights: Option<String>,
}

/// Query parameters for keyword search
#[derive(Debug, Clone)]
pub struct KeywordQuery {
    /// The search query string
    pub query: String,
    /// Maximum number of results to return
    pub limit: usize,
    /// Optional tenant filter
    pub tenant_id: Option<String>,
    /// Fields to search (default: all searchable fields)
    pub fields: Option<Vec<String>>,
}

impl KeywordQuery {
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            limit: 10,
            tenant_id: None,
            fields: None,
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

    pub fn with_fields(mut self, fields: Vec<String>) -> Self {
        self.fields = Some(fields);
        self
    }
}

/// Schema field references for the keyword search index
#[cfg(feature = "keyword-search")]
#[derive(Clone)]
struct SchemaFields {
    event_id: Field,
    tenant_id: Field,
    event_type: Field,
    entity_id: Field,
    payload: Field,
}

/// KeywordSearchEngine with BM25 ranking
///
/// Features:
/// - Full-text search using Tantivy (Rust Lucene alternative)
/// - BM25 ranking for relevance scoring
/// - Query parser for complex queries (AND, OR, NOT, phrases, wildcards)
/// - Incremental index updates
/// - Multi-tenant support
pub struct KeywordSearchEngine {
    config: KeywordSearchEngineConfig,
    #[cfg(feature = "keyword-search")]
    index: Index,
    #[cfg(feature = "keyword-search")]
    schema_fields: SchemaFields,
    #[cfg(feature = "keyword-search")]
    writer: Arc<RwLock<IndexWriter>>,
    #[cfg(feature = "keyword-search")]
    reader: IndexReader,
    #[cfg(not(feature = "keyword-search"))]
    fallback: super::keyword_fallback::InMemoryKeywordIndex,
    /// Document count per tenant
    tenant_counts: Arc<RwLock<HashMap<String, usize>>>,
    /// Statistics
    stats: Arc<RwLock<EngineStats>>,
}

#[derive(Debug, Default, Clone)]
struct EngineStats {
    total_indexed: u64,
    total_searches: u64,
    total_deleted: u64,
}

#[cfg(feature = "keyword-search")]
fn build_schema() -> (Schema, SchemaFields) {
    let mut schema_builder = Schema::builder();

    // event_id: stored for retrieval, not analyzed
    let event_id = schema_builder.add_text_field("event_id", STRING | STORED);

    // tenant_id: for filtering, not stored
    let tenant_id = schema_builder.add_text_field("tenant_id", STRING);

    // event_type: searchable and stored
    let event_type = schema_builder.add_text_field("event_type", TEXT | STORED);

    // entity_id: searchable and stored
    let entity_id = schema_builder.add_text_field("entity_id", TEXT | STORED);

    // payload: full-text searchable, stored for highlights
    let text_options = TextOptions::default()
        .set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("en_stem")
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        )
        .set_stored();
    let payload = schema_builder.add_text_field("payload", text_options);

    let schema = schema_builder.build();
    let fields = SchemaFields {
        event_id,
        tenant_id,
        event_type,
        entity_id,
        payload,
    };

    (schema, fields)
}

impl KeywordSearchEngine {
    /// Create a new KeywordSearchEngine with default configuration (in-memory index)
    #[cfg(feature = "keyword-search")]
    pub fn new() -> Result<Self> {
        Self::with_config(KeywordSearchEngineConfig::default())
    }

    /// Create a new KeywordSearchEngine with custom configuration (in-memory index)
    #[cfg(feature = "keyword-search")]
    pub fn with_config(config: KeywordSearchEngineConfig) -> Result<Self> {
        let (schema, schema_fields) = build_schema();

        // Create in-memory index
        let index = Index::create_in_ram(schema);

        // Create writer
        let writer = index.writer(config.writer_heap_size).map_err(|e| {
            AllSourceError::InternalError(format!("Failed to create index writer: {e}"))
        })?;

        // Create reader with automatic reload
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .map_err(|e| {
                AllSourceError::InternalError(format!("Failed to create index reader: {e}"))
            })?;

        Ok(Self {
            config,
            index,
            schema_fields,
            writer: Arc::new(RwLock::new(writer)),
            reader,
            tenant_counts: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(EngineStats::default())),
        })
    }

    /// Create a new KeywordSearchEngine with a directory-backed index
    #[cfg(feature = "keyword-search")]
    pub fn with_directory(
        path: impl AsRef<std::path::Path>,
        config: KeywordSearchEngineConfig,
    ) -> Result<Self> {
        let (schema, schema_fields) = build_schema();
        let path_ref = path.as_ref();

        // Create or open directory-backed index
        let index = Index::create_in_dir(path_ref, schema)
            .or_else(|_| Index::open_in_dir(path_ref))
            .map_err(|e| {
                AllSourceError::InternalError(format!("Failed to create/open index: {e}"))
            })?;

        // Create writer
        let writer = index.writer(config.writer_heap_size).map_err(|e| {
            AllSourceError::InternalError(format!("Failed to create index writer: {e}"))
        })?;

        // Create reader
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .map_err(|e| {
                AllSourceError::InternalError(format!("Failed to create index reader: {e}"))
            })?;

        Ok(Self {
            config,
            index,
            schema_fields,
            writer: Arc::new(RwLock::new(writer)),
            reader,
            tenant_counts: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(EngineStats::default())),
        })
    }

    /// Without `keyword-search` the engine ranks with an in-memory BM25 index
    /// instead of Tantivy; plain terms only, no query syntax.
    #[cfg(not(feature = "keyword-search"))]
    pub fn new() -> Result<Self> {
        Self::with_config(KeywordSearchEngineConfig::default())
    }

    #[cfg(not(feature = "keyword-search"))]
    pub fn with_config(config: KeywordSearchEngineConfig) -> Result<Self> {
        Ok(Self {
            config,
            fallback: super::keyword_fallback::InMemoryKeywordIndex::default(),
            tenant_counts: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(EngineStats::default())),
        })
    }

    /// Get the engine configuration
    pub fn config(&self) -> &KeywordSearchEngineConfig {
        &self.config
    }

    /// Index an event for keyword search
    ///
    /// Extracts text content from the event payload and indexes:
    /// - event_type
    /// - entity_id (if present)
    /// - payload (JSON converted to text)
    #[cfg(feature = "keyword-search")]
    pub fn index_event(
        &self,
        event_id: Uuid,
        tenant_id: &str,
        event_type: &str,
        entity_id: Option<&str>,
        payload: &serde_json::Value,
    ) -> Result<()> {
        let payload_text = self.extract_text_from_payload(payload);

        let mut writer = self.writer.write();

        // Create the document
        let mut doc = TantivyDocument::new();
        doc.add_text(self.schema_fields.event_id, event_id.to_string());
        doc.add_text(self.schema_fields.tenant_id, tenant_id);
        doc.add_text(self.schema_fields.event_type, event_type);
        if let Some(eid) = entity_id {
            doc.add_text(self.schema_fields.entity_id, eid);
        }
        doc.add_text(self.schema_fields.payload, &payload_text);

        writer
            .add_document(doc)
            .map_err(|e| AllSourceError::InternalError(format!("Failed to add document: {e}")))?;

        // Update tenant counts
        {
            let mut counts = self.tenant_counts.write();
            *counts.entry(tenant_id.to_string()).or_insert(0) += 1;
        }

        // Update stats
        {
            let mut stats = self.stats.write();
            stats.total_indexed += 1;
        }

        // Auto-commit if configured
        if self.config.auto_commit {
            writer
                .commit()
                .map_err(|e| AllSourceError::InternalError(format!("Failed to commit: {e}")))?;
            // Reload reader to see changes immediately
            self.reader.reload().map_err(|e| {
                AllSourceError::InternalError(format!("Failed to reload reader: {e}"))
            })?;
        }

        Ok(())
    }

    #[cfg(not(feature = "keyword-search"))]
    pub fn index_event(
        &self,
        event_id: Uuid,
        tenant_id: &str,
        event_type: &str,
        entity_id: Option<&str>,
        payload: &serde_json::Value,
    ) -> Result<()> {
        let payload_text = self.extract_text_from_payload(payload);
        let is_new = self.fallback.insert(IndexedDocument {
            event_id,
            tenant_id: tenant_id.to_string(),
            event_type: event_type.to_string(),
            entity_id: entity_id.map(str::to_string),
            payload_text,
        });
        if is_new {
            *self
                .tenant_counts
                .write()
                .entry(tenant_id.to_string())
                .or_insert(0) += 1;
        }
        self.stats.write().total_indexed += 1;
        Ok(())
    }

    /// Commit pending changes to the index
    #[cfg(feature = "keyword-search")]
    pub fn commit(&self) -> Result<()> {
        let mut writer = self.writer.write();
        writer
            .commit()
            .map_err(|e| AllSourceError::InternalError(format!("Failed to commit: {e}")))?;
        // Reload the reader to see the committed changes immediately
        self.reader
            .reload()
            .map_err(|e| AllSourceError::InternalError(format!("Failed to reload reader: {e}")))?;
        Ok(())
    }

    #[cfg(not(feature = "keyword-search"))]
    pub fn commit(&self) -> Result<()> {
        Ok(())
    }

    /// Search for events using keyword query with BM25 ranking
    ///
    /// Supports complex queries:
    /// - Boolean: "rust AND performance" or "rust OR go"
    /// - Phrases: "\"exact phrase\""
    /// - Field-specific: "event_type:UserCreated"
    /// - Wildcards: "user*"
    /// - Negation: "-deprecated"
    #[cfg(feature = "keyword-search")]
    pub fn search_keywords(&self, query: &KeywordQuery) -> Result<Vec<KeywordSearchResult>> {
        // Update stats
        {
            let mut stats = self.stats.write();
            stats.total_searches += 1;
        }

        let searcher = self.reader.searcher();

        // Determine which fields to search
        let search_fields = if let Some(ref fields) = query.fields {
            let mut f = Vec::new();
            for field_name in fields {
                match field_name.as_str() {
                    "event_type" => f.push(self.schema_fields.event_type),
                    "entity_id" => f.push(self.schema_fields.entity_id),
                    "payload" => f.push(self.schema_fields.payload),
                    _ => {}
                }
            }
            f
        } else {
            vec![
                self.schema_fields.event_type,
                self.schema_fields.entity_id,
                self.schema_fields.payload,
            ]
        };

        // Build query with optional tenant filter
        let query_str = if let Some(ref tenant_id) = query.tenant_id {
            format!("({}) AND tenant_id:{}", query.query, tenant_id)
        } else {
            query.query.clone()
        };

        // Add tenant_id to fields for the query parser when filtering by tenant
        let mut all_fields = search_fields.clone();
        if query.tenant_id.is_some() {
            all_fields.push(self.schema_fields.tenant_id);
        }

        let query_parser = QueryParser::for_index(&self.index, all_fields);
        let parsed_query = query_parser
            .parse_query(&query_str)
            .map_err(|e| AllSourceError::InvalidInput(format!("Invalid query: {e}")))?;

        // Execute search with BM25 scoring.
        // tantivy 0.26 made `TopDocs` a builder: it no longer implements
        // `Collector` directly. `.order_by_score()` yields the BM25-ranked
        // collector with `Fruit = Vec<(Score, DocAddress)>` — the same shape the
        // result loop below destructures as `(score, doc_address)`.
        let top_docs = searcher
            .search(
                &parsed_query,
                &TopDocs::with_limit(query.limit).order_by_score(),
            )
            .map_err(|e| AllSourceError::InternalError(format!("Search failed: {e}")))?;

        // Collect results
        let mut results = Vec::with_capacity(top_docs.len());
        for (score, doc_address) in top_docs {
            let doc: TantivyDocument = searcher.doc(doc_address).map_err(|e| {
                AllSourceError::InternalError(format!("Failed to retrieve document: {e}"))
            })?;

            // Extract fields from document
            let event_id_str = doc
                .get_first(self.schema_fields.event_id)
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    AllSourceError::InternalError("Missing event_id in document".to_string())
                })?;

            let event_id = Uuid::parse_str(event_id_str)
                .map_err(|e| AllSourceError::InternalError(format!("Invalid event_id: {e}")))?;

            let event_type = doc
                .get_first(self.schema_fields.event_type)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let entity_id = doc
                .get_first(self.schema_fields.entity_id)
                .and_then(|v| v.as_str())
                .map(std::string::ToString::to_string);

            results.push(KeywordSearchResult {
                event_id,
                score,
                event_type,
                entity_id,
                highlights: None, // Could implement snippet extraction here
            });
        }

        Ok(results)
    }

    #[cfg(not(feature = "keyword-search"))]
    pub fn search_keywords(&self, query: &KeywordQuery) -> Result<Vec<KeywordSearchResult>> {
        self.stats.write().total_searches += 1;
        Ok(self.fallback.search(query))
    }

    /// Delete an event from the index by event_id
    #[cfg(feature = "keyword-search")]
    pub fn delete_event(&self, event_id: Uuid) -> Result<()> {
        use tantivy::Term;

        let mut writer = self.writer.write();
        let term = Term::from_field_text(self.schema_fields.event_id, &event_id.to_string());
        writer.delete_term(term);

        // Update stats
        {
            let mut stats = self.stats.write();
            stats.total_deleted += 1;
        }

        if self.config.auto_commit {
            writer
                .commit()
                .map_err(|e| AllSourceError::InternalError(format!("Failed to commit: {e}")))?;
            self.reader.reload().map_err(|e| {
                AllSourceError::InternalError(format!("Failed to reload reader: {e}"))
            })?;
        }

        Ok(())
    }

    #[cfg(not(feature = "keyword-search"))]
    pub fn delete_event(&self, event_id: Uuid) -> Result<()> {
        if let Some(tenant_id) = self.fallback.remove(event_id)
            && let Some(count) = self.tenant_counts.write().get_mut(&tenant_id)
        {
            *count = count.saturating_sub(1);
        }
        self.stats.write().total_deleted += 1;
        Ok(())
    }

    /// Delete all events for a tenant
    #[cfg(feature = "keyword-search")]
    pub fn delete_by_tenant(&self, tenant_id: &str) -> Result<usize> {
        use tantivy::Term;

        let count = {
            let mut counts = self.tenant_counts.write();
            counts.remove(tenant_id).unwrap_or(0)
        };

        let mut writer = self.writer.write();
        let term = Term::from_field_text(self.schema_fields.tenant_id, tenant_id);
        writer.delete_term(term);

        // Update stats
        {
            let mut stats = self.stats.write();
            stats.total_deleted += count as u64;
        }

        if self.config.auto_commit {
            writer
                .commit()
                .map_err(|e| AllSourceError::InternalError(format!("Failed to commit: {e}")))?;
            self.reader.reload().map_err(|e| {
                AllSourceError::InternalError(format!("Failed to reload reader: {e}"))
            })?;
        }

        Ok(count)
    }

    #[cfg(not(feature = "keyword-search"))]
    pub fn delete_by_tenant(&self, tenant_id: &str) -> Result<usize> {
        let removed = self.fallback.remove_tenant(tenant_id);
        self.tenant_counts.write().remove(tenant_id);
        self.stats.write().total_deleted += removed as u64;
        Ok(removed)
    }

    /// Get the number of indexed documents
    pub fn count(&self, tenant_id: Option<&str>) -> usize {
        if let Some(tid) = tenant_id {
            self.tenant_counts.read().get(tid).copied().unwrap_or(0)
        } else {
            self.tenant_counts.read().values().sum()
        }
    }

    /// Get engine statistics
    pub fn stats(&self) -> (u64, u64, u64) {
        let stats = self.stats.read();
        (
            stats.total_indexed,
            stats.total_searches,
            stats.total_deleted,
        )
    }

    /// Extract searchable text from a JSON payload
    fn extract_text_from_payload(&self, payload: &serde_json::Value) -> String {
        let mut text_parts = Vec::new();
        Self::collect_text_recursive(payload, &mut text_parts);
        text_parts.join(" ")
    }

    fn collect_text_recursive(value: &serde_json::Value, parts: &mut Vec<String>) {
        match value {
            serde_json::Value::String(s) => {
                if !s.is_empty() {
                    parts.push(s.clone());
                }
            }
            serde_json::Value::Object(map) => {
                for (key, val) in map {
                    // Skip internal/metadata fields
                    if !key.starts_with('_') && key != "id" && key != "timestamp" {
                        Self::collect_text_recursive(val, parts);
                    }
                }
            }
            serde_json::Value::Array(arr) => {
                for item in arr {
                    Self::collect_text_recursive(item, parts);
                }
            }
            serde_json::Value::Number(n) => {
                parts.push(n.to_string());
            }
            serde_json::Value::Bool(b) => {
                parts.push(b.to_string());
            }
            serde_json::Value::Null => {}
        }
    }

    /// Health check
    pub fn health_check(&self) -> Result<()> {
        #[cfg(feature = "keyword-search")]
        {
            // Try to get a searcher - this will fail if the index is corrupt
            let _ = self.reader.searcher();
        }
        Ok(())
    }
}

/// Unit tests for KeywordSearchEngine
///
/// These tests run without the `keyword-search` feature enabled for basic tests,
/// and with the feature for integration tests.
#[cfg(test)]
#[cfg(not(feature = "keyword-search"))]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_test_engine() -> KeywordSearchEngine {
        KeywordSearchEngine::with_config(KeywordSearchEngineConfig::default()).unwrap()
    }

    #[test]
    fn test_config_default() {
        let config = KeywordSearchEngineConfig::default();
        assert_eq!(config.writer_heap_size, 50_000_000);
        assert_eq!(config.default_limit, 10);
        assert!(!config.auto_commit);
    }

    #[test]
    fn test_keyword_query_builder() {
        let query = KeywordQuery::new("test query")
            .with_limit(20)
            .with_tenant("tenant-1")
            .with_fields(vec!["payload".to_string()]);

        assert_eq!(query.query, "test query");
        assert_eq!(query.limit, 20);
        assert_eq!(query.tenant_id, Some("tenant-1".to_string()));
        assert_eq!(query.fields, Some(vec!["payload".to_string()]));
    }

    #[test]
    fn test_extract_text_from_string_payload() {
        let engine = create_test_engine();
        let payload = json!("Hello world");
        let text = engine.extract_text_from_payload(&payload);
        assert_eq!(text, "Hello world");
    }

    #[test]
    fn test_extract_text_from_object_payload() {
        let engine = create_test_engine();
        let payload = json!({
            "title": "Test Title",
            "content": "Test content here",
            "count": 42
        });
        let text = engine.extract_text_from_payload(&payload);
        assert!(text.contains("Test Title"));
        assert!(text.contains("Test content here"));
        assert!(text.contains("42"));
    }

    #[test]
    fn test_extract_text_skips_internal_fields() {
        let engine = create_test_engine();
        let payload = json!({
            "_internal": "should skip",
            "id": "123",
            "timestamp": "2024-01-01",
            "content": "visible content"
        });
        let text = engine.extract_text_from_payload(&payload);
        assert!(text.contains("visible content"));
        assert!(!text.contains("should skip"));
    }

    #[test]
    fn test_extract_text_from_array() {
        let engine = create_test_engine();
        let payload = json!(["one", "two", "three"]);
        let text = engine.extract_text_from_payload(&payload);
        assert!(text.contains("one"));
        assert!(text.contains("two"));
        assert!(text.contains("three"));
    }

    #[test]
    fn test_extract_text_from_nested() {
        let engine = create_test_engine();
        let payload = json!({
            "user": {
                "name": "John",
                "email": "john@example.com"
            },
            "tags": ["rust", "search"]
        });
        let text = engine.extract_text_from_payload(&payload);
        assert!(text.contains("John"));
        assert!(text.contains("john@example.com"));
        assert!(text.contains("rust"));
        assert!(text.contains("search"));
    }

    #[test]
    fn test_count_without_feature() {
        let engine = create_test_engine();
        assert_eq!(engine.count(None), 0);
    }

    #[test]
    fn test_stats_without_feature() {
        let engine = create_test_engine();
        let (indexed, searches, deleted) = engine.stats();
        assert_eq!(indexed, 0);
        assert_eq!(searches, 0);
        assert_eq!(deleted, 0);
    }

    #[test]
    fn test_health_check() {
        let engine = create_test_engine();
        assert!(engine.health_check().is_ok());
    }

    #[test]
    fn indexed_event_is_found_by_payload_term_without_tantivy() {
        let engine = create_test_engine();
        let event_id = Uuid::new_v4();
        engine
            .index_event(
                event_id,
                "tenant-1",
                "UserCreated",
                Some("user-123"),
                &json!({"name": "Ada Lovelace"}),
            )
            .unwrap();

        let results = engine
            .search_keywords(&KeywordQuery::new("lovelace").with_tenant("tenant-1"))
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].event_id, event_id);
        assert_eq!(results[0].event_type, "UserCreated");
        assert_eq!(results[0].entity_id.as_deref(), Some("user-123"));
        assert!(results[0].score > 0.0);
        assert_eq!(engine.count(Some("tenant-1")), 1);
    }

    #[test]
    fn keyword_search_without_tantivy_never_returns_another_tenants_event() {
        let engine = create_test_engine();
        engine
            .index_event(
                Uuid::new_v4(),
                "tenant-a",
                "Note",
                None,
                &json!({"text": "shared secret"}),
            )
            .unwrap();

        let results = engine
            .search_keywords(&KeywordQuery::new("secret").with_tenant("tenant-b"))
            .unwrap();

        assert!(results.is_empty());
    }

    #[test]
    fn rarer_term_ranks_its_event_first_without_tantivy() {
        let engine = create_test_engine();
        let common = Uuid::new_v4();
        let rare = Uuid::new_v4();
        engine
            .index_event(common, "t", "Note", None, &json!({"text": "order shipped"}))
            .unwrap();
        engine
            .index_event(
                rare,
                "t",
                "Note",
                None,
                &json!({"text": "order refunded refunded"}),
            )
            .unwrap();

        let results = engine
            .search_keywords(&KeywordQuery::new("order refunded").with_tenant("t"))
            .unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].event_id, rare);
    }

    #[test]
    fn deleted_event_is_no_longer_found_without_tantivy() {
        let engine = create_test_engine();
        let event_id = Uuid::new_v4();
        engine
            .index_event(event_id, "t", "Note", None, &json!({"text": "ephemeral"}))
            .unwrap();
        engine.delete_event(event_id).unwrap();

        let results = engine
            .search_keywords(&KeywordQuery::new("ephemeral"))
            .unwrap();

        assert!(results.is_empty());
        assert_eq!(engine.count(Some("t")), 0);
    }
}

/// Integration tests that require the keyword-search feature
#[cfg(test)]
#[cfg(feature = "keyword-search")]
mod integration_tests {
    use super::*;
    use serde_json::json;

    fn create_test_engine() -> KeywordSearchEngine {
        KeywordSearchEngine::with_config(KeywordSearchEngineConfig {
            auto_commit: true,
            ..Default::default()
        })
        .unwrap()
    }

    #[test]
    fn test_index_and_search_basic() {
        let engine = create_test_engine();

        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        engine
            .index_event(
                id1,
                "tenant-1",
                "UserCreated",
                Some("user-123"),
                &json!({"name": "John Doe", "email": "john@example.com"}),
            )
            .unwrap();

        engine
            .index_event(
                id2,
                "tenant-1",
                "OrderCreated",
                Some("order-456"),
                &json!({"product": "Widget", "quantity": 5}),
            )
            .unwrap();

        let query = KeywordQuery::new("John").with_limit(10);
        let results = engine.search_keywords(&query).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].event_id, id1);
        assert_eq!(results[0].event_type, "UserCreated");
    }

    #[test]
    fn test_search_by_event_type() {
        let engine = create_test_engine();

        engine
            .index_event(
                Uuid::new_v4(),
                "tenant-1",
                "UserCreated",
                None,
                &json!({"data": "test"}),
            )
            .unwrap();

        engine
            .index_event(
                Uuid::new_v4(),
                "tenant-1",
                "UserDeleted",
                None,
                &json!({"data": "test"}),
            )
            .unwrap();

        let query = KeywordQuery::new("event_type:UserCreated").with_limit(10);
        let results = engine.search_keywords(&query).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].event_type, "UserCreated");
    }

    #[test]
    fn test_search_with_tenant_filter() {
        let engine = create_test_engine();

        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        engine
            .index_event(
                id1,
                "tenant-1",
                "Event",
                None,
                &json!({"content": "shared data"}),
            )
            .unwrap();

        engine
            .index_event(
                id2,
                "tenant-2",
                "Event",
                None,
                &json!({"content": "shared data"}),
            )
            .unwrap();

        let query = KeywordQuery::new("shared").with_tenant("tenant-1");
        let results = engine.search_keywords(&query).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].event_id, id1);
    }

    #[test]
    fn test_search_complex_query() {
        let engine = create_test_engine();

        engine
            .index_event(
                Uuid::new_v4(),
                "tenant-1",
                "Article",
                None,
                &json!({"title": "Rust Programming", "content": "Systems programming language"}),
            )
            .unwrap();

        engine
            .index_event(
                Uuid::new_v4(),
                "tenant-1",
                "Article",
                None,
                &json!({"title": "Go Programming", "content": "Another systems language"}),
            )
            .unwrap();

        engine
            .index_event(
                Uuid::new_v4(),
                "tenant-1",
                "Article",
                None,
                &json!({"title": "JavaScript", "content": "Web development"}),
            )
            .unwrap();

        // Boolean AND query
        let query = KeywordQuery::new("systems AND programming");
        let results = engine.search_keywords(&query).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_delete_event() {
        let engine = create_test_engine();

        let id = Uuid::new_v4();
        engine
            .index_event(id, "tenant-1", "Event", None, &json!({"data": "test"}))
            .unwrap();

        let query = KeywordQuery::new("test");
        let results = engine.search_keywords(&query).unwrap();
        assert_eq!(results.len(), 1);

        engine.delete_event(id).unwrap();

        let results = engine.search_keywords(&query).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_delete_by_tenant() {
        let engine = create_test_engine();

        for i in 0..3 {
            engine
                .index_event(
                    Uuid::new_v4(),
                    "tenant-1",
                    "Event",
                    None,
                    &json!({"data": format!("data {}", i)}),
                )
                .unwrap();
        }

        for i in 0..2 {
            engine
                .index_event(
                    Uuid::new_v4(),
                    "tenant-2",
                    "Event",
                    None,
                    &json!({"data": format!("data {}", i)}),
                )
                .unwrap();
        }

        assert_eq!(engine.count(Some("tenant-1")), 3);
        assert_eq!(engine.count(Some("tenant-2")), 2);

        let deleted = engine.delete_by_tenant("tenant-1").unwrap();
        assert_eq!(deleted, 3);
        assert_eq!(engine.count(Some("tenant-1")), 0);
        assert_eq!(engine.count(Some("tenant-2")), 2);
    }

    #[test]
    fn test_bm25_ranking() {
        let engine = create_test_engine();

        // Document with "rust" appearing multiple times should rank higher
        engine
            .index_event(
                Uuid::new_v4(),
                "tenant-1",
                "Article",
                None,
                &json!({"content": "Rust is great. Rust is fast. Rust is safe."}),
            )
            .unwrap();

        engine
            .index_event(
                Uuid::new_v4(),
                "tenant-1",
                "Article",
                None,
                &json!({"content": "Rust is a programming language."}),
            )
            .unwrap();

        let query = KeywordQuery::new("rust");
        let results = engine.search_keywords(&query).unwrap();

        assert_eq!(results.len(), 2);
        // First result should have higher score (more term frequency)
        assert!(results[0].score >= results[1].score);
    }

    #[test]
    fn test_stats() {
        let engine = create_test_engine();

        for i in 0..5 {
            engine
                .index_event(
                    Uuid::new_v4(),
                    "tenant-1",
                    "Event",
                    None,
                    &json!({"data": format!("test {}", i)}),
                )
                .unwrap();
        }

        let query = KeywordQuery::new("test");
        engine.search_keywords(&query).unwrap();
        engine.search_keywords(&query).unwrap();

        let (indexed, searches, _) = engine.stats();
        assert_eq!(indexed, 5);
        assert_eq!(searches, 2);
    }

    #[test]
    fn test_health_check_with_feature() {
        let engine = create_test_engine();
        assert!(engine.health_check().is_ok());
    }

    #[test]
    fn test_search_with_specific_fields() {
        let engine = create_test_engine();

        engine
            .index_event(
                Uuid::new_v4(),
                "tenant-1",
                "TestEvent",
                Some("entity-test"),
                &json!({"content": "some content"}),
            )
            .unwrap();

        // Search only in event_type field
        let query = KeywordQuery::new("TestEvent").with_fields(vec!["event_type".to_string()]);
        let results = engine.search_keywords(&query).unwrap();
        assert_eq!(results.len(), 1);

        // Search in payload field for something that's only in event_type - should not match
        let query = KeywordQuery::new("TestEvent").with_fields(vec!["payload".to_string()]);
        let results = engine.search_keywords(&query).unwrap();
        assert_eq!(results.len(), 0);
    }
}
