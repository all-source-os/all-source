use axum::{
    Json,
    body::{Body, to_bytes},
    extract::Request,
    http::{StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde_json::json;
use tokio::sync::Semaphore;

static RESPONSES: Semaphore = Semaphore::const_new(16);

pub async fn bounded(request: Request, next: Next) -> Response {
    let Ok(_permit) = RESPONSES.try_acquire() else {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    };
    let response = next.run(request).await;
    if response.status() == StatusCode::SWITCHING_PROTOCOLS {
        return response;
    }
    let (mut parts, body) = response.into_parts();
    match to_bytes(body, crate::domain::entities::egress::MAX_RESPONSE_BYTES as usize).await {
        Ok(bytes) => {
            parts.headers.remove(header::TRANSFER_ENCODING);
            Response::from_parts(parts, Body::from(bytes))
        }
        Err(_) => (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(json!({"error": "response_too_large", "message": "Request a smaller bounded page."})),
        ).into_response(),
    }
}
