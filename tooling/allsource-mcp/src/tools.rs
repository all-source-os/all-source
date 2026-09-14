//! MCP tool definitions and execution.

use std::fmt::Write;

use allsource_core::embedded::{EmbeddedCore, Query};
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{
    diagnostics::DiagnosticPolicy,
    protocol::{ToolAnnotations, ToolDef, tool_error, tool_result},
};

const DEFAULT_LIMIT: usize = 50;
const MAX_LIMIT: usize = 500;

/// Build a read-only tool descriptor with shared diagnostic input and annotations.
fn read_tool(name: &str, title: &str, description: &str, mut input_schema: Value) -> ToolDef {
    input_schema["properties"]["diagnostic"] = json!({
        "type": "object",
        "description": "Optional correlation identifiers carried into diagnostic context; never used as tenant authorization.",
        "properties": {
            "requestId": { "type": "string" },
            "traceId": { "type": "string" },
            "runId": { "type": "string" },
            "workflowRunId": { "type": "string" },
            "entityId": { "type": "string" },
            "conversationId": { "type": "string" }
        },
        "additionalProperties": false
    });
    ToolDef {
        name: name.to_string(),
        title: title.to_string(),
        description: description.to_string(),
        input_schema,
        output_schema: json!({
            "type": "object",
            "properties": { "context": { "type": "object" } },
            "required": ["context"],
            "additionalProperties": true
        }),
        annotations: ToolAnnotations {
            read_only_hint: true,
            destructive_hint: false,
            idempotent_hint: true,
            open_world_hint: false,
        },
    }
}

/// Return all available tool definitions.
#[allow(clippy::too_many_lines)] // Keeping deterministic descriptor order visible aids MCP review.
pub fn tool_definitions(policy: &DiagnosticPolicy) -> Vec<ToolDef> {
    let payload_modes = if policy.is_hosted_tenant() {
        json!(["none", "keys", "redacted"])
    } else {
        json!(["none", "keys", "redacted", "full"])
    };
    vec![
        read_tool(
            "query_events",
            "Query events",
            "Read a tenant-bound, paginated event window with explicit completeness.",
            json!({
                "type": "object",
                "properties": {
                    "entity_id": { "type": "string", "description": "Filter by entity ID (exact match)" },
                    "event_type": { "type": "string", "description": "Filter by event type prefix (e.g. 'workflow_run' matches 'workflow_run.started')" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": MAX_LIMIT, "default": DEFAULT_LIMIT },
                    "cursor": { "type": "string", "description": "Opaque cursor returned by a previous identical query" },
                    "order": { "type": "string", "enum": ["asc", "desc"], "default": "asc" },
                    "payload_mode": { "type": "string", "enum": payload_modes.clone(), "description": "Hosted mode excludes full payloads; local and operator modes may request them" },
                    "since": { "type": "string", "format": "date-time" },
                    "until": { "type": "string", "format": "date-time" }
                }
            }),
        ),
        read_tool(
            "sample_events",
            "Sample recent events",
            "Discover recent events inside this server's verified tenant boundary.",
            json!({
                "type": "object",
                "properties": {
                    "count": { "type": "integer", "minimum": 1, "maximum": 100, "default": 20 },
                    "cursor": { "type": "string", "description": "Opaque cursor returned by a previous identical sample" },
                    "payload_mode": { "type": "string", "enum": payload_modes.clone() }
                }
            }),
        ),
        read_tool(
            "quick_stats",
            "Inspect event store",
            "Report exact scoped counters, freshness, and durability without hidden sampling.",
            json!({
                "type": "object",
                "properties": {}
            }),
        ),
        read_tool(
            "get_snapshot",
            "Get projection state",
            "Read one named authoritative projection. Never guesses or falls back to payload merging.",
            json!({
                "type": "object",
                "properties": {
                    "entity_id": { "type": "string" },
                    "projection_name": { "type": "string" }
                },
                "required": ["entity_id", "projection_name"]
            }),
        ),
        read_tool(
            "event_timeline",
            "Trace entity timeline",
            "Read a stable, paginated lifecycle timeline for one entity.",
            json!({
                "type": "object",
                "properties": {
                    "entity_id": { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": MAX_LIMIT, "default": 100 },
                    "cursor": { "type": "string" },
                    "since": { "type": "string", "format": "date-time" },
                    "until": { "type": "string", "format": "date-time" }
                },
                "required": ["entity_id"]
            }),
        ),
        read_tool(
            "explain_entity",
            "Explain entity history",
            "Summarize a bounded entity lifecycle and disclose incomplete history.",
            json!({
                "type": "object",
                "properties": {
                    "entity_id": { "type": "string", "description": "The entity ID to explain" }
                },
                "required": ["entity_id"]
            }),
        ),
        read_tool(
            "reconstruct_state",
            "Preview payload fold (deprecated)",
            "Deprecated heuristic payload fold. Result is never authoritative; prefer get_snapshot.",
            json!({
                "type": "object",
                "properties": {
                    "entity_id": { "type": "string", "description": "The entity ID to reconstruct" }
                },
                "required": ["entity_id"]
            }),
        ),
        read_tool(
            "analyze_changes",
            "Analyze entity changes",
            "Read bounded event changes for one entity with explicit completeness.",
            json!({
                "type": "object",
                "properties": {
                    "entity_id": { "type": "string", "description": "The entity ID to analyze" },
                    "since": { "type": "string", "format": "date-time" },
                    "until": { "type": "string", "format": "date-time" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": MAX_LIMIT, "default": 100 },
                    "cursor": { "type": "string" },
                    "payload_mode": { "type": "string", "enum": payload_modes }
                },
                "required": ["entity_id"]
            }),
        ),
    ]
}

/// Execute a tool call and return the MCP result.
pub async fn execute_tool(
    core: &EmbeddedCore,
    policy: &DiagnosticPolicy,
    name: &str,
    args: &Value,
) -> Value {
    match execute_tool_inner(core, policy, name, args).await {
        Ok(mut result) => {
            DiagnosticPolicy::attach_correlation(&mut result, args);
            tool_result(&result)
        }
        Err(error) => {
            let detail = error.to_string();
            let mut result = if detail.starts_with("invalid argument:") {
                tool_error("INVALID_ARGUMENT", &detail, false, "NARROW_QUERY")
            } else if detail.starts_with("not found:") {
                tool_error("NOT_FOUND", &detail, false, "SELECT_SOURCE")
            } else if detail.starts_with("access denied:") {
                tool_error("ACCESS_DENIED", &detail, false, "CONTACT_OPERATOR")
            } else {
                tracing::error!(tool = name, error = %detail, "AllSource MCP tool failed");
                tool_error(
                    "SOURCE_UNAVAILABLE",
                    "AllSource query failed. Check source health, then retry.",
                    true,
                    "RETRY_AFTER",
                )
            };
            result["structuredContent"]["context"] = policy.context(None);
            DiagnosticPolicy::attach_correlation(&mut result["structuredContent"], args);
            result
        }
    }
}

/// Dispatch a validated tool call to its implementation.
async fn execute_tool_inner(
    core: &EmbeddedCore,
    policy: &DiagnosticPolicy,
    name: &str,
    args: &Value,
) -> Result<Value> {
    match name {
        "query_events" => exec_query_events(core, policy, args).await,
        "sample_events" => exec_sample_events(core, policy, args).await,
        "quick_stats" => exec_quick_stats(core, policy).await,
        "get_snapshot" => exec_get_snapshot(core, policy, args),
        "event_timeline" => exec_event_timeline(core, policy, args).await,
        "explain_entity" => exec_explain_entity(core, policy, args).await,
        "reconstruct_state" => exec_reconstruct_state(core, policy, args).await,
        "analyze_changes" => exec_analyze_changes(core, policy, args).await,
        _ => anyhow::bail!("invalid argument: unknown tool '{name}'"),
    }
}

#[derive(Clone, Copy, Debug)]
enum PayloadMode {
    None,
    Keys,
    Redacted,
    Full,
}

/// Resolve payload exposure mode while enforcing hosted redaction policy.
fn payload_mode(args: &Value, policy: &DiagnosticPolicy) -> Result<PayloadMode> {
    let default = if policy.is_hosted_tenant() {
        "redacted"
    } else {
        "full"
    };
    match args
        .get("payload_mode")
        .and_then(Value::as_str)
        .unwrap_or(default)
    {
        "none" => Ok(PayloadMode::None),
        "keys" => Ok(PayloadMode::Keys),
        "redacted" => Ok(PayloadMode::Redacted),
        "full" if policy.is_hosted_tenant() => anyhow::bail!(
            "access denied: payload_mode full is unavailable for hosted tenant profiles"
        ),
        "full" => Ok(PayloadMode::Full),
        value => anyhow::bail!(
            "invalid argument: payload_mode must be none, keys, redacted, or full; got '{value}'"
        ),
    }
}

/// Read and bound a positive pagination argument.
fn limit_arg(args: &Value, key: &str, default: usize, maximum: usize) -> Result<usize> {
    let raw = args
        .get(key)
        .and_then(Value::as_u64)
        .unwrap_or(default as u64);
    let limit = usize::try_from(raw)
        .map_err(|_| anyhow::anyhow!("invalid argument: {key} exceeds platform range"))?;
    if !(1..=maximum).contains(&limit) {
        anyhow::bail!("invalid argument: {key} must be between 1 and {maximum}");
    }
    Ok(limit)
}

/// Parse an optional RFC 3339 timestamp argument.
fn timestamp_arg(args: &Value, key: &str) -> Result<Option<DateTime<Utc>>> {
    args.get(key)
        .and_then(Value::as_str)
        .map(|raw| {
            raw.parse::<DateTime<Utc>>().map_err(|_| {
                anyhow::anyhow!("invalid argument: {key} must be an RFC 3339 timestamp")
            })
        })
        .transpose()
}

/// Hash tenant, source, tool, effective limit, and filters into cursor identity.
fn query_signature(
    policy: &DiagnosticPolicy,
    tool_name: &str,
    effective_limit: usize,
    args: &Value,
) -> String {
    let mut hasher = Sha256::new();
    for value in [
        tool_name,
        policy.tenant_id().unwrap_or("*"),
        policy.source_id(),
        args.get("entity_id").and_then(Value::as_str).unwrap_or(""),
        args.get("event_type").and_then(Value::as_str).unwrap_or(""),
        args.get("since").and_then(Value::as_str).unwrap_or(""),
        args.get("until").and_then(Value::as_str).unwrap_or(""),
        args.get("order").and_then(Value::as_str).unwrap_or("asc"),
        args.get("payload_mode")
            .and_then(Value::as_str)
            .unwrap_or(""),
    ] {
        hasher.update(value.as_bytes());
        hasher.update([0]);
    }
    hasher.update(effective_limit.to_le_bytes());
    format!("{:x}", hasher.finalize())
}

/// Validate an opaque cursor against current query identity and return its offset.
fn cursor_offset(args: &Value, signature: &str) -> Result<usize> {
    let Some(cursor) = args.get("cursor").and_then(Value::as_str) else {
        return Ok(0);
    };
    let mut parts = cursor.split(':');
    let valid_version = parts.next() == Some("v1");
    let offset = parts.next().and_then(|value| value.parse::<usize>().ok());
    let cursor_signature = parts.next();
    if !valid_version
        || parts.next().is_some()
        || cursor_signature != Some(signature)
        || offset.is_none()
    {
        anyhow::bail!("invalid argument: cursor does not match this tenant-bound query");
    }
    Ok(offset.unwrap_or_default())
}

/// Build a tenant-bound query and matching cursor signature.
fn scoped_query(
    policy: &DiagnosticPolicy,
    tool_name: &str,
    args: &Value,
    limit: usize,
) -> Result<(Query, String)> {
    let since = timestamp_arg(args, "since")?;
    let until = timestamp_arg(args, "until")?;
    if since.zip(until).is_some_and(|(start, end)| start > end) {
        anyhow::bail!("invalid argument: since must not be after until");
    }

    let signature = query_signature(policy, tool_name, limit, args);
    let offset = cursor_offset(args, &signature)?;
    let descending = match args.get("order").and_then(Value::as_str).unwrap_or("asc") {
        "asc" => false,
        "desc" => true,
        value => anyhow::bail!("invalid argument: order must be asc or desc; got '{value}'"),
    };

    let mut query = Query::new()
        .limit(limit)
        .offset(offset)
        .descending(descending);
    if let Some(tenant_id) = policy.tenant_id() {
        query = query.tenant_id(tenant_id);
    }

    if let Some(entity_id) = args.get("entity_id").and_then(|v| v.as_str()) {
        query = query.entity_id(entity_id);
    }
    if let Some(event_type) = args.get("event_type").and_then(|v| v.as_str()) {
        query = query.event_type_prefix(event_type);
    }
    if let Some(since) = since {
        query = query.since(since);
    }
    if let Some(until) = until {
        query = query.until(until);
    }

    Ok((query, signature))
}

/// Recursively redact values under credential-like object keys.
fn redact(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| {
                    let normalized = key.to_ascii_lowercase();
                    let sensitive = [
                        "authorization",
                        "cookie",
                        "password",
                        "private_key",
                        "secret",
                        "token",
                        "api_key",
                    ]
                    .iter()
                    .any(|needle| normalized.contains(needle));
                    (
                        key.clone(),
                        if sensitive {
                            Value::String("[REDACTED]".to_string())
                        } else {
                            redact(value)
                        },
                    )
                })
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.iter().map(redact).collect()),
        scalar => scalar.clone(),
    }
}

/// Render an event payload according to selected exposure mode.
fn event_payload(payload: &Value, mode: PayloadMode) -> Value {
    match mode {
        PayloadMode::None => Value::Null,
        PayloadMode::Keys => json!(
            payload
                .as_object()
                .map(|object| object.keys().cloned().collect::<Vec<_>>())
                .unwrap_or_default()
        ),
        PayloadMode::Redacted => redact(payload),
        PayloadMode::Full => payload.clone(),
    }
}

#[derive(Clone, Copy)]
struct EvidencePageOptions<'a> {
    requested_limit: usize,
    signature: &'a str,
    items: &'a Value,
    incomplete_reason: &'a str,
    force_incomplete: bool,
}

/// Wrap a query page with provenance, pagination, and completeness metadata.
fn evidence_page(
    policy: &DiagnosticPolicy,
    page: &allsource_core::embedded::QueryPage,
    options: EvidencePageOptions<'_>,
) -> Value {
    let fresh_through = page.events.iter().map(|event| event.timestamp).max();
    let next_cursor = page
        .next_offset
        .map(|offset| format!("v1:{offset}:{}", options.signature));
    let complete = !options.force_incomplete && next_cursor.is_none();
    let reason = if options.force_incomplete {
        Some(options.incomplete_reason)
    } else {
        (!complete).then_some(options.incomplete_reason)
    };
    let consumed = page.next_offset.unwrap_or(page.total_count);
    json!({
        "context": policy.context(fresh_through.as_ref().map(DateTime::to_rfc3339).as_deref()),
        "items": options.items,
        "page": {
            "requestedLimit": options.requested_limit,
            "returned": page.events.len(),
            "totalCount": page.total_count,
            "nextCursor": next_cursor,
        },
        "completeness": {
            "complete": complete,
            "reason": reason,
            "scanned": page.total_count,
            "matched": page.total_count,
            "omitted": page.total_count.saturating_sub(consumed),
            "unparsedTimestamps": 0,
            "sourcesRequested": 1,
            "sourcesRead": 1,
            "sourcesSkipped": [],
        }
    })
}

/// Execute a filtered tenant-bound event query.
async fn exec_query_events(
    core: &EmbeddedCore,
    policy: &DiagnosticPolicy,
    args: &Value,
) -> Result<Value> {
    let limit = limit_arg(args, "limit", DEFAULT_LIMIT, MAX_LIMIT)?;
    let mode = payload_mode(args, policy)?;
    let (query, signature) = scoped_query(policy, "query_events", args, limit)?;

    let page = core.query_page(query).await?;
    let result: Vec<Value> = page
        .events
        .iter()
        .map(|e| {
            json!({
                "id": e.id.to_string(),
                "entity_id": e.entity_id,
                "event_type": e.event_type,
                "tenant_id": e.tenant_id,
                "timestamp": e.timestamp.to_rfc3339(),
                "version": e.version,
                "payload": event_payload(&e.payload, mode),
                "metadata": e.metadata.as_ref().map(redact),
            })
        })
        .collect();

    let items = Value::Array(result);
    Ok(evidence_page(
        policy,
        &page,
        EvidencePageOptions {
            requested_limit: limit,
            signature: &signature,
            items: &items,
            incomplete_reason: "limit_reached",
            force_incomplete: false,
        },
    ))
}

/// Execute a bounded newest-first event sample.
async fn exec_sample_events(
    core: &EmbeddedCore,
    policy: &DiagnosticPolicy,
    args: &Value,
) -> Result<Value> {
    if policy.is_hosted_tenant() && policy.tenant_id().is_none() {
        anyhow::bail!("access denied: hosted sampling requires a verified tenant binding");
    }
    let count = limit_arg(args, "count", 20, 100)?;
    let mode = payload_mode(args, policy)?;
    let mut scoped_args = args.clone();
    scoped_args["order"] = Value::String("desc".to_string());
    let (query, signature) = scoped_query(policy, "sample_events", &scoped_args, count)?;
    let page = core.query_page(query).await?;
    let result: Vec<Value> = page
        .events
        .iter()
        .map(|e| {
            json!({
                "entity_id": e.entity_id,
                "event_type": e.event_type,
                "tenant_id": e.tenant_id,
                "timestamp": e.timestamp.to_rfc3339(),
                "payload": event_payload(&e.payload, mode),
            })
        })
        .collect();

    let items = Value::Array(result);
    Ok(evidence_page(
        policy,
        &page,
        EvidencePageOptions {
            requested_limit: count,
            signature: &signature,
            items: &items,
            incomplete_reason: "sampled",
            force_incomplete: true,
        },
    ))
}

/// Return exact scoped counts, freshness, and durability.
async fn exec_quick_stats(core: &EmbeddedCore, policy: &DiagnosticPolicy) -> Result<Value> {
    let durability = core.durability_status();
    let (statistics, fresh_through) = if let Some(tenant_id) = policy.tenant_id() {
        let stats = core.stats_for_tenant(tenant_id);
        let fresh_through = stats.newest_event.map(|timestamp| timestamp.to_rfc3339());
        (serde_json::to_value(stats)?, fresh_through)
    } else {
        let stats = core.stats();
        let newest = core
            .query_page(Query::new().limit(1).descending(true))
            .await?
            .events
            .first()
            .map(|event| event.timestamp.to_rfc3339());
        (serde_json::to_value(stats)?, newest)
    };

    Ok(json!({
        "context": policy.context(fresh_through.as_deref()),
        "statistics": statistics,
        "completeness": {
            "complete": true,
            "reason": null,
            "sampled": false,
        },
        "durability": {
            "memory_events": durability.memory_events,
            "wal_enabled": durability.wal_enabled,
            "wal_entries": durability.wal_entries,
            "parquet_enabled": durability.parquet_enabled,
            "parquet_files": durability.parquet_files,
            "durable": durability.durable,
            "warnings": durability.warnings,
        },
    }))
}

/// Read one named authoritative projection state when policy permits it.
fn exec_get_snapshot(
    core: &EmbeddedCore,
    policy: &DiagnosticPolicy,
    args: &Value,
) -> Result<Value> {
    let entity_id = args
        .get("entity_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("invalid argument: entity_id is required"))?;
    let projection_name = args
        .get("projection_name")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("invalid argument: projection_name is required"))?;

    if policy.is_hosted_tenant() {
        anyhow::bail!(
            "access denied: tenant-scoped projection reads are unavailable; query tenant-bound events instead"
        );
    }

    let Some(state) = core.projection(projection_name, entity_id) else {
        anyhow::bail!(
            "not found: projection '{projection_name}' has no state for entity '{entity_id}'"
        );
    };

    Ok(json!({
        "context": policy.context(None),
        "entityId": entity_id,
        "projectionName": projection_name,
        "authoritative": true,
        "completeness": {
            "complete": true,
            "reason": null,
        },
        "state": state,
    }))
}

/// Return a stable paginated timeline for one entity.
async fn exec_event_timeline(
    core: &EmbeddedCore,
    policy: &DiagnosticPolicy,
    args: &Value,
) -> Result<Value> {
    let entity_id = args
        .get("entity_id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("invalid argument: entity_id is required"))?;
    let limit = limit_arg(args, "limit", 100, MAX_LIMIT)?;
    let (query, signature) = scoped_query(policy, "event_timeline", args, limit)?;
    let page = core.query_page(query.entity_id(entity_id)).await?;

    let timeline: Vec<Value> = page
        .events
        .iter()
        .map(|event| {
            json!({
                "id": event.id,
                "timestamp": event.timestamp.to_rfc3339(),
                "event_type": event.event_type,
                "version": event.version,
                "summary": summarize_payload(&event.payload),
            })
        })
        .collect();

    let items = Value::Array(timeline);
    Ok(evidence_page(
        policy,
        &page,
        EvidencePageOptions {
            requested_limit: limit,
            signature: &signature,
            items: &items,
            incomplete_reason: "limit_reached",
            force_incomplete: false,
        },
    ))
}

/// Summarize a bounded entity lifecycle without claiming omitted history.
async fn exec_explain_entity(
    core: &EmbeddedCore,
    policy: &DiagnosticPolicy,
    args: &Value,
) -> Result<Value> {
    let entity_id = args
        .get("entity_id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("invalid argument: entity_id is required"))?;
    let limit = MAX_LIMIT;
    let (query, _) = scoped_query(policy, "explain_entity", args, limit)?;
    let page = core.query_page(query.entity_id(entity_id)).await?;

    if page.events.is_empty() {
        return Ok(json!({
            "context": policy.context(None),
            "entityId": entity_id,
            "explanation": "No events found for this entity inside the current tenant boundary.",
            "completeness": {
                "complete": true,
                "reason": null,
            }
        }));
    }

    let first = &page.events[0];
    let last = &page.events[page.events.len() - 1];
    let mut type_counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    let mut phases: Vec<String> = Vec::new();
    let mut previous_type = "";
    for event in &page.events {
        *type_counts.entry(&event.event_type).or_default() += 1;
        if event.event_type != previous_type {
            phases.push(format!(
                "{} ({})",
                event.event_type,
                event.timestamp.format("%Y-%m-%d %H:%M:%S")
            ));
            previous_type = &event.event_type;
        }
    }

    Ok(json!({
        "context": policy.context(Some(&last.timestamp.to_rfc3339())),
        "entityId": entity_id,
        "eventsReturned": page.events.len(),
        "totalEvents": page.total_count,
        "created": first.timestamp.to_rfc3339(),
        "lastActivity": last.timestamp.to_rfc3339(),
        "eventTypes": type_counts,
        "lifecyclePhases": phases,
        "completeness": {
            "complete": !page.has_more,
            "reason": page.has_more.then_some("limit_reached"),
            "omitted": page.total_count.saturating_sub(page.events.len()),
        }
    }))
}

/// Produce a deprecated non-authoritative last-write-wins payload fold.
async fn exec_reconstruct_state(
    core: &EmbeddedCore,
    policy: &DiagnosticPolicy,
    args: &Value,
) -> Result<Value> {
    let entity_id = args
        .get("entity_id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("invalid argument: entity_id is required"))?;

    let (query, _) = scoped_query(policy, "reconstruct_state", args, MAX_LIMIT)?;
    let page = core.query_page(query.entity_id(entity_id)).await?;

    if page.events.is_empty() {
        return Ok(json!({
            "context": policy.context(None),
            "entityId": entity_id,
            "authoritative": false,
            "method": "heuristic_last_write_wins",
            "state": null,
            "warning": "Deprecated heuristic. No events found inside the current tenant boundary.",
            "completeness": {
                "complete": true,
                "reason": null,
                "omitted": 0,
            },
        }));
    }

    let mut state = serde_json::Map::new();
    for e in &page.events {
        state.insert(
            "_last_event_type".to_string(),
            Value::String(e.event_type.clone()),
        );
        state.insert(
            "_last_updated".to_string(),
            Value::String(e.timestamp.to_rfc3339()),
        );
        state.insert("_version".to_string(), json!(e.version));

        if let Some(obj) = e.payload.as_object() {
            for (k, v) in obj {
                state.insert(k.clone(), v.clone());
            }
        }
    }

    let fresh_through = page.events.last().map(|event| event.timestamp.to_rfc3339());
    Ok(json!({
        "context": policy.context(fresh_through.as_deref()),
        "entityId": entity_id,
        "authoritative": false,
        "method": "heuristic_last_write_wins",
        "deprecated": true,
        "warning": "This payload fold is not domain state. Use a named registered projection for authoritative state.",
        "eventsFolded": page.events.len(),
        "completeness": {
            "complete": !page.has_more,
            "reason": page.has_more.then_some("limit_reached"),
            "omitted": page.total_count.saturating_sub(page.events.len()),
        },
        "state": Value::Object(state),
    }))
}

/// Return bounded event-level changes for one entity.
async fn exec_analyze_changes(
    core: &EmbeddedCore,
    policy: &DiagnosticPolicy,
    args: &Value,
) -> Result<Value> {
    let entity_id = args
        .get("entity_id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("invalid argument: entity_id is required"))?;
    let limit = limit_arg(args, "limit", 100, MAX_LIMIT)?;
    let mode = payload_mode(args, policy)?;
    let (query, signature) = scoped_query(policy, "analyze_changes", args, limit)?;
    let page = core.query_page(query.entity_id(entity_id)).await?;

    let changes: Vec<Value> = page
        .events
        .iter()
        .map(|e| {
            json!({
                "id": e.id,
                "timestamp": e.timestamp.to_rfc3339(),
                "event_type": e.event_type,
                "changed_fields": e.payload.as_object().map(|o| o.keys().collect::<Vec<_>>()),
                "payload": event_payload(&e.payload, mode),
            })
        })
        .collect();

    let items = Value::Array(changes);
    Ok(evidence_page(
        policy,
        &page,
        EvidencePageOptions {
            requested_limit: limit,
            signature: &signature,
            items: &items,
            incomplete_reason: "limit_reached",
            force_incomplete: false,
        },
    ))
}

/// Summarize a payload to a short string for timeline display.
fn summarize_payload(payload: &Value) -> String {
    match payload {
        Value::Object(map) => {
            let keys: Vec<&String> = map.keys().take(5).collect();
            if keys.is_empty() {
                "{}".to_string()
            } else {
                let mut s = keys
                    .iter()
                    .map(|k| k.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                if map.len() > 5 {
                    write!(s, " (+{} more)", map.len() - 5).unwrap();
                }
                s
            }
        }
        _ => payload.to_string().chars().take(100).collect(),
    }
}

#[cfg(test)]
mod tests {
    use allsource_core::embedded::{Config, EmbeddedCore, QueryPage};
    use serde_json::{Value, json};

    use super::{
        EvidencePageOptions, evidence_page, exec_reconstruct_state, payload_mode, query_signature,
        redact, tool_definitions,
    };
    use crate::diagnostics::{AccessProfile, DiagnosticPolicy};

    #[test]
    fn redaction_covers_nested_credential_keys() {
        let value = json!({
            "safe": "visible",
            "nested": { "authorization": "Bearer secret", "api_key": "key" }
        });

        let redacted = redact(&value);

        assert_eq!(redacted["safe"], "visible");
        assert_eq!(redacted["nested"]["authorization"], "[REDACTED]");
        assert_eq!(redacted["nested"]["api_key"], "[REDACTED]");
    }

    #[test]
    fn cursor_signature_is_bound_to_tenant_and_query_shape() {
        let tenant_a = DiagnosticPolicy::new(
            AccessProfile::HostedTenant,
            Some("tenant-a".to_string()),
            "prod",
        )
        .expect("tenant policy");
        let tenant_b = DiagnosticPolicy::new(
            AccessProfile::HostedTenant,
            Some("tenant-b".to_string()),
            "prod",
        )
        .expect("tenant policy");
        let args = json!({ "entity_id": "same-id", "limit": 25, "payload_mode": "redacted" });

        assert_ne!(
            query_signature(&tenant_a, "query_events", 25, &args),
            query_signature(&tenant_b, "query_events", 25, &args)
        );
        assert_ne!(
            query_signature(&tenant_a, "query_events", 25, &args),
            query_signature(&tenant_a, "query_events", 50, &json!({ "entity_id": "same-id" }))
        );
        assert_ne!(
            query_signature(&tenant_a, "query_events", 25, &args),
            query_signature(&tenant_a, "sample_events", 25, &args)
        );
        assert_eq!(
            query_signature(&tenant_a, "query_events", 50, &json!({ "limit": 50 })),
            query_signature(&tenant_a, "query_events", 50, &json!({}))
        );
    }

    #[test]
    fn hosted_profiles_cannot_request_or_advertise_full_payloads() {
        let policy = DiagnosticPolicy::new(
            AccessProfile::HostedTenant,
            Some("tenant-a".to_string()),
            "prod",
        )
        .expect("tenant policy");

        let error = payload_mode(&json!({ "payload_mode": "full" }), &policy)
            .expect_err("hosted profile must reject raw payload access");
        assert!(error.to_string().starts_with("access denied:"));

        for tool in tool_definitions(&policy) {
            if let Some(modes) = tool.input_schema["properties"]["payload_mode"]["enum"].as_array()
            {
                assert!(!modes.contains(&json!("full")));
            }
        }
    }

    #[tokio::test]
    async fn empty_reconstruction_reports_complete_evidence() {
        let core = EmbeddedCore::open(Config::builder().build().expect("valid config"))
            .await
            .expect("in-memory core");
        let policy =
            DiagnosticPolicy::new(AccessProfile::Local, None, "local").expect("local policy");

        let result = exec_reconstruct_state(&core, &policy, &json!({ "entity_id": "missing" }))
            .await
            .expect("empty reconstruction is a successful result");

        assert_eq!(result["completeness"]["complete"], true);
        assert_eq!(result["completeness"]["omitted"], 0);
    }

    #[test]
    fn sampled_pages_never_claim_completeness() {
        let policy =
            DiagnosticPolicy::new(AccessProfile::Local, None, "local").expect("local policy");
        let page = QueryPage {
            events: vec![],
            total_count: 0,
            has_more: false,
            next_offset: None,
        };
        let items = Value::Array(vec![]);

        let result = evidence_page(
            &policy,
            &page,
            EvidencePageOptions {
                requested_limit: 20,
                signature: "signature",
                items: &items,
                incomplete_reason: "sampled",
                force_incomplete: true,
            },
        );

        assert_eq!(result["completeness"]["complete"], false);
        assert_eq!(result["completeness"]["reason"], "sampled");
    }
}
