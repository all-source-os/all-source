//! Payload filtering and shaping for `events`.
//!
//! Agent-facing stores carry multi-kilobyte payloads (model output, tool
//! transcripts). Printing them whole floods a terminal or an LLM context, so an
//! event can be narrowed to its top-level keys or to named dot paths.

use allsource_core::embedded::EventView;
use serde_json::{Map, Value, json};

/// True when every needle occurs in the payload's JSON text.
pub fn payload_contains(payload: &Value, needles: &[String]) -> bool {
    if needles.is_empty() {
        return true;
    }
    let text = payload.to_string();
    needles.iter().all(|needle| text.contains(needle.as_str()))
}

/// Select `a.b.c` paths from a payload. Numeric segments index arrays; a path
/// that does not resolve renders as `null` so the caller can see it was asked for.
pub fn project(payload: &Value, fields: &[String]) -> Value {
    let mut out = Map::new();
    for field in fields {
        let pointer = format!("/{}", field.replace('.', "/"));
        let value = payload.pointer(&pointer).cloned().unwrap_or(Value::Null);
        out.insert(field.clone(), value);
    }
    Value::Object(out)
}

/// How much of each payload to print.
pub enum PayloadShape<'a> {
    Full,
    Keys,
    Fields(&'a [String]),
}

impl<'a> PayloadShape<'a> {
    pub fn from_flags(keys: bool, fields: &'a [String]) -> Self {
        if keys {
            Self::Keys
        } else if fields.is_empty() {
            Self::Full
        } else {
            Self::Fields(fields)
        }
    }

    pub fn apply(&self, payload: &Value) -> Value {
        match self {
            Self::Full => payload.clone(),
            Self::Keys => payload.as_object().map_or(Value::Null, |obj| {
                Value::from(obj.keys().cloned().collect::<Vec<_>>())
            }),
            Self::Fields(fields) => project(payload, fields),
        }
    }
}

/// One event as a JSON line, with the payload shaped.
///
/// `metadata` carries correlation and source identity. Omitting it here while
/// `wal --format json` still prints it would give the two JSON paths different
/// event shapes, and a consumer reading correlation IDs from `events` would get
/// a missing field rather than an error. Only the payload is ever shaped.
pub fn render(event: &EventView, shape: &PayloadShape<'_>) -> Value {
    json!({
        "id": event.id,
        "timestamp": event.timestamp,
        "event_type": event.event_type,
        "entity_id": event.entity_id,
        "tenant_id": event.tenant_id,
        "version": event.version,
        "metadata": event.metadata,
        "payload": shape.apply(&event.payload),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_needle_must_appear_in_the_payload_text() {
        let payload = json!({"run_id": "r1", "output": "{\"domain\":\"example.com\"}"});
        assert!(payload_contains(&payload, &[]));
        assert!(payload_contains(
            &payload,
            &["r1".into(), "example.com".into()]
        ));
        assert!(!payload_contains(&payload, &["r1".into(), "absent".into()]));
    }

    #[test]
    fn fields_walk_objects_and_arrays_and_missing_paths_render_null() {
        let payload =
            json!({"id": "w1", "steps": [{"skill": "a"}, {"skill": "b", "approval": true}]});
        let projected = project(
            &payload,
            &[
                "id".into(),
                "steps.1.approval".into(),
                "steps.9.skill".into(),
            ],
        );
        assert_eq!(
            projected,
            json!({"id": "w1", "steps.1.approval": true, "steps.9.skill": null})
        );
    }

    #[test]
    fn keys_wins_over_fields_and_empty_fields_mean_the_whole_payload() {
        let payload = json!({"b": 1, "a": {"deep": true}});
        let fields = vec!["a.deep".to_string()];
        assert_eq!(
            PayloadShape::from_flags(true, &fields).apply(&payload),
            json!(["a", "b"])
        );
        assert_eq!(
            PayloadShape::from_flags(false, &[]).apply(&payload),
            payload
        );
        assert_eq!(
            PayloadShape::from_flags(false, &fields).apply(&payload),
            json!({"a.deep": true})
        );
    }
}
