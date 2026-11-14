use super::EventStore;
use crate::{
    domain::entities::Event,
    infrastructure::{
        hash_state::FastDashMap,
        observability::heap::{StructureSize, hash_entry_bytes, json_heap_bytes},
    },
};
use std::mem::size_of;

/// Events walked per estimate; the rest are extrapolated from them, which
/// keeps the read lock short on a store holding hundreds of thousands.
const EVENT_SAMPLE: usize = 4096;

fn event_heap_bytes(event: &Event) -> usize {
    event.entity_id_str().len()
        + event.event_type_str().len()
        + event.tenant_id_str().len()
        + json_heap_bytes(&event.payload)
        + event.metadata.as_ref().map_or(0, json_heap_bytes)
}

fn keyed_map<V>(name: &'static str, map: &FastDashMap<String, V>) -> StructureSize {
    let bytes: usize = map
        .iter()
        .map(|item| hash_entry_bytes::<String, V>() + item.key().len())
        .sum();
    StructureSize {
        name,
        entries: map.len() as u64,
        bytes: Some(bytes as u64),
    }
}

impl EventStore {
    /// Estimated resident size of each in-memory structure, for the memory
    /// gauges. Structures without a byte estimate report entries only.
    pub fn resident_structures(&self) -> Vec<StructureSize> {
        let mut sizes = vec![self.resident_events()];
        sizes.extend(self.index.resident_structures());
        sizes.push(keyed_map("entity_versions", &self.entity_versions));
        sizes.push(keyed_map(
            "version_index_tenants",
            &self.version_index_tenants,
        ));
        sizes.push(keyed_map(
            "version_index_entities",
            &self.version_index_entities,
        ));
        sizes.push(StructureSize {
            name: "feed_indexes",
            entries: self.feed_indexes.len() as u64,
            bytes: None,
        });
        sizes.push(StructureSize {
            name: "read_models",
            entries: self.read_models.lock().len() as u64,
            bytes: None,
        });
        sizes.push(StructureSize {
            name: "geo_index",
            entries: self.geo_index.len() as u64,
            bytes: None,
        });
        sizes
    }

    fn resident_events(&self) -> StructureSize {
        let events = self.events.read();
        let count = events.len();
        let stride = (count / EVENT_SAMPLE).max(1);
        let (sampled, sampled_bytes) = events
            .iter()
            .step_by(stride)
            .fold((0usize, 0usize), |(n, bytes), event| {
                (n + 1, bytes + event_heap_bytes(event))
            });
        let heap = if sampled == 0 {
            0
        } else {
            (sampled_bytes as u128 * count as u128 / sampled as u128) as usize
        };
        StructureSize {
            name: "events",
            entries: count as u64,
            bytes: Some((events.capacity() * size_of::<Event>() + heap) as u64),
        }
    }
}
