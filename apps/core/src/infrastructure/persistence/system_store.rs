use crate::{
    domain::{
        entities::Event,
        value_objects::{
            EntityId,
            system_stream::{
                SYSTEM_ENTITY_ID_PREFIX, SystemDomain, is_system_event_type,
                system_entity_id_value, system_event_type, system_tenant_id,
            },
        },
    },
    error::{AllSourceError, Result},
    infrastructure::persistence::wal::{WALConfig, WriteAheadLog},
};
use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde_json::Value as JsonValue;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

/// SystemMetadataStore provides durable storage for operational metadata.
///
/// It uses the same WAL-based durability model as the main EventStore but
/// writes to a separate directory for fault isolation. This means:
///
/// - A full disk on data storage does not prevent metadata writes
/// - System stream WAL can be backed up independently
/// - Different compaction/retention policies can apply
///
/// # Architecture
///
/// ```text
/// ┌──────────────────────────┐
/// │   SystemMetadataStore    │
/// │  ┌────────────────────┐  │
/// │  │  In-Memory Events  │  │  ← DashMap-backed read cache
/// │  │  (Vec<Event>)      │  │
/// │  └────────────────────┘  │
/// │  ┌────────────────────┐  │
/// │  │  WAL (fsync)       │  │  ← Durability guarantee
/// │  │  {system_dir}/wal  │  │
/// │  └────────────────────┘  │
/// └──────────────────────────┘
/// ```
pub struct SystemMetadataStore {
    /// In-memory event storage (recovered from WAL on startup)
    events: Arc<RwLock<Vec<Event>>>,

    /// Write-ahead log for durability
    wal: Arc<WriteAheadLog>,

    /// Base directory for system data
    data_dir: PathBuf,
}

impl SystemMetadataStore {
    /// Create a new SystemMetadataStore with the given data directory.
    ///
    /// On creation, the store:
    /// 1. Initializes WAL in `{data_dir}/wal/`
    /// 2. Recovers any existing events from WAL
    /// 3. Populates the in-memory cache
    pub fn new(data_dir: impl Into<PathBuf>) -> Result<Self> {
        let data_dir = data_dir.into();
        let wal_dir = data_dir.join("wal");

        // Ensure directories exist
        std::fs::create_dir_all(&wal_dir).map_err(|e| {
            AllSourceError::StorageError(format!(
                "Failed to create system WAL directory {}: {}",
                wal_dir.display(),
                e
            ))
        })?;

        let wal_config = WALConfig {
            max_file_size: 16 * 1024 * 1024, // 16 MB (smaller than data WAL)
            sync_on_write: true,
            max_wal_files: 5,
            compress: false,
            ..WALConfig::default()
        };

        let wal = WriteAheadLog::new(&wal_dir, wal_config)?;
        let wal = Arc::new(wal);

        let events = Arc::new(RwLock::new(Vec::new()));

        // Recover from WAL
        match wal.recover_for_append() {
            Ok(recovered_events) if !recovered_events.is_empty() => {
                tracing::info!(
                    "Recovered {} system events from WAL",
                    recovered_events.len()
                );
                let mut event_vec = events.write();
                for event in recovered_events {
                    if is_system_event_type(event.event_type_str()) {
                        event_vec.push(event);
                    } else {
                        tracing::warn!(
                            "Skipping non-system event in system WAL: {}",
                            event.event_type_str()
                        );
                    }
                }
            }
            Ok(_) => {
                tracing::debug!("No system events to recover from WAL");
            }
            Err(e) => {
                tracing::error!("System WAL recovery failed: {}", e);
                return Err(AllSourceError::StorageError(format!(
                    "System WAL recovery failed: {e}"
                )));
            }
        }

        Ok(Self {
            events,
            wal,
            data_dir,
        })
    }

    /// Append a system event to the store.
    ///
    /// The event is:
    /// 1. Validated as a system event
    /// 2. Written to WAL (fsync)
    /// 3. Added to in-memory cache
    pub fn append(&self, event: Event) -> Result<()> {
        // Validate: must be a system event
        if !is_system_event_type(event.event_type_str()) {
            return Err(AllSourceError::ValidationError(format!(
                "SystemMetadataStore only accepts _system.* event types, got '{}'",
                event.event_type_str()
            )));
        }

        // Write to WAL first (durability)
        self.wal.append(event.clone())?;

        // Add to in-memory cache
        self.events.write().push(event);

        Ok(())
    }

    /// Create and append a system event from components.
    pub fn append_system_event(
        &self,
        event_type_str: &str,
        entity_id: EntityId,
        payload: JsonValue,
        metadata: Option<JsonValue>,
    ) -> Result<Event> {
        let event_type = system_event_type(event_type_str);
        let tenant_id = system_tenant_id();

        let event = if let Some(metadata) = metadata {
            Event::with_metadata(event_type, entity_id, tenant_id, payload, metadata)
        } else {
            Event::new(event_type, entity_id, tenant_id, payload)
        };

        self.append(event.clone())?;
        Ok(event)
    }

    /// Read all events from a system stream (filtered by entity_id prefix).
    ///
    /// For example, `read_stream(SystemDomain::Tenant)` returns all events
    /// with entity_id starting with `_system:tenant:`.
    pub fn read_stream(&self, domain: SystemDomain) -> Vec<Event> {
        let prefix = format!("{}{}", SYSTEM_ENTITY_ID_PREFIX, domain.as_str());
        let events = self.events.read();
        events
            .iter()
            .filter(|e| e.entity_id_str().starts_with(&prefix))
            .cloned()
            .collect()
    }

    /// Read all events for a specific entity in a system stream.
    ///
    /// For example, `read_entity(SystemDomain::Tenant, "acme-corp")` returns all
    /// events with entity_id `_system:tenant:acme-corp`.
    pub fn read_entity(&self, domain: SystemDomain, resource_id: &str) -> Vec<Event> {
        let entity_id = system_entity_id_value(domain, resource_id);
        let entity_id_str = entity_id.as_str();
        let events = self.events.read();
        events
            .iter()
            .filter(|e| e.entity_id_str() == entity_id_str)
            .cloned()
            .collect()
    }

    /// Read events from a system stream starting from a given version (position).
    ///
    /// This is useful for incremental catch-up after initial bootstrap.
    pub fn read_stream_from(&self, domain: SystemDomain, from_version: usize) -> Vec<Event> {
        let prefix = format!("{}{}", SYSTEM_ENTITY_ID_PREFIX, domain.as_str());
        let events = self.events.read();
        events
            .iter()
            .filter(|e| e.entity_id_str().starts_with(&prefix))
            .skip(from_version)
            .cloned()
            .collect()
    }

    /// Read all system events (across all domains).
    pub fn read_all(&self) -> Vec<Event> {
        self.events.read().clone()
    }

    /// Count events in a system stream.
    pub fn count_stream(&self, domain: SystemDomain) -> usize {
        let prefix = format!("{}{}", SYSTEM_ENTITY_ID_PREFIX, domain.as_str());
        let events = self.events.read();
        events
            .iter()
            .filter(|e| e.entity_id_str().starts_with(&prefix))
            .count()
    }

    /// Total number of system events.
    pub fn total_events(&self) -> usize {
        self.events.read().len()
    }

    /// Get the data directory path.
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Read events filtered by event type within a time range.
    pub fn query_by_type_and_time(
        &self,
        event_type_str: &str,
        start: Option<DateTime<Utc>>,
        end: Option<DateTime<Utc>>,
        limit: Option<usize>,
    ) -> Vec<Event> {
        let events = self.events.read();
        let mut results: Vec<Event> = events
            .iter()
            .filter(|e| {
                if e.event_type_str() != event_type_str {
                    return false;
                }
                if let Some(start) = start
                    && e.timestamp() < start
                {
                    return false;
                }
                if let Some(end) = end
                    && e.timestamp() > end
                {
                    return false;
                }
                true
            })
            .cloned()
            .collect();

        results.sort_by_key(crate::domain::entities::event::Event::timestamp);

        if let Some(limit) = limit {
            results.truncate(limit);
        }

        results
    }
}

impl std::fmt::Debug for SystemMetadataStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SystemMetadataStore")
            .field("data_dir", &self.data_dir)
            .field("total_events", &self.events.read().len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::value_objects::system_stream::{SystemDomain, tenant_events};
    use tempfile::TempDir;

    fn create_test_store() -> (SystemMetadataStore, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let store = SystemMetadataStore::new(temp_dir.path().join("__system")).unwrap();
        (store, temp_dir)
    }

    #[test]
    fn test_create_system_store() {
        let (store, _dir) = create_test_store();
        assert_eq!(store.total_events(), 0);
    }

    #[test]
    fn test_append_system_event() {
        let (store, _dir) = create_test_store();

        let entity_id = system_entity_id_value(SystemDomain::Tenant, "acme");
        let event = store
            .append_system_event(
                tenant_events::CREATED,
                entity_id,
                serde_json::json!({"name": "ACME Corp", "quotas": {}}),
                None,
            )
            .unwrap();

        assert_eq!(store.total_events(), 1);
        assert_eq!(event.event_type_str(), "_system.tenant.created");
        assert_eq!(event.entity_id_str(), "_system:tenant:acme");
        assert_eq!(event.tenant_id_str(), "_system");
    }

    #[test]
    fn test_reject_non_system_event() {
        let (store, _dir) = create_test_store();

        let event = Event::from_strings(
            "order.placed".to_string(),
            "order-123".to_string(),
            "default".to_string(),
            serde_json::json!({}),
            None,
        )
        .unwrap();

        let result = store.append(event);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("only accepts _system.*")
        );
    }

    #[test]
    fn test_read_stream() {
        let (store, _dir) = create_test_store();

        // Add tenant events
        store
            .append_system_event(
                tenant_events::CREATED,
                system_entity_id_value(SystemDomain::Tenant, "acme"),
                serde_json::json!({"name": "ACME"}),
                None,
            )
            .unwrap();

        store
            .append_system_event(
                tenant_events::CREATED,
                system_entity_id_value(SystemDomain::Tenant, "beta"),
                serde_json::json!({"name": "Beta"}),
                None,
            )
            .unwrap();

        // Add config event (different domain)
        store
            .append_system_event(
                "_system.config.set",
                system_entity_id_value(SystemDomain::Config, "max_conn"),
                serde_json::json!({"value": 100}),
                None,
            )
            .unwrap();

        // Read tenant stream only
        let tenant_events = store.read_stream(SystemDomain::Tenant);
        assert_eq!(tenant_events.len(), 2);

        // Read config stream only
        let config_events = store.read_stream(SystemDomain::Config);
        assert_eq!(config_events.len(), 1);

        // Total
        assert_eq!(store.total_events(), 3);
    }

    #[test]
    fn test_read_entity() {
        let (store, _dir) = create_test_store();

        store
            .append_system_event(
                tenant_events::CREATED,
                system_entity_id_value(SystemDomain::Tenant, "acme"),
                serde_json::json!({"name": "ACME"}),
                None,
            )
            .unwrap();

        store
            .append_system_event(
                tenant_events::UPDATED,
                system_entity_id_value(SystemDomain::Tenant, "acme"),
                serde_json::json!({"name": "ACME Corp"}),
                None,
            )
            .unwrap();

        store
            .append_system_event(
                tenant_events::CREATED,
                system_entity_id_value(SystemDomain::Tenant, "other"),
                serde_json::json!({"name": "Other"}),
                None,
            )
            .unwrap();

        let acme_events = store.read_entity(SystemDomain::Tenant, "acme");
        assert_eq!(acme_events.len(), 2);
        assert_eq!(acme_events[0].event_type_str(), "_system.tenant.created");
        assert_eq!(acme_events[1].event_type_str(), "_system.tenant.updated");
    }

    #[test]
    fn test_read_stream_from_version() {
        let (store, _dir) = create_test_store();

        for i in 0..5 {
            store
                .append_system_event(
                    tenant_events::CREATED,
                    system_entity_id_value(SystemDomain::Tenant, &format!("t{i}")),
                    serde_json::json!({"index": i}),
                    None,
                )
                .unwrap();
        }

        // Read from version 3 (skip first 3)
        let events = store.read_stream_from(SystemDomain::Tenant, 3);
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_count_stream() {
        let (store, _dir) = create_test_store();

        store
            .append_system_event(
                tenant_events::CREATED,
                system_entity_id_value(SystemDomain::Tenant, "a"),
                serde_json::json!({}),
                None,
            )
            .unwrap();

        store
            .append_system_event(
                tenant_events::CREATED,
                system_entity_id_value(SystemDomain::Tenant, "b"),
                serde_json::json!({}),
                None,
            )
            .unwrap();

        assert_eq!(store.count_stream(SystemDomain::Tenant), 2);
        assert_eq!(store.count_stream(SystemDomain::Config), 0);
    }

    #[test]
    fn test_wal_recovery() {
        let temp_dir = TempDir::new().unwrap();
        let system_dir = temp_dir.path().join("__system");

        // Write events
        {
            let store = SystemMetadataStore::new(&system_dir).unwrap();
            store
                .append_system_event(
                    tenant_events::CREATED,
                    system_entity_id_value(SystemDomain::Tenant, "acme"),
                    serde_json::json!({"name": "ACME"}),
                    None,
                )
                .unwrap();

            store
                .append_system_event(
                    "_system.config.set",
                    system_entity_id_value(SystemDomain::Config, "key1"),
                    serde_json::json!({"value": "hello"}),
                    None,
                )
                .unwrap();

            assert_eq!(store.total_events(), 2);
        }
        // Store dropped — simulates restart

        // Recover from WAL
        {
            let store = SystemMetadataStore::new(&system_dir).unwrap();
            assert_eq!(store.total_events(), 2);

            let tenant_events = store.read_stream(SystemDomain::Tenant);
            assert_eq!(tenant_events.len(), 1);
            assert_eq!(tenant_events[0].entity_id_str(), "_system:tenant:acme");

            let config_events = store.read_stream(SystemDomain::Config);
            assert_eq!(config_events.len(), 1);
        }
    }

    #[test]
    fn test_query_by_type_and_time() {
        let (store, _dir) = create_test_store();

        store
            .append_system_event(
                tenant_events::CREATED,
                system_entity_id_value(SystemDomain::Tenant, "a"),
                serde_json::json!({}),
                None,
            )
            .unwrap();

        store
            .append_system_event(
                tenant_events::UPDATED,
                system_entity_id_value(SystemDomain::Tenant, "a"),
                serde_json::json!({}),
                None,
            )
            .unwrap();

        let results = store.query_by_type_and_time(tenant_events::CREATED, None, None, None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].event_type_str(), "_system.tenant.created");
    }

    #[test]
    fn test_debug_impl() {
        let (store, _dir) = create_test_store();
        let debug_str = format!("{store:?}");
        assert!(debug_str.contains("SystemMetadataStore"));
        assert!(debug_str.contains("total_events"));
    }
}
