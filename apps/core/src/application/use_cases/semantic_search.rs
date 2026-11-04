use crate::{
    application::{
        dto::EventDto,
        services::{SemanticSearchRequest, VectorSearchService},
    },
    domain::{repositories::EventRepository, value_objects::EmbeddingVector},
    error::{AllSourceError, Result},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

/// Use Case: Semantic Search
///
/// This use case handles semantic (vector-based) search operations.
///
/// Responsibilities:
/// - Validate search parameters
/// - Execute vector similarity search
/// - Optionally enrich results with full event data
/// - Apply filters and pagination
pub struct SemanticSearchUseCase {
    vector_service: Arc<VectorSearchService>,
    event_repository: Arc<dyn EventRepository>,
}

impl SemanticSearchUseCase {
    pub fn new(
        vector_service: Arc<VectorSearchService>,
        event_repository: Arc<dyn EventRepository>,
    ) -> Self {
        Self {
            vector_service,
            event_repository,
        }
    }

    /// Execute semantic search and return results
    pub async fn execute(
        &self,
        request: SemanticSearchUseCaseRequest,
    ) -> Result<SemanticSearchUseCaseResponse> {
        // Validate query
        let embedding = request.query_embedding.ok_or_else(|| {
            AllSourceError::InvalidInput("query_embedding is required".to_string())
        })?;

        if embedding.is_empty() {
            return Err(AllSourceError::InvalidInput(
                "query_embedding cannot be empty".to_string(),
            ));
        }

        // Validate k
        let k = request.k.unwrap_or(10);
        if k == 0 {
            return Err(AllSourceError::InvalidInput(
                "k must be greater than 0".to_string(),
            ));
        }
        if k > 1000 {
            return Err(AllSourceError::InvalidInput(
                "k cannot exceed 1000".to_string(),
            ));
        }

        // Build search request
        let search_request = SemanticSearchRequest {
            query_embedding: Some(embedding),
            k: Some(k),
            tenant_id: request.tenant_id.clone(),
            event_type: request.event_type.clone(),
            min_similarity: request.min_similarity,
            max_distance: request.max_distance,
            metric: request.metric.clone(),
            include_events: request.include_events.unwrap_or(false),
        };

        // Execute search
        let search_response = self.vector_service.search(search_request).await?;

        // If we need full events, fetch them
        let events = if request.include_events.unwrap_or(false) {
            let mut events = Vec::with_capacity(search_response.results.len());
            for result in &search_response.results {
                if let Some(event) = self.event_repository.find_by_id(result.event_id).await? {
                    events.push(EventDto::from(&event));
                }
            }
            Some(events)
        } else {
            None
        };

        Ok(SemanticSearchUseCaseResponse {
            results: search_response
                .results
                .into_iter()
                .map(|r| SemanticSearchResultDto {
                    event_id: r.event_id,
                    score: r.score,
                    source_text: r.source_text,
                })
                .collect(),
            events,
            count: search_response.count,
            metric: search_response.metric,
            vectors_searched: search_response.stats.vectors_searched,
            search_time_us: search_response.stats.search_time_us,
        })
    }

    /// Find similar events to a given event
    pub async fn find_similar(
        &self,
        event_id: Uuid,
        k: usize,
        tenant_id: Option<String>,
    ) -> Result<SemanticSearchUseCaseResponse> {
        // Get the embedding for the source event
        let entry = self
            .vector_service
            .get_embedding(event_id)
            .await?
            .ok_or_else(|| {
                AllSourceError::EventNotFound(format!("No embedding found for event {event_id}"))
            })?;

        // Search for similar events (excluding the source event)
        let search_request = SemanticSearchRequest {
            query_embedding: Some(entry.embedding.values().to_vec()),
            k: Some(k + 1), // Get one extra to exclude the source
            tenant_id,
            event_type: None,
            min_similarity: None,
            max_distance: None,
            metric: None,
            include_events: false,
        };

        let mut response = self.vector_service.search(search_request).await?;

        // Filter out the source event and limit to k results
        response.results.retain(|r| r.event_id != event_id);
        response.results.truncate(k);
        response.count = response.results.len();

        Ok(SemanticSearchUseCaseResponse {
            results: response
                .results
                .into_iter()
                .map(|r| SemanticSearchResultDto {
                    event_id: r.event_id,
                    score: r.score,
                    source_text: r.source_text,
                })
                .collect(),
            events: None,
            count: response.count,
            metric: response.metric,
            vectors_searched: response.stats.vectors_searched,
            search_time_us: response.stats.search_time_us,
        })
    }
}

/// Request for semantic search use case
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticSearchUseCaseRequest {
    /// The query embedding vector
    pub query_embedding: Option<Vec<f32>>,
    /// Number of results to return (default: 10, max: 1000)
    pub k: Option<usize>,
    /// Filter by tenant
    pub tenant_id: Option<String>,
    /// Filter by event type
    pub event_type: Option<String>,
    /// Minimum similarity threshold
    pub min_similarity: Option<f32>,
    /// Maximum distance threshold
    pub max_distance: Option<f32>,
    /// Distance metric ("cosine", "euclidean", "dot_product")
    pub metric: Option<String>,
    /// Whether to include full event data
    pub include_events: Option<bool>,
}

impl Default for SemanticSearchUseCaseRequest {
    fn default() -> Self {
        Self {
            query_embedding: None,
            k: Some(10),
            tenant_id: None,
            event_type: None,
            min_similarity: None,
            max_distance: None,
            metric: None,
            include_events: None,
        }
    }
}

/// A single result from semantic search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticSearchResultDto {
    pub event_id: Uuid,
    pub score: f32,
    pub source_text: Option<String>,
}

/// Response from semantic search use case
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticSearchUseCaseResponse {
    /// Search results
    pub results: Vec<SemanticSearchResultDto>,
    /// Full event data (if requested)
    pub events: Option<Vec<EventDto>>,
    /// Number of results
    pub count: usize,
    /// Metric used for scoring
    pub metric: String,
    /// Number of vectors searched
    pub vectors_searched: usize,
    /// Search time in microseconds
    pub search_time_us: u64,
}

/// Use Case: Index Event Embedding
///
/// Handles indexing of event embeddings for semantic search.
pub struct IndexEventEmbeddingUseCase {
    vector_service: Arc<VectorSearchService>,
}

impl IndexEventEmbeddingUseCase {
    pub fn new(vector_service: Arc<VectorSearchService>) -> Self {
        Self { vector_service }
    }

    /// Index a single event embedding
    pub async fn execute(&self, request: IndexEventEmbeddingRequest) -> Result<()> {
        // Validate embedding
        let embedding = EmbeddingVector::new(request.embedding)?;

        // Index the embedding
        self.vector_service
            .index_event(crate::application::services::IndexEventRequest {
                event_id: request.event_id,
                tenant_id: request.tenant_id,
                embedding,
                source_text: request.source_text,
            })
            .await
    }

    /// Index multiple embeddings in batch
    pub async fn execute_batch(
        &self,
        requests: Vec<IndexEventEmbeddingRequest>,
    ) -> Result<BatchIndexResponse> {
        let mut indexed = 0;
        let mut failed = 0;
        let mut errors = Vec::new();

        for request in requests {
            match EmbeddingVector::new(request.embedding) {
                Ok(embedding) => {
                    match self
                        .vector_service
                        .index_event(crate::application::services::IndexEventRequest {
                            event_id: request.event_id,
                            tenant_id: request.tenant_id,
                            embedding,
                            source_text: request.source_text,
                        })
                        .await
                    {
                        Ok(()) => indexed += 1,
                        Err(e) => {
                            failed += 1;
                            errors.push(format!("Event {}: {}", request.event_id, e));
                        }
                    }
                }
                Err(e) => {
                    failed += 1;
                    errors.push(format!("Event {}: {}", request.event_id, e));
                }
            }
        }

        Ok(BatchIndexResponse {
            indexed,
            failed,
            errors,
        })
    }
}

/// Request to index an event embedding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexEventEmbeddingRequest {
    pub event_id: Uuid,
    pub tenant_id: String,
    pub embedding: Vec<f32>,
    pub source_text: Option<String>,
}

/// Response from batch indexing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchIndexResponse {
    pub indexed: usize,
    pub failed: usize,
    pub errors: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::entities::Event, infrastructure::repositories::InMemoryVectorSearchRepository,
    };
    use async_trait::async_trait;
    use chrono::Utc;
    use serde_json::json;

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

    fn create_test_use_case() -> (SemanticSearchUseCase, Arc<VectorSearchService>) {
        let vector_repo = Arc::new(InMemoryVectorSearchRepository::new());
        let vector_service = Arc::new(VectorSearchService::new(vector_repo));

        let events = vec![
            Event::from_strings(
                "user.created".to_string(),
                "user-1".to_string(),
                "tenant-1".to_string(),
                json!({"name": "Test"}),
                None,
            )
            .unwrap(),
        ];

        let event_repo = Arc::new(MockEventRepository::with_events(events));

        (
            SemanticSearchUseCase::new(vector_service.clone(), event_repo),
            vector_service,
        )
    }

    #[tokio::test]
    async fn test_semantic_search() {
        let (use_case, vector_service) = create_test_use_case();

        // Index some embeddings
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        vector_service
            .index_event(crate::application::services::IndexEventRequest {
                event_id: id1,
                tenant_id: "tenant-1".to_string(),
                embedding: EmbeddingVector::new(vec![1.0, 0.0, 0.0]).unwrap(),
                source_text: Some("first document".to_string()),
            })
            .await
            .unwrap();

        vector_service
            .index_event(crate::application::services::IndexEventRequest {
                event_id: id2,
                tenant_id: "tenant-1".to_string(),
                embedding: EmbeddingVector::new(vec![0.0, 1.0, 0.0]).unwrap(),
                source_text: Some("second document".to_string()),
            })
            .await
            .unwrap();

        // Search
        let response = use_case
            .execute(SemanticSearchUseCaseRequest {
                query_embedding: Some(vec![1.0, 0.0, 0.0]),
                k: Some(2),
                tenant_id: Some("tenant-1".to_string()),
                ..Default::default()
            })
            .await
            .unwrap();

        assert_eq!(response.count, 2);
        assert_eq!(response.results[0].event_id, id1);
        assert!((response.results[0].score - 1.0).abs() < 1e-6);
    }

    #[tokio::test]
    async fn test_find_similar() {
        let (use_case, vector_service) = create_test_use_case();

        // Index embeddings
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let id3 = Uuid::new_v4();

        vector_service
            .index_event(crate::application::services::IndexEventRequest {
                event_id: id1,
                tenant_id: "tenant-1".to_string(),
                embedding: EmbeddingVector::new(vec![1.0, 0.0, 0.0]).unwrap(),
                source_text: None,
            })
            .await
            .unwrap();

        vector_service
            .index_event(crate::application::services::IndexEventRequest {
                event_id: id2,
                tenant_id: "tenant-1".to_string(),
                embedding: EmbeddingVector::new(vec![0.9, 0.1, 0.0]).unwrap(),
                source_text: None,
            })
            .await
            .unwrap();

        vector_service
            .index_event(crate::application::services::IndexEventRequest {
                event_id: id3,
                tenant_id: "tenant-1".to_string(),
                embedding: EmbeddingVector::new(vec![0.0, 1.0, 0.0]).unwrap(),
                source_text: None,
            })
            .await
            .unwrap();

        // Find similar to id1
        let response = use_case
            .find_similar(id1, 2, Some("tenant-1".to_string()))
            .await
            .unwrap();

        // Should not include id1 itself
        assert!(!response.results.iter().any(|r| r.event_id == id1));
        assert!(response.results.len() <= 2);

        // id2 should be first (most similar to id1)
        assert_eq!(response.results[0].event_id, id2);
    }

    #[tokio::test]
    async fn test_validation_errors() {
        let (use_case, _) = create_test_use_case();

        // Missing embedding
        let result = use_case
            .execute(SemanticSearchUseCaseRequest {
                query_embedding: None,
                ..Default::default()
            })
            .await;
        assert!(result.is_err());

        // Empty embedding
        let result = use_case
            .execute(SemanticSearchUseCaseRequest {
                query_embedding: Some(vec![]),
                ..Default::default()
            })
            .await;
        assert!(result.is_err());

        // k = 0
        let result = use_case
            .execute(SemanticSearchUseCaseRequest {
                query_embedding: Some(vec![1.0, 0.0, 0.0]),
                k: Some(0),
                ..Default::default()
            })
            .await;
        assert!(result.is_err());

        // k too large
        let result = use_case
            .execute(SemanticSearchUseCaseRequest {
                query_embedding: Some(vec![1.0, 0.0, 0.0]),
                k: Some(2000),
                ..Default::default()
            })
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_index_use_case() {
        use crate::domain::repositories::VectorSearchRepository;

        let vector_repo = Arc::new(InMemoryVectorSearchRepository::new());
        let vector_service = Arc::new(VectorSearchService::new(vector_repo.clone()));
        let use_case = IndexEventEmbeddingUseCase::new(vector_service);

        let event_id = Uuid::new_v4();
        use_case
            .execute(IndexEventEmbeddingRequest {
                event_id,
                tenant_id: "tenant-1".to_string(),
                embedding: vec![1.0, 0.0, 0.0],
                source_text: Some("test content".to_string()),
            })
            .await
            .unwrap();

        assert_eq!(
            VectorSearchRepository::count(&*vector_repo, None)
                .await
                .unwrap(),
            1
        );
    }

    #[tokio::test]
    async fn test_batch_index_use_case() {
        let vector_repo = Arc::new(InMemoryVectorSearchRepository::new());
        let vector_service = Arc::new(VectorSearchService::new(vector_repo.clone()));
        let use_case = IndexEventEmbeddingUseCase::new(vector_service);

        let requests: Vec<_> = (0..5)
            .map(|i| IndexEventEmbeddingRequest {
                event_id: Uuid::new_v4(),
                tenant_id: "tenant-1".to_string(),
                embedding: vec![i as f32, 0.0, 0.0],
                source_text: None,
            })
            .collect();

        let response = use_case.execute_batch(requests).await.unwrap();
        assert_eq!(response.indexed, 5);
        assert_eq!(response.failed, 0);
    }

    /// Integration test: ingest events -> embed -> semantic search -> verify results
    #[tokio::test]
    async fn test_ingest_embed_search_integration() {
        use crate::application::{
            dto::IngestEventRequest, use_cases::ingest_event::IngestEventUseCase,
        };
        use std::sync::Mutex;

        // Mutable event repository shared between ingest and search
        struct SharedEventRepository {
            events: Mutex<Vec<Event>>,
        }

        impl SharedEventRepository {
            fn new() -> Self {
                Self {
                    events: Mutex::new(Vec::new()),
                }
            }
        }

        #[async_trait]
        impl EventRepository for SharedEventRepository {
            async fn save(&self, event: &Event) -> Result<()> {
                let mut events = self.events.lock().unwrap();
                events.push(Event::reconstruct_from_strings(
                    event.id(),
                    event.event_type_str().to_string(),
                    event.entity_id_str().to_string(),
                    event.tenant_id_str().to_string(),
                    event.payload().clone(),
                    event.timestamp(),
                    event.metadata().cloned(),
                    event.version(),
                ));
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

        // Step 1: Set up shared infrastructure
        let event_repo = Arc::new(SharedEventRepository::new());
        let vector_repo = Arc::new(InMemoryVectorSearchRepository::new());
        let vector_service = Arc::new(VectorSearchService::new(vector_repo));

        // Step 2: Ingest events
        let ingest_use_case = IngestEventUseCase::new(event_repo.clone());

        let response1 = ingest_use_case
            .execute(IngestEventRequest {
                event_type: "user.created".to_string(),
                entity_id: "user-1".to_string(),
                tenant_id: Some("tenant-1".to_string()),
                payload: json!({"name": "Alice", "role": "admin"}),
                metadata: None,
                expected_version: None,
                valid_from: None,
                valid_to: None,
            })
            .await
            .unwrap();

        let response2 = ingest_use_case
            .execute(IngestEventRequest {
                event_type: "order.placed".to_string(),
                entity_id: "order-1".to_string(),
                tenant_id: Some("tenant-1".to_string()),
                payload: json!({"amount": 99.99, "item": "widget"}),
                metadata: None,
                expected_version: None,
                valid_from: None,
                valid_to: None,
            })
            .await
            .unwrap();

        let response3 = ingest_use_case
            .execute(IngestEventRequest {
                event_type: "user.updated".to_string(),
                entity_id: "user-1".to_string(),
                tenant_id: Some("tenant-1".to_string()),
                payload: json!({"name": "Alice", "role": "superadmin"}),
                metadata: None,
                expected_version: None,
                valid_from: None,
                valid_to: None,
            })
            .await
            .unwrap();

        // Verify events were ingested
        assert_eq!(event_repo.events.lock().unwrap().len(), 3);

        // Step 3: Embed events (simulate embedding generation)
        let index_use_case = IndexEventEmbeddingUseCase::new(vector_service.clone());

        // user.created -> embedding close to [1, 0, 0]
        index_use_case
            .execute(IndexEventEmbeddingRequest {
                event_id: response1.event_id,
                tenant_id: "tenant-1".to_string(),
                embedding: vec![0.9, 0.1, 0.0],
                source_text: Some("user created Alice admin".to_string()),
            })
            .await
            .unwrap();

        // order.placed -> embedding close to [0, 1, 0]
        index_use_case
            .execute(IndexEventEmbeddingRequest {
                event_id: response2.event_id,
                tenant_id: "tenant-1".to_string(),
                embedding: vec![0.1, 0.9, 0.0],
                source_text: Some("order placed widget".to_string()),
            })
            .await
            .unwrap();

        // user.updated -> embedding close to [1, 0, 0] (similar to user.created)
        index_use_case
            .execute(IndexEventEmbeddingRequest {
                event_id: response3.event_id,
                tenant_id: "tenant-1".to_string(),
                embedding: vec![0.85, 0.15, 0.0],
                source_text: Some("user updated Alice superadmin".to_string()),
            })
            .await
            .unwrap();

        // Step 4: Semantic search — query for user-related events
        let search_use_case =
            SemanticSearchUseCase::new(vector_service.clone(), event_repo.clone());

        let search_response = search_use_case
            .execute(SemanticSearchUseCaseRequest {
                query_embedding: Some(vec![1.0, 0.0, 0.0]),
                k: Some(3),
                tenant_id: Some("tenant-1".to_string()),
                include_events: Some(true),
                ..Default::default()
            })
            .await
            .unwrap();

        // Step 5: Verify results
        assert_eq!(search_response.count, 3);

        // First result should be the most similar to [1, 0, 0] — user.created (0.9, 0.1, 0.0)
        assert_eq!(search_response.results[0].event_id, response1.event_id);
        // Second should be user.updated (0.85, 0.15, 0.0)
        assert_eq!(search_response.results[1].event_id, response3.event_id);
        // Third should be order.placed (0.1, 0.9, 0.0) — least similar
        assert_eq!(search_response.results[2].event_id, response2.event_id);

        // Scores should be descending
        assert!(search_response.results[0].score >= search_response.results[1].score);
        assert!(search_response.results[1].score >= search_response.results[2].score);

        // Events should be included in the response
        let events = search_response.events.expect("events should be included");
        assert_eq!(events.len(), 3);

        // Verify find_by_id was used to enrich results — check event data is correct
        assert_eq!(events[0].event_type, "user.created");
        assert_eq!(events[1].event_type, "user.updated");
        assert_eq!(events[2].event_type, "order.placed");
    }
}
