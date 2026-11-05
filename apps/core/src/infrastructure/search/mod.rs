//! Search Infrastructure
//!
//! This module provides search engines for event queries:
//! - VectorSearchEngine: Semantic similarity search using embeddings and HNSW
//! - KeywordSearchEngine: Full-text BM25 keyword search using Tantivy
//! - HybridSearchEngine: Combined semantic and keyword search orchestrator

mod event_search;
mod hybrid_search_engine;
#[cfg(not(feature = "keyword-search"))]
mod keyword_fallback;
mod keyword_search_engine;
mod vector_search_engine;

pub use event_search::*;
pub use hybrid_search_engine::*;
pub use keyword_search_engine::*;
pub use vector_search_engine::*;
