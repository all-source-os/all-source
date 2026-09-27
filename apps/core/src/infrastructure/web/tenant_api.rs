use crate::{
    domain::{
        entities::{SchemaEnforcement, TenantQuotas, UsageMeter},
        value_objects::TenantId,
    },
    infrastructure::security::middleware::{Admin, Authenticated},
};
use axum::{Json, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};

// AppState is defined in api_v1.rs
use crate::infrastructure::web::api_v1::AppState;

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct CreateTenantRequest {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub quota_preset: Option<String>, // "trial", "free", "professional", "unlimited"
    pub quotas: Option<TenantQuotas>,
    /// Opaque initial metadata, committed atomically with tenant creation.
    pub metadata: Option<serde_json::Value>,
    #[serde(default)]
    pub is_demo: bool,
}

#[derive(Debug, Serialize)]
pub struct TenantResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub quotas: TenantQuotas,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub active: bool,
    pub is_demo: bool,
    pub schema_enforcement: SchemaEnforcement,
    /// Operational metadata (subscription tier, quotas, billing). Persisted via
    /// PUT /tenants/{id}; the Control Plane reads subscription/entitlement state
    /// from here. Must be serialized back out or the plan reads as "free".
    pub metadata: serde_json::Value,
}

impl TenantResponse {
    fn from_domain(tenant: &crate::domain::entities::Tenant) -> Self {
        Self {
            id: tenant.id().as_str().to_string(),
            name: tenant.name().to_string(),
            description: tenant.description().map(std::string::ToString::to_string),
            quotas: tenant.quotas().clone(),
            created_at: tenant.created_at(),
            updated_at: tenant.updated_at(),
            active: tenant.is_active(),
            is_demo: tenant.is_demo(),
            schema_enforcement: tenant.schema_enforcement(),
            metadata: tenant.metadata().clone(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct UpdateTenantRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub is_demo: Option<bool>,
    pub quotas: Option<TenantQuotas>,
    /// Operational metadata (subscription tier, quotas, billing). Replaces the
    /// tenant's metadata when present — callers send the full merged map.
    /// Once durable query admission owns the meter, its counter is preserved.
    /// The explicit reset API advances its separate query-period generation.
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateQuotasRequest {
    pub quotas: TenantQuotas,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSchemaEnforcementRequest {
    /// One of `permissive`, `warn`, `strict`.
    pub schema_enforcement: SchemaEnforcement,
}

/// Body for `POST /api/v1/tenants/{id}/usage/increment`.
///
/// `type` is the [`UsageMeter`] discriminator and DEFAULTS to `events` when
/// absent — that keeps the original untyped `{"count": N}` body the Query
/// Service sent before this field existed valid during a rolling deploy.
#[derive(Debug, Deserialize)]
pub struct IncrementUsageRequest {
    /// How much to add to the counter. Must be > 0. A `u64` field also makes
    /// serde reject negative numbers with a 400 before the handler runs.
    pub count: u64,
    /// Which meter to bump: `events` → `events_used`, `queries` →
    /// `queries_used`. Defaults to `events`.
    #[serde(default, rename = "type")]
    pub meter: UsageMeter,
}

/// Returned by the usage-increment endpoint: the meter that was bumped and its
/// new value, so the caller (and tests) can confirm the write landed.
#[derive(Debug, Serialize)]
pub struct IncrementUsageResponse {
    pub tenant_id: String,
    /// `events` or `queries`.
    #[serde(rename = "type")]
    pub meter: UsageMeter,
    /// The post-increment counter value (`metadata.quotas.<field>`).
    pub used: u64,
}

// ============================================================================
// Handlers
// ============================================================================

/// Resolve the quotas for a tenant being created.
///
/// Policy (prompt 048): NEW self-service tenants default to the TRIAL preset,
/// never free — a 14-day evaluation, not a permanent free tier (the Control
/// Plane owns the `trial_expires_at` clock + the expiry suspend sweep). The
/// `free_tier()` quota is RETAINED but only resolves when a caller EXPLICITLY
/// asks for `quota_preset: "free"` — that path exists for GRANDFATHERED tenants
/// (an operator re-provisioning an existing free tenant), not for new signups.
///
/// Precedence: an explicit `quotas` value wins; otherwise a recognized preset;
/// otherwise (unknown preset OR no preset at all) we fall back to the trial
/// tier, so an under-specified create can never silently mint a free-quota
/// tenant.
fn resolve_create_quotas(quotas: Option<TenantQuotas>, quota_preset: Option<&str>) -> TenantQuotas {
    if let Some(quotas) = quotas {
        return quotas;
    }
    match quota_preset {
        Some("trial") => TenantQuotas::trial_tier(),
        Some("free") => TenantQuotas::free_tier(), // grandfathered tenants only
        Some("professional") => TenantQuotas::professional(),
        Some("unlimited") => TenantQuotas::unlimited(),
        _ => TenantQuotas::trial_tier(),
    }
}

/// Create tenant (admin only)
/// POST /api/v1/tenants
pub async fn create_tenant_handler(
    State(state): State<AppState>,
    Admin(_): Admin,
    Json(req): Json<CreateTenantRequest>,
) -> Result<(StatusCode, Json<TenantResponse>), (StatusCode, String)> {
    // Determine quotas (see resolve_create_quotas for the policy + rationale).
    let quotas = resolve_create_quotas(req.quotas, req.quota_preset.as_deref());

    let tenant_id = TenantId::new(req.id).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    if let Some(metadata) = req.metadata {
        if !metadata.is_object() || metadata.to_string().len() > 32_768 {
            return Err((
                StatusCode::BAD_REQUEST,
                "Initial metadata must be an object within 32 KiB".into(),
            ));
        }
        let mut initial = crate::domain::entities::Tenant::new(tenant_id, req.name, quotas)
            .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
        initial.update_metadata(metadata);
        initial.update_description(req.description);
        initial.set_is_demo(req.is_demo);
        let tenant = state
            .tenant_repo
            .create_initialized(initial)
            .await
            .map_err(|error| {
                let status =
                    if matches!(error, crate::error::AllSourceError::TenantAlreadyExists(_)) {
                        StatusCode::CONFLICT
                    } else {
                        StatusCode::INTERNAL_SERVER_ERROR
                    };
                (status, error.to_string())
            })?;
        return Ok((
            StatusCode::CREATED,
            Json(TenantResponse::from_domain(&tenant)),
        ));
    }

    let mut tenant = state
        .tenant_repo
        .create(tenant_id, req.name, quotas)
        .await
        .map_err(|e| {
            let status = match &e {
                crate::error::AllSourceError::TenantAlreadyExists(_) => StatusCode::CONFLICT,
                _ => StatusCode::BAD_REQUEST,
            };
            (status, e.to_string())
        })?;

    // Apply optional fields that aren't part of create()
    if let Some(desc) = req.description {
        tenant.update_description(Some(desc));
    }
    if req.is_demo {
        tenant.set_is_demo(true);
    }

    // Persist the updated fields
    state
        .tenant_repo
        .save(&tenant)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok((
        StatusCode::CREATED,
        Json(TenantResponse::from_domain(&tenant)),
    ))
}

/// Get tenant
/// GET /api/v1/tenants/:id
pub async fn get_tenant_handler(
    State(state): State<AppState>,
    Authenticated(auth_ctx): Authenticated,
    axum::extract::Path(tenant_id): axum::extract::Path<String>,
) -> Result<Json<TenantResponse>, (StatusCode, String)> {
    // Users can only view their own tenant, admins can view any
    if tenant_id != auth_ctx.tenant_id() {
        auth_ctx
            .require_permission(crate::infrastructure::security::auth::Permission::Admin)
            .map_err(|_| {
                (
                    StatusCode::FORBIDDEN,
                    "Can only view own tenant".to_string(),
                )
            })?;
    }

    let tid =
        TenantId::new(tenant_id.clone()).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    let tenant = state
        .tenant_repo
        .find_by_id(&tid)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Tenant not found: {tenant_id}"),
            )
        })?;

    Ok(Json(TenantResponse::from_domain(&tenant)))
}

/// Merge a partial object into a tenant's metadata (tenant-scoped)
/// PATCH /api/v1/tenants/:id/metadata
///
/// Deep-merges the JSON object body into `metadata`, preserving every sibling
/// key, and persists atomically against concurrent quota bumps. The Query
/// Service uses this to store a tenant's opaque enabled-projection set
/// without clobbering `metadata.quotas`. Non-admin callers cannot patch billing
/// objects, and a managed query counter is preserved even for admin patches.
/// Other merged keys remain opaque (see `docs/proposals/PER_TENANT_PROJECTIONS.md`).
/// The current router requires admin permission for all tenant paths. This
/// handler additionally enforces tenant scope and billing ownership if reused
/// behind a tenant-facing transport; it does not grant that transport permission.
pub async fn merge_tenant_metadata_handler(
    State(state): State<AppState>,
    Authenticated(auth_ctx): Authenticated,
    axum::extract::Path(tenant_id): axum::extract::Path<String>,
    Json(partial): Json<serde_json::Value>,
) -> Result<Json<TenantResponse>, (StatusCode, String)> {
    // Callers may patch only their own tenant; admins may patch any.
    if tenant_id != auth_ctx.tenant_id() {
        auth_ctx
            .require_permission(crate::infrastructure::security::auth::Permission::Admin)
            .map_err(|_| {
                (
                    StatusCode::FORBIDDEN,
                    "Can only modify own tenant".to_string(),
                )
            })?;
    }

    if !partial.is_object() {
        return Err((
            StatusCode::BAD_REQUEST,
            "metadata patch must be a JSON object".to_string(),
        ));
    }

    // Billing policy and canonical meters are service-owned even when the
    // caller owns this tenant. Opaque product metadata remains tenant-scoped.
    if ["quotas", "subscription", "overage"]
        .iter()
        .any(|key| partial.get(*key).is_some())
        && auth_ctx
            .require_permission(crate::infrastructure::security::auth::Permission::Admin)
            .is_err()
    {
        return Err((
            StatusCode::FORBIDDEN,
            "Billing metadata requires administrator authority".into(),
        ));
    }

    let tid =
        TenantId::new(tenant_id.clone()).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    if state
        .tenant_repo
        .merge_metadata(&tid, partial)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .is_none()
    {
        return Err((
            StatusCode::NOT_FOUND,
            format!("Tenant not found: {tenant_id}"),
        ));
    }

    // Re-fetch for the full updated representation (metadata was mutated and
    // persisted under the per-tenant lock inside merge_metadata).
    let tenant = state
        .tenant_repo
        .find_by_id(&tid)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Tenant not found: {tenant_id}"),
            )
        })?;

    Ok(Json(TenantResponse::from_domain(&tenant)))
}

/// List all tenants (admin only)
/// GET /api/v1/tenants
pub async fn list_tenants_handler(
    State(state): State<AppState>,
    Admin(_): Admin,
) -> Result<Json<Vec<TenantResponse>>, (StatusCode, String)> {
    let tenants = state
        .tenant_repo
        .find_all(10_000, 0)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(
        tenants.iter().map(TenantResponse::from_domain).collect(),
    ))
}

/// Get tenant statistics
/// GET /api/v1/tenants/:id/stats
pub async fn get_tenant_stats_handler(
    State(state): State<AppState>,
    Authenticated(auth_ctx): Authenticated,
    axum::extract::Path(tenant_id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    // Users can only view their own tenant stats
    if tenant_id != auth_ctx.tenant_id() {
        auth_ctx
            .require_permission(crate::infrastructure::security::auth::Permission::Admin)
            .map_err(|_| {
                (
                    StatusCode::FORBIDDEN,
                    "Can only view own tenant stats".to_string(),
                )
            })?;
    }

    let tid =
        TenantId::new(tenant_id.clone()).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    let tenant = state
        .tenant_repo
        .find_by_id(&tid)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Tenant not found: {tenant_id}"),
            )
        })?;

    let stats = build_tenant_stats(&tenant);
    Ok(Json(stats))
}

/// Update tenant quotas (admin only)
/// PUT /api/v1/tenants/:id/quotas
pub async fn update_quotas_handler(
    State(state): State<AppState>,
    Admin(_): Admin,
    axum::extract::Path(tenant_id): axum::extract::Path<String>,
    Json(req): Json<UpdateQuotasRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let tid =
        TenantId::new(tenant_id.clone()).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    let updated = state
        .tenant_repo
        .update_quotas(&tid, req.quotas)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !updated {
        return Err((
            StatusCode::NOT_FOUND,
            format!("Tenant not found: {tenant_id}"),
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}

/// Increment a tenant's forward usage counter (admin only).
/// POST /api/v1/tenants/:id/usage/increment
///
/// Write half of forward usage-metering. The Query Service POSTs one increment
/// per metered activity — events ingested (`type: "events"`) or queries run
/// (`type: "queries"`) — and Core atomically bumps the matching
/// `metadata.quotas.{events_used,queries_used}` counter the dashboard reads.
///
/// Body: `{ "count": <u64>, "type": "events" | "queries" }`. `type` defaults
/// to `"events"` so the pre-typing `{count}` body keeps working during a
/// rolling deploy. Returns the post-increment value. `404` if the tenant is
/// unknown; `400` (via serde) for a missing/negative `count`; `400` for an
/// explicit `count: 0` (a no-op increment is a client bug, not a write).
///
/// Atomicity is the repository's job — see
/// [`TenantRepository::increment_usage`]; concurrent batched increments for
/// the same tenant are serialized so no counts are lost.
pub async fn increment_usage_handler(
    State(state): State<AppState>,
    Admin(_): Admin,
    axum::extract::Path(tenant_id): axum::extract::Path<String>,
    Json(req): Json<IncrementUsageRequest>,
) -> Result<Json<IncrementUsageResponse>, (StatusCode, String)> {
    if req.count == 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            "count must be greater than zero".to_string(),
        ));
    }

    let tid =
        TenantId::new(tenant_id.clone()).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    let new_value = state
        .tenant_repo
        .increment_usage(&tid, req.meter, req.count)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Tenant not found: {tenant_id}"),
            )
        })?;

    Ok(Json(IncrementUsageResponse {
        tenant_id,
        meter: req.meter,
        used: new_value,
    }))
}

/// Set a tenant's schema-enforcement mode (admin only).
/// PUT /api/v1/tenants/:id/schema-enforcement
///
/// Gap 3 toggle: flips registered-schema validation on ingest between
/// `permissive` (default), `warn`, and `strict`. The gateway exposes this to a
/// tenant for their own tenant; Core keeps it admin-gated and internal.
pub async fn update_schema_enforcement_handler(
    State(state): State<AppState>,
    Admin(_): Admin,
    axum::extract::Path(tenant_id): axum::extract::Path<String>,
    Json(req): Json<UpdateSchemaEnforcementRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let tid =
        TenantId::new(tenant_id.clone()).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    let updated = state
        .tenant_repo
        .update_schema_enforcement(&tid, req.schema_enforcement)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !updated {
        return Err((
            StatusCode::NOT_FOUND,
            format!("Tenant not found: {tenant_id}"),
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}

/// Deactivate tenant (admin only)
/// POST /api/v1/tenants/:id/deactivate
pub async fn deactivate_tenant_handler(
    State(state): State<AppState>,
    Admin(_): Admin,
    axum::extract::Path(tenant_id): axum::extract::Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let tid =
        TenantId::new(tenant_id.clone()).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    let deactivated = state
        .tenant_repo
        .deactivate(&tid)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !deactivated {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Tenant not found: {tenant_id}"),
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}

/// Activate tenant (admin only)
/// POST /api/v1/tenants/:id/activate
pub async fn activate_tenant_handler(
    State(state): State<AppState>,
    Admin(_): Admin,
    axum::extract::Path(tenant_id): axum::extract::Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let tid =
        TenantId::new(tenant_id.clone()).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    let activated = state
        .tenant_repo
        .activate(&tid)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !activated {
        return Err((
            StatusCode::NOT_FOUND,
            format!("Tenant not found: {tenant_id}"),
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}

/// Update tenant (admin only)
/// PUT /api/v1/tenants/:id
pub async fn update_tenant_handler(
    State(state): State<AppState>,
    Admin(_): Admin,
    axum::extract::Path(tenant_id): axum::extract::Path<String>,
    Json(req): Json<UpdateTenantRequest>,
) -> Result<Json<TenantResponse>, (StatusCode, String)> {
    let tid =
        TenantId::new(tenant_id.clone()).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    let mut tenant = state
        .tenant_repo
        .find_by_id(&tid)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Tenant not found: {tenant_id}"),
            )
        })?;

    if let Some(name) = req.name {
        tenant
            .update_name(name)
            .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    }
    if let Some(desc) = req.description {
        tenant.update_description(Some(desc));
    }
    if let Some(is_demo) = req.is_demo {
        tenant.set_is_demo(is_demo);
    }
    if let Some(quotas) = req.quotas {
        tenant.update_quotas(quotas);
    }
    if let Some(metadata) = req.metadata {
        tenant.update_metadata(metadata);
    }

    state
        .tenant_repo
        .save(&tenant)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // A managed meter may have preserved a newer counter than the caller sent.
    // Return the persisted representation, not that stale request snapshot.
    let persisted = state
        .tenant_repo
        .find_by_id(&tid)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Tenant not found".to_string()))?;
    Ok(Json(TenantResponse::from_domain(&persisted)))
}

/// Delete tenant (admin only)
/// DELETE /api/v1/tenants/:id
pub async fn delete_tenant_handler(
    State(state): State<AppState>,
    Admin(_): Admin,
    axum::extract::Path(tenant_id): axum::extract::Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let tid =
        TenantId::new(tenant_id.clone()).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    let deleted = state
        .tenant_repo
        .delete(&tid)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !deleted {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Tenant not found: {tenant_id}"),
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// Helpers
// ============================================================================

/// Read a non-negative meter (`events_used` / `queries_used`) from the tenant's
/// `metadata.quotas` blob.
///
/// WHY this exists: the durable per-tenant event/query totals live in
/// `metadata.quotas.{events_used,queries_used}` — the counter the forward-metering
/// path bumps (`POST /api/v1/tenants/{id}/usage/increment`,
/// [`crate::domain::entities::UsageMeter`]) and the backfill reconciles. The
/// in-memory [`crate::domain::entities::TenantUsage`] struct's `total_events`
/// field is only ever moved by `record_event()`, which the real ingest path never
/// calls — so `usage.total_events` is structurally always 0. `/stats` must report
/// the metered counter, the same number the Query Service dashboard reads
/// (`tenant_controller.ex` `events_used`), or every downstream consumer (the admin
/// tenant list/detail event_count, fleet-health's has-data gate, cluster status)
/// reads 0 for tenants that demonstrably have events. JSON numbers arrive as
/// u64/f64 depending on the writer; accept either and treat anything else as 0.
fn metered_quota_counter(tenant: &crate::domain::entities::Tenant, field: &str) -> u64 {
    tenant
        .metadata()
        .get("quotas")
        .and_then(|q| q.get(field))
        .and_then(|v| v.as_u64().or_else(|| v.as_f64().map(|f| f.max(0.0) as u64)))
        .unwrap_or(0)
}

/// Build tenant statistics JSON (presentation concern).
fn build_tenant_stats(tenant: &crate::domain::entities::Tenant) -> serde_json::Value {
    let quotas = tenant.quotas();
    let usage = tenant.usage();

    // The authoritative lifetime totals come from the metered metadata counter,
    // NOT the in-memory TenantUsage struct (see `metered_quota_counter`). These are
    // the numbers the dashboard + admin console show.
    let events_used = metered_quota_counter(tenant, "events_used");
    let queries_used = metered_quota_counter(tenant, "queries_used");

    let events_pct = if quotas.max_events_per_day() > 0 {
        (usage.events_today() as f64 / quotas.max_events_per_day() as f64) * 100.0
    } else {
        0.0
    };

    let storage_pct = if quotas.max_storage_bytes() > 0 {
        (usage.storage_bytes() as f64 / quotas.max_storage_bytes() as f64) * 100.0
    } else {
        0.0
    };

    let queries_pct = if quotas.max_queries_per_hour() > 0 {
        (usage.queries_this_hour() as f64 / quotas.max_queries_per_hour() as f64) * 100.0
    } else {
        0.0
    };

    // Serialize the live usage struct, then overlay the real metered lifetime
    // totals so `usage.total_events` reflects the durable counter instead of the
    // always-0 in-memory field. The Control Plane's tolerant stats decoder
    // (tenant_stats.go) reads `usage.total_events` (then `event_count`), so this
    // overlay alone fixes every CP consumer with no CP change required.
    let mut usage_json = serde_json::to_value(usage).unwrap_or_else(|_| serde_json::json!({}));
    if let Some(obj) = usage_json.as_object_mut() {
        obj.insert("total_events".to_string(), serde_json::json!(events_used));
        obj.insert("queries_used".to_string(), serde_json::json!(queries_used));
    }

    serde_json::json!({
        "tenant_id": tenant.id().as_str(),
        "name": tenant.name(),
        "active": tenant.is_active(),
        "is_demo": tenant.is_demo(),
        // Flat lifetime totals — the primary fields the admin Events/Queries
        // columns want. Sourced from the durable metered counter.
        "event_count": events_used,
        "query_count": queries_used,
        "usage": usage_json,
        "quotas": tenant.quotas(),
        "utilization": {
            "events_today": {
                "used": usage.events_today(),
                "limit": quotas.max_events_per_day(),
                "percentage": events_pct.min(100.0)
            },
            "storage": {
                "used_bytes": usage.storage_bytes(),
                "limit_bytes": quotas.max_storage_bytes(),
                "percentage": storage_pct.min(100.0)
            },
            "queries_this_hour": {
                "used": usage.queries_this_hour(),
                "limit": quotas.max_queries_per_hour(),
                "percentage": queries_pct.min(100.0)
            }
        },
        "created_at": tenant.created_at(),
        "updated_at": tenant.updated_at()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn increment_request_defaults_meter_to_events() {
        // Backward compat: the pre-typing Query Service sent just `{count}`.
        // That body must still parse and target the events meter, so an old
        // QS hitting a new Core during a rolling deploy keeps metering events.
        let req: IncrementUsageRequest = serde_json::from_str(r#"{"count": 7}"#).unwrap();
        assert_eq!(req.count, 7);
        assert_eq!(req.meter, UsageMeter::Events);
        assert_eq!(req.meter.quota_field(), "events_used");
    }

    #[test]
    fn increment_request_parses_typed_events_and_queries() {
        let ev: IncrementUsageRequest =
            serde_json::from_str(r#"{"count": 3, "type": "events"}"#).unwrap();
        assert_eq!(ev.meter, UsageMeter::Events);

        let qs: IncrementUsageRequest =
            serde_json::from_str(r#"{"count": 9, "type": "queries"}"#).unwrap();
        assert_eq!(qs.meter, UsageMeter::Queries);
        assert_eq!(qs.meter.quota_field(), "queries_used");
    }

    #[test]
    fn increment_request_rejects_negative_count() {
        // `count: u64` makes serde reject negatives before the handler runs,
        // which axum surfaces as a 400 — no separate validation needed.
        let err = serde_json::from_str::<IncrementUsageRequest>(r#"{"count": -1}"#);
        assert!(err.is_err(), "negative count must fail to deserialize");
    }

    #[test]
    fn new_tenant_defaults_to_trial_quota_not_free() {
        // Prompt 048: a create with no quotas and no preset must resolve to the
        // TRIAL tier (the new-signup default), never free and never the old
        // standard fallback — so an under-specified create can't mint free.
        assert_eq!(
            resolve_create_quotas(None, None),
            TenantQuotas::trial_tier()
        );
        // An unknown preset also falls back to trial, never free.
        assert_eq!(
            resolve_create_quotas(None, Some("bogus")),
            TenantQuotas::trial_tier()
        );
        // The explicit "trial" preset resolves to the trial tier.
        assert_eq!(
            resolve_create_quotas(None, Some("trial")),
            TenantQuotas::trial_tier()
        );
    }

    #[test]
    fn free_preset_still_resolves_to_free_for_grandfathered_tenants() {
        // The free_tier() quota is RETAINED: an operator explicitly asking for
        // `quota_preset:"free"` (re-provisioning a grandfathered tenant) still
        // gets the free quota. This change stops NEW free, it does not delete it.
        assert_eq!(
            resolve_create_quotas(None, Some("free")),
            TenantQuotas::free_tier()
        );
    }

    #[test]
    fn explicit_quotas_win_over_preset_default() {
        // A caller-supplied quotas value always wins (e.g. the demo flow sending
        // unlimited), regardless of the trial default.
        let custom = TenantQuotas::unlimited();
        assert_eq!(resolve_create_quotas(Some(custom.clone()), None), custom);
    }

    #[test]
    fn increment_response_serializes_type_and_used() {
        let resp = IncrementUsageResponse {
            tenant_id: "acme".to_string(),
            meter: UsageMeter::Queries,
            used: 42,
        };
        let v = serde_json::to_value(&resp).unwrap();
        assert_eq!(v["tenant_id"], "acme");
        assert_eq!(v["type"], "queries");
        assert_eq!(v["used"], 42);
    }

    use crate::domain::{
        entities::{Tenant, TenantQuotas},
        value_objects::TenantId,
    };

    fn tenant_with_quota_usage(events_used: u64, queries_used: u64) -> Tenant {
        let mut t = Tenant::new(
            TenantId::new("acme".to_string()).unwrap(),
            "Acme".to_string(),
            TenantQuotas::free_tier(),
        )
        .unwrap();
        // Mirror exactly what the metering increment path writes:
        // metadata.quotas.{events_used,queries_used}.
        t.update_metadata(serde_json::json!({
            "quotas": { "events_used": events_used, "queries_used": queries_used }
        }));
        t
    }

    #[test]
    fn stats_event_count_reflects_metered_counter_not_inmemory_usage() {
        // The in-memory TenantUsage.total_events is never bumped by the real
        // ingest path, so /stats must report the durable metered counter from
        // metadata.quotas.events_used — the same number the QS dashboard shows.
        let tenant = tenant_with_quota_usage(257, 12);
        let stats = build_tenant_stats(&tenant);

        // Flat fields the admin Events/Queries columns prefer.
        assert_eq!(stats["event_count"], 257);
        assert_eq!(stats["query_count"], 12);
        // Overlaid into the usage block so the CP's tolerant decoder
        // (tenant_stats.go reads usage.total_events) also picks it up.
        assert_eq!(stats["usage"]["total_events"], 257);
        assert_eq!(stats["usage"]["queries_used"], 12);
    }

    #[test]
    fn stats_event_count_is_zero_when_unmetered() {
        // A tenant whose metadata has no quota counters reports 0 (guarded),
        // never a missing field or a crash.
        let tenant = Tenant::new(
            TenantId::new("empty".to_string()).unwrap(),
            "Empty".to_string(),
            TenantQuotas::free_tier(),
        )
        .unwrap();
        let stats = build_tenant_stats(&tenant);
        assert_eq!(stats["event_count"], 0);
        assert_eq!(stats["usage"]["total_events"], 0);
    }

    #[tokio::test]
    async fn stats_reflects_real_metering_path_end_to_end() {
        // The full chain the admin console depends on, in one test:
        //   increment_usage (the real QS metering path, writes
        //   metadata.quotas.events_used) -> find_by_id -> build_tenant_stats.
        // Proves the number the metering path records is the number /stats reports
        // as event_count — closing the counts=0 gap at the source.
        use crate::{
            domain::repositories::TenantRepository,
            infrastructure::repositories::InMemoryTenantRepository,
        };

        let repo = InMemoryTenantRepository::new();
        let id = TenantId::new("acme".to_string()).unwrap();
        repo.create(id.clone(), "ACME".to_string(), TenantQuotas::free_tier())
            .await
            .unwrap();

        // Meter 257 events + 12 queries exactly as the Query Service does.
        repo.increment_usage(&id, UsageMeter::Events, 257)
            .await
            .unwrap();
        repo.increment_usage(&id, UsageMeter::Queries, 12)
            .await
            .unwrap();

        let tenant = repo.find_by_id(&id).await.unwrap().unwrap();
        let stats = build_tenant_stats(&tenant);

        assert_eq!(
            stats["event_count"], 257,
            "stats must report the metered events"
        );
        assert_eq!(
            stats["query_count"], 12,
            "stats must report the metered queries"
        );
        assert_eq!(stats["usage"]["total_events"], 257);
    }

    #[test]
    fn stats_metered_counter_accepts_float_json() {
        // JSON numbers can arrive as f64 depending on the writer; the reader
        // must coerce, not drop the value to 0.
        let mut tenant = Tenant::new(
            TenantId::new("floaty".to_string()).unwrap(),
            "Floaty".to_string(),
            TenantQuotas::free_tier(),
        )
        .unwrap();
        tenant.update_metadata(serde_json::json!({
            "quotas": { "events_used": 5.0, "queries_used": 3.0 }
        }));
        let stats = build_tenant_stats(&tenant);
        assert_eq!(stats["event_count"], 5);
        assert_eq!(stats["query_count"], 3);
    }
}
