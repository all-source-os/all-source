use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Value Object: TenantId
///
/// Represents a unique identifier for a tenant in the multi-tenant system.
///
/// Domain Rules:
/// - Cannot be empty
/// - Must be alphanumeric with hyphens/underscores only
/// - Must be between 1 and 64 characters
/// - Case-sensitive
/// - Immutable once created
///
/// This is a Value Object, not an Entity:
/// - Defined by its value, not identity
/// - Immutable
/// - No lifecycle
/// - Compared by value equality
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TenantId(String);

impl TenantId {
    pub(crate) fn heap_capacity(&self) -> usize {
        self.0.capacity()
    }

    /// Create a new TenantId with validation
    ///
    /// # Errors
    /// Returns error if:
    /// - ID is empty
    /// - ID is longer than 64 characters
    /// - ID contains invalid characters (only a-z, A-Z, 0-9, -, _ allowed)
    ///
    /// # Examples
    /// ```
    /// use allsource_core::domain::value_objects::TenantId;
    ///
    /// let tenant_id = TenantId::new("acme-corp".to_string()).unwrap();
    /// assert_eq!(tenant_id.as_str(), "acme-corp");
    /// ```
    pub fn new(value: String) -> Result<Self> {
        Self::validate(&value)?;
        Ok(Self(value))
    }

    /// Create TenantId without validation (for internal use, e.g., from trusted storage)
    ///
    /// # Safety
    /// This bypasses validation. Only use when loading from trusted sources
    /// where validation has already occurred.
    pub(crate) fn new_unchecked(value: String) -> Self {
        Self(value)
    }

    /// Create default tenant ID
    pub fn default_tenant() -> Self {
        Self("default".to_string())
    }

    /// Get the string value
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Get the inner String (consumes self)
    pub fn into_inner(self) -> String {
        self.0
    }

    /// Validate a tenant ID string
    fn validate(value: &str) -> Result<()> {
        // Rule: Cannot be empty
        if value.is_empty() {
            return Err(crate::error::AllSourceError::InvalidInput(
                "Tenant ID cannot be empty".to_string(),
            ));
        }

        // Rule: Maximum length 64 characters
        if value.len() > 64 {
            return Err(crate::error::AllSourceError::InvalidInput(format!(
                "Tenant ID cannot exceed 64 characters, got {}",
                value.len()
            )));
        }

        // Rule: Only alphanumeric, hyphens, and underscores
        if !value
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return Err(crate::error::AllSourceError::InvalidInput(format!(
                "Tenant ID '{value}' contains invalid characters. Only alphanumeric, hyphens, and underscores allowed"
            )));
        }

        Ok(())
    }
}

// Implement Display for ergonomic string conversion
impl fmt::Display for TenantId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// Implement From<&str> for convenience
impl TryFrom<&str> for TenantId {
    type Error = crate::error::AllSourceError;

    fn try_from(value: &str) -> Result<Self> {
        TenantId::new(value.to_string())
    }
}

// Implement From<String> for convenience
impl TryFrom<String> for TenantId {
    type Error = crate::error::AllSourceError;

    fn try_from(value: String) -> Result<Self> {
        TenantId::new(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_valid_tenant_id() {
        // Valid simple tenant ID
        let tenant_id = TenantId::new("acme-corp".to_string());
        assert!(tenant_id.is_ok());
        assert_eq!(tenant_id.unwrap().as_str(), "acme-corp");

        // Valid with underscores
        let tenant_id = TenantId::new("tenant_123".to_string());
        assert!(tenant_id.is_ok());

        // Valid with mixed case
        let tenant_id = TenantId::new("TenantABC".to_string());
        assert!(tenant_id.is_ok());

        // Valid alphanumeric
        let tenant_id = TenantId::new("tenant123".to_string());
        assert!(tenant_id.is_ok());
    }

    #[test]
    fn test_reject_empty_tenant_id() {
        let result = TenantId::new(String::new());
        assert!(result.is_err());

        if let Err(e) = result {
            assert!(e.to_string().contains("cannot be empty"));
        }
    }

    #[test]
    fn test_reject_too_long_tenant_id() {
        // Create a 65-character string (exceeds max of 64)
        let long_id = "a".repeat(65);
        let result = TenantId::new(long_id);
        assert!(result.is_err());

        if let Err(e) = result {
            assert!(e.to_string().contains("cannot exceed 64 characters"));
        }
    }

    #[test]
    fn test_accept_max_length_tenant_id() {
        // Exactly 64 characters should be OK
        let max_id = "a".repeat(64);
        let result = TenantId::new(max_id);
        assert!(result.is_ok());
    }

    #[test]
    fn test_reject_invalid_characters() {
        // Space is invalid
        let result = TenantId::new("tenant 123".to_string());
        assert!(result.is_err());

        // Special characters are invalid
        let result = TenantId::new("tenant@123".to_string());
        assert!(result.is_err());

        let result = TenantId::new("tenant.123".to_string());
        assert!(result.is_err());

        let result = TenantId::new("tenant/123".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn test_default_tenant() {
        let tenant_id = TenantId::default_tenant();
        assert_eq!(tenant_id.as_str(), "default");
    }

    #[test]
    fn test_display_trait() {
        let tenant_id = TenantId::new("test-tenant".to_string()).unwrap();
        assert_eq!(format!("{tenant_id}"), "test-tenant");
    }

    #[test]
    fn test_try_from_str() {
        let tenant_id: Result<TenantId> = "valid-tenant".try_into();
        assert!(tenant_id.is_ok());
        assert_eq!(tenant_id.unwrap().as_str(), "valid-tenant");

        let invalid: Result<TenantId> = "".try_into();
        assert!(invalid.is_err());
    }

    #[test]
    fn test_try_from_string() {
        let tenant_id: Result<TenantId> = "valid-tenant".to_string().try_into();
        assert!(tenant_id.is_ok());

        let invalid: Result<TenantId> = String::new().try_into();
        assert!(invalid.is_err());
    }

    #[test]
    fn test_into_inner() {
        let tenant_id = TenantId::new("test".to_string()).unwrap();
        let inner = tenant_id.into_inner();
        assert_eq!(inner, "test");
    }

    #[test]
    fn test_equality() {
        let tenant1 = TenantId::new("tenant-a".to_string()).unwrap();
        let tenant2 = TenantId::new("tenant-a".to_string()).unwrap();
        let tenant3 = TenantId::new("tenant-b".to_string()).unwrap();

        // Value equality
        assert_eq!(tenant1, tenant2);
        assert_ne!(tenant1, tenant3);
    }

    #[test]
    fn test_cloning() {
        let tenant1 = TenantId::new("tenant".to_string()).unwrap();
        let tenant2 = tenant1.clone();
        assert_eq!(tenant1, tenant2);
    }

    #[test]
    fn test_hash_consistency() {
        use std::collections::HashSet;

        let tenant1 = TenantId::new("tenant".to_string()).unwrap();
        let tenant2 = TenantId::new("tenant".to_string()).unwrap();

        let mut set = HashSet::new();
        set.insert(tenant1);

        // Should find the same value (value equality)
        assert!(set.contains(&tenant2));
    }

    #[test]
    fn test_serde_serialization() {
        let tenant_id = TenantId::new("test-tenant".to_string()).unwrap();

        // Serialize
        let json = serde_json::to_string(&tenant_id).unwrap();
        assert_eq!(json, "\"test-tenant\"");

        // Deserialize
        let deserialized: TenantId = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, tenant_id);
    }

    #[test]
    fn test_new_unchecked() {
        // Should create without validation (for internal use)
        let tenant_id = TenantId::new_unchecked("invalid chars!@#".to_string());
        assert_eq!(tenant_id.as_str(), "invalid chars!@#");
    }
}
