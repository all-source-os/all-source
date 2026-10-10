/// AllSource error types
#[derive(Debug, thiserror::Error)]
pub enum AllSourceError {
    #[error("Event not found: {0}")]
    EventNotFound(String),

    #[error("Entity not found: {0}")]
    EntityNotFound(String),

    #[error("Tenant already exists: {0}")]
    TenantAlreadyExists(String),

    #[error("Tenant not found: {0}")]
    TenantNotFound(String),

    #[error("Invalid event: {0}")]
    InvalidEvent(String),

    #[error("Invalid query: {0}")]
    InvalidQuery(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Arrow error: {0}")]
    ArrowError(String),

    #[error("Index error: {0}")]
    IndexError(String),

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Concurrency error: {0}")]
    ConcurrencyError(String),

    #[error("Version conflict: expected {expected}, current {current}")]
    VersionConflict { expected: u64, current: u64 },

    #[error("Queue full: {0}")]
    QueueFull(String),

    /// The store was opened read-only (e.g. a second process attached to a
    /// data-dir already owned by a live writer). Writes are rejected so the
    /// owner's WAL is never corrupted. See `EventStoreConfig::read_only`.
    #[error("Store is read-only: {0}")]
    ReadOnly(String),

    /// A conditional write could not be authorised yet because the tenant's
    /// version manifest does not account for every archive file. The scan
    /// persisted what it folded, so retrying resolves strictly fewer files
    /// than this attempt did and the condition clears on its own.
    ///
    /// `reason` carries the limit that stopped the fold. Without it an
    /// operator cannot tell which budget dimension a stuck tenant is hitting,
    /// and that is the first thing worth knowing.
    #[error(
        "Archive version index incomplete: {remaining} archive file(s) not yet folded, stopped by \
         {reason}; retry to resume"
    )]
    ArchiveIndexIncomplete { remaining: usize, reason: String },

    #[error("Internal error: {0}")]
    InternalError(String),

    /// Event payload failed validation against a registered schema. The
    /// HTTP layer maps this to 422 with a structured body so callers can
    /// distinguish schema problems from generic validation failures.
    #[error(
        "Schema violation for event_type '{event_type}' (schema v{schema_version}): {errors:?}"
    )]
    SchemaViolation {
        event_type: String,
        schema_version: u32,
        errors: Vec<String>,
    },
}

impl AllSourceError {
    /// Returns true for transient errors that may succeed on retry
    /// (storage I/O, concurrency conflicts, queue pressure).
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            AllSourceError::StorageError(_)
                | AllSourceError::ConcurrencyError(_)
                | AllSourceError::VersionConflict { .. }
                | AllSourceError::QueueFull(_)
                | AllSourceError::ArchiveIndexIncomplete { .. }
        )
    }
}

// Alias for domain layer convenience
pub use AllSourceError as Error;

impl From<arrow::error::ArrowError> for AllSourceError {
    fn from(err: arrow::error::ArrowError) -> Self {
        AllSourceError::ArrowError(err.to_string())
    }
}

impl From<parquet::errors::ParquetError> for AllSourceError {
    fn from(err: parquet::errors::ParquetError) -> Self {
        AllSourceError::StorageError(err.to_string())
    }
}

impl From<crate::infrastructure::persistence::SimdJsonError> for AllSourceError {
    fn from(err: crate::infrastructure::persistence::SimdJsonError) -> Self {
        AllSourceError::SerializationError(serde_json::Error::io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            err.to_string(),
        )))
    }
}

#[cfg(feature = "postgres")]
impl From<sqlx::Error> for AllSourceError {
    fn from(err: sqlx::Error) -> Self {
        AllSourceError::StorageError(format!("Database error: {err}"))
    }
}

/// Custom Result type for AllSource operations
pub type Result<T> = std::result::Result<T, AllSourceError>;

#[cfg(feature = "server")]
mod axum_impl {
    use super::AllSourceError;
    use axum::{
        http::StatusCode,
        response::{IntoResponse, Response},
    };

    /// Implement IntoResponse for axum error handling
    impl IntoResponse for AllSourceError {
        fn into_response(self) -> Response {
            let (status, error_message) = match self {
                AllSourceError::EventNotFound(_)
                | AllSourceError::EntityNotFound(_)
                | AllSourceError::TenantNotFound(_) => (StatusCode::NOT_FOUND, self.to_string()),
                AllSourceError::InvalidEvent(_)
                | AllSourceError::InvalidQuery(_)
                | AllSourceError::InvalidInput(_)
                | AllSourceError::ValidationError(_) => (StatusCode::BAD_REQUEST, self.to_string()),
                AllSourceError::VersionConflict { expected, current } => {
                    let body = serde_json::json!({
                        "error": "version_conflict",
                        "expected_version": expected,
                        "current_version": current,
                    });
                    return (StatusCode::CONFLICT, axum::Json(body)).into_response();
                }
                AllSourceError::TenantAlreadyExists(_) | AllSourceError::ConcurrencyError(_) => {
                    (StatusCode::CONFLICT, self.to_string())
                }
                AllSourceError::ArchiveIndexIncomplete {
                    remaining,
                    ref reason,
                } => {
                    // Retry-After 1s, not a back-off: each attempt folds more
                    // files, so waiting longer does not make it likelier.
                    let body = serde_json::json!({
                        "error": "archive_index_incomplete",
                        "remaining_files": remaining,
                        "stopped_by": reason,
                    });
                    return (
                        StatusCode::SERVICE_UNAVAILABLE,
                        [(axum::http::header::RETRY_AFTER, "1")],
                        axum::Json(body),
                    )
                        .into_response();
                }
                AllSourceError::QueueFull(_) => (StatusCode::SERVICE_UNAVAILABLE, self.to_string()),
                AllSourceError::ReadOnly(_) => (StatusCode::SERVICE_UNAVAILABLE, self.to_string()),
                AllSourceError::StorageError(_)
                | AllSourceError::ArrowError(_)
                | AllSourceError::IndexError(_)
                | AllSourceError::InternalError(_) => {
                    (StatusCode::INTERNAL_SERVER_ERROR, self.to_string())
                }
                AllSourceError::SerializationError(_) => {
                    (StatusCode::UNPROCESSABLE_ENTITY, self.to_string())
                }
                AllSourceError::SchemaViolation {
                    event_type,
                    schema_version,
                    errors,
                } => {
                    let body = serde_json::json!({
                        "error": {
                            "code": "schema_violation",
                            "message": format!(
                                "Event payload does not match registered schema for '{event_type}'"
                            ),
                            "details": {
                                "event_type": event_type,
                                "schema_version": schema_version,
                                "errors": errors,
                            }
                        }
                    });
                    return (StatusCode::UNPROCESSABLE_ENTITY, axum::Json(body)).into_response();
                }
            };

            let body = serde_json::json!({
                "error": error_message,
            });

            (status, axum::Json(body)).into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "server")]
    use axum::{http::StatusCode, response::IntoResponse};

    #[test]
    fn test_error_display() {
        let err = AllSourceError::EventNotFound("event-123".to_string());
        assert_eq!(err.to_string(), "Event not found: event-123");

        let err = AllSourceError::EntityNotFound("entity-456".to_string());
        assert_eq!(err.to_string(), "Entity not found: entity-456");

        let err = AllSourceError::TenantAlreadyExists("tenant-1".to_string());
        assert_eq!(err.to_string(), "Tenant already exists: tenant-1");

        let err = AllSourceError::TenantNotFound("tenant-2".to_string());
        assert_eq!(err.to_string(), "Tenant not found: tenant-2");
    }

    #[test]
    fn test_error_variants() {
        let errors: Vec<AllSourceError> = vec![
            AllSourceError::InvalidEvent("bad event".to_string()),
            AllSourceError::InvalidQuery("bad query".to_string()),
            AllSourceError::InvalidInput("bad input".to_string()),
            AllSourceError::StorageError("storage failed".to_string()),
            AllSourceError::ArrowError("arrow failed".to_string()),
            AllSourceError::IndexError("index failed".to_string()),
            AllSourceError::ValidationError("validation failed".to_string()),
            AllSourceError::ConcurrencyError("conflict".to_string()),
            AllSourceError::QueueFull("queue full".to_string()),
            AllSourceError::InternalError("internal error".to_string()),
        ];

        for err in errors {
            let msg = err.to_string();
            assert!(!msg.is_empty());
        }
    }

    #[cfg(feature = "server")]
    #[test]
    fn test_into_response_not_found() {
        let err = AllSourceError::EventNotFound("event-123".to_string());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let err = AllSourceError::EntityNotFound("entity-456".to_string());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let err = AllSourceError::TenantNotFound("tenant-1".to_string());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[cfg(feature = "server")]
    #[test]
    fn test_into_response_bad_request() {
        let err = AllSourceError::InvalidEvent("bad event".to_string());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let err = AllSourceError::InvalidQuery("bad query".to_string());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let err = AllSourceError::InvalidInput("bad input".to_string());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let err = AllSourceError::ValidationError("validation failed".to_string());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[cfg(feature = "server")]
    #[test]
    fn test_into_response_conflict() {
        let err = AllSourceError::TenantAlreadyExists("tenant-1".to_string());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::CONFLICT);

        let err = AllSourceError::ConcurrencyError("conflict".to_string());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[cfg(feature = "server")]
    #[test]
    fn test_into_response_service_unavailable() {
        let err = AllSourceError::QueueFull("queue is full".to_string());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    /// Locks the wire contract from neotoma-gaps bead t-0795 AC #3:
    /// 422 with `{error: {code: "schema_violation", details: {event_type,
    /// schema_version, errors}}}`. Callers parse this shape — don't break it.
    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_into_response_schema_violation() {
        use axum::body::to_bytes;
        let err = AllSourceError::SchemaViolation {
            event_type: "user.created".to_string(),
            schema_version: 2,
            errors: vec!["Missing required field: email".to_string()],
        };
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

        let body_bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(body["error"]["code"], "schema_violation");
        assert_eq!(body["error"]["details"]["event_type"], "user.created");
        assert_eq!(body["error"]["details"]["schema_version"], 2);
        let errors = body["error"]["details"]["errors"].as_array().unwrap();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].as_str().unwrap().contains("email"));
    }

    #[cfg(feature = "server")]
    #[test]
    fn test_into_response_internal_error() {
        let err = AllSourceError::StorageError("storage error".to_string());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let err = AllSourceError::ArrowError("arrow error".to_string());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let err = AllSourceError::IndexError("index error".to_string());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let err = AllSourceError::InternalError("internal error".to_string());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_from_arrow_error() {
        let arrow_err = arrow::error::ArrowError::InvalidArgumentError("test".to_string());
        let err: AllSourceError = arrow_err.into();
        assert!(matches!(err, AllSourceError::ArrowError(_)));
    }

    #[test]
    fn test_from_parquet_error() {
        let parquet_err = parquet::errors::ParquetError::General("test".to_string());
        let err: AllSourceError = parquet_err.into();
        assert!(matches!(err, AllSourceError::StorageError(_)));
    }

    #[test]
    fn test_error_debug() {
        let err = AllSourceError::EventNotFound("test".to_string());
        let debug_str = format!("{err:?}");
        assert!(debug_str.contains("EventNotFound"));
    }

    #[test]
    fn test_is_retryable() {
        // Retryable errors
        assert!(AllSourceError::StorageError("io".to_string()).is_retryable());
        assert!(AllSourceError::ConcurrencyError("conflict".to_string()).is_retryable());
        assert!(AllSourceError::QueueFull("backpressure".to_string()).is_retryable());

        // Non-retryable errors
        assert!(!AllSourceError::EventNotFound("e1".to_string()).is_retryable());
        assert!(!AllSourceError::InvalidEvent("bad".to_string()).is_retryable());
        assert!(!AllSourceError::InvalidQuery("bad".to_string()).is_retryable());
        assert!(!AllSourceError::ValidationError("bad".to_string()).is_retryable());
        assert!(!AllSourceError::TenantNotFound("t1".to_string()).is_retryable());
        assert!(!AllSourceError::TenantAlreadyExists("t1".to_string()).is_retryable());
        assert!(!AllSourceError::InternalError("bug".to_string()).is_retryable());
    }
}
