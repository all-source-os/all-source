//! Event search routes: `GET /api/v1/search/semantic` and
//! `POST /api/v1/search/hybrid`.
//!
//! Mounted inside the authenticated v1 router. The tenant is always the
//! credential's; a `tenant_id` in the query or body is accepted only when it
//! names that same tenant.

use crate::infrastructure::{
    search::{
        EventSearch, EventSearchError, EventSearchHit, HybridSearchParams, SemanticSearchParams,
    },
    security::{auth::Permission, middleware::Authenticated},
};
use axum::{
    Json, Router,
    extract::{
        Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tokio::sync::Notify;

pub const SEMANTIC_PATH: &str = "/api/v1/search/semantic";
pub const HYBRID_PATH: &str = "/api/v1/search/hybrid";

const MAINTENANCE_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Clone)]
pub struct SearchApiState {
    search: Option<Arc<EventSearch>>,
    demand: Arc<Notify>,
}

impl SearchApiState {
    /// State for an [`EventSearch`] whose embedder and indexing the caller drives.
    pub fn new(search: Option<Arc<EventSearch>>) -> Self {
        Self {
            search,
            demand: Arc::new(Notify::new()),
        }
    }
}

pub fn router<S>(state: SearchApiState) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route(SEMANTIC_PATH, get(semantic))
        .route(HYBRID_PATH, post(hybrid))
        .with_state(state)
}

/// Builds event search over the live store. On the first search request a
/// background task starts keeping the index caught up with ingest. With
/// `prime-vectors` it first loads the embedding model; without it, or when the
/// model fails to load, hybrid search answers keyword-only and semantic search
/// answers 503.
pub fn start(store: Arc<crate::store::EventStore>) -> SearchApiState {
    let search = match EventSearch::new(store) {
        Ok(search) => search,
        Err(e) => {
            tracing::warn!("event search disabled: {e}");
            return SearchApiState::new(None);
        }
    };
    let state = SearchApiState::new(Some(Arc::clone(&search)));

    let demand = Arc::clone(&state.demand);
    tokio::spawn(async move {
        demand.notified().await;
        #[cfg(feature = "prime-vectors")]
        match tokio::task::spawn_blocking(crate::prime::vectors::TextEmbedder::new).await {
            Ok(Ok(embedder)) => search.set_embedder(Arc::new(embedder)),
            Ok(Err(e)) => {
                tracing::warn!("event search is keyword-only: embedding model failed to load: {e}");
            }
            Err(e) => {
                tracing::warn!("event search is keyword-only: embedding model load panicked: {e}");
            }
        }
        loop {
            if let Err(e) = search.catch_up().await {
                tracing::warn!("event search indexing failed: {e}");
            }
            tokio::time::sleep(MAINTENANCE_INTERVAL).await;
        }
    });

    state
}

#[derive(Debug, Deserialize)]
struct SemanticQuery {
    query: String,
    limit: Option<usize>,
    event_type: Option<String>,
    min_similarity: Option<f32>,
    tenant_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct HybridBody {
    query: String,
    limit: Option<usize>,
    event_type: Option<String>,
    entity_id: Option<String>,
    semantic_weight: Option<f32>,
    keyword_weight: Option<f32>,
    tenant_id: Option<String>,
}

async fn semantic(
    State(state): State<SearchApiState>,
    auth: Authenticated,
    query: Result<Query<SemanticQuery>, QueryRejection>,
) -> Response {
    let Query(query) = match query {
        Ok(q) => q,
        Err(rejection) => return error(StatusCode::BAD_REQUEST, &rejection.body_text()),
    };
    let (search, tenant) = match admit(&state, &auth, query.tenant_id.as_deref()) {
        Ok(admitted) => admitted,
        Err((status, message)) => return error(status, message),
    };
    let params = SemanticSearchParams {
        query: query.query,
        limit: query.limit,
        event_type: query.event_type,
        min_similarity: query.min_similarity,
    };
    respond(search.semantic(&tenant, params).await, |h| {
        json!({
            "event_id": h.event_id,
            "score": h.score,
            "event_type": h.event_type,
            "entity_id": h.entity_id,
            "timestamp": h.timestamp,
        })
    })
}

async fn hybrid(
    State(state): State<SearchApiState>,
    auth: Authenticated,
    body: Result<Json<HybridBody>, JsonRejection>,
) -> Response {
    let Json(body) = match body {
        Ok(b) => b,
        Err(rejection) => return error(StatusCode::BAD_REQUEST, &rejection.body_text()),
    };
    let (search, tenant) = match admit(&state, &auth, body.tenant_id.as_deref()) {
        Ok(admitted) => admitted,
        Err((status, message)) => return error(status, message),
    };
    let params = HybridSearchParams {
        query: body.query,
        limit: body.limit,
        event_type: body.event_type,
        entity_id: body.entity_id,
        semantic_weight: body.semantic_weight,
        keyword_weight: body.keyword_weight,
    };
    respond(search.hybrid(&tenant, params), |h| {
        json!({
            "event_id": h.event_id,
            "score": h.score,
            "semantic_score": h.semantic_score,
            "keyword_score": h.keyword_score,
            "event_type": h.event_type,
            "entity_id": h.entity_id,
            "timestamp": h.timestamp,
        })
    })
}

fn admit(
    state: &SearchApiState,
    auth: &Authenticated,
    claimed_tenant: Option<&str>,
) -> Result<(Arc<EventSearch>, String), (StatusCode, &'static str)> {
    let ctx = &auth.0;
    if ctx.require_permission(Permission::Read).is_err() {
        return Err((
            StatusCode::FORBIDDEN,
            "the credential lacks read permission",
        ));
    }
    let tenant = ctx.tenant_id();
    if tenant.trim().is_empty() {
        return Err((StatusCode::UNAUTHORIZED, "the credential carries no tenant"));
    }
    if claimed_tenant.is_some_and(|claimed| claimed != tenant) {
        return Err((
            StatusCode::BAD_REQUEST,
            "tenant_id does not match the authenticated tenant",
        ));
    }
    state.demand.notify_one();
    let Some(search) = state.search.clone() else {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "event search failed to start in this Core",
        ));
    };
    Ok((search, tenant.to_string()))
}

fn respond(
    result: Result<Vec<EventSearchHit>, EventSearchError>,
    render: impl Fn(&EventSearchHit) -> Value,
) -> Response {
    match result {
        Ok(hits) => {
            let results: Vec<Value> = hits.iter().map(render).collect();
            let count = results.len();
            Json(json!({ "results": results, "count": count })).into_response()
        }
        Err(EventSearchError::Unavailable(m)) => error(StatusCode::SERVICE_UNAVAILABLE, &m),
        Err(EventSearchError::InvalidInput(m)) => error(StatusCode::BAD_REQUEST, &m),
        Err(EventSearchError::Internal(m)) => {
            tracing::error!("event search failed: {m}");
            error(StatusCode::INTERNAL_SERVER_ERROR, &m)
        }
    }
}

fn error(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

#[cfg(test)]
#[path = "search_api_tests.rs"]
mod search_api_tests;
