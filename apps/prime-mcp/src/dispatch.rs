//! Transport-agnostic MCP JSON-RPC dispatch.
//!
//! Both transports — stdio (`transport.rs`) and Streamable HTTP (`http.rs`,
//! the `/mcp` endpoint) — hand a parsed [`Request`] to [`handle_request`] and
//! write back the returned [`Response`]. Keeping the method dispatch here means
//! the two transports can never drift in which methods/tools they expose.

use allsource_core::prime::{Prime, recall::RecallEngine};

use crate::{
    protocol::{self, Request, Response},
    tools,
};

/// Agent-facing cookbook: usage patterns for Prime tools.
pub const COOKBOOK: &str = "\
# Prime Cookbook

## Pattern: Store new knowledge
1. `prime_add_node` — create entity with domain tag
2. `prime_embed` — store embedding for semantic search
3. `prime_add_edge` — connect to existing nodes (cross-domain edges are most valuable)

## Pattern: Answer a cross-domain question
1. `prime_index` — get compressed knowledge map (shows domain connections)
2. `prime_context` — retrieve facts from relevant domains
3. Combine index cross-references with retrieved facts in your answer

## Pattern: Answer a single-domain question
1. `prime_recall` — semantic search with graph expansion
2. `prime_neighbors` — explore around the best match if needed

## Pattern: Track what changed
1. `prime_history` — full audit trail for any entity
2. `prime_stats` — overall graph state

## Pattern: Correct wrong knowledge
1. `prime_forget` — soft-delete the wrong fact (preserved in history)
2. `prime_add_node` — store the correct fact
3. `prime_add_edge` — reconnect relationships

## Anti-patterns
- DON'T embed without creating a node first (orphaned vectors can't be traversed)
- DON'T use prime_search for semantic queries (use prime_recall — it uses embeddings)
- DON'T skip the domain tag on nodes (disables cross-domain reasoning)
- DON'T forget without checking prime_history first (you might lose the only copy)
- DON'T fall back to prime_search when prime_recall reports `degraded` — it
  already degraded for you, and prime_search cannot rank by relevance at all

## Token budget
- prime_index: ~100-500 tokens (scales with knowledge base)
- prime_context: configurable via max_tokens (default: uncapped)
- prime_stats: ~50 tokens
- prime_neighbors: ~100 tokens per node returned
- prime_search: ~30 tokens per summary row, 50 rows max per call — read `total`
  and page with `offset` rather than raising `limit`
";

/// Dispatch one JSON-RPC request. Returns `None` for notifications (no reply),
/// `Some(Response)` otherwise. Pure with respect to transport — the caller owns
/// reading the request bytes and writing the response.
pub async fn handle_request(
    prime: &Prime,
    recall: &RecallEngine,
    auto_inject: bool,
    auto_inject_max_tokens: usize,
    req: &Request,
) -> Option<Response> {
    match req.method.as_str() {
        "initialize" => Some(Response::success(
            req.id.clone(),
            protocol::server_info(auto_inject),
        )),
        "notifications/initialized" => None,

        "tools/list" => {
            let defs = tools::tool_definitions();
            Some(Response::success(
                req.id.clone(),
                serde_json::json!({ "tools": defs }),
            ))
        }

        "tools/call" => {
            let params = req.params.as_ref();
            let tool_name = params
                .and_then(|p| p.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let args = params
                .and_then(|p| p.get("arguments"))
                .cloned()
                .unwrap_or(serde_json::json!({}));

            let result = tools::call_tool(prime, recall, tool_name, &args).await;
            Some(Response::success(req.id.clone(), result))
        }

        "resources/list" => {
            let mut resources = vec![serde_json::json!({
                "uri": "prime://cookbook",
                "name": "prime_cookbook",
                "description": "Usage patterns and best practices for Prime tools. Read this to learn the recommended workflows.",
                "mimeType": "text/markdown"
            })];

            if auto_inject {
                resources.push(serde_json::json!({
                    "uri": "prime://auto-context",
                    "name": "prime_auto_context",
                    "description": "Compressed knowledge index for system prompt injection. Updates automatically as memory grows.",
                    "mimeType": "text/markdown"
                }));
            }

            Some(Response::success(
                req.id.clone(),
                serde_json::json!({ "resources": resources }),
            ))
        }

        "resources/read" => {
            let uri = req
                .params
                .as_ref()
                .and_then(|p| p.get("uri"))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            match uri {
                "prime://cookbook" => Some(Response::success(
                    req.id.clone(),
                    serde_json::json!({
                        "contents": [{
                            "uri": "prime://cookbook",
                            "mimeType": "text/markdown",
                            "text": COOKBOOK
                        }]
                    }),
                )),

                "prime://auto-context" if auto_inject => {
                    let index = recall.index().await;
                    let markdown = if index.token_count > auto_inject_max_tokens {
                        let target_words = auto_inject_max_tokens * 10 / 13;
                        let truncated: String = index
                            .markdown
                            .split_whitespace()
                            .take(target_words)
                            .collect::<Vec<_>>()
                            .join(" ");
                        format!("{truncated}\n...(truncated to {auto_inject_max_tokens} tokens)")
                    } else {
                        index.markdown
                    };

                    Some(Response::success(
                        req.id.clone(),
                        serde_json::json!({
                            "contents": [{
                                "uri": "prime://auto-context",
                                "mimeType": "text/markdown",
                                "text": markdown
                            }]
                        }),
                    ))
                }

                _ => Some(Response::error(
                    req.id.clone(),
                    -32602,
                    format!("Unknown resource: {uri}"),
                )),
            }
        }

        _ => Some(Response::error(
            req.id.clone(),
            -32601,
            format!("Method not found: {}", req.method),
        )),
    }
}
