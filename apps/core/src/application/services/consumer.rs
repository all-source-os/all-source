use crate::{
    domain::value_objects::system_stream::{SystemDomain, consumer_events, system_entity_id_value},
    infrastructure::{
        memory::ledger::{MapCharge, Owner, string_heap, strings_heap},
        persistence::SystemMetadataStore,
    },
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{mem::size_of, sync::Arc};

/// Owned heap of a consumer: its id and filter strings, not the inline value.
fn consumer_heap(consumer: &Consumer) -> u64 {
    string_heap(&consumer.consumer_id) + strings_heap(&consumer.event_type_filters)
}

/// A durable consumer tracks a cursor (last-acknowledged position) so it can
/// resume from where it left off after disconnection or restart.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Consumer {
    pub consumer_id: String,
    /// Event type prefix filters (e.g. `["scheduler.*", "index.*"]`).
    /// Empty means "all events".
    pub event_type_filters: Vec<String>,
    /// Global event offset of the last acknowledged event.
    /// Events after this offset are "unprocessed" for this consumer.
    /// `None` means the consumer hasn't acked anything yet (start from beginning).
    pub cursor_position: Option<u64>,
}

/// Registry that manages durable consumers with persistent cursor positions.
///
/// Consumer state is stored as system events in the WAL pipeline so it
/// survives Core restarts. In-memory state is held in a DashMap for O(1) access.
///
/// Two modes:
/// - **In-memory** (`new()`): no persistence, for tests and backward compatibility
/// - **Durable** (`new_durable()`): backed by `SystemMetadataStore`, survives restarts
pub struct ConsumerRegistry {
    consumers: Arc<DashMap<String, Consumer>>,
    consumers_charge: MapCharge,
    system_store: Option<Arc<SystemMetadataStore>>,
}

impl Default for ConsumerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ConsumerRegistry {
    /// Create an in-memory-only consumer registry (no persistence).
    pub fn new() -> Self {
        Self {
            consumers: Arc::new(DashMap::new()),
            consumers_charge: MapCharge::new(Owner::Misc, size_of::<(String, Consumer)>()),
            system_store: None,
        }
    }

    /// Create a durable consumer registry backed by `SystemMetadataStore`.
    ///
    /// On construction, replays all `_system.consumer.*` events to rebuild
    /// the in-memory cache from WAL.
    pub fn new_durable(system_store: Arc<SystemMetadataStore>) -> Self {
        let registry = Self {
            consumers: Arc::new(DashMap::new()),
            consumers_charge: MapCharge::new(Owner::Misc, size_of::<(String, Consumer)>()),
            system_store: Some(system_store),
        };
        registry.rebuild_cache();
        registry
    }

    /// Register a consumer (or update its filters if it already exists).
    pub fn register(&self, consumer_id: &str, event_type_filters: &[String]) -> Consumer {
        let c = self.consumers_charge.upsert(
            &self.consumers,
            consumer_id.to_string(),
            || Consumer {
                consumer_id: consumer_id.to_string(),
                event_type_filters: event_type_filters.to_vec(),
                cursor_position: None,
            },
            |_, c| consumer_heap(c),
            |existing| {
                if existing.event_type_filters != event_type_filters {
                    existing.event_type_filters = event_type_filters.to_vec();
                }
                existing.clone()
            },
        );

        // Persist registration event
        if let Some(ref store) = self.system_store {
            let entity_id = system_entity_id_value(SystemDomain::Consumer, consumer_id);
            let payload = json!({
                "consumer_id": consumer_id,
                "event_type_filters": event_type_filters,
            });
            if let Err(e) =
                store.append_system_event(consumer_events::REGISTERED, entity_id, payload, None)
            {
                tracing::warn!("Failed to persist consumer registration: {}", e);
            }
        }

        c
    }

    /// Get a consumer by ID. Returns None if not registered.
    pub fn get(&self, consumer_id: &str) -> Option<Consumer> {
        self.consumers.get(consumer_id).map(|c| c.clone())
    }

    /// Get or implicitly create a consumer.
    pub fn get_or_create(&self, consumer_id: &str) -> Consumer {
        self.consumers_charge.upsert(
            &self.consumers,
            consumer_id.to_string(),
            || Consumer {
                consumer_id: consumer_id.to_string(),
                event_type_filters: vec![],
                cursor_position: None,
            },
            |_, c| consumer_heap(c),
            |c| c.clone(),
        )
    }

    /// Acknowledge events up to a given global offset.
    /// Returns Ok(()) on success, Err if the position is beyond the max offset.
    pub fn ack(&self, consumer_id: &str, position: u64, max_offset: u64) -> Result<(), String> {
        if position > max_offset {
            return Err(format!(
                "Position {position} is beyond the latest event offset {max_offset}"
            ));
        }

        self.consumers_charge.upsert(
            &self.consumers,
            consumer_id.to_string(),
            || Consumer {
                consumer_id: consumer_id.to_string(),
                event_type_filters: vec![],
                cursor_position: None,
            },
            |_, c| consumer_heap(c),
            |entry| {
                let current = entry.cursor_position.unwrap_or(0);
                if position > current {
                    entry.cursor_position = Some(position);

                    if let Some(ref store) = self.system_store {
                        let entity_id = system_entity_id_value(SystemDomain::Consumer, consumer_id);
                        let payload = json!({
                            "consumer_id": consumer_id,
                            "position": position,
                        });
                        if let Err(e) = store.append_system_event(
                            consumer_events::ACK_UPDATED,
                            entity_id,
                            payload,
                            None,
                        ) {
                            tracing::warn!("Failed to persist consumer ack: {}", e);
                        }
                    }
                }
            },
        );

        Ok(())
    }

    /// Restore consumer state (called during WAL recovery).
    pub fn restore(&self, consumer: Consumer) {
        self.consumers_charge.insert(
            &self.consumers,
            consumer.consumer_id.clone(),
            consumer,
            |_, c| consumer_heap(c),
        );
    }

    /// Number of registered consumers.
    pub fn count(&self) -> usize {
        self.consumers.len()
    }

    /// Rebuild in-memory cache from system store events.
    fn rebuild_cache(&self) {
        let Some(ref store) = self.system_store else {
            return;
        };
        let events = store.read_stream(SystemDomain::Consumer);
        for event in &events {
            self.apply_event(event);
        }
        tracing::info!(
            "Rebuilt consumer cache: {} consumers from {} events",
            self.consumers.len(),
            events.len()
        );
    }

    /// Apply a single system event to the in-memory cache.
    fn apply_event(&self, event: &crate::domain::entities::Event) {
        let event_type = event.event_type_str();
        let payload = event.payload();

        match event_type {
            consumer_events::REGISTERED => {
                let consumer_id = payload
                    .get("consumer_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                let event_type_filters: Vec<String> = payload
                    .get("event_type_filters")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();

                if !consumer_id.is_empty() {
                    self.consumers_charge.insert(
                        &self.consumers,
                        consumer_id.clone(),
                        Consumer {
                            consumer_id,
                            event_type_filters,
                            cursor_position: None,
                        },
                        |_, c| consumer_heap(c),
                    );
                }
            }
            consumer_events::ACK_UPDATED => {
                let consumer_id = payload
                    .get("consumer_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                let position = payload.get("position").and_then(serde_json::Value::as_u64);

                if !consumer_id.is_empty() {
                    self.consumers_charge.upsert(
                        &self.consumers,
                        consumer_id.clone(),
                        || Consumer {
                            consumer_id: consumer_id.clone(),
                            event_type_filters: vec![],
                            cursor_position: position,
                        },
                        |_, c| consumer_heap(c),
                        |c| {
                            if let Some(pos) = position {
                                let current = c.cursor_position.unwrap_or(0);
                                if pos > current {
                                    c.cursor_position = Some(pos);
                                }
                            }
                        },
                    );
                }
            }
            consumer_events::DELETED => {
                let consumer_id = payload
                    .get("consumer_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                if !consumer_id.is_empty() {
                    self.consumers_charge
                        .remove_entry(&self.consumers, consumer_id, |_, c| consumer_heap(c));
                }
            }
            _ => {}
        }
    }

    /// Check if an event type matches a consumer's filters.
    /// Empty filters = match all.
    pub fn matches_filters(event_type: &str, filters: &[String]) -> bool {
        if filters.is_empty() {
            return true;
        }
        filters.iter().any(|filter| {
            if let Some(prefix) = filter.strip_suffix(".*") {
                event_type.starts_with(prefix)
                    && event_type
                        .as_bytes()
                        .get(prefix.len())
                        .is_none_or(|&b| b == b'.')
            } else {
                event_type == filter
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_and_get() {
        let registry = ConsumerRegistry::new();
        let c = registry.register("c1", &["scheduler.*".into()]);
        assert_eq!(c.consumer_id, "c1");
        assert_eq!(c.event_type_filters, vec!["scheduler.*"]);
        assert_eq!(c.cursor_position, None);

        let fetched = registry.get("c1").unwrap();
        assert_eq!(fetched.consumer_id, "c1");
    }

    #[test]
    fn test_get_or_create() {
        let registry = ConsumerRegistry::new();
        assert!(registry.get("c1").is_none());

        let c = registry.get_or_create("c1");
        assert_eq!(c.consumer_id, "c1");
        assert!(c.event_type_filters.is_empty());

        // Second call returns same consumer
        let c2 = registry.get_or_create("c1");
        assert_eq!(c2.consumer_id, "c1");
    }

    #[test]
    fn test_ack_advances_cursor() {
        let registry = ConsumerRegistry::new();
        registry.register("c1", &[]);

        registry.ack("c1", 5, 10).unwrap();
        assert_eq!(registry.get("c1").unwrap().cursor_position, Some(5));

        // Advance further
        registry.ack("c1", 8, 10).unwrap();
        assert_eq!(registry.get("c1").unwrap().cursor_position, Some(8));
    }

    #[test]
    fn test_ack_idempotent_no_regression() {
        let registry = ConsumerRegistry::new();
        registry.register("c1", &[]);

        registry.ack("c1", 5, 10).unwrap();
        // Acking an earlier position is a no-op
        registry.ack("c1", 3, 10).unwrap();
        assert_eq!(registry.get("c1").unwrap().cursor_position, Some(5));
    }

    #[test]
    fn test_ack_beyond_max_fails() {
        let registry = ConsumerRegistry::new();
        registry.register("c1", &[]);

        let result = registry.ack("c1", 15, 10);
        assert!(result.is_err());
    }

    #[test]
    fn test_ack_auto_creates_consumer() {
        let registry = ConsumerRegistry::new();
        registry.ack("c1", 5, 10).unwrap();
        assert_eq!(registry.get("c1").unwrap().cursor_position, Some(5));
    }

    #[test]
    fn test_matches_filters_empty() {
        assert!(ConsumerRegistry::matches_filters("anything", &[]));
    }

    #[test]
    fn test_matches_filters_prefix() {
        let filters = vec!["scheduler.*".to_string()];
        assert!(ConsumerRegistry::matches_filters(
            "scheduler.started",
            &filters
        ));
        assert!(ConsumerRegistry::matches_filters(
            "scheduler.completed",
            &filters
        ));
        assert!(!ConsumerRegistry::matches_filters(
            "trade.executed",
            &filters
        ));
    }

    #[test]
    fn test_matches_filters_exact() {
        let filters = vec!["scheduler.started".to_string()];
        assert!(ConsumerRegistry::matches_filters(
            "scheduler.started",
            &filters
        ));
        assert!(!ConsumerRegistry::matches_filters(
            "scheduler.completed",
            &filters
        ));
    }

    #[test]
    fn test_matches_filters_multiple() {
        let filters = vec!["scheduler.*".to_string(), "index.*".to_string()];
        assert!(ConsumerRegistry::matches_filters(
            "scheduler.started",
            &filters
        ));
        assert!(ConsumerRegistry::matches_filters("index.created", &filters));
        assert!(!ConsumerRegistry::matches_filters(
            "trade.executed",
            &filters
        ));
    }

    #[test]
    fn test_restore() {
        let registry = ConsumerRegistry::new();
        registry.restore(Consumer {
            consumer_id: "c1".into(),
            event_type_filters: vec!["scheduler.*".into()],
            cursor_position: Some(42),
        });

        let c = registry.get("c1").unwrap();
        assert_eq!(c.cursor_position, Some(42));
        assert_eq!(c.event_type_filters, vec!["scheduler.*"]);
    }

    #[test]
    fn test_count() {
        let registry = ConsumerRegistry::new();
        assert_eq!(registry.count(), 0);
        registry.register("c1", &[]);
        assert_eq!(registry.count(), 1);
        registry.register("c2", &[]);
        assert_eq!(registry.count(), 2);
    }

    #[test]
    fn test_in_memory_mode_still_works() {
        // Ensure new() works without a system store
        let registry = ConsumerRegistry::new();
        registry.register("c1", &["trade.*".into()]);
        registry.ack("c1", 10, 100).unwrap();
        let c = registry.get("c1").unwrap();
        assert_eq!(c.cursor_position, Some(10));
        assert_eq!(c.event_type_filters, vec!["trade.*"]);
    }

    #[test]
    fn test_durable_register_persists() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let store = Arc::new(SystemMetadataStore::new(temp_dir.path()).unwrap());

        // Register a consumer with durable registry
        {
            let registry = ConsumerRegistry::new_durable(store.clone());
            registry.register("c1", &["scheduler.*".into()]);
            assert_eq!(registry.count(), 1);
        }

        // Recreate from same store — should recover consumer
        {
            let registry = ConsumerRegistry::new_durable(store.clone());
            assert_eq!(registry.count(), 1);
            let c = registry.get("c1").unwrap();
            assert_eq!(c.consumer_id, "c1");
            assert_eq!(c.event_type_filters, vec!["scheduler.*"]);
            assert_eq!(c.cursor_position, None);
        }
    }

    #[test]
    fn test_durable_ack_persists() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let store = Arc::new(SystemMetadataStore::new(temp_dir.path()).unwrap());

        // Register + ack
        {
            let registry = ConsumerRegistry::new_durable(store.clone());
            registry.register("c1", &[]);
            registry.ack("c1", 42, 100).unwrap();
            assert_eq!(registry.get("c1").unwrap().cursor_position, Some(42));
        }

        // Recreate — cursor should survive
        {
            let registry = ConsumerRegistry::new_durable(store.clone());
            let c = registry.get("c1").unwrap();
            assert_eq!(c.cursor_position, Some(42));
        }
    }

    #[test]
    fn test_durable_recovery_multiple_acks() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let store = Arc::new(SystemMetadataStore::new(temp_dir.path()).unwrap());

        // Ack multiple times
        {
            let registry = ConsumerRegistry::new_durable(store.clone());
            registry.register("c1", &[]);
            registry.ack("c1", 10, 100).unwrap();
            registry.ack("c1", 25, 100).unwrap();
            registry.ack("c1", 50, 100).unwrap();
        }

        // Only latest position should be recovered
        {
            let registry = ConsumerRegistry::new_durable(store.clone());
            let c = registry.get("c1").unwrap();
            assert_eq!(c.cursor_position, Some(50));
        }
    }
}
