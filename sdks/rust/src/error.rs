use serde_json::Value;
use std::time::Duration;

/// Errors returned by the AllSource SDK.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The AllSource API returned a non-2xx status code.
    #[error("AllSource API error: {status} — {message}")]
    Api {
        status: u16,
        message: String,
        body: Option<Value>,
    },

    /// Configuration error (e.g., missing base URL).
    #[error("configuration error: {0}")]
    Config(String),

    /// HTTP transport error.
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// JSON serialization/deserialization error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Circuit breaker is open — backend is unavailable.
    #[error("circuit breaker open: backend unavailable, retry after {retry_after_secs}s")]
    CircuitOpen { retry_after_secs: u64 },

    /// The server answered with a well-formed response that breaks the API
    /// contract — e.g. it ignored a pagination parameter, so the client cannot
    /// make progress. Distinct from [`Error::Json`] (the body did not parse)
    /// and [`Error::Api`] (the server said no): here the server said yes and
    /// meant something else.
    #[error("server contract violation: {0}")]
    Protocol(String),

    /// A compare-and-swap ingest was rejected: the entity was not at the
    /// version the write expected.
    ///
    /// Raised when [`crate::IngestEventInput::expected_version`] is set and
    /// Core's actual version differs. Deliberately **not** retryable — the
    /// write cannot succeed unchanged, so the caller must re-read state,
    /// recompute against `current`, and issue a fresh write. Retrying the same
    /// body would either fail identically or, worse, succeed against a version
    /// the caller never inspected.
    #[error("version conflict: expected entity at version {expected}, but it is at {current}")]
    VersionConflict {
        /// The version the write required the entity to be at.
        expected: u64,
        /// The version Core actually holds.
        current: u64,
    },

    /// The server refused the request for rate or capacity reasons: a 429, or
    /// a 503 whose body names a refusal `scope`.
    ///
    /// Not retryable inside the client. While `retry_after` is pending, every
    /// call on the same client fails with this error without a network call.
    /// It never counts towards the circuit breaker.
    #[error("rate limited (scope: {}), retry after {:?}", scope.as_deref().unwrap_or("unknown"), retry_after)]
    RateLimited {
        /// The server's `Retry-After`, or the time left of it on a fast fail.
        retry_after: Option<Duration>,
        /// The refusal `scope` from the body: `tenant`, `global_fair_share`
        /// or `global`.
        scope: Option<String>,
    },

    /// A query sent with `if_none_match` matched: nothing changed since that
    /// `ETag`. [`crate::QueryClient::query_events_conditional`] returns this
    /// as [`crate::EventsQuery::NotModified`] instead.
    #[error("not modified (etag: {})", etag.as_deref().unwrap_or("none"))]
    NotModified { etag: Option<String> },

    /// WebSocket transport error.
    #[cfg(feature = "ws")]
    #[error("WebSocket error: {0}")]
    WebSocket(String),
}

impl Error {
    /// Returns true if this is a 401 Unauthorized error.
    pub fn is_unauthorized(&self) -> bool {
        matches!(self, Error::Api { status: 401, .. })
    }

    /// Returns true if the server refused the request for rate or capacity.
    pub fn is_rate_limited(&self) -> bool {
        matches!(
            self,
            Error::RateLimited { .. } | Error::Api { status: 429, .. }
        )
    }

    /// The `Retry-After` a rate-limit refusal carried, if any.
    pub fn retry_after(&self) -> Option<Duration> {
        match self {
            Error::RateLimited { retry_after, .. } => *retry_after,
            _ => None,
        }
    }

    /// Returns true if this is a 400 Bad Request error (e.g., invalid event type format).
    pub fn is_bad_request(&self) -> bool {
        matches!(self, Error::Api { status: 400, .. })
    }

    /// Returns true if this is a 404 Not Found error.
    pub fn is_not_found(&self) -> bool {
        matches!(self, Error::Api { status: 404, .. })
    }

    /// Returns true if this is a server error (5xx).
    pub fn is_server_error(&self) -> bool {
        matches!(self, Error::Api { status, .. } if *status >= 500)
    }

    /// Returns true if the circuit breaker is open.
    pub fn is_circuit_open(&self) -> bool {
        matches!(self, Error::CircuitOpen { .. })
    }

    /// Returns true if this is a transient error that might succeed on retry.
    pub fn is_retryable(&self) -> bool {
        match self {
            Error::Api { status, .. } => matches!(status, 408 | 500 | 502 | 503 | 504),
            Error::Http(_) => true,
            _ => false,
        }
    }
}
