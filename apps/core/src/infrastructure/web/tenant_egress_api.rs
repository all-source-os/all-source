use crate::{
    domain::{entities::egress::*, value_objects::TenantId},
    infrastructure::{security::middleware::Admin, web::api_v1::AppState},
};
use axum::{
    Json,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use serde_json::json;
use tokio::sync::Semaphore;

static REQUESTS: Semaphore = Semaphore::const_new(16);

pub async fn mutate(
    State(state): State<AppState>,
    Admin(_): Admin,
    Path(tenant): Path<String>,
    Json(command): Json<EgressCommand>,
) -> Response {
    let Ok(_permit) = REQUESTS.try_acquire() else {
        return denial(EgressDenial::Busy);
    };
    let Ok(id) = TenantId::new(tenant) else {
        return denial(EgressDenial::Invalid);
    };
    match state.tenant_repo.mutate_egress(&id, command).await {
        Ok(Some(Ok(admission))) => Json(json!({
            "protocol": "funded-egress-v1", "admission": admission,
        }))
        .into_response(),
        Ok(Some(Err(reason))) => denial(reason),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "tenant_not_found"})),
        )
            .into_response(),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error": "egress_ledger_unavailable"})),
        )
            .into_response(),
    }
}

pub async fn snapshot(
    State(state): State<AppState>,
    Admin(_): Admin,
    Path(tenant): Path<String>,
) -> Response {
    let Ok(_permit) = REQUESTS.try_acquire() else {
        return denial(EgressDenial::Busy);
    };
    let Ok(id) = TenantId::new(tenant) else {
        return denial(EgressDenial::Invalid);
    };
    match state.tenant_repo.egress_snapshot(&id).await {
        Ok(Some(snapshot)) => {
            Json(json!({"protocol": "funded-egress-v1", "snapshot": snapshot})).into_response()
        }
        Ok(None) => denial(EgressDenial::Unconfigured),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error": "egress_ledger_unavailable"})),
        )
            .into_response(),
    }
}

fn denial(reason: EgressDenial) -> Response {
    let status = match reason {
        EgressDenial::Exhausted => StatusCode::PAYMENT_REQUIRED,
        EgressDenial::Busy | EgressDenial::Capacity => StatusCode::TOO_MANY_REQUESTS,
        EgressDenial::Conflict => StatusCode::CONFLICT,
        EgressDenial::Expired => StatusCode::GONE,
        EgressDenial::Unconfigured => StatusCode::SERVICE_UNAVAILABLE,
        EgressDenial::Invalid => StatusCode::BAD_REQUEST,
    };
    (
        status,
        [(header::RETRY_AFTER, "30")],
        Json(json!({"error": "egress_admission_denied", "reason": reason})),
    )
        .into_response()
}
