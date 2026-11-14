use crate::{
    domain::entities::Event,
    error::Result,
    infrastructure::{
        hash_state::{FastDashMap, fast_dashmap},
        memory::ledger::{Owner, Tally},
        observability::heap::{StructureSize, hash_entry_bytes},
    },
};
use dashmap::mapref::entry::Entry;
use std::{
    mem::size_of,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use uuid::Uuid;

/// Valid-time interval of an indexed event. `from` is the effective start
/// (`valid_from`, or record time when unset); `to` is `None` when open-ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidWindow {
    pub from: chrono::DateTime<chrono::Utc>,
    pub to: Option<chrono::DateTime<chrono::Utc>>,
}

impl ValidWindow {
    pub fn of(event: &Event) -> Self {
        Self {
            from: event.effective_valid_from(),
            to: event.valid_to,
        }
    }
}

/// Event index entry
#[derive(Debug, Clone)]
pub struct IndexEntry {
    pub event_id: Uuid,
    pub offset: usize,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub valid: ValidWindow,
}

/// High-performance concurrent index for fast event lookups
pub struct EventIndex {
    /// Index by entity_id -> list of event entries
    entity_index: Arc<FastDashMap<String, Vec<IndexEntry>>>,

    /// Index by event_type -> list of event entries
    type_index: Arc<FastDashMap<String, Vec<IndexEntry>>>,

    /// Index by event_id -> offset (for direct lookups)
    id_index: Arc<FastDashMap<Uuid, usize>>,

    /// Total indexed events
    total_events: parking_lot::RwLock<usize>,

    entity_entries: Tally,
    entity_table: Tally,
    entity_slots: AtomicUsize,
    type_entries: Tally,
    type_table: Tally,
    type_slots: AtomicUsize,
    id_table: Tally,
    id_slots: AtomicUsize,
}

impl EventIndex {
    pub fn new() -> Self {
        Self {
            entity_index: Arc::new(fast_dashmap()),
            type_index: Arc::new(fast_dashmap()),
            id_index: Arc::new(fast_dashmap()),
            total_events: parking_lot::RwLock::new(0),
            entity_entries: Tally::new(Owner::IndexEntity),
            entity_table: Tally::new(Owner::IndexEntity),
            entity_slots: AtomicUsize::new(0),
            type_entries: Tally::new(Owner::IndexType),
            type_table: Tally::new(Owner::IndexType),
            type_slots: AtomicUsize::new(0),
            id_table: Tally::new(Owner::IndexId),
            id_slots: AtomicUsize::new(0),
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
        valid: ValidWindow,
        offset: usize,
    ) -> Result<()> {
        let entry = IndexEntry {
            event_id,
            offset,
            timestamp,
            valid,
        };

        let entity_new = push_entry(
            &self.entity_index,
            &self.entity_entries,
            entity_id,
            entry.clone(),
        );
        if entity_new {
            charge_table(
                &self.entity_index,
                &self.entity_table,
                &self.entity_slots,
                size_of::<(String, Vec<IndexEntry>)>(),
            );
        }

        let type_new = push_entry(&self.type_index, &self.type_entries, event_type, entry);
        if type_new {
            charge_table(
                &self.type_index,
                &self.type_table,
                &self.type_slots,
                size_of::<(String, Vec<IndexEntry>)>(),
            );
        }

        if self.id_index.insert(event_id, offset).is_none() {
            charge_table(
                &self.id_index,
                &self.id_table,
                &self.id_slots,
                size_of::<(Uuid, usize)>(),
            );
        }

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

    /// Allocated slots of one entity's offset vector, 0 when the key is absent.
    pub fn entity_capacity(&self, entity_id: &str) -> usize {
        self.entity_index
            .get(entity_id)
            .map_or(0, |entries| entries.capacity())
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

    /// Estimated resident size of each of the three maps. Walks every key, one
    /// shard lock at a time, so it belongs on a slow timer.
    pub fn resident_structures(&self) -> [StructureSize; 3] {
        let list_map = |name, map: &FastDashMap<String, Vec<IndexEntry>>| {
            let bytes: usize = map
                .iter()
                .map(|item| {
                    hash_entry_bytes::<String, Vec<IndexEntry>>()
                        + item.key().len()
                        + item.value().capacity() * size_of::<IndexEntry>()
                })
                .sum();
            StructureSize {
                name,
                entries: map.len() as u64,
                bytes: Some(bytes as u64),
            }
        };
        let ids = self.id_index.len();
        [
            list_map("entity_index", &self.entity_index),
            list_map("type_index", &self.type_index),
            StructureSize {
                name: "id_index",
                entries: ids as u64,
                bytes: Some((ids * hash_entry_bytes::<Uuid, usize>()) as u64),
            },
        ]
    }

    /// Clear all indices (useful for testing)
    ///
    /// The maps keep their tables, so only the owned key and offset-vector
    /// bytes are released. The table charge stays until the index is dropped.
    pub fn clear(&self) {
        self.entity_index.clear();
        self.entity_entries.take();
        self.type_index.clear();
        self.type_entries.take();
        self.id_index.clear();
        let mut total = self.total_events.write();
        *total = 0;
    }
}

/// Push onto `key`'s offset vector and charge the owned bytes. Returns whether
/// the key was new, which is when the caller re-reads the table.
fn push_entry(
    map: &FastDashMap<String, Vec<IndexEntry>>,
    entries: &Tally,
    key: &str,
    entry: IndexEntry,
) -> bool {
    match map.entry(key.to_string()) {
        Entry::Occupied(mut occupied) => {
            let vec = occupied.get_mut();
            let before = vec.capacity();
            vec.push(entry);
            entries.grow(before, vec.capacity(), size_of::<IndexEntry>());
            false
        }
        Entry::Vacant(vacant) => {
            let key_bytes = vacant.key().len() as u64;
            let vec = vacant.insert(vec![entry]);
            entries.add(key_bytes + (vec.capacity() * size_of::<IndexEntry>()) as u64);
            true
        }
    }
}

/// Charge the table growth since the last read. Called after a new key is
/// inserted and the entry guard is dropped, because reading the capacity takes
/// every shard lock.
fn charge_table<K: Eq + std::hash::Hash, V>(
    map: &FastDashMap<K, V>,
    table: &Tally,
    slots: &AtomicUsize,
    slot_bytes: usize,
) {
    let now = map.capacity();
    let before = slots.swap(now, Ordering::Relaxed);
    table.grow(before, now, slot_bytes);
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

    fn open(from: chrono::DateTime<chrono::Utc>) -> ValidWindow {
        ValidWindow { from, to: None }
    }

    #[test]
    fn test_index_event() {
        let index = EventIndex::new();
        let event_id = Uuid::new_v4();
        let timestamp = chrono::Utc::now();

        index
            .index_event(
                event_id,
                "user-123",
                "user.created",
                timestamp,
                open(timestamp),
                0,
            )
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
            .index_event(
                event_id,
                "user-123",
                "user.created",
                timestamp,
                open(timestamp),
                0,
            )
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
            .index_event(
                event_id,
                "user-123",
                "user.created",
                timestamp,
                open(timestamp),
                0,
            )
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
            .index_event(Uuid::new_v4(), "e-1", "index.created", ts, open(ts), 0)
            .unwrap();
        index
            .index_event(Uuid::new_v4(), "e-2", "index.updated", ts, open(ts), 1)
            .unwrap();
        index
            .index_event(Uuid::new_v4(), "e-3", "trade.created", ts, open(ts), 2)
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
