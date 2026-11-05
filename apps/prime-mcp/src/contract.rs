//! Argument parsing and result shaping for typed writes, kinds and provenance,
//! shared by the embedded dispatcher, the hosted dispatcher and the HTTP API so
//! the three surfaces accept and refuse the same inputs with the same text.

use allsource_core::{
    AllSourceError,
    prime::{
        Assertion, AssertionInput, EnforcementMode, PrimeError, SchemaEntry, SchemaKind,
        projections::{
            MergePolicy, ProjectionDef,
            declarative::{Observation, provenance_for_field},
        },
    },
};
use serde_json::{Value, json};

/// Key that carries the assertion source on edge writes, where `source` is the
/// edge's source node. Node writes accept it too.
pub const ASSERTION_SOURCE_KEY: &str = "assertion_source";

/// Refusal text for an embedded write. Validation refusals carry the bare
/// message so it matches the hosted refusal byte for byte.
pub fn prime_error_text(e: &PrimeError) -> String {
    e.validation_message().unwrap_or_else(|| e.to_string())
}

/// Refusal text for a hosted write; see [`prime_error_text`].
pub fn hosted_error_text(e: &AllSourceError) -> String {
    match e {
        AllSourceError::ValidationError(msg) => msg.clone(),
        other => other.to_string(),
    }
}

/// Read optional assertion fields from tool or HTTP arguments.
///
/// `source_key` names the field holding the assertion source: `source` for
/// nodes, [`ASSERTION_SOURCE_KEY`] for edges. `None` when no field was sent.
pub fn assertion_from_args(args: &Value, source_key: &str) -> Result<Option<Assertion>, String> {
    let text = |key: &str| -> Result<Option<String>, String> {
        match args.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => Ok(Some(s.clone())),
            Some(other) => Err(format!(
                "assertion.{key}: expected a string, received {other}"
            )),
        }
    };
    let source = match text(source_key)? {
        Some(s) => Some(s),
        None if source_key != ASSERTION_SOURCE_KEY => text(ASSERTION_SOURCE_KEY)?,
        None => None,
    };
    let derived_from = match args.get("derived_from") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(vec![s.clone()]),
        Some(Value::Array(items)) => Some(
            items
                .iter()
                .map(|v| {
                    v.as_str().map(str::to_owned).ok_or_else(|| {
                        format!("assertion.derived_from: expected strings, received {v}")
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
        ),
        Some(other) => {
            return Err(format!(
                "assertion.derived_from: expected an array of strings, received {other}"
            ));
        }
    };
    let confidence = match args.get("confidence") {
        None | Some(Value::Null) => None,
        Some(v) => Some(v.as_f64().ok_or_else(|| {
            format!("assertion.confidence: expected a number in 0.0..=1.0, received {v}")
        })?),
    };
    let source_priority = match args.get("source_priority") {
        None | Some(Value::Null) => None,
        Some(v) => Some(
            v.as_i64()
                .and_then(|i| i32::try_from(i).ok())
                .ok_or_else(|| {
                    format!("assertion.source_priority: expected an integer, received {v}")
                })?,
        ),
    };
    AssertionInput {
        source,
        actor: text("actor")?,
        model: text("model")?,
        run_id: text("run_id")?,
        derived_from,
        confidence,
        source_priority,
    }
    .into_assertion()
    .map_err(|e| prime_error_text(&e))
}

/// JSON Schema properties for the optional assertion fields, merged into the
/// node, update and edge tool schemas.
pub fn assertion_schema(source_key: &str) -> Value {
    let mut props = json!({
        "actor": { "type": "string", "description": "Who made the assertion (user id, agent name)." },
        "model": { "type": "string", "description": "Model that produced the fact, if any." },
        "run_id": { "type": "string", "description": "Run or job that produced the fact." },
        "derived_from": { "type": "array", "items": { "type": "string" }, "description": "Entity ids, documents or URLs the fact was derived from." },
        "confidence": { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "Confidence in 0.0..=1.0. Not used to pick a winner." },
        "source_priority": { "type": "integer", "description": "Overrides the priority derived from the source (user 1000, import 500, agent 100, extraction 0)." }
    });
    props[source_key] = json!({
        "type": "string",
        "enum": ["user", "agent", "extraction", "import"],
        "description": "Who asserted the fact. Defaults to agent when any assertion field is sent."
    });
    props
}

/// Merge [`assertion_schema`] into a tool's `inputSchema.properties`.
pub fn with_assertion_schema(mut tool: Value, source_key: &str) -> Value {
    if let (Some(props), Value::Object(extra)) = (
        tool.pointer_mut("/inputSchema/properties")
            .and_then(Value::as_object_mut),
        assertion_schema(source_key),
    ) {
        props.extend(extra);
    }
    tool
}

/// Parse `node` / `edge`.
pub fn parse_kind(args: &Value) -> Result<SchemaKind, String> {
    let raw = args.get("kind").and_then(Value::as_str).unwrap_or("");
    SchemaKind::parse(raw)
        .ok_or_else(|| format!("kind: expected node or edge, received {}", json!(raw)))
}

/// Parse `open` / `warn` / `strict`.
pub fn parse_mode(args: &Value) -> Result<EnforcementMode, String> {
    let raw = args.get("mode").and_then(Value::as_str).unwrap_or("");
    EnforcementMode::parse(raw).ok_or_else(|| {
        format!(
            "mode: expected open, warn or strict, received {}",
            json!(raw)
        )
    })
}

/// Wire shape of a registered kind.
pub fn kind_json(e: &SchemaEntry) -> Value {
    json!({
        "type_name": e.type_name,
        "kind": e.kind.as_str(),
        "version": e.version,
        "schema": e.schema,
    })
}

/// Wire shape of the kind list.
pub fn kinds_json(kinds: &[SchemaEntry], mode: EnforcementMode) -> Value {
    let types: Vec<Value> = kinds.iter().map(kind_json).collect();
    json!({ "total": types.len(), "types": types, "enforcement": mode.as_str() })
}

/// `node:{type}:{id}` → `type`.
pub fn entity_type_from_node_id(node_id: &str) -> Option<&str> {
    let mut parts = node_id.splitn(3, ':');
    if parts.next()? != "node" {
        return None;
    }
    let entity_type = parts.next()?;
    parts.next()?;
    Some(entity_type)
}

/// Provenance for one field. A field with no registered policy, or a node
/// type with no projection, is answered under `last_write`;
/// `policy_source` says which applied.
pub fn provenance_json(
    node_id: &str,
    field: &str,
    observations: &[Observation],
    def: Option<ProjectionDef>,
) -> Result<Value, String> {
    let entity_type = entity_type_from_node_id(node_id).ok_or_else(|| {
        format!("node_id must be in 'node:{{type}}:{{id}}' format, got: {node_id}")
    })?;
    let (def, policy_source) = match def {
        Some(d) if d.field_policies.contains_key(field) => (d, "projection"),
        _ => (
            ProjectionDef::new(entity_type, [(field.to_string(), MergePolicy::LastWrite)]),
            "default",
        ),
    };
    let p = provenance_for_field(observations, &def, field).ok_or_else(|| {
        format!("no provenance available for field '{field}' on {node_id}: no observation sets it")
    })?;
    Ok(json!({
        "field": p.field,
        "value": p.value,
        "source_event_id": p.source_event_id,
        "source_event_at": p.source_event_at,
        "merge_policy_applied": p.merge_policy_applied,
        "policy_source": policy_source,
        "source_priority": p.source_priority,
        "assertion": p.assertion,
        "contributors": p.contributors,
    }))
}

/// Refuse a caller-supplied tenant that differs from the authenticated one.
/// Absent is fine; a matching value is fine.
pub fn conflicting_tenant(supplied: Option<&Value>, authenticated: &str) -> Option<String> {
    match supplied {
        None | Some(Value::Null) => None,
        Some(v) if v.as_str() == Some(authenticated) => None,
        Some(v) => Some(format!(
            "tenant_id {v} conflicts with the authenticated tenant; the tenant is taken from \
             the credential, never from the request"
        )),
    }
}

#[cfg(test)]
#[path = "contract_tests.rs"]
mod tests;
