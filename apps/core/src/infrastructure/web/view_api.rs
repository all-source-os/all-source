//! Views: tenant-defined read models folded by the Query Service (ADR-021).
//! These handlers store definitions and serve rows; they never fold.

use super::{
    conditional::conditional_json,
    read_model_api::{ModelError, is_admin, slot},
};
use crate::{
    error::AllSourceError,
    infrastructure::security::{auth::Permission, middleware::Authenticated},
    store::{
        EventStore,
        view::{self, ViewError},
    },
};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};

const MAX_WAIT_SECS: u64 = 20;

pub enum ViewApiError {
    Disabled,
    Forbidden(&'static str),
    Model(ModelError),
    View(ViewError),
}

impl From<ModelError> for ViewApiError {
    fn from(error: ModelError) -> Self {
        Self::Model(error)
    }
}
impl From<ViewError> for ViewApiError {
    fn from(error: ViewError) -> Self {
        Self::View(error)
    }
}
impl From<AllSourceError> for ViewApiError {
    fn from(error: AllSourceError) -> Self {
        Self::View(ViewError::Core(error))
    }
}
impl From<serde_json::Error> for ViewApiError {
    fn from(error: serde_json::Error) -> Self {
        Self::View(ViewError::Core(error.into()))
    }
}

impl IntoResponse for ViewApiError {
    fn into_response(self) -> Response {
        match self {
            Self::Disabled => (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "views_disabled"})),
            )
                .into_response(),
            Self::Forbidden(code) => {
                (StatusCode::FORBIDDEN, Json(json!({"error": code}))).into_response()
            }
            Self::Model(error) => error.into_response(),
            Self::View(ViewError::Invalid(detail)) => (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "invalid_view", "detail": detail})),
            )
                .into_response(),
            Self::View(ViewError::NotFound) => (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "view_not_found"})),
            )
                .into_response(),
            Self::View(ViewError::Reset { generation }) => (
                StatusCode::CONFLICT,
                Json(json!({"error": "view_reset", "generation": generation})),
            )
                .into_response(),
            Self::View(ViewError::Core(error)) => error.into_response(),
        }
    }
}

type Result<T> = std::result::Result<T, ViewApiError>;

fn enabled() -> Result<()> {
    if view::views_enabled() {
        Ok(())
    } else {
        Err(ViewApiError::Disabled)
    }
}

/// Runs a store read or write on the blocking pool under a read-model slot.
async fn blocking<T, F>(work: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce() -> std::result::Result<T, ViewError> + Send + 'static,
{
    let permit = slot().await?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        work()
    })
    .await
    .map_err(|e| AllSourceError::StorageError(e.to_string()))?
    .map_err(ViewApiError::from)
}

#[derive(Deserialize)]
pub struct EntitiesQuery {
    #[serde(default)]
    pub prefix: String,
    pub after: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Deserialize)]
pub struct ChangesQuery {
    pub after: Option<u64>,
    pub limit: Option<usize>,
    pub wait: Option<u64>,
}

#[derive(Deserialize)]
pub struct AdminViewsQuery {
    pub after: Option<String>,
    pub limit: Option<usize>,
    pub revision: Option<u64>,
}

pub async fn put_view(
    State(store): State<Arc<EventStore>>,
    Authenticated(auth): Authenticated,
    Path(name): Path<String>,
    Json(definition): Json<Value>,
) -> Result<Json<Value>> {
    enabled()?;
    auth.require_permission(Permission::Write)?;
    let tenant = auth.tenant_id().to_string();
    let info = blocking(move || store.define_view(&tenant, &name, &definition)).await?;
    Ok(Json(serde_json::to_value(info)?))
}

pub async fn get_view(
    State(store): State<Arc<EventStore>>,
    Authenticated(auth): Authenticated,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> Result<Response> {
    enabled()?;
    auth.require_permission(Permission::Read)?;
    let tenant = auth.tenant_id().to_string();
    let info = blocking(move || store.view_info(&tenant, &name)).await?;
    Ok(conditional_json(&headers, &info)?)
}

pub async fn delete_view(
    State(store): State<Arc<EventStore>>,
    Authenticated(auth): Authenticated,
    Path(name): Path<String>,
) -> Result<Json<Value>> {
    enabled()?;
    auth.require_permission(Permission::Write)?;
    let tenant = auth.tenant_id().to_string();
    let view = name.clone();
    blocking(move || store.delete_view(&tenant, &view)).await?;
    Ok(Json(json!({"view": name, "deleted": true})))
}

pub async fn list_views(
    State(store): State<Arc<EventStore>>,
    Authenticated(auth): Authenticated,
) -> Result<Json<Value>> {
    enabled()?;
    auth.require_permission(Permission::Read)?;
    let tenant = auth.tenant_id().to_string();
    let views = blocking(move || store.list_views(&tenant)).await?;
    Ok(Json(json!({"views": views})))
}

pub async fn get_entity(
    State(store): State<Arc<EventStore>>,
    Authenticated(auth): Authenticated,
    Path((name, key)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Response> {
    enabled()?;
    auth.require_permission(Permission::Read)?;
    let tenant = auth.tenant_id().to_string();
    let (view, row_key) = (name.clone(), key.clone());
    let read = blocking(move || store.view_row(&tenant, &view, &row_key)).await?;
    let Some((state, version)) = read.row else {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "not_found", "ready": read.meta.ready})),
        )
            .into_response());
    };
    let body = json!({
        "view": name,
        "key": key,
        "state": state,
        "version": version,
        "revision": read.meta.revision,
        "generation": read.generation,
        "ready": read.meta.ready,
    });
    Ok(conditional_json(&headers, &body)?)
}

pub async fn list_entities(
    State(store): State<Arc<EventStore>>,
    Authenticated(auth): Authenticated,
    Path(name): Path<String>,
    Query(query): Query<EntitiesQuery>,
    headers: HeaderMap,
) -> Result<Response> {
    enabled()?;
    auth.require_permission(Permission::Read)?;
    let tenant = auth.tenant_id().to_string();
    let view = name.clone();
    let (generation, page) = blocking(move || {
        store.view_rows(
            &tenant,
            &view,
            &query.prefix,
            query.after.as_deref(),
            query.limit.unwrap_or(100),
        )
    })
    .await?;
    let body = json!({
        "view": name,
        "rows": page.rows,
        "next": page.next,
        "revision": page.meta.revision,
        "content_revision": page.content_revision,
        "generation": generation,
        "ready": page.meta.ready,
    });
    Ok(conditional_json(&headers, &body)?)
}

/// With `wait`, an empty answer is held until a commit lands on the view or
/// the wait elapses. The held request owns no read-model slot: it takes one
/// only for each read.
pub async fn changes(
    State(store): State<Arc<EventStore>>,
    Authenticated(auth): Authenticated,
    Path(name): Path<String>,
    Query(query): Query<ChangesQuery>,
) -> Result<Json<Value>> {
    enabled()?;
    auth.require_permission(Permission::Read)?;
    let tenant = auth.tenant_id().to_string();
    let after = query.after.unwrap_or(0);
    let limit = query.limit.unwrap_or(100);
    let deadline = tokio::time::Instant::now()
        + Duration::from_secs(query.wait.unwrap_or(0).min(MAX_WAIT_SECS));
    let mut watch = store.view_watch(&tenant, &name);
    loop {
        let (reader, owner, view) = (store.clone(), tenant.clone(), name.clone());
        let (generation, page) =
            blocking(move || reader.view_changes(&owner, &view, after, limit)).await?;
        if !page.changes.is_empty() || tokio::time::Instant::now() >= deadline {
            return Ok(Json(json!({
                "view": name,
                "changes": page.changes,
                "revision": page.revision,
                "caught_up": page.caught_up,
                "generation": generation,
            })));
        }
        match tokio::time::timeout_at(deadline, watch.changed()).await {
            Ok(true) | Err(_) => {}
            Ok(false) => tokio::time::sleep_until(deadline).await,
        }
    }
}

/// Every tenant's definitions, for the Query Service folder.
pub async fn admin_views(
    State(store): State<Arc<EventStore>>,
    Authenticated(auth): Authenticated,
    Query(query): Query<AdminViewsQuery>,
) -> Result<Json<Value>> {
    enabled()?;
    if !is_admin(&auth) {
        return Err(ViewApiError::Forbidden("admin_required"));
    }
    let page = blocking(move || {
        store
            .view_index_page(
                query.after.as_deref(),
                query.limit.unwrap_or(500),
                query.revision,
            )
            .map_err(ViewError::from)
    })
    .await?;
    let views: Vec<&Value> = page.rows.values().collect();
    Ok(Json(json!({
        "views": views,
        "next": page.next,
        "revision": page.revision,
        "content_revision": page.content_revision,
        "unchanged": page.unchanged,
    })))
}
