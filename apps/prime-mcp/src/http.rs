//! HTTP REST API server for Prime (axum-based).
//!
//! Exposes the full Prime API as REST endpoints, consistent with Core's HTTP stack.
//! Activated via `--mode http --port 3905`. Every route is declared once in
//! [`http_routes`], which builds both the router and the `OpenAPI` document
//! served at [`OPENAPI_PATH`].

use std::{collections::HashMap, sync::Arc};

use allsource_core::{
    AllSourceError,
    prime::{Direction, Prime, PrimeError, hosted::HostedPrime, recall::RecallEngine},
};
use axum::{
    Extension, Json, Router,
    body::Bytes,
    extract::{Path, Query, Request, State},
    http::{HeaderMap, StatusCode},
    middleware::{Next, from_fn_with_state},
    response::{Html, IntoResponse, Response},
    routing::{MethodRouter, delete, get, patch, post, put},
};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};
use tower_http::trace::TraceLayer;

use crate::contract;

/// Path of the generated `OpenAPI` document.
pub const OPENAPI_PATH: &str = "/api/v1/prime/openapi.json";

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
    pub hosted: Option<Arc<HostedPrime>>,
    /// The gateway's service credential (`PRIME_API_KEY`). Required on `/mcp`
    /// when set; required on every hosted request, where it is what makes the
    /// tenant header trustworthy.
    pub api_key: Option<String>,
}

/// The tenant bound to an authenticated hosted request. Inserted only by
/// [`auth_boundary`] after the service credential matched; handlers never read
/// a tenant from anywhere else.
#[derive(Clone, Debug)]
pub struct TenantScope(pub String);

type Scope = Option<Extension<TenantScope>>;

fn scoped_tenant(scope: Scope) -> Option<String> {
    scope.map(|Extension(TenantScope(t))| t)
}

/// One routed endpoint. The router and the `OpenAPI` document are both built
/// from [`http_routes`], so a route cannot be served undocumented.
pub struct HttpRoute {
    pub method: &'static str,
    pub path: &'static str,
    pub summary: &'static str,
    pub success: u16,
    pub request: Option<Value>,
    handler: MethodRouter<Arc<AppState>>,
}

impl HttpRoute {
    fn new(
        method: &'static str,
        path: &'static str,
        summary: &'static str,
        success: u16,
        handler: MethodRouter<Arc<AppState>>,
    ) -> Self {
        Self {
            method,
            path,
            summary,
            success,
            request: None,
            handler,
        }
    }

    fn body(mut self, schema: Value) -> Self {
        self.request = Some(schema);
        self
    }
}

#[allow(clippy::needless_pass_by_value)]
fn object_schema(required: &[&str], properties: Value) -> Value {
    json!({ "type": "object", "required": required, "properties": properties })
}

fn with_assertion(mut schema: Value, source_key: &str) -> Value {
    if let (Some(props), Value::Object(extra)) = (
        schema.get_mut("properties").and_then(Value::as_object_mut),
        contract::assertion_schema(source_key),
    ) {
        props.extend(extra);
    }
    schema
}

/// Every route this server answers, in registration order. Profiling routes
/// (feature `profiling`) are merged separately and stay undocumented.
pub fn http_routes() -> Vec<HttpRoute> {
    vec![
        HttpRoute::new(
            "post",
            "/api/v1/prime/nodes",
            "Create a node",
            201,
            post(create_node),
        )
        .body(with_assertion(
            object_schema(
                &["type", "properties"],
                json!({ "type": {"type": "string"}, "properties": {"type": "object"} }),
            ),
            "source",
        )),
        HttpRoute::new(
            "get",
            "/api/v1/prime/nodes/{id}",
            "Get a node",
            200,
            get(get_node),
        ),
        HttpRoute::new(
            "patch",
            "/api/v1/prime/nodes/{id}",
            "Update a node",
            200,
            patch(update_node),
        )
        .body(with_assertion(
            object_schema(&["properties"], json!({ "properties": {"type": "object"} })),
            "source",
        )),
        HttpRoute::new(
            "delete",
            "/api/v1/prime/nodes/{id}",
            "Delete a node",
            200,
            delete(delete_node),
        ),
        HttpRoute::new(
            "get",
            "/api/v1/prime/nodes/{id}/neighbors",
            "Neighbors of a node",
            200,
            get(get_neighbors),
        ),
        HttpRoute::new(
            "get",
            "/api/v1/prime/nodes/{id}/subgraph",
            "Two-hop subgraph around a node",
            200,
            get(get_subgraph),
        ),
        HttpRoute::new(
            "get",
            "/api/v1/prime/nodes/{id}/history",
            "Event history of a node",
            200,
            get(get_history),
        ),
        HttpRoute::new(
            "get",
            "/api/v1/prime/nodes/{id}/fields/{field}/provenance",
            "Provenance of one node field",
            200,
            get(field_provenance),
        ),
        HttpRoute::new(
            "post",
            "/api/v1/prime/edges",
            "Create an edge",
            201,
            post(create_edge),
        )
        .body(with_assertion(
            object_schema(
                &["source", "target", "relation"],
                json!({
                    "source": {"type": "string"},
                    "target": {"type": "string"},
                    "relation": {"type": "string"},
                    "properties": {"type": "object"},
                    "weight": {"type": "number"}
                }),
            ),
            contract::ASSERTION_SOURCE_KEY,
        )),
        HttpRoute::new(
            "delete",
            "/api/v1/prime/edges/{id}",
            "Delete an edge",
            200,
            delete(delete_edge),
        ),
        HttpRoute::new(
            "post",
            "/api/v1/prime/types",
            "Register or re-register a node or edge kind",
            201,
            post(define_type),
        )
        .body(object_schema(
            &["type_name", "kind", "schema"],
            json!({
                "type_name": {"type": "string"},
                "kind": {"type": "string", "enum": ["node", "edge"]},
                "schema": {"type": "object"}
            }),
        )),
        HttpRoute::new(
            "get",
            "/api/v1/prime/types",
            "List registered kinds and the enforcement mode",
            200,
            get(list_types),
        ),
        HttpRoute::new(
            "put",
            "/api/v1/prime/enforcement",
            "Set the enforcement mode",
            200,
            put(set_enforcement),
        )
        .body(object_schema(
            &["mode"],
            json!({ "mode": {"type": "string", "enum": ["open", "warn", "strict"]} }),
        )),
        HttpRoute::new(
            "get",
            "/api/v1/prime/enforcement",
            "Current enforcement mode",
            200,
            get(get_enforcement),
        ),
        HttpRoute::new(
            "post",
            "/api/v1/prime/vectors",
            "Store a vector",
            201,
            post(store_vector),
        )
        .body(object_schema(
            &["id"],
            json!({
                "id": {"type": "string"},
                "text": {"type": "string"},
                "vector": {"type": "array", "items": {"type": "number"}},
                "metadata": {"type": "object"}
            }),
        )),
        HttpRoute::new(
            "post",
            "/api/v1/prime/vectors/search",
            "Vector similarity search",
            200,
            post(search_vectors),
        )
        .body(object_schema(
            &[],
            json!({
                "vector": {"type": "array", "items": {"type": "number"}},
                "text": {"type": "string"},
                "top_k": {"type": "integer"}
            }),
        )),
        HttpRoute::new(
            "delete",
            "/api/v1/prime/vectors/{id}",
            "Delete a vector",
            200,
            delete(delete_vector),
        ),
        HttpRoute::new(
            "post",
            "/api/v1/prime/shortest-path",
            "Shortest path between two nodes",
            200,
            post(shortest_path),
        )
        .body(object_schema(
            &["from", "to"],
            json!({
                "from": {"type": "string"},
                "to": {"type": "string"},
                "relation": {"type": "string"}
            }),
        )),
        HttpRoute::new(
            "post",
            "/api/v1/prime/recall",
            "Hybrid recall",
            200,
            post(recall),
        )
        .body(object_schema(
            &[],
            json!({
                "vector": {"type": "array", "items": {"type": "number"}},
                "text": {"type": "string"},
                "node_type": {"type": "string"},
                "depth": {"type": "integer"},
                "top_k": {"type": "integer"},
                "tenant_id": {"type": "string", "description": "Embedded only; hosted requests must omit it or match the authenticated tenant."}
            }),
        )),
        HttpRoute::new(
            "get",
            "/api/v1/prime/diff",
            "Graph change summary",
            200,
            get(get_diff),
        ),
        HttpRoute::new(
            "get",
            "/api/v1/prime/stats",
            "Graph statistics",
            200,
            get(get_stats),
        ),
        HttpRoute::new(
            "get",
            "/api/v1/prime/graph",
            "Full materialized graph",
            200,
            get(get_full_graph),
        ),
        HttpRoute::new(
            "get",
            "/api/v1/prime/graph.html",
            "Self-contained graph viewer page",
            200,
            get(graph_viewer),
        ),
        HttpRoute::new(
            "get",
            OPENAPI_PATH,
            "This OpenAPI document",
            200,
            get(openapi_json),
        ),
        HttpRoute::new(
            "post",
            "/mcp",
            "MCP JSON-RPC over HTTP",
            200,
            post(mcp_handler),
        )
        .body(json!({ "type": "object", "description": "A JSON-RPC 2.0 request" })),
        HttpRoute::new("get", "/health", "Liveness and writability", 200, get(health)),
    ]
}

/// The full application router, including the auth boundary.
pub fn router(state: Arc<AppState>) -> Router {
    http_routes()
        .into_iter()
        .fold(Router::new(), |r, route| r.route(route.path, route.handler))
        .merge(crate::profiling::routes())
        .layer(from_fn_with_state(Arc::clone(&state), auth_boundary))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// `OpenAPI` 3.1 document for every route in [`http_routes`].
pub fn openapi_document() -> Value {
    let mut paths = serde_json::Map::new();
    for r in http_routes() {
        let parameters: Vec<Value> = r
            .path
            .split('/')
            .filter_map(|seg| seg.strip_prefix('{').and_then(|p| p.strip_suffix('}')))
            .map(|name| {
                json!({"name": name, "in": "path", "required": true, "schema": {"type": "string"}})
            })
            .collect();
        let mut op = json!({
            "summary": r.summary,
            "operationId": format!("{}{}", r.method, r.path.replace(['/', '{', '}', '.', '-'], "_")),
            "responses": {
                r.success.to_string(): {"description": "Success", "content": {"application/json": {"schema": {"type": "object"}}}},
                "400": {"description": "Invalid request or refused write; a typed refusal names the field, expected kind and received value"},
                "401": {"description": "Missing or wrong service credential"},
                "403": {"description": "A tenant in the request conflicts with the authenticated tenant"}
            }
        });
        if !parameters.is_empty() {
            op["parameters"] = Value::Array(parameters);
        }
        if let Some(schema) = r.request {
            op["requestBody"] =
                json!({"required": true, "content": {"application/json": {"schema": schema}}});
        }
        let entry = paths.entry(r.path.to_string()).or_insert_with(|| json!({}));
        entry[r.method] = op;
    }
    json!({
        "openapi": "3.1.0",
        "info": {"title": "AllSource Prime MCP server API", "version": env!("CARGO_PKG_VERSION")},
        "paths": paths
    })
}

async fn openapi_json() -> impl IntoResponse {
    Json(openapi_document())
}

/// Start the HTTP server on the given port.
pub async fn serve(state: Arc<AppState>, port: u16) -> anyhow::Result<()> {
    let app = router(state);

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
// Auth boundary
// =============================================================================

fn provided_credential(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.strip_prefix("Bearer ").unwrap_or(s))
        .or_else(|| headers.get("x-api-key").and_then(|v| v.to_str().ok()))
}

fn credential_matches(headers: &HeaderMap, expected: &str) -> bool {
    provided_credential(headers).is_some_and(|got| {
        got.len() == expected.len()
            && got
                .bytes()
                .zip(expected.bytes())
                .fold(0u8, |acc, (a, b)| acc | (a ^ b))
                == 0
    })
}

fn tenant_from_headers(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-tenant-id")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
}

fn refuse(status: StatusCode, msg: &str) -> Response {
    (status, Json(json!({ "error": msg }))).into_response()
}

/// Binds a hosted request to its tenant before any handler runs.
///
/// Trust model: the Control Plane authenticates the end user, resolves their
/// tenant, and forwards with `X-Tenant-Id` plus the shared `PRIME_API_KEY`
/// service credential, which only it holds. So the header is trusted only on
/// a request carrying that credential; without the credential configured,
/// hosted requests are refused outright. A tenant supplied anywhere else in the
/// request (`?tenant_id=`, a body field, a tool argument) never sets scope and
/// is refused when it differs.
///
/// Embedded mode keeps its existing behaviour: `/mcp` requires the key when one
/// is set, REST is open, and no tenant is bound.
async fn auth_boundary(
    State(state): State<Arc<AppState>>,
    mut req: Request,
    next: Next,
) -> Response {
    let path = req.uri().path().to_string();
    let is_mcp = path == "/mcp";
    if !is_mcp && !path.starts_with("/api/v1/prime") {
        return next.run(req).await;
    }
    if state.hosted.is_none() {
        if is_mcp
            && let Some(key) = state.api_key.as_deref()
            && !credential_matches(req.headers(), key)
        {
            return refuse(StatusCode::UNAUTHORIZED, "invalid or missing API key");
        }
        return next.run(req).await;
    }

    let Some(key) = state.api_key.as_deref() else {
        return refuse(
            StatusCode::SERVICE_UNAVAILABLE,
            "hosted Prime requires PRIME_API_KEY; without it no request can be bound to a tenant",
        );
    };
    if !credential_matches(req.headers(), key) {
        return refuse(StatusCode::UNAUTHORIZED, "invalid or missing API key");
    }
    if path == OPENAPI_PATH || path == "/api/v1/prime/graph.html" {
        return next.run(req).await;
    }
    let Some(tenant) = tenant_from_headers(req.headers()) else {
        return missing_tenant_response().into_response();
    };
    let query_tenant = Query::<HashMap<String, String>>::try_from_uri(req.uri())
        .ok()
        .and_then(|Query(q)| q.get("tenant_id").cloned())
        .map(Value::String);
    if let Some(msg) = contract::conflicting_tenant(query_tenant.as_ref(), &tenant) {
        return refuse(StatusCode::FORBIDDEN, &msg);
    }
    req.extensions_mut().insert(TenantScope(tenant));
    next.run(req).await
}

// =============================================================================
// MCP-over-HTTP (Streamable HTTP transport)
// =============================================================================

/// MCP Streamable-HTTP endpoint. Accepts a single JSON-RPC request and returns
/// the JSON-RPC response as `application/json`, or `202 Accepted` for
/// notifications (which have no reply).
///
/// Hosted requests are dispatched through
/// [`crate::hosted_dispatch::handle_request_hosted`] for the tenant bound by
/// [`auth_boundary`]; embedded requests go to the local [`Prime`] via
/// [`crate::dispatch::handle_request`], identical to the stdio transport.
async fn mcp_handler(
    State(state): State<Arc<AppState>>,
    scope: Scope,
    body: Bytes,
) -> impl IntoResponse {
    let req: crate::protocol::Request = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            let resp = crate::protocol::Response::error(None, -32700, format!("Parse error: {e}"));
            return (StatusCode::OK, Json(resp)).into_response();
        }
    };

    if let Some(hosted) = state.hosted.as_ref() {
        let Some(tenant) = scoped_tenant(scope) else {
            return missing_tenant_response().into_response();
        };
        return match crate::hosted_dispatch::handle_request_hosted(hosted, &tenant, &req).await {
            Some(resp) => (StatusCode::OK, Json(resp)).into_response(),
            None => StatusCode::ACCEPTED.into_response(),
        };
    }

    let (Some(prime), Some(recall)) = (state.prime.as_ref(), state.recall.as_ref()) else {
        return missing_tenant_response().into_response();
    };
    match crate::dispatch::handle_request(prime, recall, false, 1000, &req).await {
        Some(resp) => (StatusCode::OK, Json(resp)).into_response(),
        None => StatusCode::ACCEPTED.into_response(),
    }
}

/// Standard 400 for the hosted deployment when a request has no bound tenant.
/// The hosted engine is per-tenant, so there is no default tenant to fall back
/// to and no embedded store to serve from.
fn missing_tenant_response() -> (StatusCode, Json<Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({ "error": "X-Tenant-Id required: this server is stateless (hosted mode)" })),
    )
}

fn embedded(state: &AppState) -> &Prime {
    state
        .prime
        .as_ref()
        .expect("embedded prime present when hosted is None")
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
struct DefineTypeRequest {
    type_name: String,
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
/// Mirrors Core's contract. On the embedded store `tenant_id` filters on the
/// `properties.tenant_id` marker; on the hosted backend the tenant comes from
/// the credential and a differing `tenant_id` is refused before this runs.
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
    /// the embedded backend: the hosted one is scoped by its credential.
    tenant_id: Option<String>,
}

type Reply = (StatusCode, Json<Value>);

fn bad_request(msg: &str) -> Reply {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": msg })))
}

fn parse_body<T: DeserializeOwned>(body: &Value) -> Result<T, Reply> {
    serde_json::from_value(body.clone()).map_err(|e| bad_request(&e.to_string()))
}

/// Status and body for a refused embedded write. A typed refusal carries the
/// structured `violation`.
fn embedded_write_error(e: &PrimeError) -> Reply {
    let status = match e {
        PrimeError::NodeNotFound(_) | PrimeError::EdgeNotFound(_) => StatusCode::NOT_FOUND,
        PrimeError::ValidationFailed(_) | PrimeError::SchemaViolation(_) => StatusCode::BAD_REQUEST,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    let mut body = json!({ "error": contract::prime_error_text(e) });
    if let PrimeError::SchemaViolation(v) = e {
        body["violation"] = json!(v);
    }
    (status, Json(body))
}

/// Status and body for a refused hosted write.
fn hosted_write_error(e: &AllSourceError) -> Reply {
    let status = match e {
        AllSourceError::EntityNotFound(_) => StatusCode::NOT_FOUND,
        AllSourceError::ValidationError(_) | AllSourceError::InvalidInput(_) => {
            StatusCode::BAD_REQUEST
        }
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (
        status,
        Json(json!({ "error": contract::hosted_error_text(e) })),
    )
}

fn server_error(e: impl std::fmt::Display) -> Reply {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": e.to_string() })),
    )
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

async fn get_stats(State(state): State<Arc<AppState>>, scope: Scope) -> impl IntoResponse {
    if let Some(hosted) = state.hosted.as_ref() {
        let Some(tenant) = scoped_tenant(scope) else {
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
            Err(e) => server_error(e),
        };
    }
    let stats = embedded(&state).stats();
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

/// `GET /api/v1/prime/graph` — full materialized knowledge graph, identical
/// contract to Core's `/api/v1/prime/graph` (see the doc comment on Core's
/// `get_full_graph` for the verbatim JSON shape).
async fn get_full_graph(
    State(state): State<Arc<AppState>>,
    scope: Scope,
    Query(q): Query<GraphQuery>,
) -> impl IntoResponse {
    if let Some(hosted) = state.hosted.as_ref() {
        let Some(tenant) = scoped_tenant(scope) else {
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
            Err(e) => server_error(e),
        };
    }
    let graph =
        embedded(&state).full_graph(q.tenant_id.as_deref(), q.node_type.as_deref(), q.limit);
    (
        StatusCode::OK,
        Json(serde_json::to_value(&graph).unwrap_or(Value::Null)),
    )
}

async fn create_node(
    State(state): State<Arc<AppState>>,
    scope: Scope,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let req: CreateNodeRequest = match parse_body(&body) {
        Ok(r) => r,
        Err(reply) => return reply,
    };
    let assertion = match contract::assertion_from_args(&body, "source") {
        Ok(a) => a,
        Err(e) => return bad_request(&e),
    };
    let created = |id: &str| {
        let entity_id = allsource_core::prime::EntityId::node(&req.node_type, id).to_wire();
        (
            StatusCode::CREATED,
            Json(json!({"node_id": id, "entity_id": entity_id})),
        )
    };
    if let Some(hosted) = state.hosted.as_ref() {
        let Some(tenant) = scoped_tenant(scope) else {
            return missing_tenant_response();
        };
        return match hosted
            .add_node_asserted(&tenant, &req.node_type, req.properties.clone(), assertion)
            .await
        {
            Ok(id) => created(id.as_str()),
            Err(e) => hosted_write_error(&e),
        };
    }
    match embedded(&state)
        .add_node_asserted(&req.node_type, req.properties.clone(), assertion)
        .await
    {
        Ok(id) => created(id.as_str()),
        Err(e) => embedded_write_error(&e),
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
    scope: Scope,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let not_found = || {
        (
            StatusCode::NOT_FOUND,
            Json(json!({"error": format!("node not found: {id}")})),
        )
    };
    if let Some(hosted) = state.hosted.as_ref() {
        let Some(tenant) = scoped_tenant(scope) else {
            return missing_tenant_response();
        };
        return match hosted.get_node(&tenant, &id).await {
            Ok(Some(node)) => (StatusCode::OK, Json(node_detail_json(&node))),
            Ok(None) => not_found(),
            Err(e) => server_error(e),
        };
    }
    match embedded(&state).get_node(&id) {
        Some(node) => (StatusCode::OK, Json(node_detail_json(&node))),
        None => not_found(),
    }
}

async fn update_node(
    State(state): State<Arc<AppState>>,
    scope: Scope,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let req: UpdateNodeRequest = match parse_body(&body) {
        Ok(r) => r,
        Err(reply) => return reply,
    };
    let assertion = match contract::assertion_from_args(&body, "source") {
        Ok(a) => a,
        Err(e) => return bad_request(&e),
    };
    if let Some(hosted) = state.hosted.as_ref() {
        let Some(tenant) = scoped_tenant(scope) else {
            return missing_tenant_response();
        };
        return match hosted
            .update_node_asserted(&tenant, &id, req.properties, assertion)
            .await
        {
            Ok(()) => (StatusCode::OK, Json(json!({"updated": true}))),
            Err(e) => hosted_write_error(&e),
        };
    }
    match embedded(&state)
        .update_node_asserted(&id, req.properties, assertion)
        .await
    {
        Ok(()) => (StatusCode::OK, Json(json!({"updated": true}))),
        Err(e) => embedded_write_error(&e),
    }
}

async fn delete_node(
    State(state): State<Arc<AppState>>,
    scope: Scope,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Some(hosted) = state.hosted.as_ref() {
        let Some(tenant) = scoped_tenant(scope) else {
            return missing_tenant_response();
        };
        return match hosted.delete_node(&tenant, &id).await {
            Ok(()) => (StatusCode::OK, Json(json!({"deleted": true}))),
            Err(e) => (StatusCode::NOT_FOUND, Json(json!({"error": e.to_string()}))),
        };
    }
    match embedded(&state).delete_node(&id).await {
        Ok(()) => (StatusCode::OK, Json(json!({"deleted": true}))),
        Err(e) => (StatusCode::NOT_FOUND, Json(json!({"error": e.to_string()}))),
    }
}

fn node_brief(n: &allsource_core::prime::Node) -> Value {
    json!({"id": n.id.as_str(), "type": n.node_type, "properties": n.properties})
}

async fn get_neighbors(
    State(state): State<Arc<AppState>>,
    scope: Scope,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Some(hosted) = state.hosted.as_ref() {
        let Some(tenant) = scoped_tenant(scope) else {
            return missing_tenant_response();
        };
        return match hosted.neighbors(&tenant, &id, None, Direction::Both).await {
            Ok(nodes) => {
                let nodes_json: Vec<Value> = nodes.iter().map(node_brief).collect();
                (StatusCode::OK, Json(json!({"nodes": nodes_json})))
            }
            Err(e) => server_error(e),
        };
    }
    let nodes = embedded(&state).neighbors(&id, None, Direction::Both);
    let nodes_json: Vec<Value> = nodes.iter().map(node_brief).collect();
    (StatusCode::OK, Json(json!({"nodes": nodes_json})))
}

async fn get_subgraph(
    State(state): State<Arc<AppState>>,
    scope: Scope,
    Path(id): Path<String>,
) -> impl IntoResponse {
    // HostedPrime has no subgraph/ego-network traversal yet (only single-hop
    // neighbors). Rather than fake a depth-2 expansion or silently return a
    // 1-hop set under the subgraph contract, return 501 on the hosted path so
    // callers know the capability is unavailable on the stateless backend.
    if state.hosted.is_some() {
        if scoped_tenant(scope).is_none() {
            return missing_tenant_response();
        }
        return (
            StatusCode::NOT_IMPLEMENTED,
            Json(json!({
                "error": "subgraph traversal is not yet available on the hosted backend; \
                          use /nodes/{id}/neighbors for single-hop neighbors"
            })),
        );
    }
    let sg = embedded(&state).subgraph(&id, 2);
    let nodes_json: Vec<Value> = sg.nodes.iter().map(node_brief).collect();
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
    scope: Scope,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Some(hosted) = state.hosted.as_ref() {
        let Some(tenant) = scoped_tenant(scope) else {
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
            Err(e) => server_error(e),
        };
    }
    match embedded(&state).history(&id).await {
        Ok(entries) => {
            let events: Vec<Value> = entries
                .iter()
                .map(|e| json!({"type": e.event_type, "timestamp": e.timestamp.to_rfc3339(), "payload": e.payload}))
                .collect();
            (StatusCode::OK, Json(json!({"events": events})))
        }
        Err(e) => server_error(e),
    }
}

/// `GET /nodes/{id}/fields/{field}/provenance`. Uses the projection registered
/// for the node type; a field without one is answered under `last_write`.
async fn field_provenance(
    State(state): State<Arc<AppState>>,
    scope: Scope,
    Path((id, field)): Path<(String, String)>,
) -> impl IntoResponse {
    let Some(entity_type) = contract::entity_type_from_node_id(&id) else {
        return bad_request(&format!(
            "node_id must be in 'node:{{type}}:{{id}}' format, got: {id}"
        ));
    };
    let (observations, def) = if let Some(hosted) = state.hosted.as_ref() {
        let Some(tenant) = scoped_tenant(scope) else {
            return missing_tenant_response();
        };
        let defs = match hosted.projection_defs(&tenant).await {
            Ok(d) => d,
            Err(e) => return server_error(e),
        };
        match hosted.observations(&tenant, &id).await {
            Ok(o) => (o, defs.into_iter().find(|d| d.entity_type == entity_type)),
            Err(e) => return server_error(e),
        }
    } else {
        let prime = embedded(&state);
        let defs = match prime.load_projection_defs().await {
            Ok(d) => d,
            Err(e) => return server_error(e),
        };
        match prime.observations(&id).await {
            Ok(o) => (o, defs.into_iter().find(|d| d.entity_type == entity_type)),
            Err(e) => return server_error(e),
        }
    };
    match contract::provenance_json(&id, &field, &observations, def) {
        Ok(v) => (StatusCode::OK, Json(v)),
        Err(e) => (StatusCode::NOT_FOUND, Json(json!({ "error": e }))),
    }
}

async fn define_type(
    State(state): State<Arc<AppState>>,
    scope: Scope,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let req: DefineTypeRequest = match parse_body(&body) {
        Ok(r) => r,
        Err(reply) => return reply,
    };
    let kind = match contract::parse_kind(&body) {
        Ok(k) => k,
        Err(e) => return bad_request(&e),
    };
    let Some(schema) = body.get("schema").filter(|s| s.is_object()).cloned() else {
        return bad_request("missing 'schema' (object)");
    };
    let conflict = |msg: String| (StatusCode::CONFLICT, Json(json!({ "error": msg })));
    if let Some(hosted) = state.hosted.as_ref() {
        let Some(tenant) = scoped_tenant(scope) else {
            return missing_tenant_response();
        };
        return match hosted
            .register_kind(&tenant, kind, &req.type_name, schema)
            .await
        {
            Ok(entry) => (StatusCode::CREATED, Json(contract::kind_json(&entry))),
            Err(e) if e.to_string().contains("re-registration refused") => {
                conflict(contract::hosted_error_text(&e))
            }
            Err(e) => hosted_write_error(&e),
        };
    }
    match embedded(&state)
        .register_kind(kind, &req.type_name, schema)
        .await
    {
        Ok(entry) => (StatusCode::CREATED, Json(contract::kind_json(&entry))),
        Err(e) if e.to_string().contains("re-registration refused") => {
            conflict(contract::prime_error_text(&e))
        }
        Err(e) => embedded_write_error(&e),
    }
}

async fn list_types(State(state): State<Arc<AppState>>, scope: Scope) -> impl IntoResponse {
    if let Some(hosted) = state.hosted.as_ref() {
        let Some(tenant) = scoped_tenant(scope) else {
            return missing_tenant_response();
        };
        let kinds = match hosted.kinds(&tenant).await {
            Ok(k) => k,
            Err(e) => return server_error(e),
        };
        return match hosted.enforcement(&tenant).await {
            Ok(mode) => (StatusCode::OK, Json(contract::kinds_json(&kinds, mode))),
            Err(e) => server_error(e),
        };
    }
    let prime = embedded(&state);
    (
        StatusCode::OK,
        Json(contract::kinds_json(&prime.schemas(), prime.enforcement())),
    )
}

async fn set_enforcement(
    State(state): State<Arc<AppState>>,
    scope: Scope,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let mode = match contract::parse_mode(&body) {
        Ok(m) => m,
        Err(e) => return bad_request(&e),
    };
    let ok = || (StatusCode::OK, Json(json!({ "mode": mode.as_str() })));
    if let Some(hosted) = state.hosted.as_ref() {
        let Some(tenant) = scoped_tenant(scope) else {
            return missing_tenant_response();
        };
        return match hosted.set_enforcement(&tenant, mode).await {
            Ok(()) => ok(),
            Err(e) => hosted_write_error(&e),
        };
    }
    match embedded(&state).set_enforcement(mode).await {
        Ok(()) => ok(),
        Err(e) => embedded_write_error(&e),
    }
}

async fn get_enforcement(State(state): State<Arc<AppState>>, scope: Scope) -> impl IntoResponse {
    if let Some(hosted) = state.hosted.as_ref() {
        let Some(tenant) = scoped_tenant(scope) else {
            return missing_tenant_response();
        };
        return match hosted.enforcement(&tenant).await {
            Ok(mode) => (StatusCode::OK, Json(json!({ "mode": mode.as_str() }))),
            Err(e) => server_error(e),
        };
    }
    (
        StatusCode::OK,
        Json(json!({ "mode": embedded(&state).enforcement().as_str() })),
    )
}

async fn create_edge(
    State(state): State<Arc<AppState>>,
    scope: Scope,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let req: CreateEdgeRequest = match parse_body(&body) {
        Ok(r) => r,
        Err(reply) => return reply,
    };
    let assertion = match contract::assertion_from_args(&body, contract::ASSERTION_SOURCE_KEY) {
        Ok(a) => a,
        Err(e) => return bad_request(&e),
    };
    if let Some(hosted) = state.hosted.as_ref() {
        let Some(tenant) = scoped_tenant(scope) else {
            return missing_tenant_response();
        };
        return match hosted
            .add_edge_asserted(
                &tenant,
                &req.source,
                &req.target,
                &req.relation,
                req.weight,
                req.properties,
                assertion,
            )
            .await
        {
            Ok(id) => (StatusCode::CREATED, Json(json!({"edge_id": id.as_str()}))),
            Err(e) => hosted_write_error(&e),
        };
    }
    match embedded(&state)
        .add_edge_asserted(
            &req.source,
            &req.target,
            &req.relation,
            req.weight,
            req.properties,
            assertion,
        )
        .await
    {
        Ok(id) => (StatusCode::CREATED, Json(json!({"edge_id": id.as_str()}))),
        Err(e) => embedded_write_error(&e),
    }
}

async fn delete_edge(
    State(state): State<Arc<AppState>>,
    scope: Scope,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Some(hosted) = state.hosted.as_ref() {
        let Some(tenant) = scoped_tenant(scope) else {
            return missing_tenant_response();
        };
        return match hosted.delete_edge(&tenant, &id).await {
            Ok(()) => (StatusCode::OK, Json(json!({"deleted": true}))),
            Err(e) => (StatusCode::NOT_FOUND, Json(json!({"error": e.to_string()}))),
        };
    }
    match embedded(&state).delete_edge(&id).await {
        Ok(()) => (StatusCode::OK, Json(json!({"deleted": true}))),
        Err(e) => (StatusCode::NOT_FOUND, Json(json!({"error": e.to_string()}))),
    }
}

async fn store_vector(
    State(state): State<Arc<AppState>>,
    scope: Scope,
    Json(req): Json<StoreVectorRequest>,
) -> impl IntoResponse {
    if let Some(hosted) = state.hosted.as_ref() {
        let Some(tenant) = scoped_tenant(scope) else {
            return missing_tenant_response();
        };
        // The hosted backend stores a precomputed embedding (in-process
        // text→vector embedding downloads a model on first use and is never
        // exercised on the hosted path). Reject text-only with a clear error,
        // mirroring the hosted MCP dispatch.
        let Some(vector) = req.vector else {
            return bad_request(
                "missing 'vector' — the hosted backend requires a precomputed embedding vector",
            );
        };
        return match hosted.embed(&tenant, &req.id, vector, req.metadata).await {
            Ok(()) => (
                StatusCode::CREATED,
                Json(json!({"stored": true, "id": req.id})),
            ),
            Err(e) => bad_request(&e.to_string()),
        };
    }
    let prime = embedded(&state);
    let vector = match req.vector {
        Some(v) => v,
        None => match req.text.as_deref() {
            Some(t) => match prime.embed_text(t) {
                Ok(v) => v,
                Err(e) => return server_error(format!("server-side embedding failed: {e}")),
            },
            None => return bad_request("missing 'vector' or 'text' — supply at least one"),
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
        Err(e) => bad_request(&e.to_string()),
    }
}

async fn search_vectors(
    State(state): State<Arc<AppState>>,
    scope: Scope,
    Json(req): Json<VectorSearchRequest>,
) -> impl IntoResponse {
    // HostedPrime exposes semantic recall (`/recall`, returning hydrated graph
    // nodes) but no raw `{id, score, text}` vector_search. Rather than reshape
    // recall into a different contract, return 501 on the hosted path and point
    // callers at /recall.
    if state.hosted.is_some() {
        if scoped_tenant(scope).is_none() {
            return missing_tenant_response();
        }
        return (
            StatusCode::NOT_IMPLEMENTED,
            Json(json!({
                "error": "raw vector search is not available on the hosted backend; use /api/v1/prime/recall"
            })),
        );
    }
    let prime = embedded(&state);
    let top_k = req.top_k.unwrap_or(10);
    let vector = match req.vector {
        Some(v) => v,
        None => match req.text.as_deref() {
            Some(t) => match prime.embed_text(t) {
                Ok(v) => v,
                Err(e) => return server_error(format!("server-side embedding failed: {e}")),
            },
            None => return bad_request("missing 'vector' or 'text' — supply at least one"),
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
    scope: Scope,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Some(hosted) = state.hosted.as_ref() {
        let Some(tenant) = scoped_tenant(scope) else {
            return missing_tenant_response();
        };
        return match hosted.delete_vector(&tenant, &id).await {
            Ok(()) => (StatusCode::OK, Json(json!({"deleted": true}))),
            Err(e) => (StatusCode::NOT_FOUND, Json(json!({"error": e.to_string()}))),
        };
    }
    match embedded(&state).delete_vector(&id).await {
        Ok(()) => (StatusCode::OK, Json(json!({"deleted": true}))),
        Err(e) => (StatusCode::NOT_FOUND, Json(json!({"error": e.to_string()}))),
    }
}

async fn shortest_path(
    State(state): State<Arc<AppState>>,
    scope: Scope,
    Json(req): Json<ShortestPathRequest>,
) -> impl IntoResponse {
    let found = |path: Vec<allsource_core::prime::Node>| {
        let nodes: Vec<Value> = path.iter().map(node_brief).collect();
        (StatusCode::OK, Json(json!({"path": nodes})))
    };
    let none = || {
        (
            StatusCode::OK,
            Json(json!({"path": null, "message": "No path found"})),
        )
    };
    if let Some(hosted) = state.hosted.as_ref() {
        let Some(tenant) = scoped_tenant(scope) else {
            return missing_tenant_response();
        };
        return match hosted
            .shortest_path(&tenant, &req.from, &req.to, req.relation.as_deref())
            .await
        {
            Ok(Some(path)) => found(path),
            Ok(None) => none(),
            Err(e) => server_error(e),
        };
    }
    match embedded(&state).shortest_path(&req.from, &req.to, req.relation.as_deref()) {
        Some(path) => found(path),
        None => none(),
    }
}

async fn recall(
    State(state): State<Arc<AppState>>,
    scope: Scope,
    Json(req): Json<RecallRequest>,
) -> impl IntoResponse {
    use allsource_core::prime::types::RecallQuery;

    if let Some(hosted) = state.hosted.as_ref() {
        let Some(tenant) = scoped_tenant(scope) else {
            return missing_tenant_response();
        };
        let supplied = req.tenant_id.clone().map(Value::String);
        if let Some(msg) = contract::conflicting_tenant(supplied.as_ref(), &tenant) {
            return (StatusCode::FORBIDDEN, Json(json!({ "error": msg })));
        }
        // The hosted backend takes a precomputed query vector (in-process
        // text→vector embedding downloads a model on first use and is never
        // exercised here). Reject text-only with a clear error, mirroring the
        // hosted MCP dispatch.
        let Some(vector) = req.vector else {
            return bad_request(
                "missing 'vector' — the hosted backend requires a precomputed query embedding",
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
            Err(e) => server_error(e),
        };
    }

    let prime = embedded(&state);
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
        Err(e) => server_error(e),
    }
}

async fn get_diff(State(state): State<Arc<AppState>>, scope: Scope) -> impl IntoResponse {
    // The embedded diff returns a stats summary (no from/to params). HostedPrime
    // exposes the same stats via `stats`, so the hosted path serves the identical
    // summary shape rather than 501.
    if let Some(hosted) = state.hosted.as_ref() {
        let Some(tenant) = scoped_tenant(scope) else {
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
            Err(e) => server_error(e),
        };
    }
    let stats = embedded(&state).stats();
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

#[cfg(test)]
#[path = "http_contract_tests.rs"]
mod contract_tests;
