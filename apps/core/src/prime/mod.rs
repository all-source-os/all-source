//! # AllSource Prime — Unified Agent Memory Engine
//!
//! Prime provides vectors, graph relationships, and temporal history in a single
//! embedded engine. Everything is stored as immutable events in the WAL, with
//! projections maintaining indexed views for fast queries.
//!
//! ## Quick Start
//!
//! ```rust,ignore
//! use allsource_core::prime::{Prime, Direction};
//!
//! let prime = Prime::open("~/.agent/memory").await?;
//!
//! // Graph
//! let alice = prime.add_node("person", json!({"name": "Alice"})).await?;
//! let project = prime.add_node("project", json!({"name": "Prime"})).await?;
//! prime.add_edge(&alice, &project, "works_on", None).await?;
//!
//! // Traverse
//! let team = prime.neighbors(&project, None, Direction::Incoming).await?;
//! ```

pub mod error;
pub mod event_store;
pub mod facade;
#[cfg(feature = "prime-recall")]
pub mod hosted;
#[cfg(feature = "prime-recall")]
pub mod http_core;
pub mod import_export;
pub mod lexical;
#[cfg(feature = "prime-recall")]
pub mod projection_bundle;
pub mod projections;
#[cfg(feature = "prime-recall")]
pub mod recall;
// In the lib, not tests/: the only job that runs core tests is `--lib`.
#[cfg(all(test, feature = "prime-vectors"))]
mod recall_degradation_tests;
pub mod schema;
pub mod sync;
#[cfg(feature = "prime-recall")]
pub mod tenant_cache;
pub mod types;
#[cfg(feature = "prime-vectors")]
pub mod vectors;

// Re-export commonly used types
pub use error::{PrimeError, PrimeResult};
pub use event_store::EventStore;
pub use facade::{ConversationScope, Prime};
#[cfg(feature = "prime-recall")]
pub use http_core::HttpCore;
pub use types::{
    Direction, Edge, EdgeId, EntityId, GraphDiff, HistoryEntry, Node, NodeId, PrimeStats, SubGraph,
    edge_entity_id, event_types, node_entity_id,
};
