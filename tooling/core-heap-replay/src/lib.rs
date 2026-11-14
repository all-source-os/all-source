//! Evaluators for the allsource-core heap replay.
//!
//! The replay itself is the `heap_replay` example in `apps/core`. This crate
//! builds and runs it, checks every gate, and emits one JSONL v1 envelope.

pub mod envelope;
pub mod gates;
pub mod listing;
pub mod policy;
pub mod protocol;
pub mod replay;
pub mod runner;
pub mod scan;
pub mod shape;
pub mod steps;

/// Lowercase hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}
