//! Error types for AllSource Prime.

use std::fmt;

/// Errors that can occur during Prime graph operations.
#[derive(Debug)]
pub enum PrimeError {
    /// The requested node was not found or has been deleted.
    NodeNotFound(String),
    /// The requested edge was not found or has been deleted.
    EdgeNotFound(String),
    /// A node with this ID already exists.
    DuplicateNode(String),
    /// A traversal operation encountered an invalid state.
    InvalidTraversal(String),
    /// A projection failed to process an event.
    ProjectionError(String),
    /// An error from the underlying Core engine.
    CoreError(anyhow::Error),
    /// Schema validation failed (missing required fields, type mismatch, etc.)
    ValidationFailed(String),
    /// A write broke a typed kind constraint under `strict` enforcement.
    SchemaViolation(Box<super::schema::Violation>),
}

impl fmt::Display for PrimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NodeNotFound(id) => write!(f, "node not found: {id}"),
            Self::EdgeNotFound(id) => write!(f, "edge not found: {id}"),
            Self::DuplicateNode(id) => write!(f, "duplicate node: {id}"),
            Self::InvalidTraversal(msg) => write!(f, "invalid traversal: {msg}"),
            Self::ProjectionError(msg) => write!(f, "projection error: {msg}"),
            Self::CoreError(e) => write!(f, "core error: {e}"),
            Self::ValidationFailed(msg) => write!(f, "validation failed: {msg}"),
            Self::SchemaViolation(v) => write!(f, "validation failed: {v}"),
        }
    }
}

impl PrimeError {
    /// The message of a validation refusal, without the `validation failed: `
    /// prefix; `None` for every other error. Hosted writes carry exactly this
    /// text so both paths report the same refusal.
    pub fn validation_message(&self) -> Option<String> {
        match self {
            Self::ValidationFailed(msg) => Some(msg.clone()),
            Self::SchemaViolation(v) => Some(v.to_string()),
            _ => None,
        }
    }
}

impl From<super::schema::ValidationError> for PrimeError {
    fn from(e: super::schema::ValidationError) -> Self {
        Self::ValidationFailed(e.to_string())
    }
}

impl std::error::Error for PrimeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::CoreError(e) => Some(e.as_ref()),
            _ => None,
        }
    }
}

impl From<anyhow::Error> for PrimeError {
    fn from(e: anyhow::Error) -> Self {
        Self::CoreError(e)
    }
}

impl From<crate::error::AllSourceError> for PrimeError {
    fn from(e: crate::error::AllSourceError) -> Self {
        Self::CoreError(e.into())
    }
}

/// Result type for Prime operations.
pub type PrimeResult<T> = std::result::Result<T, PrimeError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let e = PrimeError::NodeNotFound("n-123".to_string());
        assert_eq!(e.to_string(), "node not found: n-123");

        let e = PrimeError::EdgeNotFound("e-456".to_string());
        assert_eq!(e.to_string(), "edge not found: e-456");

        let e = PrimeError::DuplicateNode("n-789".to_string());
        assert_eq!(e.to_string(), "duplicate node: n-789");

        let e = PrimeError::InvalidTraversal("cycle detected".to_string());
        assert_eq!(e.to_string(), "invalid traversal: cycle detected");

        let e = PrimeError::ProjectionError("failed to snapshot".to_string());
        assert_eq!(e.to_string(), "projection error: failed to snapshot");
    }

    #[test]
    fn test_from_anyhow() {
        let anyhow_err = anyhow::anyhow!("something went wrong");
        let prime_err = PrimeError::from(anyhow_err);
        assert!(matches!(prime_err, PrimeError::CoreError(_)));
        assert!(prime_err.to_string().contains("something went wrong"));
    }
}
