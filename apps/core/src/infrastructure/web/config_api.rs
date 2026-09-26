use crate::{
    error::{AllSourceError, Result},
    infrastructure::{
        repositories::event_sourced_config_repository::ConfigCondition,
        security::middleware::Admin, web::api_v1::AppState,
    },
};
use axum::{Json, extract::State, http::StatusCode};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct SetConfigRequest {
    pub key: String,
    pub value: serde_json::Value,
    pub changed_by: Option<String>,
    pub condition: Option<ConfigCondition>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateConfigRequest {
    pub value: serde_json::Value,
    pub changed_by: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ConfigEntryResponse {
    pub key: String,
    pub value: serde_json::Value,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<String>,
    pub revision: uuid::Uuid,
}

#[derive(Debug, Serialize)]
pub struct ListConfigsResponse {
    pub configs: Vec<ConfigEntryResponse>,
    pub total: usize,
}

// ============================================================================
// Handlers
// ============================================================================

/// List all config entries
/// GET /api/v1/config
pub async fn list_configs(
    State(state): State<AppState>,
    Admin(_): Admin,
) -> Result<Json<ListConfigsResponse>> {
    let config_repo = state
        .service_container
        .config_repository()
        .ok_or_else(|| AllSourceError::InternalError("Config repository not configured".into()))?;

    let entries = config_repo.list();
    let total = entries.len();

    let configs: Vec<ConfigEntryResponse> = entries
        .into_iter()
        .map(|e| ConfigEntryResponse {
            key: e.key,
            value: e.value,
            updated_at: e.updated_at,
            updated_by: e.updated_by,
            revision: e.revision,
        })
        .collect();

    Ok(Json(ListConfigsResponse { configs, total }))
}

/// Get a config entry by key
/// GET /api/v1/config/:key
pub async fn get_config(
    State(state): State<AppState>,
    Admin(_): Admin,
    axum::extract::Path(key): axum::extract::Path<String>,
) -> Result<Json<ConfigEntryResponse>> {
    let config_repo = state
        .service_container
        .config_repository()
        .ok_or_else(|| AllSourceError::InternalError("Config repository not configured".into()))?;

    let entry = config_repo
        .get(&key)
        .ok_or_else(|| AllSourceError::EntityNotFound(format!("Config key not found: {key}")))?;

    Ok(Json(ConfigEntryResponse {
        key: entry.key,
        value: entry.value,
        updated_at: entry.updated_at,
        updated_by: entry.updated_by,
        revision: entry.revision,
    }))
}

/// Set a config entry (upsert)
/// POST /api/v1/config
pub async fn set_config(
    State(state): State<AppState>,
    Admin(_): Admin,
    Json(req): Json<SetConfigRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>)> {
    let config_repo = state
        .service_container
        .config_repository()
        .ok_or_else(|| AllSourceError::InternalError("Config repository not configured".into()))?;

    if req.key.is_empty() {
        return Err(AllSourceError::InvalidInput(
            "Config key cannot be empty".into(),
        ));
    }

    let entry = config_repo.set_conditionally(
        &req.key,
        req.value,
        req.changed_by.as_deref(),
        req.condition.as_ref(),
    )?;

    tracing::debug!("Config set: {}", req.key);

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "key": req.key,
            "saved": true,
            "revision": entry.revision,
        })),
    ))
}

/// Set a config entry with a required atomic precondition.
/// POST /api/v1/config/conditional/set
/// Conditional clients use this distinct route so older servers cannot silently
/// ignore an unknown condition field and perform an unconditional upsert.
pub async fn set_config_conditionally(
    state: State<AppState>,
    admin: Admin,
    Json(req): Json<SetConfigRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>)> {
    if req.condition.is_none() {
        return Err(AllSourceError::InvalidInput(
            "A configuration condition is required".into(),
        ));
    }
    set_config(state, admin, Json(req)).await
}

/// Update a config entry
/// PUT /api/v1/config/:key
pub async fn update_config(
    State(state): State<AppState>,
    Admin(_): Admin,
    axum::extract::Path(key): axum::extract::Path<String>,
    Json(req): Json<UpdateConfigRequest>,
) -> Result<Json<ConfigEntryResponse>> {
    let config_repo = state
        .service_container
        .config_repository()
        .ok_or_else(|| AllSourceError::InternalError("Config repository not configured".into()))?;

    // Verify key exists
    if config_repo.get(&key).is_none() {
        return Err(AllSourceError::EntityNotFound(format!(
            "Config key not found: {key}"
        )));
    }

    config_repo.set(&key, req.value.clone(), req.changed_by.as_deref())?;

    let entry = config_repo.get(&key).ok_or_else(|| {
        AllSourceError::InternalError("Config entry disappeared after update".into())
    })?;

    tracing::debug!("Config updated: {}", key);

    Ok(Json(ConfigEntryResponse {
        key: entry.key,
        value: entry.value,
        updated_at: entry.updated_at,
        updated_by: entry.updated_by,
        revision: entry.revision,
    }))
}

/// Delete a config entry
/// DELETE /api/v1/config/:key
pub async fn delete_config(
    State(state): State<AppState>,
    Admin(_): Admin,
    axum::extract::Path(key): axum::extract::Path<String>,
) -> Result<StatusCode> {
    let config_repo = state
        .service_container
        .config_repository()
        .ok_or_else(|| AllSourceError::InternalError("Config repository not configured".into()))?;

    let deleted = config_repo.delete(&key, None)?;

    if !deleted {
        return Err(AllSourceError::EntityNotFound(format!(
            "Config key not found: {key}"
        )));
    }

    tracing::debug!("Config deleted: {}", key);

    Ok(StatusCode::NO_CONTENT)
}
