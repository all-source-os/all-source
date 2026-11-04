use crate::{
    application::{
        dto::{
            EventDto, IngestEventRequest, IngestEventResponse, QueryEventsRequest,
            QueryEventsResponse,
        },
        use_cases::{IngestEventUseCase, IngestEventsBatchUseCase, QueryEventsUseCase},
    },
    domain::repositories::EventRepository,
    error::Result,
};
use std::sync::Arc;

/// Coordinator service for event operations.
///
/// This service orchestrates event ingestion, querying, and analytics
/// workflows, providing a unified interface for complex event operations.
pub struct EventCoordinator {
    ingest_event: IngestEventUseCase,
    ingest_batch: IngestEventsBatchUseCase,
    query_events: QueryEventsUseCase,
    event_repo: Arc<dyn EventRepository>,
}

impl EventCoordinator {
    pub fn new(event_repo: Arc<dyn EventRepository>) -> Self {
        Self {
            ingest_event: IngestEventUseCase::new(event_repo.clone()),
            ingest_batch: IngestEventsBatchUseCase::new(event_repo.clone()),
            query_events: QueryEventsUseCase::new(event_repo.clone()),
            event_repo,
        }
    }

    /// Ingest a single event
    pub async fn ingest(&self, request: IngestEventRequest) -> Result<IngestEventResponse> {
        self.ingest_event.execute(request).await
    }

    /// Ingest multiple events in a batch
    pub async fn ingest_batch(
        &self,
        requests: Vec<IngestEventRequest>,
    ) -> Result<BatchIngestResult> {
        let total = requests.len();
        let response = self.ingest_batch.execute(requests).await?;

        Ok(BatchIngestResult {
            total,
            ingested: response.len(),
            events: response,
        })
    }

    /// Query events with filters
    pub async fn query(&self, request: QueryEventsRequest) -> Result<QueryEventsResponse> {
        self.query_events.execute(request).await
    }

    /// Get entity history - all events for a specific entity
    pub async fn get_entity_history(
        &self,
        entity_id: &str,
        tenant_id: Option<&str>,
    ) -> Result<EntityHistory> {
        let tenant = tenant_id.unwrap_or("default");
        let events = self.event_repo.find_by_entity(entity_id, tenant).await?;

        let event_dtos: Vec<EventDto> = events.iter().map(EventDto::from).collect();
        let event_types: Vec<String> = event_dtos
            .iter()
            .map(|e| e.event_type.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        let first_event = event_dtos.first().map(|e| e.timestamp);
        let last_event = event_dtos.last().map(|e| e.timestamp);

        Ok(EntityHistory {
            entity_id: entity_id.to_string(),
            total_events: event_dtos.len(),
            event_types,
            first_event_at: first_event,
            last_event_at: last_event,
            events: event_dtos,
        })
    }

    /// Get events by type with pagination
    pub async fn get_events_by_type(
        &self,
        event_type: &str,
        tenant_id: Option<&str>,
        limit: usize,
    ) -> Result<QueryEventsResponse> {
        self.query(QueryEventsRequest {
            as_of_valid: None,
            since_valid: None,
            until_valid: None,
            entity_id: None,
            event_type: Some(event_type.to_string()),
            tenant_id: tenant_id.map(std::string::ToString::to_string),
            as_of: None,
            since: None,
            until: None,
            limit: Some(limit),
            event_type_prefix: None,
            exclude_event_type_prefix: None,
            payload_filter: None,
        })
        .await
    }

    /// Get point-in-time snapshot of an entity (time-travel query)
    pub async fn get_entity_snapshot(
        &self,
        entity_id: &str,
        as_of: chrono::DateTime<chrono::Utc>,
        tenant_id: Option<&str>,
    ) -> Result<EntitySnapshot> {
        let tenant = tenant_id.unwrap_or("default");
        let events = self
            .event_repo
            .find_by_entity_as_of(entity_id, tenant, as_of)
            .await?;

        let event_dtos: Vec<EventDto> = events.iter().map(EventDto::from).collect();
        let latest_version = event_dtos.last().map_or(0, |e| e.version);

        Ok(EntitySnapshot {
            entity_id: entity_id.to_string(),
            as_of,
            events_count: event_dtos.len(),
            current_version: latest_version,
            events: event_dtos,
        })
    }

    /// Get event count for a tenant
    pub async fn get_event_count(&self, tenant_id: &str) -> Result<usize> {
        self.event_repo.count(tenant_id).await
    }
}

/// Result of batch event ingestion
#[derive(Debug)]
pub struct BatchIngestResult {
    pub total: usize,
    pub ingested: usize,
    pub events: Vec<IngestEventResponse>,
}

/// Entity history with all events
#[derive(Debug)]
pub struct EntityHistory {
    pub entity_id: String,
    pub total_events: usize,
    pub event_types: Vec<String>,
    pub first_event_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_event_at: Option<chrono::DateTime<chrono::Utc>>,
    pub events: Vec<EventDto>,
}

/// Point-in-time entity snapshot
#[derive(Debug)]
pub struct EntitySnapshot {
    pub entity_id: String,
    pub as_of: chrono::DateTime<chrono::Utc>,
    pub events_count: usize,
    pub current_version: i64,
    pub events: Vec<EventDto>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_ingest_result() {
        let result = BatchIngestResult {
            total: 10,
            ingested: 10,
            events: Vec::new(),
        };
        assert_eq!(result.total, result.ingested);
    }

    #[test]
    fn test_entity_history_creation() {
        let history = EntityHistory {
            entity_id: "test-entity".to_string(),
            total_events: 5,
            event_types: vec!["created".to_string(), "updated".to_string()],
            first_event_at: None,
            last_event_at: None,
            events: Vec::new(),
        };
        assert_eq!(history.entity_id, "test-entity");
        assert_eq!(history.event_types.len(), 2);
    }

    #[test]
    fn test_entity_snapshot_creation() {
        let snapshot = EntitySnapshot {
            entity_id: "test-entity".to_string(),
            as_of: chrono::Utc::now(),
            events_count: 3,
            current_version: 3,
            events: Vec::new(),
        };
        assert_eq!(snapshot.entity_id, "test-entity");
        assert_eq!(snapshot.current_version, 3);
    }
}
