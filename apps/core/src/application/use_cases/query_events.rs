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

        // Bounds are inclusive, matching the store's filters.
        if let Some(since) = request.since {
            events.retain(|e| e.timestamp() >= since);
        }
        if let Some(until) = request.until {
            events.retain(|e| e.timestamp() <= until);
        }
        if let Some(as_of_valid) = request.as_of_valid {
            events.retain(|e| {
                e.effective_valid_from() <= as_of_valid
                    && e.valid_to.is_none_or(|to| to > as_of_valid)
            });
        }
        if let Some(since_valid) = request.since_valid {
            events.retain(|e| e.effective_valid_from() >= since_valid);
        }
        if let Some(until_valid) = request.until_valid {
            events.retain(|e| e.effective_valid_from() <= until_valid);
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
    use crate::domain::{
        entities::Event,
        value_objects::{EntityId, EventType, TenantId},
    };
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
            as_of_valid: None,
            since_valid: None,
            until_valid: None,
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
            as_of_valid: None,
            since_valid: None,
            until_valid: None,
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
            as_of_valid: None,
            since_valid: None,
            until_valid: None,
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
            as_of_valid: None,
            since_valid: None,
            until_valid: None,
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

    fn event_at(offset_secs: i64, base: chrono::DateTime<Utc>) -> Event {
        Event::reconstruct(
            Uuid::new_v4(),
            EventType::new("order.placed".to_string()).unwrap(),
            EntityId::new("order-1".to_string()).unwrap(),
            TenantId::new("tenant-1".to_string()).unwrap(),
            json!({}),
            base + chrono::Duration::seconds(offset_secs),
            None,
            1,
        )
    }

    #[tokio::test]
    async fn since_and_until_are_inclusive_on_type_and_time_range_strategies() {
        let base = Utc::now();
        let repo = Arc::new(MockEventRepository::with_events(vec![
            event_at(-1, base),
            event_at(0, base),
            event_at(5, base),
            event_at(10, base),
            event_at(11, base),
        ]));
        let use_case = QueryEventsUseCase::new(repo);
        let window = |event_type: Option<&str>| QueryEventsRequest {
            as_of_valid: None,
            since_valid: None,
            until_valid: None,
            entity_id: None,
            event_type: event_type.map(str::to_string),
            tenant_id: Some("tenant-1".to_string()),
            as_of: None,
            since: Some(base),
            until: Some(base + chrono::Duration::seconds(10)),
            limit: None,
            event_type_prefix: None,
            exclude_event_type_prefix: None,
            payload_filter: None,
        };

        let by_type = use_case
            .execute(window(Some("order.placed")))
            .await
            .unwrap();
        let by_range = use_case.execute(window(None)).await.unwrap();

        assert_eq!(
            by_type.count, 3,
            "type strategy must keep both boundary events"
        );
        assert_eq!(
            by_range.count, 3,
            "time-range strategy must keep both boundary events"
        );
    }

    #[tokio::test]
    async fn valid_time_bounds_filter_on_the_fact_interval() {
        let base = Utc::now();
        let at = |secs: i64| base + chrono::Duration::seconds(secs);
        let recorded_later = event_at(5, base)
            .with_valid_time(Some(at(1)), None)
            .unwrap();
        let closed = event_at(10, base)
            .with_valid_time(Some(at(3)), Some(at(4)))
            .unwrap();
        let repo = Arc::new(MockEventRepository::with_events(vec![
            event_at(0, base),
            recorded_later,
            closed,
        ]));
        let use_case = QueryEventsUseCase::new(repo);
        let by_valid = |as_of_valid: Option<i64>, since_valid: Option<i64>| QueryEventsRequest {
            as_of_valid: as_of_valid.map(at),
            since_valid: since_valid.map(at),
            until_valid: None,
            entity_id: None,
            event_type: Some("order.placed".to_string()),
            tenant_id: Some("tenant-1".to_string()),
            ..QueryEventsRequest::default()
        };

        let inside_both = use_case.execute(by_valid(Some(2), None)).await.unwrap();
        assert_eq!(
            inside_both.count, 2,
            "a fact filed at 5s but true from 1s is visible at 2s; the one not yet true is not"
        );

        let at_its_end = use_case.execute(by_valid(Some(4), None)).await.unwrap();
        assert_eq!(
            at_its_end.count, 2,
            "valid_to is exclusive: the closed fact is gone at 4s"
        );

        let since = use_case.execute(by_valid(None, Some(3))).await.unwrap();
        assert_eq!(
            since.count, 1,
            "since_valid bounds valid_from only, inclusive"
        );
    }
}
