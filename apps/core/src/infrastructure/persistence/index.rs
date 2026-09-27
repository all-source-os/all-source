use crate::error::Result;
use dashmap::DashMap;
use std::sync::Arc;
use uuid::Uuid;

/// Event index entry
#[derive(Debug, Clone)]
pub struct IndexEntry {
    pub event_id: Uuid,
    pub offset: usize,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// High-performance concurrent index for fast event lookups
pub struct EventIndex {
    /// Index by entity_id -> list of event entries
    entity_index: Arc<DashMap<String, Vec<IndexEntry>>>,

    /// Index by event_type -> list of event entries
    type_index: Arc<DashMap<String, Vec<IndexEntry>>>,

    /// Index by event_id -> offset (for direct lookups)
    id_index: Arc<DashMap<Uuid, usize>>,

    /// Total indexed events
    total_events: parking_lot::RwLock<usize>,
}

impl EventIndex {
    pub fn new() -> Self {
        Self {
            entity_index: Arc::new(DashMap::new()),
            type_index: Arc::new(DashMap::new()),
            id_index: Arc::new(DashMap::new()),
            total_events: parking_lot::RwLock::new(0),
        }
    }

    /// Add an event to all relevant indices
    #[cfg_attr(feature = "hotpath", hotpath::measure)]
    pub fn index_event(
        &self,
        event_id: Uuid,
        entity_id: &str,
        event_type: &str,
        timestamp: chrono::DateTime<chrono::Utc>,
        offset: usize,
    ) -> Result<()> {
        let entry = IndexEntry {
            event_id,
            offset,
            timestamp,
        };

        // Index by entity_id
        self.entity_index
            .entry(entity_id.to_string())
            .or_default()
            .push(entry.clone());

        // Index by event_type
        self.type_index
            .entry(event_type.to_string())
            .or_default()
            .push(entry.clone());

        // Index by event_id
        self.id_index.insert(event_id, offset);

        // Increment total
        let mut total = self.total_events.write();
        *total += 1;

        Ok(())
    }

    /// Get all event offsets for an entity
    #[cfg_attr(feature = "hotpath", hotpath::measure)]
    pub fn get_by_entity(&self, entity_id: &str) -> Option<Vec<IndexEntry>> {
        self.entity_index
            .get(entity_id)
            .map(|entries| entries.clone())
    }

    /// Refuse an oversized entity before allocating its offset vector.
    pub(crate) fn get_by_entity_bounded(
        &self,
        entity_id: &str,
        max: usize,
    ) -> Result<Vec<IndexEntry>> {
        let Some(entries) = self.entity_index.get(entity_id) else {
            return Ok(Vec::new());
        };
        if entries.len() > max {
            return Err(crate::error::AllSourceError::StorageError(
                "Strict retained entity exceeds its event budget".into(),
            ));
        }
        Ok(entries.clone())
    }

    /// Get all event offsets for an event type
    #[cfg_attr(feature = "hotpath", hotpath::measure)]
    pub fn get_by_type(&self, event_type: &str) -> Option<Vec<IndexEntry>> {
        self.type_index
            .get(event_type)
            .map(|entries| entries.clone())
    }

    /// Get event offset by ID
    #[cfg_attr(feature = "hotpath", hotpath::measure)]
    pub fn get_by_id(&self, event_id: &Uuid) -> Option<usize> {
        self.id_index.get(event_id).map(|offset| *offset)
    }

    /// Get all event entries matching a type prefix (e.g., "index." matches "index.created")
    #[cfg_attr(feature = "hotpath", hotpath::measure)]
    pub fn get_by_type_prefix(&self, prefix: &str) -> Vec<IndexEntry> {
        let mut entries = Vec::new();
        for item in self.type_index.iter() {
            if item.key().starts_with(prefix) {
                entries.extend(item.value().clone());
            }
        }
        entries
    }

    /// Get all entities being tracked
    pub fn get_all_entities(&self) -> Vec<String> {
        self.entity_index.iter().map(|e| e.key().clone()).collect()
    }

    /// Get all event types
    pub fn get_all_types(&self) -> Vec<String> {
        self.type_index.iter().map(|e| e.key().clone()).collect()
    }

    /// Get statistics
    pub fn stats(&self) -> IndexStats {
        IndexStats {
            total_events: *self.total_events.read(),
            total_entities: self.entity_index.len(),
            total_event_types: self.type_index.len(),
        }
    }

    /// Clear all indices (useful for testing)
    pub fn clear(&self) {
        self.entity_index.clear();
        self.type_index.clear();
        self.id_index.clear();
        let mut total = self.total_events.write();
        *total = 0;
    }
}

impl Default for EventIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct IndexStats {
    pub total_events: usize,
    pub total_entities: usize,
    pub total_event_types: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_event() {
        let index = EventIndex::new();
        let event_id = Uuid::new_v4();
        let timestamp = chrono::Utc::now();

        index
            .index_event(event_id, "user-123", "user.created", timestamp, 0)
            .unwrap();

        assert_eq!(index.stats().total_events, 1);
        assert_eq!(index.stats().total_entities, 1);
        assert_eq!(index.stats().total_event_types, 1);
    }

    #[test]
    fn test_get_by_entity() {
        let index = EventIndex::new();
        let event_id = Uuid::new_v4();
        let timestamp = chrono::Utc::now();

        index
            .index_event(event_id, "user-123", "user.created", timestamp, 0)
            .unwrap();

        let entries = index.get_by_entity("user-123").unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].event_id, event_id);
    }

    #[test]
    fn test_get_by_type() {
        let index = EventIndex::new();
        let event_id = Uuid::new_v4();
        let timestamp = chrono::Utc::now();

        index
            .index_event(event_id, "user-123", "user.created", timestamp, 0)
            .unwrap();

        let entries = index.get_by_type("user.created").unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].event_id, event_id);
    }

    #[test]
    fn test_get_by_type_prefix() {
        let index = EventIndex::new();
        let ts = chrono::Utc::now();

        index
            .index_event(Uuid::new_v4(), "e-1", "index.created", ts, 0)
            .unwrap();
        index
            .index_event(Uuid::new_v4(), "e-2", "index.updated", ts, 1)
            .unwrap();
        index
            .index_event(Uuid::new_v4(), "e-3", "trade.created", ts, 2)
            .unwrap();

        let entries = index.get_by_type_prefix("index.");
        assert_eq!(entries.len(), 2);

        let entries = index.get_by_type_prefix("trade.");
        assert_eq!(entries.len(), 1);

        let entries = index.get_by_type_prefix("nonexistent.");
        assert_eq!(entries.len(), 0);

        // Empty prefix matches all
        let entries = index.get_by_type_prefix("");
        assert_eq!(entries.len(), 3);
    }
}
