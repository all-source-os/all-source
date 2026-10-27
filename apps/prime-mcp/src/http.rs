//! HTTP REST API server for Prime (axum-based).
//!
//! Exposes the full Prime API as REST endpoints, consistent with Core's HTTP stack.
//! Activated via `--mode http --port 3905`.

use std::sync::Arc;

use allsource_core::prime::{Direction, Prime, hosted::HostedPrime, recall::RecallEngine};
use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, Query, Request, State},
    http::{HeaderMap, StatusCode},
    middleware::{Next, from_fn_with_state},
    response::{Html, IntoResponse, Response},
    routing::{delete, get, patch, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use tower_http::trace::TraceLayer;

/// Shared state for HTTP handlers.
///
/// In the hosted/stateless deployment (`CORE_URL` + `PRIME_API_KEY` set) `prime`
/// and `recall` are `None` — the app owns no durable store and serves every
/// request through the tenant-scoped [`HostedPrime`] over a remote Core. In
/// local/dev http mode (no `CORE_URL`) `hosted` is `None` and the embedded
/// single-store `prime`/`recall` serve instead. Exactly one of the two paths is
/// populated for a given startup; see `main.rs`.
pub struct AppState {
    /// Embedded single-store engine. `Some` only in local/dev http mode (no
    /// remote Core). `None` in the hosted deployment, which is fully stateless.
    pub prime: Option<Arc<Prime>>,
    /// Recall engine — used by the embedded `/recall` endpoint. `Some` exactly
    /// when `prime` is `Some` (both belong to the embedded path).
    #[allow(dead_code)]
    pub recall: Option<RecallEngine>,
    /// Stateless, tenant-scoped engine over a remote Core. Present only in the
    /// hosted deployment (constructed when `CORE_URL` + `PRIME_API_KEY` are set).
    /// When `Some` and a request carries a trusted `X-Tenant-Id`, every REST and
    /// MCP call is routed through this instead of the embedded `prime`.
    pub hosted: Option<Arc<HostedPrime>>,
}

/// Start the HTTP server on the given port.
pub async fn serve(state: Arc<AppState>, port: u16) -> anyhow::Result<()> {
    let app = Router::new()
        // Node endpoints
        .route("/api/v1/prime/nodes", post(create_node))
        .route("/api/v1/prime/nodes/{id}", get(get_node))
        .route("/api/v1/prime/nodes/{id}", patch(update_node))
        .route("/api/v1/prime/nodes/{id}", delete(delete_node))
        .route("/api/v1/prime/nodes/{id}/neighbors", get(get_neighbors))
        .route("/api/v1/prime/nodes/{id}/subgraph", get(get_subgraph))
        .route("/api/v1/prime/nodes/{id}/history", get(get_history))
        // Edge endpoints
        .route("/api/v1/prime/edges", post(create_edge))
        .route("/api/v1/prime/edges/{id}", delete(delete_edge))
        // Vector endpoints
        .route("/api/v1/prime/vectors", post(store_vector))
        .route("/api/v1/prime/vectors/search", post(search_vectors))
        .route("/api/v1/prime/vectors/{id}", delete(delete_vector))
        // Query endpoints
        .route("/api/v1/prime/shortest-path", post(shortest_path))
        .route("/api/v1/prime/recall", post(recall))
        .route("/api/v1/prime/diff", get(get_diff))
        // Status endpoints
        .route("/api/v1/prime/stats", get(get_stats))
        .route("/api/v1/prime/graph", get(get_full_graph))
        // Self-contained local graph viewer (single HTML page, no CDN, offline).
        // Fetches /api/v1/prime/graph from the same origin and renders the
        // local store as a bubble graph + detail list.
        .route("/api/v1/prime/graph.html", get(graph_viewer))
        // MCP-over-HTTP (Streamable HTTP transport) — lets MCP clients connect
        // to the hosted Prime with no local binary. POST a JSON-RPC request,
        // get a JSON-RPC response (or 202 for notifications).
        .route("/mcp", post(mcp_handler))
        .route("/health", get(health))
        .merge(crate::profiling::routes())
        // Gate the hosted REST surface behind the same bearer check as /mcp.
        // When a HostedPrime engine is configured, `/api/v1/prime/*` serves
        // tenant-scoped data and trusts the X-Tenant-Id header — so it MUST
        // require PRIME_API_KEY, exactly like /mcp, or a public caller could
        // spoof the tenant header on the app's exposed REST routes. (/mcp does
        // its own check; this covers REST.)
        .layer(from_fn_with_state(Arc::clone(&state), prime_rest_auth_gate))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    // Bind to the IPv6 wildcard so Fly's internal network (which resolves
    // .internal names to IPv6) can reach us. `[::]` accepts both IPv4 and
    // IPv6 connections on Linux (dual-stack), matching what Core does via
    // the ALLSOURCE_HOST=":: " env var. A plain `0.0.0.0` bind is IPv4-only
    // and caused "connection refused" on Fly's private network even though
    // the public hostname worked via Fly's edge proxy.
    let addr = format!("[::]:{port}");
    tracing::info!("HTTP server listening on {addr}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

// =============================================================================
// MCP-over-HTTP (Streamable HTTP transport)
// =============================================================================

/// Optional bearer/API-key gate for the `/mcp` endpoint.
///
/// If `PRIME_API_KEY` is set (and non-empty), the request must carry a matching
/// key in `Authorization: Bearer <key>` (or a bare `Authorization`/`X-API-Key`).
/// If the env var is unset, the endpoint is open — matching the existing REST
/// handlers' behavior. Full per-tenant key validation through the gateway is a
/// follow-up (see bead t-dbee53 notes).
fn mcp_authorized(headers: &HeaderMap) -> bool {
    let expected = match std::env::var("PRIME_API_KEY") {
        Ok(k) if !k.is_empty() => k,
        _ => return true, // no key configured → open
    };
    let provided = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.strip_prefix("Bearer ").unwrap_or(s))
        .or_else(|| headers.get("x-api-key").and_then(|v| v.to_str().ok()));
    provided == Some(expected.as_str())
}

/// Middleware: when a hosted engine is configured, require the `PRIME_API_KEY`
/// bearer on the `/api/v1/prime/*` REST surface before any handler runs — those
/// routes serve tenant-scoped data and trust the `X-Tenant-Id` header, so they
/// must be gated exactly like `/mcp`. Transparent (no-op) when `hosted` is
/// `None` (local/dev embedded mode) or `PRIME_API_KEY` is unset (open, matching
/// `mcp_authorized`). `/mcp`, `/health`, and profiling routes are unaffected.
async fn prime_rest_auth_gate(
    State(state): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Response {
    let gated = state.hosted.is_some() && req.uri().path().starts_with("/api/v1/prime");
    if gated && !mcp_authorized(req.headers()) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "invalid or missing API key" })),
        )
            .into_response();
    }
    next.run(req).await
}

/// Extract the trusted tenant id from the `X-Tenant-Id` header (case-insensitive
/// — `HeaderMap` lookups are already case-insensitive). Returns `None` when the
/// header is absent or empty.
///
/// TRUST MODEL: hosted, tenant-scoped serving is only enabled at startup when
/// `PRIME_API_KEY` is configured (see `main.rs`), so every `/mcp` request that
/// reaches the hosted path has already passed [`mcp_authorized`]'s bearer check.
/// That means only the gateway (which holds the key) can set `X-Tenant-Id` — a
/// caller without the key gets 401 before this header is read, so the tenant id
/// is trustworthy. If `PRIME_API_KEY` is unset, hosted mode is refused and this
/// header is never consulted (the embedded single-store path serves instead).
fn tenant_from_headers(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-tenant-id")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
}

/// MCP Streamable-HTTP endpoint. Accepts a single JSON-RPC request and returns
/// the JSON-RPC response as `application/json`, or `202 Accepted` for
/// notifications (which have no reply).
///
/// Two dispatch paths share the exact same request/response shaping:
/// - **Hosted, tenant-scoped:** when a [`HostedPrime`] engine is configured AND
///   the request carries a trusted `X-Tenant-Id`, the call is routed through
///   [`crate::hosted_dispatch::handle_request_hosted`] against the remote Core,
///   isolated per tenant.
/// - **Embedded (default/fallback):** otherwise, dispatch goes to the local
///   single-store [`Prime`] via [`crate::dispatch::handle_request`] — identical
///   to the stdio transport, so neither path can drift in methods/tools.
async fn mcp_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    if !mcp_authorized(&headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "invalid or missing API key" })),
        )
            .into_response();
    }

    let req: crate::protocol::Request = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            // JSON-RPC parse error: reply 200 with an error envelope (null id).
            let resp = crate::protocol::Response::error(None, -32700, format!("Parse error: {e}"));
            return (StatusCode::OK, Json(resp)).into_response();
        }
    };

    // Hosted, tenant-scoped path: only when both a HostedPrime engine and a
    // trusted tenant id are present. Falls back to the embedded path otherwise.
    if let (Some(hosted), Some(tenant)) = (state.hosted.as_ref(), tenant_from_headers(&headers)) {
        return match crate::hosted_dispatch::handle_request_hosted(hosted, &tenant, &req).await {
            Some(resp) => (StatusCode::OK, Json(resp)).into_response(),
            None => StatusCode::ACCEPTED.into_response(),
        };
    }

    // HTTP transport runs without auto-inject (that's a stdio system-prompt
    // feature); pass the same defaults the stdio binary uses when disabled.
    // In the hosted/stateless deployment there is no embedded `prime`, so a
    // request that reaches here (hosted configured but no trusted tenant) has
    // no backend to serve it — surface a clear error instead of dispatching
    // against a store that does not exist.
    let (Some(prime), Some(recall)) = (state.prime.as_ref(), state.recall.as_ref()) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "X-Tenant-Id required: this server is stateless (hosted mode) and has no embedded store" })),
        )
            .into_response();
    };
    match crate::dispatch::handle_request(prime, recall, false, 1000, &req).await {
        Some(resp) => (StatusCode::OK, Json(resp)).into_response(),
        None => StatusCode::ACCEPTED.into_response(),
    }
}

/// Standard 400 for the hosted deployment when a REST request arrives without a
/// trusted `X-Tenant-Id`. The hosted engine is per-tenant, so there is no
/// default tenant to fall back to and no embedded store to serve from.
fn missing_tenant_response() -> (StatusCode, Json<Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({ "error": "X-Tenant-Id required: this server is stateless (hosted mode)" })),
    )
}

// =============================================================================
// Request/Response types
// =============================================================================

#[derive(Deserialize)]
struct CreateNodeRequest {
    #[serde(rename = "type")]
    node_type: String,
    properties: Value,
}

#[derive(Deserialize)]
struct UpdateNodeRequest {
    properties: Value,
}

#[derive(Deserialize)]
struct CreateEdgeRequest {
    source: String,
    target: String,
    relation: String,
    properties: Option<Value>,
    weight: Option<f64>,
}

#[derive(Deserialize)]
struct StoreVectorRequest {
    id: String,
    text: Option<String>,
    /// Optional when `text` is supplied — the server embeds it in-process
    /// via the bundled fastembed model.
    vector: Option<Vec<f32>>,
    metadata: Option<Value>,
}

#[derive(Deserialize)]
struct VectorSearchRequest {
    /// Optional when `text` is supplied — the server embeds it in-process.
    vector: Option<Vec<f32>>,
    /// Natural-language query. Embedded server-side when `vector` is absent.
    text: Option<String>,
    top_k: Option<usize>,
}

#[derive(Deserialize)]
struct ShortestPathRequest {
    from: String,
    to: String,
    relation: Option<String>,
}

/// Query parameters for `GET /api/v1/prime/graph`.
///
/// Mirrors Core's contract. prime-mcp is a LOCAL single-store with no tenant,
/// so `tenant_id` is accepted for shape-parity but normally omitted.
#[derive(Deserialize)]
struct GraphQuery {
    tenant_id: Option<String>,
    node_type: Option<String>,
    limit: Option<usize>,
}

#[derive(Deserialize)]
struct RecallRequest {
    vector: Option<Vec<f32>>,
    node_type: Option<String>,
    depth: Option<usize>,
    top_k: Option<usize>,
    text: Option<String>,
    /// Restrict results to nodes stamped with this `properties.tenant_id`, the
    /// same marker the local `/graph` route filters on. Only meaningful for
    /// the embedded backend: the hosted one is already scoped by `X-Tenant-Id`.
    tenant_id: Option<String>,
}

// =============================================================================
// Handlers
// =============================================================================

/// `GET /health`.
///
/// Reports `degraded` when this process lost the Prime data-dir writer lock to
/// another process. That state is silent everywhere else: a replica serves
/// reads perfectly and rejects every write, so a plain `ok` here would let a
/// second writer look healthy to Fly while nothing it ingests is ever stored.
/// The multi-tenant ingest shape runs exactly one writer, which is precisely
/// when an accidental second one has to be visible.
///
/// Still HTTP 200: the process is up and serving reads, and flapping the Fly
/// health check would replace a diagnosable problem with a restart loop.
async fn health(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let read_only = state.prime.as_ref().is_some_and(|p| p.is_read_only());
    if read_only {
        Json(json!({
            "status": "degraded",
            "writable": false,
            "reason": "another process owns the Prime data dir; this instance is a \
                       read-only replica and rejects writes",
        }))
    } else {
        Json(json!({"status": "ok", "writable": true}))
    }
}

/// `GET /api/v1/prime/graph.html` — the self-contained local graph viewer.
///
/// The page is compiled into the binary via `include_str!` (same idiom as the
/// entity templates in `templates.rs`), so it needs no static-file server and
/// no companion assets on disk. It has no external `<script src>`/`<link href>`,
/// so it renders offline; on load it fetches the same-origin
/// `/api/v1/prime/graph` and draws the store as a bubble graph + detail list.
async fn graph_viewer() -> impl IntoResponse {
    Html(include_str!("../static/graph.html"))
}

async fn get_stats(State(state): State<Arc<AppState>>, headers: HeaderMap) -> impl IntoResponse {
    if let Some(hosted) = state.hosted.as_ref() {
        let Some(tenant) = tenant_from_headers(&headers) else {
            return missing_tenant_response();
        };
        return match hosted.stats(&tenant).await {
            Ok(stats) => (
                StatusCode::OK,
                Json(json!({
                    "total_nodes": stats.total_nodes,
                    "total_edges": stats.total_edges,
                    "deleted_nodes": stats.deleted_nodes,
                    "deleted_edges": stats.deleted_edges,
                    "event_count": stats.event_count,
                    "nodes_by_type": stats.nodes_by_type,
                    "edges_by_relation": stats.edges_by_relation,
                    "sync": crate::tools::sync_status_json(),
                })),
            ),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            ),
        };
    }
    let prime = state
        .prime
        .as_ref()
        .expect("embedded prime present when hosted is None");
    let stats = prime.stats();
    (
        StatusCode::OK,
        Json(json!({
            "total_nodes": stats.total_nodes,
            "total_edges": stats.total_edges,
            "deleted_nodes": stats.deleted_nodes,
            "deleted_edges": stats.deleted_edges,
            "event_count": stats.event_count,
            "nodes_by_type": stats.nodes_by_type,
            "edges_by_relation": stats.edges_by_relation,
            "sync": crate::tools::sync_status_json(),
        })),
    )
}

/// `GET /api/v1/prime/graph` — full materialized knowledge graph from the
/// local store, identical contract to Core's `/api/v1/prime/graph` (see the
/// doc comment on Core's `get_full_graph` for the verbatim JSON shape).
/// prime-mcp is local + single-store, so there is no tenant boundary here.
async fn get_full_graph(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<GraphQuery>,
) -> impl IntoResponse {
    if let Some(hosted) = state.hosted.as_ref() {
        let Some(tenant) = tenant_from_headers(&headers) else {
            return missing_tenant_response();
        };
        return match hosted
            .full_graph(&tenant, q.node_type.as_deref(), q.limit)
            .await
        {
            Ok(graph) => (
                StatusCode::OK,
                Json(serde_json::to_value(&graph).unwrap_or(Value::Null)),
            ),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            ),
        };
    }
    let prime = state
        .prime
        .as_ref()
        .expect("embedded prime present when hosted is None");
    let graph = prime.full_graph(q.tenant_id.as_deref(), q.node_type.as_deref(), q.limit);
    (
        StatusCode::OK,
        Json(serde_json::to_value(&graph).unwrap_or(Value::Null)),
    )
}

async fn create_node(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<CreateNodeRequest>,
) -> impl IntoResponse {
    if let Some(hosted) = state.hosted.as_ref() {
        let Some(tenant) = tenant_from_headers(&headers) else {
            return missing_tenant_response();
        };
        return match hosted
            .add_node(&tenant, &req.node_type, req.properties)
            .await
        {
            Ok(id) => {
                let entity_id =
                    allsource_core::prime::EntityId::node(&req.node_type, id.as_str()).to_wire();
                (
                    StatusCode::CREATED,
                    Json(json!({"node_id": id.as_str(), "entity_id": entity_id})),
                )
            }
            Err(e) => (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": e.to_string()})),
            ),
        };
    }
    let prime = state
        .prime
        .as_ref()
        .expect("embedded prime present when hosted is None");
    match prime.add_node(&req.node_type, req.properties).await {
        Ok(id) => {
            let entity_id =
                allsource_core::prime::EntityId::node(&req.node_type, id.as_str()).to_wire();
            (
                StatusCode::CREATED,
                Json(json!({"node_id": id.as_str(), "entity_id": entity_id})),
            )
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": e.to_string()})),
        ),
    }
}

/// Shape a [`Node`] the same way both REST paths return it, so hosted and
/// embedded `GET /nodes/{id}` are wire-identical.
fn node_detail_json(node: &allsource_core::prime::Node) -> Value {
    json!({
        "id": node.id.as_str(),
        "type": node.node_type,
        "properties": node.properties,
        "domain": node.domain,
        "labels": node.labels,
        "created_at": node.created_at.to_rfc3339(),
        "updated_at": node.updated_at.to_rfc3339(),
    })
}

async fn get_node(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Some(hosted) = state.hosted.as_ref() {
        let Some(tenant) = tenant_from_headers(&headers) else {
            return missing_tenant_response();
        };
        return match hosted.get_node(&tenant, &id).await {
            Ok(Some(node)) => (StatusCode::OK, Json(node_detail_json(&node))),
            Ok(None) => (
                StatusCode::NOT_FOUND,
                Json(json!({"error": format!("node not found: {id}")})),
            ),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            ),
        };
    }
    let prime = state
        .prime
        .as_ref()
        .expect("embedded prime present when hosted is None");
    // Try both raw id and as entity_id
    match prime.get_node(&id) {
        Some(node) => (StatusCode::OK, Json(node_detail_json(&node))),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": format!("node not found: {id}")})),
        ),
    }
}

async fn update_node(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<UpdateNodeRequest>,
) -> impl IntoResponse {
    if let Some(hosted) = state.hosted.as_ref() {
        let Some(tenant) = tenant_from_headers(&headers) else {
            return missing_tenant_response();
        };
        return match hosted.update_node(&tenant, &id, req.properties).await {
            Ok(()) => (StatusCode::OK, Json(json!({"updated": true}))),
            Err(e) => (StatusCode::NOT_FOUND, Json(json!({"error": e.to_string()}))),
        };
    }
    let prime = state
        .prime
        .as_ref()
        .expect("embedded prime present when hosted is None");
    match prime.update_node(&id, req.properties).await {
        Ok(()) => (StatusCode::OK, Json(json!({"updated": true}))),
        Err(e) => (StatusCode::NOT_FOUND, Json(json!({"error": e.to_string()}))),
    }
}

async fn delete_node(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Some(hosted) = state.hosted.as_ref() {
        let Some(tenant) = tenant_from_headers(&headers) else {
            return missing_tenant_response();
        };
        return match hosted.delete_node(&tenant, &id).await {
            Ok(()) => (StatusCode::OK, Json(json!({"deleted": true}))),
            Err(e) => (StatusCode::NOT_FOUND, Json(json!({"error": e.to_string()}))),
        };
    }
    let prime = state
        .prime
        .as_ref()
        .expect("embedded prime present when hosted is None");
    match prime.delete_node(&id).await {
        Ok(()) => (StatusCode::OK, Json(json!({"deleted": true}))),
        Err(e) => (StatusCode::NOT_FOUND, Json(json!({"error": e.to_string()}))),
    }
}

async fn get_neighbors(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Some(hosted) = state.hosted.as_ref() {
        let Some(tenant) = tenant_from_headers(&headers) else {
            return missing_tenant_response();
        };
        return match hosted.neighbors(&tenant, &id, None, Direction::Both).await {
            Ok(nodes) => {
                let nodes_json: Vec<Value> = nodes
                    .iter()
                    .map(|n| json!({"id": n.id.as_str(), "type": n.node_type, "properties": n.properties}))
                    .collect();
                (StatusCode::OK, Json(json!({"nodes": nodes_json})))
            }
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            ),
        };
    }
    let prime = state
        .prime
        .as_ref()
        .expect("embedded prime present when hosted is None");
    let nodes = prime.neighbors(&id, None, Direction::Both);
    let nodes_json: Vec<Value> = nodes
        .iter()
        .map(|n| json!({"id": n.id.as_str(), "type": n.node_type, "properties": n.properties}))
        .collect();
    (StatusCode::OK, Json(json!({"nodes": nodes_json})))
}

async fn get_subgraph(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> impl IntoResponse {
    // HostedPrime has no subgraph/ego-network traversal yet (only single-hop
    // neighbors). Rather than fake a depth-2 expansion or silently return a
    // 1-hop set under the subgraph contract, return 501 on the hosted path so
    // callers know the capability is unavailable on the stateless backend.
    if let Some(_hosted) = state.hosted.as_ref() {
        let Some(_tenant) = tenant_from_headers(&headers) else {
            return missing_tenant_response();
        };
        return (
            StatusCode::NOT_IMPLEMENTED,
            Json(json!({
                "error": "subgraph traversal is not yet available on the hosted backend; \
                          use /nodes/{id}/neighbors for single-hop neighbors"
            })),
        );
    }
    let prime = state
        .prime
        .as_ref()
        .expect("embedded prime present when hosted is None");
    let sg = prime.subgraph(&id, 2);
    let nodes_json: Vec<Value> = sg
        .nodes
        .iter()
        .map(|n| json!({"id": n.id.as_str(), "type": n.node_type, "properties": n.properties}))
        .collect();
    let edges_json: Vec<Value> = sg
        .edges
        .iter()
        .map(|e| json!({"id": e.id.as_str(), "source": e.source.as_str(), "target": e.target.as_str(), "relation": e.relation}))
        .collect();
    (
        StatusCode::OK,
        Json(json!({"nodes": nodes_json, "edges": edges_json})),
    )
}

async fn get_history(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Some(hosted) = state.hosted.as_ref() {
        let Some(tenant) = tenant_from_headers(&headers) else {
            return missing_tenant_response();
        };
        return match hosted.history(&tenant, &id).await {
            Ok(entries) => {
                let events: Vec<Value> = entries
                    .iter()
                    .map(|e| json!({"type": e.event_type, "timestamp": e.timestamp.to_rfc3339(), "payload": e.payload}))
                    .collect();
                (StatusCode::OK, Json(json!({"events": events})))
            }
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            ),
        };
    }
    let prime = state
        .prime
        .as_ref()
        .expect("embedded prime present when hosted is None");
    match prime.history(&id).await {
        Ok(entries) => {
            let events: Vec<Value> = entries
                .iter()
                .map(|e| json!({"type": e.event_type, "timestamp": e.timestamp.to_rfc3339(), "payload": e.payload}))
                .collect();
            (StatusCode::OK, Json(json!({"events": events})))
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        ),
    }
}

async fn create_edge(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<CreateEdgeRequest>,
) -> impl IntoResponse {
    if let Some(hosted) = state.hosted.as_ref() {
        let Some(tenant) = tenant_from_headers(&headers) else {
            return missing_tenant_response();
        };
        let result = if let Some(w) = req.weight {
            hosted
                .add_edge_weighted(
                    &tenant,
                    &req.source,
                    &req.target,
                    &req.relation,
                    w,
                    req.properties,
                )
                .await
        } else {
            hosted
                .add_edge(
                    &tenant,
                    &req.source,
                    &req.target,
                    &req.relation,
                    req.properties,
                )
                .await
        };
        return match result {
            Ok(id) => (StatusCode::CREATED, Json(json!({"edge_id": id.as_str()}))),
            Err(e) => (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": e.to_string()})),
            ),
        };
    }
    let prime = state
        .prime
        .as_ref()
        .expect("embedded prime present when hosted is None");
    let result = if let Some(w) = req.weight {
        prime
            .add_edge_weighted(&req.source, &req.target, &req.relation, w, req.properties)
            .await
    } else {
        prime
            .add_edge(&req.source, &req.target, &req.relation, req.properties)
            .await
    };

    match result {
        Ok(id) => (StatusCode::CREATED, Json(json!({"edge_id": id.as_str()}))),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": e.to_string()})),
        ),
    }
}

async fn delete_edge(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Some(hosted) = state.hosted.as_ref() {
        let Some(tenant) = tenant_from_headers(&headers) else {
            return missing_tenant_response();
        };
        return match hosted.delete_edge(&tenant, &id).await {
            Ok(()) => (StatusCode::OK, Json(json!({"deleted": true}))),
            Err(e) => (StatusCode::NOT_FOUND, Json(json!({"error": e.to_string()}))),
        };
    }
    let prime = state
        .prime
        .as_ref()
        .expect("embedded prime present when hosted is None");
    match prime.delete_edge(&id).await {
        Ok(()) => (StatusCode::OK, Json(json!({"deleted": true}))),
        Err(e) => (StatusCode::NOT_FOUND, Json(json!({"error": e.to_string()}))),
    }
}

async fn store_vector(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<StoreVectorRequest>,
) -> impl IntoResponse {
    if let Some(hosted) = state.hosted.as_ref() {
        let Some(tenant) = tenant_from_headers(&headers) else {
            return missing_tenant_response();
        };
        // The hosted backend stores a precomputed embedding (in-process
        // text→vector embedding downloads a model on first use and is never
        // exercised on the hosted path). Reject text-only with a clear error,
        // mirroring the hosted MCP dispatch.
        let Some(vector) = req.vector else {
            return (
                StatusCode::BAD_REQUEST,
                Json(
                    json!({"error": "missing 'vector' — the hosted backend requires a precomputed embedding vector"}),
                ),
            );
        };
        return match hosted.embed(&tenant, &req.id, vector, req.metadata).await {
            Ok(()) => (
                StatusCode::CREATED,
                Json(json!({"stored": true, "id": req.id})),
            ),
            Err(e) => (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": e.to_string()})),
            ),
        };
    }
    let prime = state
        .prime
        .as_ref()
        .expect("embedded prime present when hosted is None");
    let vector = match req.vector {
        Some(v) => v,
        None => match req.text.as_deref() {
            Some(t) => match prime.embed_text(t) {
                Ok(v) => v,
                Err(e) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({"error": format!("server-side embedding failed: {e}")})),
                    );
                }
            },
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": "missing 'vector' or 'text' — supply at least one"})),
                );
            }
        },
    };

    match prime
        .embed_with_metadata(&req.id, req.text.as_deref(), vector, req.metadata)
        .await
    {
        Ok(()) => (
            StatusCode::CREATED,
            Json(json!({"stored": true, "id": req.id})),
        ),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": e.to_string()})),
        ),
    }
}

async fn search_vectors(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<VectorSearchRequest>,
) -> impl IntoResponse {
    // HostedPrime exposes semantic recall (`/recall`, returning hydrated graph
    // nodes) but no raw `{id, score, text}` vector_search. Rather than reshape
    // recall into a different contract, return 501 on the hosted path and point
    // callers at /recall.
    if let Some(_hosted) = state.hosted.as_ref() {
        let Some(_tenant) = tenant_from_headers(&headers) else {
            return missing_tenant_response();
        };
        return (
            StatusCode::NOT_IMPLEMENTED,
            Json(json!({
                "error": "raw vector search is not available on the hosted backend; use /api/v1/prime/recall"
            })),
        );
    }
    let prime = state
        .prime
        .as_ref()
        .expect("embedded prime present when hosted is None");
    let top_k = req.top_k.unwrap_or(10);
    let vector = match req.vector {
        Some(v) => v,
        None => match req.text.as_deref() {
            Some(t) => match prime.embed_text(t) {
                Ok(v) => v,
                Err(e) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({"error": format!("server-side embedding failed: {e}")})),
                    );
                }
            },
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": "missing 'vector' or 'text' — supply at least one"})),
                );
            }
        },
    };
    let results = prime.vector_search(&vector, top_k);
    let results_json: Vec<Value> = results
        .iter()
        .map(|r| json!({"id": r.id, "score": r.score, "text": r.text}))
        .collect();
    (StatusCode::OK, Json(json!({"results": results_json})))
}

async fn delete_vector(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Some(hosted) = state.hosted.as_ref() {
        let Some(tenant) = tenant_from_headers(&headers) else {
            return missing_tenant_response();
        };
        return match hosted.delete_vector(&tenant, &id).await {
            Ok(()) => (StatusCode::OK, Json(json!({"deleted": true}))),
            Err(e) => (StatusCode::NOT_FOUND, Json(json!({"error": e.to_string()}))),
        };
    }
    let prime = state
        .prime
        .as_ref()
        .expect("embedded prime present when hosted is None");
    match prime.delete_vector(&id).await {
        Ok(()) => (StatusCode::OK, Json(json!({"deleted": true}))),
        Err(e) => (StatusCode::NOT_FOUND, Json(json!({"error": e.to_string()}))),
    }
}

async fn shortest_path(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<ShortestPathRequest>,
) -> impl IntoResponse {
    if let Some(hosted) = state.hosted.as_ref() {
        let Some(tenant) = tenant_from_headers(&headers) else {
            return missing_tenant_response();
        };
        return match hosted
            .shortest_path(&tenant, &req.from, &req.to, req.relation.as_deref())
            .await
        {
            Ok(Some(path)) => {
                let nodes: Vec<Value> = path
                    .iter()
                    .map(|n| json!({"id": n.id.as_str(), "type": n.node_type, "properties": n.properties}))
                    .collect();
                (StatusCode::OK, Json(json!({"path": nodes})))
            }
            Ok(None) => (
                StatusCode::OK,
                Json(json!({"path": null, "message": "No path found"})),
            ),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            ),
        };
    }
    let prime = state
        .prime
        .as_ref()
        .expect("embedded prime present when hosted is None");
    match prime.shortest_path(&req.from, &req.to, req.relation.as_deref()) {
        Some(path) => {
            let nodes: Vec<Value> = path
                .iter()
                .map(|n| json!({"id": n.id.as_str(), "type": n.node_type, "properties": n.properties}))
                .collect();
            (StatusCode::OK, Json(json!({"path": nodes})))
        }
        None => (
            StatusCode::OK,
            Json(json!({"path": null, "message": "No path found"})),
        ),
    }
}

async fn recall(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<RecallRequest>,
) -> impl IntoResponse {
    use allsource_core::prime::types::RecallQuery;

    if let Some(hosted) = state.hosted.as_ref() {
        let Some(tenant) = tenant_from_headers(&headers) else {
            return missing_tenant_response();
        };
        // The hosted backend takes a precomputed query vector (in-process
        // text→vector embedding downloads a model on first use and is never
        // exercised here). Reject text-only with a clear error, mirroring the
        // hosted MCP dispatch.
        let Some(vector) = req.vector else {
            return (
                StatusCode::BAD_REQUEST,
                Json(
                    json!({"error": "missing 'vector' — the hosted backend requires a precomputed query embedding"}),
                ),
            );
        };
        let top_k = req.top_k.unwrap_or(10);
        return match hosted.recall(&tenant, vector, top_k).await {
            Ok(results) => {
                // Shape to match the embedded recall contract (`{nodes:[{id,
                // type, score, depth}]}`). Hosted recall is pure vector
                // similarity (no graph expansion), so `depth` is always 0.
                let nodes: Vec<Value> = results
                    .iter()
                    .map(|(n, score)| {
                        json!({
                            "id": n.id.as_str(),
                            "type": n.node_type,
                            "score": score,
                            "depth": 0,
                        })
                    })
                    .collect();
                (StatusCode::OK, Json(json!({"nodes": nodes})))
            }
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            ),
        };
    }

    let prime = state
        .prime
        .as_ref()
        .expect("embedded prime present when hosted is None");
    // Matches the MCP path: an unavailable embedder degrades to lexical recall
    // rather than 500ing, and the response says which it was.
    let mut embed_failure: Option<String> = None;
    let vector = match req.vector {
        Some(v) => Some(v),
        None => match req.text.as_deref() {
            Some(t) => match prime.embed_text(t) {
                Ok(v) => Some(v),
                Err(e) => {
                    embed_failure = Some(e.to_string());
                    None
                }
            },
            None => None,
        },
    };

    let query = RecallQuery {
        text: req.text,
        vector,
        node_type: req.node_type,
        tenant: req.tenant_id,
        depth: req.depth.unwrap_or(1),
        top_k: req.top_k.unwrap_or(10),
        ..RecallQuery::default()
    };

    match prime.recall(query).await {
        Ok(result) => {
            let nodes: Vec<Value> = result
                .nodes
                .iter()
                .map(|sn| {
                    json!({
                        "id": sn.node.id.as_str(),
                        "type": sn.node.node_type,
                        "score": sn.score,
                        "depth": sn.depth,
                    })
                })
                .collect();
            let mut body = json!({ "nodes": nodes, "retrieval": result.retrieval });
            if let Some(note) =
                crate::tools::degraded_note(result.retrieval, embed_failure.as_deref())
            {
                body["degraded"] = json!(note);
            }
            (StatusCode::OK, Json(body))
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        ),
    }
}

async fn get_diff(State(state): State<Arc<AppState>>, headers: HeaderMap) -> impl IntoResponse {
    // The embedded diff returns a stats summary (no from/to params). HostedPrime
    // exposes the same stats via `stats`, so the hosted path serves the identical
    // summary shape rather than 501.
    if let Some(hosted) = state.hosted.as_ref() {
        let Some(tenant) = tenant_from_headers(&headers) else {
            return missing_tenant_response();
        };
        return match hosted.stats(&tenant).await {
            Ok(stats) => (
                StatusCode::OK,
                Json(json!({
                    "total_nodes": stats.total_nodes,
                    "total_edges": stats.total_edges,
                    "event_count": stats.event_count,
                })),
            ),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            ),
        };
    }
    let prime = state
        .prime
        .as_ref()
        .expect("embedded prime present when hosted is None");
    // Without from/to params, return a summary of all events
    let stats = prime.stats();
    (
        StatusCode::OK,
        Json(json!({
            "total_nodes": stats.total_nodes,
            "total_edges": stats.total_edges,
            "event_count": stats.event_count,
        })),
    )
}

#[cfg(test)]
#[path = "http_tests.rs"]
mod tests;
