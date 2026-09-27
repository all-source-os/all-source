use crate::{
    domain::{
        entities::{SchemaEnforcement, Tenant, TenantQuotas, TenantUsage, UsageMeter},
        repositories::{TenantRepository, deep_merge_metadata},
        value_objects::{
            TenantId,
            system_stream::{SystemDomain, system_entity_id_value, tenant_events},
        },
    },
    error::{AllSourceError, Result},
    infrastructure::persistence::SystemMetadataStore,
};
use async_trait::async_trait;
use dashmap::DashMap;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::Mutex;

mod query_usage;

/// Event-sourced TenantRepository backed by SystemMetadataStore.
///
/// Tenant lifecycle events are stored in the `_system:tenant:*` stream and
/// materialized into an in-memory DashMap cache. On startup, the cache is
/// rebuilt by replaying all tenant events from the system store.
///
/// # Cache Consistency
///
/// Uses `DashMap<String, Tenant>` for concurrent O(1) lookups by tenant ID.
/// The write path is: WAL fsync → cache update (via `emit_event`), so no
/// acknowledged writes are lost even on crash. On startup, the cache is
/// rebuilt by replaying all tenant events from the system store.
///
/// # Event Types
///
/// - `_system.tenant.created` — Tenant creation with name, quotas, metadata
/// - `_system.tenant.updated` — Name, description, or metadata changes
/// - `_system.tenant.suspended` — Tenant deactivation
/// - `_system.tenant.reactivated` — Tenant reactivation
/// - `_system.tenant.deleted` — Tenant removal
/// - `_system.tenant.quota_updated` — Quota changes
/// - `_system.tenant.usage_updated` — Usage counter updates
pub struct EventSourcedTenantRepository {
    /// Durable storage for system events
    system_store: Arc<SystemMetadataStore>,

    /// In-memory cache of materialized tenant state
    cache: Arc<DashMap<String, Tenant>>,

    /// Per-tenant async locks serializing the read-modify-write of forward
    /// usage increments. Usage is money-adjacent (quota / overage), so a
    /// lost-update race would silently under-bill. Each tenant gets its own
    /// `Mutex` (created lazily) so increments for different tenants stay fully
    /// concurrent while increments for the *same* tenant are serialized — the
    /// counter after N concurrent increments equals the exact sum. See
    /// `increment_usage`.
    usage_locks: Arc<DashMap<String, Arc<Mutex<()>>>>,
    query_usage: DashMap<String, query_usage::QueryUsageState>,
}

impl EventSourcedTenantRepository {
    /// Create a new event-sourced tenant repository.
    ///
    /// Replays all existing tenant events from the system store to rebuild
    /// the in-memory cache.
    pub fn new(system_store: Arc<SystemMetadataStore>) -> Self {
        let cache = Arc::new(DashMap::new());
        let repo = Self {
            system_store,
            cache,
            usage_locks: Arc::new(DashMap::new()),
            query_usage: DashMap::new(),
        };
        repo.rebuild_cache();
        repo
    }

    /// Fetch (or lazily create) the per-tenant usage lock.
    ///
    /// Returns an owned `Arc<Mutex<()>>` so the caller can `.lock().await`
    /// without holding a `DashMap` guard across the await point (holding a
    /// shard guard across `.await` would risk deadlock).
    fn usage_lock_for(&self, tenant_id: &str) -> Arc<Mutex<()>> {
        self.usage_locks
            .entry(tenant_id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .value()
            .clone()
    }

    /// Rebuild the in-memory cache from all tenant events in the system store.
    fn rebuild_cache(&self) {
        let events = self.system_store.read_stream(SystemDomain::Tenant);
        for event in events {
            self.apply_event(&event);
        }
        tracing::info!(
            "Rebuilt tenant cache: {} tenants from system store",
            self.cache.len()
        );
    }

    /// Apply a single event to the in-memory cache.
    fn apply_event(&self, event: &crate::domain::entities::Event) {
        let event_type = event.event_type_str();
        let payload = event.payload();

        // Extract tenant_id from entity_id: _system:tenant:<tenant_id>
        let Some(tenant_id_str) = event.entity_id_str().strip_prefix("_system:tenant:") else {
            tracing::warn!(
                "Unexpected entity_id in tenant stream: {}",
                event.entity_id_str()
            );
            return;
        };

        match event_type {
            t if t == tenant_events::CREATED => {
                let name = payload["name"].as_str().unwrap_or("").to_string();
                let description = payload["description"]
                    .as_str()
                    .map(std::string::ToString::to_string);
                let quotas: TenantQuotas = payload
                    .get("quotas")
                    .and_then(|q| serde_json::from_value(q.clone()).ok())
                    .unwrap_or_default();
                let metadata = payload
                    .get("metadata")
                    .cloned()
                    .unwrap_or(serde_json::json!({}));

                if let Ok(tid) = TenantId::new(tenant_id_str.to_string()) {
                    let is_demo = payload["is_demo"].as_bool().unwrap_or(false);
                    let tenant = Tenant::reconstruct(
                        tid,
                        name,
                        description,
                        quotas,
                        TenantUsage::new(),
                        event.timestamp(),
                        event.timestamp(),
                        true,
                        is_demo,
                        metadata,
                    );
                    self.cache.insert(tenant_id_str.to_string(), tenant);
                }
            }
            t if t == tenant_events::UPDATED => {
                if let Some(mut entry) = self.cache.get_mut(tenant_id_str) {
                    let tenant = entry.value_mut();
                    if let Some(name) = payload["name"].as_str() {
                        let _ = tenant.update_name(name.to_string());
                    }
                    if payload.get("description").is_some() {
                        let desc = payload["description"]
                            .as_str()
                            .map(std::string::ToString::to_string);
                        tenant.update_description(desc);
                    }
                    if let Some(is_demo) = payload["is_demo"].as_bool() {
                        tenant.set_is_demo(is_demo);
                    }
                    if let Some(metadata) = payload.get("metadata") {
                        tenant.update_metadata(metadata.clone());
                    }
                }
                if let Some(receipt) = payload.get(query_usage::ADMITTED) {
                    self.apply_query_usage(query_usage::ADMITTED, tenant_id_str, receipt);
                } else if let Some(receipt) = payload.get(query_usage::RESET) {
                    self.apply_query_usage(query_usage::RESET, tenant_id_str, receipt);
                }
            }
            t if t == tenant_events::SUSPENDED => {
                if let Some(mut entry) = self.cache.get_mut(tenant_id_str) {
                    entry.value_mut().deactivate();
                }
            }
            t if t == tenant_events::REACTIVATED => {
                if let Some(mut entry) = self.cache.get_mut(tenant_id_str) {
                    entry.value_mut().activate();
                }
            }
            t if t == tenant_events::DELETED => {
                self.cache.remove(tenant_id_str);
                self.query_usage.remove(tenant_id_str);
            }
            t if t == tenant_events::QUOTA_UPDATED => {
                if let Some(mut entry) = self.cache.get_mut(tenant_id_str)
                    && let Ok(quotas) = serde_json::from_value::<TenantQuotas>(payload.clone())
                {
                    entry.value_mut().update_quotas(quotas);
                }
            }
            t if t == tenant_events::SCHEMA_ENFORCEMENT_UPDATED => {
                if let Some(mut entry) = self.cache.get_mut(tenant_id_str)
                    && let Ok(mode) = serde_json::from_value::<SchemaEnforcement>(payload.clone())
                {
                    entry.value_mut().set_schema_enforcement(mode);
                }
            }
            t if t == tenant_events::USAGE_UPDATED => {
                // During cache rebuild (replay), we just skip usage updates.
                // Usage is volatile and resets on restart. The event is preserved
                // in the WAL for audit purposes but doesn't need to be replayed.
            }
            _ => {
                tracing::debug!("Unknown tenant event type: {}", event_type);
            }
        }
    }

    /// Emit an event to the system store and apply it to the cache.
    fn emit_event(
        &self,
        event_type_str: &str,
        tenant_id_str: &str,
        payload: serde_json::Value,
    ) -> Result<()> {
        let entity_id = system_entity_id_value(SystemDomain::Tenant, tenant_id_str);
        let event =
            self.system_store
                .append_system_event(event_type_str, entity_id, payload, None)?;
        self.apply_event(&event);
        Ok(())
    }
}

#[async_trait]
impl TenantRepository for EventSourcedTenantRepository {
    async fn create(&self, id: TenantId, name: String, quotas: TenantQuotas) -> Result<Tenant> {
        self.create_initialized(Tenant::new(id, name, quotas)?)
            .await
    }

    async fn create_initialized(&self, tenant: Tenant) -> Result<Tenant> {
        let id_str = tenant.id().as_str().to_string();
        let lock = self.usage_lock_for(&id_str);
        let _guard = lock.lock().await;

        // Check for duplicates
        if self.cache.contains_key(&id_str) {
            return Err(AllSourceError::TenantAlreadyExists(id_str));
        }

        // The first WAL event includes the full initialization. A failed or
        // concurrent signup cannot expose an unstamped trial or reset billing.
        let payload = json!({
            "name": tenant.name(),
            "description": tenant.description(),
            "is_demo": tenant.is_demo(),
            "quotas": tenant.quotas(),
            "metadata": tenant.metadata(),
        });
        self.emit_event(tenant_events::CREATED, &id_str, payload)?;

        // Return the cached tenant (built by apply_event)
        self.cache
            .get(&id_str)
            .map(|entry| entry.value().clone())
            .ok_or_else(|| {
                AllSourceError::InternalError("Tenant not in cache after creation".to_string())
            })
    }

    async fn save(&self, tenant: &Tenant) -> Result<()> {
        let id_str = tenant.id().as_str().to_string();
        let lock = self.usage_lock_for(&id_str);
        let _guard = lock.lock().await;
        let mut metadata = tenant.metadata().clone();
        self.preserve_query_meter(&id_str, &mut metadata)?;

        if self.cache.contains_key(&id_str) {
            // Update existing
            let payload = json!({
                "name": tenant.name(),
                "description": tenant.description(),
                "is_demo": tenant.is_demo(),
                "metadata": metadata,
            });
            self.emit_event(tenant_events::UPDATED, &id_str, payload)?;

            // Also update quotas if changed
            let quotas_changed = self
                .cache
                .get(&id_str)
                .is_some_and(|cached| cached.quotas() != tenant.quotas());
            if quotas_changed {
                let quota_payload = serde_json::to_value(tenant.quotas()).unwrap_or_default();
                self.emit_event(tenant_events::QUOTA_UPDATED, &id_str, quota_payload)?;
            }

            // Sync active status
            let is_cached_active = self.cache.get(&id_str).map_or(true, |t| t.is_active());
            if tenant.is_active() != is_cached_active && tenant.is_active() {
                self.emit_event(tenant_events::REACTIVATED, &id_str, json!({}))?;
            } else if tenant.is_active() != is_cached_active {
                self.emit_event(tenant_events::SUSPENDED, &id_str, json!({}))?;
            }
        } else {
            // Create new
            let payload = json!({
                "name": tenant.name(),
                "description": tenant.description(),
                "quotas": serde_json::to_value(tenant.quotas()).unwrap_or_default(),
                "metadata": tenant.metadata(),
            });
            self.emit_event(tenant_events::CREATED, &id_str, payload)?;
        }

        Ok(())
    }

    async fn find_by_id(&self, id: &TenantId) -> Result<Option<Tenant>> {
        Ok(self
            .cache
            .get(id.as_str())
            .map(|entry| entry.value().clone()))
    }

    async fn find_by_name(&self, name: &str) -> Result<Option<Tenant>> {
        let name_lower = name.to_lowercase();
        Ok(self
            .cache
            .iter()
            .find(|entry| entry.value().name().to_lowercase() == name_lower)
            .map(|entry| entry.value().clone()))
    }

    async fn find_all(&self, limit: usize, offset: usize) -> Result<Vec<Tenant>> {
        let mut tenants: Vec<Tenant> = self.cache.iter().map(|e| e.value().clone()).collect();
        tenants.sort_by_key(|t| std::cmp::Reverse(t.created_at()));
        Ok(tenants.into_iter().skip(offset).take(limit).collect())
    }

    async fn find_active(&self, limit: usize, offset: usize) -> Result<Vec<Tenant>> {
        let mut tenants: Vec<Tenant> = self
            .cache
            .iter()
            .filter(|e| e.value().is_active())
            .map(|e| e.value().clone())
            .collect();
        tenants.sort_by_key(|t| std::cmp::Reverse(t.created_at()));
        Ok(tenants.into_iter().skip(offset).take(limit).collect())
    }

    async fn count(&self) -> Result<usize> {
        Ok(self.cache.len())
    }

    async fn count_active(&self) -> Result<usize> {
        Ok(self.cache.iter().filter(|e| e.value().is_active()).count())
    }

    async fn delete(&self, id: &TenantId) -> Result<bool> {
        let id_str = id.as_str();
        let lock = self.usage_lock_for(id_str);
        let _guard = lock.lock().await;
        if !self.cache.contains_key(id_str) {
            return Ok(false);
        }
        self.emit_event(tenant_events::DELETED, id_str, json!({}))?;
        Ok(true)
    }

    async fn update_quotas(&self, id: &TenantId, quotas: TenantQuotas) -> Result<bool> {
        let id_str = id.as_str();
        let lock = self.usage_lock_for(id_str);
        let _guard = lock.lock().await;
        if !self.cache.contains_key(id_str) {
            return Ok(false);
        }
        let payload = serde_json::to_value(&quotas).unwrap_or_default();
        self.emit_event(tenant_events::QUOTA_UPDATED, id_str, payload)?;
        // Apply directly to cache since apply_event handles deserialization
        if let Some(mut entry) = self.cache.get_mut(id_str) {
            entry.value_mut().update_quotas(quotas);
        }
        Ok(true)
    }

    async fn update_schema_enforcement(
        &self,
        id: &TenantId,
        mode: SchemaEnforcement,
    ) -> Result<bool> {
        let id_str = id.as_str();
        let lock = self.usage_lock_for(id_str);
        let _guard = lock.lock().await;
        if !self.cache.contains_key(id_str) {
            return Ok(false);
        }
        let payload = serde_json::to_value(mode).unwrap_or_default();
        self.emit_event(tenant_events::SCHEMA_ENFORCEMENT_UPDATED, id_str, payload)?;
        if let Some(mut entry) = self.cache.get_mut(id_str) {
            entry.value_mut().set_schema_enforcement(mode);
        }
        Ok(true)
    }

    async fn update_usage(&self, id: &TenantId, usage: TenantUsage) -> Result<bool> {
        let id_str = id.as_str();
        let lock = self.usage_lock_for(id_str);
        let _guard = lock.lock().await;
        if !self.cache.contains_key(id_str) {
            return Ok(false);
        }
        // Usage updates are high-frequency. We update the cache directly
        // and periodically flush to the system store. For now, just update cache.
        // The usage will be persisted via the save() method when needed.
        if let Some(mut entry) = self.cache.get_mut(id_str) {
            // We can't directly set usage, but we can use usage_mut()
            // to get a mutable reference and update fields
            let tenant_usage = entry.value_mut().usage_mut();
            // Since TenantUsage doesn't have a direct "set" method,
            // we need to reconstruct the tenant with the new usage
            drop(entry);

            // Get current tenant, reconstruct with new usage
            if let Some(current) = self.cache.get(id_str) {
                let tenant = current.value();
                let updated = Tenant::reconstruct(
                    tenant.id().clone(),
                    tenant.name().to_string(),
                    tenant.description().map(std::string::ToString::to_string),
                    tenant.quotas().clone(),
                    usage,
                    tenant.created_at(),
                    chrono::Utc::now(),
                    tenant.is_active(),
                    tenant.is_demo(),
                    tenant.metadata().clone(),
                );
                drop(current);
                self.cache.insert(id_str.to_string(), updated);
            }
        }
        Ok(true)
    }

    async fn increment_usage(
        &self,
        id: &TenantId,
        meter: UsageMeter,
        count: u64,
    ) -> Result<Option<u64>> {
        let id_str = id.as_str();

        // Concurrency: serialize the whole read-modify-write per tenant. Two
        // batched increments racing on a bare cache read would both observe
        // the same base value and one write would clobber the other (lost
        // update → under-billing). Holding this tenant's lock across the
        // read, the in-place metadata bump, and the durable `emit_event`
        // guarantees the next increment sees our committed value, so N
        // concurrent `+k` increments sum exactly. Different tenants use
        // different locks and never block each other.
        let lock = self.usage_lock_for(id_str);
        let _guard = lock.lock().await;

        // Read current metadata from the cache (under the lock).
        let Some(mut metadata) = self
            .cache
            .get(id_str)
            .map(|entry| entry.value().metadata().clone())
        else {
            return Ok(None);
        };

        let field = meter.quota_field();

        // Bump metadata.quotas.<field>, creating the objects/fields as needed
        // and preserving every sibling key (subscription, *_quota, the other
        // meter, …). Counters are non-negative; we read the prior value as u64
        // (JSON numbers come back as f64/u64 depending on source) and saturate
        // on overflow rather than panic.
        let obj = metadata.as_object_mut().ok_or_else(|| {
            AllSourceError::InternalError("tenant metadata is not an object".into())
        })?;
        let quotas = obj
            .entry("quotas")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or_else(|| {
                AllSourceError::InternalError("tenant metadata.quotas is not an object".into())
            })?;

        let prev = quotas
            .get(field)
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let next = prev.saturating_add(count);
        quotas.insert(field.to_string(), json!(next));

        // Persist durably. The UPDATED event carries the full metadata and is
        // replayed on restart (unlike USAGE_UPDATED), so the dashboard counter
        // survives a reboot. We mirror save()'s UPDATED payload shape so
        // replay doesn't drop name/description/is_demo.
        let (name, description, is_demo) = self
            .cache
            .get(id_str)
            .map(|e| {
                (
                    e.name().to_string(),
                    e.description().map(std::string::ToString::to_string),
                    e.is_demo(),
                )
            })
            .ok_or_else(|| AllSourceError::TenantNotFound(id_str.to_string()))?;

        let payload = json!({
            "name": name,
            "description": description,
            "is_demo": is_demo,
            "metadata": metadata,
        });
        self.emit_event(tenant_events::UPDATED, id_str, payload)?;

        Ok(Some(next))
    }

    async fn merge_metadata(
        &self,
        id: &TenantId,
        partial: serde_json::Value,
    ) -> Result<Option<serde_json::Value>> {
        let id_str = id.as_str();

        // Share increment_usage's per-tenant lock. A metadata write (e.g. a
        // tenant enabling a projection) and a concurrent quota bump both
        // read-modify-write the same metadata blob; without serialization one
        // would clobber the other's sibling keys (dropping events_used =
        // under-billing, or dropping the enabled set). Different tenants use
        // different locks and never block each other.
        let lock = self.usage_lock_for(id_str);
        let _guard = lock.lock().await;

        // Read current metadata from the cache (under the lock).
        let Some(mut metadata) = self
            .cache
            .get(id_str)
            .map(|entry| entry.value().metadata().clone())
        else {
            return Ok(None);
        };

        deep_merge_metadata(&mut metadata, partial);
        self.preserve_query_meter(id_str, &mut metadata)?;

        // Mirror save()/increment_usage()'s UPDATED payload so replay keeps
        // name/description/is_demo alongside the merged metadata.
        let (name, description, is_demo) = self
            .cache
            .get(id_str)
            .map(|e| {
                (
                    e.name().to_string(),
                    e.description().map(std::string::ToString::to_string),
                    e.is_demo(),
                )
            })
            .ok_or_else(|| AllSourceError::TenantNotFound(id_str.to_string()))?;

        let payload = json!({
            "name": name,
            "description": description,
            "is_demo": is_demo,
            "metadata": metadata,
        });
        self.emit_event(tenant_events::UPDATED, id_str, payload)?;

        Ok(Some(metadata))
    }

    async fn activate(&self, id: &TenantId) -> Result<bool> {
        let id_str = id.as_str();
        let lock = self.usage_lock_for(id_str);
        let _guard = lock.lock().await;
        if !self.cache.contains_key(id_str) {
            return Ok(false);
        }
        self.emit_event(tenant_events::REACTIVATED, id_str, json!({}))?;
        Ok(true)
    }

    async fn deactivate(&self, id: &TenantId) -> Result<bool> {
        let id_str = id.as_str();
        let lock = self.usage_lock_for(id_str);
        let _guard = lock.lock().await;
        if !self.cache.contains_key(id_str) {
            return Ok(false);
        }
        self.emit_event(tenant_events::SUSPENDED, id_str, json!({}))?;
        Ok(true)
    }

    async fn admit_query_usage(
        &self,
        id: &TenantId,
        request: crate::domain::entities::query_usage::QueryUsageRequest,
    ) -> Result<Option<crate::domain::entities::query_usage::QueryUsageDecision>> {
        self.admit_query(id, request).await
    }

    async fn reset_query_usage(
        &self,
        id: &TenantId,
        request: crate::domain::entities::query_usage::QueryUsageReset,
    ) -> Result<Option<crate::domain::entities::query_usage::QueryUsageResetDecision>> {
        self.reset_queries(id, request).await
    }

    async fn get_query_usage(
        &self,
        id: &TenantId,
    ) -> Result<Option<crate::domain::entities::query_usage::QueryUsageSnapshot>> {
        self.query_usage_snapshot(id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::persistence::SystemMetadataStore;
    use tempfile::TempDir;

    fn create_test_repo() -> (EventSourcedTenantRepository, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let system_store =
            Arc::new(SystemMetadataStore::new(temp_dir.path().join("__system")).unwrap());
        let repo = EventSourcedTenantRepository::new(system_store);
        (repo, temp_dir)
    }

    #[tokio::test]
    async fn test_create_tenant() {
        let (repo, _dir) = create_test_repo();

        let id = TenantId::new("acme-corp".to_string()).unwrap();
        let tenant = repo
            .create(
                id.clone(),
                "ACME Corp".to_string(),
                TenantQuotas::standard(),
            )
            .await
            .unwrap();

        assert_eq!(tenant.name(), "ACME Corp");
        assert!(tenant.is_active());
    }

    #[tokio::test]
    async fn test_create_duplicate_tenant() {
        let (repo, _dir) = create_test_repo();

        let id = TenantId::new("acme".to_string()).unwrap();
        repo.create(id.clone(), "ACME".to_string(), TenantQuotas::standard())
            .await
            .unwrap();

        let result = repo
            .create(id, "ACME Again".to_string(), TenantQuotas::standard())
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_find_by_id() {
        let (repo, _dir) = create_test_repo();

        let id = TenantId::new("acme".to_string()).unwrap();
        repo.create(id.clone(), "ACME".to_string(), TenantQuotas::standard())
            .await
            .unwrap();

        let found = repo.find_by_id(&id).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name(), "ACME");

        let not_found = TenantId::new("nonexistent".to_string()).unwrap();
        assert!(repo.find_by_id(&not_found).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_find_by_name() {
        let (repo, _dir) = create_test_repo();

        let id = TenantId::new("acme".to_string()).unwrap();
        repo.create(id, "ACME Corp".to_string(), TenantQuotas::standard())
            .await
            .unwrap();

        let found = repo.find_by_name("acme corp").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name(), "ACME Corp");
    }

    #[tokio::test]
    async fn test_find_all() {
        let (repo, _dir) = create_test_repo();

        for i in 0..5 {
            let id = TenantId::new(format!("tenant-{i}")).unwrap();
            repo.create(id, format!("Tenant {i}"), TenantQuotas::standard())
                .await
                .unwrap();
        }

        let all = repo.find_all(10, 0).await.unwrap();
        assert_eq!(all.len(), 5);

        let page = repo.find_all(2, 2).await.unwrap();
        assert_eq!(page.len(), 2);
    }

    #[tokio::test]
    async fn test_activate_deactivate() {
        let (repo, _dir) = create_test_repo();

        let id = TenantId::new("acme".to_string()).unwrap();
        repo.create(id.clone(), "ACME".to_string(), TenantQuotas::standard())
            .await
            .unwrap();

        // Deactivate
        assert!(repo.deactivate(&id).await.unwrap());
        let tenant = repo.find_by_id(&id).await.unwrap().unwrap();
        assert!(!tenant.is_active());

        // Reactivate
        assert!(repo.activate(&id).await.unwrap());
        let tenant = repo.find_by_id(&id).await.unwrap().unwrap();
        assert!(tenant.is_active());
    }

    #[tokio::test]
    async fn test_delete_tenant() {
        let (repo, _dir) = create_test_repo();

        let id = TenantId::new("acme".to_string()).unwrap();
        repo.create(id.clone(), "ACME".to_string(), TenantQuotas::standard())
            .await
            .unwrap();

        assert!(repo.delete(&id).await.unwrap());
        assert!(repo.find_by_id(&id).await.unwrap().is_none());
        assert_eq!(repo.count().await.unwrap(), 0);

        // Delete non-existent returns false
        assert!(!repo.delete(&id).await.unwrap());
    }

    #[tokio::test]
    async fn test_count() {
        let (repo, _dir) = create_test_repo();

        assert_eq!(repo.count().await.unwrap(), 0);
        assert_eq!(repo.count_active().await.unwrap(), 0);

        let id1 = TenantId::new("t1".to_string()).unwrap();
        let id2 = TenantId::new("t2".to_string()).unwrap();
        repo.create(id1.clone(), "T1".to_string(), TenantQuotas::standard())
            .await
            .unwrap();
        repo.create(id2.clone(), "T2".to_string(), TenantQuotas::standard())
            .await
            .unwrap();

        assert_eq!(repo.count().await.unwrap(), 2);
        assert_eq!(repo.count_active().await.unwrap(), 2);

        repo.deactivate(&id1).await.unwrap();
        assert_eq!(repo.count().await.unwrap(), 2);
        assert_eq!(repo.count_active().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_update_quotas() {
        let (repo, _dir) = create_test_repo();

        let id = TenantId::new("acme".to_string()).unwrap();
        repo.create(id.clone(), "ACME".to_string(), TenantQuotas::free_tier())
            .await
            .unwrap();

        let new_quotas = TenantQuotas::professional();
        assert!(repo.update_quotas(&id, new_quotas.clone()).await.unwrap());

        let tenant = repo.find_by_id(&id).await.unwrap().unwrap();
        assert_eq!(tenant.quotas(), &new_quotas);

        // Non-existent tenant
        let bad_id = TenantId::new("nonexistent".to_string()).unwrap();
        assert!(
            !repo
                .update_quotas(&bad_id, TenantQuotas::standard())
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn test_cache_rebuild_on_restart() {
        let temp_dir = TempDir::new().unwrap();
        let system_dir = temp_dir.path().join("__system");

        // Create tenants
        {
            let system_store = Arc::new(SystemMetadataStore::new(&system_dir).unwrap());
            let repo = EventSourcedTenantRepository::new(system_store);

            repo.create(
                TenantId::new("acme".to_string()).unwrap(),
                "ACME Corp".to_string(),
                TenantQuotas::standard(),
            )
            .await
            .unwrap();

            repo.create(
                TenantId::new("beta".to_string()).unwrap(),
                "Beta Inc".to_string(),
                TenantQuotas::free_tier(),
            )
            .await
            .unwrap();

            // Deactivate one
            repo.deactivate(&TenantId::new("beta".to_string()).unwrap())
                .await
                .unwrap();
        }
        // Dropped — simulates restart

        // Rebuild from WAL
        {
            let system_store = Arc::new(SystemMetadataStore::new(&system_dir).unwrap());
            let repo = EventSourcedTenantRepository::new(system_store);

            assert_eq!(repo.count().await.unwrap(), 2);

            let acme = repo
                .find_by_id(&TenantId::new("acme".to_string()).unwrap())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(acme.name(), "ACME Corp");
            assert!(acme.is_active());

            let beta = repo
                .find_by_id(&TenantId::new("beta".to_string()).unwrap())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(beta.name(), "Beta Inc");
            assert!(!beta.is_active());
        }
    }

    #[tokio::test]
    async fn test_schema_enforcement_survives_restart() {
        use crate::domain::entities::SchemaEnforcement;

        let temp_dir = TempDir::new().unwrap();
        let system_dir = temp_dir.path().join("__system");
        let tid = TenantId::new("acme".to_string()).unwrap();

        // Set Strict, then drop the repo (simulates restart).
        {
            let system_store = Arc::new(SystemMetadataStore::new(&system_dir).unwrap());
            let repo = EventSourcedTenantRepository::new(system_store);
            repo.create(tid.clone(), "ACME".to_string(), TenantQuotas::standard())
                .await
                .unwrap();
            // Default is Permissive.
            assert_eq!(
                repo.find_by_id(&tid)
                    .await
                    .unwrap()
                    .unwrap()
                    .schema_enforcement(),
                SchemaEnforcement::Permissive
            );
            assert!(
                repo.update_schema_enforcement(&tid, SchemaEnforcement::Strict)
                    .await
                    .unwrap()
            );
            assert_eq!(
                repo.find_by_id(&tid)
                    .await
                    .unwrap()
                    .unwrap()
                    .schema_enforcement(),
                SchemaEnforcement::Strict
            );
        }

        // Rebuild from WAL — the SCHEMA_ENFORCEMENT_UPDATED event must replay.
        {
            let system_store = Arc::new(SystemMetadataStore::new(&system_dir).unwrap());
            let repo = EventSourcedTenantRepository::new(system_store);
            assert_eq!(
                repo.find_by_id(&tid)
                    .await
                    .unwrap()
                    .unwrap()
                    .schema_enforcement(),
                SchemaEnforcement::Strict,
                "schema enforcement mode must survive a restart"
            );
        }
    }

    #[tokio::test]
    async fn test_update_schema_enforcement_unknown_tenant_returns_false() {
        use crate::domain::entities::SchemaEnforcement;
        let (repo, _dir) = create_test_repo();
        let bad_id = TenantId::new("ghost".to_string()).unwrap();
        assert!(
            !repo
                .update_schema_enforcement(&bad_id, SchemaEnforcement::Strict)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn test_find_active() {
        let (repo, _dir) = create_test_repo();

        let id1 = TenantId::new("active1".to_string()).unwrap();
        let id2 = TenantId::new("inactive1".to_string()).unwrap();
        let id3 = TenantId::new("active2".to_string()).unwrap();

        repo.create(id1, "Active 1".to_string(), TenantQuotas::standard())
            .await
            .unwrap();
        repo.create(
            id2.clone(),
            "Inactive 1".to_string(),
            TenantQuotas::standard(),
        )
        .await
        .unwrap();
        repo.create(id3, "Active 2".to_string(), TenantQuotas::standard())
            .await
            .unwrap();

        repo.deactivate(&id2).await.unwrap();

        let active = repo.find_active(10, 0).await.unwrap();
        assert_eq!(active.len(), 2);
        assert!(
            active
                .iter()
                .all(crate::domain::entities::tenant::Tenant::is_active)
        );
    }

    #[tokio::test]
    async fn test_save_new_tenant() {
        let (repo, _dir) = create_test_repo();

        let id = TenantId::new("new-tenant".to_string()).unwrap();
        let tenant = Tenant::new(
            id.clone(),
            "New Tenant".to_string(),
            TenantQuotas::standard(),
        )
        .unwrap();

        repo.save(&tenant).await.unwrap();

        let found = repo.find_by_id(&id).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name(), "New Tenant");
    }

    // ---- forward usage-metering (increment_usage) ----

    /// Read `metadata.quotas.<field>` from the cached tenant, defaulting to 0.
    /// This is the exact path the dashboard / Control-Plane read.
    async fn quota_counter(repo: &EventSourcedTenantRepository, id: &TenantId, field: &str) -> u64 {
        repo.find_by_id(id)
            .await
            .unwrap()
            .unwrap()
            .metadata()
            .get("quotas")
            .and_then(|q| q.get(field))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0)
    }

    #[tokio::test]
    async fn increment_usage_bumps_events_used() {
        let (repo, _dir) = create_test_repo();
        let id = TenantId::new("acme".to_string()).unwrap();
        repo.create(id.clone(), "ACME".to_string(), TenantQuotas::standard())
            .await
            .unwrap();

        // Starts from nothing (no quotas object at all).
        assert_eq!(quota_counter(&repo, &id, "events_used").await, 0);

        let v = repo
            .increment_usage(&id, UsageMeter::Events, 5)
            .await
            .unwrap();
        assert_eq!(v, Some(5));
        assert_eq!(quota_counter(&repo, &id, "events_used").await, 5);

        let v = repo
            .increment_usage(&id, UsageMeter::Events, 3)
            .await
            .unwrap();
        assert_eq!(v, Some(8));
        assert_eq!(quota_counter(&repo, &id, "events_used").await, 8);
    }

    #[tokio::test]
    async fn increment_usage_separates_events_and_queries() {
        // The whole point of the typed contract: the two meters must not
        // conflate. Bump each independently and assert they don't bleed.
        let (repo, _dir) = create_test_repo();
        let id = TenantId::new("acme".to_string()).unwrap();
        repo.create(id.clone(), "ACME".to_string(), TenantQuotas::standard())
            .await
            .unwrap();

        repo.increment_usage(&id, UsageMeter::Events, 10)
            .await
            .unwrap();
        repo.increment_usage(&id, UsageMeter::Queries, 4)
            .await
            .unwrap();

        assert_eq!(quota_counter(&repo, &id, "events_used").await, 10);
        assert_eq!(quota_counter(&repo, &id, "queries_used").await, 4);
    }

    #[tokio::test]
    async fn increment_usage_preserves_sibling_metadata() {
        // Increments must not clobber subscription / *_quota that the backfill
        // and tier-apply wrote into metadata.quotas alongside the counters.
        let (repo, _dir) = create_test_repo();
        let id = TenantId::new("acme".to_string()).unwrap();
        repo.create(id.clone(), "ACME".to_string(), TenantQuotas::standard())
            .await
            .unwrap();

        // Seed realistic metadata via save() (the same path the CP uses).
        let mut tenant = repo.find_by_id(&id).await.unwrap().unwrap();
        tenant.update_metadata(json!({
            "subscription": { "tier": "professional", "status": "active" },
            "quotas": { "events_quota": 5_000_000, "queries_quota": 1_000_000, "events_used": 100 },
        }));
        repo.save(&tenant).await.unwrap();

        repo.increment_usage(&id, UsageMeter::Events, 50)
            .await
            .unwrap();

        let md = repo.find_by_id(&id).await.unwrap().unwrap();
        let md = md.metadata();
        assert_eq!(md["subscription"]["tier"], "professional");
        assert_eq!(md["quotas"]["events_quota"], 5_000_000);
        assert_eq!(md["quotas"]["queries_quota"], 1_000_000);
        assert_eq!(md["quotas"]["events_used"], 150);
    }

    #[tokio::test]
    async fn increment_usage_unknown_tenant_returns_none() {
        let (repo, _dir) = create_test_repo();
        let ghost = TenantId::new("ghost".to_string()).unwrap();
        let v = repo
            .increment_usage(&ghost, UsageMeter::Events, 1)
            .await
            .unwrap();
        assert_eq!(v, None, "unknown tenant must report None → handler 404s");
    }

    #[tokio::test]
    async fn increment_usage_survives_restart() {
        // The dashboard counter must be durable: increments go out as UPDATED
        // events (which carry metadata and ARE replayed), unlike the volatile
        // USAGE_UPDATED events. Rebuild from WAL and confirm the count holds.
        let temp_dir = TempDir::new().unwrap();
        let system_dir = temp_dir.path().join("__system");
        let id = TenantId::new("acme".to_string()).unwrap();

        {
            let system_store = Arc::new(SystemMetadataStore::new(&system_dir).unwrap());
            let repo = EventSourcedTenantRepository::new(system_store);
            repo.create(id.clone(), "ACME".to_string(), TenantQuotas::standard())
                .await
                .unwrap();
            repo.increment_usage(&id, UsageMeter::Events, 42)
                .await
                .unwrap();
            repo.increment_usage(&id, UsageMeter::Queries, 7)
                .await
                .unwrap();
        }

        {
            let system_store = Arc::new(SystemMetadataStore::new(&system_dir).unwrap());
            let repo = EventSourcedTenantRepository::new(system_store);
            assert_eq!(
                quota_counter(&repo, &id, "events_used").await,
                42,
                "events_used must survive a restart"
            );
            assert_eq!(
                quota_counter(&repo, &id, "queries_used").await,
                7,
                "queries_used must survive a restart"
            );
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn increment_usage_is_concurrency_safe_no_lost_updates() {
        // The load-bearing test: fire N concurrent +1 increments at one tenant
        // and assert the counter equals exactly N. A naive read-modify-write
        // would lose updates (final < N). The per-tenant lock serializes the
        // read-bump-emit so every increment is observed.
        let (repo, _dir) = create_test_repo();
        let repo = Arc::new(repo);
        let id = TenantId::new("acme".to_string()).unwrap();
        repo.create(id.clone(), "ACME".to_string(), TenantQuotas::standard())
            .await
            .unwrap();

        const N: u64 = 500;
        let mut handles = Vec::with_capacity(N as usize);
        for _ in 0..N {
            let repo = Arc::clone(&repo);
            let id = id.clone();
            handles.push(tokio::spawn(async move {
                repo.increment_usage(&id, UsageMeter::Events, 1)
                    .await
                    .unwrap()
            }));
        }
        for h in handles {
            h.await.unwrap();
        }

        assert_eq!(
            quota_counter(&repo, &id, "events_used").await,
            N,
            "concurrent increments must not lose updates"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn increment_usage_concurrent_mixed_meters_sum_independently() {
        // Concurrency across both meters at once: events and queries race
        // together; each counter must still equal its own exact sum.
        let (repo, _dir) = create_test_repo();
        let repo = Arc::new(repo);
        let id = TenantId::new("acme".to_string()).unwrap();
        repo.create(id.clone(), "ACME".to_string(), TenantQuotas::standard())
            .await
            .unwrap();

        const N: u64 = 300;
        let mut handles = Vec::new();
        for i in 0..N {
            let repo = Arc::clone(&repo);
            let id = id.clone();
            let meter = if i % 2 == 0 {
                UsageMeter::Events
            } else {
                UsageMeter::Queries
            };
            handles.push(tokio::spawn(async move {
                repo.increment_usage(&id, meter, 2).await.unwrap()
            }));
        }
        for h in handles {
            h.await.unwrap();
        }

        // 150 events * 2 = 300 ; 150 queries * 2 = 300
        assert_eq!(quota_counter(&repo, &id, "events_used").await, 300);
        assert_eq!(quota_counter(&repo, &id, "queries_used").await, 300);
    }

    /// Read the tenant's enabled-projection array from metadata, defaulting to empty.
    async fn enabled_projections(
        repo: &EventSourcedTenantRepository,
        id: &TenantId,
    ) -> Vec<String> {
        repo.find_by_id(id)
            .await
            .unwrap()
            .unwrap()
            .metadata()
            .get("projections")
            .and_then(|p| p.get("enabled"))
            .and_then(serde_json::Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }

    #[tokio::test]
    async fn merge_metadata_preserves_sibling_keys() {
        let (repo, _dir) = create_test_repo();
        let id = TenantId::new("acme".to_string()).unwrap();
        repo.create(id.clone(), "ACME".to_string(), TenantQuotas::standard())
            .await
            .unwrap();

        // Seed a money-adjacent counter via the usage path.
        repo.increment_usage(&id, UsageMeter::Events, 142)
            .await
            .unwrap();

        // Merge an opaque enabled-projection set — must NOT drop quotas.
        repo.merge_metadata(
            &id,
            json!({ "projections": { "enabled": ["event-count"] } }),
        )
        .await
        .unwrap();

        assert_eq!(quota_counter(&repo, &id, "events_used").await, 142);
        assert_eq!(enabled_projections(&repo, &id).await, vec!["event-count"]);

        // A second merge replaces the array (set semantics) and still keeps quotas.
        repo.merge_metadata(
            &id,
            json!({ "projections": { "enabled": ["event-count", "entity-activity"] } }),
        )
        .await
        .unwrap();
        assert_eq!(quota_counter(&repo, &id, "events_used").await, 142);
        assert_eq!(
            enabled_projections(&repo, &id).await,
            vec!["event-count", "entity-activity"]
        );

        // A further quota bump must not drop the enabled set.
        repo.increment_usage(&id, UsageMeter::Events, 8)
            .await
            .unwrap();
        assert_eq!(quota_counter(&repo, &id, "events_used").await, 150);
        assert_eq!(
            enabled_projections(&repo, &id).await,
            vec!["event-count", "entity-activity"]
        );
    }

    #[tokio::test]
    async fn merge_metadata_does_not_clobber_concurrent_quota_bumps() {
        let (repo, _dir) = create_test_repo();
        let repo = Arc::new(repo);
        let id = TenantId::new("acme".to_string()).unwrap();
        repo.create(id.clone(), "ACME".to_string(), TenantQuotas::standard())
            .await
            .unwrap();

        // Race N quota increments against a metadata merge. They share the
        // per-tenant lock, so neither clobbers the other's sibling keys.
        const N: u64 = 200;
        let mut handles = Vec::new();
        for _ in 0..N {
            let repo = Arc::clone(&repo);
            let id = id.clone();
            handles.push(tokio::spawn(async move {
                repo.increment_usage(&id, UsageMeter::Events, 1)
                    .await
                    .unwrap();
            }));
        }
        {
            let repo = Arc::clone(&repo);
            let id = id.clone();
            handles.push(tokio::spawn(async move {
                repo.merge_metadata(
                    &id,
                    json!({ "projections": { "enabled": ["event-count"] } }),
                )
                .await
                .unwrap();
            }));
        }
        for h in handles {
            h.await.unwrap();
        }

        // Every increment landed (no lost update) AND the merge survived.
        assert_eq!(quota_counter(&repo, &id, "events_used").await, N);
        assert_eq!(enabled_projections(&repo, &id).await, vec!["event-count"]);
    }

    #[tokio::test]
    async fn merge_metadata_unknown_tenant_returns_none() {
        let (repo, _dir) = create_test_repo();
        let id = TenantId::new("ghost".to_string()).unwrap();
        let res = repo.merge_metadata(&id, json!({ "x": 1 })).await.unwrap();
        assert!(res.is_none());
    }
}
