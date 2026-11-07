//! Transport for `geo.*` events.
//!
//! The emitter is deliberately thin: it turns a [`GeoEvent`] into a Core
//! ingest envelope and either POSTs it through the Control Plane gateway or
//! prints it. It knows nothing about measurement layers, bots, probe sets or
//! scoring — those are the callers' concern, and keeping them out is what lets
//! the later layer slices be written independently.

use allsource::{CoreClient, IngestEventInput};
use serde::Serialize;
use serde_json::{Value, json};

use crate::{
    config::GeoConfig,
    error::GeoError,
    events::{GeoEvent, GeoEventType},
};

/// Stamped into `metadata.emitter` on every event, so a stream reader can tell
/// which producer wrote a row. A constant, not a version string — the fixtures
/// in `docs/contracts/geo-events/examples/` would otherwise churn on every
/// release.
pub const EMITTER_NAME: &str = "tooling/geo";

/// A Core ingest body: exactly what goes over the wire, and exactly what
/// `--dry-run` prints.
///
/// Core assigns `id`, `timestamp`, and the per-entity monotonic `version`
/// server-side; the gateway injects `tenant_id`. None of those are ours to
/// send.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IngestEnvelope {
    pub event_type: String,
    pub entity_id: String,
    pub payload: Value,
    pub metadata: Value,
}

impl IngestEnvelope {
    /// Build the envelope for an event.
    pub fn build(event: &GeoEvent) -> Result<Self, GeoError> {
        Ok(Self {
            event_type: event.event_type().as_str().to_string(),
            entity_id: event.entity_id(),
            payload: event.payload()?,
            metadata: json!({
                "emitter": EMITTER_NAME,
                "idempotency_key": event.idempotency_key(),
            }),
        })
    }

    /// The pretty JSON rendering used by `--dry-run` and by the committed
    /// contract examples. One rendering, one source of truth.
    pub fn to_pretty_json(&self) -> Result<String, GeoError> {
        serde_json::to_string_pretty(self).map_err(|source| GeoError::Serialize {
            event_type: "ingest envelope",
            source,
        })
    }
}

impl From<IngestEnvelope> for IngestEventInput {
    fn from(env: IngestEnvelope) -> Self {
        // Builders rather than a struct literal: the SDK gained
        // `expected_version` in 0.24.0 and a literal has to name every field,
        // so this would break again on the next addition.
        IngestEventInput::new(env.event_type, env.entity_id, env.payload)
            .with_metadata(env.metadata)
    }
}

/// Whether the emitter writes or just prints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmitMode {
    /// Print the ingest envelope as JSON. No network, no credential needed.
    DryRun,
    /// POST to the Control Plane gateway.
    Live,
}

/// What happened to one event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmitOutcome {
    /// Nothing was written; this is the JSON that would have been.
    DryRun { json: String },
    /// Core accepted the event.
    Ingested { event_id: String, timestamp: String },
}

/// Writes `geo.*` events to AllSource through the Control Plane gateway.
///
/// The SDK's `CoreClient` is used because it is the ingest-capable client —
/// it is pointed at the *gateway* base URL, not at Core. Core never
/// authenticates public traffic; all auth terminates at the Control Plane,
/// which injects `tenant_id` and forwards.
#[derive(Debug, Clone)]
pub struct GeoEmitter {
    mode: EmitMode,
    client: Option<CoreClient>,
    api_url: String,
}

impl GeoEmitter {
    /// A dry-run emitter. Needs no credential and touches no network.
    pub fn dry_run() -> Self {
        Self {
            mode: EmitMode::DryRun,
            client: None,
            api_url: String::new(),
        }
    }

    /// A live emitter for an explicit config.
    pub fn live(config: &GeoConfig) -> Result<Self, GeoError> {
        let client = config.core_client().map_err(|source| GeoError::Client {
            api_url: config.api_url().to_string(),
            source,
        })?;
        Ok(Self {
            mode: EmitMode::Live,
            client: Some(client),
            api_url: config.api_url().to_string(),
        })
    }

    /// Dry-run when asked, otherwise live from the environment.
    ///
    /// A live emitter with no credential fails loudly here rather
    /// than degrading to a no-op — a silently dropped window is worse than a
    /// crashed job.
    pub fn from_env(mode: EmitMode) -> Result<Self, GeoError> {
        match mode {
            EmitMode::DryRun => Ok(Self::dry_run()),
            EmitMode::Live => Self::live(&GeoConfig::from_env()?),
        }
    }

    /// Current mode.
    pub fn mode(&self) -> EmitMode {
        self.mode
    }

    /// Gateway base URL (empty in dry-run).
    pub fn api_url(&self) -> &str {
        &self.api_url
    }

    /// Emit one event.
    pub async fn emit(&self, event: &GeoEvent) -> Result<EmitOutcome, GeoError> {
        let envelope = IngestEnvelope::build(event)?;
        match &self.client {
            None => Ok(EmitOutcome::DryRun {
                json: envelope.to_pretty_json()?,
            }),
            Some(client) => {
                let entity_id = envelope.entity_id.clone();
                let event_type = event.event_type();
                let resp = client
                    .ingest_event(envelope.into())
                    .await
                    .map_err(|source| GeoError::Ingest {
                        event_type: event_type.as_str(),
                        entity_id,
                        source,
                    })?;
                Ok(EmitOutcome::Ingested {
                    event_id: resp.event_id,
                    timestamp: resp.timestamp,
                })
            }
        }
    }

    /// Emit a batch, stopping at the first failure.
    ///
    /// Sequential on purpose: the caller's ordering is usually the log's
    /// ordering, and a partial failure should leave a prefix that a re-run can
    /// safely repeat (every event's `entity_id` carries its idempotency key).
    pub async fn emit_all(&self, events: &[GeoEvent]) -> Result<Vec<EmitOutcome>, GeoError> {
        let mut out = Vec::with_capacity(events.len());
        for event in events {
            out.push(self.emit(event).await?);
        }
        Ok(out)
    }
}

/// Convenience: the envelope JSON for an event, without building an emitter.
pub fn render_envelope(event: &GeoEvent) -> Result<String, GeoError> {
    IngestEnvelope::build(event)?.to_pretty_json()
}

/// The event-type strings this emitter can produce, for callers that need to
/// enumerate the family (e.g. reporting).
pub fn emitted_event_types() -> [GeoEventType; 7] {
    GeoEventType::ALL
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::samples;

    #[test]
    fn dry_run_needs_no_key_and_writes_nothing() {
        let emitter = GeoEmitter::dry_run();
        assert_eq!(emitter.mode(), EmitMode::DryRun);
        assert!(emitter.client.is_none());
    }

    #[test]
    fn envelope_carries_the_idempotency_key_in_metadata() {
        for event in samples::canonical_events() {
            let env = IngestEnvelope::build(&event).expect("envelope builds");
            assert_eq!(
                env.metadata.get("idempotency_key").and_then(Value::as_str),
                Some(event.idempotency_key().as_str()),
                "{} lost its idempotency key",
                event.event_type()
            );
            assert_eq!(
                env.metadata.get("emitter").and_then(Value::as_str),
                Some(EMITTER_NAME)
            );
        }
    }

    #[test]
    fn envelope_never_sets_server_assigned_fields() {
        // Core owns id/timestamp/version; the gateway owns tenant_id.
        for event in samples::canonical_events() {
            let env = IngestEnvelope::build(&event).expect("envelope builds");
            let value = serde_json::to_value(&env).expect("envelope serialises");
            let obj = value.as_object().expect("envelope is an object");
            for forbidden in ["id", "timestamp", "version", "tenant_id"] {
                assert!(
                    !obj.contains_key(forbidden),
                    "{} envelope must not set {forbidden}",
                    event.event_type()
                );
            }
        }
    }

    #[tokio::test]
    async fn dry_run_emit_returns_the_rendered_envelope() {
        let emitter = GeoEmitter::dry_run();
        let event = samples::sample(GeoEventType::CrawlObserved);
        let outcome = emitter.emit(&event).await.expect("dry run emits");
        match outcome {
            EmitOutcome::DryRun { json } => {
                assert_eq!(json, render_envelope(&event).expect("renders"));
                assert!(json.contains("\"geo.crawl.observed\""));
            }
            EmitOutcome::Ingested { .. } => panic!("dry run must not ingest"),
        }
    }
}
