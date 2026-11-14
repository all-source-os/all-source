//! Relevance decay projection — scores entities by recency and access frequency.
//!
//! Each entity's relevance decays exponentially over time. Accessing an entity
//! (via recall, neighbors, or explicit touch) boosts its score. The decay formula:
//!
//! ```text
//! score = base_score * e^(-decay_rate * hours_since_access)
//! ```
//!
//! Scores are computed lazily at read time — the projection stores raw data
//! (last_accessed, access_count) and applies decay on demand.

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

use super::charge;
use crate::{
    application::services::projection::Projection, domain::entities::Event, error::Result,
    infrastructure::memory::ledger::Tally, prime::types::event_types,
};

/// Event type for explicit memory access tracking.
pub const MEMORY_ACCESSED: &str = "prime.memory.accessed";

/// Raw relevance data for an entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelevanceScore {
    /// Base score (incremented on creation, update, access).
    pub base_score: f64,
    /// When the entity was last accessed or modified.
    pub last_accessed: DateTime<Utc>,
    /// Total number of accesses.
    pub access_count: u64,
    /// Decay rate per hour (default 0.01).
    pub decay_rate: f64,
}

impl RelevanceScore {
    fn new(timestamp: DateTime<Utc>, decay_rate: f64) -> Self {
        Self {
            base_score: 1.0,
            last_accessed: timestamp,
            access_count: 1,
            decay_rate,
        }
    }

    /// Compute the current relevance score with exponential decay.
    pub fn current_score(&self, now: DateTime<Utc>) -> f64 {
        let hours = (now - self.last_accessed).num_milliseconds().max(0) as f64 / 3_600_000.0;
        self.base_score * (-self.decay_rate * hours).exp()
    }

    fn touch(&mut self, timestamp: DateTime<Utc>) {
        self.base_score += 1.0;
        self.access_count += 1;
        self.last_accessed = timestamp;
    }
}

/// Projection that tracks per-entity relevance scores with exponential decay.
pub struct RelevanceDecayProjection {
    name: String,
    /// Default decay rate for new entities.
    default_decay_rate: f64,
    /// entity_id -> RelevanceScore
    scores: Arc<DashMap<String, RelevanceScore>>,
    charge: Tally,
}

impl RelevanceDecayProjection {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            default_decay_rate: 0.01,
            scores: Arc::new(DashMap::new()),
            charge: charge::tally(),
        }
    }

    /// Every score is a fixed-size value, so an entry costs its key and slot.
    fn store(&self, entity_id: String, score: RelevanceScore) {
        let key_bytes = charge::text(&entity_id);
        if self.scores.insert(entity_id, score).is_none() {
            self.charge
                .add(charge::entry::<String, RelevanceScore>(key_bytes));
        }
    }

    /// Set the default decay rate per hour for new entities.
    pub fn with_decay_rate(mut self, rate: f64) -> Self {
        self.default_decay_rate = rate;
        self
    }

    /// Get the current relevance score for an entity (with decay applied).
    pub fn relevance(&self, entity_id: &str) -> f64 {
        self.relevance_at(entity_id, Utc::now())
    }

    /// Get the relevance score at a specific time (for testing).
    pub fn relevance_at(&self, entity_id: &str, now: DateTime<Utc>) -> f64 {
        self.scores
            .get(entity_id)
            .map_or(0.0, |s| s.current_score(now))
    }

    /// Get the raw (undecayed) score data.
    pub fn raw_score(&self, entity_id: &str) -> Option<RelevanceScore> {
        self.scores.get(entity_id).map(|s| s.value().clone())
    }
}

impl Projection for RelevanceDecayProjection {
    fn name(&self) -> &str {
        &self.name
    }

    fn process(&self, event: &Event) -> Result<()> {
        let event_type = event.event_type_str();

        // Only process prime.* events
        if !event_type.starts_with("prime.") {
            return Ok(());
        }

        let entity_id = event.entity_id_str().to_string();
        let timestamp = event.timestamp;

        match event_type {
            event_types::NODE_CREATED | event_types::EDGE_CREATED => {
                self.store(
                    entity_id,
                    RelevanceScore::new(timestamp, self.default_decay_rate),
                );
            }
            event_types::NODE_UPDATED | MEMORY_ACCESSED => {
                if let Some(mut entry) = self.scores.get_mut(&entity_id) {
                    entry.touch(timestamp);
                }
            }
            event_types::NODE_DELETED | event_types::EDGE_DELETED => {
                // Keep the score entry but don't boost — deleted entities decay to zero naturally
            }
            _ => {}
        }

        Ok(())
    }

    fn get_state(&self, entity_id: &str) -> Option<Value> {
        self.scores
            .get(entity_id)
            .and_then(|s| serde_json::to_value(s.value()).ok())
    }

    fn clear(&self) {
        self.scores.clear();
        self.charge.take();
    }

    fn snapshot(&self) -> Option<Value> {
        let entries: Vec<(String, RelevanceScore)> = self
            .scores
            .iter()
            .map(|e| (e.key().clone(), e.value().clone()))
            .collect();
        serde_json::to_value(entries).ok()
    }

    fn restore(&self, snapshot: &Value) -> Result<()> {
        let entries: Vec<(String, RelevanceScore)> = serde_json::from_value(snapshot.clone())
            .map_err(|e| crate::error::AllSourceError::StorageError(e.to_string()))?;
        self.clear();
        for (k, v) in entries {
            self.store(k, v);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use uuid::Uuid;

    fn make_event(entity_id: &str, event_type: &str, payload: Value) -> Event {
        Event::reconstruct_from_strings(
            Uuid::new_v4(),
            event_type.to_string(),
            entity_id.to_string(),
            "default".to_string(),
            payload,
            Utc::now(),
            None,
            1,
        )
    }

    fn make_event_at(
        entity_id: &str,
        event_type: &str,
        payload: Value,
        timestamp: DateTime<Utc>,
    ) -> Event {
        Event::reconstruct_from_strings(
            Uuid::new_v4(),
            event_type.to_string(),
            entity_id.to_string(),
            "default".to_string(),
            payload,
            timestamp,
            None,
            1,
        )
    }

    #[test]
    fn test_node_created_sets_initial_score() {
        let proj = RelevanceDecayProjection::new("relevance");

        proj.process(&make_event(
            "node:person:alice",
            event_types::NODE_CREATED,
            serde_json::json!({"node_type": "person"}),
        ))
        .unwrap();

        let score = proj.relevance("node:person:alice");
        assert!(score > 0.9); // ~1.0, minimal decay since just created
    }

    #[test]
    fn test_score_decays_over_time() {
        let proj = RelevanceDecayProjection::new("relevance").with_decay_rate(0.1);

        let t0 = Utc::now() - Duration::hours(10);

        proj.process(&make_event_at(
            "node:person:alice",
            event_types::NODE_CREATED,
            serde_json::json!({"node_type": "person"}),
            t0,
        ))
        .unwrap();

        // Score now should be decayed: 1.0 * e^(-0.1 * 10) = e^(-1) ≈ 0.368
        let score = proj.relevance("node:person:alice");
        assert!(score < 0.5, "score should have decayed, got {score}");
        assert!(score > 0.3, "score too low, got {score}");
    }

    #[test]
    fn test_access_boosts_score() {
        let proj = RelevanceDecayProjection::new("relevance").with_decay_rate(0.1);

        let t0 = Utc::now() - Duration::hours(10);

        proj.process(&make_event_at(
            "node:person:alice",
            event_types::NODE_CREATED,
            serde_json::json!({"node_type": "person"}),
            t0,
        ))
        .unwrap();

        let score_before = proj.relevance("node:person:alice");

        // Touch (access) the entity now
        proj.process(&make_event(
            "node:person:alice",
            MEMORY_ACCESSED,
            serde_json::json!({}),
        ))
        .unwrap();

        let score_after = proj.relevance("node:person:alice");
        assert!(
            score_after > score_before,
            "access should boost score: before={score_before}, after={score_after}"
        );
    }

    #[test]
    fn test_update_boosts_score() {
        let proj = RelevanceDecayProjection::new("relevance");

        proj.process(&make_event(
            "node:person:alice",
            event_types::NODE_CREATED,
            serde_json::json!({"node_type": "person"}),
        ))
        .unwrap();

        let raw_before = proj.raw_score("node:person:alice").unwrap();
        assert_eq!(raw_before.access_count, 1);

        proj.process(&make_event(
            "node:person:alice",
            event_types::NODE_UPDATED,
            serde_json::json!({"properties": {"role": "manager"}}),
        ))
        .unwrap();

        let raw_after = proj.raw_score("node:person:alice").unwrap();
        assert_eq!(raw_after.access_count, 2);
        assert!(raw_after.base_score > raw_before.base_score);
    }

    #[test]
    fn test_nonexistent_entity_returns_zero() {
        let proj = RelevanceDecayProjection::new("relevance");
        assert_eq!(proj.relevance("ghost"), 0.0);
    }

    #[test]
    fn test_ignores_non_prime_events() {
        let proj = RelevanceDecayProjection::new("relevance");

        proj.process(&make_event(
            "user-123",
            "user.created",
            serde_json::json!({"name": "Alice"}),
        ))
        .unwrap();

        assert_eq!(proj.relevance("user-123"), 0.0);
    }

    #[test]
    fn test_snapshot_and_restore() {
        let proj = RelevanceDecayProjection::new("relevance");

        proj.process(&make_event(
            "node:person:alice",
            event_types::NODE_CREATED,
            serde_json::json!({"node_type": "person"}),
        ))
        .unwrap();

        let snap = proj.snapshot().expect("snapshot should be Some");

        proj.clear();
        assert_eq!(proj.relevance("node:person:alice"), 0.0);

        proj.restore(&snap).unwrap();
        assert!(proj.relevance("node:person:alice") > 0.0);
    }

    #[test]
    fn test_custom_decay_rate() {
        let fast_decay = RelevanceDecayProjection::new("fast").with_decay_rate(1.0);
        let slow_decay = RelevanceDecayProjection::new("slow").with_decay_rate(0.001);

        let t0 = Utc::now() - Duration::hours(5);

        fast_decay
            .process(&make_event_at(
                "node:a",
                event_types::NODE_CREATED,
                serde_json::json!({}),
                t0,
            ))
            .unwrap();

        slow_decay
            .process(&make_event_at(
                "node:a",
                event_types::NODE_CREATED,
                serde_json::json!({}),
                t0,
            ))
            .unwrap();

        let fast_score = fast_decay.relevance("node:a");
        let slow_score = slow_decay.relevance("node:a");

        assert!(
            fast_score < slow_score,
            "fast decay should produce lower score: fast={fast_score}, slow={slow_score}"
        );
    }

    #[test]
    fn test_edge_created_gets_score() {
        let proj = RelevanceDecayProjection::new("relevance");

        proj.process(&make_event(
            "edge:e-1",
            event_types::EDGE_CREATED,
            serde_json::json!({"relation": "knows"}),
        ))
        .unwrap();

        assert!(proj.relevance("edge:e-1") > 0.0);
    }
}
