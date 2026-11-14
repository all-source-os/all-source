use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Value Object: EntityId
///
/// Represents a unique identifier for an entity in the event sourcing system.
/// Entities are the subjects of events (e.g., user-123, order-456, product-789).
///
/// Domain Rules:
/// - Cannot be empty
/// - Must be between 1 and 128 characters
/// - Flexible format (allows any printable characters except whitespace)
/// - Case-sensitive
/// - Immutable once created
///
/// This is a Value Object:
/// - Defined by its value, not identity
/// - Immutable
/// - Self-validating
/// - Compared by value equality
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntityId(String);

impl EntityId {
    pub(crate) fn heap_capacity(&self) -> usize {
        self.0.capacity()
    }

    /// Create a new EntityId with validation
    ///
    /// # Errors
    /// Returns error if:
    /// - ID is empty
    /// - ID is longer than 128 characters
    /// - ID contains only whitespace
    /// - ID contains control characters
    ///
    /// # Examples
    /// ```
    /// use allsource_core::domain::value_objects::EntityId;
    ///
    /// let entity_id = EntityId::new("user-123".to_string()).unwrap();
    /// assert_eq!(entity_id.as_str(), "user-123");
    ///
    /// let entity_id = EntityId::new("order_ABC-456".to_string()).unwrap();
    /// assert_eq!(entity_id.as_str(), "order_ABC-456");
    /// ```
    pub fn new(value: String) -> Result<Self> {
        Self::validate(&value)?;
        Ok(Self(value))
    }

    /// Create EntityId without validation (for internal use, e.g., from trusted storage)
    ///
    /// # Safety
    /// This bypasses validation. Only use when loading from trusted sources
    /// where validation has already occurred.
    pub(crate) fn new_unchecked(value: String) -> Self {
        Self(value)
    }

    /// Get the string value
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Get the inner String (consumes self)
    pub fn into_inner(self) -> String {
        self.0
    }

    /// Check if this entity ID starts with a specific prefix
    ///
    /// # Examples
    /// ```
    /// use allsource_core::domain::value_objects::EntityId;
    ///
    /// let entity_id = EntityId::new("user-123".to_string()).unwrap();
    /// assert!(entity_id.starts_with("user-"));
    /// assert!(!entity_id.starts_with("order-"));
    /// ```
    pub fn starts_with(&self, prefix: &str) -> bool {
        self.0.starts_with(prefix)
    }

    /// Check if this entity ID ends with a specific suffix
    ///
    /// # Examples
    /// ```
    /// use allsource_core::domain::value_objects::EntityId;
    ///
    /// let entity_id = EntityId::new("user-123".to_string()).unwrap();
    /// assert!(entity_id.ends_with("-123"));
    /// assert!(!entity_id.ends_with("-456"));
    /// ```
    pub fn ends_with(&self, suffix: &str) -> bool {
        self.0.ends_with(suffix)
    }

    /// Extract a prefix before a delimiter (if present)
    ///
    /// # Examples
    /// ```
    /// use allsource_core::domain::value_objects::EntityId;
    ///
    /// let entity_id = EntityId::new("user-123".to_string()).unwrap();
    /// assert_eq!(entity_id.prefix('-'), Some("user"));
    ///
    /// let entity_id = EntityId::new("simple".to_string()).unwrap();
    /// assert_eq!(entity_id.prefix('-'), None);
    /// ```
    pub fn prefix(&self, delimiter: char) -> Option<&str> {
        self.0
            .split(delimiter)
            .next()
            .filter(|_| self.0.contains(delimiter))
    }

    /// Validate an entity ID string
    fn validate(value: &str) -> Result<()> {
        // Rule: Cannot be empty
        if value.is_empty() {
            return Err(crate::error::AllSourceError::InvalidInput(
                "Entity ID cannot be empty".to_string(),
            ));
        }

        // Rule: Maximum length 128 characters
        if value.len() > 128 {
            return Err(crate::error::AllSourceError::InvalidInput(format!(
                "Entity ID cannot exceed 128 characters, got {}",
                value.len()
            )));
        }

        // Rule: No control characters (check before whitespace checks)
        if value.chars().any(char::is_control) {
            return Err(crate::error::AllSourceError::InvalidInput(
                "Entity ID cannot contain control characters".to_string(),
            ));
        }

        // Rule: Cannot be only whitespace
        if value.trim().is_empty() {
            return Err(crate::error::AllSourceError::InvalidInput(
                "Entity ID cannot be only whitespace".to_string(),
            ));
        }

        // Rule: No leading/trailing whitespace
        if value != value.trim() {
            return Err(crate::error::AllSourceError::InvalidInput(
                "Entity ID cannot have leading or trailing whitespace".to_string(),
            ));
        }

        Ok(())
    }
}

// Implement Display for ergonomic string conversion
impl fmt::Display for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// Implement From<&str> for convenience
impl TryFrom<&str> for EntityId {
    type Error = crate::error::AllSourceError;

    fn try_from(value: &str) -> Result<Self> {
        EntityId::new(value.to_string())
    }
}

// Implement From<String> for convenience
impl TryFrom<String> for EntityId {
    type Error = crate::error::AllSourceError;

    fn try_from(value: String) -> Result<Self> {
        EntityId::new(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_valid_entity_ids() {
        // Simple alphanumeric
        let entity_id = EntityId::new("user123".to_string());
        assert!(entity_id.is_ok());
        assert_eq!(entity_id.unwrap().as_str(), "user123");

        // With hyphen
        let entity_id = EntityId::new("user-123".to_string());
        assert!(entity_id.is_ok());

        // With underscore
        let entity_id = EntityId::new("user_123".to_string());
        assert!(entity_id.is_ok());

        // Complex format
        let entity_id = EntityId::new("order_ABC-456-XYZ".to_string());
        assert!(entity_id.is_ok());

        // UUID format
        let entity_id = EntityId::new("550e8400-e29b-41d4-a716-446655440000".to_string());
        assert!(entity_id.is_ok());

        // With special characters (allowed)
        let entity_id = EntityId::new("entity:123@domain".to_string());
        assert!(entity_id.is_ok());
    }

    #[test]
    fn test_reject_empty_entity_id() {
        let result = EntityId::new(String::new());
        assert!(result.is_err());

        if let Err(e) = result {
            assert!(e.to_string().contains("cannot be empty"));
        }
    }

    #[test]
    fn test_reject_whitespace_only() {
        let result = EntityId::new("   ".to_string());
        assert!(result.is_err());

        if let Err(e) = result {
            assert!(e.to_string().contains("cannot be only whitespace"));
        }
    }

    #[test]
    fn test_reject_leading_trailing_whitespace() {
        // Leading whitespace
        let result = EntityId::new(" user-123".to_string());
        assert!(result.is_err());

        // Trailing whitespace
        let result = EntityId::new("user-123 ".to_string());
        assert!(result.is_err());

        // Both
        let result = EntityId::new(" user-123 ".to_string());
        assert!(result.is_err());

        if let Err(e) = EntityId::new(" test ".to_string()) {
            assert!(e.to_string().contains("leading or trailing whitespace"));
        }
    }

    #[test]
    fn test_reject_too_long_entity_id() {
        // Create a 129-character string (exceeds max of 128)
        let long_id = "a".repeat(129);
        let result = EntityId::new(long_id);
        assert!(result.is_err());

        if let Err(e) = result {
            assert!(e.to_string().contains("cannot exceed 128 characters"));
        }
    }

    #[test]
    fn test_accept_max_length_entity_id() {
        // Exactly 128 characters should be OK
        let max_id = "a".repeat(128);
        let result = EntityId::new(max_id);
        assert!(result.is_ok());
    }

    #[test]
    fn test_reject_control_characters() {
        // Newline
        let result = EntityId::new("user\n123".to_string());
        assert!(result.is_err());

        // Tab
        let result = EntityId::new("user\t123".to_string());
        assert!(result.is_err());

        // Null byte
        let result = EntityId::new("user\x00123".to_string());
        assert!(result.is_err());

        if let Err(e) = EntityId::new("test\n".to_string()) {
            assert!(e.to_string().contains("control characters"));
        }
    }

    #[test]
    fn test_starts_with() {
        let entity_id = EntityId::new("user-123".to_string()).unwrap();
        assert!(entity_id.starts_with("user-"));
        assert!(entity_id.starts_with("user"));
        assert!(!entity_id.starts_with("order-"));
    }

    #[test]
    fn test_ends_with() {
        let entity_id = EntityId::new("user-123".to_string()).unwrap();
        assert!(entity_id.ends_with("-123"));
        assert!(entity_id.ends_with("123"));
        assert!(!entity_id.ends_with("-456"));
    }

    #[test]
    fn test_prefix_extraction() {
        let entity_id = EntityId::new("user-123".to_string()).unwrap();
        assert_eq!(entity_id.prefix('-'), Some("user"));

        let entity_id = EntityId::new("order_ABC_456".to_string()).unwrap();
        assert_eq!(entity_id.prefix('_'), Some("order"));

        // No delimiter
        let entity_id = EntityId::new("simple".to_string()).unwrap();
        assert_eq!(entity_id.prefix('-'), None);
    }

    #[test]
    fn test_display_trait() {
        let entity_id = EntityId::new("user-123".to_string()).unwrap();
        assert_eq!(format!("{entity_id}"), "user-123");
    }

    #[test]
    fn test_try_from_str() {
        let entity_id: Result<EntityId> = "order-456".try_into();
        assert!(entity_id.is_ok());
        assert_eq!(entity_id.unwrap().as_str(), "order-456");

        let invalid: Result<EntityId> = "".try_into();
        assert!(invalid.is_err());
    }

    #[test]
    fn test_try_from_string() {
        let entity_id: Result<EntityId> = "product-789".to_string().try_into();
        assert!(entity_id.is_ok());

        let invalid: Result<EntityId> = String::new().try_into();
        assert!(invalid.is_err());
    }

    #[test]
    fn test_into_inner() {
        let entity_id = EntityId::new("test-entity".to_string()).unwrap();
        let inner = entity_id.into_inner();
        assert_eq!(inner, "test-entity");
    }

    #[test]
    fn test_equality() {
        let id1 = EntityId::new("entity-a".to_string()).unwrap();
        let id2 = EntityId::new("entity-a".to_string()).unwrap();
        let id3 = EntityId::new("entity-b".to_string()).unwrap();

        // Value equality
        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_cloning() {
        let id1 = EntityId::new("entity".to_string()).unwrap();
        let id2 = id1.clone();
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_hash_consistency() {
        use std::collections::HashSet;

        let id1 = EntityId::new("entity-123".to_string()).unwrap();
        let id2 = EntityId::new("entity-123".to_string()).unwrap();

        let mut set = HashSet::new();
        set.insert(id1);

        // Should find the same value (value equality)
        assert!(set.contains(&id2));
    }

    #[test]
    fn test_serde_serialization() {
        let entity_id = EntityId::new("user-123".to_string()).unwrap();

        // Serialize
        let json = serde_json::to_string(&entity_id).unwrap();
        assert_eq!(json, "\"user-123\"");

        // Deserialize
        let deserialized: EntityId = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, entity_id);
    }

    #[test]
    fn test_new_unchecked() {
        // Should create without validation (for internal use)
        let entity_id = EntityId::new_unchecked("invalid\nid".to_string());
        assert_eq!(entity_id.as_str(), "invalid\nid");
    }
}
