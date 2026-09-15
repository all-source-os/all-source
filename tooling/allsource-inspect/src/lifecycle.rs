//! Fold many entities' lifecycle events into one line each.
//!
//! "Which runs are parked right now" is a fold over every run's events, not a
//! lookup of one entity, so neither `events` nor the MCP's per-entity
//! `explain_entity` answers it. An entity's state is the suffix of its newest
//! event whose type is `<prefix><state>` for one of the named states; other
//! events under the prefix are counted but never change the state.

use std::cmp::Reverse;
use std::collections::BTreeMap;

use allsource_core::embedded::EventView;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use crate::filter::project;

pub struct LifecycleSpec<'a> {
    pub prefix: &'a str,
    pub states: &'a [String],
    /// Payload field that names the entity when events for one logical thing
    /// are written under different entity ids. Defaults to `entity_id`.
    pub key_field: Option<&'a str>,
    pub fields: &'a [String],
}

struct Entity {
    state: Option<(DateTime<Utc>, String, Value)>,
    first_at: DateTime<Utc>,
    events: usize,
}

fn key_of(event: &EventView, key_field: Option<&str>) -> String {
    key_field
        .and_then(|field| event.payload.get(field))
        .and_then(Value::as_str)
        .map_or_else(|| event.entity_id.clone(), str::to_string)
}

/// One line per entity that reached at least one named state, newest state first.
pub fn fold(events: &[EventView], spec: &LifecycleSpec<'_>) -> Vec<Value> {
    let mut entities: BTreeMap<String, Entity> = BTreeMap::new();
    for event in events {
        let Some(suffix) = event.event_type.strip_prefix(spec.prefix) else {
            continue;
        };
        let entity = entities
            .entry(key_of(event, spec.key_field))
            .or_insert_with(|| Entity {
                state: None,
                first_at: event.timestamp,
                events: 0,
            });
        entity.events += 1;
        entity.first_at = entity.first_at.min(event.timestamp);
        if spec.states.iter().any(|state| state == suffix)
            && entity
                .state
                .as_ref()
                .is_none_or(|(at, _, _)| event.timestamp >= *at)
        {
            let fields = project(&event.payload, spec.fields);
            entity.state = Some((event.timestamp, suffix.to_string(), fields));
        }
    }

    let mut lines: Vec<(DateTime<Utc>, Value)> = entities
        .into_iter()
        .filter_map(|(key, entity)| {
            let (at, state, fields) = entity.state?;
            Some((
                at,
                json!({
                    "key": key,
                    "state": state,
                    "state_at": at,
                    "first_at": entity.first_at,
                    "events": entity.events,
                    "fields": fields,
                }),
            ))
        })
        .collect();
    lines.sort_by_key(|(at, _)| Reverse(*at));
    lines.into_iter().map(|(_, line)| line).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use uuid::Uuid;

    fn event(event_type: &str, entity_id: &str, payload: Value, secs: i64) -> EventView {
        EventView {
            id: Uuid::new_v4(),
            event_type: event_type.to_string(),
            entity_id: entity_id.to_string(),
            tenant_id: "default".to_string(),
            payload,
            metadata: None,
            timestamp: Utc
                .timestamp_opt(1_789_000_000 + secs, 0)
                .single()
                .expect("valid time"),
            version: 1,
        }
    }

    fn states() -> Vec<String> {
        ["started", "completed", "parked"]
            .map(String::from)
            .to_vec()
    }

    #[test]
    fn state_is_the_newest_named_state_even_when_read_out_of_order() {
        let states = states();
        let fields = vec!["step".to_string()];
        let spec = LifecycleSpec {
            prefix: "run.",
            states: &states,
            key_field: None,
            fields: &fields,
        };
        let events = [
            event("run.parked", "r1", json!({"step": 3}), 10),
            event("run.started", "r1", json!({"step": 0}), 1),
            event("run.heartbeat", "r1", json!({}), 20),
        ];
        let lines = fold(&events, &spec);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0]["state"], "parked");
        assert_eq!(
            lines[0]["fields"],
            json!({"step": 3}),
            "fields come from the state-setting event"
        );
        assert_eq!(
            lines[0]["events"], 3,
            "unnamed events count but do not set state"
        );
    }

    #[test]
    fn a_key_field_groups_events_written_under_different_entity_ids() {
        let states = states();
        let spec = LifecycleSpec {
            prefix: "run.",
            states: &states,
            key_field: Some("run_id"),
            fields: &[],
        };
        let events = [
            event("run.started", "entity-a", json!({"run_id": "r1"}), 1),
            event("run.completed", "entity-b", json!({"run_id": "r1"}), 2),
        ];
        let lines = fold(&events, &spec);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0]["key"], "r1");
        assert_eq!(lines[0]["state"], "completed");
    }

    #[test]
    fn entities_without_a_named_state_are_left_out_and_newest_come_first() {
        let states = states();
        let spec = LifecycleSpec {
            prefix: "run.",
            states: &states,
            key_field: None,
            fields: &[],
        };
        let events = [
            event("run.heartbeat", "ghost", json!({}), 1),
            event("run.started", "old", json!({}), 2),
            event("run.started", "new", json!({}), 3),
            event("other.started", "elsewhere", json!({}), 4),
        ];
        let keys: Vec<Value> = fold(&events, &spec)
            .iter()
            .map(|line| line["key"].clone())
            .collect();
        assert_eq!(keys, [json!("new"), json!("old")]);
    }
}
