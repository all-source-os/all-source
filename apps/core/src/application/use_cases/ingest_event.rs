use crate::{
    application::dto::{IngestEventRequest, IngestEventResponse},
    domain::{entities::Event, repositories::EventRepository},
    error::Result,
};
use std::sync::Arc;

/// Use Case: Ingest Event
///
/// This use case handles the ingestion of a single event into the event store.
/// It coordinates between the domain layer (Event entity) and the repository.
///
/// Responsibilities:
/// - Validate input (DTO validation)
/// - Create domain Event entity (with domain validation)
/// - Persist via repository
/// - Return response DTO
pub struct IngestEventUseCase {
    repository: Arc<dyn EventRepository>,
}

impl IngestEventUseCase {
    pub fn new(repository: Arc<dyn EventRepository>) -> Self {
        Self { repository }
    }

    pub async fn execute(&self, request: IngestEventRequest) -> Result<IngestEventResponse> {
        // Create domain event using from_strings (validates and converts to value objects)
        let tenant_id = request.tenant_id.unwrap_or_else(|| "default".to_string());

        let event = Event::from_strings(
            request.event_type,
            request.entity_id,
            tenant_id,
            request.payload,
            request.metadata,
        )?
        .with_valid_time(request.valid_from, request.valid_to)?;

        // Persist via repository
        self.repository.save(&event).await?;

        // Return response
        Ok(IngestEventResponse::from_event(&event))
    }
}

/// Use Case: Batch Ingest Events
///
/// Optimized use case for ingesting multiple events at once.
pub struct IngestEventsBatchUseCase {
    repository: Arc<dyn EventRepository>,
}

impl IngestEventsBatchUseCase {
    pub fn new(repository: Arc<dyn EventRepository>) -> Self {
        Self { repository }
    }

    pub async fn execute(
        &self,
        requests: Vec<IngestEventRequest>,
    ) -> Result<Vec<IngestEventResponse>> {
        // Create all domain events (validates and converts to value objects)
        let mut events = Vec::with_capacity(requests.len());
        let mut responses = Vec::with_capacity(requests.len());

        for request in requests {
            let tenant_id = request.tenant_id.unwrap_or_else(|| "default".to_string());

            let event = Event::from_strings(
                request.event_type,
                request.entity_id,
                tenant_id,
                request.payload,
                request.metadata,
            )?
            .with_valid_time(request.valid_from, request.valid_to)?;

            responses.push(IngestEventResponse::from_event(&event));
            events.push(event);
        }

        // Batch persist
        self.repository.save_batch(&events).await?;

        Ok(responses)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AllSourceError;
    use async_trait::async_trait;
    use chrono::Utc;
    use serde_json::json;
    use uuid::Uuid;

    // Mock repository for testing
    struct MockEventRepository {
        events: std::sync::Mutex<Vec<Event>>,
    }

    impl MockEventRepository {
        fn new() -> Self {
            Self {
                events: std::sync::Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl EventRepository for MockEventRepository {
        async fn save(&self, event: &Event) -> Result<()> {
            self.events.lock().unwrap().push(event.clone());
            Ok(())
        }

        async fn save_batch(&self, events: &[Event]) -> Result<()> {
            for event in events {
                self.save(event).await?;
            }
            Ok(())
        }

        async fn find_by_id(&self, id: Uuid) -> Result<Option<Event>> {
            let events = self.events.lock().unwrap();
            Ok(events.iter().find(|e| e.id() == id).cloned())
        }

        async fn find_by_entity(&self, entity_id: &str, tenant_id: &str) -> Result<Vec<Event>> {
            let events = self.events.lock().unwrap();
            Ok(events
                .iter()
                .filter(|e| e.entity_id_str() == entity_id && e.tenant_id_str() == tenant_id)
                .cloned()
                .collect())
        }

        async fn find_by_type(&self, event_type: &str, tenant_id: &str) -> Result<Vec<Event>> {
            let events = self.events.lock().unwrap();
            Ok(events
                .iter()
                .filter(|e| e.event_type_str() == event_type && e.tenant_id_str() == tenant_id)
                .cloned()
                .collect())
        }

        async fn find_by_time_range(
            &self,
            tenant_id: &str,
            start: chrono::DateTime<Utc>,
            end: chrono::DateTime<Utc>,
        ) -> Result<Vec<Event>> {
            let events = self.events.lock().unwrap();
            Ok(events
                .iter()
                .filter(|e| e.tenant_id_str() == tenant_id && e.occurred_between(start, end))
                .cloned()
                .collect())
        }

        async fn find_by_entity_as_of(
            &self,
            entity_id: &str,
            tenant_id: &str,
            as_of: chrono::DateTime<Utc>,
        ) -> Result<Vec<Event>> {
            let events = self.events.lock().unwrap();
            Ok(events
                .iter()
                .filter(|e| {
                    e.entity_id_str() == entity_id
                        && e.tenant_id_str() == tenant_id
                        && e.occurred_before(as_of)
                })
                .cloned()
                .collect())
        }

        async fn count(&self, tenant_id: &str) -> Result<usize> {
            let events = self.events.lock().unwrap();
            Ok(events
                .iter()
                .filter(|e| e.tenant_id_str() == tenant_id)
                .count())
        }

        async fn health_check(&self) -> Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_ingest_event_use_case() {
        let repo = Arc::new(MockEventRepository::new());
        let use_case = IngestEventUseCase::new(repo.clone());

        let request = IngestEventRequest {
            event_type: "user.created".to_string(),
            entity_id: "user-123".to_string(),
            tenant_id: Some("tenant-1".to_string()),
            payload: json!({"name": "Alice"}),
            metadata: None,
            expected_version: None,
            valid_from: None,
            valid_to: None,
        };

        let response = use_case.execute(request).await;
        assert!(response.is_ok());

        let response = response.unwrap();
        assert_eq!(repo.events.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_ingest_event_with_default_tenant() {
        let repo = Arc::new(MockEventRepository::new());
        let use_case = IngestEventUseCase::new(repo.clone());

        let request = IngestEventRequest {
            event_type: "order.placed".to_string(),
            entity_id: "order-456".to_string(),
            tenant_id: None, // Should default to "default"
            payload: json!({"amount": 100}),
            metadata: None,
            expected_version: None,
            valid_from: None,
            valid_to: None,
        };

        let response = use_case.execute(request).await;
        assert!(response.is_ok());

        let events = repo.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].tenant_id_str(), "default");
    }

    #[tokio::test]
    async fn test_ingest_without_valid_time_stores_none_and_echoes_record_time() {
        let repo = Arc::new(MockEventRepository::new());
        let use_case = IngestEventUseCase::new(repo.clone());

        let response = use_case
            .execute(IngestEventRequest {
                event_type: "user.created".to_string(),
                entity_id: "user-1".to_string(),
                tenant_id: None,
                payload: json!({}),
                metadata: None,
                expected_version: None,
                valid_from: None,
                valid_to: None,
            })
            .await
            .unwrap();

        let events = repo.events.lock().unwrap();
        assert_eq!(events[0].valid_from, None);
        assert_eq!(events[0].valid_to, None);
        assert_eq!(response.valid_from, events[0].timestamp());
        assert_eq!(response.recorded_at, events[0].timestamp());
    }

    #[tokio::test]
    async fn test_ingest_with_valid_from_stores_it_and_echoes_it() {
        let repo = Arc::new(MockEventRepository::new());
        let use_case = IngestEventUseCase::new(repo.clone());
        let filed_from = chrono::DateTime::parse_from_rfc3339("2026-01-15T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let response = use_case
            .execute(IngestEventRequest {
                event_type: "user.created".to_string(),
                entity_id: "user-2".to_string(),
                tenant_id: None,
                payload: json!({}),
                metadata: None,
                expected_version: None,
                valid_from: Some(filed_from),
                valid_to: None,
            })
            .await
            .unwrap();

        let events = repo.events.lock().unwrap();
        assert_eq!(events[0].valid_from, Some(filed_from));
        assert_eq!(events[0].valid_to, None);
        assert_eq!(response.valid_from, filed_from);
        assert_ne!(response.recorded_at, filed_from);
    }

    #[tokio::test]
    async fn test_ingest_rejects_valid_to_not_after_valid_from() {
        let repo = Arc::new(MockEventRepository::new());
        let use_case = IngestEventUseCase::new(repo.clone());
        let start = chrono::DateTime::parse_from_rfc3339("2026-01-15T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let result = use_case
            .execute(IngestEventRequest {
                event_type: "user.created".to_string(),
                entity_id: "user-3".to_string(),
                tenant_id: None,
                payload: json!({}),
                metadata: None,
                expected_version: None,
                valid_from: Some(start),
                valid_to: Some(start),
            })
            .await;

        assert!(matches!(result, Err(AllSourceError::ValidationError(_))));
        assert!(repo.events.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_batch_ingest() {
        let repo = Arc::new(MockEventRepository::new());
        let use_case = IngestEventsBatchUseCase::new(repo.clone());

        let requests = vec![
            IngestEventRequest {
                event_type: "event.1".to_string(),
                entity_id: "e1".to_string(),
                tenant_id: Some("t1".to_string()),
                payload: json!({}),
                metadata: None,
                expected_version: None,
                valid_from: None,
                valid_to: None,
            },
            IngestEventRequest {
                event_type: "event.2".to_string(),
                entity_id: "e2".to_string(),
                tenant_id: Some("t1".to_string()),
                payload: json!({}),
                metadata: None,
                expected_version: None,
                valid_from: None,
                valid_to: None,
            },
        ];

        let responses = use_case.execute(requests).await;
        assert!(responses.is_ok());
        assert_eq!(responses.unwrap().len(), 2);
        assert_eq!(repo.events.lock().unwrap().len(), 2);
    }
}
