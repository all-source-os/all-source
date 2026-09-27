/// v1.0 API router with authentication and multi-tenancy
use crate::infrastructure::di::ServiceContainer;
#[cfg(feature = "multi-tenant")]
use crate::infrastructure::web::tenant_api::*;
use crate::{
    domain::repositories::TenantRepository,
    infrastructure::{
        cluster::{
            ClusterManager, ClusterMember, GeoReplicationManager, GeoSyncRequest, VoteRequest,
        },
        replication::{WalReceiver, WalShipper},
        security::{
            auth::AuthManager,
            middleware::{AuthState, RateLimitState, auth_middleware, rate_limit_middleware},
            rate_limit::RateLimiter,
        },
        web::{audit_api::*, auth_api::*, config_api::*},
    },
    store::EventStore,
};
use axum::{
    Json, Router,
    extract::{Path, State},
    middleware,
    response::IntoResponse,
    routing::{delete, get, post, put},
};
use std::sync::Arc;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};

/// Node role for leader-follower replication
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeRole {
    Leader,
    Follower,
}

impl NodeRole {
    /// Detect role from environment variables.
    ///
    /// Checks `ALLSOURCE_ROLE` ("leader" or "follower") first,
    /// then falls back to `ALLSOURCE_READ_ONLY` ("true" → follower).
    /// Defaults to `Leader` if neither is set.
    pub fn from_env() -> Self {
        if let Ok(role) = std::env::var("ALLSOURCE_ROLE") {
            match role.to_lowercase().as_str() {
                "follower" => return NodeRole::Follower,
                "leader" => return NodeRole::Leader,
                other => {
                    tracing::warn!(
                        "Unknown ALLSOURCE_ROLE value '{}', defaulting to leader",
                        other
                    );
                    return NodeRole::Leader;
                }
            }
        }
        if let Ok(read_only) = std::env::var("ALLSOURCE_READ_ONLY")
            && (read_only == "true" || read_only == "1")
        {
            return NodeRole::Follower;
        }
        NodeRole::Leader
    }

    pub fn is_follower(self) -> bool {
        self == NodeRole::Follower
    }

    fn to_u8(self) -> u8 {
        match self {
            NodeRole::Leader => 0,
            NodeRole::Follower => 1,
        }
    }

    fn from_u8(v: u8) -> Self {
        match v {
            1 => NodeRole::Follower,
            _ => NodeRole::Leader,
        }
    }
}

impl std::fmt::Display for NodeRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NodeRole::Leader => write!(f, "leader"),
            NodeRole::Follower => write!(f, "follower"),
        }
    }
}

/// Thread-safe, runtime-mutable node role for failover support.
///
/// Wraps an `AtomicU8` so the read-only middleware and health endpoint
/// always see the current role, even after a sentinel-triggered promotion.
#[derive(Clone)]
pub struct AtomicNodeRole(Arc<std::sync::atomic::AtomicU8>);

impl AtomicNodeRole {
    pub fn new(role: NodeRole) -> Self {
        Self(Arc::new(std::sync::atomic::AtomicU8::new(role.to_u8())))
    }

    pub fn load(&self) -> NodeRole {
        NodeRole::from_u8(self.0.load(std::sync::atomic::Ordering::Relaxed))
    }

    pub fn store(&self, role: NodeRole) {
        self.0
            .store(role.to_u8(), std::sync::atomic::Ordering::Relaxed);
    }
}

/// Unified application state for all handlers
#[derive(Clone)]
pub struct AppState {
    pub store: Arc<EventStore>,
    pub auth_manager: Arc<AuthManager>,
    pub tenant_repo: Arc<dyn TenantRepository>,
    /// Service container for paywall domain use cases (Creator, Article, Payment, etc.)
    pub service_container: ServiceContainer,
    /// Node role for leader-follower replication (runtime-mutable for failover)
    pub role: AtomicNodeRole,
    /// WAL shipper for replication status reporting (leader only).
    /// Wrapped in RwLock so a promoted follower can install a shipper at runtime.
    pub wal_shipper: Arc<tokio::sync::RwLock<Option<Arc<WalShipper>>>>,
    /// WAL receiver for replication status reporting (follower only)
    pub wal_receiver: Option<Arc<WalReceiver>>,
    /// Replication port used by the WAL shipper (needed for runtime promotion)
    pub replication_port: u16,
    /// Cluster manager for multi-node consensus and membership (optional)
    pub cluster_manager: Option<Arc<ClusterManager>>,
    /// Geo-replication manager for cross-region replication (optional)
    pub geo_replication: Option<Arc<GeoReplicationManager>>,
}

// Enable extracting Arc<EventStore> from AppState
// This allows handlers that expect State<Arc<EventStore>> to work with AppState
impl axum::extract::FromRef<AppState> for Arc<EventStore> {
    fn from_ref(state: &AppState) -> Self {
        state.store.clone()
    }
}

pub async fn serve_v1(
    store: Arc<EventStore>,
    auth_manager: Arc<AuthManager>,
    tenant_repo: Arc<dyn TenantRepository>,
    rate_limiter: Arc<RateLimiter>,
    service_container: ServiceContainer,
    addr: &str,
    role: NodeRole,
    wal_shipper: Option<Arc<WalShipper>>,
    wal_receiver: Option<Arc<WalReceiver>>,
    replication_port: u16,
    cluster_manager: Option<Arc<ClusterManager>>,
    geo_replication: Option<Arc<GeoReplicationManager>>,
) -> anyhow::Result<()> {
    let app_state = AppState {
        store,
        auth_manager: auth_manager.clone(),
        tenant_repo,
        service_container,
        role: AtomicNodeRole::new(role),
        wal_shipper: Arc::new(tokio::sync::RwLock::new(wal_shipper)),
        wal_receiver,
        replication_port,
        cluster_manager,
        geo_replication,
    };

    let auth_state = AuthState {
        auth_manager: auth_manager.clone(),
    };

    let rate_limit_state = RateLimitState { rate_limiter };

    let app = Router::new()
        // Public routes (no auth)
        .route("/health", get(health_v1))
        .route("/metrics", get(super::api::prometheus_metrics))
        // Auth routes
        .route("/api/v1/auth/register", post(register_handler))
        .route("/api/v1/auth/login", post(login_handler))
        .route("/api/v1/auth/me", get(me_handler))
        .route("/api/v1/auth/api-keys", post(create_api_key_handler))
        .route("/api/v1/auth/api-keys", get(list_api_keys_handler))
        .route("/api/v1/auth/api-keys/{id}", delete(revoke_api_key_handler))
        .route("/api/v1/auth/users", get(list_users_handler))
        .route("/api/v1/auth/users/{id}", delete(delete_user_handler))
        // Tenant routes (protected — enterprise only)
        ;
    #[cfg(feature = "multi-tenant")]
    let app = app
        .route("/api/v1/tenants", post(create_tenant_handler))
        .route("/api/v1/tenants", get(list_tenants_handler))
        .route("/api/v1/tenants/{id}", get(get_tenant_handler))
        .route("/api/v1/tenants/{id}", put(update_tenant_handler))
        .route(
            "/api/v1/tenants/{id}/metadata",
            // Qualified path: `patch` is only used here, inside the
            // `multi-tenant`-gated block, so importing it unconditionally would
            // warn unused under the community edition.
            axum::routing::patch(merge_tenant_metadata_handler),
        )
        .route("/api/v1/tenants/{id}/stats", get(get_tenant_stats_handler))
        .route("/api/v1/tenants/{id}/quotas", put(update_quotas_handler))
        .route(
            "/api/v1/tenants/{id}/usage/increment",
            post(increment_usage_handler),
        )
        .route(
            "/api/v1/tenants/{id}/usage/queries/admit",
            post(super::tenant_query_usage_api::admit_query_usage_handler)
                .layer(axum::extract::DefaultBodyLimit::max(2_048)),
        )
        .route(
            "/api/v1/tenants/{id}/usage/queries",
            get(super::tenant_query_usage_api::query_usage_handler),
        )
        .route(
            "/api/v1/tenants/{id}/usage/queries/reset",
            post(super::tenant_query_usage_api::reset_query_usage_handler)
                .layer(axum::extract::DefaultBodyLimit::max(2_048)),
        )
        .route(
            "/api/v1/tenants/{id}/schema-enforcement",
            put(update_schema_enforcement_handler),
        )
        .route(
            "/api/v1/tenants/{id}/deactivate",
            post(deactivate_tenant_handler),
        )
        .route(
            "/api/v1/tenants/{id}/activate",
            post(activate_tenant_handler),
        )
        .route("/api/v1/tenants/{id}", delete(delete_tenant_handler));
    let app = app
        // Audit endpoints (admin only)
        .route("/api/v1/audit/events", post(log_audit_event))
        .route("/api/v1/audit/events", get(query_audit_events))
        // Config endpoints (admin only)
        .route("/api/v1/config", get(list_configs))
        .route("/api/v1/config", post(set_config))
        .route(
            "/api/v1/config/conditional/set",
            post(set_config_conditionally),
        )
        .route("/api/v1/config/{key}", get(get_config))
        .route("/api/v1/config/{key}", put(update_config))
        .route("/api/v1/config/{key}", delete(delete_config))
        // Demo seeding
        .route(
            "/api/v1/demo/seed",
            post(super::demo_api::demo_seed_handler),
        )
        // Event and data routes (protected by auth)
        .route("/api/v1/events", post(super::api::ingest_event_v1))
        .route(
            "/api/v1/events/batch",
            post(super::api::ingest_events_batch_v1),
        )
        .route("/api/v1/events/query", get(super::api::query_events))
        .route(
            "/api/v1/events/{event_id}",
            get(super::api::get_event_by_id),
        )
        .route("/api/v1/events/stream", get(super::api::events_websocket))
        .route("/api/v1/entities", get(super::api::list_entities))
        .route(
            "/api/v1/entities/duplicates",
            get(super::api::detect_duplicates),
        )
        .route(
            "/api/v1/entities/{entity_id}/state",
            get(super::api::get_entity_state),
        )
        .route(
            "/api/v1/entities/{entity_id}/snapshot",
            get(super::api::get_entity_snapshot),
        )
        .route("/api/v1/stats", get(super::api::get_stats))
        // v0.10: Stream and event type discovery endpoints
        .route("/api/v1/streams", get(super::api::list_streams))
        .route("/api/v1/event-types", get(super::api::list_event_types))
        // Analytics
        .route(
            "/api/v1/analytics/frequency",
            get(super::api::analytics_frequency),
        )
        .route(
            "/api/v1/analytics/summary",
            get(super::api::analytics_summary),
        )
        .route(
            "/api/v1/analytics/correlation",
            get(super::api::analytics_correlation),
        )
        // Snapshots
        .route("/api/v1/snapshots", post(super::api::create_snapshot))
        .route("/api/v1/snapshots", get(super::api::list_snapshots))
        .route(
            "/api/v1/snapshots/{entity_id}/latest",
            get(super::api::get_latest_snapshot),
        )
        // Compaction
        .route(
            "/api/v1/compaction/trigger",
            post(super::api::trigger_compaction),
        )
        .route(
            "/api/v1/compaction/stats",
            get(super::api::compaction_stats),
        )
        // Schemas
        .route("/api/v1/schemas", post(super::api::register_schema))
        .route("/api/v1/schemas", get(super::api::list_subjects))
        .route("/api/v1/schemas/{subject}", get(super::api::get_schema))
        .route(
            "/api/v1/schemas/{subject}/versions",
            get(super::api::list_schema_versions),
        )
        .route(
            "/api/v1/schemas/validate",
            post(super::api::validate_event_schema),
        )
        .route(
            "/api/v1/schemas/{subject}/compatibility",
            put(super::api::set_compatibility_mode),
        )
        // Replay
        .route("/api/v1/replay", post(super::api::start_replay))
        .route("/api/v1/replay", get(super::api::list_replays))
        .route(
            "/api/v1/replay/{replay_id}",
            get(super::api::get_replay_progress),
        )
        .route(
            "/api/v1/replay/{replay_id}/cancel",
            post(super::api::cancel_replay),
        )
        .route(
            "/api/v1/replay/{replay_id}",
            delete(super::api::delete_replay),
        )
        // Pipelines
        .route("/api/v1/pipelines", post(super::api::register_pipeline))
        .route("/api/v1/pipelines", get(super::api::list_pipelines))
        .route(
            "/api/v1/pipelines/stats",
            get(super::api::all_pipeline_stats),
        )
        .route(
            "/api/v1/pipelines/{pipeline_id}",
            get(super::api::get_pipeline),
        )
        .route(
            "/api/v1/pipelines/{pipeline_id}",
            delete(super::api::remove_pipeline),
        )
        .route(
            "/api/v1/pipelines/{pipeline_id}/stats",
            get(super::api::get_pipeline_stats),
        )
        .route(
            "/api/v1/pipelines/{pipeline_id}/reset",
            put(super::api::reset_pipeline),
        )
        // v0.7: Projection State API for Query Service integration
        .route("/api/v1/projections", get(super::api::list_projections))
        .route(
            "/api/v1/projections/{name}",
            get(super::api::get_projection),
        )
        .route(
            "/api/v1/projections/{name}",
            delete(super::api::delete_projection),
        )
        .route(
            "/api/v1/projections/{name}/state",
            get(super::api::get_projection_state_summary),
        )
        .route(
            "/api/v1/projections/{name}/reset",
            post(super::api::reset_projection),
        )
        .route(
            "/api/v1/projections/{name}/pause",
            post(super::api::pause_projection),
        )
        .route(
            "/api/v1/projections/{name}/start",
            post(super::api::start_projection),
        )
        .route(
            "/api/v1/projections/{name}/{entity_id}/state",
            get(super::api::get_projection_state),
        )
        .route(
            "/api/v1/projections/{name}/{entity_id}/state",
            post(super::api::save_projection_state),
        )
        .route(
            "/api/v1/projections/{name}/{entity_id}/state",
            put(super::api::save_projection_state),
        )
        .route(
            "/api/v1/projections/{name}/bulk",
            post(super::api::bulk_get_projection_states),
        )
        .route(
            "/api/v1/projections/{name}/bulk/save",
            post(super::api::bulk_save_projection_states),
        )
        // v0.11: Webhook management
        .route("/api/v1/webhooks", post(super::api::register_webhook))
        .route("/api/v1/webhooks", get(super::api::list_webhooks))
        .route(
            "/api/v1/webhooks/{webhook_id}",
            get(super::api::get_webhook),
        )
        .route(
            "/api/v1/webhooks/{webhook_id}",
            put(super::api::update_webhook),
        )
        .route(
            "/api/v1/webhooks/{webhook_id}",
            delete(super::api::delete_webhook),
        )
        .route(
            "/api/v1/webhooks/{webhook_id}/deliveries",
            get(super::api::list_webhook_deliveries),
        )
        // v0.14: Durable consumer subscriptions
        .route("/api/v1/consumers", post(super::api::register_consumer))
        .route(
            "/api/v1/consumers/{consumer_id}",
            get(super::api::get_consumer),
        )
        .route(
            "/api/v1/consumers/{consumer_id}/events",
            get(super::api::poll_consumer_events),
        )
        .route(
            "/api/v1/consumers/{consumer_id}/ack",
            post(super::api::ack_consumer),
        )
        // v1.8: Cluster membership management API
        .route("/api/v1/cluster/status", get(cluster_status_handler))
        .route("/api/v1/cluster/members", get(cluster_list_members_handler))
        .route("/api/v1/cluster/members", post(cluster_add_member_handler))
        .route(
            "/api/v1/cluster/members/{node_id}",
            delete(cluster_remove_member_handler),
        )
        .route(
            "/api/v1/cluster/members/{node_id}/heartbeat",
            post(cluster_heartbeat_handler),
        )
        .route("/api/v1/cluster/vote", post(cluster_vote_handler))
        .route("/api/v1/cluster/election", post(cluster_election_handler))
        .route(
            "/api/v1/cluster/partitions",
            get(cluster_partitions_handler),
        )
        // v2.0: Advanced query features
        .route("/api/v1/graphql", post(super::api::graphql_query))
        .route("/api/v1/geospatial/query", post(super::api::geo_query))
        .route("/api/v1/geospatial/stats", get(super::api::geo_stats))
        .route(
            "/api/v1/exactly-once/stats",
            get(super::api::exactly_once_stats),
        )
        .route(
            "/api/v1/schema-evolution/history/{event_type}",
            get(super::api::schema_evolution_history),
        )
        .route(
            "/api/v1/schema-evolution/schema/{event_type}",
            get(super::api::schema_evolution_schema),
        )
        .route(
            "/api/v1/schema-evolution/stats",
            get(super::api::schema_evolution_stats),
        )
        // v1.9: Geo-replication API
        .route("/api/v1/geo/status", get(geo_status_handler))
        .route("/api/v1/geo/sync", post(geo_sync_handler))
        .route("/api/v1/geo/peers", get(geo_peers_handler))
        .route("/api/v1/geo/failover", post(geo_failover_handler));
    // Internal endpoints for sentinel-driven failover (enterprise only)
    #[cfg(feature = "replication")]
    let app = app
        .route("/internal/promote", post(promote_handler))
        .route("/internal/repoint", post(repoint_handler));
    let app = app;

    // v0.11: Bidirectional sync protocol (embedded↔server)
    #[cfg(feature = "embedded-sync")]
    let app = app
        .route("/api/v1/sync/pull", post(super::api::sync_pull_handler))
        .route("/api/v1/sync/push", post(super::api::sync_push_handler));

    let app = app
        .with_state(app_state.clone())
        // IMPORTANT: Middleware layers execute from bottom to top in Tower/Axum
        // Read-only middleware runs after auth (applied before rate limit layer)
        .layer(middleware::from_fn_with_state(
            app_state,
            read_only_middleware,
        ))
        .layer(middleware::from_fn_with_state(
            rate_limit_state,
            rate_limit_middleware,
        ))
        .layer(middleware::from_fn_with_state(auth_state, auth_middleware))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .layer(TraceLayer::new_for_http());

    // Prime API — nested router with its own state (feature-gated)
    #[cfg(feature = "prime")]
    let app = {
        let data_dir =
            std::env::var("PRIME_DATA_DIR").unwrap_or_else(|_| "/tmp/prime-data".to_string());
        match crate::prime::Prime::open(&data_dir).await {
            Ok(prime) => {
                let prime_state = Arc::new(super::prime_api::PrimeState { prime });
                tracing::info!("Prime API enabled at /api/v1/prime/*");
                app.nest(
                    "/api/v1/prime",
                    super::prime_api::prime_router().with_state(prime_state),
                )
            }
            Err(e) => {
                tracing::warn!("Prime API disabled: failed to open Prime: {e}");
                app
            }
        }
    };

    let listener = tokio::net::TcpListener::bind(addr).await?;

    // Graceful shutdown on SIGTERM (required for serverless platforms)
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("🛑 AllSource Core shutdown complete");
    Ok(())
}

/// Write paths that should be rejected when running as a follower.
const WRITE_PATHS: &[&str] = &[
    "/api/v1/events",
    "/api/v1/events/batch",
    "/api/v1/snapshots",
    "/api/v1/projections/",
    "/api/v1/schemas",
    "/api/v1/replay",
    "/api/v1/pipelines",
    "/api/v1/compaction/trigger",
    "/api/v1/audit/events",
    "/api/v1/config",
    "/api/v1/tenants",
    "/api/v1/webhooks",
    "/api/v1/demo/seed",
];

/// Returns true if this request is a write operation that should be blocked on followers.
fn is_write_request(method: &axum::http::Method, path: &str) -> bool {
    use axum::http::Method;
    // Only POST/PUT/DELETE are writes
    if method != Method::POST && method != Method::PUT && method != Method::DELETE {
        return false;
    }
    WRITE_PATHS
        .iter()
        .any(|write_path| path.starts_with(write_path))
}

/// Returns true if the request targets an internal or cluster endpoint (not subject to read-only checks).
fn is_internal_request(path: &str) -> bool {
    path.starts_with("/internal/")
        || path.starts_with("/api/v1/cluster/")
        || path.starts_with("/api/v1/geo/")
}

/// Middleware that rejects write requests when the node is a follower.
///
/// Returns HTTP 409 Conflict with `{"error": "read_only", "message": "..."}`.
/// Internal endpoints (`/internal/*`) are exempt — they are used by the sentinel
/// to trigger promotion and repointing during failover.
async fn read_only_middleware(
    State(state): State<AppState>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let path = request.uri().path();
    if state.role.load().is_follower()
        && is_write_request(request.method(), path)
        && !is_internal_request(path)
    {
        return (
            axum::http::StatusCode::CONFLICT,
            axum::Json(serde_json::json!({
                "error": "read_only",
                "message": "This node is a read-only follower"
            })),
        )
            .into_response();
    }
    next.run(request).await
}

/// Health endpoint with system stream health reporting.
///
/// Reports overall health plus detailed system metadata health when
/// event-sourced system repositories are configured.
async fn health_v1(State(state): State<AppState>) -> impl IntoResponse {
    let has_system_repos = state.service_container.has_system_repositories();

    let system_streams = if has_system_repos {
        let (tenant_count, config_count, total_events) =
            if let Some(store) = state.service_container.system_store() {
                use crate::domain::value_objects::system_stream::SystemDomain;
                (
                    store.count_stream(SystemDomain::Tenant),
                    store.count_stream(SystemDomain::Config),
                    store.total_events(),
                )
            } else {
                (0, 0, 0)
            };

        serde_json::json!({
            "status": "healthy",
            "mode": "event-sourced",
            "total_events": total_events,
            "tenant_events": tenant_count,
            "config_events": config_count,
        })
    } else {
        serde_json::json!({
            "status": "disabled",
            "mode": "in-memory",
        })
    };

    let replication = {
        #[cfg(feature = "replication")]
        {
            let shipper_guard = state.wal_shipper.read().await;
            if let Some(ref shipper) = *shipper_guard {
                serde_json::to_value(shipper.status()).unwrap_or_default()
            } else if let Some(ref receiver) = state.wal_receiver {
                serde_json::to_value(receiver.status()).unwrap_or_default()
            } else {
                serde_json::json!(null)
            }
        }
        #[cfg(not(feature = "replication"))]
        serde_json::json!({"edition": "community", "status": "not_available"})
    };

    let current_role = state.role.load();

    Json(serde_json::json!({
        "status": "healthy",
        "service": "allsource-core",
        "version": env!("CARGO_PKG_VERSION"),
        "role": current_role,
        "system_streams": system_streams,
        "replication": replication,
    }))
}

/// POST /internal/promote — Promote this follower to leader.
///
/// Called by the sentinel process during automated failover.
/// Switches the node role to leader, stops WAL receiving, and starts
/// a WAL shipper on the replication port so other followers can connect.
#[cfg(feature = "replication")]
async fn promote_handler(State(state): State<AppState>) -> impl IntoResponse {
    let current_role = state.role.load();
    if current_role == NodeRole::Leader {
        return (
            axum::http::StatusCode::OK,
            Json(serde_json::json!({
                "status": "already_leader",
                "message": "This node is already the leader",
            })),
        );
    }

    tracing::info!("PROMOTE: Switching role from follower to leader");

    // 1. Switch role — the read-only middleware will immediately start accepting writes
    state.role.store(NodeRole::Leader);

    // 2. Signal the WAL receiver to stop (it will stop reconnecting)
    if let Some(ref receiver) = state.wal_receiver {
        receiver.shutdown();
        tracing::info!("PROMOTE: WAL receiver shutdown signalled");
    }

    // 3. Start a new WAL shipper so remaining followers can connect
    let replication_port = state.replication_port;
    let (mut shipper, tx) = WalShipper::new();
    state.store.enable_wal_replication(tx);
    shipper.set_store(Arc::clone(&state.store));
    shipper.set_metrics(state.store.metrics());
    let shipper = Arc::new(shipper);

    // Install into AppState so health endpoint reports shipper status
    {
        let mut shipper_guard = state.wal_shipper.write().await;
        *shipper_guard = Some(Arc::clone(&shipper));
    }

    // Spawn the shipper server
    let shipper_clone = Arc::clone(&shipper);
    tokio::spawn(async move {
        if let Err(e) = shipper_clone.serve(replication_port).await {
            tracing::error!("Promoted WAL shipper error: {}", e);
        }
    });

    tracing::info!(
        "PROMOTE: Now accepting writes. WAL shipper listening on port {}",
        replication_port,
    );

    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!({
            "status": "promoted",
            "role": "leader",
            "replication_port": replication_port,
        })),
    )
}

/// POST /internal/repoint?leader=host:port — Switch replication target.
///
/// Called by the sentinel process to tell a follower to disconnect from
/// the old leader and reconnect to a newly promoted leader.
#[cfg(feature = "replication")]
async fn repoint_handler(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let current_role = state.role.load();
    if current_role != NodeRole::Follower {
        return (
            axum::http::StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error": "not_follower",
                "message": "Repoint only applies to follower nodes",
            })),
        );
    }

    let new_leader = match params.get("leader") {
        Some(l) if !l.is_empty() => l.clone(),
        _ => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "missing_leader",
                    "message": "Query parameter 'leader' is required (e.g. ?leader=new-leader:3910)",
                })),
            );
        }
    };

    tracing::info!("REPOINT: Switching replication target to {}", new_leader);

    if let Some(ref receiver) = state.wal_receiver {
        receiver.repoint(&new_leader);
        tracing::info!("REPOINT: WAL receiver repointed to {}", new_leader);
    } else {
        tracing::warn!("REPOINT: No WAL receiver to repoint");
    }

    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!({
            "status": "repointed",
            "new_leader": new_leader,
        })),
    )
}

// =============================================================================
// Cluster Management Handlers (v1.8)
// =============================================================================

/// GET /api/v1/cluster/status — Get cluster status including term, leader, and members
async fn cluster_status_handler(State(state): State<AppState>) -> impl IntoResponse {
    let Some(ref cm) = state.cluster_manager else {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "cluster_not_enabled",
                "message": "Cluster mode is not enabled on this node"
            })),
        );
    };

    let status = cm.status().await;
    (
        axum::http::StatusCode::OK,
        Json(serde_json::to_value(status).unwrap()),
    )
}

/// GET /api/v1/cluster/members — List all cluster members
async fn cluster_list_members_handler(State(state): State<AppState>) -> impl IntoResponse {
    let Some(ref cm) = state.cluster_manager else {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "cluster_not_enabled",
                "message": "Cluster mode is not enabled on this node"
            })),
        );
    };

    let members = cm.all_members();
    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!({
            "members": members,
            "count": members.len(),
        })),
    )
}

/// POST /api/v1/cluster/members — Add a member to the cluster
async fn cluster_add_member_handler(
    State(state): State<AppState>,
    Json(member): Json<ClusterMember>,
) -> impl IntoResponse {
    let Some(ref cm) = state.cluster_manager else {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "cluster_not_enabled",
                "message": "Cluster mode is not enabled on this node"
            })),
        );
    };

    let node_id = member.node_id;
    cm.add_member(member).await;

    tracing::info!("Cluster member {} added", node_id);
    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!({
            "status": "added",
            "node_id": node_id,
        })),
    )
}

/// DELETE /api/v1/cluster/members/{node_id} — Remove a member from the cluster
async fn cluster_remove_member_handler(
    State(state): State<AppState>,
    Path(node_id): Path<u32>,
) -> impl IntoResponse {
    let Some(ref cm) = state.cluster_manager else {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "cluster_not_enabled",
                "message": "Cluster mode is not enabled on this node"
            })),
        );
    };

    match cm.remove_member(node_id).await {
        Some(_) => {
            tracing::info!("Cluster member {} removed", node_id);
            (
                axum::http::StatusCode::OK,
                Json(serde_json::json!({
                    "status": "removed",
                    "node_id": node_id,
                })),
            )
        }
        None => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "not_found",
                "message": format!("Node {} not found in cluster", node_id),
            })),
        ),
    }
}

/// POST /api/v1/cluster/members/{node_id}/heartbeat — Update member heartbeat
#[derive(serde::Deserialize)]
struct HeartbeatRequest {
    wal_offset: u64,
    #[serde(default = "default_true")]
    healthy: bool,
}

fn default_true() -> bool {
    true
}

async fn cluster_heartbeat_handler(
    State(state): State<AppState>,
    Path(node_id): Path<u32>,
    Json(req): Json<HeartbeatRequest>,
) -> impl IntoResponse {
    let Some(ref cm) = state.cluster_manager else {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "cluster_not_enabled",
                "message": "Cluster mode is not enabled on this node"
            })),
        );
    };

    cm.update_member_heartbeat(node_id, req.wal_offset, req.healthy);
    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!({
            "status": "updated",
            "node_id": node_id,
        })),
    )
}

/// POST /api/v1/cluster/vote — Handle a vote request (simplified Raft RequestVote)
async fn cluster_vote_handler(
    State(state): State<AppState>,
    Json(request): Json<VoteRequest>,
) -> impl IntoResponse {
    let Some(ref cm) = state.cluster_manager else {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "cluster_not_enabled",
                "message": "Cluster mode is not enabled on this node"
            })),
        );
    };

    let response = cm.handle_vote_request(&request).await;
    (
        axum::http::StatusCode::OK,
        Json(serde_json::to_value(response).unwrap()),
    )
}

/// POST /api/v1/cluster/election — Trigger a leader election (manual failover)
async fn cluster_election_handler(State(state): State<AppState>) -> impl IntoResponse {
    let Some(ref cm) = state.cluster_manager else {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "cluster_not_enabled",
                "message": "Cluster mode is not enabled on this node"
            })),
        );
    };

    // Select best candidate deterministically
    let candidate = cm.select_leader_candidate();

    match candidate {
        Some(candidate_id) => {
            let new_term = cm.start_election().await;
            tracing::info!(
                "Cluster election started: term={}, candidate={}",
                new_term,
                candidate_id,
            );

            // If this node is the candidate, become leader immediately
            // (In a full Raft, we'd collect votes from a majority first)
            if candidate_id == cm.self_id() {
                cm.become_leader(new_term).await;
                tracing::info!("Node {} became leader at term {}", candidate_id, new_term);
            }

            (
                axum::http::StatusCode::OK,
                Json(serde_json::json!({
                    "status": "election_started",
                    "term": new_term,
                    "candidate_id": candidate_id,
                    "self_is_leader": candidate_id == cm.self_id(),
                })),
            )
        }
        None => (
            axum::http::StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error": "no_candidates",
                "message": "No healthy members available for leader election",
            })),
        ),
    }
}

/// GET /api/v1/cluster/partitions — Get partition distribution across nodes
async fn cluster_partitions_handler(State(state): State<AppState>) -> impl IntoResponse {
    let Some(ref cm) = state.cluster_manager else {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "cluster_not_enabled",
                "message": "Cluster mode is not enabled on this node"
            })),
        );
    };

    let registry = cm.registry();
    let distribution = registry.partition_distribution();
    let total_partitions: usize = distribution.values().map(std::vec::Vec::len).sum();

    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!({
            "total_partitions": total_partitions,
            "node_count": registry.node_count(),
            "healthy_node_count": registry.healthy_node_count(),
            "distribution": distribution,
        })),
    )
}

// =============================================================================
// Geo-Replication Handlers (v1.9)
// =============================================================================

/// GET /api/v1/geo/status — Get geo-replication status
async fn geo_status_handler(State(state): State<AppState>) -> impl IntoResponse {
    let Some(ref geo) = state.geo_replication else {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "geo_replication_not_enabled",
                "message": "Geo-replication is not enabled on this node"
            })),
        );
    };

    let status = geo.status();
    (
        axum::http::StatusCode::OK,
        Json(serde_json::to_value(status).unwrap()),
    )
}

/// POST /api/v1/geo/sync — Receive replicated events from a peer region
async fn geo_sync_handler(
    State(state): State<AppState>,
    Json(request): Json<GeoSyncRequest>,
) -> impl IntoResponse {
    let Some(ref geo) = state.geo_replication else {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "geo_replication_not_enabled",
                "message": "Geo-replication is not enabled on this node"
            })),
        );
    };

    tracing::info!(
        "Geo-sync received from region '{}': {} events",
        request.source_region,
        request.events.len(),
    );

    let response = geo.receive_sync(&request);
    (
        axum::http::StatusCode::OK,
        Json(serde_json::to_value(response).unwrap()),
    )
}

/// GET /api/v1/geo/peers — List peer regions and their health
async fn geo_peers_handler(State(state): State<AppState>) -> impl IntoResponse {
    let Some(ref geo) = state.geo_replication else {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "geo_replication_not_enabled",
                "message": "Geo-replication is not enabled on this node"
            })),
        );
    };

    let status = geo.status();
    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!({
            "region_id": status.region_id,
            "peers": status.peers,
        })),
    )
}

/// POST /api/v1/geo/failover — Trigger regional failover
async fn geo_failover_handler(State(state): State<AppState>) -> impl IntoResponse {
    let Some(ref geo) = state.geo_replication else {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "geo_replication_not_enabled",
                "message": "Geo-replication is not enabled on this node"
            })),
        );
    };

    match geo.select_failover_region() {
        Some(failover_region) => {
            tracing::info!(
                "Geo-failover: selected region '{}' as failover target",
                failover_region,
            );
            (
                axum::http::StatusCode::OK,
                Json(serde_json::json!({
                    "status": "failover_target_selected",
                    "failover_region": failover_region,
                    "message": "Region selected for failover. DNS/routing update required externally.",
                })),
            )
        }
        None => (
            axum::http::StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error": "no_healthy_peers",
                "message": "No healthy peer regions available for failover",
            })),
        ),
    }
}

/// Listen for shutdown signals (SIGTERM for serverless, SIGINT for local dev)
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {
            tracing::info!("📤 Received Ctrl+C, initiating graceful shutdown...");
        }
        () = terminate => {
            tracing::info!("📤 Received SIGTERM, initiating graceful shutdown...");
        }
    }
}
