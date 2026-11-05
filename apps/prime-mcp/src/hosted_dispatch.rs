//! Tenant-scoped MCP dispatch over a stateless [`HostedPrime`].
//!
//! This is a SEPARATE dispatch path from [`crate::dispatch`]: the embedded
//! stdio/HTTP path stays untouched. When the hosted `/mcp` handler has a
//! [`HostedPrime`] engine and a trusted tenant id, it routes `tools/call` here
//! so every operation is threaded through the tenant and executed against the
//! remote Core — rather than the local single-store embedded `Prime`.
//!
//! `tools/list` advertises only [`HOSTED_TOOLS`], the tools this dispatcher
//! serves; their definitions come from `tools::tool_definitions`, so argument
//! schemas match the embedded path. A call to any other tool is refused.

use allsource_core::prime::{Direction, hosted::HostedPrime};
use serde_json::{Value, json};

use crate::{
    contract,
    protocol::{self, Request, Response},
    tools,
    wire::{Fields, Page, node_row, paged},
};

/// Tools the hosted backend serves. Hound ingest runs where the source lives,
/// and index/context/similar/inbox need the embedded store.
pub const HOSTED_TOOLS: &[&str] = &[
    "prime_add_node",
    "prime_add_edge",
    "prime_update_node",
    "prime_neighbors",
    "prime_search",
    "prime_shortest_path",
    "prime_forget",
    "prime_history",
    "prime_stats",
    "prime_embed",
    "prime_recall",
    "prime_define_projection",
    "prime_list_projections",
    "prime_project_node",
    "prime_node_provenance",
    "prime_define_type",
    "prime_list_types",
    "prime_set_enforcement",
    "prime_list_templates",
    "prime_load_template",
    "hound_report",
    "hound_impact",
    "hound_pr_impact",
    "hound_export",
];

/// Time-axis arguments `HostedPrime` does not read yet; they are left out of
/// the hosted schemas so no caller expects them to apply.
const UNSERVED_TIME_ARGS: &[&str] = &["valid_from", "as_of", "as_of_valid"];

/// The `tools/list` result on the hosted backend.
pub fn hosted_tool_definitions() -> Value {
    let Value::Array(all) = tools::tool_definitions() else {
        return Value::Array(Vec::new());
    };
    Value::Array(
        all.into_iter()
            .filter(|t| {
                t["name"]
                    .as_str()
                    .is_some_and(|n| HOSTED_TOOLS.contains(&n))
            })
            .map(|mut t| {
                if let Some(Value::Object(props)) = t.pointer_mut("/inputSchema/properties") {
                    props.retain(|k, _| !UNSERVED_TIME_ARGS.contains(&k.as_str()));
                }
                t
            })
            .collect(),
    )
}

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
            let defs = hosted_tool_definitions();
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
/// `tenant`. A `tenant_id` argument that differs from `tenant` is refused.
async fn call_tool_hosted(hosted: &HostedPrime, tenant: &str, name: &str, args: &Value) -> Value {
    if let Some(msg) = contract::conflicting_tenant(args.get("tenant_id"), tenant) {
        return tool_error(&msg);
    }
    match name {
        "prime_add_node" => call_add_node(hosted, tenant, args).await,
        "prime_add_edge" => call_add_edge(hosted, tenant, args).await,
        "prime_update_node" => call_update_node(hosted, tenant, args).await,
        "prime_neighbors" => call_neighbors(hosted, tenant, args).await,
        "prime_search" => call_search(hosted, tenant, args).await,
        "prime_shortest_path" => call_shortest_path(hosted, tenant, args).await,
        "prime_stats" => call_stats(hosted, tenant).await,
        "prime_recall" => call_recall(hosted, tenant, args).await,
        "prime_embed" => call_embed(hosted, tenant, args).await,
        "prime_forget" => call_forget(hosted, tenant, args).await,
        "prime_history" => call_history(hosted, tenant, args).await,
        "prime_define_projection" => call_define_projection(hosted, tenant, args).await,
        "prime_list_projections" => call_list_projections(hosted, tenant).await,
        "prime_project_node" => call_project_node(hosted, tenant, args).await,
        "prime_node_provenance" => call_node_provenance(hosted, tenant, args).await,
        "prime_define_type" => call_define_type(hosted, tenant, args).await,
        "prime_list_types" => call_list_types(hosted, tenant).await,
        "prime_set_enforcement" => call_set_enforcement(hosted, tenant, args).await,
        "prime_list_templates" => tool_result(crate::templates::list_templates_summary()),
        "prime_load_template" => call_load_template(args),
        "hound_report" => call_hound_report_hosted(hosted, tenant, args).await,
        "hound_impact" => call_hound_impact_hosted(hosted, tenant, args).await,
        "hound_pr_impact" => call_hound_pr_impact_hosted(hosted, tenant, args).await,
        "hound_export" => call_hound_export_hosted(hosted, tenant, args).await,
        _ => tool_error(&format!(
            "tool {name} is not served by the hosted backend; tools/list names the tools it serves"
        )),
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
    let assertion = match contract::assertion_from_args(args, "source") {
        Ok(a) => a,
        Err(e) => return tool_error(&e),
    };

    match hosted
        .add_node_asserted(tenant, node_type, properties, assertion)
        .await
    {
        Ok(id) => {
            let entity_id = allsource_core::prime::EntityId::node(node_type, id.as_str()).to_wire();
            tool_result(json!({ "node_id": id.as_str(), "entity_id": entity_id }))
        }
        Err(e) => tool_error(&contract::hosted_error_text(&e)),
    }
}

async fn call_update_node(hosted: &HostedPrime, tenant: &str, args: &Value) -> Value {
    let Some(node_id) = args.get("node_id").and_then(Value::as_str) else {
        return tool_error("missing 'node_id'");
    };
    let Some(properties) = args.get("properties").filter(|p| p.is_object()).cloned() else {
        return tool_error("missing 'properties' (object)");
    };
    let assertion = match contract::assertion_from_args(args, "source") {
        Ok(a) => a,
        Err(e) => return tool_error(&e),
    };
    match hosted
        .update_node_asserted(tenant, node_id, properties, assertion)
        .await
    {
        Ok(()) => tool_result(json!({ "updated": true, "node_id": node_id })),
        Err(e) => tool_error(&contract::hosted_error_text(&e)),
    }
}

async fn call_shortest_path(hosted: &HostedPrime, tenant: &str, args: &Value) -> Value {
    let Some(from) = args.get("from").and_then(Value::as_str) else {
        return tool_error("missing 'from'");
    };
    let Some(to) = args.get("to").and_then(Value::as_str) else {
        return tool_error("missing 'to'");
    };
    let relation = args.get("relation").and_then(Value::as_str);
    match hosted.shortest_path(tenant, from, to, relation).await {
        Ok(Some(path)) => {
            let path_json: Vec<Value> = path.iter().map(|n| node_row(n, Fields::Full)).collect();
            tool_result(json!({ "path": path_json }))
        }
        Ok(None) => tool_result(json!({ "path": null, "message": "No path found" })),
        Err(e) => tool_error(&e.to_string()),
    }
}

fn call_load_template(args: &Value) -> Value {
    let Some(name) = args.get("name").and_then(Value::as_str) else {
        return tool_error("missing required argument: name");
    };
    match crate::templates::load_template(name) {
        Some(t) => tool_result(t),
        None => tool_error(&format!(
            "unknown template: {name} — call prime_list_templates to see what's available"
        )),
    }
}

async fn call_define_projection(hosted: &HostedPrime, tenant: &str, args: &Value) -> Value {
    use allsource_core::prime::projections::{MergePolicy, ProjectionDef};

    let Some(entity_type) = args.get("entity_type").and_then(Value::as_str) else {
        return tool_error("missing required argument: entity_type");
    };
    let Some(policies) = args.get("field_policies").and_then(Value::as_object) else {
        return tool_error("missing required argument: field_policies (object)");
    };
    let mut field_policies = std::collections::BTreeMap::new();
    for (field, value) in policies {
        let Ok(policy) = serde_json::from_value::<MergePolicy>(value.clone()) else {
            return tool_error(&format!(
                "field_policies.{field}: unknown policy {value} — valid: last_write, highest_priority, most_specific, merge_array"
            ));
        };
        field_policies.insert(field.clone(), policy);
    }
    let def = ProjectionDef {
        entity_type: entity_type.to_string(),
        field_policies,
    };
    let replaced = match hosted.projection_defs(tenant).await {
        Ok(defs) => defs.iter().any(|d| d.entity_type == entity_type),
        Err(e) => return tool_error(&e.to_string()),
    };
    match hosted.define_projection(tenant, &def).await {
        Ok(()) => tool_result(json!({
            "entity_type": entity_type,
            "replaced": replaced,
            "persisted": true,
        })),
        Err(e) => tool_error(&format!("failed to persist projection definition: {e}")),
    }
}

async fn call_list_projections(hosted: &HostedPrime, tenant: &str) -> Value {
    match hosted.projection_defs(tenant).await {
        Ok(mut defs) => {
            defs.sort_by(|a, b| a.entity_type.cmp(&b.entity_type));
            let payload: Vec<Value> = defs
                .iter()
                .map(
                    |d| json!({ "entity_type": d.entity_type, "field_policies": d.field_policies }),
                )
                .collect();
            tool_result(json!({ "projections": payload }))
        }
        Err(e) => tool_error(&e.to_string()),
    }
}

async fn projection_for(
    hosted: &HostedPrime,
    tenant: &str,
    entity_type: &str,
) -> Result<Option<allsource_core::prime::projections::ProjectionDef>, String> {
    hosted
        .projection_defs(tenant)
        .await
        .map(|defs| defs.into_iter().find(|d| d.entity_type == entity_type))
        .map_err(|e| e.to_string())
}

async fn call_project_node(hosted: &HostedPrime, tenant: &str, args: &Value) -> Value {
    use allsource_core::prime::projections::fold;

    let Some(node_id) = args.get("node_id").and_then(Value::as_str) else {
        return tool_error("missing required argument: node_id");
    };
    let Some(entity_type) = contract::entity_type_from_node_id(node_id) else {
        return tool_error(&format!(
            "node_id must be in 'node:{{type}}:{{id}}' format, got: {node_id}"
        ));
    };
    let def = match projection_for(hosted, tenant, entity_type).await {
        Ok(Some(d)) => d,
        Ok(None) => {
            return tool_error(&format!(
                "no projection defined for entity_type '{entity_type}' — register one with prime_define_projection first"
            ));
        }
        Err(e) => return tool_error(&e),
    };
    let observations = match hosted.observations(tenant, node_id).await {
        Ok(o) => o,
        Err(e) => return tool_error(&format!("failed to load observations: {e}")),
    };
    let snapshot = fold(&observations, &def);
    tool_result(json!({
        "entity_type": snapshot.entity_type,
        "fields": snapshot.fields,
        "observation_count": observations.len(),
    }))
}

async fn call_node_provenance(hosted: &HostedPrime, tenant: &str, args: &Value) -> Value {
    let Some(node_id) = args.get("node_id").and_then(Value::as_str) else {
        return tool_error("missing required argument: node_id");
    };
    let Some(field) = args.get("field").and_then(Value::as_str) else {
        return tool_error("missing required argument: field");
    };
    let Some(entity_type) = contract::entity_type_from_node_id(node_id) else {
        return tool_error(&format!(
            "node_id must be in 'node:{{type}}:{{id}}' format, got: {node_id}"
        ));
    };
    let def = match projection_for(hosted, tenant, entity_type).await {
        Ok(d) => d,
        Err(e) => return tool_error(&e),
    };
    let observations = match hosted.observations(tenant, node_id).await {
        Ok(o) => o,
        Err(e) => return tool_error(&format!("failed to load observations: {e}")),
    };
    match contract::provenance_json(node_id, field, &observations, def) {
        Ok(v) => tool_result(v),
        Err(e) => tool_error(&e),
    }
}

async fn call_define_type(hosted: &HostedPrime, tenant: &str, args: &Value) -> Value {
    let Some(type_name) = args.get("type_name").and_then(Value::as_str) else {
        return tool_error("missing 'type_name'");
    };
    let kind = match contract::parse_kind(args) {
        Ok(k) => k,
        Err(e) => return tool_error(&e),
    };
    let Some(schema) = args.get("schema").filter(|s| s.is_object()).cloned() else {
        return tool_error("missing 'schema' (object)");
    };
    match hosted.register_kind(tenant, kind, type_name, schema).await {
        Ok(entry) => tool_result(contract::kind_json(&entry)),
        Err(e) => tool_error(&contract::hosted_error_text(&e)),
    }
}

async fn call_list_types(hosted: &HostedPrime, tenant: &str) -> Value {
    let kinds = match hosted.kinds(tenant).await {
        Ok(k) => k,
        Err(e) => return tool_error(&e.to_string()),
    };
    match hosted.enforcement(tenant).await {
        Ok(mode) => tool_result(contract::kinds_json(&kinds, mode)),
        Err(e) => tool_error(&e.to_string()),
    }
}

async fn call_set_enforcement(hosted: &HostedPrime, tenant: &str, args: &Value) -> Value {
    let mode = match contract::parse_mode(args) {
        Ok(m) => m,
        Err(e) => return tool_error(&e),
    };
    match hosted.set_enforcement(tenant, mode).await {
        Ok(()) => tool_result(json!({ "mode": mode.as_str() })),
        Err(e) => tool_error(&contract::hosted_error_text(&e)),
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
    let assertion = match contract::assertion_from_args(args, contract::ASSERTION_SOURCE_KEY) {
        Ok(a) => a,
        Err(e) => return tool_error(&e),
    };

    match hosted
        .add_edge_asserted(
            tenant, source, target, relation, weight, properties, assertion,
        )
        .await
    {
        Ok(id) => tool_result(json!({ "edge_id": id.as_str() })),
        Err(e) => tool_error(&contract::hosted_error_text(&e)),
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
#[path = "hosted_dispatch_tests.rs"]
mod tests;
