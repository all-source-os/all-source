//! `TenantProjectionCache` — per-tenant warm [`GraphProjections`], lazily
//! hydrated from a remote Core.
//!
//! This is the load-bearing piece of the stateless hosted Prime (bead t-10f876,
//! `docs/proposals/PRIME_STATELESS_OVER_CORE.md`). A stateless service can't
//! afford to re-query and re-fold a tenant's whole event history on every
//! request, so it keeps each tenant's materialized projections warm in memory:
//!
//! - **cache miss** → build an [`HttpCore`] scoped to the tenant, query its
//!   `prime.*` events, fold them into a [`GraphProjections`] bundle, cache it.
//! - **cache hit** (within TTL) → return the warm bundle.
//! - **write** → [`TenantProjectionCache::apply`] updates the warm bundle in
//!   place so reads stay current without a re-query.
//! - **bound memory** → LRU eviction past `capacity`; entries past `ttl` are
//!   re-hydrated on next access.
//!
//! Mirrors the shape of Core's own per-tenant warm-set (lazy hydration + LRU),
//! just over HTTP instead of Parquet. Restart = empty cache, rebuilt on demand
//! from Core (the durable source of truth).
//!
//! Feature-gated behind `prime-recall`.

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use dashmap::DashMap;

use crate::{
    embedded::Query,
    error::Result,
    prime::{event_store::EventStore, http_core::HttpCore, projection_bundle::GraphProjections},
};

/// One cached tenant bundle plus its last-access instant (for LRU + TTL).
struct Entry {
    bundle: Arc<GraphProjections>,
    /// When the bundle was hydrated — drives TTL freshness.
    hydrated_at: Instant,
    /// Last time the bundle was read — drives LRU eviction.
    last_access: Mutex<Instant>,
}

/// Per-tenant cache of warm [`GraphProjections`], hydrated from a remote Core.
pub struct TenantProjectionCache {
    base_url: String,
    api_key: Option<String>,
    ttl: Duration,
    capacity: usize,
    entries: DashMap<String, Arc<Entry>>,
}

impl TenantProjectionCache {
    /// Create a cache pointed at `base_url`, holding at most `capacity` warm
    /// tenants, re-hydrating entries older than `ttl`.
    pub fn new(
        base_url: impl Into<String>,
        api_key: Option<String>,
        capacity: usize,
        ttl: Duration,
    ) -> Self {
        Self {
            base_url: base_url.into(),
            api_key,
            ttl,
            capacity: capacity.max(1),
            entries: DashMap::new(),
        }
    }

    /// Return the tenant's warm projection bundle, hydrating from Core on a
    /// miss or when the cached copy has aged past `ttl`.
    pub async fn get_or_hydrate(&self, tenant_id: &str) -> Result<Arc<GraphProjections>> {
        // Fast path: a fresh entry. Touch its last_access and return. The guard
        // is dropped before any await (we never hydrate on this path).
        if let Some(entry) = self.entries.get(tenant_id)
            && entry.hydrated_at.elapsed() < self.ttl
        {
            if let Ok(mut la) = entry.last_access.lock() {
                *la = Instant::now();
            }
            return Ok(Arc::clone(&entry.bundle));
        }

        // Miss or stale: hydrate from Core (no cache guard held across await).
        // A concurrent miss for the same tenant may hydrate twice; that's
        // correct (idempotent fold), just briefly wasteful — acceptable for v1.
        let bundle = Arc::new(self.hydrate(tenant_id).await?);
        let now = Instant::now();
        self.entries.insert(
            tenant_id.to_string(),
            Arc::new(Entry {
                bundle: Arc::clone(&bundle),
                hydrated_at: now,
                last_access: Mutex::new(now),
            }),
        );
        self.evict_if_over_capacity();
        Ok(bundle)
    }

    /// Apply a freshly-written event to a tenant's warm bundle, if present, so
    /// reads stay current without a re-query. No-op if the tenant isn't cached
    /// (it will fold the event in on next hydrate).
    pub fn apply(&self, tenant_id: &str, view: &crate::embedded::EventView) {
        if let Some(entry) = self.entries.get(tenant_id) {
            entry.bundle.apply(view);
        }
    }

    /// Drop a tenant's cached bundle (forces re-hydrate on next access).
    pub fn invalidate(&self, tenant_id: &str) {
        self.entries.remove(tenant_id);
    }

    /// Number of warm tenants currently cached.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache holds no warm tenants.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Query a tenant's `prime.*` events from Core and fold them into a bundle.
    async fn hydrate(&self, tenant_id: &str) -> Result<GraphProjections> {
        let core = HttpCore::new(
            self.base_url.clone(),
            self.api_key.clone(),
            Some(tenant_id.to_string()),
        );
        let events = core.query(Query::new().event_type_prefix("prime.")).await?;
        Ok(GraphProjections::from_event_views(&events))
    }

    /// Evict least-recently-used entries until at or under `capacity`.
    fn evict_if_over_capacity(&self) {
        while self.entries.len() > self.capacity {
            // Find the LRU key (oldest last_access). O(n) scan — fine for the
            // modest warm-tenant counts this cache is sized for.
            let lru = self
                .entries
                .iter()
                .min_by_key(|e| {
                    e.value()
                        .last_access
                        .lock()
                        .map_or_else(|_| Instant::now(), |t| *t)
                })
                .map(|e| e.key().clone());
            match lru {
                Some(key) => {
                    self.entries.remove(&key);
                }
                None => break,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prime::types::event_types;
    use serde_json::json;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path, query_param},
    };

    fn node_event(entity_id: &str, tenant: &str, name: &str) -> serde_json::Value {
        json!({
            "id": "00000000-0000-0000-0000-000000000001",
            "event_type": event_types::NODE_CREATED,
            "entity_id": entity_id,
            "tenant_id": tenant,
            "payload": { "id": entity_id, "node_type": "contact", "properties": { "name": name } },
            "metadata": null,
            "timestamp": "2026-06-07T00:00:00Z",
            "version": 1
        })
    }

    #[tokio::test]
    async fn miss_hydrates_then_hit_serves_warm_without_requery() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/events/query"))
            .and(query_param("tenant_id", "tenant-a"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({ "events": [node_event("node:contact:alice", "tenant-a", "Alice")] }),
            ))
            .expect(1) // hydrate must hit Core exactly once across both calls
            .mount(&server)
            .await;

        let cache = TenantProjectionCache::new(server.uri(), None, 8, Duration::from_mins(1));

        let b1 = cache.get_or_hydrate("tenant-a").await.unwrap();
        assert!(b1.node_state.get_node("node:contact:alice").is_some());

        // Second call is a warm hit — no second query (verified by expect(1)).
        let b2 = cache.get_or_hydrate("tenant-a").await.unwrap();
        assert!(Arc::ptr_eq(&b1, &b2));
    }

    #[tokio::test]
    async fn tenants_are_isolated() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/events/query"))
            .and(query_param("tenant_id", "tenant-a"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({ "events": [node_event("node:contact:alice", "tenant-a", "Alice")] }),
            ))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v1/events/query"))
            .and(query_param("tenant_id", "tenant-b"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({ "events": [node_event("node:contact:bob", "tenant-b", "Bob")] }),
            ))
            .mount(&server)
            .await;

        let cache = TenantProjectionCache::new(server.uri(), None, 8, Duration::from_mins(1));
        let a = cache.get_or_hydrate("tenant-a").await.unwrap();
        let b = cache.get_or_hydrate("tenant-b").await.unwrap();

        // Tenant A sees only A's node; tenant B only B's. No cross-tenant leak.
        assert!(a.node_state.get_node("node:contact:alice").is_some());
        assert!(a.node_state.get_node("node:contact:bob").is_none());
        assert!(b.node_state.get_node("node:contact:bob").is_some());
        assert!(b.node_state.get_node("node:contact:alice").is_none());
    }

    #[tokio::test]
    async fn lru_eviction_bounds_capacity() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/events/query"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "events": [] })))
            .mount(&server)
            .await;

        let cache = TenantProjectionCache::new(server.uri(), None, 2, Duration::from_mins(1));
        cache.get_or_hydrate("t1").await.unwrap();
        cache.get_or_hydrate("t2").await.unwrap();
        cache.get_or_hydrate("t3").await.unwrap();
        assert_eq!(cache.len(), 2, "cache must not exceed capacity");
    }
}
