//! # AllSource Core - High-Performance Event Store
//!
//! A high-performance event sourcing platform built in Rust, following Clean Architecture principles.
//!
//! ## Architecture Overview
//!
//! The codebase follows a layered Clean Architecture:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    Infrastructure Layer                     │
//! │  (HTTP handlers, WebSocket, persistence, security)          │
//! │  infrastructure::web, infrastructure::persistence,          │
//! │  infrastructure::security, infrastructure::repositories     │
//! ├─────────────────────────────────────────────────────────────┤
//! │                    Application Layer                        │
//! │  (Use cases, services, DTOs)                                │
//! │  application::use_cases, application::services,             │
//! │  application::dto                                           │
//! ├─────────────────────────────────────────────────────────────┤
//! │                      Domain Layer                           │
//! │  (Entities, value objects, repository traits)               │
//! │  domain::entities, domain::value_objects,                   │
//! │  domain::repositories                                       │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Module Organization
//!
//! - **domain**: Core business entities, value objects, and repository traits
//! - **application**: Use cases, services, and DTOs that orchestrate domain logic
//! - **infrastructure**: Concrete implementations (web, persistence, security)
//!
//! ## Quick Start
//!
//! ```rust,ignore
//! use allsource_core::{EventStore, Event, IngestEventRequest};
//!
//! let store = EventStore::new();
//! let event = Event::from_strings(
//!     "user.created".to_string(),
//!     "user-123".to_string(),
//!     "default".to_string(),
//!     serde_json::json!({"name": "Alice"}),
//!     None,
//! )?;
//! store.ingest(event)?;
//! ```

// Safety: no unsafe code allowed in this crate
#![forbid(unsafe_code)]
// Development suppressions — tighten progressively
#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(deprecated)]
// Clippy — allow patterns justified by architecture
#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]
#![allow(clippy::module_inception)]

// =============================================================================
// Clean Architecture Layers
// =============================================================================

/// Layer 1: Domain Layer - Enterprise Business Rules
///
/// Contains pure business entities, value objects, and repository traits.
/// This layer has ZERO external dependencies.
pub mod domain;

/// Layer 2: Application Layer - Application Business Rules
///
/// Contains use cases that orchestrate domain entities and services.
/// Depends only on the domain layer.
pub mod application;

/// Layer 3: Infrastructure Layer - Interface Adapters
///
/// Contains concrete implementations of abstractions.
/// Depends on domain and application layers.
pub mod infrastructure;

// =============================================================================
// Shared Modules
// =============================================================================

/// Error types for the entire crate
pub mod error;

/// Main EventStore facade
pub mod store;

/// Advanced security module (anomaly detection, encryption, KMS)
pub mod security;

/// Shared test fixture builders for integration tests and cross-crate use
pub mod test_utils;

/// Test-only probe counting `Event` clones — lets unit tests assert on how much
/// a call materializes, not just on what it returns. See issue #251.
#[cfg(test)]
mod clone_probe;

/// Async webhook delivery worker
#[cfg(feature = "server")]
pub mod webhook_worker;

/// AllSource Prime — unified agent memory engine (requires `prime` feature)
#[cfg(feature = "prime")]
pub mod prime;

/// Ergonomic embedded-mode facade (requires `embedded` feature)
#[cfg(feature = "embedded")]
pub mod embedded;

#[cfg(feature = "embedded")]
pub use embedded::EmbeddedCore;

// =============================================================================
// Public API - Commonly Used Types
// =============================================================================

// Domain layer exports
pub use domain::{entities, entities::Event, repositories};

// Application layer exports
pub use application::{
    dto::{IngestEventRequest, QueryEventsRequest},
    services::{
        AnalyticsEngine, ExactlyOnceConfig, ExactlyOnceRegistry, Pipeline, PipelineConfig,
        PipelineManager, ProjectionManager, ReplayManager, SchemaEvolutionManager, SchemaRegistry,
    },
};

// Infrastructure layer exports
#[cfg(feature = "server")]
pub use infrastructure::security::{AuthManager, Permission, Role};
#[cfg(feature = "server")]
pub use infrastructure::web::{WebSocketManager, serve};
pub use infrastructure::{
    persistence::{
        CompactionConfig, CompactionManager, EventIndex, ParquetStorage, WALConfig, WriteAheadLog,
    },
    security::RateLimiter,
};

// Error handling
pub use error::{AllSourceError, Result};

// =============================================================================
// Backward-Compatible Aliases (for binaries and external users)
// =============================================================================

/// Auth module re-export for backward compatibility
#[cfg(feature = "server")]
pub mod auth {
    pub use crate::infrastructure::security::{AuthManager, Permission, Role};
}

/// Rate limiting module re-export
pub mod rate_limit {
    pub use crate::infrastructure::security::rate_limit::{RateLimitConfig, RateLimiter};
}

/// Tenant module re-export
pub mod tenant {
    pub use crate::domain::entities::{Tenant, TenantQuotas};
}

/// Config module re-export
pub mod config {
    pub use crate::infrastructure::config::*;
}

/// Backup module re-export
pub mod backup {
    pub use crate::infrastructure::persistence::backup::*;
}

/// API v1 module re-export
#[cfg(feature = "server")]
pub mod api_v1 {
    pub use crate::infrastructure::web::api_v1::{AppState, AtomicNodeRole, NodeRole, serve_v1};
}

/// Replication module re-export (enterprise feature)
#[cfg(feature = "replication")]
pub mod replication {
    pub use crate::infrastructure::replication::{
        FollowerReplicationStatus, ReplicationMode, ReplicationStatus, WalReceiver, WalShipper,
    };
}
#[cfg(not(feature = "replication"))]
pub mod replication {
    pub use crate::infrastructure::replication::{
        FollowerReplicationStatus, ReplicationMode, ReplicationStatus, WalReceiver, WalShipper,
    };
}

/// Cluster module re-export
pub mod cluster {
    pub use crate::infrastructure::cluster::{
        ClusterManager, ClusterMember, ClusterStatus, ConflictResolution, CrdtResolver,
        GeoReplicationConfig, GeoReplicationManager, GeoReplicationStatus, GeoSyncRequest,
        GeoSyncResponse, HlcTimestamp, HybridLogicalClock, MemberRole, MergeStrategy, Node,
        NodeRegistry, PeerHealth, PeerRegion, PeerStatus, ReplicatedEvent, RequestRouter,
        VersionVector, VoteRequest, VoteResponse,
    };
}

/// RESP3 (Redis wire protocol) server re-export
#[cfg(feature = "server")]
pub mod resp {
    pub use crate::infrastructure::resp::RespServer;
}

/// Advanced query features re-export (v2.0)
pub mod query {
    #[cfg(feature = "analytics")]
    pub use crate::infrastructure::query::eventql::{
        EventQLRequest, EventQLResponse, execute_eventql,
    };
    pub use crate::infrastructure::query::{
        geospatial::{
            BoundingBox, Coordinate, GeoEventResult, GeoIndex, GeoQueryRequest, RadiusQuery,
            execute_geo_query, haversine_distance,
        },
        graphql::{
            GraphQLError, GraphQLRequest, GraphQLResponse, QueryField, event_to_json,
            introspection_schema, parse_query,
        },
    };
}

// Main store facade
pub use store::EventStore;

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod security_integration_tests;
