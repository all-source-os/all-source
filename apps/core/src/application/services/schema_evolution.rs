//! Autonomous schema evolution
//!
//! Automatically detects schema changes from ingested events and manages
//! schema evolution without manual intervention. Works alongside the existing
//! `SchemaRegistry` to:
//!
//! 1. **Infer schemas** from event payloads on first ingestion of a new event type
//! 2. **Detect drift** when new events have fields not in the registered schema
//! 3. **Auto-register** backward-compatible schema versions (new optional fields)
//! 4. **Flag breaking changes** that require manual review (type changes, removed required fields)
//!
//! ## How it works
//!
//! On each event ingestion, the `SchemaEvolutionManager` compares the event's
//! payload structure against the latest registered schema for that event type.
//! If differences are detected:
//! - New optional fields → auto-registered as a new schema version
//! - Type changes or missing required fields → flagged as breaking, not auto-registered
//!
//! ## Limitations
//!
//! - Only analyzes top-level and one level of nested fields
//! - Type inference is basic: string, number, boolean, array, object, null
//! - Does not handle polymorphic types (field sometimes string, sometimes number)
//! - No automatic data migration — schemas describe shape, don't transform data

use crate::infrastructure::memory::ledger::{Owner, Tally};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

/// Schema evolution event types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EvolutionAction {
    /// Schema was inferred from first event of this type
    Inferred,
    /// Backward-compatible change auto-registered
    AutoEvolved,
    /// Breaking change detected, requires manual review
    BreakingChangeDetected,
}

/// A record of a schema evolution event
#[derive(Debug, Clone, Serialize)]
pub struct EvolutionRecord {
    pub id: Uuid,
    pub event_type: String,
    pub action: EvolutionAction,
    pub from_version: Option<u32>,
    pub to_version: Option<u32>,
    pub added_fields: Vec<String>,
    pub removed_fields: Vec<String>,
    pub type_changes: Vec<FieldTypeChange>,
    pub timestamp: DateTime<Utc>,
}

/// A field type change (potential breaking change)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FieldTypeChange {
    pub field: String,
    pub old_type: String,
    pub new_type: String,
}

/// Inferred JSON type for a field
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InferredType {
    String,
    Number,
    Boolean,
    Array,
    Object,
    Null,
}

impl std::fmt::Display for InferredType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InferredType::String => write!(f, "string"),
            InferredType::Number => write!(f, "number"),
            InferredType::Boolean => write!(f, "boolean"),
            InferredType::Array => write!(f, "array"),
            InferredType::Object => write!(f, "object"),
            InferredType::Null => write!(f, "null"),
        }
    }
}

/// Infer the type of a JSON value
fn infer_type(value: &JsonValue) -> InferredType {
    match value {
        JsonValue::Null => InferredType::Null,
        JsonValue::Bool(_) => InferredType::Boolean,
        JsonValue::Number(_) => InferredType::Number,
        JsonValue::String(_) => InferredType::String,
        JsonValue::Array(_) => InferredType::Array,
        JsonValue::Object(_) => InferredType::Object,
    }
}

/// An inferred field schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldSchema {
    pub name: String,
    pub inferred_type: InferredType,
    pub nullable: bool,
    /// Nested fields (only if type is Object)
    pub nested: BTreeMap<String, FieldSchema>,
}

/// Schema shape inferred from a payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferredSchema {
    pub fields: BTreeMap<String, FieldSchema>,
}

/// Diff between two schemas
#[derive(Debug, Clone, Serialize)]
pub struct SchemaDiff {
    pub added_fields: Vec<String>,
    pub removed_fields: Vec<String>,
    pub type_changes: Vec<FieldTypeChange>,
    pub is_backward_compatible: bool,
}

/// Infer a schema shape from a JSON payload
pub fn infer_schema(payload: &JsonValue) -> InferredSchema {
    let mut fields = BTreeMap::new();
    if let Some(obj) = payload.as_object() {
        for (key, value) in obj {
            let mut nested = BTreeMap::new();
            if let Some(inner_obj) = value.as_object() {
                for (nk, nv) in inner_obj {
                    nested.insert(
                        nk.clone(),
                        FieldSchema {
                            name: nk.clone(),
                            inferred_type: infer_type(nv),
                            nullable: nv.is_null(),
                            nested: BTreeMap::new(),
                        },
                    );
                }
            }
            fields.insert(
                key.clone(),
                FieldSchema {
                    name: key.clone(),
                    inferred_type: infer_type(value),
                    nullable: value.is_null(),
                    nested,
                },
            );
        }
    }
    InferredSchema { fields }
}

/// Convert an inferred schema to a JSON Schema document
pub fn to_json_schema(schema: &InferredSchema) -> JsonValue {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();

    for (name, field) in &schema.fields {
        let type_str = match field.inferred_type {
            InferredType::String => "string",
            InferredType::Number => "number",
            InferredType::Boolean => "boolean",
            InferredType::Array => "array",
            InferredType::Object => "object",
            InferredType::Null => "null",
        };

        let mut prop = serde_json::json!({"type": type_str});

        // Add nested properties for objects
        if field.inferred_type == InferredType::Object && !field.nested.is_empty() {
            let nested_schema = InferredSchema {
                fields: field.nested.clone(),
            };
            let nested_json = to_json_schema(&nested_schema);
            if let Some(np) = nested_json.get("properties") {
                prop["properties"] = np.clone();
            }
        }

        properties.insert(name.clone(), prop);
        if !field.nullable {
            required.push(JsonValue::String(name.clone()));
        }
    }

    serde_json::json!({
        "type": "object",
        "properties": properties,
        "required": required,
    })
}

/// Compute the diff between an existing schema and a new payload's inferred schema
pub fn compute_diff(existing: &InferredSchema, new: &InferredSchema) -> SchemaDiff {
    let existing_keys: BTreeSet<&String> = existing.fields.keys().collect();
    let new_keys: BTreeSet<&String> = new.fields.keys().collect();

    let added: Vec<String> = new_keys
        .difference(&existing_keys)
        .map(|k| (*k).clone())
        .collect();
    let removed: Vec<String> = existing_keys
        .difference(&new_keys)
        .map(|k| (*k).clone())
        .collect();

    let mut type_changes = Vec::new();
    for key in existing_keys.intersection(&new_keys) {
        let old_field = &existing.fields[*key];
        let new_field = &new.fields[*key];
        if old_field.inferred_type != new_field.inferred_type
            && old_field.inferred_type != InferredType::Null
            && new_field.inferred_type != InferredType::Null
        {
            type_changes.push(FieldTypeChange {
                field: (*key).clone(),
                old_type: old_field.inferred_type.to_string(),
                new_type: new_field.inferred_type.to_string(),
            });
        }
    }

    // Backward compatible = no type changes, no removed required fields
    let removed_required = removed
        .iter()
        .any(|r| existing.fields.get(r).is_some_and(|f| !f.nullable));

    let is_backward_compatible = type_changes.is_empty() && !removed_required;

    SchemaDiff {
        added_fields: added,
        removed_fields: removed,
        type_changes,
        is_backward_compatible,
    }
}

/// Schema evolution manager — tracks inferred schemas and evolution history
pub struct SchemaEvolutionManager {
    /// Latest inferred schema per event type
    schemas: DashMap<String, InferredSchema>,
    /// Evolution history
    history: DashMap<String, Vec<EvolutionRecord>>,
    /// Version counter per event type
    versions: DashMap<String, u32>,
    history_bytes: Tally,
}

impl Default for SchemaEvolutionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Heap one evolution record holds beyond its slot: its event type and the
/// field names it lists.
fn record_owned_bytes(record: &EvolutionRecord) -> u64 {
    (record.event_type.len()
        + record.added_fields.iter().map(String::len).sum::<usize>()
        + record.removed_fields.iter().map(String::len).sum::<usize>()) as u64
}

impl SchemaEvolutionManager {
    pub fn new() -> Self {
        Self {
            schemas: DashMap::new(),
            history: DashMap::new(),
            versions: DashMap::new(),
            history_bytes: Tally::new(Owner::SchemaEvolution),
        }
    }

    fn push_history(&self, record: EvolutionRecord) {
        self.history_bytes
            .add(std::mem::size_of::<EvolutionRecord>() as u64 + record_owned_bytes(&record));
        self.history
            .entry(record.event_type.clone())
            .or_default()
            .push(record);
    }

    /// Analyze an event and evolve the schema if needed
    ///
    /// Returns the evolution action taken (if any)
    pub fn analyze_event(&self, event_type: &str, payload: &JsonValue) -> Option<EvolutionAction> {
        let new_schema = infer_schema(payload);

        if !self.schemas.contains_key(event_type) {
            // First event of this type — infer and register
            self.schemas.insert(event_type.to_string(), new_schema);
            self.versions.insert(event_type.to_string(), 1);
            self.push_history(EvolutionRecord {
                id: Uuid::new_v4(),
                event_type: event_type.to_string(),
                action: EvolutionAction::Inferred,
                from_version: None,
                to_version: Some(1),
                added_fields: vec![],
                removed_fields: vec![],
                type_changes: vec![],
                timestamp: Utc::now(),
            });
            return Some(EvolutionAction::Inferred);
        }

        // Compare with existing schema
        let existing = self.schemas.get(event_type).unwrap().clone();
        let diff = compute_diff(&existing, &new_schema);

        // No changes
        if diff.added_fields.is_empty()
            && diff.removed_fields.is_empty()
            && diff.type_changes.is_empty()
        {
            return None;
        }

        let current_version = *self.versions.get(event_type).unwrap();

        if diff.is_backward_compatible {
            // Auto-evolve: merge new fields into existing schema
            let mut merged = existing;
            for (key, field) in new_schema.fields {
                merged.fields.entry(key).or_insert(FieldSchema {
                    nullable: true, // New fields are optional
                    ..field
                });
            }

            let new_version = current_version + 1;
            self.schemas.insert(event_type.to_string(), merged);
            self.versions.insert(event_type.to_string(), new_version);
            self.push_history(EvolutionRecord {
                id: Uuid::new_v4(),
                event_type: event_type.to_string(),
                action: EvolutionAction::AutoEvolved,
                from_version: Some(current_version),
                to_version: Some(new_version),
                added_fields: diff.added_fields,
                removed_fields: vec![],
                type_changes: vec![],
                timestamp: Utc::now(),
            });
            Some(EvolutionAction::AutoEvolved)
        } else {
            // Breaking change — record but don't auto-evolve
            self.push_history(EvolutionRecord {
                id: Uuid::new_v4(),
                event_type: event_type.to_string(),
                action: EvolutionAction::BreakingChangeDetected,
                from_version: Some(current_version),
                to_version: None,
                added_fields: diff.added_fields,
                removed_fields: diff.removed_fields,
                type_changes: diff.type_changes,
                timestamp: Utc::now(),
            });
            Some(EvolutionAction::BreakingChangeDetected)
        }
    }

    /// Get the current inferred schema for an event type
    pub fn get_schema(&self, event_type: &str) -> Option<InferredSchema> {
        self.schemas.get(event_type).map(|s| s.clone())
    }

    /// Get the evolution history for an event type
    pub fn get_history(&self, event_type: &str) -> Vec<EvolutionRecord> {
        self.history
            .get(event_type)
            .map(|h| h.clone())
            .unwrap_or_default()
    }

    /// Get all tracked event types
    pub fn list_event_types(&self) -> Vec<String> {
        self.schemas.iter().map(|e| e.key().clone()).collect()
    }

    /// Get the current version for an event type
    pub fn get_version(&self, event_type: &str) -> Option<u32> {
        self.versions.get(event_type).map(|v| *v)
    }

    /// Statistics
    pub fn stats(&self) -> SchemaEvolutionStats {
        let total_evolutions: usize = self.history.iter().map(|h| h.value().len()).sum();
        let breaking_changes: usize = self
            .history
            .iter()
            .map(|h| {
                h.value()
                    .iter()
                    .filter(|r| r.action == EvolutionAction::BreakingChangeDetected)
                    .count()
            })
            .sum();
        SchemaEvolutionStats {
            tracked_event_types: self.schemas.len(),
            total_evolutions,
            breaking_changes,
        }
    }
}

/// Schema evolution statistics
#[derive(Debug, Clone, Serialize)]
pub struct SchemaEvolutionStats {
    pub tracked_event_types: usize,
    pub total_evolutions: usize,
    pub breaking_changes: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_infer_schema_basic() {
        let payload = serde_json::json!({"name": "Alice", "age": 30, "active": true});
        let schema = infer_schema(&payload);
        assert_eq!(schema.fields.len(), 3);
        assert_eq!(schema.fields["name"].inferred_type, InferredType::String);
        assert_eq!(schema.fields["age"].inferred_type, InferredType::Number);
        assert_eq!(schema.fields["active"].inferred_type, InferredType::Boolean);
    }

    #[test]
    fn test_infer_schema_nested() {
        let payload = serde_json::json!({"address": {"city": "NYC", "zip": 10001}});
        let schema = infer_schema(&payload);
        let addr = &schema.fields["address"];
        assert_eq!(addr.inferred_type, InferredType::Object);
        assert_eq!(addr.nested.len(), 2);
        assert_eq!(addr.nested["city"].inferred_type, InferredType::String);
    }

    #[test]
    fn test_to_json_schema() {
        let payload = serde_json::json!({"name": "Alice", "age": 30});
        let schema = infer_schema(&payload);
        let json_schema = to_json_schema(&schema);
        assert_eq!(json_schema["type"], "object");
        assert!(json_schema["properties"]["name"].is_object());
        assert_eq!(json_schema["properties"]["name"]["type"], "string");
    }

    #[test]
    fn test_compute_diff_no_changes() {
        let payload = serde_json::json!({"name": "Alice"});
        let schema = infer_schema(&payload);
        let diff = compute_diff(&schema, &schema);
        assert!(diff.added_fields.is_empty());
        assert!(diff.removed_fields.is_empty());
        assert!(diff.type_changes.is_empty());
        assert!(diff.is_backward_compatible);
    }

    #[test]
    fn test_compute_diff_added_field() {
        let old = infer_schema(&serde_json::json!({"name": "Alice"}));
        let new = infer_schema(&serde_json::json!({"name": "Alice", "email": "a@b.com"}));
        let diff = compute_diff(&old, &new);
        assert_eq!(diff.added_fields, vec!["email"]);
        assert!(diff.is_backward_compatible);
    }

    #[test]
    fn test_compute_diff_type_change() {
        let old = infer_schema(&serde_json::json!({"age": 30}));
        let new = infer_schema(&serde_json::json!({"age": "thirty"}));
        let diff = compute_diff(&old, &new);
        assert_eq!(diff.type_changes.len(), 1);
        assert_eq!(diff.type_changes[0].field, "age");
        assert!(!diff.is_backward_compatible);
    }

    #[test]
    fn test_compute_diff_removed_required_field() {
        let old = infer_schema(&serde_json::json!({"name": "Alice", "age": 30}));
        let new = infer_schema(&serde_json::json!({"name": "Alice"}));
        let diff = compute_diff(&old, &new);
        assert_eq!(diff.removed_fields, vec!["age"]);
        assert!(!diff.is_backward_compatible);
    }

    #[test]
    fn test_manager_first_event_infers() {
        let mgr = SchemaEvolutionManager::new();
        let action = mgr.analyze_event("user.created", &serde_json::json!({"name": "Alice"}));
        assert_eq!(action, Some(EvolutionAction::Inferred));
        assert_eq!(mgr.get_version("user.created"), Some(1));
    }

    #[test]
    fn test_manager_same_schema_no_action() {
        let mgr = SchemaEvolutionManager::new();
        mgr.analyze_event("user.created", &serde_json::json!({"name": "Alice"}));
        let action = mgr.analyze_event("user.created", &serde_json::json!({"name": "Bob"}));
        assert_eq!(action, None);
    }

    #[test]
    fn test_manager_auto_evolve() {
        let mgr = SchemaEvolutionManager::new();
        mgr.analyze_event("user.created", &serde_json::json!({"name": "Alice"}));
        let action = mgr.analyze_event(
            "user.created",
            &serde_json::json!({"name": "Bob", "email": "bob@example.com"}),
        );
        assert_eq!(action, Some(EvolutionAction::AutoEvolved));
        assert_eq!(mgr.get_version("user.created"), Some(2));
        // Email field should now be in the schema
        let schema = mgr.get_schema("user.created").unwrap();
        assert!(schema.fields.contains_key("email"));
    }

    #[test]
    fn test_manager_breaking_change() {
        let mgr = SchemaEvolutionManager::new();
        mgr.analyze_event(
            "user.created",
            &serde_json::json!({"name": "Alice", "age": 30}),
        );
        let action = mgr.analyze_event(
            "user.created",
            &serde_json::json!({"name": "Bob", "age": "thirty"}),
        );
        assert_eq!(action, Some(EvolutionAction::BreakingChangeDetected));
        // Version should NOT have incremented
        assert_eq!(mgr.get_version("user.created"), Some(1));
    }

    #[test]
    fn test_manager_history() {
        let mgr = SchemaEvolutionManager::new();
        mgr.analyze_event("user.created", &serde_json::json!({"name": "Alice"}));
        mgr.analyze_event(
            "user.created",
            &serde_json::json!({"name": "Bob", "email": "b@b.com"}),
        );
        let history = mgr.get_history("user.created");
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].action, EvolutionAction::Inferred);
        assert_eq!(history[1].action, EvolutionAction::AutoEvolved);
    }

    #[test]
    fn test_manager_stats() {
        let mgr = SchemaEvolutionManager::new();
        mgr.analyze_event("user.created", &serde_json::json!({"name": "Alice"}));
        mgr.analyze_event("order.placed", &serde_json::json!({"total": 99.99}));
        let stats = mgr.stats();
        assert_eq!(stats.tracked_event_types, 2);
        assert_eq!(stats.total_evolutions, 2);
        assert_eq!(stats.breaking_changes, 0);
    }

    #[test]
    fn test_manager_list_event_types() {
        let mgr = SchemaEvolutionManager::new();
        mgr.analyze_event("user.created", &serde_json::json!({"name": "Alice"}));
        mgr.analyze_event("order.placed", &serde_json::json!({"total": 99.99}));
        let types = mgr.list_event_types();
        assert_eq!(types.len(), 2);
        assert!(types.contains(&"user.created".to_string()));
        assert!(types.contains(&"order.placed".to_string()));
    }

    #[test]
    fn test_null_field_compatible() {
        let old = infer_schema(&serde_json::json!({"name": "Alice", "bio": null}));
        let new = infer_schema(&serde_json::json!({"name": "Alice", "bio": "Hello"}));
        let diff = compute_diff(&old, &new);
        // null → string is not a breaking change (null is compatible with any type)
        assert!(diff.type_changes.is_empty());
        assert!(diff.is_backward_compatible);
    }
}
