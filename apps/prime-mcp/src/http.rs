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
}

// =============================================================================
// Handlers
// =============================================================================

async fn health() -> impl IntoResponse {
    Json(json!({"status": "ok"}))
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
mod tests {
    use super::*;
    use allsource_core::prime::{
        EntityId,
        recall::{IndexConfig, RecallEngine},
    };
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use tower::ServiceExt; // for `oneshot`

    async fn test_state() -> Arc<AppState> {
        let prime = Prime::open_in_memory().await.unwrap();
        let recall = RecallEngine::with_deps(prime.recall_deps(), &IndexConfig::default());
        Arc::new(AppState {
            prime: Some(Arc::new(prime)),
            recall: Some(recall),
            hosted: None,
        })
    }

    fn graph_router(state: Arc<AppState>) -> Router {
        Router::new()
            .route("/api/v1/prime/graph", get(get_full_graph))
            .with_state(state)
    }

    /// A node + edge written to the local store appear in `/api/v1/prime/graph`
    /// with full properties and the same contract Core serves.
    #[tokio::test]
    async fn graph_endpoint_returns_written_node_and_edge() {
        let state = test_state().await;
        let prime = state.prime.as_ref().unwrap();

        let org_id = prime
            .add_node("organization", json!({ "name": "Acme" }))
            .await
            .unwrap();
        let org = EntityId::node("organization", org_id.as_str()).to_wire();
        let contact_id = prime
            .add_node("contact", json!({ "name": "Alice" }))
            .await
            .unwrap();
        let contact = EntityId::node("contact", contact_id.as_str()).to_wire();
        prime
            .add_edge(&contact, &org, "works_at", None)
            .await
            .unwrap();

        let app = graph_router(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/prime/graph")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let body: Value = serde_json::from_slice(&bytes).unwrap();

        let nodes = body["nodes"].as_array().unwrap();
        assert_eq!(nodes.len(), 2);
        let org_node = nodes.iter().find(|n| n["id"] == org).unwrap();
        assert_eq!(org_node["properties"]["name"], "Acme");
        assert_eq!(org_node["has_vector"], false);

        let edges = body["edges"].as_array().unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0]["source"], contact);
        assert_eq!(edges[0]["target"], org);
        assert_eq!(edges[0]["relation"], "works_at");

        assert_eq!(body["stats"]["node_count"], 2);
        assert_eq!(body["stats"]["edge_count"], 1);
        assert_eq!(body["has_more"], false);
    }

    // ─── MCP-over-HTTP (Streamable HTTP transport) ────────────────────────

    fn mcp_router(state: Arc<AppState>) -> Router {
        Router::new()
            .route("/mcp", post(mcp_handler))
            .with_state(state)
    }

    async fn mcp_post(app: &Router, payload: Value) -> (StatusCode, Value) {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/mcp")
                    .header("content-type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = resp.status();
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap()
        };
        (status, body)
    }

    /// A client can complete the full MCP handshake over HTTP: initialize,
    /// tools/list, and a tools/call — the bead's acceptance criterion.
    #[tokio::test]
    async fn mcp_http_initialize_list_and_call() {
        let app = mcp_router(test_state().await);

        let (status, body) = mcp_post(
            &app,
            json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize" }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["result"]["serverInfo"]["name"], "allsource-prime");
        assert!(body["result"]["protocolVersion"].is_string());

        let (status, body) = mcp_post(
            &app,
            json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let tools = body["result"]["tools"].as_array().unwrap();
        assert!(!tools.is_empty(), "tools/list must return the Prime tools");

        let (status, body) = mcp_post(
            &app,
            json!({
                "jsonrpc": "2.0", "id": 3, "method": "tools/call",
                "params": { "name": "prime_stats", "arguments": {} }
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            !body["result"].is_null(),
            "tools/call must return a result envelope"
        );
    }

    /// Notifications (no id, no reply) get 202 Accepted with an empty body.
    #[tokio::test]
    async fn mcp_http_notification_returns_202() {
        let app = mcp_router(test_state().await);
        let (status, body) = mcp_post(
            &app,
            json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        )
        .await;
        assert_eq!(status, StatusCode::ACCEPTED);
        assert_eq!(body, Value::Null);
    }

    /// Malformed JSON yields a JSON-RPC parse-error envelope (not a 500).
    #[tokio::test]
    async fn mcp_http_parse_error_is_jsonrpc_envelope() {
        let app = mcp_router(test_state().await);
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/mcp")
                    .header("content-type", "application/json")
                    .body(Body::from("not json"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["error"]["code"], -32700);
    }

    // ─── Hosted, tenant-scoped /mcp dispatch ──────────────────────────────

    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method as wm_method, path as wm_path},
    };

    /// Mount the standard empty-Core pair (GET query → `{events:[]}`, POST
    /// ingest → 200) and return an `AppState` whose `hosted` engine points at it.
    async fn hosted_state(server: &MockServer) -> Arc<AppState> {
        Mock::given(wm_method("GET"))
            .and(wm_path("/api/v1/events/query"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "events": [] })))
            .mount(server)
            .await;
        Mock::given(wm_method("POST"))
            .and(wm_path("/api/v1/events"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "ok": true })))
            .mount(server)
            .await;

        // Stateless: no embedded prime/recall — exactly the production hosted
        // shape. Every request is served through `hosted`.
        let hosted =
            HostedPrime::connect(server.uri(), None, 8, std::time::Duration::from_secs(60));
        Arc::new(AppState {
            prime: None,
            recall: None,
            hosted: Some(Arc::new(hosted)),
        })
    }

    /// Like [`hosted_state`], but backed by a STATEFUL fake Core (writes are
    /// readable back) — for the write-then-read-back round-trip tests, which the
    /// always-empty mock can't satisfy under the t-d90426 cold-write semantics.
    async fn hosted_state_stateful(server: &MockServer) -> Arc<AppState> {
        crate::hosted_test_core::mount_stateful_core(server).await;
        let hosted =
            HostedPrime::connect(server.uri(), None, 8, std::time::Duration::from_secs(60));
        Arc::new(AppState {
            prime: None,
            recall: None,
            hosted: Some(Arc::new(hosted)),
        })
    }

    async fn mcp_post_with_tenant(
        app: &Router,
        tenant: Option<&str>,
        payload: Value,
    ) -> (StatusCode, Value) {
        let mut builder = Request::builder()
            .method("POST")
            .uri("/mcp")
            .header("content-type", "application/json");
        if let Some(t) = tenant {
            builder = builder.header("x-tenant-id", t);
        }
        let resp = app
            .clone()
            .oneshot(builder.body(Body::from(payload.to_string())).unwrap())
            .await
            .unwrap();
        let status = resp.status();
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap()
        };
        (status, body)
    }

    /// With a `HostedPrime` engine and an `X-Tenant-Id`, a `prime_add_node` write
    /// followed by a `prime_search` round-trips through the hosted path, proving
    /// both the hosted dispatch and tenant threading end-to-end.
    #[tokio::test]
    async fn mcp_hosted_path_add_node_then_search() {
        let server = MockServer::start().await;
        let app = mcp_router(hosted_state_stateful(&server).await);

        let (status, body) = mcp_post_with_tenant(
            &app,
            Some("tenant-a"),
            json!({
                "jsonrpc": "2.0", "id": 1, "method": "tools/call",
                "params": { "name": "prime_add_node", "arguments": { "type": "contact", "properties": { "name": "Alice" } } }
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_ne!(
            body["result"]["isError"],
            json!(true),
            "add_node should succeed on hosted path"
        );

        let (status, body) = mcp_post_with_tenant(
            &app,
            Some("tenant-a"),
            json!({
                "jsonrpc": "2.0", "id": 2, "method": "tools/call",
                "params": { "name": "prime_search", "arguments": { "type": "contact" } }
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let text = body["result"]["content"][0]["text"].as_str().unwrap();
        assert!(
            text.contains("Alice"),
            "hosted search should return the added node; got {text}"
        );
    }

    /// A tool `HostedPrime` does not implement returns a clear tool-error (not a
    /// crash) on the hosted path.
    #[tokio::test]
    async fn mcp_hosted_path_unsupported_tool_errors() {
        let server = MockServer::start().await;
        let app = mcp_router(hosted_state(&server).await);

        let (status, body) = mcp_post_with_tenant(
            &app,
            Some("tenant-a"),
            json!({
                "jsonrpc": "2.0", "id": 1, "method": "tools/call",
                "params": { "name": "prime_index", "arguments": {} }
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["result"]["isError"], json!(true));
        let text = body["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("not yet available on the hosted backend"));
    }

    /// In the stateless hosted deployment (no embedded `prime`), a `/mcp`
    /// request without `X-Tenant-Id` has no backend to serve it — it returns a
    /// 400 with a clear "X-Tenant-Id required" message rather than dispatching
    /// against a store that does not exist.
    #[tokio::test]
    async fn mcp_no_tenant_stateless_returns_400() {
        let server = MockServer::start().await;
        let app = mcp_router(hosted_state(&server).await);

        let (status, body) = mcp_post_with_tenant(
            &app,
            None,
            json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize" }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(
            body["error"].as_str().unwrap().contains("X-Tenant-Id"),
            "expected an X-Tenant-Id error, got {body}"
        );
    }

    // ─── Hosted, tenant-scoped REST handlers ──────────────────────────────

    /// Build a router with the REST routes exercised below, sharing one state.
    fn rest_router(state: Arc<AppState>) -> Router {
        Router::new()
            .route("/api/v1/prime/nodes", post(create_node))
            .route("/api/v1/prime/nodes/{id}", get(get_node))
            .route("/api/v1/prime/graph", get(get_full_graph))
            .route("/api/v1/prime/recall", post(recall))
            .with_state(state)
    }

    /// Issue a request with optional `X-Tenant-Id` and parse the JSON body.
    async fn rest_request(
        app: &Router,
        method: &str,
        uri: &str,
        tenant: Option<&str>,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut builder = Request::builder().method(method).uri(uri);
        if let Some(t) = tenant {
            builder = builder.header("x-tenant-id", t);
        }
        let req = if let Some(b) = body {
            builder
                .header("content-type", "application/json")
                .body(Body::from(b.to_string()))
                .unwrap()
        } else {
            builder.body(Body::empty()).unwrap()
        };
        let resp = app.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let val = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap()
        };
        (status, val)
    }

    /// `POST /nodes` then `GET /nodes/{entity_id}` round-trips through the hosted
    /// engine (warm cache over an empty Core), with the embedded response shape.
    #[tokio::test]
    async fn rest_hosted_create_then_get_node() {
        let server = MockServer::start().await;
        let app = rest_router(hosted_state_stateful(&server).await);

        let (status, body) = rest_request(
            &app,
            "POST",
            "/api/v1/prime/nodes",
            Some("tenant-a"),
            Some(json!({ "type": "contact", "properties": { "name": "Alice" } })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        let entity_id = body["entity_id"].as_str().unwrap().to_string();
        assert!(body["node_id"].is_string());

        let (status, body) = rest_request(
            &app,
            "GET",
            &format!("/api/v1/prime/nodes/{entity_id}"),
            Some("tenant-a"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["type"], "contact");
        assert_eq!(body["properties"]["name"], "Alice");
        // Embedded detail shape: domain/labels/timestamps present.
        assert!(body["created_at"].is_string());
        assert!(body.get("labels").is_some());
    }

    /// `GET /graph` over the hosted engine returns the same `FullGraph` contract
    /// (`nodes`/`edges`/`stats`/`has_more`) the embedded path serves.
    #[tokio::test]
    async fn rest_hosted_full_graph_shape() {
        let server = MockServer::start().await;
        let app = rest_router(hosted_state_stateful(&server).await);

        // Seed two nodes so the graph is non-empty.
        let (_, a) = rest_request(
            &app,
            "POST",
            "/api/v1/prime/nodes",
            Some("tenant-a"),
            Some(json!({ "type": "contact", "properties": { "name": "Alice" } })),
        )
        .await;
        let _ = a;
        let (_, _b) = rest_request(
            &app,
            "POST",
            "/api/v1/prime/nodes",
            Some("tenant-a"),
            Some(json!({ "type": "contact", "properties": { "name": "Bob" } })),
        )
        .await;

        let (status, body) =
            rest_request(&app, "GET", "/api/v1/prime/graph", Some("tenant-a"), None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["stats"]["node_count"], 2);
        assert!(body["nodes"].is_array());
        assert_eq!(body["has_more"], false);
    }

    /// `POST /recall` with a precomputed query vector routes through the hosted
    /// engine and returns the embedded recall shape (`{nodes:[…]}`). With an
    /// empty Core and no embeddings, the result is an empty `nodes` array —
    /// proving the hosted path + tenant threading + response shape end-to-end
    /// without invoking the in-process embedder. (Vector→node hydration is
    /// covered by `hosted.rs`'s `embed_then_recall_returns_the_embedded_node`.)
    #[tokio::test]
    async fn rest_hosted_recall_shape() {
        let server = MockServer::start().await;
        let app = rest_router(hosted_state(&server).await);

        let (status, body) = rest_request(
            &app,
            "POST",
            "/api/v1/prime/recall",
            Some("tenant-a"),
            Some(json!({ "vector": [1.0, 0.0, 0.0, 0.0], "top_k": 5 })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["nodes"].is_array(), "recall must return a nodes array");
        assert_eq!(body["nodes"].as_array().unwrap().len(), 0);
    }

    /// `POST /recall` without a `vector` on the hosted path is a clear 400
    /// (the hosted backend never runs the in-process embedder).
    #[tokio::test]
    async fn rest_hosted_recall_without_vector_is_400() {
        let server = MockServer::start().await;
        let app = rest_router(hosted_state(&server).await);

        let (status, body) = rest_request(
            &app,
            "POST",
            "/api/v1/prime/recall",
            Some("tenant-a"),
            Some(json!({ "top_k": 5 })),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body["error"].as_str().unwrap().contains("vector"));
    }

    /// A hosted REST request without `X-Tenant-Id` returns 400 with a clear
    /// "X-Tenant-Id required" message — there is no default tenant.
    #[tokio::test]
    async fn rest_hosted_missing_tenant_returns_400() {
        let server = MockServer::start().await;
        let app = rest_router(hosted_state(&server).await);

        let (status, body) = rest_request(
            &app,
            "POST",
            "/api/v1/prime/nodes",
            None,
            Some(json!({ "type": "contact", "properties": {} })),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(
            body["error"].as_str().unwrap().contains("X-Tenant-Id"),
            "expected an X-Tenant-Id error, got {body}"
        );
    }
}
