use crate::{
    domain::value_objects::TenantId,
    error::AllSourceError,
    infrastructure::security::{
        auth::Permission,
        middleware::{AuthContext, Authenticated},
    },
    store::{
        EventStore,
        feed::FeedCursor,
        read_model::ModelCommit,
        view::{VIEWS_INDEX, is_reserved_read_model},
    },
};
use axum::{
    Json,
    extract::{Path, Query, State},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::{Semaphore, SemaphorePermit};

static REQUESTS: Semaphore = Semaphore::const_new(8);
static QUEUED: Semaphore = Semaphore::const_new(64);

pub(super) fn protect_ingest(event_type: &str, entity_id: &str) -> crate::error::Result<()> {
    if event_type == crate::store::feed::MODEL_EVENT || entity_id.starts_with("read-model:") {
        return Err(AllSourceError::InvalidInput(
            "read model journals require the read-model commit API".into(),
        ));
    }
    Ok(())
}

pub enum ModelError {
    Busy,
    Forbidden(&'static str),
    Core(AllSourceError),
}
type Result<T> = std::result::Result<T, ModelError>;

impl From<AllSourceError> for ModelError {
    fn from(error: AllSourceError) -> Self {
        Self::Core(error)
    }
}
impl From<serde_json::Error> for ModelError {
    fn from(error: serde_json::Error) -> Self {
        Self::Core(error.into())
    }
}
impl IntoResponse for ModelError {
    fn into_response(self) -> Response {
        match self {
            Self::Core(error) => error.into_response(),
            Self::Forbidden(code) => (
                axum::http::StatusCode::FORBIDDEN,
                Json(json!({"error": code})),
            )
                .into_response(),
            Self::Busy => (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                [(axum::http::header::RETRY_AFTER, "1")],
                Json(
                    json!({"error":"read_model_busy","message":"Read model service busy; retry."}),
                ),
            )
                .into_response(),
        }
    }
}

pub(super) async fn slot() -> Result<SemaphorePermit<'static>> {
    let queued = QUEUED.try_acquire().map_err(|_| ModelError::Busy)?;
    let permit = tokio::time::timeout(std::time::Duration::from_secs(2), REQUESTS.acquire())
        .await
        .map_err(|_| ModelError::Busy)?
        .map_err(|_| ModelError::Busy)?;
    drop(queued);
    Ok(permit)
}

/// The gateway mints per-request tokens carrying the caller's own role, so a
/// tenant's admin reaches Core with `Permission::Admin`; only an admin of the
/// `system` tenant is a service principal.
pub(super) fn is_admin(auth: &AuthContext) -> bool {
    auth.claims.has_permission(Permission::Admin) && auth.tenant_id() == "system"
}

/// The tenant a request reads or writes. Only the service principal (the Query
/// Service folding for a tenant) may name a tenant other than its own.
pub(super) fn acting_tenant(auth: &AuthContext, requested: Option<String>) -> Result<String> {
    match requested {
        Some(tenant) if tenant != auth.tenant_id() => {
            if !is_admin(auth) {
                return Err(ModelError::Forbidden("tenant_forbidden"));
            }
            TenantId::new(tenant.clone())?;
            Ok(tenant)
        }
        _ => Ok(auth.tenant_id().to_string()),
    }
}

#[derive(Deserialize)]
pub struct FeedQuery {
    pub epoch: Option<u64>,
    pub position: Option<usize>,
    pub limit: Option<usize>,
    pub prefixes: Option<String>,
    pub entity_id: Option<String>,
    pub tenant_id: Option<String>,
}

#[derive(Deserialize)]
pub struct ModelQuery {
    #[serde(default)]
    pub prefix: String,
    pub after: Option<String>,
    pub limit: Option<usize>,
    pub revision: Option<u64>,
    pub tenant_id: Option<String>,
}

#[derive(Deserialize)]
pub struct CommitQuery {
    pub tenant_id: Option<String>,
}

pub async fn feed(
    State(store): State<Arc<EventStore>>,
    Authenticated(auth): Authenticated,
    Query(query): Query<FeedQuery>,
) -> Result<Json<Value>> {
    auth.require_permission(Permission::Read)?;
    let tenant = acting_tenant(&auth, query.tenant_id.clone())?;
    let permit = slot().await?;
    let cursor = match (query.epoch, query.position) {
        (Some(epoch), Some(position)) => Some(FeedCursor { epoch, position }),
        (None, None) => None,
        _ => {
            return Err(AllSourceError::InvalidInput(
                "feed cursor requires epoch and position".into(),
            )
            .into());
        }
    };
    let prefixes: Vec<String> = query
        .prefixes
        .unwrap_or_default()
        .split(',')
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();
    if prefixes.len() > 8 || prefixes.iter().any(|s| s.len() > 128) {
        return Err(AllSourceError::InvalidInput(
            "feed permits at most 8 prefixes of 128 bytes".into(),
        )
        .into());
    }
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        store.tenant_feed_scoped(
            &tenant,
            cursor,
            query.limit.unwrap_or(100),
            &prefixes,
            query.entity_id.as_deref(),
        )
    })
    .await
    .map_err(|e| AllSourceError::StorageError(e.to_string()))??;
    Ok(Json(serde_json::to_value(result)?))
}

pub async fn read(
    State(store): State<Arc<EventStore>>,
    Authenticated(auth): Authenticated,
    Path(name): Path<String>,
    Query(query): Query<ModelQuery>,
) -> Result<Json<Value>> {
    auth.require_permission(Permission::Read)?;
    if name == VIEWS_INDEX && !is_admin(&auth) {
        return Err(ModelError::Forbidden("reserved_read_model"));
    }
    let tenant = acting_tenant(&auth, query.tenant_id.clone())?;
    let permit = slot().await?;
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        store.read_model_page(
            &tenant,
            &name,
            &query.prefix,
            query.after.as_deref(),
            query.limit.unwrap_or(100),
            query.revision,
        )
    })
    .await
    .map_err(|e| AllSourceError::StorageError(e.to_string()))??;
    Ok(Json(serde_json::to_value(result)?))
}

pub async fn commit(
    State(store): State<Arc<EventStore>>,
    Authenticated(auth): Authenticated,
    Path(name): Path<String>,
    Query(query): Query<CommitQuery>,
    Json(commit): Json<ModelCommit>,
) -> Result<Json<Value>> {
    auth.require_permission(Permission::Write)?;
    if is_reserved_read_model(&name) && !is_admin(&auth) {
        return Err(ModelError::Forbidden("reserved_read_model"));
    }
    let tenant = acting_tenant(&auth, query.tenant_id)?;
    let permit = slot().await?;
    let revision = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        store.commit_read_model(&tenant, &name, commit)
    })
    .await
    .map_err(|e| AllSourceError::StorageError(e.to_string()))??;
    Ok(Json(json!({"revision": revision})))
}
