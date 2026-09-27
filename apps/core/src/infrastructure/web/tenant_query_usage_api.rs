//! Administrative durable query admission; never a source-access capability.
use crate::{
    domain::{entities::query_usage::*, value_objects::TenantId},
    error::AllSourceError,
    infrastructure::{security::middleware::Admin, web::api_v1::AppState},
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde_json::{Value, json};
use tokio::sync::Semaphore;

// The authoritative leader bounds work across all Query Service replicas.
// No wait queue: retries retain the same operation ID after a busy response.
static USAGE_REQUESTS: Semaphore = Semaphore::const_new(16);

type Response = (StatusCode, Json<Value>);

pub async fn query_usage_handler(
    State(state): State<AppState>,
    Admin(_): Admin,
    Path(tenant): Path<String>,
) -> Response {
    let Ok(_permit) = USAGE_REQUESTS.try_acquire() else {
        return busy();
    };
    let Ok(id) = TenantId::new(tenant) else {
        return invalid();
    };
    match state.tenant_repo.get_query_usage(&id).await {
        Ok(Some(snapshot)) => (
            StatusCode::OK,
            Json(json!({"protocol": "canonical-query-usage-v1", "snapshot": snapshot})),
        ),
        Ok(None) => missing(),
        Err(error) => failure(&error),
    }
}

pub async fn admit_query_usage_handler(
    State(state): State<AppState>,
    Admin(_): Admin,
    Path(tenant): Path<String>,
    Json(request): Json<QueryUsageRequest>,
) -> Response {
    let Ok(_permit) = USAGE_REQUESTS.try_acquire() else {
        return busy();
    };
    let Ok(id) = TenantId::new(tenant) else {
        return invalid();
    };
    match state.tenant_repo.admit_query_usage(&id, request).await {
        Ok(Some(QueryUsageDecision::Admitted { receipt, replayed })) => (
            StatusCode::OK,
            Json(
                json!({"protocol": "canonical-query-usage-v1", "status": "admitted", "receipt": receipt, "replayed": replayed}),
            ),
        ),
        Ok(Some(QueryUsageDecision::Denied(reason))) => denied(reason),
        Ok(None) => missing(),
        Err(error) => failure(&error),
    }
}

pub async fn reset_query_usage_handler(
    State(state): State<AppState>,
    Admin(_): Admin,
    Path(tenant): Path<String>,
    Json(request): Json<QueryUsageReset>,
) -> Response {
    let Ok(_permit) = USAGE_REQUESTS.try_acquire() else {
        return busy();
    };
    let Ok(id) = TenantId::new(tenant) else {
        return invalid();
    };
    match state.tenant_repo.reset_query_usage(&id, request).await {
        Ok(Some(QueryUsageResetDecision::Reset { receipt, replayed })) => (
            StatusCode::OK,
            Json(
                json!({"protocol": "canonical-query-usage-v1", "status": "reset", "receipt": receipt, "replayed": replayed}),
            ),
        ),
        Ok(Some(QueryUsageResetDecision::Denied(reason))) => denied(reason),
        Ok(None) => missing(),
        Err(error) => failure(&error),
    }
}

fn denied(reason: QueryUsageDenial) -> Response {
    let status = match reason {
        QueryUsageDenial::QuotaExceeded => StatusCode::PAYMENT_REQUIRED,
        QueryUsageDenial::OperationConflict | QueryUsageDenial::PeriodChanged => {
            StatusCode::CONFLICT
        }
        QueryUsageDenial::ExpiredOperation => StatusCode::GONE,
        QueryUsageDenial::ReceiptCapacity => StatusCode::TOO_MANY_REQUESTS,
        QueryUsageDenial::InactiveTenant => StatusCode::FORBIDDEN,
        QueryUsageDenial::InvalidQuotaMetadata => StatusCode::SERVICE_UNAVAILABLE,
    };
    (status, Json(json!({"error": reason})))
}

fn missing() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(json!({"error": "tenant_not_found"})),
    )
}

fn busy() -> Response {
    (
        StatusCode::TOO_MANY_REQUESTS,
        Json(json!({"error": "query_usage_busy"})),
    )
}

fn invalid() -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({"error": "invalid_query_usage_request"})),
    )
}

fn failure(error: &AllSourceError) -> Response {
    if matches!(error, AllSourceError::ValidationError(_)) {
        invalid()
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error": "query_usage_unavailable"})),
        )
    }
}
