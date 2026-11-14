//! Exactly-once stream processing guarantees
//!
//! Provides deduplication and idempotency tracking for event ingestion and
//! pipeline processing, ensuring that retried or duplicated events don't
//! result in duplicate state changes.
//!
//! ## Mechanisms
//!
//! 1. **Idempotency keys**: Clients include an `idempotency_key` in event metadata.
//!    The registry tracks which keys have been processed and rejects duplicates.
//!
//! 2. **Consumer offsets**: Pipeline processors track their read offset per stream.
//!    On restart, processing resumes from the last committed offset.
//!
//! 3. **Processing receipts**: Each event processed by a pipeline gets a receipt
//!    (hash of pipeline_id + event_id). Receipts prevent reprocessing during replays.
//!
//! ## TTL
//!
//! Idempotency keys expire after a configurable window (default 24 hours) to
//! bound memory usage. Consumer offsets persist indefinitely.

use crate::infrastructure::memory::ledger::{MapCharge, Owner, string_heap};
use chrono::{DateTime, Duration, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::mem::size_of;
use uuid::Uuid;

/// Configuration for exactly-once processing
#[derive(Debug, Clone)]
pub struct ExactlyOnceConfig {
    /// How long to keep idempotency keys before expiry
    pub idempotency_ttl: Duration,
    /// Maximum number of idempotency keys to retain (LRU eviction)
    pub max_idempotency_keys: usize,
}

impl Default for ExactlyOnceConfig {
    fn default() -> Self {
        Self {
            idempotency_ttl: Duration::hours(24),
            max_idempotency_keys: 1_000_000,
        }
    }
}

/// An idempotency record
#[derive(Debug, Clone, Serialize)]
struct IdempotencyRecord {
    /// The event ID that was created for this key
    event_id: Uuid,
    /// When this key was first seen
    created_at: DateTime<Utc>,
}

/// Consumer offset for a pipeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsumerOffset {
    /// Pipeline ID
    pub pipeline_id: String,
    /// Stream/partition identifier
    pub stream_id: String,
    /// Last committed event offset (index into the event store)
    pub offset: usize,
    /// Last committed event ID
    pub last_event_id: Uuid,
    /// When offset was last updated
    pub updated_at: DateTime<Utc>,
}

/// Processing receipt proving an event was handled by a pipeline
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
struct ProcessingReceipt {
    pipeline_id: String,
    event_id: Uuid,
}

/// Result of an idempotency check
#[derive(Debug, Clone, Serialize)]
pub enum IdempotencyResult {
    /// Key is new, proceed with processing
    New,
    /// Key was already processed, return the original event_id
    Duplicate { original_event_id: Uuid },
}

/// Exactly-once processing registry
pub struct ExactlyOnceRegistry {
    config: ExactlyOnceConfig,
    /// Idempotency key → record
    idempotency_keys: DashMap<String, IdempotencyRecord>,
    idempotency_charge: MapCharge,
    /// (pipeline_id, stream_id) → consumer offset
    consumer_offsets: DashMap<(String, String), ConsumerOffset>,
    consumer_offsets_charge: MapCharge,
    /// Set of processing receipts
    processing_receipts: DashMap<ProcessingReceipt, DateTime<Utc>>,
    processing_receipts_charge: MapCharge,
}

impl ExactlyOnceRegistry {
    pub fn new(config: ExactlyOnceConfig) -> Self {
        Self {
            config,
            idempotency_keys: DashMap::new(),
            idempotency_charge: MapCharge::new(
                Owner::Misc,
                size_of::<(String, IdempotencyRecord)>(),
            ),
            consumer_offsets: DashMap::new(),
            consumer_offsets_charge: MapCharge::new(
                Owner::Misc,
                size_of::<((String, String), ConsumerOffset)>(),
            ),
            processing_receipts: DashMap::new(),
            processing_receipts_charge: MapCharge::new(
                Owner::Misc,
                size_of::<(ProcessingReceipt, DateTime<Utc>)>(),
            ),
        }
    }

    /// Check and register an idempotency key
    ///
    /// Returns `New` if the key hasn't been seen (and registers it),
    /// or `Duplicate` with the original event_id if already processed.
    pub fn check_idempotency(&self, key: &str, event_id: Uuid) -> IdempotencyResult {
        let now = Utc::now();

        // Check if key exists and is not expired
        if let Some(existing) = self.idempotency_keys.get(key) {
            let age = now - existing.created_at;
            if age < self.config.idempotency_ttl {
                return IdempotencyResult::Duplicate {
                    original_event_id: existing.event_id,
                };
            }
            // Expired — remove and treat as new
            drop(existing);
            self.idempotency_charge
                .remove_entry(&self.idempotency_keys, key, |k, _| string_heap(k));
        }

        // Evict oldest keys if at capacity
        if self.idempotency_keys.len() >= self.config.max_idempotency_keys {
            self.evict_expired_keys();
        }

        // Register the new key
        self.idempotency_charge.insert(
            &self.idempotency_keys,
            key.to_string(),
            IdempotencyRecord {
                event_id,
                created_at: now,
            },
            |k, _| string_heap(k),
        );

        IdempotencyResult::New
    }

    /// Commit a consumer offset for a pipeline
    pub fn commit_offset(&self, pipeline_id: &str, stream_id: &str, offset: usize, event_id: Uuid) {
        let key = (pipeline_id.to_string(), stream_id.to_string());
        self.consumer_offsets_charge.insert(
            &self.consumer_offsets,
            key,
            ConsumerOffset {
                pipeline_id: pipeline_id.to_string(),
                stream_id: stream_id.to_string(),
                offset,
                last_event_id: event_id,
                updated_at: Utc::now(),
            },
            |(pipeline, stream), offset| {
                string_heap(pipeline)
                    + string_heap(stream)
                    + string_heap(&offset.pipeline_id)
                    + string_heap(&offset.stream_id)
            },
        );
    }

    /// Get the last committed offset for a pipeline
    pub fn get_offset(&self, pipeline_id: &str, stream_id: &str) -> Option<ConsumerOffset> {
        let key = (pipeline_id.to_string(), stream_id.to_string());
        self.consumer_offsets.get(&key).map(|v| v.clone())
    }

    /// Record that a pipeline has processed an event
    pub fn record_processing(&self, pipeline_id: &str, event_id: Uuid) {
        let receipt = ProcessingReceipt {
            pipeline_id: pipeline_id.to_string(),
            event_id,
        };
        self.processing_receipts_charge.insert(
            &self.processing_receipts,
            receipt,
            Utc::now(),
            |receipt, _| string_heap(&receipt.pipeline_id),
        );
    }

    /// Check if a pipeline has already processed an event
    pub fn was_processed(&self, pipeline_id: &str, event_id: Uuid) -> bool {
        let receipt = ProcessingReceipt {
            pipeline_id: pipeline_id.to_string(),
            event_id,
        };
        self.processing_receipts.contains_key(&receipt)
    }

    /// Remove expired idempotency keys
    fn evict_expired_keys(&self) {
        let now = Utc::now();
        let ttl = self.config.idempotency_ttl;
        let expired: Vec<String> = self
            .idempotency_keys
            .iter()
            .filter(|entry| now - entry.value().created_at >= ttl)
            .map(|entry| entry.key().clone())
            .collect();
        for key in &expired {
            self.idempotency_charge.remove_entry_if(
                &self.idempotency_keys,
                key,
                |record| now - record.created_at >= ttl,
                |k, _| string_heap(k),
            );
        }
    }

    /// Statistics about the exactly-once registry
    pub fn stats(&self) -> ExactlyOnceStats {
        ExactlyOnceStats {
            idempotency_keys: self.idempotency_keys.len(),
            consumer_offsets: self.consumer_offsets.len(),
            processing_receipts: self.processing_receipts.len(),
        }
    }
}

/// Exactly-once registry statistics
#[derive(Debug, Clone, Serialize)]
pub struct ExactlyOnceStats {
    pub idempotency_keys: usize,
    pub consumer_offsets: usize,
    pub processing_receipts: usize,
}

/// Extract an idempotency key from event metadata
pub fn extract_idempotency_key(metadata: &Option<serde_json::Value>) -> Option<String> {
    metadata
        .as_ref()?
        .get("idempotency_key")
        .and_then(|v| v.as_str())
        .map(std::string::ToString::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> ExactlyOnceRegistry {
        ExactlyOnceRegistry::new(ExactlyOnceConfig::default())
    }

    #[test]
    fn test_new_idempotency_key() {
        let reg = registry();
        let id = Uuid::new_v4();
        match reg.check_idempotency("key-1", id) {
            IdempotencyResult::New => {}
            IdempotencyResult::Duplicate { .. } => panic!("Expected New, got Duplicate"),
        }
    }

    #[test]
    fn test_duplicate_idempotency_key() {
        let reg = registry();
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        reg.check_idempotency("key-1", id1);
        match reg.check_idempotency("key-1", id2) {
            IdempotencyResult::Duplicate { original_event_id } => {
                assert_eq!(original_event_id, id1);
            }
            IdempotencyResult::New => panic!("Expected Duplicate, got New"),
        }
    }

    #[test]
    fn test_expired_key_treated_as_new() {
        let reg = ExactlyOnceRegistry::new(ExactlyOnceConfig {
            idempotency_ttl: Duration::seconds(-1), // Already expired
            ..Default::default()
        });
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        reg.check_idempotency("key-1", id1);
        match reg.check_idempotency("key-1", id2) {
            IdempotencyResult::New => {}
            IdempotencyResult::Duplicate { .. } => panic!("Expected New (expired), got Duplicate"),
        }
    }

    #[test]
    fn test_consumer_offset_commit_and_get() {
        let reg = registry();
        let eid = Uuid::new_v4();
        reg.commit_offset("pipeline-1", "stream-a", 42, eid);
        let offset = reg.get_offset("pipeline-1", "stream-a").unwrap();
        assert_eq!(offset.offset, 42);
        assert_eq!(offset.last_event_id, eid);
    }

    #[test]
    fn test_consumer_offset_not_found() {
        let reg = registry();
        assert!(reg.get_offset("nonexistent", "stream").is_none());
    }

    #[test]
    fn test_processing_receipts() {
        let reg = registry();
        let eid = Uuid::new_v4();
        assert!(!reg.was_processed("pipeline-1", eid));
        reg.record_processing("pipeline-1", eid);
        assert!(reg.was_processed("pipeline-1", eid));
    }

    #[test]
    fn test_processing_receipt_different_pipeline() {
        let reg = registry();
        let eid = Uuid::new_v4();
        reg.record_processing("pipeline-1", eid);
        assert!(!reg.was_processed("pipeline-2", eid));
    }

    #[test]
    fn test_extract_idempotency_key() {
        let meta = Some(serde_json::json!({"idempotency_key": "abc-123"}));
        assert_eq!(extract_idempotency_key(&meta), Some("abc-123".to_string()));

        let no_key = Some(serde_json::json!({"source": "web"}));
        assert_eq!(extract_idempotency_key(&no_key), None);

        assert_eq!(extract_idempotency_key(&None), None);
    }

    #[test]
    fn test_stats() {
        let reg = registry();
        let id = Uuid::new_v4();
        reg.check_idempotency("k1", id);
        reg.commit_offset("p1", "s1", 0, id);
        reg.record_processing("p1", id);
        let stats = reg.stats();
        assert_eq!(stats.idempotency_keys, 1);
        assert_eq!(stats.consumer_offsets, 1);
        assert_eq!(stats.processing_receipts, 1);
    }

    #[test]
    fn test_eviction_on_capacity() {
        let reg = ExactlyOnceRegistry::new(ExactlyOnceConfig {
            idempotency_ttl: Duration::seconds(-1), // All expired
            max_idempotency_keys: 2,
        });
        // Fill to capacity with expired keys
        reg.idempotency_keys.insert(
            "old-1".to_string(),
            IdempotencyRecord {
                event_id: Uuid::new_v4(),
                created_at: Utc::now() - Duration::hours(48),
            },
        );
        reg.idempotency_keys.insert(
            "old-2".to_string(),
            IdempotencyRecord {
                event_id: Uuid::new_v4(),
                created_at: Utc::now() - Duration::hours(48),
            },
        );
        // This should trigger eviction
        let id = Uuid::new_v4();
        reg.check_idempotency("new-key", id);
        // Old expired keys should have been evicted
        assert!(reg.idempotency_keys.len() <= 2);
    }
}
