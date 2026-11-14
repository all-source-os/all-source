//! Event search over a live [`EventStore`]: semantic search through
//! [`SemanticSearchUseCase`] and hybrid search through [`HybridSearchEngine`],
//! both fed from the store's ingest broadcast.
//!
//! Reads never index. [`EventSearch::catch_up`] does, and refuses until an
//! embedder is installed; searches answer [`EventSearchError::Unavailable`]
//! until the first catch-up has completed.

use super::{
    EmbeddedEvent, HybridSearchEngine, KeywordSearchEngine, SearchQuery, VectorSearchEngine,
    VectorSearchEngineConfig, hybrid_search_engine::SearchMode,
};
use crate::{
    application::{
        dto::QueryEventsRequest,
        services::{IndexEventRequest, VectorSearchConfig, VectorSearchService},
        use_cases::{SemanticSearchUseCase, SemanticSearchUseCaseRequest},
    },
    domain::{entities::Event, repositories::EventRepository, value_objects::EmbeddingVector},
    error::{AllSourceError, Result},
    infrastructure::{
        memory::ledger::{Owner, Tally},
        repositories::InMemoryVectorSearchRepository,
    },
    store::EventStore,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::{
    collections::HashSet,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::sync::{Mutex, broadcast};
use uuid::Uuid;

pub const MAX_SEARCH_LIMIT: usize = 100;
const OVER_FETCH: usize = 5;
const MAX_FETCH: usize = 1000;
const RESCAN_PAGE: usize = 1000;

/// Turns text into the vector space events are indexed in.
pub trait QueryEmbedder: Send + Sync {
    fn embed(&self, text: &str) -> Result<Vec<f32>>;
}

#[cfg(feature = "prime-vectors")]
impl QueryEmbedder for crate::prime::vectors::TextEmbedder {
    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        crate::prime::vectors::TextEmbedder::embed(self, text)
            .map_err(|e| AllSourceError::InternalError(e.to_string()))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum EventSearchError {
    /// No embedder, index not built yet, or the embedder failed on the query.
    Unavailable(String),
    InvalidInput(String),
    Internal(String),
}

impl std::fmt::Display for EventSearchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable(m) | Self::InvalidInput(m) | Self::Internal(m) => f.write_str(m),
        }
    }
}

impl From<AllSourceError> for EventSearchError {
    fn from(err: AllSourceError) -> Self {
        match err {
            AllSourceError::InvalidInput(m) | AllSourceError::InvalidQuery(m) => {
                Self::InvalidInput(m)
            }
            other => Self::Internal(other.to_string()),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SemanticSearchParams {
    pub query: String,
    pub limit: Option<usize>,
    pub event_type: Option<String>,
    pub min_similarity: Option<f32>,
}

#[derive(Debug, Clone, Default)]
pub struct HybridSearchParams {
    pub query: String,
    pub limit: Option<usize>,
    pub event_type: Option<String>,
    pub entity_id: Option<String>,
    pub semantic_weight: Option<f32>,
    pub keyword_weight: Option<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EventSearchHit {
    pub event_id: Uuid,
    pub score: f32,
    pub semantic_score: Option<f32>,
    pub keyword_score: Option<f32>,
    pub event_type: String,
    pub entity_id: String,
    pub timestamp: DateTime<Utc>,
}

struct IndexerState {
    events: broadcast::Receiver<Arc<Event>>,
    indexed: HashSet<Uuid>,
    needs_rescan: bool,
    /// Events the store should hold if every ingest reached the broadcast.
    expected_total: usize,
}

pub struct EventSearch {
    store: Arc<EventStore>,
    embedder: OnceLock<Arc<dyn QueryEmbedder>>,
    vector_service: Arc<VectorSearchService>,
    semantic: SemanticSearchUseCase,
    hybrid: HybridSearchEngine,
    indexer: Mutex<IndexerState>,
    ready: AtomicBool,
    /// Indexed ids, the event payloads the keyword and hybrid indexes keep, and
    /// the embeddings: charged to `search` as they are added.
    charged: Tally,
}

impl EventSearch {
    /// Subscribes to the store's ingest broadcast immediately, so nothing
    /// ingested between now and the first catch-up is missed.
    pub fn new(store: Arc<EventStore>) -> Result<Arc<Self>> {
        let vector_service = Arc::new(
            VectorSearchService::new(Arc::new(InMemoryVectorSearchRepository::new())).with_config(
                VectorSearchConfig {
                    max_k: MAX_FETCH,
                    ..VectorSearchConfig::default()
                },
            ),
        );
        let event_repository: Arc<dyn EventRepository> =
            Arc::new(StoreEventRepository(Arc::clone(&store)));
        let semantic = SemanticSearchUseCase::new(Arc::clone(&vector_service), event_repository);
        let hybrid = HybridSearchEngine::new(
            Arc::new(VectorSearchEngine::with_external_embeddings(
                VectorSearchEngineConfig::default(),
            )),
            Arc::new(KeywordSearchEngine::new()?),
        );
        let events = store.subscribe_events();
        Ok(Arc::new(Self {
            store,
            embedder: OnceLock::new(),
            vector_service,
            semantic,
            hybrid,
            indexer: Mutex::new(IndexerState {
                events,
                indexed: HashSet::new(),
                needs_rescan: true,
                expected_total: 0,
            }),
            ready: AtomicBool::new(false),
            charged: Tally::new(Owner::Search),
        }))
    }

    /// A second embedder is ignored: the index must stay in one vector space.
    pub fn set_embedder(&self, embedder: Arc<dyn QueryEmbedder>) {
        if self.embedder.set(embedder).is_err() {
            tracing::warn!("event search embedder installed twice; keeping the first");
        }
    }

    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }

    /// Indexes every event the store holds that is not indexed yet.
    /// Returns how many events were newly indexed.
    /// Without an embedder only the keyword index is built, so hybrid search
    /// answers keyword-only and semantic search stays unavailable.
    pub async fn catch_up(&self) -> std::result::Result<usize, EventSearchError> {
        let embedder = self.embedder.get().cloned();
        let mut state = self.indexer.lock().await;

        let total_before_drain = self.store.total_events();
        let mut pending = Vec::new();
        loop {
            match state.events.try_recv() {
                Ok(event) => {
                    state.expected_total += 1;
                    pending.push(event);
                }
                Err(broadcast::error::TryRecvError::Lagged(missed)) => {
                    tracing::warn!("event search missed {missed} ingest notifications; rescanning");
                    state.needs_rescan = true;
                }
                Err(_) => break,
            }
        }
        if total_before_drain > state.expected_total {
            state.needs_rescan = true;
        }

        let mut indexed = 0;
        if state.needs_rescan {
            let total = self.store.total_events();
            let mut offset = 0u64;
            loop {
                let page = self.store.events_after_offset(offset, &[], RESCAN_PAGE);
                let Some((last, _)) = page.last() else { break };
                offset = *last;
                let events: Vec<Arc<Event>> = page.into_iter().map(|(_, e)| Arc::new(e)).collect();
                indexed += self.index(&mut state, embedder.as_ref(), events).await?;
            }
            state.expected_total = state.expected_total.max(total);
            state.needs_rescan = false;
        }
        indexed += self.index(&mut state, embedder.as_ref(), pending).await?;

        self.ready.store(true, Ordering::Release);
        Ok(indexed)
    }

    pub async fn semantic(
        &self,
        tenant_id: &str,
        params: SemanticSearchParams,
    ) -> std::result::Result<Vec<EventSearchHit>, EventSearchError> {
        let limit = validate_limit(params.limit)?;
        let query = validate_query(&params.query)?;
        if let Some(min) = params.min_similarity
            && !(min.is_finite() && (-1.0..=1.0).contains(&min))
        {
            return Err(EventSearchError::InvalidInput(
                "min_similarity must be a number between -1 and 1".to_string(),
            ));
        }
        let embedding = self.embed_query(query)?;
        let fetch = if params.event_type.is_some() {
            (limit * OVER_FETCH).min(MAX_FETCH)
        } else {
            limit
        };

        let response = self
            .semantic
            .execute(SemanticSearchUseCaseRequest {
                query_embedding: Some(embedding),
                k: Some(fetch),
                tenant_id: Some(tenant_id.to_string()),
                event_type: params.event_type.clone(),
                min_similarity: params.min_similarity,
                ..SemanticSearchUseCaseRequest::default()
            })
            .await?;

        let mut hits = Vec::with_capacity(limit);
        for result in response.results {
            let Some(event) = self.tenant_event(tenant_id, result.event_id)? else {
                continue;
            };
            if params
                .event_type
                .as_deref()
                .is_some_and(|t| t != event.event_type_str())
            {
                continue;
            }
            hits.push(hit(&event, result.score, None, None));
            if hits.len() == limit {
                break;
            }
        }
        Ok(hits)
    }

    pub fn hybrid(
        &self,
        tenant_id: &str,
        params: HybridSearchParams,
    ) -> std::result::Result<Vec<EventSearchHit>, EventSearchError> {
        let limit = validate_limit(params.limit)?;
        let query = validate_query(&params.query)?;
        for (name, weight) in [
            ("semantic_weight", params.semantic_weight),
            ("keyword_weight", params.keyword_weight),
        ] {
            if let Some(w) = weight
                && !(w.is_finite() && w >= 0.0)
            {
                return Err(EventSearchError::InvalidInput(format!(
                    "{name} must be a non-negative number"
                )));
            }
        }
        let mut search = SearchQuery::new(query)
            .with_limit(limit)
            .with_tenant(tenant_id)
            .with_weights(params.semantic_weight, params.keyword_weight);
        if self.embedder.get().is_some() {
            search = search.with_query_embedding(self.embed_query(query)?);
        } else {
            self.require_ready()?;
            search = search.with_mode(SearchMode::KeywordOnly);
        }
        if let Some(event_type) = params.event_type {
            search = search.with_event_type(event_type);
        }
        if let Some(entity_id) = params.entity_id {
            search = search.with_entity_id(entity_id);
        }

        let mut hits = Vec::new();
        for result in self.hybrid.search(&search)? {
            if let Some(event) = self.tenant_event(tenant_id, result.event_id)? {
                hits.push(hit(
                    &event,
                    result.score,
                    result.semantic_score,
                    result.keyword_score,
                ));
            }
        }
        Ok(hits)
    }

    fn embedder(&self) -> std::result::Result<Arc<dyn QueryEmbedder>, EventSearchError> {
        self.embedder.get().cloned().ok_or_else(|| {
            EventSearchError::Unavailable(
                "event search is unavailable: no embedding model is loaded in this Core"
                    .to_string(),
            )
        })
    }

    fn require_ready(&self) -> std::result::Result<(), EventSearchError> {
        if self.is_ready() {
            Ok(())
        } else {
            Err(EventSearchError::Unavailable(
                "event search index is still being built; retry shortly".to_string(),
            ))
        }
    }

    fn embed_query(&self, query: &str) -> std::result::Result<Vec<f32>, EventSearchError> {
        let embedder = self.embedder()?;
        self.require_ready()?;
        embedder.embed(query).map_err(|e| {
            EventSearchError::Unavailable(format!("could not embed the search query: {e}"))
        })
    }

    /// Defence in depth: a hit is returned only when the store still holds the
    /// event and it belongs to the caller's tenant.
    fn tenant_event(
        &self,
        tenant_id: &str,
        event_id: Uuid,
    ) -> std::result::Result<Option<Event>, EventSearchError> {
        Ok(self
            .store
            .get_event_by_id(&event_id)?
            .filter(|e| e.tenant_id_str() == tenant_id))
    }

    async fn index(
        &self,
        state: &mut IndexerState,
        embedder: Option<&Arc<dyn QueryEmbedder>>,
        events: Vec<Arc<Event>>,
    ) -> std::result::Result<usize, EventSearchError> {
        let fresh: Vec<Arc<Event>> = events
            .into_iter()
            .filter(|e| !state.indexed.contains(&e.id()))
            .collect();
        if fresh.is_empty() {
            return Ok(0);
        }

        let embeddings: Vec<Option<Vec<f32>>> = match embedder {
            Some(embedder) => self.embed_batch(embedder, &fresh).await?,
            None => vec![None; fresh.len()],
        };

        let mut documents = Vec::with_capacity(fresh.len());
        let mut charge = 0u64;
        for (event, embedding) in fresh.iter().zip(embeddings) {
            charge += event.resident_bytes();
            if let Some(values) = &embedding {
                charge += (values.len() * size_of::<f32>()) as u64;
                let source_text = VectorSearchEngine::extract_text_from_payload(event.payload());
                self.vector_service
                    .index_event(IndexEventRequest {
                        event_id: event.id(),
                        tenant_id: event.tenant_id_str().to_string(),
                        embedding: EmbeddingVector::new(values.clone())?,
                        source_text: Some(source_text),
                    })
                    .await?;
            }
            documents.push(EmbeddedEvent {
                event_id: event.id(),
                tenant_id: event.tenant_id_str().to_string(),
                event_type: event.event_type_str().to_string(),
                entity_id: Some(event.entity_id_str().to_string()),
                payload: event.payload().clone(),
                timestamp: event.timestamp(),
                embedding,
            });
        }
        self.hybrid.index_embedded(documents)?;

        self.charged.add(charge);
        let slots_before = state.indexed.capacity();
        state.indexed.extend(fresh.iter().map(|e| e.id()));
        self.charged
            .grow(slots_before, state.indexed.capacity(), size_of::<Uuid>());
        Ok(fresh.len())
    }

    async fn embed_batch(
        &self,
        embedder: &Arc<dyn QueryEmbedder>,
        events: &[Arc<Event>],
    ) -> std::result::Result<Vec<Option<Vec<f32>>>, EventSearchError> {
        let to_embed = Arc::clone(embedder);
        let batch = events.to_vec();
        tokio::task::spawn_blocking(move || {
            batch
                .iter()
                .map(|event| {
                    let text = VectorSearchEngine::extract_text_from_payload(event.payload());
                    if text.is_empty() {
                        return None;
                    }
                    to_embed
                        .embed(&text)
                        .inspect_err(|e| {
                            tracing::warn!(
                                "event search could not embed event {}: {e}",
                                event.id()
                            );
                        })
                        .ok()
                })
                .collect()
        })
        .await
        .map_err(|e| EventSearchError::Internal(format!("embedding task failed: {e}")))
    }
}

fn validate_limit(limit: Option<usize>) -> std::result::Result<usize, EventSearchError> {
    match limit.unwrap_or(10) {
        0 => Err(EventSearchError::InvalidInput(
            "limit must be at least 1".to_string(),
        )),
        n if n > MAX_SEARCH_LIMIT => Err(EventSearchError::InvalidInput(format!(
            "limit cannot exceed {MAX_SEARCH_LIMIT}"
        ))),
        n => Ok(n),
    }
}

fn validate_query(query: &str) -> std::result::Result<&str, EventSearchError> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Err(EventSearchError::InvalidInput(
            "query must not be empty".to_string(),
        ));
    }
    Ok(trimmed)
}

fn hit(
    event: &Event,
    score: f32,
    semantic_score: Option<f32>,
    keyword_score: Option<f32>,
) -> EventSearchHit {
    EventSearchHit {
        event_id: event.id(),
        score,
        semantic_score,
        keyword_score,
        event_type: event.event_type_str().to_string(),
        entity_id: event.entity_id_str().to_string(),
        timestamp: event.timestamp(),
    }
}

/// [`EventRepository`] over the live store, for the semantic search use case.
struct StoreEventRepository(Arc<EventStore>);

impl StoreEventRepository {
    fn query(&self, request: &QueryEventsRequest) -> Result<Vec<Event>> {
        self.0.query(request)
    }
}

#[async_trait]
impl EventRepository for StoreEventRepository {
    async fn save(&self, event: &Event) -> Result<()> {
        self.0.ingest(event)
    }

    async fn save_batch(&self, events: &[Event]) -> Result<()> {
        self.0.ingest_batch(events.to_vec())
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<Event>> {
        self.0.get_event_by_id(&id)
    }

    async fn find_by_entity(&self, entity_id: &str, tenant_id: &str) -> Result<Vec<Event>> {
        self.query(&QueryEventsRequest {
            entity_id: Some(entity_id.to_string()),
            tenant_id: Some(tenant_id.to_string()),
            ..QueryEventsRequest::default()
        })
    }

    async fn find_by_type(&self, event_type: &str, tenant_id: &str) -> Result<Vec<Event>> {
        self.query(&QueryEventsRequest {
            event_type: Some(event_type.to_string()),
            tenant_id: Some(tenant_id.to_string()),
            ..QueryEventsRequest::default()
        })
    }

    async fn find_by_time_range(
        &self,
        tenant_id: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<Event>> {
        self.query(&QueryEventsRequest {
            tenant_id: Some(tenant_id.to_string()),
            since: Some(start),
            until: Some(end),
            ..QueryEventsRequest::default()
        })
    }

    async fn find_by_entity_as_of(
        &self,
        entity_id: &str,
        tenant_id: &str,
        as_of: DateTime<Utc>,
    ) -> Result<Vec<Event>> {
        self.query(&QueryEventsRequest {
            entity_id: Some(entity_id.to_string()),
            tenant_id: Some(tenant_id.to_string()),
            as_of: Some(as_of),
            ..QueryEventsRequest::default()
        })
    }

    async fn count(&self, tenant_id: &str) -> Result<usize> {
        Ok(self
            .query(&QueryEventsRequest {
                tenant_id: Some(tenant_id.to_string()),
                ..QueryEventsRequest::default()
            })?
            .len())
    }

    async fn health_check(&self) -> Result<()> {
        Ok(())
    }
}
