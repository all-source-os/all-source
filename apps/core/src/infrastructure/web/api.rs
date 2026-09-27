#[cfg(feature = "replication")]
use crate::infrastructure::replication::ReplicationMode;
use crate::{
    application::{
        dto::{
            AckRequest, ConsumerEventDto, ConsumerEventsResponse, ConsumerResponse,
            DetectDuplicatesRequest, DetectDuplicatesResponse, DuplicateGroup, EntitySummary,
            EventDto, IngestEventRequest, IngestEventResponse, IngestEventsBatchRequest,
            IngestEventsBatchResponse, ListEntitiesRequest, ListEntitiesResponse,
            QueryEventsRequest, QueryEventsResponse, RegisterConsumerRequest,
        },
        services::{
            analytics::{
                AnalyticsEngine, CorrelationRequest, CorrelationResponse, EventFrequencyRequest,
                EventFrequencyResponse, StatsSummaryRequest, StatsSummaryResponse,
            },
            pipeline::{PipelineConfig, PipelineStats},
            replay::{ReplayProgress, StartReplayRequest, StartReplayResponse},
            schema::{
                CompatibilityMode, RegisterSchemaRequest, RegisterSchemaResponse,
                ValidateEventRequest, ValidateEventResponse,
            },
            webhook::{RegisterWebhookRequest, UpdateWebhookRequest},
        },
    },
    domain::{
        entities::{Event, SchemaEnforcement},
        value_objects::TenantId,
    },
    error::Result,
    infrastructure::{
        persistence::{
            compaction::CompactionResult,
            snapshot::{
                CreateSnapshotRequest, CreateSnapshotResponse, ListSnapshotsRequest,
                ListSnapshotsResponse, SnapshotInfo,
            },
        },
        query::{
            geospatial::GeoQueryRequest,
            graphql::{GraphQLError, GraphQLRequest, GraphQLResponse},
        },
        security::middleware::OptionalAuth,
        web::api_v1::AppState,
    },
    store::{EventStore, EventTypeInfo, StreamInfo},
};
use axum::{
    Json, Router,
    extract::{Path, Query, State, WebSocketUpgrade},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use serde::Deserialize;
use std::sync::Arc;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};

type SharedStore = Arc<EventStore>;

/// Wait for follower ACK(s) in semi-sync/sync replication modes.
///
/// In async mode (default), returns immediately. In semi-sync mode, waits for
/// at least 1 follower to ACK the current WAL offset. In sync mode, waits for
/// all followers. If the timeout expires, logs a warning and continues (degraded mode).
#[cfg(feature = "replication")]
async fn await_replication_ack(state: &AppState) {
    let shipper_guard = state.wal_shipper.read().await;
    if let Some(ref shipper) = *shipper_guard {
        let mode = shipper.replication_mode();
        if mode == ReplicationMode::Async {
            return;
        }

        let target_offset = shipper.current_leader_offset();
        if target_offset == 0 {
            return;
        }

        let shipper = Arc::clone(shipper);
        // Drop the read guard before the async wait to avoid holding it across await
        drop(shipper_guard);

        let timer = state
            .store
            .metrics()
            .replication_ack_wait_seconds
            .start_timer();
        let acked = shipper.wait_for_ack(target_offset).await;
        timer.observe_duration();

        if !acked {
            tracing::warn!(
                "Replication ACK timeout in {} mode (offset {}). \
                 Write succeeded locally but follower confirmation pending.",
                mode,
                target_offset,
            );
        }
    }
}
#[cfg(not(feature = "replication"))]
async fn await_replication_ack(_state: &AppState) {
    // No-op in community edition
}

pub async fn serve(store: SharedStore, addr: &str) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/health", get(health))
        .route("/metrics", get(prometheus_metrics)) // v0.6: Prometheus metrics endpoint
        .route("/api/v1/events", post(ingest_event))
        .route("/api/v1/events/batch", post(ingest_events_batch))
        .route("/api/v1/events/query", get(query_events))
        .route("/api/v1/events/{event_id}", get(get_event_by_id))
        .route("/api/v1/events/stream", get(events_websocket)) // v0.2: WebSocket streaming
        // v0.10: Stream and event type discovery endpoints
        .route("/api/v1/streams", get(list_streams))
        .route("/api/v1/event-types", get(list_event_types))
        .route("/api/v1/entities/duplicates", get(detect_duplicates))
        .route("/api/v1/entities/{entity_id}/state", get(get_entity_state))
        .route(
            "/api/v1/entities/{entity_id}/snapshot",
            get(get_entity_snapshot),
        )
        .route("/api/v1/stats", get(get_stats))
        // v0.2: Advanced analytics endpoints
        .route("/api/v1/analytics/frequency", get(analytics_frequency))
        .route("/api/v1/analytics/summary", get(analytics_summary))
        .route("/api/v1/analytics/correlation", get(analytics_correlation))
        // v0.2: Snapshot management endpoints
        .route("/api/v1/snapshots", post(create_snapshot))
        .route("/api/v1/snapshots", get(list_snapshots))
        .route(
            "/api/v1/snapshots/{entity_id}/latest",
            get(get_latest_snapshot),
        )
        // v0.2: Compaction endpoints
        .route("/api/v1/compaction/trigger", post(trigger_compaction))
        .route("/api/v1/compaction/stats", get(compaction_stats))
        // v0.5: Schema registry endpoints
        .route("/api/v1/schemas", post(register_schema))
        .route("/api/v1/schemas", get(list_subjects))
        .route("/api/v1/schemas/{subject}", get(get_schema))
        .route(
            "/api/v1/schemas/{subject}/versions",
            get(list_schema_versions),
        )
        .route("/api/v1/schemas/validate", post(validate_event_schema))
        .route(
            "/api/v1/schemas/{subject}/compatibility",
            put(set_compatibility_mode),
        )
        // v0.5: Replay and projection rebuild endpoints
        .route("/api/v1/replay", post(start_replay))
        .route("/api/v1/replay", get(list_replays))
        .route("/api/v1/replay/{replay_id}", get(get_replay_progress))
        .route("/api/v1/replay/{replay_id}/cancel", post(cancel_replay))
        .route(
            "/api/v1/replay/{replay_id}",
            axum::routing::delete(delete_replay),
        )
        // v0.5: Stream processing pipeline endpoints
        .route("/api/v1/pipelines", post(register_pipeline))
        .route("/api/v1/pipelines", get(list_pipelines))
        .route("/api/v1/pipelines/stats", get(all_pipeline_stats))
        .route("/api/v1/pipelines/{pipeline_id}", get(get_pipeline))
        .route(
            "/api/v1/pipelines/{pipeline_id}",
            axum::routing::delete(remove_pipeline),
        )
        .route(
            "/api/v1/pipelines/{pipeline_id}/stats",
            get(get_pipeline_stats),
        )
        .route("/api/v1/pipelines/{pipeline_id}/reset", put(reset_pipeline))
        // v0.7: Projection State API for Query Service integration
        .route("/api/v1/projections", get(list_projections))
        .route("/api/v1/projections/{name}", get(get_projection))
        .route(
            "/api/v1/projections/{name}",
            axum::routing::delete(delete_projection),
        )
        .route(
            "/api/v1/projections/{name}/state",
            get(get_projection_state_summary),
        )
        .route("/api/v1/projections/{name}/reset", post(reset_projection))
        .route(
            "/api/v1/projections/{name}/{entity_id}/state",
            get(get_projection_state),
        )
        .route(
            "/api/v1/projections/{name}/{entity_id}/state",
            post(save_projection_state),
        )
        .route(
            "/api/v1/projections/{name}/{entity_id}/state",
            put(save_projection_state),
        )
        .route(
            "/api/v1/projections/{name}/bulk",
            post(bulk_get_projection_states),
        )
        .route(
            "/api/v1/projections/{name}/bulk/save",
            post(bulk_save_projection_states),
        )
        // v0.11: Webhook management endpoints
        .route("/api/v1/webhooks", post(register_webhook))
        .route("/api/v1/webhooks", get(list_webhooks))
        .route("/api/v1/webhooks/{webhook_id}", get(get_webhook))
        .route("/api/v1/webhooks/{webhook_id}", put(update_webhook))
        .route(
            "/api/v1/webhooks/{webhook_id}",
            axum::routing::delete(delete_webhook),
        )
        .route(
            "/api/v1/webhooks/{webhook_id}/deliveries",
            get(list_webhook_deliveries),
        )
        // v2.0: Advanced query features
        .route("/api/v1/graphql", post(graphql_query))
        .route("/api/v1/geospatial/query", post(geo_query))
        .route("/api/v1/geospatial/stats", get(geo_stats))
        .route("/api/v1/exactly-once/stats", get(exactly_once_stats))
        .route(
            "/api/v1/schema-evolution/history/{event_type}",
            get(schema_evolution_history),
        )
        .route(
            "/api/v1/schema-evolution/schema/{event_type}",
            get(schema_evolution_schema),
        )
        .route(
            "/api/v1/schema-evolution/stats",
            get(schema_evolution_stats),
        )
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(store);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

pub async fn health() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "healthy",
        "service": "allsource-core",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

// v0.6: Prometheus metrics endpoint
pub async fn prometheus_metrics(State(store): State<SharedStore>) -> impl IntoResponse {
    let metrics = store.metrics();

    match metrics.encode() {
        Ok(encoded) => Response::builder()
            .status(200)
            .header("Content-Type", "text/plain; version=0.0.4")
            .body(encoded)
            .unwrap()
            .into_response(),
        Err(e) => Response::builder()
            .status(500)
            .body(format!("Error encoding metrics: {e}"))
            .unwrap()
            .into_response(),
    }
}

pub async fn ingest_event(
    State(store): State<SharedStore>,
    Json(req): Json<IngestEventRequest>,
) -> Result<Json<IngestEventResponse>> {
    let expected_version = req.expected_version;

    let tenant_id = req.tenant_id.unwrap_or_else(|| "default".to_string());
    let event = Event::from_strings(
        req.event_type,
        req.entity_id,
        tenant_id,
        req.payload,
        req.metadata,
    )?;

    let event_id = event.id;
    let timestamp = event.timestamp;

    let new_version =
        super::archive_work::append(Arc::clone(&store), event, expected_version).await?;

    tracing::info!("Event ingested: {}", event_id);

    Ok(Json(IngestEventResponse {
        event_id,
        timestamp,
        version: Some(new_version),
    }))
}

/// Ingest a single event with semi-sync/sync replication ACK waiting.
///
/// Used by the v1 API. Tenant comes from `req.tenant_id` — the Control Plane
/// delegation layer sets it from the authenticated caller before forwarding.
/// Core is internal-only and does not authenticate public traffic.
/// Look up the tenant's `SchemaEnforcement` mode and, if non-permissive,
/// validate the event payload against any registered schema for the
/// event_type.
///
/// Fast path: `Permissive` (default for unconfigured tenants AND for tenants
/// not present in the repo, like dev/test setups) returns `Ok(())` without
/// touching the schema registry — preserves the pre-v0.21.5 ingest cost.
///
/// `Warn`: validation runs, violations log at WARN, the write proceeds.
/// `Strict`: violations return `AllSourceError::SchemaViolation` → 422 with
/// the structured body the HTTP layer builds in `error.rs`.
///
/// If no schema is registered for the event_type, validation is a no-op
/// regardless of mode — this matches the principle that schemas are
/// opt-in per event_type, not per tenant.
async fn enforce_schema_if_configured(
    state: &AppState,
    tenant_id: &str,
    event: &Event,
) -> Result<()> {
    // Cheapest possible lookup: parse the tenant_id; if it fails, treat as
    // permissive (defensive — Event::from_strings already validated, but
    // this keeps the contract clear).
    let Ok(parsed) = TenantId::new(tenant_id.to_string()) else {
        return Ok(());
    };
    let mode = match state.tenant_repo.find_by_id(&parsed).await {
        Ok(Some(t)) => t.schema_enforcement(),
        // Unknown tenant → permissive. Avoids breaking the default tenant
        // and any dev setups that ingest without pre-registering tenants.
        _ => SchemaEnforcement::Permissive,
    };
    if matches!(mode, SchemaEnforcement::Permissive) {
        return Ok(());
    }

    // Schema lookup keyed by event_type. Latest version only (None) — schema
    // evolution is a separate concern; tenants pin a version via their own
    // registration flow if they need to.
    let registry = state.store.schema_registry();
    let Ok(schema) = registry.get_schema(event.event_type.as_str(), None) else {
        // No schema registered for this event_type → fast path, regardless
        // of enforcement mode. Schemas are opt-in.
        return Ok(());
    };

    let result = registry
        .validate(
            event.event_type.as_str(),
            Some(schema.version),
            &event.payload,
        )
        .map_err(|e| crate::error::AllSourceError::InternalError(e.to_string()))?;

    if result.valid {
        return Ok(());
    }

    match mode {
        SchemaEnforcement::Strict => Err(crate::error::AllSourceError::SchemaViolation {
            event_type: event.event_type.as_str().to_string(),
            schema_version: result.schema_version,
            errors: result.errors,
        }),
        SchemaEnforcement::Warn => {
            tracing::warn!(
                tenant = %tenant_id,
                event_type = %event.event_type.as_str(),
                schema_version = result.schema_version,
                errors = ?result.errors,
                "schema violation (warn mode — write accepted)"
            );
            Ok(())
        }
        // Already handled above
        SchemaEnforcement::Permissive => Ok(()),
    }
}

pub async fn ingest_event_v1(
    State(state): State<AppState>,
    Json(req): Json<IngestEventRequest>,
) -> Result<Json<IngestEventResponse>> {
    let expected_version = req.expected_version;

    let tenant_id = req.tenant_id.unwrap_or_else(|| "default".to_string());

    let event = Event::from_strings(
        req.event_type,
        req.entity_id,
        tenant_id.clone(),
        req.payload,
        req.metadata,
    )?;

    // Per-tenant schema enforcement (Permissive is the fast path — no
    // tenant lookup, no schema query). See neotoma-gaps bead t-0795.
    enforce_schema_if_configured(&state, &tenant_id, &event).await?;

    let event_id = event.id;
    let timestamp = event.timestamp;

    let new_version =
        super::archive_work::append(Arc::clone(&state.store), event, expected_version).await?;

    // Semi-sync/sync: wait for follower ACK(s) before returning
    await_replication_ack(&state).await;

    tracing::info!("Event ingested: {}", event_id);

    Ok(Json(IngestEventResponse {
        event_id,
        timestamp,
        version: Some(new_version),
    }))
}

/// Batch ingest multiple events in a single request
///
/// This endpoint allows ingesting multiple events atomically, which is more
/// efficient than making individual requests for each event.
pub async fn ingest_events_batch(
    State(store): State<SharedStore>,
    Json(req): Json<IngestEventsBatchRequest>,
) -> Result<Json<IngestEventsBatchResponse>> {
    let total = req.events.len();
    let mut ingested_events = Vec::with_capacity(total);

    for event_req in req.events {
        let tenant_id = event_req.tenant_id.unwrap_or_else(|| "default".to_string());
        let expected_version = event_req.expected_version;

        let event = Event::from_strings(
            event_req.event_type,
            event_req.entity_id,
            tenant_id,
            event_req.payload,
            event_req.metadata,
        )?;

        // Note: the non-v1 batch path uses State<SharedStore>, not AppState,
        // so it has no tenant_repo to check enforcement against. Schema
        // enforcement is wired through the v1 batch handler below — this
        // path remains permissive (it predates the tenant_repo wiring).

        let event_id = event.id;
        let timestamp = event.timestamp;

        let new_version =
            super::archive_work::append(Arc::clone(&store), event, expected_version).await?;

        ingested_events.push(IngestEventResponse {
            event_id,
            timestamp,
            version: Some(new_version),
        });
    }

    let ingested = ingested_events.len();
    tracing::info!("Batch ingested {} events", ingested);

    Ok(Json(IngestEventsBatchResponse {
        total,
        ingested,
        events: ingested_events,
    }))
}

/// Batch ingest with semi-sync/sync replication ACK waiting.
///
/// Used by the v1 API. Per-event tenant comes from `event_req.tenant_id` — the
/// Control Plane delegation layer sets it from the authenticated caller before
/// forwarding. Core is internal-only and does not authenticate public traffic.
pub async fn ingest_events_batch_v1(
    State(state): State<AppState>,
    Json(req): Json<IngestEventsBatchRequest>,
) -> Result<Json<IngestEventsBatchResponse>> {
    let total = req.events.len();
    let mut ingested_events = Vec::with_capacity(total);

    for event_req in req.events {
        let tenant_id = event_req.tenant_id.unwrap_or_else(|| "default".to_string());
        let expected_version = event_req.expected_version;

        let event = Event::from_strings(
            event_req.event_type,
            event_req.entity_id,
            tenant_id.clone(),
            event_req.payload,
            event_req.metadata,
        )?;

        enforce_schema_if_configured(&state, &tenant_id, &event).await?;

        let event_id = event.id;
        let timestamp = event.timestamp;

        let new_version =
            super::archive_work::append(Arc::clone(&state.store), event, expected_version).await?;

        ingested_events.push(IngestEventResponse {
            event_id,
            timestamp,
            version: Some(new_version),
        });
    }

    // Semi-sync/sync: wait for follower ACK(s) after all events are ingested
    await_replication_ack(&state).await;

    let ingested = ingested_events.len();
    tracing::info!("Batch ingested {} events", ingested);

    Ok(Json(IngestEventsBatchResponse {
        total,
        ingested,
        events: ingested_events,
    }))
}

/// Sort-order query parameter for `GET /api/v1/events/query`. Kept separate
/// from `QueryEventsRequest` so ordering is purely an HTTP-layer concern and
/// the internal query DTO (used by ~50 call sites) stays untouched.
#[derive(Debug, Deserialize)]
pub struct EventOrderParam {
    /// `asc` (oldest first, the default) or `desc` (newest first).
    pub order: Option<String>,
}

/// Pagination offset for `GET /api/v1/events/query`. Separate from
/// `QueryEventsRequest` for the same reason as `EventOrderParam`: offset is an
/// HTTP-layer windowing concern, not part of the internal query predicate that
/// `store.query()` and its ~50 call sites evaluate.
#[derive(Debug, Deserialize)]
pub struct EventOffsetParam {
    /// Number of matching events to skip before applying `limit`. Default 0.
    pub offset: Option<usize>,
}

/// Opt-in integrity protocol. Omission preserves the legacy tolerant query.
#[derive(Debug, Default, Deserialize)]
pub struct EventIntegrityParam {
    pub integrity: Option<String>,
}

pub async fn query_events(
    OptionalAuth(auth): OptionalAuth,
    Query(req): Query<QueryEventsRequest>,
    Query(order_param): Query<EventOrderParam>,
    Query(offset_param): Query<EventOffsetParam>,
    Query(integrity_param): Query<EventIntegrityParam>,
    State(store): State<SharedStore>,
) -> Result<Json<QueryEventsResponse>> {
    let offset = offset_param.offset.unwrap_or(0);
    let queried_entity_id = req.entity_id.clone();

    // Sort order. Default is ascending (oldest first) to preserve replay
    // semantics for existing consumers. `order=desc` returns newest first,
    // so `?entity_id=<id>&limit=1&order=desc` reliably yields the latest
    // event for an entity (issue #177).
    let descending = match order_param.order.as_deref() {
        None => false,
        Some(o) if o.eq_ignore_ascii_case("asc") => false,
        Some(o) if o.eq_ignore_ascii_case("desc") => true,
        Some(other) => {
            return Err(crate::error::AllSourceError::InvalidInput(format!(
                "invalid 'order' value '{other}': expected 'asc' or 'desc'"
            )));
        }
    };

    // Tenant resolution. Since Core is internal-only (bead t-0ff8), the only
    // callers are Control Plane's delegation layer and other internal Fly
    // services. Request param wins — that's what the gateway sets authoritatively
    // from the authenticated caller's identity. Auth-context fallback is kept
    // as a defense-in-depth for any internal caller that forgets to pass
    // tenant_id but is authenticated (legacy; audit and remove once all
    // internal callers are confirmed to set tenant_id explicitly).
    let enforced_tenant = req
        .tenant_id
        .clone()
        .or_else(|| auth.as_ref().map(|a| a.tenant_id().to_string()));

    if let Some(protocol) = integrity_param.integrity {
        return super::retained_query::query(
            store,
            QueryEventsRequest {
                tenant_id: enforced_tenant,
                ..req
            },
            offset,
            descending,
            &protocol,
        )
        .await
        .map(Json);
    }

    // FAIL CLOSED (tenant isolation): the public events query must NEVER return
    // cross-tenant results. The gateway always injects an auth-derived tenant_id
    // (and overwrites any client-supplied one); if neither a request tenant nor
    // an auth tenant is present we return an empty result rather than scanning
    // across tenants. A genuine cross-tenant/admin scan is a separate, explicit
    // internal path — it does not ride this endpoint.
    if enforced_tenant.as_deref().unwrap_or("").is_empty() {
        return Ok(Json(QueryEventsResponse {
            events: Vec::new(),
            count: 0,
            total_count: 0,
            has_more: false,
            entity_version: None,
            archive_integrity: None,
        }));
    }

    // One windowed pass: the store sorts borrowed matches, applies `offset` and
    // `limit`, and clones only the page — while still reporting the pre-window
    // match count for `total_count`/`has_more`. Asking for the total used to
    // mean a second, unlimited query that cloned the whole history, so
    // `?entity_id=X&limit=1&order=desc` cost as much as fetching everything
    // (issue #251). Offset is applied before limit so `offset=N&limit=N` walks
    // pages instead of returning page one forever (issue #250).
    let scoped_req = QueryEventsRequest {
        tenant_id: enforced_tenant,
        ..req
    };
    let (limited_events, total_count) = store.query_window(&scoped_req, offset, descending)?;

    let count = limited_events.len();
    // `has_more` is relative to the window actually served, not to the page
    // size — a paginator that trusts a bare `count < total_count` never
    // terminates once an offset is in play.
    let has_more = offset + count < total_count;
    let events: Vec<EventDto> = limited_events.iter().map(EventDto::from).collect();

    // Include entity_version only when filtering by a single entity_id
    let entity_version = queried_entity_id
        .as_deref()
        .map(|eid| store.get_entity_version(eid));

    tracing::debug!("Query returned {} events (total: {})", count, total_count);

    Ok(Json(QueryEventsResponse {
        events,
        count,
        total_count,
        has_more,
        entity_version,
        archive_integrity: None,
    }))
}

pub async fn list_entities(
    State(store): State<SharedStore>,
    Query(req): Query<ListEntitiesRequest>,
) -> Result<Json<ListEntitiesResponse>> {
    use std::collections::HashMap;

    // Get all events matching the filters
    let query_req = QueryEventsRequest {
        entity_id: None,
        event_type: None,
        tenant_id: None,
        as_of: None,
        since: None,
        until: None,
        limit: None,
        event_type_prefix: req.event_type_prefix,
        exclude_event_type_prefix: None,
        payload_filter: req.payload_filter,
    };
    let events = store.query(&query_req)?;

    // Group by entity_id
    let mut entity_map: HashMap<String, Vec<&Event>> = HashMap::new();
    for event in &events {
        entity_map
            .entry(event.entity_id().to_string())
            .or_default()
            .push(event);
    }

    // Sort direction by last-event time. Default is `desc` (newest activity
    // first) to preserve the endpoint's long-standing behavior; `order=asc`
    // returns oldest activity first (issue #178).
    let ascending = match req.order.as_deref() {
        None => false,
        Some(o) if o.eq_ignore_ascii_case("desc") => false,
        Some(o) if o.eq_ignore_ascii_case("asc") => true,
        Some(other) => {
            return Err(crate::error::AllSourceError::InvalidInput(format!(
                "invalid 'order' value '{other}': expected 'asc' or 'desc'"
            )));
        }
    };

    // Build entity summaries, then sort by last-event time in the requested
    // direction. Ties are broken by `entity_id` (always ascending) so the
    // total order is deterministic — required for stable offset pagination.
    let mut summaries: Vec<EntitySummary> = entity_map
        .into_iter()
        .map(|(entity_id, events)| {
            let last = events.iter().max_by_key(|e| e.timestamp()).unwrap();
            EntitySummary {
                entity_id,
                event_count: events.len(),
                last_event_type: last.event_type_str().to_string(),
                last_event_at: last.timestamp(),
            }
        })
        .collect();
    summaries.sort_by(|a, b| {
        let by_time = a.last_event_at.cmp(&b.last_event_at);
        let by_time = if ascending {
            by_time
        } else {
            by_time.reverse()
        };
        by_time.then_with(|| a.entity_id.cmp(&b.entity_id))
    });

    let total = summaries.len();

    // Apply offset and limit
    let offset = req.offset.unwrap_or(0);
    let summaries: Vec<EntitySummary> = summaries.into_iter().skip(offset).collect::<Vec<_>>();
    let summaries = if let Some(limit) = req.limit {
        let has_more = summaries.len() > limit;
        let truncated: Vec<EntitySummary> = summaries.into_iter().take(limit).collect();
        return Ok(Json(ListEntitiesResponse {
            entities: truncated,
            total,
            has_more,
        }));
    } else {
        summaries
    };

    Ok(Json(ListEntitiesResponse {
        entities: summaries,
        total,
        has_more: false,
    }))
}

pub async fn detect_duplicates(
    State(store): State<SharedStore>,
    Query(req): Query<DetectDuplicatesRequest>,
) -> Result<Json<DetectDuplicatesResponse>> {
    use std::collections::HashMap;

    let group_by_fields: Vec<&str> = req.group_by.split(',').map(str::trim).collect();

    // Query events scoped by the required prefix
    let query_req = QueryEventsRequest {
        entity_id: None,
        event_type: None,
        tenant_id: None,
        as_of: None,
        since: None,
        until: None,
        limit: None,
        event_type_prefix: Some(req.event_type_prefix),
        exclude_event_type_prefix: None,
        payload_filter: None,
    };
    let events = store.query(&query_req)?;

    // For each entity, extract the latest event's payload fields specified by group_by
    // Then group entities by those field values
    let mut entity_latest: HashMap<String, &Event> = HashMap::new();
    for event in &events {
        let eid = event.entity_id().to_string();
        entity_latest
            .entry(eid)
            .and_modify(|existing| {
                if event.timestamp() > existing.timestamp() {
                    *existing = event;
                }
            })
            .or_insert(event);
    }

    // Group entities by their payload field values
    let mut groups: HashMap<String, Vec<String>> = HashMap::new();
    for (entity_id, event) in &entity_latest {
        let payload = event.payload();
        let mut key_parts = serde_json::Map::new();
        for field in &group_by_fields {
            let value = payload
                .get(*field)
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            key_parts.insert((*field).to_string(), value);
        }
        let key_str = serde_json::to_string(&key_parts).unwrap_or_default();
        groups.entry(key_str).or_default().push(entity_id.clone());
    }

    // Filter to groups with count > 1 (actual duplicates)
    let mut duplicate_groups: Vec<DuplicateGroup> = groups
        .into_iter()
        .filter(|(_, ids)| ids.len() > 1)
        .map(|(key_str, mut ids)| {
            ids.sort();
            let key: serde_json::Value =
                serde_json::from_str(&key_str).unwrap_or(serde_json::Value::Null);
            let count = ids.len();
            DuplicateGroup {
                key,
                entity_ids: ids,
                count,
            }
        })
        .collect();

    // Sort by count descending for consistent output
    duplicate_groups.sort_by_key(|a| std::cmp::Reverse(a.count));

    let total = duplicate_groups.len();

    // Apply offset and limit
    let offset = req.offset.unwrap_or(0);
    let duplicate_groups: Vec<DuplicateGroup> = duplicate_groups.into_iter().skip(offset).collect();

    if let Some(limit) = req.limit {
        let has_more = duplicate_groups.len() > limit;
        let truncated: Vec<DuplicateGroup> = duplicate_groups.into_iter().take(limit).collect();
        return Ok(Json(DetectDuplicatesResponse {
            duplicates: truncated,
            total,
            has_more,
        }));
    }

    Ok(Json(DetectDuplicatesResponse {
        duplicates: duplicate_groups,
        total,
        has_more: false,
    }))
}

#[derive(Deserialize)]
pub struct EntityStateParams {
    as_of: Option<chrono::DateTime<chrono::Utc>>,
    /// Tenant to scope to (the gateway injects the authenticated tenant).
    ///
    /// When present, reconstruction folds only that tenant's events and skips
    /// the snapshot fast path, which carries no tenant dimension (#230).
    tenant_id: Option<String>,
}

pub async fn get_entity_state(
    State(store): State<SharedStore>,
    Path(entity_id): Path<String>,
    Query(params): Query<EntityStateParams>,
) -> Result<Json<serde_json::Value>> {
    let state = match params.tenant_id.as_deref() {
        Some(tenant_id) => {
            store.reconstruct_state_for_tenant(&entity_id, params.as_of, tenant_id)?
        }
        None => store.reconstruct_state(&entity_id, params.as_of)?,
    };

    tracing::info!("State reconstructed for entity: {}", entity_id);

    Ok(Json(state))
}

pub async fn get_entity_snapshot(
    State(store): State<SharedStore>,
    Path(entity_id): Path<String>,
    Query(params): Query<EntityStateParams>,
) -> Result<Json<serde_json::Value>> {
    // Snapshots are keyed by entity_id with no tenant dimension, so a scoped
    // caller cannot be served one safely — two tenants sharing an entity_id
    // would see each other's state. Fold that tenant's events instead (#230).
    let snapshot = match params.tenant_id.as_deref() {
        Some(tenant_id) => store.reconstruct_state_for_tenant(&entity_id, None, tenant_id)?,
        None => store.get_snapshot(&entity_id)?,
    };

    tracing::debug!("Snapshot retrieved for entity: {}", entity_id);

    Ok(Json(snapshot))
}

/// Query parameters for the stats endpoint.
#[derive(Debug, Deserialize)]
pub struct StatsParams {
    /// Tenant to scope to (the gateway injects the authenticated tenant).
    ///
    /// Absent = global, whole-store totals. That form is internal/admin only and
    /// must never be reachable by a tenant — the gateway routes its public
    /// `GET /api/v1/stats` through here with this parameter forced (#230).
    pub tenant_id: Option<String>,
}

pub async fn get_stats(
    State(store): State<SharedStore>,
    Query(params): Query<StatsParams>,
) -> impl IntoResponse {
    match params.tenant_id.as_deref() {
        Some(tenant_id) => {
            Json(serde_json::to_value(store.stats_for_tenant(tenant_id)).unwrap_or_default())
        }
        None => Json(serde_json::to_value(store.stats()).unwrap_or_default()),
    }
}

// v0.10: List all streams (entity_ids) in the event store
/// Query parameters for listing streams
#[derive(Debug, Deserialize)]
pub struct ListStreamsParams {
    /// Tenant to scope to (the gateway injects the authenticated tenant).
    pub tenant_id: Option<String>,
    /// Optional limit on number of streams to return
    pub limit: Option<usize>,
    /// Optional offset for pagination
    pub offset: Option<usize>,
}

/// Response for listing streams
#[derive(Debug, serde::Serialize)]
pub struct ListStreamsResponse {
    pub streams: Vec<StreamInfo>,
    pub total: usize,
}

pub async fn list_streams(
    OptionalAuth(auth): OptionalAuth,
    State(store): State<SharedStore>,
    Query(params): Query<ListStreamsParams>,
) -> Json<ListStreamsResponse> {
    // Tenant-scoped + fail closed: no tenant → empty, never a cross-tenant list.
    let tenant = params
        .tenant_id
        .clone()
        .or_else(|| auth.as_ref().map(|a| a.tenant_id().to_string()))
        .filter(|t| !t.is_empty());
    let Some(tenant) = tenant else {
        return Json(ListStreamsResponse {
            streams: vec![],
            total: 0,
        });
    };
    let mut streams = store.list_streams_for_tenant(&tenant);
    let total = streams.len();

    // Sort by last_event_at descending (most recent first)
    streams.sort_by_key(|a| std::cmp::Reverse(a.last_event_at));

    // Apply pagination
    if let Some(offset) = params.offset {
        if offset < streams.len() {
            streams = streams[offset..].to_vec();
        } else {
            streams = vec![];
        }
    }

    if let Some(limit) = params.limit {
        streams.truncate(limit);
    }

    tracing::debug!("Listed {} streams (total: {})", streams.len(), total);

    Json(ListStreamsResponse { streams, total })
}

// v0.10: List all event types in the event store
/// Query parameters for listing event types
#[derive(Debug, Deserialize)]
pub struct ListEventTypesParams {
    /// Tenant to scope to (the gateway injects the authenticated tenant).
    pub tenant_id: Option<String>,
    /// Optional limit on number of event types to return
    pub limit: Option<usize>,
    /// Optional offset for pagination
    pub offset: Option<usize>,
}

/// Response for listing event types
#[derive(Debug, serde::Serialize)]
pub struct ListEventTypesResponse {
    pub event_types: Vec<EventTypeInfo>,
    pub total: usize,
}

pub async fn list_event_types(
    OptionalAuth(auth): OptionalAuth,
    State(store): State<SharedStore>,
    Query(params): Query<ListEventTypesParams>,
) -> Json<ListEventTypesResponse> {
    // Tenant-scoped + fail closed: no tenant → empty, never cross-tenant types.
    let tenant = params
        .tenant_id
        .clone()
        .or_else(|| auth.as_ref().map(|a| a.tenant_id().to_string()))
        .filter(|t| !t.is_empty());
    let Some(tenant) = tenant else {
        return Json(ListEventTypesResponse {
            event_types: vec![],
            total: 0,
        });
    };
    let mut event_types = store.list_event_types_for_tenant(&tenant);
    let total = event_types.len();

    // Sort by event_count descending (most used first)
    event_types.sort_by_key(|a| std::cmp::Reverse(a.event_count));

    // Apply pagination
    if let Some(offset) = params.offset {
        if offset < event_types.len() {
            event_types = event_types[offset..].to_vec();
        } else {
            event_types = vec![];
        }
    }

    if let Some(limit) = params.limit {
        event_types.truncate(limit);
    }

    tracing::debug!(
        "Listed {} event types (total: {})",
        event_types.len(),
        total
    );

    Json(ListEventTypesResponse { event_types, total })
}

// v0.2: WebSocket endpoint for real-time event streaming
#[derive(Debug, Deserialize)]
pub struct WebSocketParams {
    pub consumer_id: Option<String>,
}

pub async fn events_websocket(
    ws: WebSocketUpgrade,
    State(store): State<SharedStore>,
    Query(params): Query<WebSocketParams>,
) -> Response {
    let websocket_manager = store.websocket_manager();

    ws.on_upgrade(move |socket| async move {
        if let Some(consumer_id) = params.consumer_id {
            websocket_manager
                .handle_socket_with_consumer(socket, consumer_id, store)
                .await;
        } else {
            websocket_manager.handle_socket(socket).await;
        }
    })
}

// v0.2: Event frequency analytics endpoint
pub async fn analytics_frequency(
    State(store): State<SharedStore>,
    Query(req): Query<EventFrequencyRequest>,
) -> Result<Json<EventFrequencyResponse>> {
    let response = AnalyticsEngine::event_frequency(&store, &req)?;

    tracing::debug!(
        "Frequency analysis returned {} buckets",
        response.buckets.len()
    );

    Ok(Json(response))
}

// v0.2: Statistical summary endpoint
pub async fn analytics_summary(
    State(store): State<SharedStore>,
    Query(req): Query<StatsSummaryRequest>,
) -> Result<Json<StatsSummaryResponse>> {
    let response = AnalyticsEngine::stats_summary(&store, &req)?;

    tracing::debug!(
        "Stats summary: {} events across {} entities",
        response.total_events,
        response.unique_entities
    );

    Ok(Json(response))
}

// v0.2: Event correlation analysis endpoint
pub async fn analytics_correlation(
    State(store): State<SharedStore>,
    Query(req): Query<CorrelationRequest>,
) -> Result<Json<CorrelationResponse>> {
    let response = AnalyticsEngine::analyze_correlation(&store, req)?;

    tracing::debug!(
        "Correlation analysis: {}/{} correlated pairs ({:.2}%)",
        response.correlated_pairs,
        response.total_a,
        response.correlation_percentage
    );

    Ok(Json(response))
}

// v0.2: Create a snapshot for an entity
pub async fn create_snapshot(
    State(store): State<SharedStore>,
    Json(req): Json<CreateSnapshotRequest>,
) -> Result<Json<CreateSnapshotResponse>> {
    store.create_snapshot(&req.entity_id)?;

    let snapshot_manager = store.snapshot_manager();
    let snapshot = snapshot_manager
        .get_latest_snapshot(&req.entity_id)
        .ok_or_else(|| crate::error::AllSourceError::EntityNotFound(req.entity_id.clone()))?;

    tracing::info!("📸 Created snapshot for entity: {}", req.entity_id);

    Ok(Json(CreateSnapshotResponse {
        snapshot_id: snapshot.id,
        entity_id: snapshot.entity_id,
        created_at: snapshot.created_at,
        event_count: snapshot.event_count,
        size_bytes: snapshot.metadata.size_bytes,
    }))
}

// v0.2: List snapshots
pub async fn list_snapshots(
    State(store): State<SharedStore>,
    Query(req): Query<ListSnapshotsRequest>,
) -> Result<Json<ListSnapshotsResponse>> {
    let snapshot_manager = store.snapshot_manager();

    let snapshots: Vec<SnapshotInfo> = if let Some(entity_id) = req.entity_id {
        snapshot_manager
            .get_all_snapshots(&entity_id)
            .into_iter()
            .map(SnapshotInfo::from)
            .collect()
    } else {
        // List all entities with snapshots
        let entities = snapshot_manager.list_entities();
        entities
            .iter()
            .flat_map(|entity_id| {
                snapshot_manager
                    .get_all_snapshots(entity_id)
                    .into_iter()
                    .map(SnapshotInfo::from)
            })
            .collect()
    };

    let total = snapshots.len();

    tracing::debug!("Listed {} snapshots", total);

    Ok(Json(ListSnapshotsResponse { snapshots, total }))
}

// v0.2: Get latest snapshot for an entity
pub async fn get_latest_snapshot(
    State(store): State<SharedStore>,
    Path(entity_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let snapshot_manager = store.snapshot_manager();

    let snapshot = snapshot_manager
        .get_latest_snapshot(&entity_id)
        .ok_or_else(|| crate::error::AllSourceError::EntityNotFound(entity_id.clone()))?;

    tracing::debug!("Retrieved latest snapshot for entity: {}", entity_id);

    Ok(Json(serde_json::json!({
        "snapshot_id": snapshot.id,
        "entity_id": snapshot.entity_id,
        "created_at": snapshot.created_at,
        "as_of": snapshot.as_of,
        "event_count": snapshot.event_count,
        "size_bytes": snapshot.metadata.size_bytes,
        "snapshot_type": snapshot.metadata.snapshot_type,
        "state": snapshot.state
    })))
}

// v0.2: Trigger manual compaction
pub async fn trigger_compaction(
    State(store): State<SharedStore>,
) -> Result<Json<CompactionResult>> {
    let compaction_manager = store.compaction_manager().ok_or_else(|| {
        crate::error::AllSourceError::InternalError(
            "Compaction not enabled (no Parquet storage)".to_string(),
        )
    })?;

    tracing::info!("📦 Manual compaction triggered via API");

    let result = compaction_manager.compact_now()?;

    Ok(Json(result))
}

// v0.2: Get compaction statistics
pub async fn compaction_stats(State(store): State<SharedStore>) -> Result<Json<serde_json::Value>> {
    let compaction_manager = store.compaction_manager().ok_or_else(|| {
        crate::error::AllSourceError::InternalError(
            "Compaction not enabled (no Parquet storage)".to_string(),
        )
    })?;

    let stats = compaction_manager.stats();
    let config = compaction_manager.config();

    Ok(Json(serde_json::json!({
        "stats": stats,
        "config": {
            "min_files_to_compact": config.min_files_to_compact,
            "target_file_size": config.target_file_size,
            "max_file_size": config.max_file_size,
            "small_file_threshold": config.small_file_threshold,
            "compaction_interval_seconds": config.compaction_interval_seconds,
            "auto_compact": config.auto_compact,
            "strategy": config.strategy
        }
    })))
}

// v0.5: Register a new schema
pub async fn register_schema(
    State(store): State<SharedStore>,
    Json(req): Json<RegisterSchemaRequest>,
) -> Result<Json<RegisterSchemaResponse>> {
    let schema_registry = store.schema_registry();

    let response =
        schema_registry.register_schema(req.subject, req.schema, req.description, req.tags)?;

    tracing::info!(
        "📋 Schema registered: v{} for '{}'",
        response.version,
        response.subject
    );

    Ok(Json(response))
}

// v0.5: Get a schema by subject and optional version
#[derive(Deserialize)]
pub struct GetSchemaParams {
    version: Option<u32>,
}

pub async fn get_schema(
    State(store): State<SharedStore>,
    Path(subject): Path<String>,
    Query(params): Query<GetSchemaParams>,
) -> Result<Json<serde_json::Value>> {
    let schema_registry = store.schema_registry();

    let schema = schema_registry.get_schema(&subject, params.version)?;

    tracing::debug!("Retrieved schema v{} for '{}'", schema.version, subject);

    Ok(Json(serde_json::json!({
        "id": schema.id,
        "subject": schema.subject,
        "version": schema.version,
        "schema": schema.schema,
        "created_at": schema.created_at,
        "description": schema.description,
        "tags": schema.tags
    })))
}

// v0.5: List all versions of a schema subject
pub async fn list_schema_versions(
    State(store): State<SharedStore>,
    Path(subject): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let schema_registry = store.schema_registry();

    let versions = schema_registry.list_versions(&subject)?;

    Ok(Json(serde_json::json!({
        "subject": subject,
        "versions": versions
    })))
}

// v0.5: List all schema subjects
pub async fn list_subjects(State(store): State<SharedStore>) -> Json<serde_json::Value> {
    let schema_registry = store.schema_registry();

    let subjects = schema_registry.list_subjects();

    Json(serde_json::json!({
        "subjects": subjects,
        "total": subjects.len()
    }))
}

// v0.5: Validate an event against a schema
pub async fn validate_event_schema(
    State(store): State<SharedStore>,
    Json(req): Json<ValidateEventRequest>,
) -> Result<Json<ValidateEventResponse>> {
    let schema_registry = store.schema_registry();

    let response = schema_registry.validate(&req.subject, req.version, &req.payload)?;

    if response.valid {
        tracing::debug!(
            "✅ Event validated against schema '{}' v{}",
            req.subject,
            response.schema_version
        );
    } else {
        tracing::warn!(
            "❌ Event validation failed for '{}': {:?}",
            req.subject,
            response.errors
        );
    }

    Ok(Json(response))
}

// v0.5: Set compatibility mode for a subject
#[derive(Deserialize)]
pub struct SetCompatibilityRequest {
    compatibility: CompatibilityMode,
}

pub async fn set_compatibility_mode(
    State(store): State<SharedStore>,
    Path(subject): Path<String>,
    Json(req): Json<SetCompatibilityRequest>,
) -> Json<serde_json::Value> {
    let schema_registry = store.schema_registry();

    schema_registry.set_compatibility_mode(subject.clone(), req.compatibility);

    tracing::info!(
        "🔧 Set compatibility mode for '{}' to {:?}",
        subject,
        req.compatibility
    );

    Json(serde_json::json!({
        "subject": subject,
        "compatibility": req.compatibility
    }))
}

// v0.5: Start a replay operation
pub async fn start_replay(
    State(store): State<SharedStore>,
    Json(req): Json<StartReplayRequest>,
) -> Result<Json<StartReplayResponse>> {
    let replay_manager = store.replay_manager();

    let response = replay_manager.start_replay(store, req)?;

    tracing::info!(
        "🔄 Started replay {} with {} events",
        response.replay_id,
        response.total_events
    );

    Ok(Json(response))
}

// v0.5: Get replay progress
pub async fn get_replay_progress(
    State(store): State<SharedStore>,
    Path(replay_id): Path<uuid::Uuid>,
) -> Result<Json<ReplayProgress>> {
    let replay_manager = store.replay_manager();

    let progress = replay_manager.get_progress(replay_id)?;

    Ok(Json(progress))
}

// v0.5: List all replay operations
pub async fn list_replays(State(store): State<SharedStore>) -> Json<serde_json::Value> {
    let replay_manager = store.replay_manager();

    let replays = replay_manager.list_replays();

    Json(serde_json::json!({
        "replays": replays,
        "total": replays.len()
    }))
}

// v0.5: Cancel a running replay
pub async fn cancel_replay(
    State(store): State<SharedStore>,
    Path(replay_id): Path<uuid::Uuid>,
) -> Result<Json<serde_json::Value>> {
    let replay_manager = store.replay_manager();

    replay_manager.cancel_replay(replay_id)?;

    tracing::info!("🛑 Cancelled replay {}", replay_id);

    Ok(Json(serde_json::json!({
        "replay_id": replay_id,
        "status": "cancelled"
    })))
}

// v0.5: Delete a completed replay
pub async fn delete_replay(
    State(store): State<SharedStore>,
    Path(replay_id): Path<uuid::Uuid>,
) -> Result<Json<serde_json::Value>> {
    let replay_manager = store.replay_manager();

    let deleted = replay_manager.delete_replay(replay_id)?;

    if deleted {
        tracing::info!("🗑️  Deleted replay {}", replay_id);
    }

    Ok(Json(serde_json::json!({
        "replay_id": replay_id,
        "deleted": deleted
    })))
}

// v0.5: Register a new pipeline
pub async fn register_pipeline(
    State(store): State<SharedStore>,
    Json(config): Json<PipelineConfig>,
) -> Result<Json<serde_json::Value>> {
    let pipeline_manager = store.pipeline_manager();

    let pipeline_id = pipeline_manager.register(config.clone());

    tracing::info!(
        "🔀 Pipeline registered: {} (name: {})",
        pipeline_id,
        config.name
    );

    Ok(Json(serde_json::json!({
        "pipeline_id": pipeline_id,
        "name": config.name,
        "enabled": config.enabled
    })))
}

// v0.5: List all pipelines
pub async fn list_pipelines(State(store): State<SharedStore>) -> Json<serde_json::Value> {
    let pipeline_manager = store.pipeline_manager();

    let pipelines = pipeline_manager.list();

    tracing::debug!("Listed {} pipelines", pipelines.len());

    Json(serde_json::json!({
        "pipelines": pipelines,
        "total": pipelines.len()
    }))
}

// v0.5: Get a specific pipeline
pub async fn get_pipeline(
    State(store): State<SharedStore>,
    Path(pipeline_id): Path<uuid::Uuid>,
) -> Result<Json<PipelineConfig>> {
    let pipeline_manager = store.pipeline_manager();

    let pipeline = pipeline_manager.get(pipeline_id).ok_or_else(|| {
        crate::error::AllSourceError::ValidationError(format!("Pipeline not found: {pipeline_id}"))
    })?;

    Ok(Json(pipeline.config().clone()))
}

// v0.5: Remove a pipeline
pub async fn remove_pipeline(
    State(store): State<SharedStore>,
    Path(pipeline_id): Path<uuid::Uuid>,
) -> Result<Json<serde_json::Value>> {
    let pipeline_manager = store.pipeline_manager();

    let removed = pipeline_manager.remove(pipeline_id);

    if removed {
        tracing::info!("🗑️  Removed pipeline {}", pipeline_id);
    }

    Ok(Json(serde_json::json!({
        "pipeline_id": pipeline_id,
        "removed": removed
    })))
}

// v0.5: Get statistics for all pipelines
pub async fn all_pipeline_stats(State(store): State<SharedStore>) -> Json<serde_json::Value> {
    let pipeline_manager = store.pipeline_manager();

    let stats = pipeline_manager.all_stats();

    Json(serde_json::json!({
        "stats": stats,
        "total": stats.len()
    }))
}

// v0.5: Get statistics for a specific pipeline
pub async fn get_pipeline_stats(
    State(store): State<SharedStore>,
    Path(pipeline_id): Path<uuid::Uuid>,
) -> Result<Json<PipelineStats>> {
    let pipeline_manager = store.pipeline_manager();

    let pipeline = pipeline_manager.get(pipeline_id).ok_or_else(|| {
        crate::error::AllSourceError::ValidationError(format!("Pipeline not found: {pipeline_id}"))
    })?;

    Ok(Json(pipeline.stats()))
}

// v0.5: Reset a pipeline's state
pub async fn reset_pipeline(
    State(store): State<SharedStore>,
    Path(pipeline_id): Path<uuid::Uuid>,
) -> Result<Json<serde_json::Value>> {
    let pipeline_manager = store.pipeline_manager();

    let pipeline = pipeline_manager.get(pipeline_id).ok_or_else(|| {
        crate::error::AllSourceError::ValidationError(format!("Pipeline not found: {pipeline_id}"))
    })?;

    pipeline.reset();

    tracing::info!("🔄 Reset pipeline {}", pipeline_id);

    Ok(Json(serde_json::json!({
        "pipeline_id": pipeline_id,
        "reset": true
    })))
}

// =============================================================================
// v0.11: Single Event Lookup by ID
// =============================================================================

/// Get a single event by UUID
pub async fn get_event_by_id(
    State(store): State<SharedStore>,
    Path(event_id): Path<uuid::Uuid>,
) -> Result<Json<serde_json::Value>> {
    let event = store.get_event_by_id(&event_id)?.ok_or_else(|| {
        crate::error::AllSourceError::EntityNotFound(format!("Event '{event_id}' not found"))
    })?;

    let dto = EventDto::from(&event);

    tracing::debug!("Event retrieved by ID: {}", event_id);

    Ok(Json(serde_json::json!({
        "event": dto,
        "found": true
    })))
}

// =============================================================================
// v0.7: Projection State API for Query Service Integration
// =============================================================================

/// List all registered projections
pub async fn list_projections(State(store): State<SharedStore>) -> Json<serde_json::Value> {
    let projection_manager = store.projection_manager();
    let status_map = store.projection_status();

    let projections: Vec<serde_json::Value> = projection_manager
        .list_projections()
        .iter()
        .map(|(name, projection)| {
            let status = status_map
                .get(name)
                .map_or_else(|| "running".to_string(), |s| s.value().clone());
            serde_json::json!({
                "name": name,
                "type": format!("{:?}", projection.name()),
                "status": status,
            })
        })
        .collect();

    tracing::debug!("Listed {} projections", projections.len());

    Json(serde_json::json!({
        "projections": projections,
        "total": projections.len()
    }))
}

/// Get projection metadata by name
pub async fn get_projection(
    State(store): State<SharedStore>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let projection_manager = store.projection_manager();

    let projection = projection_manager.get_projection(&name).ok_or_else(|| {
        crate::error::AllSourceError::EntityNotFound(format!("Projection '{name}' not found"))
    })?;

    Ok(Json(serde_json::json!({
        "name": projection.name(),
        "found": true
    })))
}

/// Get projection state for a specific entity.
///
/// Resolution order:
/// 1. **Registered projection** — if `name` is registered with the projection
///    manager, return the projection's own `get_state(entity_id)` output.
/// 2. **Projection state cache** — otherwise fall back to whatever was written
///    via `save_projection_state` / `bulk_save_projection_states`. This supports
///    SDK-managed projections (e.g. the Rust SDK's `ProjectionWorker`) that
///    compute state client-side and push it back without registering a
///    projection in Core's manager.
///
/// Returns `found: false` with `state: null` when neither source has state.
pub async fn get_projection_state(
    State(store): State<SharedStore>,
    Path((name, entity_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>> {
    let state = store
        .projection_manager()
        .get_projection(&name)
        .and_then(|p| p.get_state(&entity_id))
        .or_else(|| {
            store
                .projection_state_cache()
                .get(&format!("{name}:{entity_id}"))
                .map(|entry| entry.value().clone())
        });

    tracing::debug!("Projection state retrieved: {} / {}", name, entity_id);

    Ok(Json(serde_json::json!({
        "projection": name,
        "entity_id": entity_id,
        "state": state,
        "found": state.is_some()
    })))
}

/// Delete (clear) a projection by name
///
/// Removes all state from the projection. The projection definition remains
/// registered but its accumulated state is cleared.
pub async fn delete_projection(
    State(store): State<SharedStore>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let projection_manager = store.projection_manager();

    let projection = projection_manager.get_projection(&name).ok_or_else(|| {
        crate::error::AllSourceError::EntityNotFound(format!("Projection '{name}' not found"))
    })?;

    projection.clear();

    // Also clear any cached state for this projection
    let cache = store.projection_state_cache();
    let prefix = format!("{name}:");
    let keys_to_remove: Vec<String> = cache
        .iter()
        .filter(|entry| entry.key().starts_with(&prefix))
        .map(|entry| entry.key().clone())
        .collect();
    for key in keys_to_remove {
        cache.remove(&key);
    }

    tracing::info!("Projection deleted (cleared): {}", name);

    Ok(Json(serde_json::json!({
        "projection": name,
        "deleted": true
    })))
}

/// Query parameters for `GET /api/v1/projections/{name}/state`.
///
/// All optional: with none of them set the endpoint keeps its historical
/// behaviour of returning every cached entity for the projection, so existing
/// callers (the Query Service's `ProjectionServer` hydration, the SDKs) are
/// unaffected. Names match `ListEntitiesRequest` for consistency.
#[derive(Debug, Default, Deserialize)]
pub struct ProjectionStateSummaryParams {
    /// Maximum number of entity states to return. Unbounded when absent.
    pub limit: Option<usize>,
    /// Number of matching states to skip before applying `limit`. Default 0.
    pub offset: Option<usize>,
    /// Return only entities whose id starts with this prefix — lets a caller
    /// walk one shard of the keyspace without enumerating the whole projection.
    pub entity_id_prefix: Option<String>,
}

/// Get aggregate projection state (all entities).
///
/// Returns the cached states written via `save_projection_state` /
/// `bulk_save_projection_states`. The projection does NOT need to be
/// registered with the projection manager — this supports SDK-managed
/// projections that push state without server-side registration.
///
/// Supports `limit`, `offset` and `entity_id_prefix` (issue #249): this is the
/// only endpoint that can *enumerate* a projection — `bulk_get_projection_states`
/// needs the ids up front — so a projection with one entry per tenant needs a
/// way to bound and resume a request. Entities are ordered by `entity_id` so
/// offset paging is stable; `total` is the full match set and `has_more` tells
/// a paginator when to stop.
///
/// Returns an empty list when no state has been written.
pub async fn get_projection_state_summary(
    State(store): State<SharedStore>,
    Path(name): Path<String>,
    Query(params): Query<ProjectionStateSummaryParams>,
) -> Result<Json<serde_json::Value>> {
    let cache = store.projection_state_cache();
    let prefix = format!("{name}:");
    let offset = params.offset.unwrap_or(0);

    // Collect the matching keys first and sort them. DashMap iteration order is
    // arbitrary, so offset paging is only coherent against a total order — and
    // windowing ids instead of values means only the returned page's states are
    // cloned, not the whole projection.
    let mut entity_ids: Vec<String> = cache
        .iter()
        .filter_map(|entry| entry.key().strip_prefix(&prefix).map(ToString::to_string))
        .filter(|entity_id| {
            params
                .entity_id_prefix
                .as_ref()
                .is_none_or(|p| entity_id.starts_with(p))
        })
        .collect();
    entity_ids.sort_unstable();

    let total = entity_ids.len();

    let page = entity_ids.into_iter().skip(offset);
    let page: Vec<String> = match params.limit {
        Some(limit) => page.take(limit).collect(),
        None => page.collect(),
    };

    let states: Vec<serde_json::Value> = page
        .into_iter()
        .filter_map(|entity_id| {
            // Skip entries deleted between the key scan and the value read.
            cache.get(&format!("{prefix}{entity_id}")).map(|entry| {
                serde_json::json!({
                    "entity_id": entity_id,
                    "state": entry.value().clone()
                })
            })
        })
        .collect();

    let count = states.len();
    // Relative to the window actually served — a paginator that trusts a bare
    // `count < total` never terminates once an offset is in play (cf. #250).
    let has_more = offset + count < total;

    tracing::debug!(
        "Projection state summary: {} ({} of {} entities, offset {})",
        name,
        count,
        total,
        offset
    );

    Ok(Json(serde_json::json!({
        "projection": name,
        "states": states,
        "count": count,
        "total": total,
        "has_more": has_more
    })))
}

/// Reset a projection to its initial state
///
/// Clears all accumulated state and reprocesses events from the beginning.
pub async fn reset_projection(
    State(store): State<SharedStore>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let reprocessed = store.reset_projection(&name)?;

    tracing::info!(
        "Projection reset: {} ({} events reprocessed)",
        name,
        reprocessed
    );

    Ok(Json(serde_json::json!({
        "projection": name,
        "reset": true,
        "events_reprocessed": reprocessed
    })))
}

/// Pause a projection
///
/// Sets the projection status to "paused" so it stops processing new events.
pub async fn pause_projection(
    State(store): State<SharedStore>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let projection_manager = store.projection_manager();

    // Verify projection exists
    let _projection = projection_manager.get_projection(&name).ok_or_else(|| {
        crate::error::AllSourceError::EntityNotFound(format!("Projection '{name}' not found"))
    })?;

    store
        .projection_status()
        .insert(name.clone(), "paused".to_string());

    tracing::info!("Projection paused: {}", name);

    Ok(Json(serde_json::json!({
        "projection": name,
        "status": "paused"
    })))
}

/// Start (resume) a projection
///
/// Sets the projection status to "running" so it resumes processing events.
pub async fn start_projection(
    State(store): State<SharedStore>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let projection_manager = store.projection_manager();

    // Verify projection exists
    let _projection = projection_manager.get_projection(&name).ok_or_else(|| {
        crate::error::AllSourceError::EntityNotFound(format!("Projection '{name}' not found"))
    })?;

    store
        .projection_status()
        .insert(name.clone(), "running".to_string());

    tracing::info!("Projection started: {}", name);

    Ok(Json(serde_json::json!({
        "projection": name,
        "status": "running"
    })))
}

/// Request body for saving projection state
#[derive(Debug, Deserialize)]
pub struct SaveProjectionStateRequest {
    pub state: serde_json::Value,
}

/// Save/update projection state for an entity
///
/// This endpoint allows external services (like Elixir Query Service) to
/// store computed projection state back to the Core for persistence.
pub async fn save_projection_state(
    State(store): State<SharedStore>,
    Path((name, entity_id)): Path<(String, String)>,
    Json(req): Json<SaveProjectionStateRequest>,
) -> Result<Json<serde_json::Value>> {
    let projection_cache = store.projection_state_cache();

    // Store in the projection state cache
    projection_cache.insert(format!("{name}:{entity_id}"), req.state.clone());

    tracing::info!("Projection state saved: {} / {}", name, entity_id);

    Ok(Json(serde_json::json!({
        "projection": name,
        "entity_id": entity_id,
        "saved": true
    })))
}

/// Bulk get projection states for multiple entities
///
/// Efficient endpoint for fetching multiple entity states in a single request.
#[derive(Debug, Deserialize)]
pub struct BulkGetStateRequest {
    pub entity_ids: Vec<String>,
}

/// Bulk save projection states for multiple entities
///
/// Efficient endpoint for saving multiple entity states in a single request.
#[derive(Debug, Deserialize)]
pub struct BulkSaveStateRequest {
    pub states: Vec<BulkSaveStateItem>,
}

#[derive(Debug, Deserialize)]
pub struct BulkSaveStateItem {
    pub entity_id: String,
    pub state: serde_json::Value,
}

pub async fn bulk_get_projection_states(
    State(store): State<SharedStore>,
    Path(name): Path<String>,
    Json(req): Json<BulkGetStateRequest>,
) -> Result<Json<serde_json::Value>> {
    // Same fallback rule as `get_projection_state`: registered projection
    // wins, cache is the fallback. This lets SDK-managed projections read
    // their pushed-back state without registering in Core's projection manager.
    let projection = store.projection_manager().get_projection(&name);
    let cache = store.projection_state_cache();

    let states: Vec<serde_json::Value> = req
        .entity_ids
        .iter()
        .map(|entity_id| {
            let state = projection
                .as_ref()
                .and_then(|p| p.get_state(entity_id))
                .or_else(|| {
                    cache
                        .get(&format!("{name}:{entity_id}"))
                        .map(|entry| entry.value().clone())
                });
            serde_json::json!({
                "entity_id": entity_id,
                "state": state,
                "found": state.is_some()
            })
        })
        .collect();

    tracing::debug!(
        "Bulk projection state retrieved: {} entities from {}",
        states.len(),
        name
    );

    Ok(Json(serde_json::json!({
        "projection": name,
        "states": states,
        "total": states.len()
    })))
}

/// Bulk save projection states for multiple entities
///
/// This endpoint allows efficient batch saving of projection states,
/// critical for high-throughput event processing pipelines.
pub async fn bulk_save_projection_states(
    State(store): State<SharedStore>,
    Path(name): Path<String>,
    Json(req): Json<BulkSaveStateRequest>,
) -> Result<Json<serde_json::Value>> {
    let projection_cache = store.projection_state_cache();

    let mut saved_count = 0;
    for item in &req.states {
        projection_cache.insert(format!("{name}:{}", item.entity_id), item.state.clone());
        saved_count += 1;
    }

    tracing::info!(
        "Bulk projection state saved: {} entities for {}",
        saved_count,
        name
    );

    Ok(Json(serde_json::json!({
        "projection": name,
        "saved": saved_count,
        "total": req.states.len()
    })))
}

// =============================================================================
// v0.11: Webhook Management API
// =============================================================================

/// Query parameters for listing webhooks
#[derive(Debug, Deserialize)]
pub struct ListWebhooksParams {
    pub tenant_id: Option<String>,
}

/// Register a new webhook subscription
pub async fn register_webhook(
    State(store): State<SharedStore>,
    Json(req): Json<RegisterWebhookRequest>,
) -> Json<serde_json::Value> {
    let registry = store.webhook_registry();
    let webhook = registry.register(req);

    tracing::info!("Webhook registered: {} -> {}", webhook.id, webhook.url);

    Json(serde_json::json!({
        "webhook": webhook,
        "created": true
    }))
}

/// List webhooks, optionally filtered by tenant_id
pub async fn list_webhooks(
    State(store): State<SharedStore>,
    Query(params): Query<ListWebhooksParams>,
) -> Json<serde_json::Value> {
    let registry = store.webhook_registry();

    let webhooks = if let Some(tenant_id) = params.tenant_id {
        registry.list_by_tenant(&tenant_id)
    } else {
        // Without tenant filter, return empty (tenants should always filter)
        vec![]
    };

    let total = webhooks.len();

    Json(serde_json::json!({
        "webhooks": webhooks,
        "total": total
    }))
}

/// Get a specific webhook by ID
pub async fn get_webhook(
    State(store): State<SharedStore>,
    Path(webhook_id): Path<uuid::Uuid>,
) -> Result<Json<serde_json::Value>> {
    let registry = store.webhook_registry();

    let webhook = registry.get(webhook_id).ok_or_else(|| {
        crate::error::AllSourceError::EntityNotFound(format!("Webhook '{webhook_id}' not found"))
    })?;

    Ok(Json(serde_json::json!({
        "webhook": webhook,
        "found": true
    })))
}

/// Update a webhook subscription
pub async fn update_webhook(
    State(store): State<SharedStore>,
    Path(webhook_id): Path<uuid::Uuid>,
    Json(req): Json<UpdateWebhookRequest>,
) -> Result<Json<serde_json::Value>> {
    let registry = store.webhook_registry();

    let webhook = registry.update(webhook_id, req).ok_or_else(|| {
        crate::error::AllSourceError::EntityNotFound(format!("Webhook '{webhook_id}' not found"))
    })?;

    tracing::info!("Webhook updated: {}", webhook_id);

    Ok(Json(serde_json::json!({
        "webhook": webhook,
        "updated": true
    })))
}

/// Delete a webhook subscription
pub async fn delete_webhook(
    State(store): State<SharedStore>,
    Path(webhook_id): Path<uuid::Uuid>,
) -> Result<Json<serde_json::Value>> {
    let registry = store.webhook_registry();

    let webhook = registry.delete(webhook_id).ok_or_else(|| {
        crate::error::AllSourceError::EntityNotFound(format!("Webhook '{webhook_id}' not found"))
    })?;

    tracing::info!("Webhook deleted: {} ({})", webhook_id, webhook.url);

    Ok(Json(serde_json::json!({
        "webhook_id": webhook_id,
        "deleted": true
    })))
}

/// Query parameters for listing webhook deliveries
#[derive(Debug, Deserialize)]
pub struct ListDeliveriesParams {
    pub limit: Option<usize>,
}

/// List delivery history for a webhook
pub async fn list_webhook_deliveries(
    State(store): State<SharedStore>,
    Path(webhook_id): Path<uuid::Uuid>,
    Query(params): Query<ListDeliveriesParams>,
) -> Result<Json<serde_json::Value>> {
    let registry = store.webhook_registry();

    // Verify webhook exists
    registry.get(webhook_id).ok_or_else(|| {
        crate::error::AllSourceError::EntityNotFound(format!("Webhook '{webhook_id}' not found"))
    })?;

    let limit = params.limit.unwrap_or(50);
    let deliveries = registry.get_deliveries(webhook_id, limit);
    let total = deliveries.len();

    Ok(Json(serde_json::json!({
        "webhook_id": webhook_id,
        "deliveries": deliveries,
        "total": total
    })))
}

// =============================================================================
// v2.0: Advanced Query Features
// =============================================================================

/// EventQL: Execute SQL queries over events using DataFusion
#[cfg(feature = "analytics")]
pub async fn eventql_query(
    State(store): State<SharedStore>,
    Json(req): Json<crate::infrastructure::query::eventql::EventQLRequest>,
) -> Result<Json<serde_json::Value>> {
    let events = store.snapshot_events();
    match crate::infrastructure::query::eventql::execute_eventql(&events, &req).await {
        Ok(response) => Ok(Json(serde_json::json!({
            "columns": response.columns,
            "rows": response.rows,
            "row_count": response.row_count,
        }))),
        Err(e) => Err(crate::error::AllSourceError::InvalidQuery(e)),
    }
}

/// GraphQL: Execute GraphQL queries
pub async fn graphql_query(
    State(store): State<SharedStore>,
    Json(req): Json<GraphQLRequest>,
) -> Json<serde_json::Value> {
    let fields = match crate::infrastructure::query::graphql::parse_query(&req.query) {
        Ok(f) => f,
        Err(e) => {
            return Json(
                serde_json::to_value(GraphQLResponse {
                    data: None,
                    errors: vec![GraphQLError { message: e }],
                })
                .unwrap(),
            );
        }
    };

    let mut data = serde_json::Map::new();
    let mut errors = Vec::new();

    for field in &fields {
        match field.name.as_str() {
            "events" => {
                let request = crate::application::dto::QueryEventsRequest {
                    entity_id: field.arguments.get("entity_id").cloned(),
                    event_type: field.arguments.get("event_type").cloned(),
                    tenant_id: field.arguments.get("tenant_id").cloned(),
                    limit: field.arguments.get("limit").and_then(|l| l.parse().ok()),
                    as_of: None,
                    since: None,
                    until: None,
                    event_type_prefix: None,
                    exclude_event_type_prefix: None,
                    payload_filter: None,
                };
                match store.query(&request) {
                    Ok(events) => {
                        let json_events: Vec<serde_json::Value> = events
                            .iter()
                            .map(|e| {
                                crate::infrastructure::query::graphql::event_to_json(
                                    e,
                                    &field.fields,
                                )
                            })
                            .collect();
                        data.insert("events".to_string(), serde_json::Value::Array(json_events));
                    }
                    Err(e) => errors.push(GraphQLError {
                        message: format!("events query failed: {e}"),
                    }),
                }
            }
            "event" => {
                if let Some(id_str) = field.arguments.get("id") {
                    if let Ok(id) = uuid::Uuid::parse_str(id_str) {
                        match store.get_event_by_id(&id) {
                            Ok(Some(event)) => {
                                data.insert(
                                    "event".to_string(),
                                    crate::infrastructure::query::graphql::event_to_json(
                                        &event,
                                        &field.fields,
                                    ),
                                );
                            }
                            Ok(None) => {
                                data.insert("event".to_string(), serde_json::Value::Null);
                            }
                            Err(e) => errors.push(GraphQLError {
                                message: format!("event lookup failed: {e}"),
                            }),
                        }
                    } else {
                        errors.push(GraphQLError {
                            message: format!("Invalid UUID: {id_str}"),
                        });
                    }
                } else {
                    errors.push(GraphQLError {
                        message: "event query requires 'id' argument".to_string(),
                    });
                }
            }
            "projections" => {
                let pm = store.projection_manager();
                let names: Vec<serde_json::Value> = pm
                    .list_projections()
                    .iter()
                    .map(|(name, _)| serde_json::Value::String(name.clone()))
                    .collect();
                data.insert("projections".to_string(), serde_json::Value::Array(names));
            }
            "stats" => {
                let stats = store.stats();
                data.insert(
                    "stats".to_string(),
                    serde_json::json!({
                        "total_events": stats.total_events,
                        "total_entities": stats.total_entities,
                        "total_event_types": stats.total_event_types,
                    }),
                );
            }
            "__schema" => {
                data.insert(
                    "__schema".to_string(),
                    crate::infrastructure::query::graphql::introspection_schema(),
                );
            }
            other => {
                errors.push(GraphQLError {
                    message: format!("Unknown field: {other}"),
                });
            }
        }
    }

    Json(
        serde_json::to_value(GraphQLResponse {
            data: Some(serde_json::Value::Object(data)),
            errors,
        })
        .unwrap(),
    )
}

/// Geospatial: Query events by location
pub async fn geo_query(
    State(store): State<SharedStore>,
    Json(req): Json<GeoQueryRequest>,
) -> Json<serde_json::Value> {
    let events = store.snapshot_events();
    let geo_index = store.geo_index();
    let results =
        crate::infrastructure::query::geospatial::execute_geo_query(&events, &geo_index, &req);
    let total = results.len();
    Json(serde_json::json!({
        "results": results,
        "total": total,
    }))
}

/// Geospatial index stats
pub async fn geo_stats(State(store): State<SharedStore>) -> Json<serde_json::Value> {
    let stats = store.geo_index().stats();
    Json(serde_json::json!(stats))
}

/// Exactly-once processing stats
pub async fn exactly_once_stats(State(store): State<SharedStore>) -> Json<serde_json::Value> {
    let stats = store.exactly_once().stats();
    Json(serde_json::json!(stats))
}

/// Schema evolution history for an event type
pub async fn schema_evolution_history(
    State(store): State<SharedStore>,
    Path(event_type): Path<String>,
) -> Json<serde_json::Value> {
    let mgr = store.schema_evolution();
    let history = mgr.get_history(&event_type);
    let version = mgr.get_version(&event_type);
    Json(serde_json::json!({
        "event_type": event_type,
        "current_version": version,
        "history": history,
    }))
}

/// Current inferred schema for an event type
pub async fn schema_evolution_schema(
    State(store): State<SharedStore>,
    Path(event_type): Path<String>,
) -> Json<serde_json::Value> {
    let mgr = store.schema_evolution();
    if let Some(schema) = mgr.get_schema(&event_type) {
        let json_schema = crate::application::services::schema_evolution::to_json_schema(&schema);
        Json(serde_json::json!({
            "event_type": event_type,
            "version": mgr.get_version(&event_type),
            "inferred_schema": schema,
            "json_schema": json_schema,
        }))
    } else {
        Json(serde_json::json!({
            "event_type": event_type,
            "error": "No schema inferred for this event type"
        }))
    }
}

/// Schema evolution stats
pub async fn schema_evolution_stats(State(store): State<SharedStore>) -> Json<serde_json::Value> {
    let stats = store.schema_evolution().stats();
    let event_types = store.schema_evolution().list_event_types();
    Json(serde_json::json!({
        "stats": stats,
        "tracked_event_types": event_types,
    }))
}

// =============================================================================
// Sync Protocol Endpoints (v0.11: embedded↔server bidirectional sync)
// =============================================================================

/// POST /api/v1/sync/pull — Client sends version vector, server returns delta events.
#[cfg(feature = "embedded-sync")]
pub async fn sync_pull_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::embedded::sync_types::SyncPullRequest>,
) -> Result<Json<crate::embedded::sync_types::SyncPullResponse>> {
    use crate::infrastructure::cluster::{crdt::ReplicatedEvent, hlc::HlcTimestamp};

    let store = &state.store;

    // Compute "since" threshold from the client's version vector
    // We return all events the client hasn't seen yet
    let since = request
        .version_vector
        .values()
        .map(|ts| ts.physical_ms)
        .min()
        .and_then(|ms| chrono::DateTime::from_timestamp_millis(ms as i64));

    let events = store.query(&crate::application::dto::QueryEventsRequest {
        entity_id: None,
        event_type: None,
        tenant_id: None,
        as_of: None,
        since,
        until: None,
        limit: None,
        event_type_prefix: None,
        exclude_event_type_prefix: None,
        payload_filter: None,
    })?;

    // Convert domain events to ReplicatedEvent wire format
    let mut replicated = Vec::with_capacity(events.len());
    let mut last_ms = 0u64;
    let mut logical = 0u32;

    for event in &events {
        let event_ms = event.timestamp().timestamp_millis() as u64;
        if event_ms == last_ms {
            logical += 1;
        } else {
            last_ms = event_ms;
            logical = 0;
        }

        replicated.push(ReplicatedEvent {
            event_id: event.id().to_string(),
            hlc_timestamp: HlcTimestamp::new(event_ms, logical, 0),
            origin_region: "server".to_string(),
            event_data: serde_json::json!({
                "event_type": event.event_type_str(),
                "entity_id": event.entity_id_str(),
                "tenant_id": event.tenant_id_str(),
                "payload": event.payload,
                "metadata": event.metadata,
            }),
        });
    }

    Ok(Json(crate::embedded::sync_types::SyncPullResponse {
        events: replicated,
        version_vector: std::collections::BTreeMap::new(),
    }))
}

/// POST /api/v1/sync/push — Client pushes events, server applies CRDT resolution.
#[cfg(feature = "embedded-sync")]
pub async fn sync_push_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::embedded::sync_types::SyncPushRequest>,
) -> Result<Json<crate::embedded::sync_types::SyncPushResponse>> {
    let store = &state.store;

    let mut accepted = 0usize;
    let mut skipped = 0usize;

    for rep_event in &request.events {
        let event_data = &rep_event.event_data;
        let event_type = event_data
            .get("event_type")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let entity_id = event_data
            .get("entity_id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let tenant_id = event_data
            .get("tenant_id")
            .and_then(|v| v.as_str())
            .unwrap_or("default")
            .to_string();
        let payload = event_data
            .get("payload")
            .cloned()
            .unwrap_or(serde_json::json!({}));
        let metadata = event_data.get("metadata").cloned();

        match Event::from_strings(event_type, entity_id, tenant_id, payload, metadata) {
            Ok(domain_event) => {
                store.ingest(&domain_event)?;
                accepted += 1;
            }
            Err(_) => {
                skipped += 1;
            }
        }
    }

    Ok(Json(crate::embedded::sync_types::SyncPushResponse {
        accepted,
        skipped,
        version_vector: std::collections::BTreeMap::new(),
    }))
}

// =============================================================================
// Consumer endpoints for durable subscriptions (v0.14)
// =============================================================================

/// POST /api/v1/consumers — Register a durable consumer
pub async fn register_consumer(
    State(store): State<SharedStore>,
    Json(req): Json<RegisterConsumerRequest>,
) -> Result<Json<ConsumerResponse>> {
    let consumer = store
        .consumer_registry()
        .register(&req.consumer_id, &req.event_type_filters);

    Ok(Json(ConsumerResponse {
        consumer_id: consumer.consumer_id,
        event_type_filters: consumer.event_type_filters,
        cursor_position: consumer.cursor_position,
    }))
}

/// GET /api/v1/consumers/{consumer_id} — Get consumer metadata and cursor position
pub async fn get_consumer(
    State(store): State<SharedStore>,
    Path(consumer_id): Path<String>,
) -> Result<Json<ConsumerResponse>> {
    let consumer = store.consumer_registry().get_or_create(&consumer_id);

    Ok(Json(ConsumerResponse {
        consumer_id: consumer.consumer_id,
        event_type_filters: consumer.event_type_filters,
        cursor_position: consumer.cursor_position,
    }))
}

/// GET /api/v1/consumers/{consumer_id}/events — Poll for events since last ack
#[derive(Debug, Deserialize)]
pub struct ConsumerPollQuery {
    pub limit: Option<usize>,
}

pub async fn poll_consumer_events(
    State(store): State<SharedStore>,
    Path(consumer_id): Path<String>,
    Query(query): Query<ConsumerPollQuery>,
) -> Result<Json<ConsumerEventsResponse>> {
    let consumer = store.consumer_registry().get_or_create(&consumer_id);
    let offset = consumer.cursor_position.unwrap_or(0);
    let limit = query.limit.unwrap_or(100);

    let events = store.events_after_offset(offset, &consumer.event_type_filters, limit);
    let count = events.len();

    let consumer_events: Vec<ConsumerEventDto> = events
        .into_iter()
        .map(|(position, event)| ConsumerEventDto {
            position,
            event: EventDto::from(&event),
        })
        .collect();

    Ok(Json(ConsumerEventsResponse {
        events: consumer_events,
        count,
    }))
}

/// POST /api/v1/consumers/{consumer_id}/ack — Acknowledge processed events
pub async fn ack_consumer(
    State(store): State<SharedStore>,
    Path(consumer_id): Path<String>,
    Json(req): Json<AckRequest>,
) -> Result<Json<serde_json::Value>> {
    let max_offset = store.total_events() as u64;

    store
        .consumer_registry()
        .ack(&consumer_id, req.position, max_offset)
        .map_err(crate::error::AllSourceError::InvalidInput)?;

    Ok(Json(serde_json::json!({
        "status": "ok",
        "consumer_id": consumer_id,
        "position": req.position,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{domain::entities::Event, store::EventStore};

    fn create_test_store() -> Arc<EventStore> {
        Arc::new(EventStore::new())
    }

    /// Call the REAL `GET /api/v1/events/query` handler with `query` as the
    /// query string, parsed through the same extractors the router uses.
    ///
    /// Tests that hand the handler DTOs they built themselves cannot see
    /// parameters the DTOs never declare (issue #250) and cannot exercise the
    /// ordering/windowing composition the handler delegates to the store
    /// (issue #251), so ordering and pagination guards go through here.
    /// `tenant_id` is defaulted to the one `create_test_event` stamps.
    async fn query_page(store: &SharedStore, query: &str) -> QueryEventsResponse {
        use axum::extract::{Query, State};

        let uri: axum::http::Uri = format!("/api/v1/events/query?tenant_id=test-stream&{query}")
            .parse()
            .unwrap();
        query_events(
            OptionalAuth(None),
            Query::try_from_uri(&uri).unwrap(),
            Query::try_from_uri(&uri).unwrap(),
            Query::try_from_uri(&uri).unwrap(),
            Query::try_from_uri(&uri).unwrap(),
            State(store.clone()),
        )
        .await
        .unwrap()
        .0
    }

    fn create_test_event(entity_id: &str, event_type: &str) -> Event {
        Event::from_strings(
            event_type.to_string(),
            entity_id.to_string(),
            "test-stream".to_string(),
            serde_json::json!({
                "name": "Test",
                "value": 42
            }),
            None,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn test_query_events_has_more_and_total_count() {
        let store = create_test_store();

        // Ingest 50 events
        for i in 0..50 {
            store
                .ingest(&create_test_event(&format!("entity-{i}"), "user.created"))
                .unwrap();
        }

        // Query with limit=10 — should get has_more=true, total_count=50
        let req = QueryEventsRequest {
            entity_id: None,
            event_type: None,
            tenant_id: None,
            as_of: None,
            since: None,
            until: None,
            limit: Some(10),
            event_type_prefix: None,
            exclude_event_type_prefix: None,
            payload_filter: None,
        };

        let requested_limit = req.limit;
        let unlimited_req = QueryEventsRequest {
            limit: None,
            ..QueryEventsRequest {
                entity_id: req.entity_id,
                event_type: req.event_type,
                tenant_id: req.tenant_id,
                as_of: req.as_of,
                since: req.since,
                until: req.until,
                limit: None,
                event_type_prefix: req.event_type_prefix,
                exclude_event_type_prefix: None,
                payload_filter: req.payload_filter,
            }
        };
        let all_events = store.query(&unlimited_req).unwrap();
        let total_count = all_events.len();
        let limited_events: Vec<Event> = if let Some(limit) = requested_limit {
            all_events.into_iter().take(limit).collect()
        } else {
            all_events
        };
        let count = limited_events.len();
        let has_more = count < total_count;

        assert_eq!(count, 10);
        assert_eq!(total_count, 50);
        assert!(has_more);
    }

    // Regression guard for issue #250: `GET /api/v1/events/query` must honour
    // `offset`. It used to be dropped silently (the DTO did not declare it), so
    // every page returned the same first `limit` events and `has_more` stayed
    // true — the Rust SDK's `EventPaginator` (and the Go SDK's `QueryOptions`)
    // both send `offset`, so `collect_all()` looped forever accumulating
    // duplicates. Drives the REAL handler through query-string deserialization,
    // because the bug lived in the wire layer: a hand-rolled test that builds
    // the DTO in code cannot see a field the DTO never declares.
    #[tokio::test]
    async fn query_events_honours_offset_pagination() {
        use axum::extract::{Query, State};

        let store = create_test_store();
        for i in 0..25 {
            store
                .ingest(&create_test_event(&format!("e-{i:02}"), "user.created"))
                .unwrap();
        }

        // Parse a real query string through the same extractors the router uses.
        async fn page(store: &SharedStore, limit: usize, offset: usize) -> QueryEventsResponse {
            let uri: axum::http::Uri =
                format!("/api/v1/events/query?tenant_id=test-stream&limit={limit}&offset={offset}")
                    .parse()
                    .unwrap();
            let req: Query<QueryEventsRequest> = Query::try_from_uri(&uri).unwrap();
            let order: Query<EventOrderParam> = Query::try_from_uri(&uri).unwrap();
            let off: Query<EventOffsetParam> = Query::try_from_uri(&uri).unwrap();
            assert_eq!(off.0.offset, Some(offset), "offset must deserialize");
            query_events(
                OptionalAuth(None),
                req,
                order,
                off,
                Query::try_from_uri(&uri).unwrap(),
                State(store.clone()),
            )
            .await
            .unwrap()
            .0
        }

        let p1 = page(&store, 10, 0).await;
        let p2 = page(&store, 10, 10).await;
        let p3 = page(&store, 10, 20).await;

        assert_eq!(p1.count, 10);
        assert_eq!(p2.count, 10);
        assert_eq!(p3.count, 5, "last page returns the remainder");
        assert_eq!(p1.total_count, 25);

        // The core failure: page 2 must not be page 1 again.
        let ids = |r: &QueryEventsResponse| -> Vec<String> {
            r.events.iter().map(|e| e.entity_id.clone()).collect()
        };
        assert_ne!(ids(&p1), ids(&p2), "offset=10 must skip the first page");

        let mut all = ids(&p1);
        all.extend(ids(&p2));
        all.extend(ids(&p3));
        let unique: std::collections::HashSet<_> = all.iter().cloned().collect();
        assert_eq!(
            unique.len(),
            25,
            "paging the whole set must yield 25 distinct entities, not duplicates"
        );

        // `has_more` must account for the offset, otherwise a paginator that
        // trusts it never terminates.
        assert!(p1.has_more, "25 events, page 1 of 10 → more remain");
        assert!(p2.has_more, "25 events, page 2 of 10 → more remain");
        assert!(!p3.has_more, "offset=20 + count=5 == total → exhausted");

        // Past the end: empty page, and exhausted rather than "more".
        let past = page(&store, 10, 100).await;
        assert_eq!(past.count, 0);
        assert!(!past.has_more, "offset beyond the match set is exhausted");
    }

    // Regression guard for issue #249: `GET /api/v1/projections/{name}/state`
    // must honour `limit`, `offset` and `entity_id_prefix`. The handler used to
    // take only `Path(name)`, so query parameters were dropped on the floor and
    // the response grew linearly with the number of cached entities — a caller
    // with one entry per tenant had no way to bound a request or resume one.
    // Driven through a real router so query-string deserialization is exercised:
    // a test that hands the handler a DTO it built itself cannot fail on
    // parameters the handler never declares.
    #[tokio::test]
    async fn projection_state_summary_honours_limit_offset_and_prefix() {
        use axum::{
            body::{Body, to_bytes},
            http::Request,
        };
        use tower::ServiceExt; // for `oneshot`

        let store = create_test_store();
        let cache = store.projection_state_cache();
        for i in 0..25 {
            cache.insert(
                format!("demo:tenant-{i:02}"),
                serde_json::json!({ "n": i as u64 }),
            );
        }
        // A neighbouring projection and a same-prefix-looking key must not leak.
        cache.insert(
            "other:tenant-99".to_string(),
            serde_json::json!({ "n": 99 }),
        );

        let app = Router::new()
            .route(
                "/api/v1/projections/{name}/state",
                get(get_projection_state_summary),
            )
            .with_state(store.clone());

        async fn fetch(app: &Router, uri: &str) -> serde_json::Value {
            let resp = app
                .clone()
                .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(resp.status(), axum::http::StatusCode::OK, "GET {uri}");
            let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
            serde_json::from_slice(&bytes).unwrap()
        }

        let ids = |body: &serde_json::Value| -> Vec<String> {
            body["states"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| s["entity_id"].as_str().unwrap().to_string())
                .collect()
        };

        // No params: unchanged behaviour — the whole projection, scoped to it.
        let all = fetch(&app, "/api/v1/projections/demo/state").await;
        assert_eq!(all["total"], 25);
        assert_eq!(ids(&all).len(), 25);

        // limit bounds the body; total still reports the full match set.
        let p1 = fetch(&app, "/api/v1/projections/demo/state?limit=10").await;
        assert_eq!(ids(&p1).len(), 10, "limit must bound the response");
        assert_eq!(p1["total"], 25);
        assert_eq!(p1["count"], 10);
        assert_eq!(p1["has_more"], true);

        // offset resumes where the previous page stopped.
        let p2 = fetch(&app, "/api/v1/projections/demo/state?limit=10&offset=10").await;
        let p3 = fetch(&app, "/api/v1/projections/demo/state?limit=10&offset=20").await;
        assert_eq!(ids(&p3).len(), 5, "last page returns the remainder");
        assert_eq!(p3["has_more"], false, "offset + count == total → exhausted");
        assert_ne!(ids(&p1), ids(&p2), "offset=10 must skip the first page");

        let mut walked = ids(&p1);
        walked.extend(ids(&p2));
        walked.extend(ids(&p3));
        let unique: std::collections::HashSet<_> = walked.iter().cloned().collect();
        assert_eq!(
            unique.len(),
            25,
            "paging the whole projection must yield 25 distinct entities"
        );

        // Paging is only meaningful over a stable order — DashMap iteration is not.
        let mut sorted = walked.clone();
        sorted.sort();
        assert_eq!(walked, sorted, "pages must be ordered by entity_id");

        // Past the end: empty page, exhausted rather than "more".
        let past = fetch(&app, "/api/v1/projections/demo/state?limit=10&offset=100").await;
        assert_eq!(past["count"], 0);
        assert_eq!(past["has_more"], false);
        assert_eq!(past["total"], 25);

        // entity_id_prefix narrows to one shard of the keyspace.
        let shard = fetch(
            &app,
            "/api/v1/projections/demo/state?entity_id_prefix=tenant-1",
        )
        .await;
        assert_eq!(shard["total"], 10, "tenant-10..tenant-19");
        assert!(
            ids(&shard).iter().all(|id| id.starts_with("tenant-1")),
            "entity_id_prefix must filter"
        );

        // The projection scope itself still holds under paging.
        let other = fetch(&app, "/api/v1/projections/other/state?limit=10").await;
        assert_eq!(ids(&other), vec!["tenant-99".to_string()]);
    }

    // Regression guard for issue #251: a bounded page must cost the page, not the
    // whole match set. The handler used to re-run the query with `limit: None`
    // just to compute `total_count`, so `?entity_id=E&limit=1&order=desc` — the
    // documented "latest event for an entity" read — cloned and sorted the
    // entity's entire history on every request.
    //
    // Cost is measured by counting `Event` CLONES (`crate::clone_probe`), NOT
    // with Core's `query_results_total` metric: that counter is incremented with
    // `results.len()`, i.e. rows RETURNED, so it reads 1 whether the store
    // clones one event or clones 200 and discards 199 — it cannot fail on a
    // revert of the store-side windowing. Drives the REAL handler through
    // query-string deserialization so the count is what an HTTP caller pays.
    #[test]
    fn query_events_limit_does_not_materialize_whole_history() {
        const HISTORY: usize = 200;

        let store = create_test_store();
        for _ in 0..HISTORY {
            store
                .ingest(&create_test_event("entity-hot", "user.updated"))
                .unwrap();
        }

        // `clone_probe` is thread-local, so the handler future is driven to
        // completion on THIS thread (current-thread runtime, inside the measured
        // closure) — every clone the request makes is therefore counted.
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let (resp, materialized) = crate::clone_probe::measure(|| {
            runtime.block_on(query_page(
                &store,
                "entity_id=entity-hot&limit=1&order=desc",
            ))
        });

        // The page itself stays correct: one event, and the total/has_more pair
        // still describes the full match set.
        assert_eq!(resp.count, 1);
        assert_eq!(resp.events.len(), 1);
        assert_eq!(resp.total_count, HISTORY);
        assert!(resp.has_more);

        assert_eq!(
            materialized, 1,
            "limit=1 materialized {materialized} events out of {HISTORY}: \
             `limit` must bound what a request materializes, not just what it \
             returns"
        );
    }

    /// Like [`query_page`] but surfaces the handler's error instead of
    /// unwrapping — for the parameter values that must be REJECTED.
    async fn query_page_result(
        store: &SharedStore,
        query: &str,
    ) -> Result<Json<QueryEventsResponse>> {
        use axum::extract::{Query, State};

        let uri: axum::http::Uri = format!("/api/v1/events/query?tenant_id=test-stream&{query}")
            .parse()
            .unwrap();
        query_events(
            OptionalAuth(None),
            Query::try_from_uri(&uri).unwrap(),
            Query::try_from_uri(&uri).unwrap(),
            Query::try_from_uri(&uri).unwrap(),
            Query::try_from_uri(&uri).unwrap(),
            State(store.clone()),
        )
        .await
    }

    // A filter the server cannot apply must be an ERROR, not silence. The
    // payload filter is parsed inside the per-event predicate with `if let
    // Ok(..)`, so an unparseable one simply never matched anything and the
    // query answered as if no filter had been sent — the caller asked for
    // "events where user_id = alice" and got the tenant's whole stream, with a
    // `total_count` that agreed. Fails OPEN, which is the dangerous direction
    // for a filter, and the endpoint already rejects an unusable `order`, so
    // silence here was also inconsistent.
    #[tokio::test]
    async fn query_events_rejects_a_payload_filter_it_cannot_apply() {
        let store = create_test_store();
        for name in ["alice", "bob", "carol"] {
            let mut event = create_test_event(name, "user.created");
            event.payload = serde_json::json!({ "user_id": name });
            store.ingest(&event).unwrap();
        }

        // A well-formed filter still filters — the guard must not reject the
        // shape callers actually send (`{"user_id":"alice"}`, percent-encoded).
        let ok = query_page(&store, "payload_filter=%7B%22user_id%22%3A%22alice%22%7D").await;
        assert_eq!(ok.count, 1, "a valid payload_filter must still work");
        assert_eq!(ok.total_count, 1);

        for bad in [
            "not-json",                    // not JSON at all
            "%7B%22user_id%22%3A%22alice", // truncated object
            "%5B%22alice%22%5D",           // valid JSON, but an array, not an object
            "42",                          // valid JSON, but a scalar
        ] {
            let result = query_page_result(&store, &format!("payload_filter={bad}")).await;
            let Err(err) = result else {
                let resp = result.unwrap().0;
                panic!(
                    "payload_filter={bad} was silently ignored: returned {} of {} \
                     events unfiltered instead of rejecting a filter the server \
                     cannot apply",
                    resp.count, resp.total_count
                );
            };
            assert!(
                matches!(err, crate::error::AllSourceError::InvalidInput(_)),
                "payload_filter={bad} must be a 400, got {err:?}"
            );
        }
    }

    // `order` is the other parameter whose only legal values are a closed set.
    // It is validated, but nothing pinned that: collapsing the match to a
    // `_ => false` default arm would make `?order=descending` silently return
    // OLDEST-first — a paginator would read the wrong end of the stream and
    // nothing in the response would say so.
    #[tokio::test]
    async fn query_events_rejects_an_unusable_order_value() {
        let store = create_test_store();
        store
            .ingest(&create_test_event("e-1", "user.created"))
            .unwrap();

        for bad in ["descending", "DESCENDING", "newest", "1", "asc%20"] {
            let result = query_page_result(&store, &format!("order={bad}")).await;
            let Err(err) = result else {
                panic!("order={bad} must be rejected, not silently defaulted");
            };
            assert!(
                matches!(err, crate::error::AllSourceError::InvalidInput(_)),
                "order={bad} must be a 400, got {err:?}"
            );
        }

        // The accepted spellings stay accepted, in any case.
        for good in ["asc", "ASC", "desc", "DeSc"] {
            let accepted = query_page_result(&store, &format!("order={good}"))
                .await
                .unwrap_or_else(|e| panic!("order={good} must be accepted: {e:?}"));
            assert_eq!(accepted.0.count, 1, "order={good}");
        }
    }

    // `exclude_event_type_prefix` had no test anywhere in Core — not at this
    // handler, not in the store — despite being an HTTP-exposed filter the
    // Query Service forwards verbatim, and despite #251 having just rewritten
    // the code that decides WHEN it runs relative to the window. Its contract
    // is more than "drop these events": the DTO promises exclusion happens
    // BEFORE the limit, so excluded events never consume the result window, and
    // the prefixes are comma-separated. Both claims are only observable in
    // composition with `limit`/`offset`, so this drives the real handler.
    #[tokio::test]
    async fn query_events_exclude_prefix_applies_before_the_window() {
        const TYPES: [&str; 4] = [
            "audit.write",
            "user.created",
            "service.ping",
            "user.updated",
        ];
        let store = create_test_store();
        let base = chrono::Utc::now() - chrono::Duration::hours(24);
        let mut kept = Vec::new();
        for i in 0..12i64 {
            let mut event = create_test_event("org-1", TYPES[i as usize % 4]);
            event.timestamp = base + chrono::Duration::minutes(i);
            event.version = i + 1;
            if i % 2 == 1 {
                kept.push(event.id);
            }
            store.ingest(&event).unwrap();
        }
        assert_eq!(kept.len(), 6, "half the stream is user.*");

        // A page of 3 must be 3 SURVIVING events. Excluding after the window
        // instead would serve 3 minus however many the page happened to hit —
        // here 1 — while still reporting a plausible-looking count.
        let page = query_page(
            &store,
            "exclude_event_type_prefix=audit.,%20service.&limit=3",
        )
        .await;
        assert_eq!(
            page.events.iter().map(|e| e.id).collect::<Vec<_>>(),
            kept[..3].to_vec(),
            "limit=3 must yield 3 non-excluded events: exclusion runs before \
             the window, and the prefix list is comma-separated (whitespace \
             trimmed)"
        );
        assert!(
            page.events
                .iter()
                .all(|e| !e.event_type.starts_with("audit.")
                    && !e.event_type.starts_with("service.")),
            "excluded namespaces must not appear: {:?}",
            page.events
                .iter()
                .map(|e| &e.event_type)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            page.total_count, 6,
            "total_count is the post-exclusion match set, not the 12 ingested"
        );
        assert!(page.has_more);

        // Paging the excluded view walks the 6 survivors exactly once and stops.
        let mut walked = Vec::new();
        for offset in [0, 3, 6] {
            let p = query_page(
                &store,
                &format!("exclude_event_type_prefix=audit.,service.&limit=3&offset={offset}"),
            )
            .await;
            assert_eq!(
                p.has_more,
                offset + p.count < 6,
                "has_more must terminate on the excluded view (offset={offset})"
            );
            walked.extend(p.events.iter().map(|e| e.id));
        }
        assert_eq!(
            walked, kept,
            "exclusion + paging must cover the survivors once"
        );

        // Composes with order=desc: newest survivor first, not newest event.
        let desc = query_page(
            &store,
            "exclude_event_type_prefix=audit.,service.&limit=1&order=desc",
        )
        .await;
        assert_eq!(
            desc.events[0].id,
            *kept.last().unwrap(),
            "order=desc&limit=1 over an excluded view is the newest SURVIVOR"
        );

        // One prefix excludes only its namespace; a prefix matching nothing
        // excludes nothing.
        let audit_only = query_page(&store, "exclude_event_type_prefix=audit.").await;
        assert_eq!(audit_only.total_count, 9, "12 minus the 3 audit.* events");
        let nothing = query_page(&store, "exclude_event_type_prefix=nosuch.").await;
        assert_eq!(nothing.total_count, 12);
    }

    // `since`/`until`/`as_of` must narrow the result set on EVERY query shape,
    // including the one no index narrows: scoped by tenant only, which is what
    // the gateway forwards for "this tenant's activity since T" (the Query
    // Service passes all three straight through — `@core_compat_filters`).
    // Those three filters are evaluated against index entries, and the
    // full-scan branch of the store never consults the index, so a tenant-only
    // query silently ignored the window and answered with the whole history —
    // with `total_count`/`has_more` describing that history too, so a paginator
    // walked events the caller had explicitly excluded.
    //
    // Driven through the real handler and real query-string deserialization:
    // the parameters exist on the DTO and parse fine, so nothing at the wire
    // layer reveals that no code downstream reads them on this path.
    #[tokio::test]
    async fn query_events_honours_time_window_without_an_entity_or_type_filter() {
        use chrono::{SecondsFormat, SubsecRound};

        let store = create_test_store();
        // FIXED base, not `Utc::now()`, and truncated to whole seconds.
        //
        // This test previously seeded from `Utc::now()`, which made it depend on
        // the host clock's resolution: Linux hands back nanoseconds, macOS does
        // not. `SecondsFormat::Micros` truncates DOWNWARDS, so with a
        // nanosecond-bearing base the string `until=<t>` names an instant
        // fractionally BEFORE the event stamped at `t`, and an inclusive window
        // silently drops its boundary event. It passed on every developer
        // machine and failed only in CI — the worst kind of flake.
        //
        // The nanoseconds below are deliberate: they make the round-trip
        // assertion in `at` fail loudly if the `trunc_subsecs(0)` is ever
        // removed, on every platform rather than only on one.
        let base = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00.123456789Z")
            .unwrap()
            .to_utc()
            .trunc_subsecs(0);
        let mut ids = Vec::new();
        for i in 0..5i64 {
            let mut event = create_test_event(&format!("e-{i}"), "user.created");
            event.timestamp = base + chrono::Duration::hours(i);
            event.version = i + 1;
            ids.push(event.id);
            store.ingest(&event).unwrap();
        }
        // `Z`-suffixed so the timestamp survives a query string — an offset of
        // `+00:00` would be decoded as a space.
        let at = |h: i64| {
            let t = base + chrono::Duration::hours(h);
            let s = t.to_rfc3339_opts(SecondsFormat::Micros, true);
            // Pin the invariant the whole test rests on: the formatted string
            // must name the SAME instant the event carries. If a future edit
            // drops the `trunc_subsecs(0)` above, this fails on every platform
            // rather than only on the ones with a nanosecond clock.
            assert_eq!(
                chrono::DateTime::parse_from_rfc3339(&s).unwrap().to_utc(),
                t,
                "query-string timestamp must round-trip exactly, else the window \
                 boundary silently excludes the event stamped at it"
            );
            s
        };

        for (qs, expected) in [
            (format!("since={}", at(2)), vec![ids[2], ids[3], ids[4]]),
            (format!("until={}", at(1)), vec![ids[0], ids[1]]),
            (format!("as_of={}", at(1)), vec![ids[0], ids[1]]),
            (
                format!("since={}&until={}", at(1), at(3)),
                vec![ids[1], ids[2], ids[3]],
            ),
        ] {
            let resp = query_page(&store, &qs).await;
            let got: Vec<_> = resp.events.iter().map(|e| e.id).collect();
            assert_eq!(got, expected, "?{qs} must return only the window");
            assert_eq!(resp.count, expected.len(), "?{qs}");
            assert_eq!(
                resp.total_count,
                expected.len(),
                "?{qs}: total_count must count the window, not the history"
            );
            assert!(!resp.has_more, "?{qs}: the whole window was served");
        }

        // The window composes with paging: page 2 of a `since` window is the
        // second page OF THAT WINDOW, and `has_more` terminates on it.
        let page1 = query_page(&store, &format!("since={}&limit=2", at(2))).await;
        let page2 = query_page(&store, &format!("since={}&limit=2&offset=2", at(2))).await;
        assert_eq!(
            page1.events.iter().map(|e| e.id).collect::<Vec<_>>(),
            vec![ids[2], ids[3]]
        );
        assert!(page1.has_more, "3 in the window, 2 served");
        assert_eq!(
            page2.events.iter().map(|e| e.id).collect::<Vec<_>>(),
            vec![ids[4]]
        );
        assert!(!page2.has_more, "offset 2 + count 1 == the window's 3");
        assert_eq!(page2.total_count, 3);

        // …and with `order=desc`.
        let desc = query_page(&store, &format!("since={}&order=desc", at(2))).await;
        assert_eq!(
            desc.events.iter().map(|e| e.id).collect::<Vec<_>>(),
            vec![ids[4], ids[3], ids[2]]
        );

        // An empty window is empty, not "everything".
        let empty = query_page(&store, &format!("since={}", at(99))).await;
        assert_eq!(empty.count, 0);
        assert_eq!(empty.total_count, 0);
        assert!(!empty.has_more);
    }

    // Tenant-isolation gate: the public events query must fail CLOSED — a request
    // with no auth context and no tenant_id returns nothing, never a cross-tenant
    // scan. Calls the real handler so the boundary check is exercised.
    #[tokio::test]
    async fn query_events_fails_closed_without_tenant() {
        use axum::extract::{Query, State};

        let store = create_test_store();
        for i in 0..5 {
            store
                .ingest(&create_test_event(&format!("e-{i}"), "user.created"))
                .unwrap();
        }

        let resp = query_events(
            OptionalAuth(None),
            Query(QueryEventsRequest::default()),
            Query(EventOrderParam { order: None }),
            Query(EventOffsetParam { offset: None }),
            Query(EventIntegrityParam::default()),
            State(store.clone()),
        )
        .await
        .unwrap();
        assert_eq!(
            resp.0.total_count, 0,
            "a no-tenant query must NOT return cross-tenant events"
        );
        assert_eq!(resp.0.count, 0);

        // The same query scoped to the events' tenant returns them.
        // (create_test_event stamps tenant "test-stream" — the 3rd from_strings arg.)
        let scoped = query_events(
            OptionalAuth(None),
            Query(QueryEventsRequest {
                tenant_id: Some("test-stream".to_string()),
                ..QueryEventsRequest::default()
            }),
            Query(EventOrderParam { order: None }),
            Query(EventOffsetParam { offset: None }),
            Query(EventIntegrityParam::default()),
            State(store),
        )
        .await
        .unwrap();
        assert_eq!(
            scoped.0.total_count, 5,
            "tenant-scoped query returns its events"
        );
    }

    // The dashboard's streams + event-types counts must be per-tenant. These
    // endpoints used to scan ALL tenants (platform totals shown as "yours", and a
    // cross-tenant spill). Assert each tenant sees only its own, and no-tenant
    // fails closed.
    #[tokio::test]
    async fn list_streams_and_types_are_tenant_scoped() {
        use crate::domain::entities::Event;
        use axum::extract::{Query, State};

        let store = create_test_store();
        let ev = |entity: &str, etype: &str, tenant: &str| {
            Event::from_strings(
                etype.to_string(),
                entity.to_string(),
                tenant.to_string(),
                serde_json::json!({}),
                None,
            )
            .unwrap()
        };
        // tenant A: 2 entities, 2 types. tenant B: 1 entity, 1 type.
        store.ingest(&ev("e1", "order.placed", "tenant-a")).unwrap();
        store.ingest(&ev("e2", "user.created", "tenant-a")).unwrap();
        store
            .ingest(&ev("e9", "thing.happened", "tenant-b"))
            .unwrap();

        let streams = |tid: Option<&str>| {
            list_streams(
                OptionalAuth(None),
                State(store.clone()),
                Query(ListStreamsParams {
                    tenant_id: tid.map(String::from),
                    limit: None,
                    offset: None,
                }),
            )
        };
        assert_eq!(
            streams(Some("tenant-a")).await.0.total,
            2,
            "tenant-a streams"
        );
        assert_eq!(
            streams(Some("tenant-b")).await.0.total,
            1,
            "tenant-b streams"
        );
        assert_eq!(
            streams(None).await.0.total,
            0,
            "no tenant -> no streams (fail closed)"
        );

        let types = |tid: Option<&str>| {
            list_event_types(
                OptionalAuth(None),
                State(store.clone()),
                Query(ListEventTypesParams {
                    tenant_id: tid.map(String::from),
                    limit: None,
                    offset: None,
                }),
            )
        };
        assert_eq!(
            types(Some("tenant-a")).await.0.total,
            2,
            "tenant-a event types"
        );
        assert_eq!(
            types(Some("tenant-b")).await.0.total,
            1,
            "tenant-b event types"
        );
        assert_eq!(
            types(None).await.0.total,
            0,
            "no tenant -> no types (fail closed)"
        );
    }

    #[tokio::test]
    async fn test_query_events_no_more_results() {
        let store = create_test_store();

        // Ingest 5 events
        for i in 0..5 {
            store
                .ingest(&create_test_event(&format!("entity-{i}"), "user.created"))
                .unwrap();
        }

        // Query with limit=100 — should get has_more=false, total_count=5
        let all_events = store
            .query(&QueryEventsRequest {
                entity_id: None,
                event_type: None,
                tenant_id: None,
                as_of: None,
                since: None,
                until: None,
                limit: None,
                event_type_prefix: None,
                exclude_event_type_prefix: None,
                payload_filter: None,
            })
            .unwrap();
        let total_count = all_events.len();
        let limited_events: Vec<Event> = all_events.into_iter().take(100).collect();
        let count = limited_events.len();
        let has_more = count < total_count;

        assert_eq!(count, 5);
        assert_eq!(total_count, 5);
        assert!(!has_more);
    }

    // Regression test for issue #177: `order=desc` + `limit=1` must return
    // the NEWEST event for an entity, not the oldest. Mirrors the ordering
    // logic in `query_events` (store.query → reverse-if-desc → take(limit)).
    #[tokio::test]
    async fn test_query_events_order_desc_returns_latest() {
        // Drives the REAL handler. An earlier version of this test rebuilt the
        // ordering inline (`ascending.clone(); reverse(); take(1)`) and never
        // called `query_events`, so it could not fail on a mis-composition in
        // the code that actually serves `order=desc` — which since issue #251
        // lives in `EventStore::query_window`, not in the handler.
        let store = create_test_store();

        // Five events for the same entity with strictly increasing timestamps —
        // mimics a backfill appending corrected events.
        let base = chrono::Utc::now();
        let mut ascending_ids = Vec::new();
        for i in 0..5i64 {
            let mut event = create_test_event("org-1", "auth.org.updated");
            event.timestamp = base + chrono::Duration::seconds(i);
            event.version = i + 1;
            ascending_ids.push(event.id);
            store.ingest(&event).unwrap();
        }
        let newest_ts = base + chrono::Duration::seconds(4);

        // The documented "latest event for an entity" read.
        let latest = query_page(&store, "entity_id=org-1&limit=1&order=desc").await;
        assert_eq!(latest.count, 1);
        assert_eq!(
            latest.events[0].id,
            ascending_ids[4],
            "order=desc&limit=1 must yield the NEWEST event, got the one at \
             ascending position {:?}",
            ascending_ids
                .iter()
                .position(|id| *id == latest.events[0].id)
        );
        assert_eq!(latest.events[0].timestamp, newest_ts);
        assert_eq!(latest.total_count, 5, "total is the full match set");
        assert!(latest.has_more);

        // Default order (and an explicit `asc`) still yields the OLDEST.
        for qs in [
            "entity_id=org-1&limit=1",
            "entity_id=org-1&limit=1&order=asc",
        ] {
            let oldest = query_page(&store, qs).await;
            assert_eq!(oldest.events[0].id, ascending_ids[0], "{qs}");
            assert_eq!(oldest.events[0].timestamp, base);
        }

        // An unbounded desc page is the exact reverse of the ascending one —
        // reversal must apply to the whole match set, not just to the page.
        let all_desc = query_page(&store, "entity_id=org-1&order=desc").await;
        let got: Vec<_> = all_desc.events.iter().map(|e| e.id).collect();
        let expected: Vec<_> = ascending_ids.iter().rev().copied().collect();
        assert_eq!(got, expected, "order=desc must return newest-first");
    }

    // Regression guard for issue #251's relocation of the ordering: `order=desc`
    // moved out of the handler and into `EventStore::query_window`, where it now
    // composes with `offset` and `limit`. The contract is reverse-THEN-skip-THEN-
    // take: `order=desc&offset=1&limit=2` is "the 2nd and 3rd newest". Skipping
    // before reversing (or reversing only the page) returns a different, quietly
    // wrong page — with the same count, total_count and has_more.
    #[tokio::test]
    async fn query_events_desc_composes_with_offset_and_limit() {
        let store = create_test_store();
        let base = chrono::Utc::now();
        let mut ascending_ids = Vec::new();
        for i in 0..5i64 {
            let mut event = create_test_event("org-1", "auth.org.updated");
            event.timestamp = base + chrono::Duration::seconds(i);
            event.version = i + 1;
            ascending_ids.push(event.id);
            store.ingest(&event).unwrap();
        }
        let newest_first: Vec<_> = ascending_ids.iter().rev().copied().collect();

        for (offset, limit) in [(0, 2), (1, 2), (2, 2), (3, 2), (4, 2), (5, 2), (1, 4)] {
            let page = query_page(
                &store,
                &format!("entity_id=org-1&order=desc&offset={offset}&limit={limit}"),
            )
            .await;
            let got: Vec<_> = page.events.iter().map(|e| e.id).collect();
            let expected: Vec<_> = newest_first
                .iter()
                .skip(offset)
                .take(limit)
                .copied()
                .collect();
            assert_eq!(
                got, expected,
                "order=desc&offset={offset}&limit={limit} must reverse, then \
                 skip, then take"
            );
            assert_eq!(page.count, expected.len());
            assert_eq!(page.total_count, 5);
            assert_eq!(
                page.has_more,
                offset + expected.len() < 5,
                "has_more must account for the offset (offset={offset})"
            );
        }

        // Walking the whole entity newest-first must visit every event exactly
        // once — the property a `order=desc` paginator depends on.
        let mut walked = Vec::new();
        for offset in (0..5).step_by(2) {
            let page = query_page(
                &store,
                &format!("entity_id=org-1&order=desc&offset={offset}&limit=2"),
            )
            .await;
            walked.extend(page.events.iter().map(|e| e.id));
        }
        assert_eq!(walked, newest_first, "desc paging must cover the set once");
    }

    #[tokio::test]
    async fn test_list_entities_by_type_prefix() {
        let store = create_test_store();

        // 3 index entities
        store
            .ingest(&create_test_event("idx-1", "index.created"))
            .unwrap();
        store
            .ingest(&create_test_event("idx-1", "index.updated"))
            .unwrap();
        store
            .ingest(&create_test_event("idx-2", "index.created"))
            .unwrap();
        store
            .ingest(&create_test_event("idx-3", "index.created"))
            .unwrap();
        // 2 trade entities
        store
            .ingest(&create_test_event("trade-1", "trade.created"))
            .unwrap();
        store
            .ingest(&create_test_event("trade-2", "trade.created"))
            .unwrap();

        // List entities for index.*
        let req = ListEntitiesRequest {
            event_type_prefix: Some("index.".to_string()),
            ..Default::default()
        };
        let query_req = QueryEventsRequest {
            entity_id: None,
            event_type: None,
            tenant_id: None,
            as_of: None,
            since: None,
            until: None,
            limit: None,
            event_type_prefix: req.event_type_prefix,
            exclude_event_type_prefix: None,
            payload_filter: req.payload_filter,
        };
        let events = store.query(&query_req).unwrap();

        // Group and verify
        let mut entity_map: std::collections::HashMap<String, Vec<&Event>> =
            std::collections::HashMap::new();
        for event in &events {
            entity_map
                .entry(event.entity_id().to_string())
                .or_default()
                .push(event);
        }

        assert_eq!(entity_map.len(), 3); // idx-1, idx-2, idx-3
        assert_eq!(entity_map["idx-1"].len(), 2); // 2 events for idx-1
        assert_eq!(entity_map["idx-2"].len(), 1);
        assert_eq!(entity_map["idx-3"].len(), 1);
    }

    // Issue #178: `list_entities` accepts an `order` param and pages
    // deterministically over the resulting sort.
    #[tokio::test]
    async fn test_list_entities_order_and_pagination() {
        let store = create_test_store();

        // Three entities with strictly increasing last-event times.
        let base = chrono::Utc::now();
        for (i, eid) in ["org-a", "org-b", "org-c"].iter().enumerate() {
            let mut event = create_test_event(eid, "auth.org.created");
            event.timestamp = base + chrono::Duration::seconds(i as i64);
            store.ingest(&event).unwrap();
        }
        let prefix = || Some("auth.org.".to_string());

        // Default: newest activity first (desc) — preserves prior behavior.
        let desc = list_entities(
            State(store.clone()),
            Query(ListEntitiesRequest {
                event_type_prefix: prefix(),
                ..Default::default()
            }),
        )
        .await
        .unwrap();
        let desc_ids: Vec<&str> = desc
            .0
            .entities
            .iter()
            .map(|e| e.entity_id.as_str())
            .collect();
        assert_eq!(desc_ids, ["org-c", "org-b", "org-a"]);

        // order=asc: oldest activity first.
        let asc = list_entities(
            State(store.clone()),
            Query(ListEntitiesRequest {
                event_type_prefix: prefix(),
                order: Some("asc".to_string()),
                ..Default::default()
            }),
        )
        .await
        .unwrap();
        let asc_ids: Vec<&str> = asc
            .0
            .entities
            .iter()
            .map(|e| e.entity_id.as_str())
            .collect();
        assert_eq!(asc_ids, ["org-a", "org-b", "org-c"]);

        // Offset pagination over the deterministic asc order: page 2, size 1.
        let page2 = list_entities(
            State(store.clone()),
            Query(ListEntitiesRequest {
                event_type_prefix: prefix(),
                order: Some("asc".to_string()),
                limit: Some(1),
                offset: Some(1),
                ..Default::default()
            }),
        )
        .await
        .unwrap();
        assert_eq!(page2.0.entities.len(), 1);
        assert_eq!(page2.0.entities[0].entity_id, "org-b");
        assert_eq!(page2.0.total, 3);
        assert!(page2.0.has_more);

        // Invalid order value is rejected.
        let err = list_entities(
            State(store.clone()),
            Query(ListEntitiesRequest {
                event_type_prefix: prefix(),
                order: Some("sideways".to_string()),
                ..Default::default()
            }),
        )
        .await;
        assert!(err.is_err(), "invalid order value must be rejected");
    }

    fn create_test_event_with_payload(
        entity_id: &str,
        event_type: &str,
        payload: serde_json::Value,
    ) -> Event {
        Event::from_strings(
            event_type.to_string(),
            entity_id.to_string(),
            "test-stream".to_string(),
            payload,
            None,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn test_detect_duplicates_by_payload_fields() {
        let store = create_test_store();

        // Create entities with duplicate "name" field values
        store
            .ingest(&create_test_event_with_payload(
                "idx-1",
                "index.created",
                serde_json::json!({"name": "S&P 500", "user_id": "alice"}),
            ))
            .unwrap();
        store
            .ingest(&create_test_event_with_payload(
                "idx-2",
                "index.created",
                serde_json::json!({"name": "S&P 500", "user_id": "bob"}),
            ))
            .unwrap();
        store
            .ingest(&create_test_event_with_payload(
                "idx-3",
                "index.created",
                serde_json::json!({"name": "NASDAQ", "user_id": "alice"}),
            ))
            .unwrap();
        store
            .ingest(&create_test_event_with_payload(
                "idx-4",
                "index.created",
                serde_json::json!({"name": "NASDAQ", "user_id": "carol"}),
            ))
            .unwrap();
        store
            .ingest(&create_test_event_with_payload(
                "idx-5",
                "index.created",
                serde_json::json!({"name": "DAX", "user_id": "dave"}),
            ))
            .unwrap();

        // Group by name — should find 2 groups: "S&P 500" (idx-1, idx-2) and "NASDAQ" (idx-3, idx-4)
        let query_req = QueryEventsRequest {
            entity_id: None,
            event_type: None,
            tenant_id: None,
            as_of: None,
            since: None,
            until: None,
            limit: None,
            event_type_prefix: Some("index.".to_string()),
            exclude_event_type_prefix: None,
            payload_filter: None,
        };
        let events = store.query(&query_req).unwrap();

        // Manually replicate the handler logic for testing
        let group_by_fields = vec!["name"];
        let mut entity_latest: std::collections::HashMap<String, &Event> =
            std::collections::HashMap::new();
        for event in &events {
            let eid = event.entity_id().to_string();
            entity_latest
                .entry(eid)
                .and_modify(|existing| {
                    if event.timestamp() > existing.timestamp() {
                        *existing = event;
                    }
                })
                .or_insert(event);
        }

        let mut groups: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        for (entity_id, event) in &entity_latest {
            let payload = event.payload();
            let mut key_parts = serde_json::Map::new();
            for field in &group_by_fields {
                let value = payload
                    .get(*field)
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                key_parts.insert((*field).to_string(), value);
            }
            let key_str = serde_json::to_string(&key_parts).unwrap_or_default();
            groups.entry(key_str).or_default().push(entity_id.clone());
        }

        let duplicate_groups: Vec<_> = groups
            .into_iter()
            .filter(|(_, ids)| ids.len() > 1)
            .collect();

        assert_eq!(duplicate_groups.len(), 2); // S&P 500 and NASDAQ groups
        for (_, ids) in &duplicate_groups {
            assert_eq!(ids.len(), 2);
        }
    }

    #[tokio::test]
    async fn test_detect_duplicates_no_duplicates() {
        let store = create_test_store();

        // All unique names
        store
            .ingest(&create_test_event_with_payload(
                "idx-1",
                "index.created",
                serde_json::json!({"name": "A"}),
            ))
            .unwrap();
        store
            .ingest(&create_test_event_with_payload(
                "idx-2",
                "index.created",
                serde_json::json!({"name": "B"}),
            ))
            .unwrap();

        let query_req = QueryEventsRequest {
            entity_id: None,
            event_type: None,
            tenant_id: None,
            as_of: None,
            since: None,
            until: None,
            limit: None,
            event_type_prefix: Some("index.".to_string()),
            exclude_event_type_prefix: None,
            payload_filter: None,
        };
        let events = store.query(&query_req).unwrap();

        let mut entity_latest: std::collections::HashMap<String, &Event> =
            std::collections::HashMap::new();
        for event in &events {
            entity_latest
                .entry(event.entity_id().to_string())
                .or_insert(event);
        }

        let mut groups: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        for (entity_id, event) in &entity_latest {
            let key_str =
                serde_json::to_string(&serde_json::json!({"name": event.payload().get("name")}))
                    .unwrap();
            groups.entry(key_str).or_default().push(entity_id.clone());
        }

        let duplicate_groups: Vec<_> = groups
            .into_iter()
            .filter(|(_, ids)| ids.len() > 1)
            .collect();

        assert_eq!(duplicate_groups.len(), 0); // No duplicates
    }

    #[tokio::test]
    async fn test_detect_duplicates_multi_field_group_by() {
        let store = create_test_store();

        // Two entities with same name AND user_id = true duplicate
        store
            .ingest(&create_test_event_with_payload(
                "idx-1",
                "index.created",
                serde_json::json!({"name": "S&P 500", "user_id": "alice"}),
            ))
            .unwrap();
        store
            .ingest(&create_test_event_with_payload(
                "idx-2",
                "index.created",
                serde_json::json!({"name": "S&P 500", "user_id": "alice"}),
            ))
            .unwrap();
        // Same name but different user_id = NOT a duplicate in multi-field group
        store
            .ingest(&create_test_event_with_payload(
                "idx-3",
                "index.created",
                serde_json::json!({"name": "S&P 500", "user_id": "bob"}),
            ))
            .unwrap();

        let query_req = QueryEventsRequest {
            entity_id: None,
            event_type: None,
            tenant_id: None,
            as_of: None,
            since: None,
            until: None,
            limit: None,
            event_type_prefix: Some("index.".to_string()),
            exclude_event_type_prefix: None,
            payload_filter: None,
        };
        let events = store.query(&query_req).unwrap();

        let group_by_fields = vec!["name", "user_id"];
        let mut entity_latest: std::collections::HashMap<String, &Event> =
            std::collections::HashMap::new();
        for event in &events {
            entity_latest
                .entry(event.entity_id().to_string())
                .and_modify(|existing| {
                    if event.timestamp() > existing.timestamp() {
                        *existing = event;
                    }
                })
                .or_insert(event);
        }

        let mut groups: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        for (entity_id, event) in &entity_latest {
            let payload = event.payload();
            let mut key_parts = serde_json::Map::new();
            for field in &group_by_fields {
                let value = payload
                    .get(*field)
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                key_parts.insert((*field).to_string(), value);
            }
            let key_str = serde_json::to_string(&key_parts).unwrap_or_default();
            groups.entry(key_str).or_default().push(entity_id.clone());
        }

        let duplicate_groups: Vec<_> = groups
            .into_iter()
            .filter(|(_, ids)| ids.len() > 1)
            .collect();

        // Only 1 duplicate group: name=S&P 500, user_id=alice (idx-1, idx-2)
        assert_eq!(duplicate_groups.len(), 1);
        let (_, ref ids) = duplicate_groups[0];
        assert_eq!(ids.len(), 2);
        let mut sorted_ids = ids.clone();
        sorted_ids.sort();
        assert_eq!(sorted_ids, vec!["idx-1", "idx-2"]);
    }

    #[tokio::test]
    async fn test_projection_state_cache() {
        let store = create_test_store();

        // Test cache insertion
        let cache = store.projection_state_cache();
        cache.insert(
            "entity_snapshots:user-123".to_string(),
            serde_json::json!({"name": "Test User", "age": 30}),
        );

        // Test cache retrieval
        let state = cache.get("entity_snapshots:user-123");
        assert!(state.is_some());
        let state = state.unwrap();
        assert_eq!(state["name"], "Test User");
        assert_eq!(state["age"], 30);
    }

    #[tokio::test]
    async fn test_projection_manager_list_projections() {
        let store = create_test_store();

        // List projections (built-in projections should be available)
        let projection_manager = store.projection_manager();
        let projections = projection_manager.list_projections();

        // Should have entity_snapshots and event_counters
        assert!(projections.len() >= 2);

        let names: Vec<&str> = projections.iter().map(|(name, _)| name.as_str()).collect();
        assert!(names.contains(&"entity_snapshots"));
        assert!(names.contains(&"event_counters"));
    }

    #[tokio::test]
    async fn test_projection_state_after_event_ingestion() {
        let store = create_test_store();

        // Ingest an event
        let event = create_test_event("user-456", "user.created");
        store.ingest(&event).unwrap();

        // Get projection state
        let projection_manager = store.projection_manager();
        let snapshot_projection = projection_manager
            .get_projection("entity_snapshots")
            .unwrap();

        let state = snapshot_projection.get_state("user-456");
        assert!(state.is_some());
        let state = state.unwrap();
        assert_eq!(state["name"], "Test");
        assert_eq!(state["value"], 42);
    }

    #[tokio::test]
    async fn test_projection_state_cache_multiple_entities() {
        let store = create_test_store();
        let cache = store.projection_state_cache();

        // Insert multiple entities
        for i in 0..10 {
            cache.insert(
                format!("entity_snapshots:entity-{i}"),
                serde_json::json!({"id": i, "status": "active"}),
            );
        }

        // Verify all insertions
        assert_eq!(cache.len(), 10);

        // Verify each entity
        for i in 0..10 {
            let key = format!("entity_snapshots:entity-{i}");
            let state = cache.get(&key);
            assert!(state.is_some());
            assert_eq!(state.unwrap()["id"], i);
        }
    }

    #[tokio::test]
    async fn test_projection_state_update() {
        let store = create_test_store();
        let cache = store.projection_state_cache();

        // Initial state
        cache.insert(
            "entity_snapshots:user-789".to_string(),
            serde_json::json!({"balance": 100}),
        );

        // Update state
        cache.insert(
            "entity_snapshots:user-789".to_string(),
            serde_json::json!({"balance": 150}),
        );

        // Verify update
        let state = cache.get("entity_snapshots:user-789").unwrap();
        assert_eq!(state["balance"], 150);
    }

    #[tokio::test]
    async fn test_event_counter_projection() {
        let store = create_test_store();

        // Ingest events of different types
        store
            .ingest(&create_test_event("user-1", "user.created"))
            .unwrap();
        store
            .ingest(&create_test_event("user-2", "user.created"))
            .unwrap();
        store
            .ingest(&create_test_event("user-1", "user.updated"))
            .unwrap();

        // Get event counter projection
        let projection_manager = store.projection_manager();
        let counter_projection = projection_manager.get_projection("event_counters").unwrap();

        // Check counts
        let created_state = counter_projection.get_state("user.created");
        assert!(created_state.is_some());
        assert_eq!(created_state.unwrap()["count"], 2);

        let updated_state = counter_projection.get_state("user.updated");
        assert!(updated_state.is_some());
        assert_eq!(updated_state.unwrap()["count"], 1);
    }

    #[tokio::test]
    async fn test_projection_state_cache_key_format() {
        let store = create_test_store();
        let cache = store.projection_state_cache();

        // Test standard key format: {projection_name}:{entity_id}
        let key = "orders:order-12345".to_string();
        cache.insert(key.clone(), serde_json::json!({"total": 99.99}));

        let state = cache.get(&key).unwrap();
        assert_eq!(state["total"], 99.99);
    }

    #[tokio::test]
    async fn test_projection_state_cache_removal() {
        let store = create_test_store();
        let cache = store.projection_state_cache();

        // Insert and then remove
        cache.insert(
            "test:entity-1".to_string(),
            serde_json::json!({"data": "value"}),
        );
        assert_eq!(cache.len(), 1);

        cache.remove("test:entity-1");
        assert_eq!(cache.len(), 0);
        assert!(cache.get("test:entity-1").is_none());
    }

    #[tokio::test]
    async fn test_get_nonexistent_projection() {
        let store = create_test_store();
        let projection_manager = store.projection_manager();

        // Requesting a non-existent projection should return None
        let projection = projection_manager.get_projection("nonexistent_projection");
        assert!(projection.is_none());
    }

    #[tokio::test]
    async fn test_get_nonexistent_entity_state() {
        let store = create_test_store();
        let projection_manager = store.projection_manager();

        // Get state for non-existent entity
        let snapshot_projection = projection_manager
            .get_projection("entity_snapshots")
            .unwrap();
        let state = snapshot_projection.get_state("nonexistent-entity-xyz");
        assert!(state.is_none());
    }

    #[tokio::test]
    async fn test_projection_state_cache_concurrent_access() {
        let store = create_test_store();
        let cache = store.projection_state_cache();

        // Simulate concurrent writes
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let cache_clone = cache.clone();
                tokio::spawn(async move {
                    cache_clone.insert(
                        format!("concurrent:entity-{i}"),
                        serde_json::json!({"thread": i}),
                    );
                })
            })
            .collect();

        for handle in handles {
            handle.await.unwrap();
        }

        // All 10 entries should be present
        assert_eq!(cache.len(), 10);
    }

    #[tokio::test]
    async fn test_projection_state_large_payload() {
        let store = create_test_store();
        let cache = store.projection_state_cache();

        // Create a large JSON payload (~10KB)
        let large_array: Vec<serde_json::Value> = (0..1000)
            .map(|i| serde_json::json!({"item": i, "description": "test item with some padding data to increase size"}))
            .collect();

        cache.insert(
            "large:entity-1".to_string(),
            serde_json::json!({"items": large_array}),
        );

        let state = cache.get("large:entity-1").unwrap();
        let items = state["items"].as_array().unwrap();
        assert_eq!(items.len(), 1000);
    }

    #[tokio::test]
    async fn test_projection_state_complex_json() {
        let store = create_test_store();
        let cache = store.projection_state_cache();

        // Complex nested JSON structure
        let complex_state = serde_json::json!({
            "user": {
                "id": "user-123",
                "profile": {
                    "name": "John Doe",
                    "email": "john@example.com",
                    "settings": {
                        "theme": "dark",
                        "notifications": true
                    }
                },
                "roles": ["admin", "user"],
                "metadata": {
                    "created_at": "2025-01-01T00:00:00Z",
                    "last_login": null
                }
            }
        });

        cache.insert("complex:user-123".to_string(), complex_state);

        let state = cache.get("complex:user-123").unwrap();
        assert_eq!(state["user"]["profile"]["name"], "John Doe");
        assert_eq!(state["user"]["roles"][0], "admin");
        assert!(state["user"]["metadata"]["last_login"].is_null());
    }

    #[tokio::test]
    async fn test_projection_state_cache_iteration() {
        let store = create_test_store();
        let cache = store.projection_state_cache();

        // Insert entries
        for i in 0..5 {
            cache.insert(format!("iter:entity-{i}"), serde_json::json!({"index": i}));
        }

        // Iterate over all entries
        let entries: Vec<_> = cache.iter().map(|entry| entry.key().clone()).collect();
        assert_eq!(entries.len(), 5);
    }

    #[tokio::test]
    async fn test_projection_manager_get_entity_snapshots() {
        let store = create_test_store();
        let projection_manager = store.projection_manager();

        // Get entity_snapshots projection specifically
        let projection = projection_manager.get_projection("entity_snapshots");
        assert!(projection.is_some());
        assert_eq!(projection.unwrap().name(), "entity_snapshots");
    }

    #[tokio::test]
    async fn test_projection_manager_get_event_counters() {
        let store = create_test_store();
        let projection_manager = store.projection_manager();

        // Get event_counters projection specifically
        let projection = projection_manager.get_projection("event_counters");
        assert!(projection.is_some());
        assert_eq!(projection.unwrap().name(), "event_counters");
    }

    #[tokio::test]
    async fn test_projection_state_cache_overwrite() {
        let store = create_test_store();
        let cache = store.projection_state_cache();

        // Initial value
        cache.insert(
            "overwrite:entity-1".to_string(),
            serde_json::json!({"version": 1}),
        );

        // Overwrite with new value
        cache.insert(
            "overwrite:entity-1".to_string(),
            serde_json::json!({"version": 2}),
        );

        // Overwrite again
        cache.insert(
            "overwrite:entity-1".to_string(),
            serde_json::json!({"version": 3}),
        );

        let state = cache.get("overwrite:entity-1").unwrap();
        assert_eq!(state["version"], 3);

        // Should still be only 1 entry
        assert_eq!(cache.len(), 1);
    }

    #[tokio::test]
    async fn test_projection_state_multiple_projections() {
        let store = create_test_store();
        let cache = store.projection_state_cache();

        // Store states for different projections
        cache.insert(
            "entity_snapshots:user-1".to_string(),
            serde_json::json!({"name": "Alice"}),
        );
        cache.insert(
            "event_counters:user.created".to_string(),
            serde_json::json!({"count": 5}),
        );
        cache.insert(
            "custom_projection:order-1".to_string(),
            serde_json::json!({"total": 150.0}),
        );

        // Verify each projection's state
        assert_eq!(
            cache.get("entity_snapshots:user-1").unwrap()["name"],
            "Alice"
        );
        assert_eq!(
            cache.get("event_counters:user.created").unwrap()["count"],
            5
        );
        assert_eq!(
            cache.get("custom_projection:order-1").unwrap()["total"],
            150.0
        );
    }

    #[tokio::test]
    async fn test_bulk_projection_state_access() {
        let store = create_test_store();

        // Ingest multiple events for different entities
        for i in 0..5 {
            let event = create_test_event(&format!("bulk-user-{i}"), "user.created");
            store.ingest(&event).unwrap();
        }

        // Get projection and verify bulk access
        let projection_manager = store.projection_manager();
        let snapshot_projection = projection_manager
            .get_projection("entity_snapshots")
            .unwrap();

        // Verify we can access all entities
        for i in 0..5 {
            let state = snapshot_projection.get_state(&format!("bulk-user-{i}"));
            assert!(state.is_some(), "Entity bulk-user-{i} should have state");
        }
    }

    #[tokio::test]
    async fn test_bulk_save_projection_states() {
        let store = create_test_store();
        let cache = store.projection_state_cache();

        // Simulate bulk save request
        let states = vec![
            BulkSaveStateItem {
                entity_id: "bulk-entity-1".to_string(),
                state: serde_json::json!({"name": "Entity 1", "value": 100}),
            },
            BulkSaveStateItem {
                entity_id: "bulk-entity-2".to_string(),
                state: serde_json::json!({"name": "Entity 2", "value": 200}),
            },
            BulkSaveStateItem {
                entity_id: "bulk-entity-3".to_string(),
                state: serde_json::json!({"name": "Entity 3", "value": 300}),
            },
        ];

        let projection_name = "test_projection";

        // Save states to cache (simulating bulk_save_projection_states handler)
        for item in &states {
            cache.insert(
                format!("{projection_name}:{}", item.entity_id),
                item.state.clone(),
            );
        }

        // Verify all states were saved
        assert_eq!(cache.len(), 3);

        let state1 = cache.get("test_projection:bulk-entity-1").unwrap();
        assert_eq!(state1["name"], "Entity 1");
        assert_eq!(state1["value"], 100);

        let state2 = cache.get("test_projection:bulk-entity-2").unwrap();
        assert_eq!(state2["name"], "Entity 2");
        assert_eq!(state2["value"], 200);

        let state3 = cache.get("test_projection:bulk-entity-3").unwrap();
        assert_eq!(state3["name"], "Entity 3");
        assert_eq!(state3["value"], 300);
    }

    #[tokio::test]
    async fn test_bulk_save_empty_states() {
        let store = create_test_store();
        let cache = store.projection_state_cache();

        // Clear cache
        cache.clear();

        // Empty states should work fine
        let states: Vec<BulkSaveStateItem> = vec![];
        assert_eq!(states.len(), 0);

        // Cache should remain empty
        assert_eq!(cache.len(), 0);
    }

    #[tokio::test]
    async fn test_bulk_save_overwrites_existing() {
        let store = create_test_store();
        let cache = store.projection_state_cache();

        // Insert initial state
        cache.insert(
            "test:entity-1".to_string(),
            serde_json::json!({"version": 1, "data": "initial"}),
        );

        // Bulk save with updated state
        let new_state = serde_json::json!({"version": 2, "data": "updated"});
        cache.insert("test:entity-1".to_string(), new_state);

        // Verify overwrite
        let state = cache.get("test:entity-1").unwrap();
        assert_eq!(state["version"], 2);
        assert_eq!(state["data"], "updated");
    }

    #[tokio::test]
    async fn test_bulk_save_high_volume() {
        let store = create_test_store();
        let cache = store.projection_state_cache();

        // Simulate high volume save (1000 entities)
        for i in 0..1000 {
            cache.insert(
                format!("volume_test:entity-{i}"),
                serde_json::json!({"index": i, "status": "active"}),
            );
        }

        // Verify count
        assert_eq!(cache.len(), 1000);

        // Spot check some entries
        assert_eq!(cache.get("volume_test:entity-0").unwrap()["index"], 0);
        assert_eq!(cache.get("volume_test:entity-500").unwrap()["index"], 500);
        assert_eq!(cache.get("volume_test:entity-999").unwrap()["index"], 999);
    }

    #[tokio::test]
    async fn test_bulk_save_different_projections() {
        let store = create_test_store();
        let cache = store.projection_state_cache();

        // Save to multiple projections in bulk
        let projections = ["entity_snapshots", "event_counters", "custom_analytics"];

        for proj in &projections {
            for i in 0..5 {
                cache.insert(
                    format!("{proj}:entity-{i}"),
                    serde_json::json!({"projection": proj, "id": i}),
                );
            }
        }

        // Verify total count (3 projections * 5 entities)
        assert_eq!(cache.len(), 15);

        // Verify each projection
        for proj in &projections {
            let state = cache.get(&format!("{proj}:entity-0")).unwrap();
            assert_eq!(state["projection"], *proj);
        }
    }

    // -------------------------------------------------------------------------
    // Cache-fallback tests for the projection-state read handlers (v0.19.1).
    //
    // SDK-managed projections write state via save_projection_state /
    // bulk_save_projection_states without registering in projection_manager.
    // These tests verify the read handlers fall back to the cache instead of
    // 404-ing. Registered projection wins where both exist; cache fills the
    // gap when not.
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn get_projection_state_falls_back_to_cache_when_unregistered() {
        let store = create_test_store();
        store.projection_state_cache().insert(
            "assets:BTC".to_string(),
            serde_json::json!({"symbol": "BTC", "altname": "Bitcoin"}),
        );

        let resp = get_projection_state(
            State(Arc::clone(&store)),
            Path(("assets".to_string(), "BTC".to_string())),
        )
        .await
        .expect("should not error when projection is not registered");

        assert_eq!(resp.0["found"], serde_json::Value::Bool(true));
        assert_eq!(resp.0["state"]["symbol"], "BTC");
        assert_eq!(resp.0["state"]["altname"], "Bitcoin");
    }

    #[tokio::test]
    async fn get_projection_state_returns_not_found_when_absent_everywhere() {
        let store = create_test_store();

        let resp = get_projection_state(
            State(Arc::clone(&store)),
            Path(("assets".to_string(), "UNKNOWN".to_string())),
        )
        .await
        .unwrap();

        assert_eq!(resp.0["found"], serde_json::Value::Bool(false));
        assert_eq!(resp.0["state"], serde_json::Value::Null);
    }

    #[tokio::test]
    async fn get_projection_state_registered_wins_over_cache() {
        let store = create_test_store();

        // Ingest an event so entity_snapshots (a registered projection) has state.
        let event = create_test_event("user-777", "user.created");
        store.ingest(&event).unwrap();

        // Plant a conflicting cache entry for the same (projection, entity).
        store.projection_state_cache().insert(
            "entity_snapshots:user-777".to_string(),
            serde_json::json!({"stolen": "value"}),
        );

        let resp = get_projection_state(
            State(Arc::clone(&store)),
            Path(("entity_snapshots".to_string(), "user-777".to_string())),
        )
        .await
        .unwrap();

        // Registered projection wins — cache fallback is only consulted when
        // the registered projection has no state for this entity.
        assert_eq!(resp.0["found"], serde_json::Value::Bool(true));
        assert!(
            resp.0["state"].get("stolen").is_none(),
            "cache entry must not shadow registered projection state: got {:?}",
            resp.0["state"]
        );
    }

    #[tokio::test]
    async fn get_projection_state_summary_returns_cache_without_registration() {
        let store = create_test_store();
        let cache = store.projection_state_cache();
        cache.insert("assets:BTC".into(), serde_json::json!({"symbol": "BTC"}));
        cache.insert("assets:ETH".into(), serde_json::json!({"symbol": "ETH"}));
        // Different projection name — must not appear in the summary.
        cache.insert("trades:t-1".into(), serde_json::json!({"x": 1}));

        let resp = get_projection_state_summary(
            State(Arc::clone(&store)),
            Path("assets".to_string()),
            Query(ProjectionStateSummaryParams::default()),
        )
        .await
        .unwrap();

        assert_eq!(resp.0["total"], 2);
        let states = resp.0["states"].as_array().unwrap();
        let entity_ids: Vec<&str> = states
            .iter()
            .map(|s| s["entity_id"].as_str().unwrap())
            .collect();
        assert!(entity_ids.contains(&"BTC"));
        assert!(entity_ids.contains(&"ETH"));
    }

    #[tokio::test]
    async fn bulk_get_projection_states_falls_back_to_cache() {
        let store = create_test_store();
        let cache = store.projection_state_cache();
        cache.insert("assets:BTC".into(), serde_json::json!({"symbol": "BTC"}));
        cache.insert("assets:ETH".into(), serde_json::json!({"symbol": "ETH"}));

        let req = BulkGetStateRequest {
            entity_ids: vec!["BTC".into(), "ETH".into(), "MISSING".into()],
        };

        let resp = bulk_get_projection_states(
            State(Arc::clone(&store)),
            Path("assets".to_string()),
            Json(req),
        )
        .await
        .unwrap();

        assert_eq!(resp.0["total"], 3);
        let states = resp.0["states"].as_array().unwrap();
        let by_id: std::collections::HashMap<&str, &serde_json::Value> = states
            .iter()
            .map(|s| (s["entity_id"].as_str().unwrap(), s))
            .collect();

        assert_eq!(by_id["BTC"]["found"], serde_json::Value::Bool(true));
        assert_eq!(by_id["BTC"]["state"]["symbol"], "BTC");
        assert_eq!(by_id["ETH"]["found"], serde_json::Value::Bool(true));
        assert_eq!(by_id["MISSING"]["found"], serde_json::Value::Bool(false));
    }

    /// Pins the poll wire shape the Rust SDK's `poll_consumer_events` decodes:
    /// `ConsumerEventDto` flattens the event, so its fields sit next to
    /// `position` rather than nested under an `event` key.
    #[tokio::test]
    async fn poll_consumer_events_flattens_event_alongside_position() {
        let store = create_test_store();
        store
            .ingest(&create_test_event("user-1", "user.created"))
            .unwrap();
        store
            .ingest(&create_test_event("user-2", "user.updated"))
            .unwrap();
        store.consumer_registry().register("w1", &[]);

        let resp = poll_consumer_events(
            State(Arc::clone(&store)),
            Path("w1".to_string()),
            Query(ConsumerPollQuery { limit: Some(10) }),
        )
        .await
        .unwrap();

        let body = serde_json::to_value(&resp.0).unwrap();
        assert_eq!(body["count"], 2);
        let first = &body["events"][0];
        assert_eq!(first["position"], 1);
        assert!(
            first.get("event").is_none(),
            "event must be flattened, not nested: got {first:?}"
        );
        assert_eq!(first["event_type"], "user.created");
        assert_eq!(first["entity_id"], "user-1");
    }
}
