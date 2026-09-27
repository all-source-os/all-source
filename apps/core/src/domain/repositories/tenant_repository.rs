// Copyright 2024-2025 AllSource Team
// Licensed under the Business Source License 1.1 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     See LICENSE-BSL in the repository root
//
// Change Date: 2029-03-01
// Change License: Apache License, Version 2.0

use crate::{
    domain::{
        entities::{SchemaEnforcement, Tenant, TenantQuotas, TenantUsage, UsageMeter},
        value_objects::TenantId,
    },
    error::Result,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};

/// Repository trait for tenant management
///
/// Provides persistent storage and retrieval operations for tenants.
/// All implementations must enforce tenant isolation and data integrity.
///
/// # Responsibilities
/// - CRUD operations for tenants
/// - Tenant activation/deactivation
/// - Quota management
/// - Usage tracking
/// - Tenant querying and filtering
///
/// # Thread Safety
/// Implementations must be thread-safe (Send + Sync).
///
/// # Example
/// ```rust
/// use allsource_core::domain::repositories::TenantRepository;
/// use allsource_core::infrastructure::repositories::InMemoryTenantRepository;
/// use allsource_core::domain::value_objects::TenantId;
/// use allsource_core::domain::entities::TenantQuotas;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let repo = InMemoryTenantRepository::new();
///
///     // Create tenant
///     let tenant_id = TenantId::new("acme-corp".to_string())?;
///     let quotas = TenantQuotas::standard();
///     let tenant = repo.create(tenant_id.clone(), "ACME Corp".to_string(), quotas).await?;
///
///     // Find tenant
///     let found = repo.find_by_id(&tenant_id).await?;
///     assert!(found.is_some());
///
///     Ok(())
/// }
/// ```
#[async_trait]
pub trait TenantRepository: Send + Sync {
    /// Create a new tenant
    ///
    /// # Arguments
    /// * `id` - Unique tenant identifier
    /// * `name` - Human-readable tenant name
    /// * `quotas` - Resource quotas for the tenant
    ///
    /// # Returns
    /// The created tenant
    ///
    /// # Errors
    /// - `TenantAlreadyExists` - If a tenant with this ID already exists
    /// - `ValidationError` - If the name is invalid
    /// - `StorageError` - If the operation fails
    async fn create(&self, id: TenantId, name: String, quotas: TenantQuotas) -> Result<Tenant>;

    /// Create an already validated tenant with its initial operational metadata
    /// in the same durable creation. Never update a tenant that already exists.
    /// Backends without this guarantee must refuse instead of exposing a
    /// partially initialized tenant or overwriting a concurrent subscription.
    async fn create_initialized(&self, _tenant: Tenant) -> Result<Tenant> {
        Err(crate::error::AllSourceError::InternalError(
            "Atomic tenant initialization is unavailable".into(),
        ))
    }

    /// Save or update a tenant
    ///
    /// If the tenant doesn't exist, it will be created.
    /// If it exists, it will be updated.
    /// Event-sourced backends preserve an already managed query counter;
    /// explicit query resets use `reset_query_usage`, not a stale snapshot.
    ///
    /// # Arguments
    /// * `tenant` - The tenant to save
    ///
    /// # Errors
    /// - `ValidationError` - If tenant data is invalid
    /// - `StorageError` - If the operation fails
    async fn save(&self, tenant: &Tenant) -> Result<()>;

    /// Find a tenant by ID
    ///
    /// # Arguments
    /// * `id` - The tenant ID to search for
    ///
    /// # Returns
    /// `Some(Tenant)` if found, `None` otherwise
    ///
    /// # Errors
    /// - `StorageError` - If the operation fails
    async fn find_by_id(&self, id: &TenantId) -> Result<Option<Tenant>>;

    /// Find a tenant by name (case-insensitive)
    ///
    /// # Arguments
    /// * `name` - The tenant name to search for
    ///
    /// # Returns
    /// `Some(Tenant)` if found, `None` otherwise
    ///
    /// # Errors
    /// - `StorageError` - If the operation fails
    async fn find_by_name(&self, name: &str) -> Result<Option<Tenant>>;

    /// Get all tenants with pagination
    ///
    /// # Arguments
    /// * `limit` - Maximum number of tenants to return
    /// * `offset` - Number of tenants to skip
    ///
    /// # Returns
    /// Vector of tenants, ordered by creation date (newest first)
    ///
    /// # Errors
    /// - `StorageError` - If the operation fails
    async fn find_all(&self, limit: usize, offset: usize) -> Result<Vec<Tenant>>;

    /// Get active tenants only
    ///
    /// # Arguments
    /// * `limit` - Maximum number of tenants to return
    /// * `offset` - Number of tenants to skip
    ///
    /// # Returns
    /// Vector of active tenants
    ///
    /// # Errors
    /// - `StorageError` - If the operation fails
    async fn find_active(&self, limit: usize, offset: usize) -> Result<Vec<Tenant>>;

    /// Count total number of tenants
    ///
    /// # Returns
    /// Total number of tenants in the system
    ///
    /// # Errors
    /// - `StorageError` - If the operation fails
    async fn count(&self) -> Result<usize>;

    /// Count active tenants
    ///
    /// # Returns
    /// Number of active tenants
    ///
    /// # Errors
    /// - `StorageError` - If the operation fails
    async fn count_active(&self) -> Result<usize>;

    /// Delete a tenant
    ///
    /// # Warning
    /// This is a destructive operation. Consider deactivating instead.
    ///
    /// # Arguments
    /// * `id` - The tenant ID to delete
    ///
    /// # Returns
    /// `true` if the tenant was deleted, `false` if it didn't exist
    ///
    /// # Errors
    /// - `StorageError` - If the operation fails
    async fn delete(&self, id: &TenantId) -> Result<bool>;

    /// Update tenant quotas
    ///
    /// # Arguments
    /// * `id` - The tenant ID
    /// * `quotas` - New quotas to apply
    ///
    /// # Returns
    /// `true` if updated, `false` if tenant not found
    ///
    /// # Errors
    /// - `StorageError` - If the operation fails
    async fn update_quotas(&self, id: &TenantId, quotas: TenantQuotas) -> Result<bool>;

    /// Update a tenant's schema-enforcement mode (Gap 3 toggle).
    ///
    /// Controls whether registered schemas are enforced on event ingest:
    /// `Permissive` (default), `Warn`, or `Strict`. See [`SchemaEnforcement`].
    ///
    /// # Returns
    /// `true` if updated, `false` if tenant not found
    ///
    /// # Errors
    /// - `StorageError` - If the operation fails
    async fn update_schema_enforcement(
        &self,
        id: &TenantId,
        mode: SchemaEnforcement,
    ) -> Result<bool>;

    /// Update tenant usage statistics
    ///
    /// # Arguments
    /// * `id` - The tenant ID
    /// * `usage` - New usage statistics
    ///
    /// # Returns
    /// `true` if updated, `false` if tenant not found
    ///
    /// # Errors
    /// - `StorageError` - If the operation fails
    async fn update_usage(&self, id: &TenantId, usage: TenantUsage) -> Result<bool>;

    /// Atomically increment a tenant's forward usage counter in
    /// `metadata.quotas` and return the new counter value.
    ///
    /// This is the write half of forward usage-metering: the Query Service
    /// POSTs one increment per metered activity (events ingested / queries
    /// run) and `meter` selects which `metadata.quotas` field is bumped
    /// (`events_used` for [`UsageMeter::Events`], `queries_used` for
    /// [`UsageMeter::Queries`]) — the exact fields the dashboard reads back.
    ///
    /// # Atomicity
    ///
    /// Implementations MUST make the read-modify-write atomic *per tenant*.
    /// Increments arrive batched and concurrently; a naive
    /// read-then-write would lose updates under load (two callers read the
    /// same base, both add, one write wins), silently under-billing. The
    /// counter after N concurrent `+k` increments must equal `start + N*k`.
    ///
    /// # Arguments
    /// * `id` - The tenant ID
    /// * `meter` - Which counter to bump (events vs queries)
    /// * `count` - Amount to add (callers reject 0 / negative upstream)
    ///
    /// # Returns
    /// `Some(new_value)` with the post-increment counter, or `None` if the
    /// tenant does not exist.
    ///
    /// # Errors
    /// - `StorageError` - If the durable write fails
    async fn increment_usage(
        &self,
        id: &TenantId,
        meter: UsageMeter,
        count: u64,
    ) -> Result<Option<u64>>;

    /// Durably admit a bounded query operation against the canonical quota.
    /// Backends without atomic counter/receipt persistence must refuse.
    async fn admit_query_usage(
        &self,
        _id: &TenantId,
        _request: crate::domain::entities::query_usage::QueryUsageRequest,
    ) -> Result<Option<crate::domain::entities::query_usage::QueryUsageDecision>> {
        Err(crate::error::AllSourceError::InternalError(
            "Durable query admission is unavailable".into(),
        ))
    }

    async fn get_query_usage(
        &self,
        _id: &TenantId,
    ) -> Result<Option<crate::domain::entities::query_usage::QueryUsageSnapshot>> {
        Err(crate::error::AllSourceError::InternalError(
            "Durable query admission is unavailable".into(),
        ))
    }

    /// Explicitly advance a managed query meter's period generation. Replaying
    /// the same transition cannot erase charges made after its first commit.
    async fn reset_query_usage(
        &self,
        _id: &TenantId,
        _request: crate::domain::entities::query_usage::QueryUsageReset,
    ) -> Result<Option<crate::domain::entities::query_usage::QueryUsageResetDecision>> {
        Err(crate::error::AllSourceError::InternalError(
            "Durable query reset is unavailable".into(),
        ))
    }

    /// Activate a tenant
    ///
    /// # Arguments
    /// * `id` - The tenant ID to activate
    ///
    /// # Returns
    /// `true` if activated, `false` if tenant not found
    ///
    /// # Errors
    /// - `StorageError` - If the operation fails
    async fn activate(&self, id: &TenantId) -> Result<bool>;

    /// Deactivate a tenant
    ///
    /// Deactivated tenants cannot ingest events or perform operations.
    ///
    /// # Arguments
    /// * `id` - The tenant ID to deactivate
    ///
    /// # Returns
    /// `true` if deactivated, `false` if tenant not found
    ///
    /// # Errors
    /// - `StorageError` - If the operation fails
    async fn deactivate(&self, id: &TenantId) -> Result<bool>;

    /// Check if a tenant exists
    ///
    /// # Arguments
    /// * `id` - The tenant ID to check
    ///
    /// # Returns
    /// `true` if the tenant exists, `false` otherwise
    ///
    /// # Errors
    /// - `StorageError` - If the operation fails
    async fn exists(&self, id: &TenantId) -> Result<bool> {
        Ok(self.find_by_id(id).await?.is_some())
    }

    /// Check if a tenant is active
    ///
    /// # Arguments
    /// * `id` - The tenant ID to check
    ///
    /// # Returns
    /// `true` if the tenant exists and is active, `false` otherwise
    ///
    /// # Errors
    /// - `StorageError` - If the operation fails
    async fn is_active(&self, id: &TenantId) -> Result<bool> {
        match self.find_by_id(id).await? {
            Some(tenant) => Ok(tenant.is_active()),
            None => Ok(false),
        }
    }

    /// Atomically deep-merge `partial` into the tenant's `metadata`, preserving
    /// every sibling key, and persist durably.
    ///
    /// `partial` is treated as opaque JSON: object keys are merged one level at
    /// a time (nested objects merged recursively; arrays and scalars replace),
    /// so writing `{"projections": {"enabled": [...]}}` leaves `metadata.quotas`
    /// (and any other key) untouched. Core does NOT interpret these keys — it is
    /// the generic storage path the Query Service uses to persist a tenant's
    /// enabled projection set (see `docs/proposals/PER_TENANT_PROJECTIONS.md`).
    /// An event-sourced backend preserves the canonical query counter after
    /// durable admission owns it; generic metadata cannot reset that meter.
    ///
    /// Returns the merged metadata, or `None` if the tenant does not exist.
    ///
    /// The default implementation is a read-modify-write via `find_by_id` +
    /// `save`. Event-sourced implementations override it to serialize against
    /// concurrent quota bumps (`increment_usage`) under a per-tenant lock so a
    /// metadata write and a money-adjacent counter bump never clobber each
    /// other's sibling keys.
    ///
    /// # Errors
    /// - `StorageError` - If the read or persist fails
    async fn merge_metadata(
        &self,
        id: &TenantId,
        partial: serde_json::Value,
    ) -> Result<Option<serde_json::Value>> {
        let Some(mut tenant) = self.find_by_id(id).await? else {
            return Ok(None);
        };
        let mut metadata = tenant.metadata().clone();
        deep_merge_metadata(&mut metadata, partial);
        tenant.update_metadata(metadata.clone());
        self.save(&tenant).await?;
        Ok(Some(metadata))
    }
}

/// Deep-merge `patch` into `target` in place. Object keys are merged
/// recursively; arrays and scalars in `patch` replace the value at `target`. If
/// `patch` is an object and `target` is not, `target` is first coerced to an
/// empty object so the merge preserves no stale scalar. Used by
/// [`TenantRepository::merge_metadata`] to apply a partial metadata update
/// without dropping sibling keys.
pub fn deep_merge_metadata(target: &mut serde_json::Value, patch: serde_json::Value) {
    match patch {
        serde_json::Value::Object(patch_obj) => {
            if !target.is_object() {
                *target = serde_json::Value::Object(serde_json::Map::new());
            }
            let target_obj = target
                .as_object_mut()
                .expect("target coerced to object above");
            for (key, value) in patch_obj {
                deep_merge_metadata(
                    target_obj.entry(key).or_insert(serde_json::Value::Null),
                    value,
                );
            }
        }
        other => *target = other,
    }
}

/// Query filter for finding tenants
#[derive(Debug, Clone, Default)]
pub struct TenantQuery {
    pub active_only: bool,
    pub name_contains: Option<String>,
    pub created_after: Option<DateTime<Utc>>,
    pub created_before: Option<DateTime<Utc>>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

impl TenantQuery {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn active_only(mut self) -> Self {
        self.active_only = true;
        self
    }

    pub fn with_name_filter(mut self, name: String) -> Self {
        self.name_contains = Some(name);
        self
    }

    pub fn created_after(mut self, date: DateTime<Utc>) -> Self {
        self.created_after = Some(date);
        self
    }

    pub fn created_before(mut self, date: DateTime<Utc>) -> Self {
        self.created_before = Some(date);
        self
    }

    pub fn with_pagination(mut self, limit: usize, offset: usize) -> Self {
        self.limit = Some(limit);
        self.offset = Some(offset);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tenant_query_builder() {
        let query = TenantQuery::new()
            .active_only()
            .with_name_filter("acme".to_string())
            .with_pagination(10, 0);

        assert!(query.active_only);
        assert_eq!(query.name_contains, Some("acme".to_string()));
        assert_eq!(query.limit, Some(10));
        assert_eq!(query.offset, Some(0));
    }

    #[test]
    fn test_tenant_query_with_dates() {
        let now = Utc::now();
        let yesterday = now - chrono::Duration::days(1);

        let query = TenantQuery::new()
            .created_after(yesterday)
            .created_before(now);

        assert!(query.created_after.is_some());
        assert!(query.created_before.is_some());
    }

    #[test]
    fn deep_merge_preserves_siblings_and_recurses() {
        let mut target = serde_json::json!({
            "quotas": { "events_used": 142, "queries_used": 7 },
            "subscription": { "tier": "studio" }
        });
        deep_merge_metadata(
            &mut target,
            serde_json::json!({ "projections": { "enabled": ["event-count"] } }),
        );
        // New nested key added; existing siblings untouched.
        assert_eq!(target["quotas"]["events_used"], 142);
        assert_eq!(target["subscription"]["tier"], "studio");
        assert_eq!(target["projections"]["enabled"][0], "event-count");

        // Merging into an existing nested object keeps that object's other keys.
        deep_merge_metadata(
            &mut target,
            serde_json::json!({ "quotas": { "events_used": 150 } }),
        );
        assert_eq!(target["quotas"]["events_used"], 150);
        assert_eq!(target["quotas"]["queries_used"], 7);
    }

    #[test]
    fn deep_merge_arrays_and_scalars_replace() {
        let mut target = serde_json::json!({ "enabled": ["a", "b"], "n": 1 });
        deep_merge_metadata(&mut target, serde_json::json!({ "enabled": ["c"], "n": 2 }));
        assert_eq!(target["enabled"], serde_json::json!(["c"]));
        assert_eq!(target["n"], 2);
    }

    #[test]
    fn deep_merge_object_over_scalar_coerces() {
        let mut target = serde_json::json!({ "projections": "stale" });
        deep_merge_metadata(
            &mut target,
            serde_json::json!({ "projections": { "enabled": [] } }),
        );
        assert!(target["projections"].is_object());
        assert_eq!(target["projections"]["enabled"], serde_json::json!([]));
    }
}
