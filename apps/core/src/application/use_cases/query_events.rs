use crate::{
    application::dto::{EventDto, QueryEventsRequest, QueryEventsResponse},
    domain::repositories::EventRepository,
    error::Result,
};
use std::sync::Arc;

/// Use Case: Query Events
///
/// This use case handles querying events from the event store with various filters.
///
/// Responsibilities:
/// - Validate query parameters
/// - Determine query strategy (by entity, by type, by time range, etc.)
/// - Execute query via repository
/// - Transform domain events to DTOs
/// - Apply limits and pagination
pub struct QueryEventsUseCase {
    repository: Arc<dyn EventRepository>,
}

impl QueryEventsUseCase {
    pub fn new(repository: Arc<dyn EventRepository>) -> Self {
        Self { repository }
    }

    pub async fn execute(&self, request: QueryEventsRequest) -> Result<QueryEventsResponse> {
        // Determine tenant_id (default to "default" if not provided)
        let tenant_id = request.tenant_id.unwrap_or_else(|| "default".to_string());

        // Determine query strategy based on filters
        let mut events = if let Some(entity_id) = request.entity_id {
            // Query by entity (most specific)
            if let Some(as_of) = request.as_of {
                // Time-travel query
                self.repository
                    .find_by_entity_as_of(&entity_id, &tenant_id, as_of)
                    .await?
            } else {
                self.repository
                    .find_by_entity(&entity_id, &tenant_id)
                    .await?
            }
        } else if let Some(event_type) = request.event_type {
            // Query by type
            self.repository
                .find_by_type(&event_type, &tenant_id)
                .await?
        } else if let (Some(since), Some(until)) = (request.since, request.until) {
            // Query by time range
            self.repository
                .find_by_time_range(&tenant_id, since, until)
                .await?
        } else {
            // No specific filter - this could be expensive!
            // In production, you might want to require at least one filter
            return Err(crate::error::Error::InvalidInput(
                "Query requires at least one filter (entity_id, event_type, or time range)"
                    .to_string(),
            ));
        };

        // Apply time filters if provided (for non-time-range queries)
        if let Some(since) = request.since {
            events.retain(|e| e.occurred_after(since));
        }
        if let Some(until) = request.until {
            events.retain(|e| e.occurred_before(until));
        }

        // Capture total before applying limit
        let total_count = events.len();

        // Apply limit
        if let Some(limit) = request.limit {
            events.truncate(limit);
        }

        // Convert to DTOs
        let event_dtos: Vec<EventDto> = events.iter().map(EventDto::from).collect();
        let count = event_dtos.len();
        let has_more = count < total_count;

        Ok(QueryEventsResponse {
            events: event_dtos,
            count,
            total_count,
            has_more,
            entity_version: None,
            archive_integrity: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::entities::Event;
    use async_trait::async_trait;
    use chrono::Utc;
    use serde_json::json;
    use uuid::Uuid;

    // Mock repository for testing
    struct MockEventRepository {
        events: Vec<Event>,
    }

    impl MockEventRepository {
        fn with_events(events: Vec<Event>) -> Self {
            Self { events }
        }
    }

    #[async_trait]
    impl EventRepository for MockEventRepository {
        async fn save(&self, _event: &Event) -> Result<()> {
            Ok(())
        }

        async fn save_batch(&self, _events: &[Event]) -> Result<()> {
            Ok(())
        }

        async fn find_by_id(&self, id: Uuid) -> Result<Option<Event>> {
            Ok(self.events.iter().find(|e| e.id() == id).cloned())
        }

        async fn find_by_entity(&self, entity_id: &str, tenant_id: &str) -> Result<Vec<Event>> {
            Ok(self
                .events
                .iter()
                .filter(|e| e.entity_id_str() == entity_id && e.tenant_id_str() == tenant_id)
                .cloned()
                .collect())
        }

        async fn find_by_type(&self, event_type: &str, tenant_id: &str) -> Result<Vec<Event>> {
            Ok(self
                .events
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
            Ok(self
                .events
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
            Ok(self
                .events
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
            Ok(self
                .events
                .iter()
                .filter(|e| e.tenant_id_str() == tenant_id)
                .count())
        }

        async fn health_check(&self) -> Result<()> {
            Ok(())
        }
    }

    fn create_test_events() -> Vec<Event> {
        vec![
            Event::from_strings(
                "user.created".to_string(),
                "user-1".to_string(),
                "tenant-1".to_string(),
                json!({"name": "Alice"}),
                None,
            )
            .unwrap(),
            Event::from_strings(
                "user.created".to_string(),
                "user-2".to_string(),
                "tenant-1".to_string(),
                json!({"name": "Bob"}),
                None,
            )
            .unwrap(),
            Event::from_strings(
                "order.placed".to_string(),
                "order-1".to_string(),
                "tenant-1".to_string(),
                json!({"amount": 100}),
                None,
            )
            .unwrap(),
        ]
    }

    #[tokio::test]
    async fn test_query_by_entity() {
        let events = create_test_events();
        let entity_id = events[0].entity_id().to_string();
        let repo = Arc::new(MockEventRepository::with_events(events));
        let use_case = QueryEventsUseCase::new(repo);

        let request = QueryEventsRequest {
            entity_id: Some(entity_id),
            event_type: None,
            tenant_id: Some("tenant-1".to_string()),
            as_of: None,
            since: None,
            until: None,
            limit: None,
            event_type_prefix: None,
            exclude_event_type_prefix: None,
            payload_filter: None,
        };

        let response = use_case.execute(request).await;
        assert!(response.is_ok());

        let response = response.unwrap();
        assert_eq!(response.count, 1);
    }

    #[tokio::test]
    async fn test_query_by_type() {
        let events = create_test_events();
        let repo = Arc::new(MockEventRepository::with_events(events));
        let use_case = QueryEventsUseCase::new(repo);

        let request = QueryEventsRequest {
            entity_id: None,
            event_type: Some("user.created".to_string()),
            tenant_id: Some("tenant-1".to_string()),
            as_of: None,
            since: None,
            until: None,
            limit: None,
            event_type_prefix: None,
            exclude_event_type_prefix: None,
            payload_filter: None,
        };

        let response = use_case.execute(request).await;
        assert!(response.is_ok());

        let response = response.unwrap();
        assert_eq!(response.count, 2);
    }

    #[tokio::test]
    async fn test_query_with_limit() {
        let events = create_test_events();
        let repo = Arc::new(MockEventRepository::with_events(events));
        let use_case = QueryEventsUseCase::new(repo);

        let request = QueryEventsRequest {
            entity_id: None,
            event_type: Some("user.created".to_string()),
            tenant_id: Some("tenant-1".to_string()),
            as_of: None,
            since: None,
            until: None,
            limit: Some(1),
            event_type_prefix: None,
            exclude_event_type_prefix: None,
            payload_filter: None,
        };

        let response = use_case.execute(request).await;
        assert!(response.is_ok());

        let response = response.unwrap();
        assert_eq!(response.count, 1);
    }

    #[tokio::test]
    async fn test_query_requires_filter() {
        let events = create_test_events();
        let repo = Arc::new(MockEventRepository::with_events(events));
        let use_case = QueryEventsUseCase::new(repo);

        let request = QueryEventsRequest {
            entity_id: None,
            event_type: None,
            tenant_id: Some("tenant-1".to_string()),
            as_of: None,
            since: None,
            until: None,
            limit: None,
            event_type_prefix: None,
            exclude_event_type_prefix: None,
            payload_filter: None,
        };

        let response = use_case.execute(request).await;
        assert!(response.is_err());
    }
}
