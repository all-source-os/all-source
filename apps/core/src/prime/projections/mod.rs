//! Projection modules for AllSource Prime.
//!
//! Each projection maintains an indexed view over the Prime event stream,
//! rebuilt from WAL on startup (with optional snapshot acceleration).

pub mod adjacency;
pub(crate) mod charge;
pub mod contradiction;
pub mod cross_domain;
pub mod declarative;
pub mod domain_index;
pub mod node_state;
pub mod node_type_index;
pub mod relevance;
pub mod stats;

pub use adjacency::{
    AdjEntry, AdjacencyDirection, AdjacencyListProjection, DirectedAdjacencyProjection,
    ReverseIndexProjection,
};
pub use contradiction::{Contradiction, ContradictionDetectionProjection};
pub use cross_domain::{CrossDomainLink, CrossDomainProjection};
pub use declarative::{
    EntitySnapshot, MergePolicy, Observation, ProjectionDef, Provenance, ProvenanceSnapshot, fold,
    fold_with_provenance, provenance_for_field,
};
pub use domain_index::DomainIndexProjection;
pub use node_state::NodeStateProjection;
pub use node_type_index::NodeTypeIndexProjection;
pub use relevance::{RelevanceDecayProjection, RelevanceScore};
pub use stats::GraphStatsProjection;
