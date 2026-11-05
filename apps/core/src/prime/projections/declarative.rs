//! Declarative entity-state projection with per-field merge policies.
//!
//! Closes the headline differentiator from neotoma-comparison.md §"Gap 1":
//! Neotoma ships built-in deterministic reducers with four declarative merge
//! policies (`last_write`, `highest_priority`, `most_specific`, `merge_array`);
//! AllSource users today have to write their own fold over the event stream
//! to get current entity state. This module supplies the equivalent.
//!
//! # Determinism guarantee
//!
//! `fold(observations, def)` produces the same snapshot regardless of the
//! observation input order, given the same merge policy per field. This is
//! the load-bearing contract — it's enforced by
//! `tests::fold_is_order_independent_for_each_policy` (not a doc link: the
//! `tests` module is `#[cfg(test)]`, so it does not exist in a docs build)
//! and the proptest in this file. Read it before changing any policy
//! implementation: the moment a policy reads input order without going
//! through one of `observed_at`, `source_priority`, or `specificity_score`,
//! the contract breaks and downstream "what did I know about X as of Y?"
//! queries become non-reproducible.
//!
//! # Scope (v0.21.5 cut)
//!
//! This module is the **pure folding library**. It deliberately does not:
//! - Define an MCP tool (`prime_define_projection`) — that wiring lands in
//!   apps/prime-mcp once this primitive stabilises
//! - Persist [`ProjectionDef`]s to a store — definitions are passed in by
//!   the caller for now
//! - Integrate with [`crate::prime::projections::node_state::NodeStateProjection`] —
//!   the existing fixed-merge projection stays the default; declarative
//!   projection is opt-in per entity_type
//!
//! Those follow-ups are tracked under bead t-4d54 of the neotoma-gaps epic.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::prime::assertion::Assertion;

/// How to reconcile multiple observations of the same field.
///
/// Declarative — chosen per field, never inline code. This is what makes the
/// fold deterministic: each policy reads observation metadata (`observed_at`
/// for `LastWrite`, `source_priority` for `HighestPriority`, etc.) instead
/// of input order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MergePolicy {
    /// Pick the value from the observation with the latest `observed_at`.
    /// Right for fields that change over time — status, amount, address.
    LastWrite,
    /// Pick the value from the observation with the highest `source_priority`.
    /// Right for identity-shaped fields where a user correction (high priority)
    /// must beat a structured agent write or AI extraction (low priority).
    HighestPriority,
    /// Pick the value from the observation with the highest `specificity_score`.
    /// Right when one source produces dense schema-aligned facts and another
    /// produces shallow ones — prefer the dense source.
    MostSpecific,
    /// Union all observed values for this field. Each observation contributes
    /// its value (or, if the value is itself an array, its elements). Duplicates
    /// are dropped via stable JSON equality. Right for aliases, tags, accumulating
    /// sets.
    MergeArray,
}

/// A single observed fact about an entity at a point in time.
///
/// `fields` is intentionally a `BTreeMap<String, Value>` rather than a free
/// `serde_json::Value` so iteration order is canonical — load-bearing for
/// the determinism guarantee.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    /// When this fact became true. Used by `MergePolicy::LastWrite`.
    pub observed_at: DateTime<Utc>,
    /// When the observation was filed. Breaks `observed_at` ties under
    /// `MergePolicy::LastWrite`, so a correction filed later with the same
    /// valid time wins. Unset observations sort before filed ones.
    #[serde(default)]
    pub recorded_at: Option<DateTime<Utc>>,
    /// Source quality / trust level. Higher = more authoritative. Used by
    /// `MergePolicy::HighestPriority`. Convention: user correction = 1000,
    /// structured agent write = 100, AI extraction = 0.
    #[serde(default)]
    pub source_priority: i32,
    /// How dense / schema-aligned this observation is. Higher = denser.
    /// Used by `MergePolicy::MostSpecific`.
    #[serde(default)]
    pub specificity_score: i32,
    /// Stable identifier of the source event — kept for provenance lookups
    /// (per the per-field provenance bead t-af6f that will land on top of
    /// this primitive).
    pub source_event_id: String,
    /// The observed field values.
    pub fields: BTreeMap<String, Value>,
    /// Who asserted this observation. Never consulted by any merge policy.
    #[serde(default)]
    pub assertion: Assertion,
}

/// Per-entity-type folding rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectionDef {
    /// Which entity type these rules apply to (e.g. `"contact"`).
    pub entity_type: String,
    /// Policy per field. Fields not in this map are dropped from the snapshot
    /// — the caller must opt-in every field they want surfaced. (Strict by
    /// default. Future versions may add a `fallback_policy` for unknown fields.)
    pub field_policies: BTreeMap<String, MergePolicy>,
}

impl ProjectionDef {
    /// Build a definition from a list of (field, policy) pairs. Convenience
    /// for tests and inline construction; production code typically loads
    /// definitions from a registry.
    pub fn new<I>(entity_type: impl Into<String>, fields: I) -> Self
    where
        I: IntoIterator<Item = (String, MergePolicy)>,
    {
        Self {
            entity_type: entity_type.into(),
            field_policies: fields.into_iter().collect(),
        }
    }
}

/// Snapshot of an entity after folding observations through a projection
/// definition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EntitySnapshot {
    pub entity_type: String,
    /// Final value per field after applying the merge policy.
    pub fields: BTreeMap<String, Value>,
}

/// Fold a set of observations into a snapshot per the projection definition.
///
/// Determinism contract: same inputs (any order) always yield the same
/// output. Tested by `tests::fold_is_order_independent_for_each_policy` and a proptest.
///
/// Fields not in `def.field_policies` are dropped — the projection defines
/// what's surfaced.
pub fn fold(observations: &[Observation], def: &ProjectionDef) -> EntitySnapshot {
    let mut out: BTreeMap<String, Value> = BTreeMap::new();
    for (field, policy) in &def.field_policies {
        if let Some(value) = apply_policy(*policy, field, observations) {
            out.insert(field.clone(), value);
        }
    }
    EntitySnapshot {
        entity_type: def.entity_type.clone(),
        fields: out,
    }
}

fn apply_policy(policy: MergePolicy, field: &str, observations: &[Observation]) -> Option<Value> {
    apply_policy_with_source(policy, field, observations).map(|(value, _source)| value)
}

/// Same as [`apply_policy`] but additionally returns the single observation
/// that contributed the value, when one exists. Returns `None` for the
/// observation slot under [`MergePolicy::MergeArray`] because the union
/// pulls from many — there's no single picker.
///
/// This is what [`provenance_for_field`] sits on top of; keeping it as a
/// shared helper means `fold` and `fold_with_provenance` agree exactly on
/// which observation wins, by construction.
fn apply_policy_with_source<'a>(
    policy: MergePolicy,
    field: &str,
    observations: &'a [Observation],
) -> Option<(Value, Option<&'a Observation>)> {
    match policy {
        MergePolicy::LastWrite => pick_observation(observations, field, |a, b| {
            a.observed_at
                .cmp(&b.observed_at)
                .then_with(|| a.recorded_at.cmp(&b.recorded_at))
                // Stable tiebreak: when two observations share observed_at,
                // prefer the one with the lexicographically smaller
                // source_event_id so input order can't change the result.
                .then_with(|| b.source_event_id.cmp(&a.source_event_id))
        })
        .map(|o| (o.fields[field].clone(), Some(o))),

        MergePolicy::HighestPriority => pick_observation(observations, field, |a, b| {
            a.source_priority
                .cmp(&b.source_priority)
                .then_with(|| a.observed_at.cmp(&b.observed_at))
                .then_with(|| b.source_event_id.cmp(&a.source_event_id))
        })
        .map(|o| (o.fields[field].clone(), Some(o))),

        MergePolicy::MostSpecific => pick_observation(observations, field, |a, b| {
            a.specificity_score
                .cmp(&b.specificity_score)
                .then_with(|| a.observed_at.cmp(&b.observed_at))
                .then_with(|| b.source_event_id.cmp(&a.source_event_id))
        })
        .map(|o| (o.fields[field].clone(), Some(o))),

        MergePolicy::MergeArray => merge_array(observations, field).map(|value| (value, None)),
    }
}

/// Generic single-picker: pick the observation that maximises the given
/// ordering (among those that contain the field).
fn pick_observation<'a, F>(
    observations: &'a [Observation],
    field: &str,
    mut cmp: F,
) -> Option<&'a Observation>
where
    F: FnMut(&Observation, &Observation) -> std::cmp::Ordering,
{
    observations
        .iter()
        .filter(|o| o.fields.contains_key(field))
        .max_by(|a, b| cmp(a, b))
}

fn merge_array(observations: &[Observation], field: &str) -> Option<Value> {
    // Collect every observed value (scalar OR array elements), dedupe by
    // stable JSON equality, and emit in a canonical order (sorted by JSON
    // string representation) so the result doesn't depend on observation
    // input order.
    let mut collected: Vec<Value> = Vec::new();
    for obs in observations {
        if let Some(v) = obs.fields.get(field) {
            match v {
                Value::Array(items) => {
                    for item in items {
                        if !collected.contains(item) {
                            collected.push(item.clone());
                        }
                    }
                }
                other => {
                    if !collected.contains(other) {
                        collected.push(other.clone());
                    }
                }
            }
        }
    }
    if collected.is_empty() {
        None
    } else {
        collected.sort_by(|a, b| {
            serde_json::to_string(a)
                .unwrap_or_default()
                .cmp(&serde_json::to_string(b).unwrap_or_default())
        });
        Some(Value::Array(collected))
    }
}

// ─── Provenance ───────────────────────────────────────────────────────────

/// One event that contributed to a field's value.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Contributor {
    pub source_event_id: String,
    pub observed_at: DateTime<Utc>,
    pub source_priority: i32,
    pub assertion: Assertion,
}

impl From<&Observation> for Contributor {
    fn from(o: &Observation) -> Self {
        Self {
            source_event_id: o.source_event_id.clone(),
            observed_at: o.observed_at,
            source_priority: o.source_priority,
            assertion: o.assertion.clone(),
        }
    }
}

/// Provenance for a single field's value in a projected snapshot.
///
/// For single-winner policies `source_event_id` is the winning event and
/// `contributors` holds just that event. For `MergeArray` the value is a
/// union: `contributors` lists every event carrying the field, ordered by
/// `(observed_at, source_event_id)`, and the single-source fields describe the
/// most recent of them.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Provenance {
    /// Field whose provenance this describes.
    pub field: String,
    /// The current value for this field after folding.
    pub value: Value,
    /// Event that supplied this value.
    pub source_event_id: String,
    /// Timestamp on that event (the `observed_at` of the winning observation).
    pub source_event_at: DateTime<Utc>,
    /// Which merge policy was in effect — useful for debugging "why did this
    /// value win over that one?" without re-reading the projection def.
    pub merge_policy_applied: MergePolicy,
    #[serde(default)]
    pub source_priority: i32,
    #[serde(default)]
    pub assertion: Assertion,
    #[serde(default)]
    pub contributors: Vec<Contributor>,
}

/// Per-field provenance for an entire snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProvenanceSnapshot {
    pub entity_type: String,
    pub fields: BTreeMap<String, Provenance>,
}

/// Return the provenance for a single field. `None` if the field has no
/// observations or isn't in the projection def.
pub fn provenance_for_field(
    observations: &[Observation],
    def: &ProjectionDef,
    field: &str,
) -> Option<Provenance> {
    let policy = *def.field_policies.get(field)?;
    let (value, winner) = apply_policy_with_source(policy, field, observations)?;
    let (head, contributors) = if let Some(w) = winner {
        (w, vec![Contributor::from(w)])
    } else {
        let mut carrying: Vec<&Observation> = observations
            .iter()
            .filter(|o| o.fields.contains_key(field))
            .collect();
        carrying.sort_by(|a, b| {
            a.observed_at
                .cmp(&b.observed_at)
                .then_with(|| a.source_event_id.cmp(&b.source_event_id))
        });
        carrying.dedup_by(|a, b| a.source_event_id == b.source_event_id);
        let head = pick_observation(observations, field, |a, b| {
            a.observed_at
                .cmp(&b.observed_at)
                .then_with(|| b.source_event_id.cmp(&a.source_event_id))
        })?;
        (head, carrying.into_iter().map(Contributor::from).collect())
    };
    Some(Provenance {
        field: field.to_string(),
        value,
        source_event_id: head.source_event_id.clone(),
        source_event_at: head.observed_at,
        merge_policy_applied: policy,
        source_priority: head.source_priority,
        assertion: head.assertion.clone(),
        contributors,
    })
}

/// Fold observations into a snapshot AND record provenance for every field
/// in the definition that has observations.
pub fn fold_with_provenance(
    observations: &[Observation],
    def: &ProjectionDef,
) -> ProvenanceSnapshot {
    let mut fields: BTreeMap<String, Provenance> = BTreeMap::new();
    for field in def.field_policies.keys() {
        if let Some(prov) = provenance_for_field(observations, def, field) {
            fields.insert(field.clone(), prov);
        }
    }
    ProvenanceSnapshot {
        entity_type: def.entity_type.clone(),
        fields,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use serde_json::json;

    // Test convenience: `json!({...})` returns a `Value` by value and
    // borrowing every call-site would be noise. Intentional.
    #[allow(clippy::needless_pass_by_value)]
    fn obs(
        ts: i64,
        priority: i32,
        specificity: i32,
        id: &str,
        fields: serde_json::Value,
    ) -> Observation {
        let map = fields
            .as_object()
            .expect("test obs needs an object")
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        Observation {
            observed_at: Utc.timestamp_opt(ts, 0).single().unwrap(),
            recorded_at: None,
            source_priority: priority,
            specificity_score: specificity,
            source_event_id: id.to_string(),
            fields: map,
            assertion: Assertion::default(),
        }
    }

    fn def(field: &str, policy: MergePolicy) -> ProjectionDef {
        ProjectionDef::new("contact", vec![(field.to_string(), policy)])
    }

    // ─── LastWrite ────────────────────────────────────────────────────────

    #[test]
    fn last_write_picks_latest_observed_at() {
        let obs = vec![
            obs(100, 0, 0, "e1", json!({"status": "cold"})),
            obs(200, 0, 0, "e2", json!({"status": "warm"})),
            obs(150, 0, 0, "e3", json!({"status": "lukewarm"})),
        ];
        let snap = fold(&obs, &def("status", MergePolicy::LastWrite));
        assert_eq!(snap.fields.get("status"), Some(&json!("warm")));
    }

    #[test]
    fn last_write_with_tied_timestamps_breaks_by_event_id() {
        // Stable tiebreaker: smaller event_id wins. Without this, input
        // order would decide and the determinism guarantee breaks.
        let obs = vec![
            obs(100, 0, 0, "evt-b", json!({"status": "B"})),
            obs(100, 0, 0, "evt-a", json!({"status": "A"})),
        ];
        let snap = fold(&obs, &def("status", MergePolicy::LastWrite));
        assert_eq!(snap.fields.get("status"), Some(&json!("A")));
    }

    // ─── HighestPriority ──────────────────────────────────────────────────

    #[test]
    fn highest_priority_user_correction_beats_ai_extraction() {
        let obs = vec![
            obs(100, 0, 0, "ai1", json!({"name": "alice chen"})), // AI extraction
            obs(150, 100, 0, "agent1", json!({"name": "Alice C."})), // structured
            obs(200, 1000, 0, "user1", json!({"name": "Alice Chen"})), // user correction
        ];
        let snap = fold(&obs, &def("name", MergePolicy::HighestPriority));
        assert_eq!(snap.fields.get("name"), Some(&json!("Alice Chen")));
    }

    // ─── MostSpecific ─────────────────────────────────────────────────────

    #[test]
    fn most_specific_picks_densest_observation() {
        let obs = vec![
            obs(100, 0, 1, "shallow", json!({"role": "engineer"})),
            obs(
                100,
                0,
                10,
                "dense",
                json!({"role": "Staff Engineer, Platform"}),
            ),
        ];
        let snap = fold(&obs, &def("role", MergePolicy::MostSpecific));
        assert_eq!(
            snap.fields.get("role"),
            Some(&json!("Staff Engineer, Platform"))
        );
    }

    // ─── MergeArray ───────────────────────────────────────────────────────

    #[test]
    fn merge_array_unions_scalars_into_a_sorted_set() {
        let obs = vec![
            obs(100, 0, 0, "e1", json!({"tags": "rust"})),
            obs(200, 0, 0, "e2", json!({"tags": "agents"})),
            obs(300, 0, 0, "e3", json!({"tags": "rust"})), // dup
        ];
        let snap = fold(&obs, &def("tags", MergePolicy::MergeArray));
        assert_eq!(snap.fields.get("tags"), Some(&json!(["agents", "rust"])));
    }

    #[test]
    fn merge_array_flattens_array_inputs() {
        let obs = vec![
            obs(100, 0, 0, "e1", json!({"tags": ["a", "b"]})),
            obs(200, 0, 0, "e2", json!({"tags": ["b", "c"]})),
        ];
        let snap = fold(&obs, &def("tags", MergePolicy::MergeArray));
        assert_eq!(snap.fields.get("tags"), Some(&json!(["a", "b", "c"])));
    }

    // ─── Cross-cutting ────────────────────────────────────────────────────

    #[test]
    fn fields_not_in_definition_are_dropped() {
        let obs = vec![obs(
            100,
            0,
            0,
            "e1",
            json!({"name": "Alice", "secret": "x"}),
        )];
        let snap = fold(&obs, &def("name", MergePolicy::LastWrite));
        assert_eq!(snap.fields.len(), 1);
        assert!(snap.fields.contains_key("name"));
        assert!(!snap.fields.contains_key("secret"));
    }

    #[test]
    fn missing_field_yields_empty_snapshot_field() {
        let obs = vec![obs(100, 0, 0, "e1", json!({"other": "x"}))];
        let snap = fold(&obs, &def("name", MergePolicy::LastWrite));
        assert!(snap.fields.is_empty());
    }

    /// Hand-rolled determinism check: explicit known-permutations test for
    /// each of the four policies. Paired with the proptest below — proptest
    /// gives breadth, this gives a fast-failing canary for the common case.
    #[test]
    fn fold_is_order_independent_for_each_policy() {
        let policies = [
            MergePolicy::LastWrite,
            MergePolicy::HighestPriority,
            MergePolicy::MostSpecific,
            MergePolicy::MergeArray,
        ];
        for &policy in &policies {
            let a = obs(100, 1, 1, "e1", json!({"f": 1}));
            let b = obs(200, 2, 2, "e2", json!({"f": 2}));
            let c = obs(150, 3, 3, "e3", json!({"f": 3}));
            let def = def("f", policy);
            let s1 = fold(&[a.clone(), b.clone(), c.clone()], &def);
            let s2 = fold(&[c, b, a], &def);
            assert_eq!(s1, s2, "policy {policy:?} is order-dependent — bug");
        }
    }

    // ─── Provenance ───────────────────────────────────────────────────────

    #[test]
    fn provenance_for_last_write_picks_winning_event() {
        let obs = vec![
            obs(100, 0, 0, "evt-a", json!({"status": "cold"})),
            obs(200, 0, 0, "evt-b", json!({"status": "warm"})),
            obs(150, 0, 0, "evt-c", json!({"status": "lukewarm"})),
        ];
        let d = def("status", MergePolicy::LastWrite);
        let p = provenance_for_field(&obs, &d, "status").expect("should have provenance");
        assert_eq!(p.field, "status");
        assert_eq!(p.value, json!("warm"));
        assert_eq!(p.source_event_id, "evt-b");
        assert_eq!(p.merge_policy_applied, MergePolicy::LastWrite);
    }

    #[test]
    fn provenance_for_highest_priority_credits_the_user_correction() {
        // Same data as the user-correction-beats-AI test: provenance must
        // surface the user correction as the source, not the AI extraction.
        let obs = vec![
            obs(100, 0, 0, "ai-evt", json!({"name": "alice chen"})),
            obs(200, 1000, 0, "user-evt", json!({"name": "Alice Chen"})),
        ];
        let d = def("name", MergePolicy::HighestPriority);
        let p = provenance_for_field(&obs, &d, "name").unwrap();
        assert_eq!(p.source_event_id, "user-evt");
        assert_eq!(p.value, json!("Alice Chen"));
    }

    #[test]
    fn merge_array_provenance_lists_every_contributing_event_in_order() {
        let obs = vec![
            obs(200, 0, 0, "e2", json!({"tags": "agents"})),
            obs(100, 0, 0, "e1", json!({"tags": "rust"})),
            obs(150, 0, 0, "e3", json!({"other": "x"})),
        ];
        let d = def("tags", MergePolicy::MergeArray);
        let p = provenance_for_field(&obs, &d, "tags").expect("union has provenance");
        assert_eq!(p.value, json!(["agents", "rust"]));
        let ids: Vec<&str> = p
            .contributors
            .iter()
            .map(|c| c.source_event_id.as_str())
            .collect();
        assert_eq!(ids, vec!["e1", "e2"]);
        assert_eq!(p.source_event_id, "e2");

        let reversed: Vec<Observation> = obs.into_iter().rev().collect();
        assert_eq!(
            provenance_for_field(&reversed, &d, "tags").unwrap(),
            p,
            "contributors must not depend on input order"
        );
    }

    #[test]
    fn provenance_for_unknown_field_returns_none() {
        let obs = vec![obs(100, 0, 0, "e1", json!({"status": "x"}))];
        let d = def("name", MergePolicy::LastWrite);
        assert!(provenance_for_field(&obs, &d, "name").is_none());
    }

    #[test]
    fn provenance_for_field_not_in_def_returns_none() {
        let obs = vec![obs(100, 0, 0, "e1", json!({"status": "x"}))];
        let d = def("name", MergePolicy::LastWrite); // def doesn't include "status"
        assert!(provenance_for_field(&obs, &d, "status").is_none());
    }

    #[test]
    fn fold_with_provenance_covers_every_observed_field() {
        let mut policies = BTreeMap::new();
        policies.insert("status".to_string(), MergePolicy::LastWrite);
        policies.insert("name".to_string(), MergePolicy::HighestPriority);
        policies.insert("tags".to_string(), MergePolicy::MergeArray);
        let def = ProjectionDef {
            entity_type: "contact".to_string(),
            field_policies: policies,
        };
        let obs = vec![
            obs(100, 0, 0, "e1", json!({"status": "cold", "tags": "a"})),
            obs(200, 1000, 0, "e2", json!({"name": "Alice", "tags": "b"})),
            obs(150, 0, 0, "e3", json!({"status": "warm"})),
        ];
        let snap = fold_with_provenance(&obs, &def);
        assert_eq!(snap.fields.len(), 3);
        assert_eq!(snap.fields["status"].source_event_id, "e3");
        assert_eq!(snap.fields["status"].value, json!("warm"));
        assert_eq!(snap.fields["name"].source_event_id, "e2");
        assert_eq!(snap.fields["tags"].contributors.len(), 2);
    }

    /// Determinism contract extends to provenance: same observations in any
    /// order produce the same provenance (and therefore the same
    /// source_event_id). If this ever fails, the tiebreakers in
    /// `pick_observation` callsites have leaked input order.
    #[test]
    fn provenance_is_order_independent() {
        let a = obs(100, 1, 1, "e-a", json!({"f": "A"}));
        let b = obs(200, 2, 2, "e-b", json!({"f": "B"}));
        let c = obs(150, 3, 3, "e-c", json!({"f": "C"}));
        for &policy in &[
            MergePolicy::LastWrite,
            MergePolicy::HighestPriority,
            MergePolicy::MostSpecific,
        ] {
            let d = def("f", policy);
            let p1 = provenance_for_field(&[a.clone(), b.clone(), c.clone()], &d, "f").unwrap();
            let p2 = provenance_for_field(&[c.clone(), b.clone(), a.clone()], &d, "f").unwrap();
            assert_eq!(
                p1, p2,
                "policy {policy:?} provenance is order-dependent — bug"
            );
        }
    }

    // ─── Proptest: determinism under any input permutation ────────────────

    use proptest::prelude::*;

    fn arb_observation() -> impl Strategy<Value = Observation> {
        // Bounded ranges so the test is fast; doesn't change what's being
        // tested (order-independence is a structural property).
        (
            0i64..1_000_i64,                     // observed_at (epoch secs)
            -100i32..1000i32,                    // source_priority
            0i32..10i32,                         // specificity_score
            "[a-z]{1,8}".prop_map(String::from), // source_event_id
            prop_oneof![
                Just(json!("A")),
                Just(json!("B")),
                Just(json!(42)),
                Just(json!(["x", "y"])),
                Just(json!(["y", "z"])),
            ],
        )
            .prop_map(|(ts, prio, spec, id, val)| {
                let mut fields = BTreeMap::new();
                fields.insert("f".to_string(), val);
                Observation {
                    observed_at: Utc.timestamp_opt(ts, 0).single().unwrap(),
                    recorded_at: None,
                    source_priority: prio,
                    specificity_score: spec,
                    source_event_id: id,
                    fields,
                    assertion: Assertion::default(),
                }
            })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        /// For any observation set and any of the four policies, folding
        /// the set in any order must produce the same snapshot.
        #[test]
        fn fold_determinism_under_shuffling(
            mut obs in prop::collection::vec(arb_observation(), 1..6),
            policy_idx in 0u32..4u32,
        ) {
            let policy = match policy_idx {
                0 => MergePolicy::LastWrite,
                1 => MergePolicy::HighestPriority,
                2 => MergePolicy::MostSpecific,
                _ => MergePolicy::MergeArray,
            };
            let def = def("f", policy);
            let baseline = fold(&obs, &def);

            // Shuffle in a deterministic but non-trivial way: reverse,
            // rotate by 1, swap first/last. Together these cover every
            // small permutation that could expose order leakage.
            obs.reverse();
            prop_assert_eq!(&baseline, &fold(&obs, &def));

            if obs.len() > 1 {
                obs.rotate_left(1);
                prop_assert_eq!(&baseline, &fold(&obs, &def));

                let last = obs.len() - 1;
                obs.swap(0, last);
                prop_assert_eq!(&baseline, &fold(&obs, &def));
            }
        }
    }
}
