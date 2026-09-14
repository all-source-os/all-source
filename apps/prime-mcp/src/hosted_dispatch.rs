//! Tenant-scoped MCP dispatch over a stateless [`HostedPrime`].
//!
//! This is a SEPARATE dispatch path from [`crate::dispatch`]: the embedded
//! stdio/HTTP path stays untouched. When the hosted `/mcp` handler has a
//! [`HostedPrime`] engine and a trusted tenant id, it routes `tools/call` here
//! so every operation is threaded through the tenant and executed against the
//! remote Core — rather than the local single-store embedded `Prime`.
//!
//! Only the subset of tools [`HostedPrime`] implements is supported; the rest
//! return a clear "not yet available on the hosted backend" tool-error so a
//! caller never silently gets a wrong answer or a crash. `initialize` and
//! `tools/list` reuse the shared `protocol::server_info` + `tools::tool_definitions`
//! so the advertised surface matches the embedded path exactly.

use allsource_core::prime::{Direction, hosted::HostedPrime};
use serde_json::{Value, json};

use crate::{
    protocol::{self, Request, Response},
    tools,
    wire::{Fields, Page, node_row, paged},
};

/// Dispatch one JSON-RPC request against a tenant-scoped [`HostedPrime`].
///
/// Returns `None` for notifications (no reply), `Some(Response)` otherwise —
/// the same contract as [`crate::dispatch::handle_request`], so the HTTP
/// transport shapes both identically (200 JSON / 202 notification).
pub async fn handle_request_hosted(
    hosted: &HostedPrime,
    tenant: &str,
    req: &Request,
) -> Option<Response> {
    match req.method.as_str() {
        // Auto-inject is a stdio system-prompt feature; the hosted HTTP path
        // never enables it, matching the embedded `/mcp` handler's `false`.
        "initialize" => Some(Response::success(
            req.id.clone(),
            protocol::server_info(false),
        )),
        "notifications/initialized" => None,

        "tools/list" => {
            let defs = tools::tool_definitions();
            Some(Response::success(req.id.clone(), json!({ "tools": defs })))
        }

        "tools/call" => {
            let params = req.params.as_ref();
            let tool_name = params
                .and_then(|p| p.get("name"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let args = params
                .and_then(|p| p.get("arguments"))
                .cloned()
                .unwrap_or_else(|| json!({}));

            let result = call_tool_hosted(hosted, tenant, tool_name, &args).await;
            Some(Response::success(req.id.clone(), result))
        }

        _ => Some(Response::error(
            req.id.clone(),
            -32601,
            format!("Method not found: {}", req.method),
        )),
    }
}

/// Dispatch a single tool call to the matching [`HostedPrime`] method, threading
/// `tenant`. Tools the hosted engine does not yet implement return a tool-error
/// rather than panicking or silently no-op'ing.
async fn call_tool_hosted(hosted: &HostedPrime, tenant: &str, name: &str, args: &Value) -> Value {
    match name {
        "prime_add_node" => call_add_node(hosted, tenant, args).await,
        "prime_add_edge" => call_add_edge(hosted, tenant, args).await,
        "prime_neighbors" => call_neighbors(hosted, tenant, args).await,
        "prime_search" => call_search(hosted, tenant, args).await,
        "prime_stats" => call_stats(hosted, tenant).await,
        "prime_recall" => call_recall(hosted, tenant, args).await,
        "prime_embed" => call_embed(hosted, tenant, args).await,
        "prime_forget" => call_forget(hosted, tenant, args).await,
        "prime_history" => call_history(hosted, tenant, args).await,

        // Tools the embedded path serves but HostedPrime does not implement yet.
        // Return a clear, non-fatal error so an agent learns the capability is
        // unavailable on this backend rather than getting a wrong/empty answer.
        "prime_index"
        | "prime_context"
        | "prime_similar"
        | "prime_shortest_path"
        | "prime_list_templates"
        | "prime_load_template"
        | "prime_define_projection"
        | "prime_list_projections"
        | "prime_project_node"
        | "prime_node_provenance" => tool_error(&format!(
            "tool {name} not yet available on the hosted backend"
        )),

        // Hound INGEST is local-first (runs where your source lives); writes go
        // to the embedded store and sync to the tenant. The READ-side hound tools,
        // though, work over the SHARED hosted graph — that's the team-graph use:
        // one teammate ingests + syncs, everyone queries the same hosted graph.
        "hound_ingest" | "hound_ingest_docs" => tool_error(
            "Hound ingest runs in local mode where your source lives, not on the hosted \
             backend. Run prime-mcp locally and sync to your tenant; then query the shared \
             graph here with hound_report / hound_impact / hound_pr_impact / hound_export.",
        ),
        "hound_report" => call_hound_report_hosted(hosted, tenant, args).await,
        "hound_impact" => call_hound_impact_hosted(hosted, tenant, args).await,
        "hound_pr_impact" => call_hound_pr_impact_hosted(hosted, tenant, args).await,
        "hound_export" => call_hound_export_hosted(hosted, tenant, args).await,

        _ => tool_error(&format!("Unknown tool: {name}")),
    }
}

// ── Result envelope helpers ──────────────────────────────────────────────
//
// Minimal duplicates of `tools.rs`'s envelope helpers. We do NOT reuse
// `tools::tool_result` because that applies the global TOON/JSON format
// switch; the hosted HTTP transport always emits plain JSON. Keeping these
// local also keeps this dispatch fully independent of `tools::call_tool`.

#[allow(clippy::needless_pass_by_value)]
fn tool_result(content: Value) -> Value {
    let text = serde_json::to_string_pretty(&content).unwrap_or_default();
    json!({
        "content": [{ "type": "text", "text": text }]
    })
}

fn tool_error(msg: &str) -> Value {
    json!({
        "isError": true,
        "content": [{ "type": "text", "text": msg }]
    })
}

// ── Tool implementations (mirroring tools.rs arg parsing + result shaping) ──

async fn call_add_node(hosted: &HostedPrime, tenant: &str, args: &Value) -> Value {
    let node_type = args
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let properties = args.get("properties").cloned().unwrap_or_else(|| json!({}));

    match hosted.add_node(tenant, node_type, properties).await {
        Ok(id) => {
            let entity_id = allsource_core::prime::EntityId::node(node_type, id.as_str()).to_wire();
            tool_result(json!({ "node_id": id.as_str(), "entity_id": entity_id }))
        }
        Err(e) => tool_error(&e.to_string()),
    }
}

async fn call_add_edge(hosted: &HostedPrime, tenant: &str, args: &Value) -> Value {
    let Some(source) = args.get("source").and_then(Value::as_str) else {
        return tool_error("missing 'source'");
    };
    let Some(target) = args.get("target").and_then(Value::as_str) else {
        return tool_error("missing 'target'");
    };
    let Some(relation) = args.get("relation").and_then(Value::as_str) else {
        return tool_error("missing 'relation'");
    };
    let properties = args.get("properties").cloned();
    let weight = args.get("weight").and_then(Value::as_f64);

    let result = if let Some(w) = weight {
        hosted
            .add_edge_weighted(tenant, source, target, relation, w, properties)
            .await
    } else {
        hosted
            .add_edge(tenant, source, target, relation, properties)
            .await
    };

    match result {
        Ok(id) => tool_result(json!({ "edge_id": id.as_str() })),
        Err(e) => tool_error(&e.to_string()),
    }
}

async fn call_neighbors(hosted: &HostedPrime, tenant: &str, args: &Value) -> Value {
    let Some(node_id) = args.get("node_id").and_then(Value::as_str) else {
        return tool_error("missing 'node_id'");
    };
    let relation = args.get("relation").and_then(Value::as_str);
    let direction = match args.get("direction").and_then(Value::as_str) {
        Some("incoming") => Direction::Incoming,
        Some("outgoing") => Direction::Outgoing,
        _ => Direction::Both,
    };

    // HostedPrime serves single-hop neighbors; depth>1 is part of the embedded
    // graph traversal and is not available on the hosted backend yet.
    match hosted.neighbors(tenant, node_id, relation, direction).await {
        Ok(nodes) => {
            let fields = Fields::from_args(args, Fields::Full);
            let page = Page::from_args(args);
            let (nodes, total) = page.apply(nodes);
            let nodes_json: Vec<Value> = nodes.iter().map(|n| node_row(n, fields)).collect();
            tool_result(paged("nodes", nodes_json, page, total))
        }
        Err(e) => tool_error(&e.to_string()),
    }
}

async fn call_search(hosted: &HostedPrime, tenant: &str, args: &Value) -> Value {
    let Some(node_type) = args.get("type").and_then(Value::as_str) else {
        return tool_error("missing 'type'");
    };
    match hosted.search(tenant, node_type).await {
        Ok(nodes) => {
            let fields = Fields::from_args(args, Fields::Summary);
            let page = Page::from_args(args);
            let (nodes, total) = page.apply(nodes);
            let nodes_json: Vec<Value> = nodes.iter().map(|n| node_row(n, fields)).collect();
            tool_result(paged("nodes", nodes_json, page, total))
        }
        Err(e) => tool_error(&e.to_string()),
    }
}

async fn call_stats(hosted: &HostedPrime, tenant: &str) -> Value {
    match hosted.stats(tenant).await {
        Ok(stats) => tool_result(json!({
            "total_nodes": stats.total_nodes,
            "total_edges": stats.total_edges,
            "deleted_nodes": stats.deleted_nodes,
            "deleted_edges": stats.deleted_edges,
            "event_count": stats.event_count,
            "nodes_by_type": stats.nodes_by_type,
            "edges_by_relation": stats.edges_by_relation,
        })),
        Err(e) => tool_error(&e.to_string()),
    }
}

async fn call_recall(hosted: &HostedPrime, tenant: &str, args: &Value) -> Value {
    // The hosted recall takes a precomputed query vector. The in-process
    // text→vector embedder (`embed_text`) downloads a model on first use and is
    // never exercised on the hosted path here, so we require an explicit vector.
    let Some(vector) = args.get("vector").and_then(|v| v.as_array()) else {
        return tool_error(
            "missing 'vector' — the hosted backend requires a precomputed query embedding",
        );
    };
    let query_vector: Vec<f32> = vector
        .iter()
        .filter_map(|v| v.as_f64().map(|f| f as f32))
        .collect();
    let top_k = args
        .get("top_k")
        .and_then(Value::as_u64)
        .map_or(10, |v| v as usize);

    match hosted.recall(tenant, query_vector, top_k).await {
        Ok(results) => {
            let nodes_json: Vec<Value> = results
                .iter()
                .map(|(n, score)| {
                    let mut row = node_row(n, Fields::Full);
                    row["score"] = json!(score);
                    row
                })
                .collect();
            tool_result(json!({ "nodes": nodes_json }))
        }
        Err(e) => tool_error(&e.to_string()),
    }
}

async fn call_embed(hosted: &HostedPrime, tenant: &str, args: &Value) -> Value {
    let Some(id) = args.get("id").and_then(Value::as_str) else {
        return tool_error("missing 'id'");
    };
    // As with recall, the hosted path takes a precomputed vector to avoid the
    // first-use model download. Text-only embedding is a follow-up.
    let Some(arr) = args.get("vector").and_then(|v| v.as_array()) else {
        return tool_error(
            "missing 'vector' — the hosted backend requires a precomputed embedding vector",
        );
    };
    let vector: Vec<f32> = arr
        .iter()
        .filter_map(|v| v.as_f64().map(|f| f as f32))
        .collect();
    let metadata = args.get("metadata").cloned();

    match hosted.embed(tenant, id, vector, metadata).await {
        Ok(()) => tool_result(json!({ "stored": true, "id": id })),
        Err(e) => tool_error(&e.to_string()),
    }
}

async fn call_forget(hosted: &HostedPrime, tenant: &str, args: &Value) -> Value {
    let Some(node_id) = args.get("node_id").and_then(Value::as_str) else {
        return tool_error("missing 'node_id'");
    };
    match hosted.delete_node(tenant, node_id).await {
        Ok(()) => tool_result(json!({ "deleted": true })),
        Err(e) => tool_error(&e.to_string()),
    }
}

async fn call_history(hosted: &HostedPrime, tenant: &str, args: &Value) -> Value {
    let Some(entity_id) = args.get("entity_id").and_then(Value::as_str) else {
        return tool_error("missing 'entity_id'");
    };
    match hosted.history(tenant, entity_id).await {
        Ok(entries) => {
            let events_json: Vec<Value> = entries
                .iter()
                .map(|e| {
                    json!({
                        "type": e.event_type,
                        "timestamp": e.timestamp.to_rfc3339(),
                        "payload": e.payload,
                    })
                })
                .collect();
            tool_result(json!({ "events": events_json }))
        }
        Err(e) => tool_error(&e.to_string()),
    }
}

// ── Hound read-side tools over the shared hosted graph (team graphs) ────────
//
// All compute purely over the tenant's materialized `full_graph`, reusing the
// same `report`/`pr`/`export` functions the embedded path uses — so a team that
// syncs to one tenant queries the same graph here.

async fn call_hound_report_hosted(hosted: &HostedPrime, tenant: &str, args: &Value) -> Value {
    let top = args.get("top").and_then(Value::as_u64).unwrap_or(10) as usize;
    let markdown = args
        .get("markdown")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    match hosted.full_graph(tenant, None, None).await {
        Ok(g) => {
            let data = crate::report::compute(&g, top);
            if markdown {
                tool_result(json!({ "markdown": data.to_markdown() }))
            } else {
                tool_result(data.to_json())
            }
        }
        Err(e) => tool_error(&e.to_string()),
    }
}

async fn call_hound_impact_hosted(hosted: &HostedPrime, tenant: &str, args: &Value) -> Value {
    let Some(target) = args.get("target").and_then(Value::as_str) else {
        return tool_error("missing 'target' — a function name or a node:function:<id> wire id");
    };
    let depth = args.get("depth").and_then(Value::as_u64).unwrap_or(2) as usize;
    match hosted.full_graph(tenant, None, None).await {
        Ok(g) => tool_result(crate::pr::impact_of(&g, target, depth)),
        Err(e) => tool_error(&e.to_string()),
    }
}

async fn call_hound_pr_impact_hosted(hosted: &HostedPrime, tenant: &str, args: &Value) -> Value {
    let Some(files_val) = args.get("files").and_then(Value::as_array) else {
        return tool_error("missing 'files' — changed file paths (e.g. `git diff --name-only`)");
    };
    let files: Vec<String> = files_val
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();
    if files.is_empty() {
        return tool_error("'files' is empty — pass the PR's changed file paths");
    }
    let depth = args.get("depth").and_then(Value::as_u64).unwrap_or(2) as usize;
    let top = args.get("top").and_then(Value::as_u64).unwrap_or(20) as usize;
    match hosted.full_graph(tenant, None, None).await {
        Ok(g) => tool_result(crate::pr::pr_impact(&g, &files, depth).to_json(top)),
        Err(e) => tool_error(&e.to_string()),
    }
}

async fn call_hound_export_hosted(hosted: &HostedPrime, tenant: &str, args: &Value) -> Value {
    let Some(format) = args.get("format").and_then(Value::as_str) else {
        return tool_error(&format!(
            "missing 'format' — one of: {}",
            crate::export::format_keys()
        ));
    };
    let mermaid_max = args
        .get("mermaid_max")
        .and_then(Value::as_u64)
        .unwrap_or(60) as usize;
    match hosted.full_graph(tenant, None, None).await {
        Ok(g) => {
            if let Some(text) = crate::export::render_text(&g, format, mermaid_max) {
                tool_result(json!({ "format": format, "content": text }))
            } else if let Some(files) = crate::export::render_files(&g, format) {
                let files_json: Vec<Value> = files
                    .into_iter()
                    .map(|(path, content)| json!({ "path": path, "content": content }))
                    .collect();
                tool_result(json!({ "format": format, "files": files_json }))
            } else {
                tool_error(&format!(
                    "unknown format '{format}' — choose one of: {}",
                    crate::export::format_keys()
                ))
            }
        }
        Err(e) => tool_error(&e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use wiremock::MockServer;

    // A stateful fake Core (POSTed events are returned on query), so the
    // write-then-read-back round-trip works under the cold-write semantics from
    // t-d90426. See `crate::hosted_test_core`.
    async fn mount_empty_core(server: &MockServer) {
        crate::hosted_test_core::mount_stateful_core(server).await;
    }

    #[allow(clippy::needless_pass_by_value)] // test helper; clarity over &Value
    fn req(id: i64, method: &str, params: Value) -> Request {
        // Build a Request via JSON so we don't depend on field visibility.
        serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }))
        .unwrap()
    }

    #[tokio::test]
    async fn initialize_returns_server_info() {
        let server = MockServer::start().await;
        let hosted = HostedPrime::connect(server.uri(), None, 8, Duration::from_secs(60));
        let r = req(1, "initialize", json!({}));
        let resp = handle_request_hosted(&hosted, "tenant-a", &r)
            .await
            .unwrap();
        let v = serde_json::to_value(&resp).unwrap();
        assert_eq!(v["result"]["serverInfo"]["name"], "allsource-prime");
    }

    #[tokio::test]
    async fn notifications_initialized_has_no_reply() {
        let server = MockServer::start().await;
        let hosted = HostedPrime::connect(server.uri(), None, 8, Duration::from_secs(60));
        let r = req(0, "notifications/initialized", json!({}));
        assert!(
            handle_request_hosted(&hosted, "tenant-a", &r)
                .await
                .is_none()
        );
    }

    #[tokio::test]
    async fn add_node_then_search_round_trips_with_tenant() {
        let server = MockServer::start().await;
        mount_empty_core(&server).await;
        let hosted = HostedPrime::connect(server.uri(), None, 8, Duration::from_secs(60));

        let add = req(
            1,
            "tools/call",
            json!({ "name": "prime_add_node", "arguments": { "type": "contact", "properties": { "name": "Alice" } } }),
        );
        let resp = handle_request_hosted(&hosted, "tenant-a", &add)
            .await
            .unwrap();
        let v = serde_json::to_value(&resp).unwrap();
        assert_ne!(
            v["result"]["isError"],
            json!(true),
            "add_node should succeed"
        );

        let search = req(
            2,
            "tools/call",
            json!({ "name": "prime_search", "arguments": { "type": "contact" } }),
        );
        let resp = handle_request_hosted(&hosted, "tenant-a", &search)
            .await
            .unwrap();
        let v = serde_json::to_value(&resp).unwrap();
        let text = v["result"]["content"][0]["text"].as_str().unwrap();
        assert!(
            text.contains("Alice"),
            "search should return the added node; got {text}"
        );
    }

    #[tokio::test]
    async fn unsupported_tool_errors_without_crashing() {
        let server = MockServer::start().await;
        let hosted = HostedPrime::connect(server.uri(), None, 8, Duration::from_secs(60));
        let r = req(
            1,
            "tools/call",
            json!({ "name": "prime_index", "arguments": {} }),
        );
        let resp = handle_request_hosted(&hosted, "tenant-a", &r)
            .await
            .unwrap();
        let v = serde_json::to_value(&resp).unwrap();
        assert_eq!(v["result"]["isError"], json!(true));
        let text = v["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("not yet available on the hosted backend"));
    }

    /// Team graphs: one tenant's shared hosted graph answers the read-side hound
    /// tools (`report` / `impact` / `pr_impact`) over the SAME `report`/`pr` functions
    /// the embedded path uses.
    #[tokio::test]
    async fn hound_read_tools_over_shared_hosted_graph() {
        let server = MockServer::start().await;
        mount_empty_core(&server).await; // stateful fake Core
        let hosted = HostedPrime::connect(server.uri(), None, 8, Duration::from_secs(60));
        let t = "team-a";
        let _ = hosted.stats(t).await; // warm the tenant so writes apply to the cache

        // Seed: a.rs defines alpha + beta; alpha calls beta.
        let _f = hosted
            .add_node(t, "file", json!({ "path": "a.rs", "domain": "code" }))
            .await
            .unwrap();
        let a = hosted
            .add_node(
                t,
                "function",
                json!({ "name": "alpha", "file": "a.rs", "domain": "code" }),
            )
            .await
            .unwrap();
        let b = hosted
            .add_node(
                t,
                "function",
                json!({ "name": "beta", "file": "a.rs", "domain": "code" }),
            )
            .await
            .unwrap();
        hosted
            .add_edge_weighted(
                t,
                &format!("node:function:{}", a.as_str()),
                &format!("node:function:{}", b.as_str()),
                "calls",
                0.6,
                None,
            )
            .await
            .unwrap();

        // hound_report over the shared graph: 3 nodes.
        let rep = call_hound_report_hosted(&hosted, t, &json!({})).await;
        let text = rep["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("\"node_count\": 3"), "report: {text}");

        // hound_impact of beta: alpha transitively calls it.
        let imp = call_hound_impact_hosted(&hosted, t, &json!({ "target": "beta" })).await;
        let text = imp["content"][0]["text"].as_str().unwrap();
        assert!(
            text.contains("alpha"),
            "impact should surface caller alpha: {text}"
        );

        // hound_pr_impact for a.rs returns a review queue.
        let pri = call_hound_pr_impact_hosted(&hosted, t, &json!({ "files": ["a.rs"] })).await;
        let text = pri["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("review_queue"), "pr_impact: {text}");
    }
}
