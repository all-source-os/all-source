use crate::domain::entities::Event;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// DTO for ingesting a new event
#[derive(Debug, Deserialize)]
pub struct IngestEventRequest {
    pub event_type: String,
    pub entity_id: String,
    pub tenant_id: Option<String>, // Optional, defaults to "default"
    pub payload: serde_json::Value,
    pub metadata: Option<serde_json::Value>,
    /// Optional optimistic concurrency control: if set, the write is rejected
    /// with 409 Conflict unless the entity's current version matches this value.
    pub expected_version: Option<u64>,
}

/// DTO for event ingestion response
#[derive(Debug, Serialize)]
pub struct IngestEventResponse {
    pub event_id: Uuid,
    pub timestamp: DateTime<Utc>,
    /// The entity's version after this event was appended
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<u64>,
}

impl IngestEventResponse {
    pub fn from_event(event: &Event) -> Self {
        Self {
            event_id: event.id(),
            timestamp: event.timestamp(),
            version: None,
        }
    }
}

/// DTO for querying events
#[derive(Debug, Default, Deserialize)]
pub struct QueryEventsRequest {
    /// Filter by entity ID
    pub entity_id: Option<String>,

    /// Filter by event type
    pub event_type: Option<String>,

    /// Tenant ID (required for multi-tenancy)
    pub tenant_id: Option<String>,

    /// Time-travel: get events as of this timestamp
    pub as_of: Option<DateTime<Utc>>,

    /// Get events since this timestamp
    pub since: Option<DateTime<Utc>>,

    /// Get events until this timestamp
    pub until: Option<DateTime<Utc>>,

    /// Limit number of results
    pub limit: Option<usize>,

    /// Filter by event type prefix (e.g., "index." matches "index.created", "index.updated")
    pub event_type_prefix: Option<String>,

    /// Exclude events whose type starts with ANY of these prefixes
    /// (comma-separated, e.g. `"audit.,service.,_system."`). Applied before the
    /// limit so excluded events don't consume the result window — lets callers
    /// drop high-frequency/operational namespaces from a recent-activity feed.
    pub exclude_event_type_prefix: Option<String>,

    /// Filter by payload fields (JSON string, e.g., `{"user_id":"abc-123"}`).
    /// Matches events where payload contains ALL specified key-value pairs.
    pub payload_filter: Option<String>,
}

/// DTO for query response
#[derive(Debug, Serialize)]
pub struct QueryEventsResponse {
    pub events: Vec<EventDto>,
    pub count: usize,
    pub total_count: usize,
    pub has_more: bool,
    /// Current version of the entity (present only when query filters by entity_id)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_version: Option<u64>,
    /// Present only after a bounded, strict read of this retained entity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archive_integrity: Option<RetainedEntityIntegrity>,
}

#[derive(Debug, Serialize)]
pub struct RetainedEntityIntegrity {
    pub protocol: &'static str,
    pub tenant_id: String,
    pub entity_id: String,
}

/// DTO for a single event in responses
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EventDto {
    pub id: Uuid,
    pub event_type: String,
    pub entity_id: String,
    pub tenant_id: String,
    pub payload: serde_json::Value,
    pub timestamp: DateTime<Utc>,
    pub metadata: Option<serde_json::Value>,
    pub version: i64,
}

impl From<&Event> for EventDto {
    fn from(event: &Event) -> Self {
        Self {
            id: event.id(),
            event_type: event.event_type().to_string(),
            entity_id: event.entity_id().to_string(),
            tenant_id: event.tenant_id().to_string(),
            payload: event.payload().clone(),
            timestamp: event.timestamp(),
            metadata: event.metadata().cloned(),
            version: event.version(),
        }
    }
}

impl From<Event> for EventDto {
    fn from(event: Event) -> Self {
        EventDto::from(&event)
    }
}

/// Request parameters for listing entities
#[derive(Debug, Default, Deserialize)]
pub struct ListEntitiesRequest {
    /// Filter entities by event type prefix
    pub event_type_prefix: Option<String>,
    /// Filter by payload fields (JSON string)
    pub payload_filter: Option<String>,
    /// Limit number of entities returned
    pub limit: Option<usize>,
    /// Offset for pagination
    pub offset: Option<usize>,
    /// Sort direction by last-event time: `desc` (newest activity first, the
    /// default) or `asc` (oldest activity first). Entities with the same
    /// last-event time are tie-broken by `entity_id` ascending so offset
    /// pagination is stable.
    pub order: Option<String>,
}

/// A single entity summary in the list response
#[derive(Debug, Serialize)]
pub struct EntitySummary {
    pub entity_id: String,
    pub event_count: usize,
    pub last_event_type: String,
    pub last_event_at: DateTime<Utc>,
}

/// Response from listing entities
#[derive(Debug, Serialize)]
pub struct ListEntitiesResponse {
    pub entities: Vec<EntitySummary>,
    pub total: usize,
    pub has_more: bool,
}

/// Request parameters for detecting duplicate entities
#[derive(Debug, Deserialize)]
pub struct DetectDuplicatesRequest {
    /// Required: event type prefix to scope the search (no full-store scans)
    pub event_type_prefix: String,
    /// Comma-separated list of payload field names to group by (e.g., "name,user_id")
    pub group_by: String,
    /// Limit number of duplicate groups returned
    pub limit: Option<usize>,
    /// Offset for pagination
    pub offset: Option<usize>,
}

/// A group of entities that share the same payload field values
#[derive(Debug, Serialize)]
pub struct DuplicateGroup {
    /// The shared field values that define this group
    pub key: serde_json::Value,
    /// Entity IDs in this group
    pub entity_ids: Vec<String>,
    /// Number of entities in this group
    pub count: usize,
}

/// Response from duplicate entity detection
#[derive(Debug, Serialize)]
pub struct DetectDuplicatesResponse {
    /// Groups where count > 1
    pub duplicates: Vec<DuplicateGroup>,
    /// Total number of duplicate groups found
    pub total: usize,
    /// Whether more results are available
    pub has_more: bool,
}

/// DTO for batch ingesting multiple events
#[derive(Debug, Deserialize)]
pub struct IngestEventsBatchRequest {
    pub events: Vec<IngestEventRequest>,
}

/// DTO for batch ingestion response
#[derive(Debug, Serialize)]
pub struct IngestEventsBatchResponse {
    /// Total number of events submitted
    pub total: usize,
    /// Number of events successfully ingested
    pub ingested: usize,
    /// Individual results for each event
    pub events: Vec<IngestEventResponse>,
}
