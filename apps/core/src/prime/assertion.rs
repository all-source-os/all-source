//! The write contract for Prime facts: who asserted a node or edge write, with
//! what model and run, derived from what, and with what confidence.
//!
//! An [`Assertion`] travels in event **metadata** under [`METADATA_KEY`], never
//! in the payload. Writers that predate it decode as [`AssertionSource::Unknown`]
//! rather than a defaulted agent, so old history never claims an author it did
//! not record.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use super::{
    error::{PrimeError, PrimeResult},
    projections::declarative::Observation,
    types::event_types,
};
use crate::embedded::EventView;

/// Metadata key holding the serialized [`Assertion`].
pub const METADATA_KEY: &str = "assertion";
/// Metadata key read by `MergePolicy::HighestPriority` folding.
pub const PRIORITY_KEY: &str = "source_priority";
/// Metadata key recording the kind version a write was validated against.
pub const KIND_VERSION_KEY: &str = "kind_version";
/// Metadata key holding violations accepted under `warn` enforcement.
pub const WARNINGS_KEY: &str = "validation_warnings";

/// Who asserted a fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssertionSource {
    User,
    Agent,
    Extraction,
    Import,
    /// Decoded from an event written before assertions existed. Never accepted
    /// on a write.
    #[default]
    Unknown,
}

impl AssertionSource {
    /// Default `source_priority`: user 1000, import 500, agent 100,
    /// extraction 0, unknown 0.
    pub fn default_priority(self) -> i32 {
        match self {
            Self::User => 1000,
            Self::Import => 500,
            Self::Agent => 100,
            Self::Extraction | Self::Unknown => 0,
        }
    }

    /// Parse a wire value. `unknown` is refused: it is reserved for legacy reads.
    pub fn parse_writable(s: &str) -> PrimeResult<Self> {
        match s {
            "user" => Ok(Self::User),
            "agent" => Ok(Self::Agent),
            "extraction" => Ok(Self::Extraction),
            "import" => Ok(Self::Import),
            other => Err(PrimeError::ValidationFailed(format!(
                "assertion.source: expected one of user, agent, extraction, import, received {}",
                json!(other)
            ))),
        }
    }
}

/// Provenance carried by one write.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Assertion {
    #[serde(default)]
    pub source: AssertionSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub derived_from: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    /// Overrides [`AssertionSource::default_priority`] when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_priority: Option<i32>,
}

impl Assertion {
    pub fn new(source: AssertionSource) -> Self {
        Self {
            source,
            ..Self::default()
        }
    }

    /// The assertion a legacy, assertion-less write API records.
    pub fn agent() -> Self {
        Self::new(AssertionSource::Agent)
    }

    pub fn user() -> Self {
        Self::new(AssertionSource::User)
    }

    pub fn extraction() -> Self {
        Self::new(AssertionSource::Extraction)
    }

    pub fn import() -> Self {
        Self::new(AssertionSource::Import)
    }

    #[must_use]
    pub fn with_actor(mut self, actor: impl Into<String>) -> Self {
        self.actor = Some(actor.into());
        self
    }

    #[must_use]
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    #[must_use]
    pub fn with_run_id(mut self, run_id: impl Into<String>) -> Self {
        self.run_id = Some(run_id.into());
        self
    }

    #[must_use]
    pub fn with_derived_from(mut self, derived_from: Vec<String>) -> Self {
        self.derived_from = derived_from;
        self
    }

    #[must_use]
    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.confidence = Some(confidence);
        self
    }

    #[must_use]
    pub fn with_source_priority(mut self, priority: i32) -> Self {
        self.source_priority = Some(priority);
        self
    }

    /// Priority used by `highest_priority` folding.
    pub fn priority(&self) -> i32 {
        self.source_priority
            .unwrap_or_else(|| self.source.default_priority())
    }

    /// Refuse a write-side assertion: `unknown` source, or a confidence that is
    /// not finite or outside `0.0..=1.0`. Runs before any event is appended,
    /// whatever the enforcement mode.
    pub fn validate(&self) -> PrimeResult<()> {
        if self.source == AssertionSource::Unknown {
            return Err(PrimeError::ValidationFailed(
                "assertion.source: expected one of user, agent, extraction, import, received \"unknown\""
                    .to_string(),
            ));
        }
        if let Some(c) = self.confidence
            && !(c.is_finite() && (0.0..=1.0).contains(&c))
        {
            let received = if c.is_finite() {
                json!(c).to_string()
            } else {
                format!("\"{c}\"")
            };
            return Err(PrimeError::ValidationFailed(format!(
                "assertion.confidence: expected a number in 0.0..=1.0, received {received}"
            )));
        }
        Ok(())
    }

    /// Decode from event metadata. Absent → `source: unknown`.
    pub fn from_metadata(metadata: Option<&Value>) -> Self {
        metadata
            .and_then(|m| m.get(METADATA_KEY))
            .and_then(|a| serde_json::from_value(a.clone()).ok())
            .unwrap_or_default()
    }
}

/// Optional, flat assertion fields as MCP and HTTP callers send them.
/// Every field absent means "no assertion supplied".
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AssertionInput {
    /// Edge writes carry this as `assertion_source`: their `source` key is
    /// the edge's source node.
    #[serde(default, alias = "assertion_source")]
    pub source: Option<String>,
    #[serde(default)]
    pub actor: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub run_id: Option<String>,
    #[serde(default)]
    pub derived_from: Option<Vec<String>>,
    #[serde(default)]
    pub confidence: Option<f64>,
    #[serde(default)]
    pub source_priority: Option<i32>,
}

impl AssertionInput {
    pub fn is_empty(&self) -> bool {
        self.source.is_none()
            && self.actor.is_none()
            && self.model.is_none()
            && self.run_id.is_none()
            && self.derived_from.is_none()
            && self.confidence.is_none()
            && self.source_priority.is_none()
    }

    /// `None` when no field was sent; otherwise a validated assertion whose
    /// source defaults to `agent`.
    pub fn into_assertion(self) -> PrimeResult<Option<Assertion>> {
        if self.is_empty() {
            return Ok(None);
        }
        let source = match self.source.as_deref() {
            Some(s) => AssertionSource::parse_writable(s)?,
            None => AssertionSource::Agent,
        };
        let assertion = Assertion {
            source,
            actor: self.actor,
            model: self.model,
            run_id: self.run_id,
            derived_from: self.derived_from.unwrap_or_default(),
            confidence: self.confidence,
            source_priority: self.source_priority,
        };
        assertion.validate()?;
        Ok(Some(assertion))
    }
}

/// Metadata stored on a node or edge write event.
///
/// `extra` carries caller keys such as `conversation_id`; the assertion,
/// priority, kind version and warnings are written over it.
pub fn write_metadata(
    assertion: &Assertion,
    kind_version: Option<u32>,
    warnings: &[super::schema::Violation],
    extra: Option<&Value>,
) -> Value {
    let mut map = extra
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_else(Map::new);
    map.insert(
        METADATA_KEY.to_string(),
        serde_json::to_value(assertion).unwrap_or(Value::Null),
    );
    map.insert(PRIORITY_KEY.to_string(), json!(assertion.priority()));
    if let Some(v) = kind_version {
        map.insert(KIND_VERSION_KEY.to_string(), json!(v));
    }
    if !warnings.is_empty() {
        map.insert(
            WARNINGS_KEY.to_string(),
            serde_json::to_value(warnings).unwrap_or(Value::Null),
        );
    }
    Value::Object(map)
}

/// One node event as an [`Observation`]. `None` for events that are not node
/// creates or updates.
pub fn observation_from_event(event: &EventView) -> Option<Observation> {
    let et = event.event_type.as_str();
    if et != event_types::NODE_CREATED && et != event_types::NODE_UPDATED {
        return None;
    }
    let fields = event
        .payload
        .get("properties")
        .and_then(Value::as_object)
        .map(|obj| {
            obj.iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let metadata = event.metadata.as_ref();
    let assertion = Assertion::from_metadata(metadata);
    let source_priority = metadata
        .and_then(|m| m.get(PRIORITY_KEY))
        .and_then(Value::as_i64)
        .map_or_else(|| assertion.priority(), |i| i as i32);
    let specificity_score = metadata
        .and_then(|m| m.get("specificity_score"))
        .and_then(Value::as_i64)
        .map_or(0, |i| i as i32);
    Some(Observation {
        observed_at: super::temporal::valid_from(event),
        recorded_at: Some(event.timestamp),
        source_priority,
        specificity_score,
        source_event_id: event.id.to_string(),
        fields,
        assertion,
    })
}

/// Every node create/update in `events` as an [`Observation`].
pub fn observations_from_events(events: &[EventView]) -> Vec<Observation> {
    events.iter().filter_map(observation_from_event).collect()
}

/// Latest projection definition per entity type from
/// `prime.projection.defined` events; later timestamps win, ties go to the
/// later event in the slice.
pub fn latest_projection_defs(
    events: &[EventView],
) -> Vec<super::projections::declarative::ProjectionDef> {
    use super::projections::declarative::ProjectionDef;
    use std::collections::HashMap;

    let mut latest: HashMap<String, (chrono::DateTime<chrono::Utc>, ProjectionDef)> =
        HashMap::new();
    for event in events
        .iter()
        .filter(|e| e.event_type == event_types::PROJECTION_DEFINED)
    {
        let def: ProjectionDef = match serde_json::from_value(event.payload.clone()) {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!(
                    entity_id = %event.entity_id,
                    error = %e,
                    "skipping malformed prime.projection.defined event during replay"
                );
                continue;
            }
        };
        latest
            .entry(event.entity_id.clone())
            .and_modify(|(ts, existing)| {
                if event.timestamp >= *ts {
                    *ts = event.timestamp;
                    *existing = def.clone();
                }
            })
            .or_insert((event.timestamp, def));
    }
    latest.into_values().map(|(_, def)| def).collect()
}
