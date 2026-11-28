use super::{api_v1::AppState, archive_work};
use crate::{
    application::dto::EventDto,
    error::{AllSourceError, Result},
    infrastructure::security::middleware::Admin,
    store::{EventStore, history::HistorySelection},
};
use axum::{
    Extension, Json, Router,
    extract::{FromRequestParts, Query, State},
    http::{StatusCode, header, request::Parts},
    routing::get,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryRequest {
    tenant_id: String,
    entity_id: Option<String>,
    since: Option<DateTime<Utc>>,
    until: Option<DateTime<Utc>>,
    event_id: Option<Uuid>,
}

#[derive(Serialize)]
pub struct HistoryResponse {
    protocol: &'static str,
    tenant_id: String,
    entity_id: Option<String>,
    since: Option<DateTime<Utc>>,
    until: Option<DateTime<Utc>>,
    event_id: Option<Uuid>,
    read_point: DateTime<Utc>,
    coverage: &'static str,
    truncated: bool,
    events: Vec<EventDto>,
}

type Private<T> = ([(header::HeaderName, &'static str); 1], Json<T>);

#[derive(Clone)]
pub struct HistoryGatewaySubject(pub String);

pub struct HistoryGateway;

pub(super) fn trace_span<B>(request: &axum::http::Request<B>) -> tracing::Span {
    use crate::infrastructure::security::middleware::is_event_history_path;
    use tower_http::trace::{DefaultMakeSpan, MakeSpan};
    if is_event_history_path(request.uri().path()) {
        tracing::debug_span!("request", method = %request.method(), path = request.uri().path())
    } else {
        DefaultMakeSpan::new().make_span(request)
    }
}

impl<S: Send + Sync> FromRequestParts<S> for HistoryGateway {
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> std::result::Result<Self, Self::Rejection> {
        let Admin(auth) = Admin::from_request_parts(parts, state).await?;
        let allowed = parts
            .extensions
            .get::<HistoryGatewaySubject>()
            .is_some_and(|pin| !pin.0.is_empty() && pin.0 == auth.claims.sub);
        if allowed {
            Ok(Self)
        } else {
            Err((StatusCode::FORBIDDEN, "History gateway required"))
        }
    }
}

pub fn routes(subject: Option<String>) -> Router<AppState> {
    Router::new()
        .route("/api/v1/events/history", get(history))
        .route("/api/v1/events/history/workspaces", get(workspaces))
        .layer(Extension(HistoryGatewaySubject(
            subject.unwrap_or_default(),
        )))
}

pub async fn history(
    _: HistoryGateway,
    State(store): State<Arc<EventStore>>,
    Query(req): Query<HistoryRequest>,
) -> Result<Private<HistoryResponse>> {
    let selection = match (&req.entity_id, req.since, req.until, req.event_id) {
        (Some(entity_id), Some(since), Some(until), None) => HistorySelection::Timeline {
            entity_id: entity_id.clone(),
            since,
            until,
        },
        (None, None, None, Some(id)) => HistorySelection::Evidence(id),
        _ => {
            return Err(AllSourceError::InvalidInput(
                "Specify an entity with both time bounds, or one event ID".into(),
            ));
        }
    };
    let page = archive_work::history(store, req.tenant_id.clone(), selection).await?;
    Ok((
        [(header::CACHE_CONTROL, "private, no-store")],
        Json(HistoryResponse {
            protocol: "retained-history-v1",
            tenant_id: req.tenant_id,
            entity_id: req.entity_id,
            since: req.since,
            until: req.until,
            event_id: req.event_id,
            read_point: page.read_point,
            coverage: if page.certified {
                "complete"
            } else {
                "partial"
            },
            truncated: page.truncated,
            events: page.events,
        }),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WorkspaceQuery {
    subject_id: String,
}

#[derive(Serialize)]
pub(super) struct WorkspaceCandidates {
    tenant_ids: Vec<String>,
}

pub(super) async fn workspaces(
    _: HistoryGateway,
    State(state): State<AppState>,
    Query(query): Query<WorkspaceQuery>,
) -> Result<Private<WorkspaceCandidates>> {
    if query.subject_id.is_empty()
        || query.subject_id.len() > 256
        || query.subject_id.chars().any(char::is_control)
    {
        return Err(AllSourceError::InvalidInput(
            "Invalid workspace subject".into(),
        ));
    }
    let repo = state
        .service_container
        .config_repository()
        .ok_or_else(|| AllSourceError::StorageError("Workspace directory unavailable".into()))?;
    let tenant_ids = repo.membership_candidates(&query.subject_id);
    Ok((
        [(header::CACHE_CONTROL, "private, no-store")],
        Json(WorkspaceCandidates { tenant_ids }),
    ))
}
