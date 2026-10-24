use better_auth_core::error::{AuthError, DatabaseError};
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum AllsourceAuthError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Allsource API error ({status}): {message}")]
    Api { status: u16, message: String },

    /// AllSource refused the request for rate or capacity: a 429, or a 503
    /// naming a refusal `scope`. While `retry_after` is pending, every call
    /// on the same client returns this without a network call.
    #[error("Allsource rate limited (scope: {}), retry after {:?}", scope.as_deref().unwrap_or("unknown"), retry_after)]
    RateLimited {
        retry_after: Option<Duration>,
        scope: Option<String>,
    },
}

impl From<AllsourceAuthError> for AuthError {
    fn from(err: AllsourceAuthError) -> Self {
        match err {
            AllsourceAuthError::RateLimited { .. } => AuthError::RateLimited,
            other => AuthError::Database(DatabaseError::Query(other.to_string())),
        }
    }
}
