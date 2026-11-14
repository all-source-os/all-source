//! Hasher for maps on the heap replay's path.
//!
//! Default builds use `RandomState` and dashmap's default shard count, so every
//! alias here is the same type as the plain std or dashmap map. Under
//! `deterministic-hash` the hasher is fixed and DashMaps get 32 shards, so
//! iteration order, and therefore response bytes, repeat across processes and
//! hosts. That build loses hash-flooding resistance and must never be deployed.
//!
//! Lives outside `infrastructure/memory/` because the replay's source scan
//! bans `RandomState` there.

use dashmap::DashMap;
use std::collections::{HashMap, HashSet};

#[cfg(not(feature = "deterministic-hash"))]
pub type HashState = std::collections::hash_map::RandomState;

#[cfg(feature = "deterministic-hash")]
pub type HashState = std::hash::BuildHasherDefault<std::collections::hash_map::DefaultHasher>;

#[cfg(feature = "deterministic-hash")]
const DASH_SHARDS: usize = 32;

pub type FastMap<K, V> = HashMap<K, V, HashState>;
pub type FastSet<T> = HashSet<T, HashState>;
pub type FastDashMap<K, V> = DashMap<K, V, HashState>;

pub fn fast_map<K, V>() -> FastMap<K, V> {
    HashMap::with_hasher(HashState::default())
}

pub fn fast_map_with_capacity<K, V>(capacity: usize) -> FastMap<K, V> {
    HashMap::with_capacity_and_hasher(capacity, HashState::default())
}

pub fn fast_set<T>() -> FastSet<T> {
    HashSet::with_hasher(HashState::default())
}

pub fn fast_set_with_capacity<T>(capacity: usize) -> FastSet<T> {
    HashSet::with_capacity_and_hasher(capacity, HashState::default())
}

pub fn fast_dashmap<K: Eq + std::hash::Hash, V>() -> FastDashMap<K, V> {
    #[cfg(not(feature = "deterministic-hash"))]
    {
        DashMap::with_hasher(HashState::default())
    }
    #[cfg(feature = "deterministic-hash")]
    {
        DashMap::with_hasher_and_shard_amount(HashState::default(), DASH_SHARDS)
    }
}
