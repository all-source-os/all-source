use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Value Object: EventType
///
/// Represents the type/name of an event in the event store.
///
/// Domain Rules:
/// - Cannot be empty
/// - Must follow naming convention: lowercase, dot-separated namespacing
/// - Only alphanumeric, dots, and underscores allowed
/// - Must be between 1 and 128 characters
/// - Convention: namespace.entity.action (e.g., "order.placed", "user.profile.updated")
/// - Immutable once created
///
/// This is a Value Object:
/// - Defined by its value, not identity
/// - Immutable
/// - Self-validating
/// - Compared by value equality
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EventType(String);

impl EventType {
    pub(crate) fn heap_capacity(&self) -> usize {
        self.0.capacity()
    }

    /// Create a new EventType with validation
    ///
    /// # Errors
    /// Returns error if:
    /// - Type is empty
    /// - Type is longer than 128 characters
    /// - Type contains uppercase letters or invalid characters
    /// - Type doesn't follow naming convention
    ///
    /// # Examples
    /// ```
    /// use allsource_core::domain::value_objects::EventType;
    ///
    /// let event_type = EventType::new("order.placed".to_string()).unwrap();
    /// assert_eq!(event_type.as_str(), "order.placed");
    ///
    /// let event_type = EventType::new("user.profile.updated".to_string()).unwrap();
    /// assert_eq!(event_type.as_str(), "user.profile.updated");
    /// ```
    pub fn new(value: String) -> Result<Self> {
        Self::validate(&value)?;
        Ok(Self(value))
    }

    /// Create EventType without validation (for internal use, e.g., from trusted storage)
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

    /// Get the namespace of the event type (everything before the first dot)
    ///
    /// # Examples
    /// ```
    /// use allsource_core::domain::value_objects::EventType;
    ///
    /// let event_type = EventType::new("order.placed".to_string()).unwrap();
    /// assert_eq!(event_type.namespace(), Some("order"));
    ///
    /// let event_type = EventType::new("user.profile.updated".to_string()).unwrap();
    /// assert_eq!(event_type.namespace(), Some("user"));
    ///
    /// let event_type = EventType::new("simple".to_string()).unwrap();
    /// assert_eq!(event_type.namespace(), None);
    /// ```
    pub fn namespace(&self) -> Option<&str> {
        self.0.split('.').next().filter(|s| self.0.contains('.'))
    }

    /// Get the action part of the event type (everything after the last dot)
    ///
    /// # Examples
    /// ```
    /// use allsource_core::domain::value_objects::EventType;
    ///
    /// let event_type = EventType::new("order.placed".to_string()).unwrap();
    /// assert_eq!(event_type.action(), Some("placed"));
    ///
    /// let event_type = EventType::new("user.profile.updated".to_string()).unwrap();
    /// assert_eq!(event_type.action(), Some("updated"));
    ///
    /// let event_type = EventType::new("simple".to_string()).unwrap();
    /// assert_eq!(event_type.action(), None);
    /// ```
    pub fn action(&self) -> Option<&str> {
        self.0.rsplit('.').next().filter(|_| self.0.contains('.'))
    }

    /// Check if this event type is in a specific namespace
    ///
    /// # Examples
    /// ```
    /// use allsource_core::domain::value_objects::EventType;
    ///
    /// let event_type = EventType::new("order.placed".to_string()).unwrap();
    /// assert!(event_type.is_in_namespace("order"));
    /// assert!(!event_type.is_in_namespace("user"));
    /// ```
    pub fn is_in_namespace(&self, namespace: &str) -> bool {
        self.namespace() == Some(namespace)
    }

    /// Check if this event type belongs to the system namespace (`_system.*`).
    ///
    /// System event types are reserved for operational metadata (tenants, config,
    /// audit, schemas, policies) and must not be used by user applications.
    pub fn is_system(&self) -> bool {
        self.0
            .starts_with(crate::domain::value_objects::system_stream::SYSTEM_EVENT_TYPE_PREFIX)
    }

    /// Validate an event type string
    fn validate(value: &str) -> Result<()> {
        // Rule: Cannot be empty
        if value.is_empty() {
            return Err(crate::error::AllSourceError::InvalidInput(
                "Event type cannot be empty".to_string(),
            ));
        }

        // Rule: Maximum length 128 characters
        if value.len() > 128 {
            return Err(crate::error::AllSourceError::InvalidInput(format!(
                "Event type cannot exceed 128 characters, got {}",
                value.len()
            )));
        }

        // Rule: Must be lowercase with dots/underscores
        if !value
            .chars()
            .all(|c| c.is_lowercase() || c.is_numeric() || c == '.' || c == '_')
        {
            return Err(crate::error::AllSourceError::InvalidInput(format!(
                "Event type '{value}' must be lowercase with dots/underscores. Convention: namespace.entity.action"
            )));
        }

        // Rule: Cannot start or end with a dot
        if value.starts_with('.') || value.ends_with('.') {
            return Err(crate::error::AllSourceError::InvalidInput(format!(
                "Event type '{value}' cannot start or end with a dot"
            )));
        }

        // Rule: Cannot have consecutive dots
        if value.contains("..") {
            return Err(crate::error::AllSourceError::InvalidInput(format!(
                "Event type '{value}' cannot have consecutive dots"
            )));
        }

        Ok(())
    }
}

// Implement Display for ergonomic string conversion
impl fmt::Display for EventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// Implement From<&str> for convenience
impl TryFrom<&str> for EventType {
    type Error = crate::error::AllSourceError;

    fn try_from(value: &str) -> Result<Self> {
        EventType::new(value.to_string())
    }
}

// Implement From<String> for convenience
impl TryFrom<String> for EventType {
    type Error = crate::error::AllSourceError;

    fn try_from(value: String) -> Result<Self> {
        EventType::new(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_valid_event_types() {
        // Valid two-part event type
        let event_type = EventType::new("order.placed".to_string());
        assert!(event_type.is_ok());
        assert_eq!(event_type.unwrap().as_str(), "order.placed");

        // Valid three-part event type
        let event_type = EventType::new("user.profile.updated".to_string());
        assert!(event_type.is_ok());

        // Valid with underscores
        let event_type = EventType::new("order_item.created".to_string());
        assert!(event_type.is_ok());

        // Valid with numbers
        let event_type = EventType::new("payment.v2.processed".to_string());
        assert!(event_type.is_ok());

        // Valid single word (no namespace)
        let event_type = EventType::new("created".to_string());
        assert!(event_type.is_ok());
    }

    #[test]
    fn test_reject_empty_event_type() {
        let result = EventType::new(String::new());
        assert!(result.is_err());

        if let Err(e) = result {
            assert!(e.to_string().contains("cannot be empty"));
        }
    }

    #[test]
    fn test_reject_too_long_event_type() {
        // Create a 129-character string (exceeds max of 128)
        let long_type = "a".repeat(129);
        let result = EventType::new(long_type);
        assert!(result.is_err());

        if let Err(e) = result {
            assert!(e.to_string().contains("cannot exceed 128 characters"));
        }
    }

    #[test]
    fn test_accept_max_length_event_type() {
        // Exactly 128 characters should be OK
        let max_type = "a".repeat(128);
        let result = EventType::new(max_type);
        assert!(result.is_ok());
    }

    #[test]
    fn test_reject_uppercase() {
        let result = EventType::new("Order.Placed".to_string());
        assert!(result.is_err());

        let result = EventType::new("order.PLACED".to_string());
        assert!(result.is_err());

        if let Err(e) = EventType::new("User.Created".to_string()) {
            assert!(e.to_string().contains("must be lowercase"));
        }
    }

    #[test]
    fn test_reject_invalid_characters() {
        // Space is invalid
        let result = EventType::new("order placed".to_string());
        assert!(result.is_err());

        // Special characters are invalid
        let result = EventType::new("order@placed".to_string());
        assert!(result.is_err());

        let result = EventType::new("order/placed".to_string());
        assert!(result.is_err());

        let result = EventType::new("order-placed".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn test_reject_starting_with_dot() {
        let result = EventType::new(".order.placed".to_string());
        assert!(result.is_err());

        if let Err(e) = result {
            assert!(e.to_string().contains("cannot start or end with a dot"));
        }
    }

    #[test]
    fn test_reject_ending_with_dot() {
        let result = EventType::new("order.placed.".to_string());
        assert!(result.is_err());

        if let Err(e) = result {
            assert!(e.to_string().contains("cannot start or end with a dot"));
        }
    }

    #[test]
    fn test_reject_consecutive_dots() {
        let result = EventType::new("order..placed".to_string());
        assert!(result.is_err());

        if let Err(e) = result {
            assert!(e.to_string().contains("cannot have consecutive dots"));
        }
    }

    #[test]
    fn test_namespace_extraction() {
        let event_type = EventType::new("order.placed".to_string()).unwrap();
        assert_eq!(event_type.namespace(), Some("order"));

        let event_type = EventType::new("user.profile.updated".to_string()).unwrap();
        assert_eq!(event_type.namespace(), Some("user"));

        // No namespace (single word)
        let event_type = EventType::new("created".to_string()).unwrap();
        assert_eq!(event_type.namespace(), None);
    }

    #[test]
    fn test_action_extraction() {
        let event_type = EventType::new("order.placed".to_string()).unwrap();
        assert_eq!(event_type.action(), Some("placed"));

        let event_type = EventType::new("user.profile.updated".to_string()).unwrap();
        assert_eq!(event_type.action(), Some("updated"));

        // No action (single word)
        let event_type = EventType::new("created".to_string()).unwrap();
        assert_eq!(event_type.action(), None);
    }

    #[test]
    fn test_is_in_namespace() {
        let event_type = EventType::new("order.placed".to_string()).unwrap();
        assert!(event_type.is_in_namespace("order"));
        assert!(!event_type.is_in_namespace("user"));

        let event_type = EventType::new("user.profile.updated".to_string()).unwrap();
        assert!(event_type.is_in_namespace("user"));
        assert!(!event_type.is_in_namespace("order"));
    }

    #[test]
    fn test_display_trait() {
        let event_type = EventType::new("order.placed".to_string()).unwrap();
        assert_eq!(format!("{event_type}"), "order.placed");
    }

    #[test]
    fn test_try_from_str() {
        let event_type: Result<EventType> = "order.created".try_into();
        assert!(event_type.is_ok());
        assert_eq!(event_type.unwrap().as_str(), "order.created");

        let invalid: Result<EventType> = "".try_into();
        assert!(invalid.is_err());
    }

    #[test]
    fn test_try_from_string() {
        let event_type: Result<EventType> = "user.deleted".to_string().try_into();
        assert!(event_type.is_ok());

        let invalid: Result<EventType> = String::new().try_into();
        assert!(invalid.is_err());
    }

    #[test]
    fn test_into_inner() {
        let event_type = EventType::new("test.event".to_string()).unwrap();
        let inner = event_type.into_inner();
        assert_eq!(inner, "test.event");
    }

    #[test]
    fn test_equality() {
        let type1 = EventType::new("order.placed".to_string()).unwrap();
        let type2 = EventType::new("order.placed".to_string()).unwrap();
        let type3 = EventType::new("order.cancelled".to_string()).unwrap();

        // Value equality
        assert_eq!(type1, type2);
        assert_ne!(type1, type3);
    }

    #[test]
    fn test_cloning() {
        let type1 = EventType::new("event.test".to_string()).unwrap();
        let type2 = type1.clone();
        assert_eq!(type1, type2);
    }

    #[test]
    fn test_hash_consistency() {
        use std::collections::HashSet;

        let type1 = EventType::new("order.placed".to_string()).unwrap();
        let type2 = EventType::new("order.placed".to_string()).unwrap();

        let mut set = HashSet::new();
        set.insert(type1);

        // Should find the same value (value equality)
        assert!(set.contains(&type2));
    }

    #[test]
    fn test_serde_serialization() {
        let event_type = EventType::new("order.placed".to_string()).unwrap();

        // Serialize
        let json = serde_json::to_string(&event_type).unwrap();
        assert_eq!(json, "\"order.placed\"");

        // Deserialize
        let deserialized: EventType = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, event_type);
    }

    #[test]
    fn test_new_unchecked() {
        // Should create without validation (for internal use)
        let event_type = EventType::new_unchecked("INVALID.Type!".to_string());
        assert_eq!(event_type.as_str(), "INVALID.Type!");
    }
}
