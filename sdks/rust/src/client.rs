use reqwest::header::{HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use std::{
    sync::{Arc, Mutex, PoisonError},
    time::{Duration, Instant},
};

use crate::{
    circuit_breaker::{CircuitBreaker, State as CircuitState},
    error::Error,
    fold::EventFolder,
    latest::view_refusal,
    normalize::normalize_event_type,
    paginate::{EntityPaginator, EventPaginator, DEFAULT_PAGE_SIZE},
    response_meta::{
        is_rate_limit_refusal, parse_retry_after, refusal_scope, EventsQuery, ResponseMeta,
    },
    types::*,
};

/// Retry configuration for transient failures.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts. Default: 3.
    pub max_retries: u32,
    /// Base delay between retries. Default: 200ms.
    pub base_delay: Duration,
    /// Backoff multiplier. Default: 2.0.
    pub backoff_factor: f64,
    /// Maximum delay between retries. Default: 10s.
    pub max_delay: Duration,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay: Duration::from_millis(200),
            backoff_factor: 2.0,
            max_delay: Duration::from_secs(10),
        }
    }
}

/// Full configuration for building a client.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Base URL of the service.
    pub base_url: String,
    /// API key for authentication.
    pub api_key: String,
    /// HTTP request timeout. Default: 30s.
    pub timeout: Duration,
    /// Retry configuration.
    pub retry: RetryConfig,
    /// Circuit breaker failure threshold. Default: 5.
    pub circuit_breaker_threshold: u32,
    /// Circuit breaker recovery timeout. Default: 30s.
    pub circuit_breaker_recovery: Duration,
}

impl ClientConfig {
    pub fn new(base_url: &str, api_key: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            api_key: api_key.to_string(),
            timeout: Duration::from_secs(30),
            retry: RetryConfig::default(),
            circuit_breaker_threshold: 5,
            circuit_breaker_recovery: Duration::from_secs(30),
        }
    }
}

/// Shared HTTP transport with retry and circuit breaker.
#[derive(Debug)]
pub(crate) struct HttpTransport {
    pub(crate) base_url: String,
    /// Kept alongside the reqwest default headers so non-HTTP transports
    /// (the WebSocket stream) can authenticate with the same credential.
    pub(crate) api_key: String,
    http: reqwest::Client,
    retry: RetryConfig,
    circuit_breaker: CircuitBreaker,
    /// The instant before which every call fails fast with
    /// [`Error::RateLimited`], and the scope of the refusal that set it.
    not_before: Mutex<Option<(Instant, Option<String>)>>,
}

impl HttpTransport {
    fn new(config: &ClientConfig) -> Result<Self, Error> {
        if config.base_url.is_empty() {
            return Err(Error::Config("base_url is required".into()));
        }
        if config.api_key.is_empty() {
            return Err(Error::Config("api_key is required".into()));
        }

        let mut headers = HeaderMap::new();
        headers.insert(
            "Authorization",
            HeaderValue::from_str(&format!("Bearer {}", config.api_key))
                .map_err(|e| Error::Config(format!("invalid API key: {e}")))?,
        );
        headers.insert("Accept", HeaderValue::from_static("application/json"));

        let http = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(config.timeout)
            .build()?;

        Ok(Self {
            base_url: config.base_url.trim_end_matches('/').to_string(),
            api_key: config.api_key.clone(),
            http,
            retry: config.retry.clone(),
            circuit_breaker: CircuitBreaker::new(
                config.circuit_breaker_threshold,
                config.circuit_breaker_recovery,
            ),
            not_before: Mutex::new(None),
        })
    }

    pub(crate) fn circuit_state(&self) -> CircuitState {
        self.circuit_breaker.state()
    }

    fn pending_rate_limit(&self) -> Option<Error> {
        let mut guard = self
            .not_before
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let (until, scope) = guard.as_ref()?;
        let now = Instant::now();
        if now < *until {
            return Some(Error::RateLimited {
                retry_after: Some(*until - now),
                scope: scope.clone(),
            });
        }
        *guard = None;
        None
    }

    fn note_rate_limit(&self, err: &Error) {
        if let Error::RateLimited {
            retry_after: Some(wait),
            scope,
        } = err
        {
            *self
                .not_before
                .lock()
                .unwrap_or_else(PoisonError::into_inner) =
                Some((Instant::now() + *wait, scope.clone()));
        }
    }

    /// GET that sends `If-None-Match` when given and returns `None` on a 304,
    /// together with the response's [`ResponseMeta`].
    pub(crate) async fn get_with_meta<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        query: &[(&str, String)],
        if_none_match: Option<&str>,
    ) -> Result<(Option<T>, ResponseMeta), Error> {
        self.with_retry(|| async {
            let url = format!("{}{}", self.base_url, path);
            let mut req = self.http.get(&url).query(query);
            if let Some(etag) = if_none_match {
                req = req.header(reqwest::header::IF_NONE_MATCH, etag);
            }
            let resp = req.send().await?;
            let meta = ResponseMeta::from_headers(resp.headers());
            if resp.status() == reqwest::StatusCode::NOT_MODIFIED {
                return Ok((None, meta));
            }
            let value: T = self.handle_response(resp).await?;
            Ok((Some(value), meta))
        })
        .await
    }

    pub(crate) async fn get<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T, Error> {
        self.with_retry(|| async {
            let url = format!("{}{}", self.base_url, path);
            let resp = self.http.get(&url).send().await?;
            self.handle_response(resp).await
        })
        .await
    }

    pub(crate) async fn get_with_query<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<T, Error> {
        self.with_retry(|| async {
            let url = format!("{}{}", self.base_url, path);
            let resp = self.http.get(&url).query(query).send().await?;
            self.handle_response(resp).await
        })
        .await
    }

    pub(crate) async fn post<T: for<'de> Deserialize<'de>, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, Error> {
        self.with_retry(|| async {
            let url = format!("{}{}", self.base_url, path);
            let resp = self.http.post(&url).json(body).send().await?;
            self.handle_response(resp).await
        })
        .await
    }

    pub(crate) async fn put<T: for<'de> Deserialize<'de>, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, Error> {
        self.with_retry(|| async {
            let url = format!("{}{}", self.base_url, path);
            let resp = self.http.put(&url).json(body).send().await?;
            self.handle_response(resp).await
        })
        .await
    }

    /// DELETE that ignores the response body, so an empty `200` or `204` succeeds.
    pub(crate) async fn delete(&self, path: &str) -> Result<(), Error> {
        self.with_retry(|| async {
            let url = format!("{}{}", self.base_url, path);
            let resp = self.http.delete(&url).send().await?;
            if resp.status().is_success() {
                return Ok(());
            }
            self.handle_response::<serde_json::Value>(resp)
                .await
                .map(|_| ())
        })
        .await
    }

    /// GET returning `None` on 404. Useful for "does this key exist" lookups
    /// where absence is a normal case, not an error.
    pub(crate) async fn get_optional<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
    ) -> Result<Option<T>, Error> {
        self.with_retry(|| async {
            let url = format!("{}{}", self.base_url, path);
            let resp = self.http.get(&url).send().await?;
            if resp.status().as_u16() == 404 {
                return Ok(None);
            }
            let value: T = self.handle_response(resp).await?;
            Ok(Some(value))
        })
        .await
    }

    async fn with_retry<T, F, Fut>(&self, f: F) -> Result<T, Error>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T, Error>>,
    {
        if let Some(err) = self.pending_rate_limit() {
            return Err(err);
        }
        let permit = self
            .circuit_breaker
            .acquire()
            .map_err(|retry_after_secs| Error::CircuitOpen { retry_after_secs })?;
        // A half-open probe is one request, not one request plus its retries.
        let max_retries = if permit.is_probe() {
            0
        } else {
            self.retry.max_retries
        };

        let mut last_error = None;
        for attempt in 0..=max_retries {
            if attempt > 0 {
                let delay = self
                    .retry
                    .base_delay
                    .mul_f64(self.retry.backoff_factor.powi(attempt as i32 - 1));
                let delay = delay.min(self.retry.max_delay);
                tracing::debug!(attempt, delay_ms = delay.as_millis(), "retrying request");
                tokio::time::sleep(delay).await;
            }

            match f().await {
                Ok(result) => {
                    self.circuit_breaker.record_success();
                    return Ok(result);
                }
                Err(e @ Error::RateLimited { .. }) => {
                    self.note_rate_limit(&e);
                    return Err(e);
                }
                Err(e) if e.is_retryable() && attempt < max_retries => {
                    tracing::warn!(attempt, error = %e, "transient error, will retry");
                    self.circuit_breaker.record_failure();
                    last_error = Some(e);
                }
                Err(e) => {
                    if e.is_server_error() || matches!(e, Error::Http(_)) {
                        self.circuit_breaker.record_failure();
                    }
                    return Err(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| Error::Config("retry exhausted".into())))
    }

    async fn handle_response<T: for<'de> Deserialize<'de>>(
        &self,
        resp: reqwest::Response,
    ) -> Result<T, Error> {
        let status = resp.status().as_u16();
        if !(200..300).contains(&status) {
            let retry_after = parse_retry_after(resp.headers());
            let body: Option<serde_json::Value> = resp.json().await.ok();
            let scope = refusal_scope(body.as_ref());
            if is_rate_limit_refusal(status, scope.as_deref()) {
                return Err(Error::RateLimited { retry_after, scope });
            }
            let message = body
                .as_ref()
                .and_then(|b| b.get("error").and_then(|e| e.get("message")))
                .and_then(|m| m.as_str())
                .or_else(|| {
                    body.as_ref()
                        .and_then(|b| b.get("error"))
                        .and_then(|e| e.as_str())
                })
                .unwrap_or("Unknown error")
                .to_string();
            if let Some(err) = view_refusal(status, body.as_ref()) {
                return Err(err);
            }
            // A compare-and-swap rejection is a distinct outcome, not a
            // transport failure: the caller has to re-read and recompute, and
            // must NOT blindly retry. Core answers 409 with
            // {"error":"version_conflict","expected_version":N,"current_version":M},
            // so surface it as a typed variant carrying both versions.
            if status == 409 {
                if let Some(body) = body.as_ref() {
                    if body.get("error").and_then(|e| e.as_str()) == Some("version_conflict") {
                        let field = |name: &str| body.get(name).and_then(serde_json::Value::as_u64);
                        if let (Some(expected), Some(current)) =
                            (field("expected_version"), field("current_version"))
                        {
                            return Err(Error::VersionConflict { expected, current });
                        }
                    }
                }
            }
            return Err(Error::Api {
                status,
                message,
                body,
            });
        }
        Ok(resp.json().await?)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// QueryClient — reads from Query Service
// ═══════════════════════════════════════════════════════════════════════════

/// Client for the AllSource Query Service (port 3902).
///
/// Use this when your traffic goes through the gateway: multi-tenant, external
/// clients, per-tenant rate limits, or billing/quota enforcement. QS validates
/// the API key (120s ETS cache, then delegates to Core), applies tenant
/// context, and forwards to Core.
///
/// For co-located services where you control both sides and don't need those
/// gateway features, prefer [`CoreClient`] — one hop, no cache layer.
///
/// Cloning is cheap; the underlying `reqwest::Client` and its connection pool
/// are shared via `Arc`.
#[derive(Debug, Clone)]
pub struct QueryClient {
    transport: Arc<HttpTransport>,
}

/// Gateway responses wrap their payload in a `{"data": ...}` envelope.
#[derive(Deserialize)]
struct Envelope<T> {
    data: T,
}

/// Request body for [`QueryClient::define_prime_projection`].
#[derive(Serialize)]
struct DefinePrimeProjectionRequest {
    entity_type: String,
    field_policies: std::collections::BTreeMap<String, String>,
}

impl QueryClient {
    /// Create a new Query Service client.
    pub fn new(base_url: &str, api_key: &str) -> Result<Self, Error> {
        Self::with_config(ClientConfig::new(base_url, api_key))
    }

    /// Create with full configuration.
    #[allow(clippy::needless_pass_by_value)] // public API — taking ownership is idiomatic
    pub fn with_config(config: ClientConfig) -> Result<Self, Error> {
        Ok(Self {
            transport: Arc::new(HttpTransport::new(&config)?),
        })
    }

    /// Query events with filters.
    ///
    /// Uses Core's `/api/v1/events/query` endpoint. Results are ordered by
    /// `(timestamp, version)` — ascending (oldest first) by default, or set
    /// [`QueryEventsParams::order`] / [`QueryEventsParams::order_desc`] for
    /// newest-first.
    ///
    /// The response headers are in [`QueryEventsResponse::meta`]. With
    /// [`QueryEventsParams::if_none_match`] set, a 304 fails with
    /// [`Error::NotModified`]; use [`Self::query_events_conditional`] to get it
    /// as a value instead.
    pub async fn query_events(
        &self,
        params: QueryEventsParams,
    ) -> Result<QueryEventsResponse, Error> {
        match self.query_events_conditional(params).await? {
            EventsQuery::Modified(resp) => Ok(resp),
            EventsQuery::NotModified(meta) => Err(Error::NotModified { etag: meta.etag }),
        }
    }

    /// Query events, sending [`QueryEventsParams::if_none_match`] as
    /// `If-None-Match`, and report a 304 as [`EventsQuery::NotModified`].
    pub async fn query_events_conditional(
        &self,
        params: QueryEventsParams,
    ) -> Result<EventsQuery, Error> {
        let pairs = params.to_query_pairs();
        let (body, meta) = self
            .transport
            .get_with_meta::<QueryEventsResponse>(
                "/api/v1/events/query",
                &pairs,
                params.if_none_match.as_deref(),
            )
            .await?;
        Ok(match body {
            Some(mut resp) => {
                resp.meta = meta;
                EventsQuery::Modified(resp)
            }
            None => EventsQuery::NotModified(meta),
        })
    }

    /// The circuit breaker's current state.
    pub fn circuit_state(&self) -> CircuitState {
        self.transport.circuit_state()
    }

    pub(crate) fn http(&self) -> &Arc<HttpTransport> {
        &self.transport
    }

    /// Get all events for a specific entity.
    pub async fn get_entity_events(&self, entity_id: &str) -> Result<QueryEventsResponse, Error> {
        self.query_events(QueryEventsParams::new().entity_id(entity_id))
            .await
    }

    /// Get all events of a specific type.
    pub async fn get_events_by_type(&self, event_type: &str) -> Result<QueryEventsResponse, Error> {
        self.query_events(QueryEventsParams::new().event_type(event_type))
            .await
    }

    /// List projections.
    pub async fn list_projections(&self) -> Result<ProjectionsResponse, Error> {
        self.transport.get("/api/v1/projections").await
    }

    /// Analyze one enabled tenant projection without mutating events or state.
    pub async fn analyze_projection_replay(
        &self,
        projection_name: &str,
    ) -> Result<ProjectionReplayAnalysis, Error> {
        let resp: Envelope<ProjectionReplayAnalysis> = self
            .transport
            .post(
                "/api/replay/preview",
                &serde_json::json!({"projection_name": projection_name}),
            )
            .await?;
        Ok(resp.data)
    }

    /// Start an atomic rebuild of one enabled tenant projection.
    pub async fn start_projection_replay(
        &self,
        projection_name: &str,
    ) -> Result<ProjectionReplayRun, Error> {
        let resp: Envelope<ProjectionReplayRun> = self
            .transport
            .post(
                "/api/replay",
                &serde_json::json!({"projection_name": projection_name}),
            )
            .await?;
        Ok(resp.data)
    }

    /// List replay runs belonging to the authenticated tenant.
    pub async fn list_projection_replays(&self) -> Result<Vec<ProjectionReplayRun>, Error> {
        let resp: Envelope<Vec<ProjectionReplayRun>> = self.transport.get("/api/replay").await?;
        Ok(resp.data)
    }

    /// Read one tenant-scoped replay run.
    pub async fn get_projection_replay(
        &self,
        replay_id: &str,
    ) -> Result<ProjectionReplayRun, Error> {
        let path = format!("/api/replay/{replay_id}");
        let resp: Envelope<ProjectionReplayRun> = self.transport.get(&path).await?;
        Ok(resp.data)
    }

    /// Cancel a running rebuild without publishing its partial generation.
    pub async fn cancel_projection_replay(
        &self,
        replay_id: &str,
    ) -> Result<ProjectionReplayRun, Error> {
        let path = format!("/api/replay/{replay_id}/cancel");
        let resp: Envelope<ProjectionReplayRun> =
            self.transport.post(&path, &serde_json::json!({})).await?;
        Ok(resp.data)
    }

    /// List all Prime projection definitions.
    ///
    /// Proxies the gateway's `GET /api/v1/prime/projections`. Each entry maps an
    /// entity type to its per-field merge policies.
    pub async fn list_prime_projections(&self) -> Result<Vec<PrimeProjection>, Error> {
        let resp: Envelope<Vec<PrimeProjection>> =
            self.transport.get("/api/v1/prime/projections").await?;
        Ok(resp.data)
    }

    /// Define (or replace) a Prime projection for an entity type.
    ///
    /// Proxies the gateway's `POST /api/v1/prime/projections`. `field_policies`
    /// maps each field to a policy string
    /// (`last_write` | `highest_priority` | `most_specific` | `merge_array`).
    pub async fn define_prime_projection(
        &self,
        entity_type: &str,
        field_policies: std::collections::BTreeMap<String, String>,
    ) -> Result<PrimeProjectionAck, Error> {
        let body = DefinePrimeProjectionRequest {
            entity_type: entity_type.to_string(),
            field_policies,
        };
        let resp: Envelope<PrimeProjectionAck> = self
            .transport
            .post("/api/v1/prime/projections", &body)
            .await?;
        Ok(resp.data)
    }

    /// Materialize a Prime node's current projected state.
    ///
    /// Proxies the gateway's `POST /api/v1/prime/nodes/{id}/project`. Node ids
    /// (e.g. `node:contact:1`) are interpolated raw — the `:` characters are
    /// path-safe — to stay consistent with the Go and TypeScript SDKs.
    pub async fn project_node(&self, node_id: &str) -> Result<PrimeSnapshot, Error> {
        let path = format!("/api/v1/prime/nodes/{node_id}/project");
        let resp: Envelope<PrimeSnapshot> =
            self.transport.post(&path, &serde_json::json!({})).await?;
        Ok(resp.data)
    }

    /// Fetch provenance for a single field of a Prime node.
    ///
    /// Proxies the gateway's
    /// `GET /api/v1/prime/nodes/{id}/fields/{field}/provenance`. Returns `None`
    /// when the gateway responds 404 (no provenance recorded for that field).
    pub async fn node_field_provenance(
        &self,
        node_id: &str,
        field: &str,
    ) -> Result<Option<PrimeProvenance>, Error> {
        let path = format!("/api/v1/prime/nodes/{node_id}/fields/{field}/provenance");
        let resp: Option<Envelope<PrimeProvenance>> = self.transport.get_optional(&path).await?;
        Ok(resp.map(|e| e.data))
    }

    /// Check health.
    pub async fn health(&self) -> Result<HealthResponse, Error> {
        self.transport.get("/health").await
    }

    /// List distinct entities, optionally filtered by event type prefix.
    ///
    /// Uses Core's `/api/v1/entities` endpoint. Entities are sorted by
    /// last-event time — most recently active first by default, or set
    /// [`ListEntitiesParams::order`] / [`ListEntitiesParams::order_asc`] for the
    /// opposite. The order is total (ties broken by `entity_id`), so
    /// `limit`/`offset` pagination is stable; see [`Self::list_entities_paged`].
    pub async fn list_entities(
        &self,
        params: ListEntitiesParams,
    ) -> Result<ListEntitiesResponse, Error> {
        let pairs = params.to_query_pairs();
        self.transport
            .get_with_query("/api/v1/entities", &pairs)
            .await
    }

    /// Auto-paginating cursor over [`Self::query_events`].
    ///
    /// Drives `limit`/`offset` paging for you: each [`EventPaginator::next_page`]
    /// fetches the next batch until the server reports no more. The `limit` set
    /// on `params` (default [`DEFAULT_PAGE_SIZE`]) becomes the per-page size;
    /// any `offset` on `params` is the starting offset.
    pub fn query_events_paged(&self, params: QueryEventsParams) -> EventPaginator {
        let page_size = params.limit.unwrap_or(DEFAULT_PAGE_SIZE);
        EventPaginator::new(self.clone(), params, page_size)
    }

    /// Auto-paginating cursor over [`Self::list_entities`].
    ///
    /// Drives `limit`/`offset` paging for you: each [`EntityPaginator::next_page`]
    /// fetches the next batch until the server reports no more. The `limit` set
    /// on `params` (default [`DEFAULT_PAGE_SIZE`]) becomes the per-page size;
    /// any `offset` on `params` is the starting offset.
    pub fn list_entities_paged(&self, params: ListEntitiesParams) -> EntityPaginator {
        let page_size = params.limit.unwrap_or(DEFAULT_PAGE_SIZE);
        EntityPaginator::new(self.clone(), params, page_size)
    }

    /// Detect duplicate entities by grouping on payload field values.
    ///
    /// Uses Core's `/api/v1/entities/duplicates` endpoint.
    /// Requires `event_type_prefix` to scope the search (no full-store scans).
    pub async fn detect_duplicates(
        &self,
        event_type_prefix: &str,
        group_by: &str,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<DetectDuplicatesResponse, Error> {
        let mut pairs: Vec<(&str, String)> = vec![
            ("event_type_prefix", event_type_prefix.to_string()),
            ("group_by", group_by.to_string()),
        ];
        if let Some(limit) = limit {
            pairs.push(("limit", limit.to_string()));
        }
        if let Some(offset) = offset {
            pairs.push(("offset", offset.to_string()));
        }
        self.transport
            .get_with_query("/api/v1/entities/duplicates", &pairs)
            .await
    }

    /// Query events and fold them into domain state in one call.
    ///
    /// Convenience method that combines [`Self::query_events`] + [`crate::fold::fold_events`].
    pub async fn query_and_fold<F: EventFolder>(
        &self,
        params: QueryEventsParams,
    ) -> Result<Option<F::State>, Error> {
        let resp = self.query_events(params).await?;
        Ok(crate::fold::fold_events::<F>(&resp.events))
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// CoreClient — writes to Core
// ═══════════════════════════════════════════════════════════════════════════

/// Client for AllSource Core (port 3900).
///
/// Low-latency path: one hop, in-process DashMap key validation (~μs), no rate
/// limit or quota layer. Use for co-located services doing writes,
/// projection work, or anything latency-sensitive. Powers
/// [`ProjectionWorker`](crate::ProjectionWorker) internally.
///
/// For multi-tenant / external / billing-gated traffic, use [`QueryClient`]
/// instead — that goes through the Query Service which owns rate limiting
/// and quota enforcement.
///
/// Cloning is cheap; the underlying `reqwest::Client` and its connection pool
/// are shared via `Arc`. Constructing a fresh `CoreClient` per request is an
/// anti-pattern; construct once and clone as needed.
///
/// # Example
///
/// ```no_run
/// use allsource::{CoreClient, IngestEventInput};
/// use serde_json::json;
///
/// # async fn run() -> Result<(), allsource::Error> {
/// let core = CoreClient::new("http://localhost:3900", "ask_...")?;
/// core.ingest_event(IngestEventInput::new(
///     "order.placed",
///     "order-42",
///     json!({"total_usd": 19.99}),
/// )).await?;
///
/// // Compare-and-swap: only append if the entity is still at version 3.
/// // A mismatch fails with `Error::VersionConflict { expected, current }`.
/// core.ingest_event(
///     IngestEventInput::new("order.shipped", "order-42", json!({}))
///         .with_expected_version(3),
/// ).await?;
/// # Ok(()) }
/// ```
#[derive(Debug, Clone)]
pub struct CoreClient {
    transport: Arc<HttpTransport>,
}

impl CoreClient {
    /// Create a new Core client.
    pub fn new(base_url: &str, api_key: &str) -> Result<Self, Error> {
        Self::with_config(ClientConfig::new(base_url, api_key))
    }

    /// Create with full configuration.
    #[allow(clippy::needless_pass_by_value)] // public API — taking ownership is idiomatic
    pub fn with_config(config: ClientConfig) -> Result<Self, Error> {
        Ok(Self {
            transport: Arc::new(HttpTransport::new(&config)?),
        })
    }

    /// The circuit breaker's current state.
    pub fn circuit_state(&self) -> CircuitState {
        self.transport.circuit_state()
    }

    /// Shared transport handle, for in-crate modules that add more HTTP methods.
    pub(crate) fn transport(&self) -> &HttpTransport {
        &self.transport
    }

    pub(crate) fn http(&self) -> &Arc<HttpTransport> {
        &self.transport
    }

    /// Ingest a single event.
    ///
    /// Event types are automatically normalized to AllSource's `lowercase.dot.separated`
    /// convention. For example, `VerificationCreated` becomes `verification.created`.
    pub async fn ingest_event(&self, mut input: IngestEventInput) -> Result<IngestResponse, Error> {
        input.event_type = normalize_event_type(&input.event_type);
        self.transport.post("/api/v1/events", &input).await
    }

    /// Ingest a batch of events.
    ///
    /// Event types are automatically normalized to AllSource's `lowercase.dot.separated`
    /// convention. For example, `VerificationCreated` becomes `verification.created`.
    ///
    /// [`IngestEventInput::expected_version`] is honoured **per item** — Core
    /// reads the field off each event, not off the request — so a batch can mix
    /// compare-and-swap and unconditional writes. Note that items are applied in
    /// order and a conflict fails that item, so a partially-applied batch is
    /// possible; do not treat a batch as atomic.
    pub async fn ingest_batch(
        &self,
        mut events: Vec<IngestEventInput>,
    ) -> Result<BatchIngestResponse, Error> {
        for event in &mut events {
            event.event_type = normalize_event_type(&event.event_type);
        }
        #[derive(Serialize)]
        struct BatchRequest {
            events: Vec<IngestEventInput>,
        }
        self.transport
            .post("/api/v1/events/batch", &BatchRequest { events })
            .await
    }

    /// Check Core health.
    pub async fn health(&self) -> Result<HealthResponse, Error> {
        self.transport.get("/health").await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_validation() {
        assert!(QueryClient::new("", "key").is_err());
        assert!(QueryClient::new("http://localhost:3902", "").is_err());
        assert!(QueryClient::new("http://localhost:3902", "test-key").is_ok());
    }

    #[test]
    fn test_core_client_config_validation() {
        assert!(CoreClient::new("", "key").is_err());
        assert!(CoreClient::new("http://localhost:3900", "").is_err());
        assert!(CoreClient::new("http://localhost:3900", "test-key").is_ok());
    }

    #[test]
    fn test_base_url_trailing_slash() {
        let client = QueryClient::new("http://localhost:3902/", "key").unwrap();
        assert_eq!(client.transport.base_url, "http://localhost:3902");
    }

    #[test]
    fn test_retry_config_defaults() {
        let cfg = RetryConfig::default();
        assert_eq!(cfg.max_retries, 3);
        assert_eq!(cfg.base_delay, Duration::from_millis(200));
        assert_eq!(cfg.backoff_factor, 2.0);
    }

    mod prime {
        use super::*;
        use serde_json::json;
        use wiremock::{
            matchers::{method, path},
            Mock, MockServer, ResponseTemplate,
        };

        fn make_client(server: &MockServer) -> QueryClient {
            QueryClient::new(&server.uri(), "test-key").unwrap()
        }

        #[tokio::test]
        async fn list_prime_projections_unwraps_data() {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/api/v1/prime/projections"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "data": [
                        {
                            "entity_type": "contact",
                            "field_policies": {
                                "name": "last_write",
                                "tags": "merge_array"
                            }
                        }
                    ],
                    "count": 1
                })))
                .mount(&server)
                .await;

            let client = make_client(&server);
            let projections = client.list_prime_projections().await.unwrap();
            assert_eq!(projections.len(), 1);
            assert_eq!(projections[0].entity_type, "contact");
            assert_eq!(
                projections[0]
                    .field_policies
                    .get("name")
                    .map(String::as_str),
                Some("last_write")
            );
            assert_eq!(
                projections[0]
                    .field_policies
                    .get("tags")
                    .map(String::as_str),
                Some("merge_array")
            );
        }

        #[tokio::test]
        async fn define_prime_projection_returns_ack() {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/api/v1/prime/projections"))
                .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                    "data": { "entity_type": "contact", "persisted": true }
                })))
                .mount(&server)
                .await;

            let client = make_client(&server);
            let mut policies = std::collections::BTreeMap::new();
            policies.insert("name".to_string(), "last_write".to_string());
            let ack = client
                .define_prime_projection("contact", policies)
                .await
                .unwrap();
            assert_eq!(ack.entity_type, "contact");
            assert!(ack.persisted);
        }

        #[tokio::test]
        async fn project_node_returns_snapshot() {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/api/v1/prime/nodes/node:contact:1/project"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "data": {
                        "entity_type": "contact",
                        "fields": { "name": "Ada", "email": "ada@example.com" },
                        "observation_count": 3
                    }
                })))
                .mount(&server)
                .await;

            let client = make_client(&server);
            let snapshot = client.project_node("node:contact:1").await.unwrap();
            assert_eq!(snapshot.entity_type, "contact");
            assert_eq!(snapshot.observation_count, 3);
            assert_eq!(
                snapshot.fields.get("name").and_then(|v| v.as_str()),
                Some("Ada")
            );
        }

        #[tokio::test]
        async fn node_field_provenance_returns_some() {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path(
                    "/api/v1/prime/nodes/node:contact:1/fields/name/provenance",
                ))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "data": {
                        "field": "name",
                        "value": "Ada",
                        "source_event_id": "evt-7",
                        "source_event_at": "2026-05-29T12:00:00Z",
                        "merge_policy_applied": "last_write"
                    }
                })))
                .mount(&server)
                .await;

            let client = make_client(&server);
            let prov = client
                .node_field_provenance("node:contact:1", "name")
                .await
                .unwrap()
                .expect("provenance should be present");
            assert_eq!(prov.field, "name");
            assert_eq!(prov.value.as_str(), Some("Ada"));
            assert_eq!(prov.source_event_id, "evt-7");
            assert_eq!(prov.merge_policy_applied, "last_write");
        }

        #[tokio::test]
        async fn node_field_provenance_404_returns_none() {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path(
                    "/api/v1/prime/nodes/node:contact:1/fields/missing/provenance",
                ))
                .respond_with(ResponseTemplate::new(404).set_body_json(json!({
                    "error": "not found"
                })))
                .mount(&server)
                .await;

            let client = make_client(&server);
            let prov = client
                .node_field_provenance("node:contact:1", "missing")
                .await
                .unwrap();
            assert!(prov.is_none());
        }
    }
}
