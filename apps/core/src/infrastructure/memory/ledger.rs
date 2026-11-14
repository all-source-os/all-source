//! Counts resident heap bytes per owner. The ledger counts and never refuses,
//! reserves or evicts: a charge or release is a plain atomic add.
//!
//! Owners are charged where their bytes become resident and released where
//! they stop being resident. Owner counters change only through this API.

use crate::domain::entities::event::Event;
use dashmap::DashMap;
use std::sync::atomic::{AtomicI64, AtomicU64, AtomicUsize, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    EventsResident,
    StoragePending,
    IndexEntity,
    IndexType,
    IndexId,
    FeedIndex,
    CacheGenerations,
    TenantLoader,
    ReadModels,
    PipelineWindows,
    PipelineState,
    WebhookQueue,
    Broadcast,
    ProjectionEventCounters,
    Query,
    EntityVersions,
    VersionIndexTenants,
    VersionIndexEntities,
    GeoIndex,
    SchemaEvolution,
    ProjectionPermanent,
    Search,
    SystemStore,
    Misc,
    RateLimiter,
    Metrics,
}

impl Owner {
    pub const COUNT: usize = 26;

    pub const ALL: [Owner; Self::COUNT] = [
        Owner::EventsResident,
        Owner::StoragePending,
        Owner::IndexEntity,
        Owner::IndexType,
        Owner::IndexId,
        Owner::FeedIndex,
        Owner::CacheGenerations,
        Owner::TenantLoader,
        Owner::ReadModels,
        Owner::PipelineWindows,
        Owner::PipelineState,
        Owner::WebhookQueue,
        Owner::Broadcast,
        Owner::ProjectionEventCounters,
        Owner::Query,
        Owner::EntityVersions,
        Owner::VersionIndexTenants,
        Owner::VersionIndexEntities,
        Owner::GeoIndex,
        Owner::SchemaEvolution,
        Owner::ProjectionPermanent,
        Owner::Search,
        Owner::SystemStore,
        Owner::Misc,
        Owner::RateLimiter,
        Owner::Metrics,
    ];

    /// The `owner` label value in the metrics.
    pub const fn label(self) -> &'static str {
        match self {
            Owner::EventsResident => "events.resident",
            Owner::StoragePending => "storage.pending",
            Owner::IndexEntity => "index.entity",
            Owner::IndexType => "index.type",
            Owner::IndexId => "index.id",
            Owner::FeedIndex => "feed_index",
            Owner::CacheGenerations => "cache_generations",
            Owner::TenantLoader => "tenant_loader",
            Owner::ReadModels => "read_models",
            Owner::PipelineWindows => "pipeline.windows",
            Owner::PipelineState => "pipeline.state",
            Owner::WebhookQueue => "webhook.queue",
            Owner::Broadcast => "broadcast",
            Owner::ProjectionEventCounters => "projection.event_counters",
            Owner::Query => "query",
            Owner::EntityVersions => "entity_versions",
            Owner::VersionIndexTenants => "version_index_tenants",
            Owner::VersionIndexEntities => "version_index_entities",
            Owner::GeoIndex => "geo_index",
            Owner::SchemaEvolution => "schema_evolution",
            Owner::ProjectionPermanent => "projection.permanent",
            Owner::Search => "search",
            Owner::SystemStore => "system_store",
            Owner::Misc => "misc",
            Owner::RateLimiter => "rate_limiter",
            Owner::Metrics => "metrics",
        }
    }

    /// Owners whose bytes are never released, summed in
    /// `allsource_core_permanent_bytes`.
    pub const fn is_permanent(self) -> bool {
        matches!(
            self,
            Owner::CacheGenerations
                | Owner::EntityVersions
                | Owner::VersionIndexTenants
                | Owner::VersionIndexEntities
                | Owner::GeoIndex
                | Owner::SchemaEvolution
                | Owner::ProjectionPermanent
                | Owner::Search
                | Owner::SystemStore
                | Owner::Misc
                | Owner::Metrics
        )
    }
}

pub struct ResidentLedger {
    owners: [AtomicI64; Owner::COUNT],
    rows_decoded: AtomicU64,
}

impl ResidentLedger {
    pub const fn new() -> Self {
        Self {
            owners: [const { AtomicI64::new(0) }; Owner::COUNT],
            rows_decoded: AtomicU64::new(0),
        }
    }

    pub fn charge(&self, owner: Owner, bytes: u64) {
        self.owners[owner as usize].fetch_add(saturating_i64(bytes), Ordering::Relaxed);
    }

    pub fn release(&self, owner: Owner, bytes: u64) {
        self.owners[owner as usize].fetch_sub(saturating_i64(bytes), Ordering::Relaxed);
    }

    /// Charge the growth of a `Vec`-like table from `before` to `after` slots,
    /// or release a shrink. Each slot is `slot_bytes` wide.
    pub fn track_capacity(&self, owner: Owner, before: usize, after: usize, slot_bytes: usize) {
        if after >= before {
            self.charge(owner, saturating_u64((after - before) * slot_bytes));
        } else {
            self.release(owner, saturating_u64((before - after) * slot_bytes));
        }
    }

    pub fn record_rows_decoded(&self, rows: u64) {
        self.rows_decoded.fetch_add(rows, Ordering::Relaxed);
    }

    pub fn rows_decoded_total(&self) -> u64 {
        self.rows_decoded.load(Ordering::Relaxed)
    }

    pub fn owner_bytes(&self, owner: Owner) -> i64 {
        self.owners[owner as usize].load(Ordering::Relaxed)
    }

    pub fn total_bytes(&self) -> i64 {
        self.owners
            .iter()
            .map(|owner| owner.load(Ordering::Relaxed))
            .sum()
    }

    pub fn permanent_bytes(&self) -> i64 {
        Owner::ALL
            .iter()
            .filter(|owner| owner.is_permanent())
            .map(|owner| self.owner_bytes(*owner))
            .sum()
    }
}

impl Default for ResidentLedger {
    fn default() -> Self {
        Self::new()
    }
}

static RESIDENT: ResidentLedger = ResidentLedger::new();

/// The process-wide ledger. Heap is process-wide too, so the unattributed gauge
/// compares the same scope on both sides.
pub fn resident() -> &'static ResidentLedger {
    &RESIDENT
}

/// The bytes one structure has charged to one owner. Releases never exceed
/// what this tally holds, and dropping it releases the rest, so an owner that
/// goes away cannot leave its bytes counted.
pub struct Tally {
    owner: Owner,
    bytes: AtomicU64,
}

impl Tally {
    pub const fn new(owner: Owner) -> Self {
        Self {
            owner,
            bytes: AtomicU64::new(0),
        }
    }

    pub fn add(&self, bytes: u64) {
        if bytes == 0 {
            return;
        }
        resident().charge(self.owner, bytes);
        self.bytes.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn sub(&self, bytes: u64) {
        let held = self
            .bytes
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |held| {
                Some(held.saturating_sub(bytes))
            })
            .unwrap_or(0);
        let released = held.min(bytes);
        if released > 0 {
            resident().release(self.owner, released);
        }
    }

    /// Charge the growth of a slot table from `before` to `after` slots, or
    /// release a shrink. Each slot is `slot_bytes` wide.
    pub fn grow(&self, before: usize, after: usize, slot_bytes: usize) {
        let delta = after.abs_diff(before) as u64 * slot_bytes as u64;
        if after >= before {
            self.add(delta);
        } else {
            self.sub(delta);
        }
    }

    /// Charge or release the change of one value's heap from `before` to `after`.
    pub fn adjust(&self, before: u64, after: u64) {
        if after >= before {
            self.add(after - before);
        } else {
            self.sub(before - after);
        }
    }

    /// Move the held amount to `target`, charging or releasing the difference.
    pub fn set_to(&self, target: u64) {
        let held = self.bytes();
        if target >= held {
            self.add(target - held);
        } else {
            self.sub(held - target);
        }
    }

    /// Release the `resident_bytes()` of every event that `matches` selects.
    pub fn release_matching(&self, events: &[Event], matches: impl Fn(&Event) -> bool) {
        let released: u64 = events
            .iter()
            .filter(|event| matches(event))
            .map(Event::resident_bytes)
            .sum();
        self.sub(released);
    }

    /// Release everything held and return the released amount.
    pub fn take(&self) -> u64 {
        let held = self.bytes.swap(0, Ordering::Relaxed);
        if held > 0 {
            resident().release(self.owner, held);
        }
        held
    }

    pub fn bytes(&self) -> u64 {
        self.bytes.load(Ordering::Relaxed)
    }
}

impl Drop for Tally {
    fn drop(&mut self) {
        self.take();
    }
}

/// Charges a concurrent map's new keys: each key's owned bytes, and the growth
/// of the map's table. The table stays charged after its entries are removed,
/// because the map keeps its allocation.
pub struct MapCharge {
    owned: Tally,
    table: Tally,
    slots: AtomicUsize,
    slot_bytes: usize,
}

impl MapCharge {
    pub const fn new(owner: Owner, slot_bytes: usize) -> Self {
        Self {
            owned: Tally::new(owner),
            table: Tally::new(owner),
            slots: AtomicUsize::new(0),
            slot_bytes,
        }
    }

    /// Insert `key` with `value` if it is absent, charging a new key once.
    /// The table is re-read only after the entry guard is dropped, because
    /// reading the capacity takes every shard lock.
    pub fn ensure<K, Q, V, S>(
        &self,
        map: &DashMap<K, V, S>,
        key: &Q,
        to_key: impl FnOnce() -> K,
        value: impl FnOnce() -> V,
        owned_bytes: u64,
    ) where
        K: Eq + std::hash::Hash + std::borrow::Borrow<Q>,
        Q: Eq + std::hash::Hash + ?Sized,
        S: std::hash::BuildHasher + Clone,
    {
        if map.contains_key(key) {
            return;
        }
        let inserted = match map.entry(to_key()) {
            dashmap::mapref::entry::Entry::Vacant(vacant) => {
                vacant.insert(value());
                true
            }
            dashmap::mapref::entry::Entry::Occupied(_) => false,
        };
        if inserted {
            self.owned.add(owned_bytes);
            let now = map.capacity();
            let before = self.slots.swap(now, Ordering::Relaxed);
            self.table.grow(before, now, self.slot_bytes);
        }
    }

    pub fn release_owned(&self, bytes: u64) {
        self.owned.sub(bytes);
    }

    /// Remove `key` from `map` and release its owned bytes if it was present.
    /// The removal happens inside the call, so two removers cannot both release.
    pub fn forget<K, Q, V, S>(&self, map: &DashMap<K, V, S>, key: &Q, owned_bytes: u64)
    where
        K: Eq + std::hash::Hash + std::borrow::Borrow<Q>,
        Q: Eq + std::hash::Hash + ?Sized,
        S: std::hash::BuildHasher + Clone,
    {
        if map.remove(key).is_some() {
            self.owned.sub(owned_bytes);
        }
    }

    /// Release every owned byte, for a map cleared as a whole.
    pub fn clear_owned(&self) {
        self.owned.take();
    }

    /// Insert `value` under `key`, replacing any previous value. The entry's
    /// heap is `measure(key, value)`, and the table growth is charged too.
    pub fn insert<K, V, S>(
        &self,
        map: &DashMap<K, V, S>,
        key: K,
        value: V,
        measure: impl Fn(&K, &V) -> u64,
    ) where
        K: Eq + std::hash::Hash,
        S: std::hash::BuildHasher + Clone,
    {
        match map.entry(key) {
            dashmap::mapref::entry::Entry::Vacant(vacant) => {
                self.owned.add(measure(vacant.key(), &value));
                vacant.insert(value);
            }
            dashmap::mapref::entry::Entry::Occupied(mut occupied) => {
                let before = measure(occupied.key(), occupied.get());
                let after = measure(occupied.key(), &value);
                self.owned.adjust(before, after);
                occupied.insert(value);
            }
        }
        self.resize(map);
    }

    /// Run `update` on the entry under `key`, first inserting `fresh()` if the
    /// key is absent. The insert, its charge and `update` run under one entry
    /// guard, so a concurrent `remove_entry` either sees the entry with its
    /// bytes charged or runs before the insert. `update` may change the value's
    /// heap; the charge follows the value's `measure` before and after it.
    pub fn upsert<K, V, S, R>(
        &self,
        map: &DashMap<K, V, S>,
        key: K,
        fresh: impl FnOnce() -> V,
        measure: impl Fn(&K, &V) -> u64,
        update: impl FnOnce(&mut V) -> R,
    ) -> R
    where
        K: Eq + std::hash::Hash,
        S: std::hash::BuildHasher + Clone,
    {
        let (inserted, result) = match map.entry(key) {
            dashmap::mapref::entry::Entry::Vacant(vacant) => {
                let mut slot = vacant.insert(fresh());
                let result = update(slot.value_mut());
                self.owned.add(measure(slot.key(), slot.value()));
                (true, result)
            }
            dashmap::mapref::entry::Entry::Occupied(mut occupied) => {
                let before = measure(occupied.key(), occupied.get());
                let result = update(occupied.get_mut());
                let after = measure(occupied.key(), occupied.get());
                self.owned.adjust(before, after);
                (false, result)
            }
        };
        if inserted {
            self.resize(map);
        }
        result
    }

    /// Bytes charged to the entries' owned heap, excluding the table.
    pub fn owned_bytes(&self) -> u64 {
        self.owned.bytes()
    }

    /// Remove `key` and release the heap its entry held.
    pub fn remove_entry<K, Q, V, S>(
        &self,
        map: &DashMap<K, V, S>,
        key: &Q,
        measure: impl Fn(&K, &V) -> u64,
    ) -> Option<V>
    where
        K: Eq + std::hash::Hash + std::borrow::Borrow<Q>,
        Q: Eq + std::hash::Hash + ?Sized,
        S: std::hash::BuildHasher + Clone,
    {
        let (old_key, old_value) = map.remove(key)?;
        self.owned.sub(measure(&old_key, &old_value));
        self.resize(map);
        Some(old_value)
    }

    /// Remove `key` when `remove_when` holds for its value, releasing the entry's heap.
    pub fn remove_entry_if<K, Q, V, S>(
        &self,
        map: &DashMap<K, V, S>,
        key: &Q,
        remove_when: impl FnOnce(&V) -> bool,
        measure: impl Fn(&K, &V) -> u64,
    ) -> Option<V>
    where
        K: Eq + std::hash::Hash + std::borrow::Borrow<Q>,
        Q: Eq + std::hash::Hash + ?Sized,
        S: std::hash::BuildHasher + Clone,
    {
        let (old_key, old_value) = map.remove_if(key, |_, value| remove_when(value))?;
        self.owned.sub(measure(&old_key, &old_value));
        self.resize(map);
        Some(old_value)
    }

    /// Re-read the map's table after an insert or removal made it grow or shrink.
    pub fn resize<K, V, S>(&self, map: &DashMap<K, V, S>)
    where
        K: Eq + std::hash::Hash,
        S: std::hash::BuildHasher + Clone,
    {
        let now = map.capacity();
        let before = self.slots.swap(now, Ordering::Relaxed);
        self.table.grow(before, now, self.slot_bytes);
    }

    /// Charge or release the heap change of a value the map holds in place.
    pub fn adjust_owned(&self, before: u64, after: u64) {
        self.owned.adjust(before, after);
    }
}

/// Heap of an owned `String`: its capacity.
pub fn string_heap(text: &String) -> u64 {
    saturating_u64(text.capacity())
}

/// Heap of a `Vec<String>`: its slots, and each string's capacity.
pub fn strings_heap(items: &Vec<String>) -> u64 {
    saturating_u64(items.capacity() * std::mem::size_of::<String>())
        + items.iter().map(string_heap).sum::<u64>()
}

/// Heap of an optional `String`.
pub fn option_string_heap(text: &Option<String>) -> u64 {
    text.as_ref().map_or(0, string_heap)
}

/// Bytes charged to an owner for as long as the value holding the lease lives.
#[derive(Debug)]
pub struct Lease {
    owner: Owner,
    bytes: u64,
}

impl Lease {
    pub fn new(owner: Owner, bytes: u64) -> Self {
        resident().charge(owner, bytes);
        Self { owner, bytes }
    }

    /// Hold the `resident_bytes()` of `events` under `owner`.
    pub fn for_events(owner: Owner, events: &[Event]) -> Self {
        Self::new(owner, events.iter().map(Event::resident_bytes).sum())
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        resident().release(self.owner, self.bytes);
    }
}

/// Estimated heap held by one in-process broadcast ring: the slots a ring keeps
/// for live receivers, times the running mean `resident_bytes()` of the events
/// sent. A send returns `Ok` only when a receiver exists, so the slot count
/// rises on each `Ok` and resets when a send finds no receiver.
pub struct BroadcastRing {
    capacity: u64,
    state: parking_lot::Mutex<BroadcastState>,
}

struct BroadcastState {
    slots: u64,
    sent_bytes: u64,
    sent: u64,
    charged: Tally,
}

impl BroadcastRing {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity as u64,
            state: parking_lot::Mutex::new(BroadcastState {
                slots: 0,
                sent_bytes: 0,
                sent: 0,
                charged: Tally::new(Owner::Broadcast),
            }),
        }
    }

    /// Record one send. `delivered` is whether the send returned `Ok`.
    pub fn observe(&self, delivered: bool, event_bytes: u64) {
        let mut state = self.state.lock();
        if !delivered {
            state.slots = 0;
            state.charged.take();
            return;
        }
        state.slots = (state.slots + 1).min(self.capacity);
        state.sent_bytes += event_bytes;
        state.sent += 1;
        let mean = state.sent_bytes / state.sent;
        let value = state.slots * mean;
        let held = state.charged.bytes();
        if value >= held {
            state.charged.add(value - held);
        } else {
            state.charged.sub(held - value);
        }
    }
}

fn saturating_i64(bytes: u64) -> i64 {
    i64::try_from(bytes).unwrap_or(i64::MAX)
}

fn saturating_u64(bytes: usize) -> u64 {
    u64::try_from(bytes).unwrap_or(u64::MAX)
}
