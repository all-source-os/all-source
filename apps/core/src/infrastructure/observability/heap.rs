//! Which in-memory structure holds the live heap. The server binary counts
//! live bytes at the global allocator; these estimates attribute them.

use std::mem::size_of;

/// Estimated resident size of one in-memory structure. `bytes` is `None` for
/// structures only counted, so a missing estimate never reads as zero.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructureSize {
    pub name: &'static str,
    pub entries: u64,
    pub bytes: Option<u64>,
}

/// Charged per map entry for allocator and tree-node bookkeeping.
const MAP_ENTRY_OVERHEAD: usize = 16;

/// Heap bytes owned by a JSON value beyond its inline size. String lengths
/// stand in for capacities, so this is a floor rather than an exact figure.
pub fn json_heap_bytes(value: &serde_json::Value) -> usize {
    use serde_json::Value;
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) => 0,
        Value::String(text) => text.len(),
        Value::Array(items) => {
            items.len() * size_of::<Value>() + items.iter().map(json_heap_bytes).sum::<usize>()
        }
        Value::Object(map) => map
            .iter()
            .map(|(key, item)| {
                MAP_ENTRY_OVERHEAD
                    + size_of::<String>()
                    + key.len()
                    + size_of::<Value>()
                    + json_heap_bytes(item)
            })
            .sum(),
    }
}

/// Bytes one hash-map entry costs in its table: the pair itself, scaled for
/// the 7/8 maximum load factor, plus one control byte.
pub const fn hash_entry_bytes<K, V>() -> usize {
    size_of::<(K, V)>() * 8 / 7 + 1
}

/// Live bytes as jemalloc counts them. The stats epoch is advanced first,
/// because jemalloc serves cached statistics until it is.
#[cfg(all(feature = "heap-profile", target_os = "linux"))]
pub fn jemalloc_live_bytes() -> u64 {
    tikv_jemalloc_ctl::epoch::advance().expect("jemalloc epoch");
    tikv_jemalloc_ctl::stats::allocated::read().expect("jemalloc allocated") as u64
}

/// A pprof dump of the live heap profile. Fails when the process was not
/// started with profiling active.
#[cfg(all(feature = "heap-profile", target_os = "linux"))]
pub async fn heap_profile_pprof() -> anyhow::Result<Vec<u8>> {
    let ctl = jemalloc_pprof::PROF_CTL
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("jemalloc profiling is not compiled in"))?;
    let mut ctl = ctl.lock().await;
    ctl.dump_pprof()
}
