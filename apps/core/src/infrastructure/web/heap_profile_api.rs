//! Admin-only heap profile dump. Registered only in `heap-profile` builds.

use crate::infrastructure::{observability::heap::heap_profile_pprof, security::middleware::Admin};
use axum::{
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};

/// GET /api/v1/admin/heap-profile
pub async fn heap_profile(Admin(_): Admin) -> Response {
    match heap_profile_pprof().await {
        Ok(bytes) => ([(header::CONTENT_TYPE, "application/octet-stream")], bytes).into_response(),
        Err(err) => (StatusCode::SERVICE_UNAVAILABLE, err.to_string()).into_response(),
    }
}
