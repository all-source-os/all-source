//! Serves Core's port while the bootstrap replays, so a client gets
//! `503 not_ready` with `Retry-After` instead of a refused connection.
//! The gateways resend a request answered `not_ready`: no handler has run it.

use axum::{
    Json, Router,
    extract::Request,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use std::sync::{Arc, OnceLock};
use tower::Service;

#[derive(Default)]
pub struct BootGate {
    app: OnceLock<Router>,
}

impl BootGate {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Hands every later request to `app`. A second call is ignored.
    pub fn open(&self, app: Router) {
        if self.app.set(app).is_err() {
            tracing::warn!("boot gate opened twice; keeping the first router");
        }
    }

    pub fn is_open(&self) -> bool {
        self.app.get().is_some()
    }

    /// The router to serve on the port from the moment it is bound.
    pub fn router(self: &Arc<Self>) -> Router {
        let gate = Arc::clone(self);
        Router::new().fallback(move |request: Request| {
            let gate = Arc::clone(&gate);
            async move {
                let Some(app) = gate.app.get() else {
                    return not_ready();
                };
                match app.clone().call(request).await {
                    Ok(response) => response,
                    Err(never) => match never {},
                }
            }
        })
    }
}

pub fn not_ready() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        [(header::RETRY_AFTER, "1")],
        Json(serde_json::json!({
            "error": "not_ready",
            "message": "Core is starting; retry shortly.",
        })),
    )
        .into_response()
}
