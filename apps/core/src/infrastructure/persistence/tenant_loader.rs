//! Per-tenant lazy-load bookkeeping (Step 2 of the sustainable data
//! strategy).
//!
//! Boot used to read every Parquet file into a single shared
//! in-memory pile. Once the dataset crossed the available memory
//! cap, Core OOM'd during recovery (issue #160). Step 2 replaces
//! the boot-time full-load with on-demand per-tenant hydration:
//! the first query for tenant `T` triggers `load_events_for_tenant(T)`,
//! and subsequent queries hit the now-warm cache.
//!
//! This module owns the bookkeeping — which tenants are loaded, and a
//! per-tenant lock so two concurrent first-queries for the same
//! tenant don't double-load. The actual disk read lives in
//! `ParquetStorage::load_events_for_tenant`; the side-effect of
//! splicing loaded events into the in-memory index/projection
//! structures lives in `EventStore::ensure_tenant_loaded`.
//!
//! Step 3 will add a memory-budgeted cache that evicts cold tenants;
//! at that point the `loaded` set becomes "currently in memory"
//! rather than "ever loaded since boot", and a `mark_unloaded`
//! method shows up.

use dashmap::DashMap;
use parking_lot::Mutex;
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

/// Default ceiling on how long a query may wait for an in-flight
/// load of the same tenant before giving up. The 100k-event load
/// budget for cold tenants is single-digit seconds (per the Step 2
/// acceptance criteria); 30s is comfortable headroom for that, well
/// short of a request-timeout indistinguishable from a hang.
pub const DEFAULT_LOAD_TIMEOUT: Duration = Duration::from_secs(30);

/// Tracks which tenants have been hydrated into memory, serializes
/// concurrent first-loads of the same tenant, and accounts for the
/// approximate bytes each loaded tenant occupies (Step 3 cache
/// budget input).
///
/// `loaded` is the source of truth for "is this tenant warm?". It's
/// insert-only until Step 3 #2's `mark_unloaded` (eviction) lands.
///
/// `locks` holds one Mutex per tenant ever queried. Concurrent
/// first-queries get the same Mutex; the second waiter re-checks
/// `loaded` after acquiring it (the standard double-checked-lock
/// pattern, sound here because `loaded`'s DashMap insert
/// happens-before the lock release).
///
/// `bytes` accumulates per-tenant resident-byte estimates as
/// events get spliced in via `append_loaded_event`. It's the input
/// the budget-enforcement step (Step 3 #3) keys off — the LRU
/// eviction policy compares the sum of these counters against the
/// configured byte budget.
pub struct TenantLoader {
    // The value records whether this load verified every archive file. Tolerant
    // queries may cache partial history; that cannot authorize an OCC write.
    loaded: DashMap<String, bool>,
    locks: DashMap<String, Arc<Mutex<()>>>,
    bytes: DashMap<String, u64>,
    /// Last-used Instant per tenant — refreshed on every query
    /// via `touch`. The eviction policy (Step 3 #3) picks the
    /// tenant with the oldest entry as the LRU victim.
    last_used: DashMap<String, Instant>,
    /// Cache byte budget. 0 disables enforcement (keep every
    /// loaded tenant resident). Production reads this from the
    /// `ALLSOURCE_CACHE_BYTES` env var.
    byte_budget: AtomicU64,
    load_timeout: Duration,
}

impl TenantLoader {
    pub fn new() -> Self {
        Self::with_timeout(DEFAULT_LOAD_TIMEOUT)
    }

    pub fn with_timeout(load_timeout: Duration) -> Self {
        Self {
            loaded: DashMap::new(),
            locks: DashMap::new(),
            bytes: DashMap::new(),
            last_used: DashMap::new(),
            byte_budget: AtomicU64::new(0),
            load_timeout,
        }
    }

    /// Set the cache byte budget. 0 disables enforcement.
    pub fn set_byte_budget(&self, budget: u64) {
        self.byte_budget.store(budget, Ordering::Relaxed);
    }

    pub fn byte_budget(&self) -> u64 {
        self.byte_budget.load(Ordering::Relaxed)
    }

    /// True iff a budget is set AND total resident bytes exceed it.
    pub fn over_budget(&self) -> bool {
        let b = self.byte_budget();
        b != 0 && self.total_bytes() > b
    }

    /// Mark `tenant_id` as just-used (LRU-Y end of the order).
    /// Cheap — single DashMap insert. Called on every query that
    /// touches this tenant. If the eviction policy picks an LRU
    /// victim, the most-recently-touched tenants are last to go.
    pub fn touch(&self, tenant_id: &str) {
        self.last_used.insert(tenant_id.to_string(), Instant::now());
    }

    /// Pick the loaded tenant with the oldest `last_used` Instant,
    /// excluding `excluded` (the freshly-loaded tenant we don't
    /// want to immediately evict — would thrash). Returns None if
    /// no other tenant is loaded — the caller should accept the
    /// over-budget state in that case (a single tenant whose data
    /// alone exceeds the budget can't be evicted in favor of
    /// itself).
    ///
    /// `mark_loaded` always stamps `last_used`, so every loaded
    /// tenant has a touch timestamp by construction.
    pub fn pick_lru_excluding(&self, excluded: &str) -> Option<String> {
        let mut victim: Option<(String, Instant)> = None;
        for kv in &self.loaded {
            let tenant = kv.key();
            if tenant == excluded {
                continue;
            }
            let Some(last) = self.last_used.get(tenant).map(|v| *v) else {
                // mark_loaded guarantees a stamp; if it's missing
                // we have a state-management bug. Skip rather than
                // crash production.
                tracing::warn!(
                    tenant_id = %tenant,
                    "loaded tenant missing last_used stamp — skipping in LRU pick"
                );
                continue;
            };
            match &victim {
                None => victim = Some((tenant.clone(), last)),
                Some((_, t)) if last < *t => victim = Some((tenant.clone(), last)),
                _ => {}
            }
        }
        victim.map(|(t, _)| t)
    }

    /// Fast path probe — true if a previous call to `mark_loaded`
    /// has succeeded for this tenant.
    pub fn is_loaded(&self, tenant_id: &str) -> bool {
        self.loaded.contains_key(tenant_id)
    }

    /// Record that this tenant has been hydrated without asserting that
    /// every archive file was readable. Conditional writes must verify
    /// integrity before treating this cache as complete history.
    /// Also stamps a fresh `last_used` so the tenant immediately
    /// participates in LRU ordering and isn't picked as an
    /// eviction victim before its first explicit touch.
    pub fn mark_loaded(&self, tenant_id: &str) {
        self.mark_loaded_with_integrity(tenant_id, false);
    }

    pub(crate) fn is_complete(&self, tenant_id: &str) -> bool {
        self.loaded.get(tenant_id).is_some_and(|entry| *entry)
    }

    pub(crate) fn mark_loaded_with_integrity(&self, tenant_id: &str, complete: bool) {
        self.loaded.insert(tenant_id.to_string(), complete);
        self.last_used.insert(tenant_id.to_string(), Instant::now());
    }

    /// Reverse of `mark_loaded`: forget that this tenant is in
    /// memory and reset its byte counter to 0. The next call to
    /// `is_loaded` returns false; the next ensure-load will
    /// re-walk the tenant's subtree from disk. Called by the
    /// eviction path.
    pub fn mark_unloaded(&self, tenant_id: &str) {
        self.loaded.remove(tenant_id);
        self.bytes.remove(tenant_id);
        self.last_used.remove(tenant_id);
    }

    /// Add `n` bytes to the resident-size estimate for `tenant_id`.
    /// Called once per event spliced into memory for that tenant.
    /// The total is what the budget check compares against.
    pub fn add_bytes(&self, tenant_id: &str, n: u64) {
        *self.bytes.entry(tenant_id.to_string()).or_insert(0) += n;
    }

    /// Resident-byte estimate for a single tenant. Returns 0 for
    /// tenants that have never been loaded (or that were evicted —
    /// once eviction lands the counter resets to 0).
    pub fn bytes_for(&self, tenant_id: &str) -> u64 {
        self.bytes.get(tenant_id).map_or(0, |v| *v)
    }

    /// Sum of resident-byte estimates across every loaded tenant —
    /// the input the budget check compares against. O(loaded
    /// tenants), expected to be small.
    pub fn total_bytes(&self) -> u64 {
        self.bytes.iter().map(|kv| *kv.value()).sum()
    }

    /// Snapshot of `(tenant_id, bytes)` pairs for every tenant
    /// that has any resident bytes. Used by the eviction policy
    /// to pick a victim and by metrics endpoints.
    pub fn bytes_per_tenant(&self) -> Vec<(String, u64)> {
        self.bytes
            .iter()
            .map(|kv| (kv.key().clone(), *kv.value()))
            .collect()
    }

    /// Get-or-insert the per-tenant Mutex used for singleflight
    /// loading. The first caller for a given tenant creates the
    /// Mutex; later callers see the same instance and serialize on
    /// it. Returns an `Arc` so the caller can hold the lock guard
    /// without keeping a borrow into the DashMap.
    pub fn lock_for(&self, tenant_id: &str) -> Arc<Mutex<()>> {
        self.locks
            .entry(tenant_id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    pub fn load_timeout(&self) -> Duration {
        self.load_timeout
    }

    /// Number of tenants currently marked loaded. Diagnostic only.
    #[cfg(test)]
    pub fn loaded_count(&self) -> usize {
        self.loaded.len()
    }
}

impl Default for TenantLoader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        thread,
    };

    #[test]
    fn test_is_loaded_false_until_marked() {
        let loader = TenantLoader::new();
        assert!(!loader.is_loaded("alice"));
        loader.mark_loaded("alice");
        assert!(loader.is_loaded("alice"));
        // Other tenants stay cold — independent state per tenant.
        assert!(!loader.is_loaded("bob"));
    }

    #[test]
    fn test_mark_loaded_is_idempotent() {
        let loader = TenantLoader::new();
        loader.mark_loaded("alice");
        loader.mark_loaded("alice");
        assert_eq!(loader.loaded_count(), 1);
    }

    #[test]
    fn test_lock_for_returns_same_mutex_per_tenant() {
        // Two callers for the same tenant must get the same lock —
        // otherwise they don't serialize and the singleflight guarantee
        // is broken.
        let loader = TenantLoader::new();
        let lock_a1 = loader.lock_for("alice");
        let lock_a2 = loader.lock_for("alice");
        let lock_b = loader.lock_for("bob");

        assert!(Arc::ptr_eq(&lock_a1, &lock_a2));
        assert!(!Arc::ptr_eq(&lock_a1, &lock_b));
    }

    #[test]
    fn test_singleflight_blocks_second_caller_until_first_releases() {
        // Mimics the lazy-load path: caller 1 holds the Mutex while
        // "loading", caller 2 must wait until caller 1 releases.
        // We assert ordering by recording each thread's progress in a
        // counter.
        let loader = Arc::new(TenantLoader::new());
        let progress = Arc::new(AtomicUsize::new(0));

        let l1 = loader.clone();
        let p1 = progress.clone();
        let t1 = thread::spawn(move || {
            let lock = l1.lock_for("alice");
            let _g = lock.lock();
            // Caller 1 is now "loading" — record that we have the lock.
            p1.store(1, Ordering::SeqCst);
            thread::sleep(Duration::from_millis(80));
            p1.store(2, Ordering::SeqCst);
            l1.mark_loaded("alice");
        });

        // Make sure thread 1 has acquired the lock before thread 2
        // starts. Avoids a flaky race where t2 acquires first.
        while progress.load(Ordering::SeqCst) < 1 {
            thread::sleep(Duration::from_millis(5));
        }

        let l2 = loader.clone();
        let p2 = progress.clone();
        let t2 = thread::spawn(move || {
            let lock = l2.lock_for("alice");
            let _g = lock.lock();
            // When we get here, t1 must already have advanced to 2.
            assert_eq!(
                p2.load(Ordering::SeqCst),
                2,
                "second caller acquired lock before first finished — singleflight broken"
            );
            // And the tenant must now be marked loaded.
            assert!(l2.is_loaded("alice"));
        });

        t1.join().unwrap();
        t2.join().unwrap();
    }

    #[test]
    fn test_mark_unloaded_clears_loaded_and_bytes() {
        let loader = TenantLoader::new();
        loader.mark_loaded("alice");
        loader.add_bytes("alice", 100);
        loader.mark_loaded("bob");
        loader.add_bytes("bob", 200);

        loader.mark_unloaded("alice");

        assert!(!loader.is_loaded("alice"));
        assert_eq!(loader.bytes_for("alice"), 0);
        // Bob untouched.
        assert!(loader.is_loaded("bob"));
        assert_eq!(loader.bytes_for("bob"), 200);
        assert_eq!(loader.total_bytes(), 200);
    }

    #[test]
    fn test_bytes_default_to_zero() {
        let loader = TenantLoader::new();
        assert_eq!(loader.bytes_for("alice"), 0);
        assert_eq!(loader.total_bytes(), 0);
        assert!(loader.bytes_per_tenant().is_empty());
    }

    #[test]
    fn test_add_bytes_accumulates_per_tenant() {
        let loader = TenantLoader::new();
        loader.add_bytes("alice", 100);
        loader.add_bytes("alice", 50);
        loader.add_bytes("bob", 200);
        assert_eq!(loader.bytes_for("alice"), 150);
        assert_eq!(loader.bytes_for("bob"), 200);
        assert_eq!(loader.total_bytes(), 350);

        let mut snapshot = loader.bytes_per_tenant();
        snapshot.sort();
        assert_eq!(
            snapshot,
            vec![("alice".to_string(), 150), ("bob".to_string(), 200)]
        );
    }

    #[test]
    fn test_pick_lru_returns_oldest_excluding_target() {
        // Touch alice, then bob, then carol — alice is oldest.
        // Pick excluding "carol" (the freshly-loaded one) → alice.
        let loader = TenantLoader::new();
        loader.mark_loaded("alice");
        std::thread::sleep(std::time::Duration::from_millis(5));
        loader.mark_loaded("bob");
        std::thread::sleep(std::time::Duration::from_millis(5));
        loader.mark_loaded("carol");

        assert_eq!(
            loader.pick_lru_excluding("carol"),
            Some("alice".to_string())
        );

        // After touching alice, bob becomes oldest.
        loader.touch("alice");
        assert_eq!(loader.pick_lru_excluding("carol"), Some("bob".to_string()));
    }

    #[test]
    fn test_pick_lru_returns_none_when_only_excluded_is_loaded() {
        let loader = TenantLoader::new();
        loader.mark_loaded("alice");
        // Excluding alice from a single-tenant set → no victim.
        assert_eq!(loader.pick_lru_excluding("alice"), None);
    }

    #[test]
    fn test_pick_lru_returns_none_when_nothing_loaded() {
        let loader = TenantLoader::new();
        assert_eq!(loader.pick_lru_excluding("anyone"), None);
    }

    #[test]
    fn test_over_budget_zero_budget_means_disabled() {
        let loader = TenantLoader::new();
        loader.mark_loaded("alice");
        loader.add_bytes("alice", 1_000_000_000);
        // No budget set (default 0) → over_budget is false even
        // with massive resident bytes.
        assert!(!loader.over_budget());

        loader.set_byte_budget(100);
        assert!(loader.over_budget());
        loader.set_byte_budget(2_000_000_000);
        assert!(!loader.over_budget());
    }

    #[test]
    fn test_lock_for_distinct_tenants_does_not_serialize() {
        // Two tenants must get distinct locks — alice loading must
        // not block bob's queries.
        let loader = Arc::new(TenantLoader::new());

        let l1 = loader.clone();
        let alice_lock = l1.lock_for("alice");
        let _alice_held = alice_lock.lock();

        // bob's lock should be acquirable immediately even while
        // alice's is held. try_lock returns Some(_) on success.
        let bob_lock = loader.lock_for("bob");
        let bob_held = bob_lock.try_lock();
        assert!(
            bob_held.is_some(),
            "bob's lock should not be blocked by alice's"
        );
    }
}
