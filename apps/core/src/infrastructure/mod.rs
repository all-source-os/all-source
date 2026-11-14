// Infrastructure layer - concrete implementations
// This contains:
// - repositories/ (In-memory, PostgreSQL, RocksDB implementations with SierraDB patterns)
// - persistence/ (Storage integrity, checksums, lock-free structures)
// - cluster/ (Node registry, request routing for distributed systems)
// - web/ (HTTP handlers, WebSocket handlers)
// - security/ (Authentication, authorization, rate limiting)
// - config/ (Configuration loading)
// - observability/ (Metrics, tracing)
// - search/ (Vector search engine with HNSW indexing)
// - di/ (Dependency injection container)

pub mod cluster;
pub mod config;
pub mod di;
pub mod hash_state;
pub mod memory;
pub mod observability;
pub mod persistence;
pub mod query;
#[cfg(feature = "replication")]
pub mod replication;
#[cfg(not(feature = "replication"))]
pub mod replication {
    //! Stub types for community builds (replication disabled at compile time).
    /// Stub — replication not available in community edition.
    pub struct WalShipper;
    /// Stub — replication not available in community edition.
    pub struct WalReceiver;
    /// Stub — replication not available in community edition.
    #[derive(Debug, Clone, Copy, serde::Serialize)]
    pub enum ReplicationMode {
        Async,
    }
    /// Stub — replication not available in community edition.
    #[derive(Debug, Clone, serde::Serialize)]
    pub struct ReplicationStatus {
        pub enabled: bool,
    }
    /// Stub — replication not available in community edition.
    #[derive(Debug, Clone, serde::Serialize)]
    pub struct FollowerReplicationStatus {
        pub connected: bool,
    }
}
pub mod repositories;
#[cfg(feature = "server")]
pub mod resp;
pub mod search;
pub mod security;
#[cfg(feature = "server")]
pub mod web;
