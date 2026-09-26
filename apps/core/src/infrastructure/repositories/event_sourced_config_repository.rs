use crate::{
    domain::value_objects::system_stream::{SystemDomain, config_events, system_entity_id_value},
    error::{AllSourceError, Result},
    infrastructure::persistence::SystemMetadataStore,
};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// A configuration entry with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigEntry {
    pub key: String,
    pub value: serde_json::Value,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<String>,
    /// The durable event identifier, stable across restart and unique per write.
    pub revision: Uuid,
}

/// A condition evaluated against the same locked state that is durably changed.
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ConfigCondition {
    Absent {},
    Revision { revision: Uuid },
}

/// Event-sourced ConfigRepository backed by SystemMetadataStore.
///
/// Stores configuration as key-value pairs using log-compaction semantics:
/// only the latest value per key matters for the current state, but the full
/// history is available for temporal queries.
///
/// # Cache Consistency
///
/// Uses `DashMap<String, ConfigEntry>` for concurrent O(1) lookups by key.
/// The write path is: WAL fsync → cache update, so no acknowledged writes are
/// lost even on crash. On startup, the cache is rebuilt by replaying all config
/// events from the system store.
///
/// # Event Types
///
/// - `_system.config.set` — Set or update a config key
/// - `_system.config.deleted` — Delete a config key
pub struct EventSourcedConfigRepository {
    system_store: Arc<SystemMetadataStore>,
    cache: Arc<DashMap<String, ConfigEntry>>,
    // Operational configuration is not the ingest hot path. A bounded single
    // lock also serializes legacy writes/deletes with conditional writes, so
    // they cannot invalidate a successful check before its WAL/cache update.
    write_lock: Mutex<()>,
}

impl EventSourcedConfigRepository {
    /// Create a new event-sourced config repository.
    ///
    /// Replays all existing config events from the system store.
    pub fn new(system_store: Arc<SystemMetadataStore>) -> Self {
        let cache = Arc::new(DashMap::new());
        let repo = Self {
            system_store,
            cache,
            write_lock: Mutex::new(()),
        };
        repo.rebuild_cache();
        repo
    }

    /// Rebuild the in-memory cache from system store events.
    fn rebuild_cache(&self) {
        let events = self.system_store.read_stream(SystemDomain::Config);
        for event in events {
            let event_type = event.event_type_str();
            let payload = event.payload();

            let key = match event.entity_id_str().strip_prefix("_system:config:") {
                Some(k) => k.to_string(),
                None => continue,
            };

            match event_type {
                t if t == config_events::SET => {
                    let value = payload
                        .get("value")
                        .cloned()
                        .unwrap_or(serde_json::Value::Null);
                    let updated_by = payload["changed_by"]
                        .as_str()
                        .map(std::string::ToString::to_string);
                    self.cache.insert(
                        key.clone(),
                        ConfigEntry {
                            key,
                            value,
                            updated_at: event.timestamp(),
                            updated_by,
                            revision: event.id(),
                        },
                    );
                }
                t if t == config_events::DELETED => {
                    self.cache.remove(&key);
                }
                _ => {}
            }
        }
        tracing::info!(
            "Rebuilt config cache: {} entries from system store",
            self.cache.len()
        );
    }

    /// Emit an event to the system store and return the stored event.
    ///
    /// The system store append is durable (WAL fsync) before the caller
    /// updates the cache, ensuring no acknowledged writes are lost.
    fn emit_event(
        &self,
        event_type_str: &str,
        key: &str,
        payload: serde_json::Value,
    ) -> Result<crate::domain::entities::Event> {
        let entity_id = system_entity_id_value(SystemDomain::Config, key);
        self.system_store
            .append_system_event(event_type_str, entity_id, payload, None)
    }

    /// Set a configuration value.
    pub fn set(&self, key: &str, value: serde_json::Value, changed_by: Option<&str>) -> Result<()> {
        self.set_conditionally(key, value, changed_by, None)?;
        Ok(())
    }

    /// Compare and durably replace a value. A mismatch writes no event.
    ///
    /// Conditions share the writer lock with unconditional writes and deletes.
    /// Event IDs, rather than value equality, reject stale ABA observations.
    pub fn set_conditionally(
        &self,
        key: &str,
        value: serde_json::Value,
        changed_by: Option<&str>,
        condition: Option<&ConfigCondition>,
    ) -> Result<ConfigEntry> {
        let _guard = self.write_lock.lock().map_err(|_| {
            AllSourceError::InternalError("Configuration writer unavailable".into())
        })?;

        let matches = match condition {
            None => true,
            Some(ConfigCondition::Absent {}) => !self.cache.contains_key(key),
            Some(ConfigCondition::Revision { revision }) => self
                .cache
                .get(key)
                .is_some_and(|entry| entry.revision == *revision),
        };
        if !matches {
            return Err(AllSourceError::ConcurrencyError(
                "Configuration precondition failed".into(),
            ));
        }

        let payload = serde_json::json!({
            "value": value,
            "changed_by": changed_by,
        });

        let event = self.emit_event(config_events::SET, key, payload)?;

        let entry = ConfigEntry {
            key: key.to_string(),
            value,
            updated_at: event.timestamp(),
            updated_by: changed_by.map(std::string::ToString::to_string),
            revision: event.id(),
        };
        self.cache.insert(key.to_string(), entry.clone());
        Ok(entry)
    }

    /// Get the current value for a key.
    pub fn get(&self, key: &str) -> Option<ConfigEntry> {
        self.cache.get(key).map(|entry| entry.value().clone())
    }

    /// Get just the value for a key.
    pub fn get_value(&self, key: &str) -> Option<serde_json::Value> {
        self.cache.get(key).map(|entry| entry.value().value.clone())
    }

    /// List all current config entries.
    pub fn list(&self) -> Vec<ConfigEntry> {
        self.cache.iter().map(|e| e.value().clone()).collect()
    }

    /// Delete a configuration key.
    pub fn delete(&self, key: &str, deleted_by: Option<&str>) -> Result<bool> {
        let _guard = self.write_lock.lock().map_err(|_| {
            AllSourceError::InternalError("Configuration writer unavailable".into())
        })?;
        if !self.cache.contains_key(key) {
            return Ok(false);
        }

        let payload = serde_json::json!({
            "deleted_by": deleted_by,
        });
        self.emit_event(config_events::DELETED, key, payload)?;

        self.cache.remove(key);
        Ok(true)
    }

    /// Get the config value at a specific point in time.
    ///
    /// Replays config events up to the given timestamp to determine
    /// what the value was at that time.
    pub fn get_at(&self, key: &str, as_of: DateTime<Utc>) -> Option<serde_json::Value> {
        let events = self.system_store.read_entity(SystemDomain::Config, key);
        let mut current_value: Option<serde_json::Value> = None;

        for event in events {
            if event.timestamp() > as_of {
                break;
            }
            match event.event_type_str() {
                t if t == config_events::SET => {
                    current_value = event.payload().get("value").cloned();
                }
                t if t == config_events::DELETED => {
                    current_value = None;
                }
                _ => {}
            }
        }

        current_value
    }

    /// Count of current config entries.
    pub fn count(&self) -> usize {
        self.cache.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::persistence::SystemMetadataStore;
    use tempfile::TempDir;

    fn create_test_repo() -> (EventSourcedConfigRepository, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let system_store =
            Arc::new(SystemMetadataStore::new(temp_dir.path().join("__system")).unwrap());
        let repo = EventSourcedConfigRepository::new(system_store);
        (repo, temp_dir)
    }

    #[test]
    fn test_set_and_get() {
        let (repo, _dir) = create_test_repo();

        repo.set("max_connections", serde_json::json!(100), Some("admin"))
            .unwrap();

        let entry = repo.get("max_connections");
        assert!(entry.is_some());
        let entry = entry.unwrap();
        assert_eq!(entry.value, serde_json::json!(100));
        assert_eq!(entry.updated_by, Some("admin".to_string()));
    }

    #[test]
    fn test_get_nonexistent() {
        let (repo, _dir) = create_test_repo();
        assert!(repo.get("nonexistent").is_none());
    }

    #[test]
    fn test_update_value() {
        let (repo, _dir) = create_test_repo();

        repo.set("key1", serde_json::json!("initial"), None)
            .unwrap();
        repo.set("key1", serde_json::json!("updated"), None)
            .unwrap();

        let value = repo.get_value("key1").unwrap();
        assert_eq!(value, serde_json::json!("updated"));
    }

    #[test]
    fn test_delete() {
        let (repo, _dir) = create_test_repo();

        repo.set("key1", serde_json::json!("value"), None).unwrap();
        assert!(repo.delete("key1", Some("admin")).unwrap());
        assert!(repo.get("key1").is_none());

        // Delete non-existent returns false
        assert!(!repo.delete("key1", None).unwrap());
    }

    #[test]
    fn test_list() {
        let (repo, _dir) = create_test_repo();

        repo.set("key1", serde_json::json!("a"), None).unwrap();
        repo.set("key2", serde_json::json!("b"), None).unwrap();
        repo.set("key3", serde_json::json!("c"), None).unwrap();

        let entries = repo.list();
        assert_eq!(entries.len(), 3);
    }

    #[test]
    fn test_count() {
        let (repo, _dir) = create_test_repo();
        assert_eq!(repo.count(), 0);

        repo.set("k1", serde_json::json!(1), None).unwrap();
        repo.set("k2", serde_json::json!(2), None).unwrap();
        assert_eq!(repo.count(), 2);

        repo.delete("k1", None).unwrap();
        assert_eq!(repo.count(), 1);
    }

    #[test]
    fn test_wal_recovery() {
        let temp_dir = TempDir::new().unwrap();
        let system_dir = temp_dir.path().join("__system");

        // Write config
        {
            let system_store = Arc::new(SystemMetadataStore::new(&system_dir).unwrap());
            let repo = EventSourcedConfigRepository::new(system_store);
            repo.set("key1", serde_json::json!("value1"), None).unwrap();
            repo.set("key2", serde_json::json!("value2"), None).unwrap();
            repo.delete("key2", None).unwrap();
        }

        // Recover
        {
            let system_store = Arc::new(SystemMetadataStore::new(&system_dir).unwrap());
            let repo = EventSourcedConfigRepository::new(system_store);
            assert_eq!(repo.count(), 1);
            assert_eq!(repo.get_value("key1").unwrap(), serde_json::json!("value1"));
            assert!(repo.get("key2").is_none());
        }
    }

    #[test]
    fn test_temporal_query() {
        let (repo, _dir) = create_test_repo();

        repo.set("key1", serde_json::json!("v1"), None).unwrap();
        let after_v1 = chrono::Utc::now();

        // Small delay to ensure distinct timestamps
        std::thread::sleep(std::time::Duration::from_millis(10));

        repo.set("key1", serde_json::json!("v2"), None).unwrap();

        // Current value
        assert_eq!(repo.get_value("key1").unwrap(), serde_json::json!("v2"));

        // Value at point in time
        let past_value = repo.get_at("key1", after_v1);
        assert_eq!(past_value, Some(serde_json::json!("v1")));
    }
}
