use axum::{
    Json,
    body::{Body, to_bytes},
    extract::Request,
    http::{StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde_json::json;
use std::time::Duration;
use tokio::sync::Semaphore;
use tower_http::{CompressionLevel, compression::CompressionLayer};

static RESPONSES: Semaphore = Semaphore::const_new(16);
static WAITING: Semaphore = Semaphore::const_new(64);

/// Gzip for clients that send `Accept-Encoding: gzip`. Layer it outside
/// [`bounded`], so the response bound keeps measuring uncompressed bytes.
/// Fastest level: the release read gate measures this cost on every read.
pub fn compression() -> CompressionLayer {
    CompressionLayer::new().quality(CompressionLevel::Fastest)
}

/// A `views/{view}/changes?wait=N` request may be held for up to 20 s with
/// nothing to send; holding a response permit for that long would let 16
/// long-polls starve every other response.
fn is_long_poll(request: &Request) -> bool {
    let path = request.uri().path();
    path.starts_with("/api/v1/views/")
        && path.ends_with("/changes")
        && request.uri().query().is_some_and(|query| {
            query
                .split('&')
                .any(|pair| pair.starts_with("wait=") && pair != "wait=0")
        })
}

pub async fn bounded(request: Request, next: Next) -> Response {
    let Ok(waiting) = WAITING.try_acquire() else {
        return (StatusCode::TOO_MANY_REQUESTS, [(header::RETRY_AFTER, "1")]).into_response();
    };
    if is_long_poll(&request) {
        drop(waiting);
        let response = next.run(request).await;
        let Ok(Ok(_permit)) =
            tokio::time::timeout(Duration::from_secs(2), RESPONSES.acquire()).await
        else {
            return (StatusCode::TOO_MANY_REQUESTS, [(header::RETRY_AFTER, "1")]).into_response();
        };
        return buffer(response).await;
    }
    let Ok(Ok(_permit)) = tokio::time::timeout(Duration::from_secs(2), RESPONSES.acquire()).await
    else {
        return (StatusCode::TOO_MANY_REQUESTS, [(header::RETRY_AFTER, "1")]).into_response();
    };
    drop(waiting);
    let response = next.run(request).await;
    if response.status() == StatusCode::SWITCHING_PROTOCOLS {
        return response;
    }
    buffer(response).await
}

async fn buffer(response: Response) -> Response {
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
