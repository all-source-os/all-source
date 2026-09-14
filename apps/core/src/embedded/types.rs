use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Input for [`EmbeddedCore::ingest`](super::EmbeddedCore::ingest).
///
/// All fields are plain strings — no value object construction required.
/// In single-tenant mode, `tenant_id` is ignored (always "default").
pub struct IngestEvent<'a> {
    /// The entity this event belongs to (e.g., "order-123").
    pub entity_id: &'a str,
    /// Event type in dot-notation (e.g., "order.placed"). Must be lowercase.
    pub event_type: &'a str,
    /// Arbitrary JSON payload.
    pub payload: serde_json::Value,
    /// Optional metadata (e.g., source system, correlation IDs).
    pub metadata: Option<serde_json::Value>,
    /// Optional tenant ID. In single-tenant mode this is ignored.
    /// In multi-tenant mode, defaults to "default" when `None`.
    pub tenant_id: Option<&'a str>,
}

/// Builder for query parameters passed to [`EmbeddedCore::query`](super::EmbeddedCore::query).
#[derive(Default)]
pub struct Query {
    pub(crate) entity_id: Option<String>,
    pub(crate) event_type: Option<String>,
    pub(crate) event_type_prefix: Option<String>,
    pub(crate) exclude_event_type_prefix: Option<String>,
    pub(crate) tenant_id: Option<String>,
    pub(crate) limit: Option<usize>,
    pub(crate) since: Option<DateTime<Utc>>,
    pub(crate) until: Option<DateTime<Utc>>,
    pub(crate) offset: usize,
    pub(crate) descending: bool,
}

impl Query {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn entity_id(mut self, id: impl Into<String>) -> Self {
        self.entity_id = Some(id.into());
        self
    }

    pub fn event_type(mut self, t: impl Into<String>) -> Self {
        self.event_type = Some(t.into());
        self
    }

    pub fn event_type_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.event_type_prefix = Some(prefix.into());
        self
    }

    /// Exclude events whose type starts with any of these comma-separated
    /// prefixes (e.g. `"audit.,service."`). Applied before the limit.
    pub fn exclude_event_type_prefix(mut self, prefixes: impl Into<String>) -> Self {
        self.exclude_event_type_prefix = Some(prefixes.into());
        self
    }

    pub fn tenant_id(mut self, id: impl Into<String>) -> Self {
        self.tenant_id = Some(id.into());
        self
    }

    pub fn limit(mut self, n: usize) -> Self {
        self.limit = Some(n);
        self
    }

    pub fn since(mut self, t: DateTime<Utc>) -> Self {
        self.since = Some(t);
        self
    }

    pub fn until(mut self, t: DateTime<Utc>) -> Self {
        self.until = Some(t);
        self
    }

    /// Skip the first `n` matching events after stable ordering.
    pub fn offset(mut self, n: usize) -> Self {
        self.offset = n;
        self
    }

    /// Return newest matching events first. Default is chronological order.
    pub fn descending(mut self, enabled: bool) -> Self {
        self.descending = enabled;
        self
    }
}

/// A single event returned from [`EmbeddedCore::query`](super::EmbeddedCore::query).
///
/// All fields are plain Rust types — no value objects exposed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventView {
    pub id: Uuid,
    pub event_type: String,
    pub entity_id: String,
    pub tenant_id: String,
    pub payload: serde_json::Value,
    pub metadata: Option<serde_json::Value>,
    pub timestamp: DateTime<Utc>,
    pub version: i64,
}

/// Stable, bounded query page with truthful continuation metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryPage {
    pub events: Vec<EventView>,
    pub total_count: usize,
    pub has_more: bool,
    pub next_offset: Option<usize>,
}

/// Durability status comparing in-memory, WAL, and Parquet layers.
///
/// Returned by [`EmbeddedCore::durability_status`](super::EmbeddedCore::durability_status).
/// When `durable` is `false`, events exist only in volatile memory and will
/// be lost on process exit.
#[derive(Debug, Clone, Serialize)]
pub struct DurabilityStatus {
    /// Events currently in the in-memory store.
    pub memory_events: usize,
    /// Whether WAL persistence is configured.
    pub wal_enabled: bool,
    /// Number of entries written to WAL since last truncate.
    pub wal_entries: u64,
    /// Total bytes written to WAL since last truncate.
    pub wal_bytes: u64,
    /// Current WAL sequence number.
    pub wal_sequence: u64,
    /// Whether Parquet persistence is configured.
    pub parquet_enabled: bool,
    /// Number of Parquet files on disk.
    pub parquet_files: usize,
    /// Total bytes in Parquet files.
    pub parquet_bytes: u64,
    /// Events buffered in Parquet batch (not yet flushed to file).
    pub parquet_pending_batch: usize,
    /// Whether all in-memory events are backed by at least one durable layer.
    pub durable: bool,
    /// Human-readable warnings about data loss risks.
    pub warnings: Vec<String>,
}

impl From<&crate::domain::entities::Event> for EventView {
    fn from(e: &crate::domain::entities::Event) -> Self {
        Self {
            id: e.id(),
            event_type: e.event_type_str().to_string(),
            entity_id: e.entity_id_str().to_string(),
            tenant_id: e.tenant_id_str().to_string(),
            payload: e.payload().clone(),
            metadata: e.metadata().cloned(),
            timestamp: e.timestamp(),
            version: e.version(),
        }
    }
}
