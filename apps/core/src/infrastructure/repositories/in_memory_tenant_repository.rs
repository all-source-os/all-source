use crate::{
    domain::{
        entities::{SchemaEnforcement, Tenant, TenantQuotas, TenantUsage, UsageMeter},
        repositories::TenantRepository,
        value_objects::TenantId,
    },
    error::{AllSourceError, Result},
};
use async_trait::async_trait;
use chrono::Utc;
use dashmap::DashMap;
use serde_json::json;
use std::sync::Arc;

/// In-memory implementation of TenantRepository
///
/// Thread-safe tenant storage using DashMap for concurrent access.
/// Suitable for testing, development, and single-node deployments.
///
/// # Thread Safety
/// Uses DashMap internally, providing lock-free reads and fine-grained locking for writes.
///
/// # Example
/// ```rust
/// use allsource_core::infrastructure::repositories::InMemoryTenantRepository;
/// use allsource_core::domain::repositories::TenantRepository;
/// use allsource_core::domain::value_objects::TenantId;
/// use allsource_core::domain::entities::TenantQuotas;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let repo = InMemoryTenantRepository::new();
///
///     let tenant_id = TenantId::new("acme".to_string())?;
///     let quotas = TenantQuotas::standard();
///     let tenant = repo.create(tenant_id.clone(), "ACME Corp".to_string(), quotas).await?;
///
///     assert!(repo.exists(&tenant_id).await?);
///     Ok(())
/// }
/// ```
pub struct InMemoryTenantRepository {
    tenants: Arc<DashMap<String, Tenant>>,
}

impl InMemoryTenantRepository {
    /// Create a new empty in-memory tenant repository
    pub fn new() -> Self {
        Self {
            tenants: Arc::new(DashMap::new()),
        }
    }

    /// Create a new repository with initial tenants
    pub fn with_tenants(tenants: Vec<Tenant>) -> Self {
        let repo = Self::new();
        for tenant in tenants {
            repo.tenants
                .insert(tenant.id().as_str().to_string(), tenant);
        }
        repo
    }

    /// Get the current number of tenants in memory
    pub fn len(&self) -> usize {
        self.tenants.len()
    }

    /// Check if the repository is empty
    pub fn is_empty(&self) -> bool {
        self.tenants.is_empty()
    }

    /// Clear all tenants (useful for testing)
    pub fn clear(&self) {
        self.tenants.clear();
    }
}

impl Default for InMemoryTenantRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TenantRepository for InMemoryTenantRepository {
    async fn create_initialized(&self, tenant: Tenant) -> Result<Tenant> {
        match self.tenants.entry(tenant.id().as_str().to_string()) {
            dashmap::mapref::entry::Entry::Vacant(entry) => {
                entry.insert(tenant.clone());
                Ok(tenant)
            }
            dashmap::mapref::entry::Entry::Occupied(_) => Err(AllSourceError::TenantAlreadyExists(
                tenant.id().as_str().to_string(),
            )),
        }
    }

    async fn create(&self, id: TenantId, name: String, quotas: TenantQuotas) -> Result<Tenant> {
        let key = id.as_str().to_string();

        // Check if tenant already exists
        if self.tenants.contains_key(&key) {
            return Err(AllSourceError::TenantAlreadyExists(id.as_str().to_string()));
        }

        // Create the tenant
        let tenant = Tenant::new(id.clone(), name, quotas)?;

        // Store the tenant
        self.tenants.insert(key, tenant.clone());

        Ok(tenant)
    }

    async fn save(&self, tenant: &Tenant) -> Result<()> {
        let key = tenant.id().as_str().to_string();
        self.tenants.insert(key, tenant.clone());
        Ok(())
    }

    async fn find_by_id(&self, id: &TenantId) -> Result<Option<Tenant>> {
        let key = id.as_str().to_string();
        Ok(self.tenants.get(&key).map(|entry| entry.value().clone()))
    }

    async fn find_by_name(&self, name: &str) -> Result<Option<Tenant>> {
        let name_lower = name.to_lowercase();
        Ok(self
            .tenants
            .iter()
            .find(|entry| entry.value().name().to_lowercase() == name_lower)
            .map(|entry| entry.value().clone()))
    }

    async fn find_all(&self, limit: usize, offset: usize) -> Result<Vec<Tenant>> {
        let mut tenants: Vec<Tenant> = self
            .tenants
            .iter()
            .map(|entry| entry.value().clone())
            .collect();

        // Sort by created_at (newest first)
        tenants.sort_by_key(|t| std::cmp::Reverse(t.created_at()));

        // Apply pagination
        Ok(tenants.into_iter().skip(offset).take(limit).collect())
    }

    async fn find_active(&self, limit: usize, offset: usize) -> Result<Vec<Tenant>> {
        let mut tenants: Vec<Tenant> = self
            .tenants
            .iter()
            .filter(|entry| entry.value().is_active())
            .map(|entry| entry.value().clone())
            .collect();

        // Sort by created_at (newest first)
        tenants.sort_by_key(|t| std::cmp::Reverse(t.created_at()));

        // Apply pagination
        Ok(tenants.into_iter().skip(offset).take(limit).collect())
    }

    async fn count(&self) -> Result<usize> {
        Ok(self.tenants.len())
    }

    async fn count_active(&self) -> Result<usize> {
        Ok(self
            .tenants
            .iter()
            .filter(|entry| entry.value().is_active())
            .count())
    }

    async fn delete(&self, id: &TenantId) -> Result<bool> {
        let key = id.as_str().to_string();
        Ok(self.tenants.remove(&key).is_some())
    }

    async fn update_quotas(&self, id: &TenantId, quotas: TenantQuotas) -> Result<bool> {
        let key = id.as_str().to_string();

        if let Some(mut entry) = self.tenants.get_mut(&key) {
            let tenant = entry.value().clone();
            let updated = Tenant::reconstruct(
                tenant.id().clone(),
                tenant.name().to_string(),
                tenant.description().map(std::string::ToString::to_string),
                quotas,
                tenant.usage().clone(),
                tenant.created_at(),
                Utc::now(), // Update the updated_at timestamp
                tenant.is_active(),
                tenant.is_demo(),
                tenant.metadata().clone(),
            );
            *entry = updated;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn update_schema_enforcement(
        &self,
        id: &TenantId,
        mode: SchemaEnforcement,
    ) -> Result<bool> {
        // Mutate in place rather than `reconstruct` — reconstruct doesn't carry
        // the schema_enforcement field and would reset it. `set_schema_enforcement`
        // also bumps `updated_at`.
        if let Some(mut entry) = self.tenants.get_mut(id.as_str()) {
            entry.value_mut().set_schema_enforcement(mode);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn update_usage(&self, id: &TenantId, usage: TenantUsage) -> Result<bool> {
        let key = id.as_str().to_string();

        if let Some(mut entry) = self.tenants.get_mut(&key) {
            let tenant = entry.value().clone();
            let updated = Tenant::reconstruct(
                tenant.id().clone(),
                tenant.name().to_string(),
                tenant.description().map(std::string::ToString::to_string),
                tenant.quotas().clone(),
                usage,
                tenant.created_at(),
                Utc::now(), // Update the updated_at timestamp
                tenant.is_active(),
                tenant.is_demo(),
                tenant.metadata().clone(),
            );
            *entry = updated;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn increment_usage(
        &self,
        id: &TenantId,
        meter: UsageMeter,
        count: u64,
    ) -> Result<Option<u64>> {
        let key = id.as_str().to_string();

        // Concurrency: `DashMap::get_mut` holds the shard's write lock for the
        // lifetime of `entry`, so the read-modify-write below is atomic per
        // tenant — concurrent increments to the same tenant serialize and the
        // counter sums exactly. The critical section is fully synchronous (no
        // .await), so holding the guard is safe.
        if let Some(mut entry) = self.tenants.get_mut(&key) {
            let mut metadata = entry.value().metadata().clone();
            let field = meter.quota_field();

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

            let tenant = entry.value().clone();
            let updated = Tenant::reconstruct(
                tenant.id().clone(),
                tenant.name().to_string(),
                tenant.description().map(std::string::ToString::to_string),
                tenant.quotas().clone(),
                tenant.usage().clone(),
                tenant.created_at(),
                Utc::now(),
                tenant.is_active(),
                tenant.is_demo(),
                metadata,
            );
            *entry = updated;
            Ok(Some(next))
        } else {
            Ok(None)
        }
    }

    async fn activate(&self, id: &TenantId) -> Result<bool> {
        let key = id.as_str().to_string();

        if let Some(mut entry) = self.tenants.get_mut(&key) {
            let tenant = entry.value().clone();
            let updated = Tenant::reconstruct(
                tenant.id().clone(),
                tenant.name().to_string(),
                tenant.description().map(std::string::ToString::to_string),
                tenant.quotas().clone(),
                tenant.usage().clone(),
                tenant.created_at(),
                Utc::now(), // Update the updated_at timestamp
                true,       // Activate
                tenant.is_demo(),
                tenant.metadata().clone(),
            );
            *entry = updated;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn deactivate(&self, id: &TenantId) -> Result<bool> {
        let key = id.as_str().to_string();

        if let Some(mut entry) = self.tenants.get_mut(&key) {
            let tenant = entry.value().clone();
            let updated = Tenant::reconstruct(
                tenant.id().clone(),
                tenant.name().to_string(),
                tenant.description().map(std::string::ToString::to_string),
                tenant.quotas().clone(),
                tenant.usage().clone(),
                tenant.created_at(),
                Utc::now(), // Update the updated_at timestamp
                false,      // Deactivate
                tenant.is_demo(),
                tenant.metadata().clone(),
            );
            *entry = updated;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_tenant_id(suffix: &str) -> TenantId {
        TenantId::new(format!("test-tenant-{suffix}")).unwrap()
    }

    #[tokio::test]
    async fn test_create_repository() {
        let repo = InMemoryTenantRepository::new();
        assert_eq!(repo.len(), 0);
        assert!(repo.is_empty());
    }

    #[tokio::test]
    async fn test_create_tenant() {
        let repo = InMemoryTenantRepository::new();
        let tenant_id = test_tenant_id("1");
        let quotas = TenantQuotas::standard();

        let tenant = repo
            .create(tenant_id.clone(), "Test Tenant".to_string(), quotas)
            .await
            .unwrap();

        assert_eq!(tenant.id(), &tenant_id);
        assert_eq!(tenant.name(), "Test Tenant");
        assert_eq!(repo.len(), 1);
    }

    #[tokio::test]
    async fn test_create_duplicate_tenant() {
        let repo = InMemoryTenantRepository::new();
        let tenant_id = test_tenant_id("1");
        let quotas = TenantQuotas::standard();

        repo.create(tenant_id.clone(), "Test Tenant".to_string(), quotas.clone())
            .await
            .unwrap();

        let result = repo
            .create(tenant_id, "Duplicate Tenant".to_string(), quotas)
            .await;

        assert!(result.is_err());
        assert_eq!(repo.len(), 1);
    }

    #[tokio::test]
    async fn test_find_by_id() {
        let repo = InMemoryTenantRepository::new();
        let tenant_id = test_tenant_id("1");
        let quotas = TenantQuotas::standard();

        repo.create(tenant_id.clone(), "Test Tenant".to_string(), quotas)
            .await
            .unwrap();

        let found = repo.find_by_id(&tenant_id).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name(), "Test Tenant");

        let not_found = repo.find_by_id(&test_tenant_id("999")).await.unwrap();
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn test_find_by_name() {
        let repo = InMemoryTenantRepository::new();
        let tenant_id = test_tenant_id("1");
        let quotas = TenantQuotas::standard();

        repo.create(tenant_id, "ACME Corporation".to_string(), quotas)
            .await
            .unwrap();

        // Case-insensitive search
        let found = repo.find_by_name("acme corporation").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name(), "ACME Corporation");

        let not_found = repo.find_by_name("NotFound Corp").await.unwrap();
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn test_find_all() {
        let repo = InMemoryTenantRepository::new();
        let quotas = TenantQuotas::standard();

        for i in 1..=5 {
            repo.create(
                test_tenant_id(&i.to_string()),
                format!("Tenant {i}"),
                quotas.clone(),
            )
            .await
            .unwrap();
        }

        let all = repo.find_all(10, 0).await.unwrap();
        assert_eq!(all.len(), 5);

        // Test pagination
        let page = repo.find_all(2, 2).await.unwrap();
        assert_eq!(page.len(), 2);
    }

    #[tokio::test]
    async fn test_find_active() {
        let repo = InMemoryTenantRepository::new();
        let quotas = TenantQuotas::standard();

        // Create 5 tenants
        for i in 1..=5 {
            repo.create(
                test_tenant_id(&i.to_string()),
                format!("Tenant {i}"),
                quotas.clone(),
            )
            .await
            .unwrap();
        }

        // Deactivate 2 of them
        repo.deactivate(&test_tenant_id("2")).await.unwrap();
        repo.deactivate(&test_tenant_id("4")).await.unwrap();

        let active = repo.find_active(10, 0).await.unwrap();
        assert_eq!(active.len(), 3);

        // All returned tenants should be active
        for tenant in active {
            assert!(tenant.is_active());
        }
    }

    #[tokio::test]
    async fn test_count() {
        let repo = InMemoryTenantRepository::new();
        let quotas = TenantQuotas::standard();

        assert_eq!(repo.count().await.unwrap(), 0);

        for i in 1..=3 {
            repo.create(
                test_tenant_id(&i.to_string()),
                format!("Tenant {i}"),
                quotas.clone(),
            )
            .await
            .unwrap();
        }

        assert_eq!(repo.count().await.unwrap(), 3);
    }

    #[tokio::test]
    async fn test_count_active() {
        let repo = InMemoryTenantRepository::new();
        let quotas = TenantQuotas::standard();

        for i in 1..=4 {
            repo.create(
                test_tenant_id(&i.to_string()),
                format!("Tenant {i}"),
                quotas.clone(),
            )
            .await
            .unwrap();
        }

        repo.deactivate(&test_tenant_id("1")).await.unwrap();
        repo.deactivate(&test_tenant_id("3")).await.unwrap();

        assert_eq!(repo.count().await.unwrap(), 4);
        assert_eq!(repo.count_active().await.unwrap(), 2);
    }

    #[tokio::test]
    async fn test_delete() {
        let repo = InMemoryTenantRepository::new();
        let tenant_id = test_tenant_id("1");
        let quotas = TenantQuotas::standard();

        repo.create(tenant_id.clone(), "Test Tenant".to_string(), quotas)
            .await
            .unwrap();

        assert_eq!(repo.len(), 1);

        let deleted = repo.delete(&tenant_id).await.unwrap();
        assert!(deleted);
        assert_eq!(repo.len(), 0);

        // Deleting again should return false
        let deleted_again = repo.delete(&tenant_id).await.unwrap();
        assert!(!deleted_again);
    }

    #[tokio::test]
    async fn test_update_quotas() {
        let repo = InMemoryTenantRepository::new();
        let tenant_id = test_tenant_id("1");
        let quotas = TenantQuotas::standard();

        repo.create(tenant_id.clone(), "Test Tenant".to_string(), quotas)
            .await
            .unwrap();

        let new_quotas = TenantQuotas::professional();
        let updated = repo
            .update_quotas(&tenant_id, new_quotas.clone())
            .await
            .unwrap();
        assert!(updated);

        let tenant = repo.find_by_id(&tenant_id).await.unwrap().unwrap();
        assert_eq!(tenant.quotas(), &new_quotas);
    }

    #[tokio::test]
    async fn test_update_usage() {
        let repo = InMemoryTenantRepository::new();
        let tenant_id = test_tenant_id("1");
        let quotas = TenantQuotas::standard();

        repo.create(tenant_id.clone(), "Test Tenant".to_string(), quotas)
            .await
            .unwrap();

        let mut usage = TenantUsage::new();
        usage.record_event();
        usage.record_event();

        let updated = repo.update_usage(&tenant_id, usage.clone()).await.unwrap();
        assert!(updated);

        let tenant = repo.find_by_id(&tenant_id).await.unwrap().unwrap();
        assert_eq!(tenant.usage().events_today(), 2);
    }

    #[tokio::test]
    async fn test_increment_usage_typed_and_unknown() {
        let repo = InMemoryTenantRepository::new();
        let id = test_tenant_id("1");
        repo.create(id.clone(), "T".to_string(), TenantQuotas::standard())
            .await
            .unwrap();

        assert_eq!(
            repo.increment_usage(&id, UsageMeter::Events, 5)
                .await
                .unwrap(),
            Some(5)
        );
        assert_eq!(
            repo.increment_usage(&id, UsageMeter::Queries, 2)
                .await
                .unwrap(),
            Some(2)
        );
        assert_eq!(
            repo.increment_usage(&id, UsageMeter::Events, 1)
                .await
                .unwrap(),
            Some(6)
        );

        let md = repo.find_by_id(&id).await.unwrap().unwrap();
        assert_eq!(md.metadata()["quotas"]["events_used"], 6);
        assert_eq!(md.metadata()["quotas"]["queries_used"], 2);

        // Unknown tenant → None (handler 404s).
        assert_eq!(
            repo.increment_usage(&test_tenant_id("ghost"), UsageMeter::Events, 1)
                .await
                .unwrap(),
            None
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_increment_usage_concurrency_no_lost_updates() {
        // DashMap's get_mut write-lock serializes the per-tenant RMW; N
        // concurrent +1 increments must sum to exactly N.
        let repo = Arc::new(InMemoryTenantRepository::new());
        let id = test_tenant_id("1");
        repo.create(id.clone(), "T".to_string(), TenantQuotas::standard())
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

        let md = repo.find_by_id(&id).await.unwrap().unwrap();
        assert_eq!(md.metadata()["quotas"]["events_used"], N);
    }

    #[tokio::test]
    async fn test_activate_deactivate() {
        let repo = InMemoryTenantRepository::new();
        let tenant_id = test_tenant_id("1");
        let quotas = TenantQuotas::standard();

        repo.create(tenant_id.clone(), "Test Tenant".to_string(), quotas)
            .await
            .unwrap();

        // Should be active by default
        assert!(repo.is_active(&tenant_id).await.unwrap());

        // Deactivate
        repo.deactivate(&tenant_id).await.unwrap();
        assert!(!repo.is_active(&tenant_id).await.unwrap());

        // Activate
        repo.activate(&tenant_id).await.unwrap();
        assert!(repo.is_active(&tenant_id).await.unwrap());
    }

    #[tokio::test]
    async fn test_exists() {
        let repo = InMemoryTenantRepository::new();
        let tenant_id = test_tenant_id("1");
        let quotas = TenantQuotas::standard();

        assert!(!repo.exists(&tenant_id).await.unwrap());

        repo.create(tenant_id.clone(), "Test Tenant".to_string(), quotas)
            .await
            .unwrap();

        assert!(repo.exists(&tenant_id).await.unwrap());
    }

    #[tokio::test]
    async fn test_save() {
        let repo = InMemoryTenantRepository::new();
        let tenant_id = test_tenant_id("1");
        let quotas = TenantQuotas::standard();

        let tenant = repo
            .create(tenant_id.clone(), "Test Tenant".to_string(), quotas.clone())
            .await
            .unwrap();

        // Update the tenant
        let updated_tenant = Tenant::reconstruct(
            tenant.id().clone(),
            "Updated Tenant".to_string(),
            tenant.description().map(std::string::ToString::to_string),
            quotas,
            tenant.usage().clone(),
            tenant.created_at(),
            Utc::now(),
            tenant.is_active(),
            tenant.is_demo(),
            tenant.metadata().clone(),
        );

        repo.save(&updated_tenant).await.unwrap();

        let found = repo.find_by_id(&tenant_id).await.unwrap().unwrap();
        assert_eq!(found.name(), "Updated Tenant");
    }

    #[tokio::test]
    async fn test_with_tenants() {
        let quotas = TenantQuotas::standard();
        let tenants = vec![
            Tenant::new(test_tenant_id("1"), "Tenant 1".to_string(), quotas.clone()).unwrap(),
            Tenant::new(test_tenant_id("2"), "Tenant 2".to_string(), quotas.clone()).unwrap(),
        ];

        let repo = InMemoryTenantRepository::with_tenants(tenants);
        assert_eq!(repo.len(), 2);
    }

    #[tokio::test]
    async fn test_clear() {
        let repo = InMemoryTenantRepository::new();
        let quotas = TenantQuotas::standard();

        for i in 1..=3 {
            repo.create(
                test_tenant_id(&i.to_string()),
                format!("Tenant {i}"),
                quotas.clone(),
            )
            .await
            .unwrap();
        }

        assert_eq!(repo.len(), 3);

        repo.clear();
        assert_eq!(repo.len(), 0);
        assert!(repo.is_empty());
    }
}
