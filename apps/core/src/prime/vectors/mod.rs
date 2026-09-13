//! Vector storage and search for AllSource Prime.
//!
//! Gated behind the `prime-vectors` feature flag. Provides embedding storage
//! as events and HNSW-indexed similarity search via projections.
//!
//! This module is only compiled when `prime-vectors` is enabled, so that
//! graph-only users don't pay the fastembed/instant-distance compilation cost.

pub mod embedder;
pub mod index;
pub mod node_text;
pub mod types;

pub use embedder::{DEFAULT_EMBEDDING_DIMENSIONS, TextEmbedder};
pub use index::{SearchHit, VectorIndexConfig, VectorIndexProjection};
pub use node_text::{NODE_TEXT_VERSION, NODE_TEXT_VERSION_KEY, node_text};
pub use types::{VectorEntry, VectorSearchResult, event_types, vector_entity_id};
