//! MCP tool definitions and execution.

use std::{
    collections::BTreeMap,
    fmt::Write,
    time::{Duration, Instant},
};

use allsource_core::embedded::{EmbeddedCore, Query};
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{
    diagnostics::DiagnosticPolicy,
    protocol::{ToolAnnotations, ToolDef, tool_error, tool_result},
    stores::{DEFAULT_STORE, Store, StoreRegistry},
};

const DEFAULT_LIMIT: usize = 50;
const MAX_LIMIT: usize = 500;
/// Events read per page while filtering by payload text, which the store cannot do itself.
const SCAN_PAGE: usize = 500;
const DEFAULT_MAX_SCAN: usize = 5_000;
const MAX_SCAN: usize = 50_000;
/// Longest a watch may hold a request open, so one call can never outlive a caller's timeout.
const MAX_WAIT_SECONDS: u64 = 55;
const WATCH_POLL: Duration = Duration::from_millis(500);

/// Build a read-only tool descriptor with shared diagnostic input and annotations.
fn read_tool(name: &str, title: &str, description: &str, mut input_schema: Value) -> ToolDef {
    if name != "list_stores" {
        input_schema["properties"]["store"] = json!({
            "type": "string",
            "description": "Which configured store to read; list_stores names them. Defaults to 'default'.",
        });
    }
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
                    "event_type_exact": { "type": "string", "description": "Filter by one exact event type; unlike event_type it never matches longer types" },
                    "payload_contains": { "type": "array", "items": { "type": "string" }, "description": "Keep only events whose payload text contains every one of these strings. Scans up to max_scan events, so the page is not cursor-paginated." },
                    "fields": { "type": "array", "items": { "type": "string" }, "description": "Project payloads to these dotted paths (e.g. 'run.id'); a path the payload lacks renders null." },
                    "max_scan": { "type": "integer", "minimum": 1, "maximum": MAX_SCAN, "default": DEFAULT_MAX_SCAN, "description": "Upper bound on events read while filtering by payload_contains." },
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
            "list_stores",
            "List readable stores",
            "Name every store this server may read, with its path and event count.",
            json!({
                "type": "object",
                "properties": {}
            }),
        ),
        read_tool(
            "watch_events",
            "Wait for new events",
            "Return events newer than a checkpoint, waiting up to wait_seconds for one to arrive. The caller loops on the returned checkpoint.",
            json!({
                "type": "object",
                "properties": {
                    "event_type": { "type": "string", "description": "Event type prefix to watch" },
                    "entity_id": { "type": "string" },
                    "checkpoint": { "type": "string", "description": "Checkpoint returned by the previous call. Omit to start from the newest event, so a first call does not replay history." },
                    "wait_seconds": { "type": "integer", "minimum": 0, "maximum": MAX_WAIT_SECONDS, "default": 0, "description": "How long to wait for the first new event. 0 returns immediately." },
                    "limit": { "type": "integer", "minimum": 1, "maximum": MAX_LIMIT, "default": DEFAULT_LIMIT },
                    "payload_mode": { "type": "string", "enum": payload_modes.clone() },
                    "fields": { "type": "array", "items": { "type": "string" } }
                }
            }),
        ),
        read_tool(
            "fold_entity_lifecycle",
            "Fold entities by state",
            "Group a family of events by entity and report each entity's latest state, so a caller never folds a lifecycle by hand.",
            json!({
                "type": "object",
                "properties": {
                    "event_type": { "type": "string", "description": "Event type prefix that forms the family, e.g. 'workflow_run'" },
                    "state": { "type": "string", "description": "Keep only entities whose latest state equals this (the segment after the last dot, e.g. 'completed')" },
                    "entity_id": { "type": "string", "description": "Fold one entity only" },
                    "fields": { "type": "array", "items": { "type": "string" }, "description": "Dotted payload paths carried from each entity's latest event" },
                    "since": { "type": "string", "format": "date-time" },
                    "until": { "type": "string", "format": "date-time" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": MAX_LIMIT, "default": DEFAULT_LIMIT },
                    "max_scan": { "type": "integer", "minimum": 1, "maximum": MAX_SCAN, "default": DEFAULT_MAX_SCAN }
                },
                "required": ["event_type"]
            }),
        ),
        read_tool(
            "fold_steps",
            "Fold work items by key",
            "Pair start and terminal events that share a payload key, reporting elapsed time and which items are still open.",
            json!({
                "type": "object",
                "properties": {
                    "event_type": { "type": "string", "description": "Event type prefix that forms the family, e.g. 'step_run'" },
                    "item_key": { "type": "string", "description": "Payload key identifying one item, e.g. 'step_run_id'" },
                    "group_key": { "type": "string", "description": "Payload key to filter on, e.g. 'run_id'" },
                    "group_value": { "type": "string", "description": "Value that group_key must equal" },
                    "terminal_states": { "type": "array", "items": { "type": "string" }, "description": "Type suffixes that close an item; defaults to completed, failed, cancelled. An item with none stays open, and its elapsed_ms is null." },
                    "fields": { "type": "array", "items": { "type": "string" }, "description": "Dotted payload paths carried from each item's latest event" },
                    "since": { "type": "string", "format": "date-time" },
                    "until": { "type": "string", "format": "date-time" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": MAX_LIMIT, "default": DEFAULT_LIMIT },
                    "max_scan": { "type": "integer", "minimum": 1, "maximum": MAX_SCAN, "default": DEFAULT_MAX_SCAN }
                },
                "required": ["event_type", "item_key"]
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
    stores: &StoreRegistry,
    policy: &DiagnosticPolicy,
    name: &str,
    args: &Value,
) -> Value {
    match execute_tool_inner(stores, policy, name, args).await {
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
    stores: &StoreRegistry,
    policy: &DiagnosticPolicy,
    name: &str,
    args: &Value,
) -> Result<Value> {
    if name == "list_stores" {
        return exec_list_stores(stores, policy);
    }

    let core = &selected_store(stores, policy, args)?.core;
    match name {
        "query_events" => exec_query_events(core, policy, args).await,
        "watch_events" => exec_watch_events(core, policy, args).await,
        "fold_entity_lifecycle" => exec_fold_entity_lifecycle(core, policy, args).await,
        "fold_steps" => exec_fold_steps(core, policy, args).await,
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

/// Resolve the store a call selected, refusing selection for a hosted tenant.
///
/// A hosted tenant is bound to one store for the life of the process, so letting a
/// request name another would let it read outside that binding.
fn selected_store<'a>(
    stores: &'a StoreRegistry,
    policy: &DiagnosticPolicy,
    args: &Value,
) -> Result<&'a Store> {
    let requested = args.get("store").and_then(Value::as_str);
    if requested.is_some_and(|name| name != DEFAULT_STORE) && policy.is_hosted_tenant() {
        anyhow::bail!("access denied: hosted tenant profiles read only their bound store");
    }
    stores.get(requested)
}

/// List the stores this server may read.
fn exec_list_stores(stores: &StoreRegistry, policy: &DiagnosticPolicy) -> Result<Value> {
    if policy.is_hosted_tenant() {
        anyhow::bail!("access denied: hosted tenant profiles read only their bound store");
    }
    let items: Vec<Value> = stores
        .iter()
        .map(|(name, store)| {
            json!({
                "store": name,
                "path": store.path.display().to_string(),
                "events": store.core.event_count(),
            })
        })
        .collect();
    Ok(json!({
        "context": policy.context(None),
        "items": items,
        "completeness": {
            "complete": true,
            "reason": Value::Null,
            "scanned": items.len(),
            "matched": items.len(),
            "omitted": 0,
            "unparsedTimestamps": 0,
            "sourcesRequested": items.len(),
            "sourcesRead": items.len(),
            "sourcesSkipped": [],
        }
    }))
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
    if let Some(event_type) = args.get("event_type_exact").and_then(|v| v.as_str()) {
        query = query.event_type(event_type);
    }
    if let Some(since) = since {
        query = query.since(since);
    }
    if let Some(until) = until {
        query = query.until(until);
    }

    Ok((query, signature))
}

/// Read an optional array-of-strings argument.
fn string_list(args: &Value, key: &str) -> Result<Vec<String>> {
    let Some(value) = args.get(key) else {
        return Ok(Vec::new());
    };
    let items = value
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("invalid argument: {key} must be an array of strings"))?;
    items
        .iter()
        .map(|item| {
            item.as_str().map(str::to_string).ok_or_else(|| {
                anyhow::anyhow!("invalid argument: {key} must be an array of strings")
            })
        })
        .collect()
}

/// Whether a payload's JSON text contains every needle.
fn payload_contains_all(payload: &Value, needles: &[String]) -> bool {
    let text = payload.to_string();
    needles.iter().all(|needle| text.contains(needle.as_str()))
}

/// Project a payload onto dotted paths; a path the payload lacks renders null.
fn project_fields(payload: &Value, paths: &[String]) -> Value {
    let projected: serde_json::Map<String, Value> = paths
        .iter()
        .map(|path| {
            let mut cursor = payload;
            for segment in path.split('.') {
                cursor = cursor.get(segment).unwrap_or(&Value::Null);
            }
            (path.clone(), cursor.clone())
        })
        .collect();
    Value::Object(projected)
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
    let fields = string_list(args, "fields")?;
    let needles = string_list(args, "payload_contains")?;

    if !needles.is_empty() {
        return scan_query_events(core, policy, args, limit, mode, &fields, &needles).await;
    }

    let (query, signature) = scoped_query(policy, "query_events", args, limit)?;

    let page = core.query_page(query).await?;
    let result: Vec<Value> = page
        .events
        .iter()
        .map(|e| render_event(e, mode, &fields))
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

/// Where a watch resumed from: a timestamp, and how many events at that exact
/// timestamp were already delivered.
///
/// A timestamp alone is not enough. `since` is inclusive, so resuming from it
/// repeats every event sharing that millisecond; resuming from the millisecond
/// after it drops the ones that have not been delivered yet.
struct Checkpoint {
    at: DateTime<Utc>,
    delivered_at_same_ms: usize,
}

impl Checkpoint {
    /// Parse `v1:<rfc3339>:<count>`.
    fn parse(raw: &str) -> Result<Self> {
        let mut parts = raw.splitn(3, '|');
        let version = parts.next();
        let at = parts.next().and_then(|value| value.parse().ok());
        let delivered = parts.next().and_then(|value| value.parse().ok());
        match (version, at, delivered) {
            (Some("v1"), Some(at), Some(delivered_at_same_ms)) => Ok(Self {
                at,
                delivered_at_same_ms,
            }),
            _ => anyhow::bail!("invalid argument: checkpoint is not one this server issued"),
        }
    }

    fn encode(at: DateTime<Utc>, delivered_at_same_ms: usize) -> String {
        format!("v1|{}|{delivered_at_same_ms}", at.to_rfc3339())
    }
}

/// Return events after a checkpoint, waiting briefly for the first one.
///
/// MCP is request/response, so a watch is a bounded long-poll the caller loops on,
/// never a stream. With no checkpoint it reports the newest event as the starting
/// point and returns nothing, so a first call cannot replay the whole store.
async fn exec_watch_events(
    core: &EmbeddedCore,
    policy: &DiagnosticPolicy,
    args: &Value,
) -> Result<Value> {
    let limit = limit_arg(args, "limit", DEFAULT_LIMIT, MAX_LIMIT)?;
    let mode = payload_mode(args, policy)?;
    let fields = string_list(args, "fields")?;
    let wait = args
        .get("wait_seconds")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .min(MAX_WAIT_SECONDS);

    let checkpoint = args
        .get("checkpoint")
        .and_then(Value::as_str)
        .map(Checkpoint::parse)
        .transpose()?;

    let Some(checkpoint) = checkpoint else {
        return watch_start(core, policy, args).await;
    };

    let deadline = Instant::now() + Duration::from_secs(wait);
    loop {
        let query =
            watch_query(policy, args, limit + checkpoint.delivered_at_same_ms).since(checkpoint.at);
        let page = core.query_page(query).await?;

        // Events at the checkpoint's own millisecond were already delivered.
        let fresh: Vec<&allsource_core::embedded::EventView> = page
            .events
            .iter()
            .filter(|event| event.timestamp >= checkpoint.at)
            .skip(checkpoint.delivered_at_same_ms)
            .take(limit)
            .collect();

        if !fresh.is_empty() || Instant::now() >= deadline {
            let newest = fresh.last().map_or(checkpoint.at, |event| event.timestamp);
            let delivered_at_newest = if fresh.is_empty() {
                checkpoint.delivered_at_same_ms
            } else {
                let at_newest = fresh
                    .iter()
                    .filter(|event| event.timestamp == newest)
                    .count();
                if newest == checkpoint.at {
                    checkpoint.delivered_at_same_ms + at_newest
                } else {
                    at_newest
                }
            };
            let items: Vec<Value> = fresh
                .iter()
                .map(|event| render_event(event, mode, &fields))
                .collect();
            return Ok(json!({
                "context": policy.context(Some(newest.to_rfc3339()).as_deref()),
                "items": items,
                "checkpoint": Checkpoint::encode(newest, delivered_at_newest),
                "page": {
                    "requestedLimit": limit,
                    "returned": items.len(),
                    "totalCount": items.len(),
                    "nextCursor": Value::Null,
                },
                "completeness": {
                    "complete": items.len() < limit,
                    "reason": if items.len() < limit { Value::Null } else { json!("limit_reached") },
                    "scanned": page.events.len(),
                    "matched": items.len(),
                    "omitted": 0,
                    "unparsedTimestamps": 0,
                    "sourcesRequested": 1,
                    "sourcesRead": 1,
                    "sourcesSkipped": [],
                }
            }));
        }

        tokio::time::sleep(WATCH_POLL).await;
    }
}

/// Answer a watch that carried no checkpoint: report where to start, return nothing.
async fn watch_start(
    core: &EmbeddedCore,
    policy: &DiagnosticPolicy,
    args: &Value,
) -> Result<Value> {
    let query = watch_query(policy, args, 1).descending(true);
    let page = core.query_page(query).await?;
    let newest = page.events.first().map(|event| event.timestamp);
    let at_newest = usize::from(newest.is_some());

    Ok(json!({
        "context": policy.context(newest.as_ref().map(DateTime::to_rfc3339).as_deref()),
        "items": [],
        "checkpoint": Checkpoint::encode(newest.unwrap_or_else(Utc::now), at_newest),
        "page": {
            "requestedLimit": 0,
            "returned": 0,
            "totalCount": 0,
            "nextCursor": Value::Null,
        },
        "completeness": {
            "complete": true,
            "reason": Value::Null,
            "scanned": page.events.len(),
            "matched": 0,
            "omitted": 0,
            "unparsedTimestamps": 0,
            "sourcesRequested": 1,
            "sourcesRead": 1,
            "sourcesSkipped": [],
        }
    }))
}

/// A tenant-bound query carrying only the filters a watch accepts.
fn watch_query(policy: &DiagnosticPolicy, args: &Value, limit: usize) -> Query {
    let mut query = Query::new().limit(limit.min(MAX_LIMIT)).descending(false);
    if let Some(tenant_id) = policy.tenant_id() {
        query = query.tenant_id(tenant_id);
    }
    if let Some(entity_id) = args.get("entity_id").and_then(Value::as_str) {
        query = query.entity_id(entity_id);
    }
    if let Some(event_type) = args.get("event_type").and_then(Value::as_str) {
        query = query.event_type_prefix(event_type);
    }
    query
}

/// Read every event of one family, oldest first, bounded by `max_scan`.
///
/// A fold has to see a whole family to be correct, so it scans rather than reading
/// one page, and reports how far it got instead of implying it saw everything.
async fn scan_family(
    core: &EmbeddedCore,
    policy: &DiagnosticPolicy,
    args: &Value,
    tool_name: &str,
) -> Result<(Vec<allsource_core::embedded::EventView>, usize, bool)> {
    let max_scan = limit_arg(args, "max_scan", DEFAULT_MAX_SCAN, MAX_SCAN)?;
    let mut events = Vec::new();
    let mut exhausted = false;

    while events.len() < max_scan {
        let page_size = SCAN_PAGE.min(max_scan - events.len());
        let (query, _) = scoped_query(policy, tool_name, args, page_size)?;
        let page = core.query_page(query.offset(events.len())).await?;
        if page.events.is_empty() {
            exhausted = true;
            break;
        }
        let last_page = page.next_offset.is_none();
        events.extend(page.events);
        if last_page {
            exhausted = true;
            break;
        }
    }

    let scanned = events.len();
    Ok((events, scanned, exhausted))
}

/// The segment after the last dot of an event type: `workflow_run.started` -> `started`.
fn state_of(event_type: &str) -> &str {
    event_type.rsplit('.').next().unwrap_or(event_type)
}

/// Wrap folded items in the same evidence envelope every other tool returns.
fn fold_result(
    policy: &DiagnosticPolicy,
    items: Vec<Value>,
    limit: usize,
    scanned: usize,
    exhausted: bool,
    fresh_through: Option<DateTime<Utc>>,
) -> Value {
    let truncated = items.len() > limit;
    let items: Vec<Value> = items.into_iter().take(limit).collect();
    let complete = exhausted && !truncated;
    json!({
        "context": policy.context(fresh_through.as_ref().map(DateTime::to_rfc3339).as_deref()),
        "items": items,
        "page": {
            "requestedLimit": limit,
            "returned": items.len(),
            "totalCount": items.len(),
            "nextCursor": Value::Null,
        },
        "completeness": {
            "complete": complete,
            "reason": if complete { Value::Null } else if truncated { json!("limit_reached") } else { json!("max_scan_reached") },
            "scanned": scanned,
            "matched": items.len(),
            "omitted": 0,
            "unparsedTimestamps": 0,
            "sourcesRequested": 1,
            "sourcesRead": 1,
            "sourcesSkipped": [],
        }
    })
}

/// Fold a family of events into one row per entity carrying its latest state.
async fn exec_fold_entity_lifecycle(
    core: &EmbeddedCore,
    policy: &DiagnosticPolicy,
    args: &Value,
) -> Result<Value> {
    if args.get("event_type").and_then(Value::as_str).is_none() {
        anyhow::bail!("invalid argument: event_type is required");
    }
    let limit = limit_arg(args, "limit", DEFAULT_LIMIT, MAX_LIMIT)?;
    let fields = string_list(args, "fields")?;
    let wanted_state = args.get("state").and_then(Value::as_str);

    let (events, scanned, exhausted) =
        scan_family(core, policy, args, "fold_entity_lifecycle").await?;
    let fresh_through = events.iter().map(|event| event.timestamp).max();

    // Last writer by timestamp wins, which is how every reader of this store folds.
    let mut folded: BTreeMap<String, &allsource_core::embedded::EventView> = BTreeMap::new();
    let mut first_seen: BTreeMap<String, DateTime<Utc>> = BTreeMap::new();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for event in &events {
        first_seen
            .entry(event.entity_id.clone())
            .and_modify(|seen| *seen = (*seen).min(event.timestamp))
            .or_insert(event.timestamp);
        *counts.entry(event.entity_id.clone()).or_default() += 1;
        folded
            .entry(event.entity_id.clone())
            .and_modify(|latest| {
                if event.timestamp >= latest.timestamp {
                    *latest = event;
                }
            })
            .or_insert(event);
    }

    let items: Vec<Value> = folded
        .iter()
        .filter(|(_, latest)| {
            wanted_state.is_none_or(|state| state_of(&latest.event_type) == state)
        })
        .map(|(entity_id, latest)| {
            json!({
                "entity_id": entity_id,
                "state": state_of(&latest.event_type),
                "state_event_type": latest.event_type,
                "state_at": latest.timestamp.to_rfc3339(),
                "first_seen_at": first_seen.get(entity_id).map(DateTime::to_rfc3339),
                "events": counts.get(entity_id.as_str()).copied().unwrap_or(0),
                "fields": project_fields(&latest.payload, &fields),
            })
        })
        .collect();

    Ok(fold_result(
        policy,
        items,
        limit,
        scanned,
        exhausted,
        fresh_through,
    ))
}

/// Pair start and terminal events that share a payload key.
async fn exec_fold_steps(
    core: &EmbeddedCore,
    policy: &DiagnosticPolicy,
    args: &Value,
) -> Result<Value> {
    if args.get("event_type").and_then(Value::as_str).is_none() {
        anyhow::bail!("invalid argument: event_type is required");
    }
    let item_key = args
        .get("item_key")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("invalid argument: item_key is required"))?;
    let limit = limit_arg(args, "limit", DEFAULT_LIMIT, MAX_LIMIT)?;
    let fields = string_list(args, "fields")?;
    let group_key = args.get("group_key").and_then(Value::as_str);
    let group_value = args.get("group_value").and_then(Value::as_str);
    if group_key.is_some() != group_value.is_some() {
        anyhow::bail!("invalid argument: group_key and group_value are given together");
    }
    let terminal = {
        let configured = string_list(args, "terminal_states")?;
        if configured.is_empty() {
            vec![
                "completed".to_string(),
                "failed".to_string(),
                "cancelled".to_string(),
            ]
        } else {
            configured
        }
    };

    let (events, scanned, exhausted) = scan_family(core, policy, args, "fold_steps").await?;
    let fresh_through = events.iter().map(|event| event.timestamp).max();

    let mut items: BTreeMap<String, Value> = BTreeMap::new();
    let mut opened: BTreeMap<String, DateTime<Utc>> = BTreeMap::new();
    let mut closed: BTreeMap<String, (DateTime<Utc>, String)> = BTreeMap::new();
    let mut latest: BTreeMap<String, &allsource_core::embedded::EventView> = BTreeMap::new();

    for event in &events {
        if let (Some(key), Some(value)) = (group_key, group_value)
            && event.payload.get(key).and_then(Value::as_str) != Some(value)
        {
            continue;
        }
        let Some(item) = event.payload.get(item_key).and_then(Value::as_str) else {
            continue;
        };
        let item = item.to_string();
        opened
            .entry(item.clone())
            .and_modify(|at| *at = (*at).min(event.timestamp))
            .or_insert(event.timestamp);
        if terminal
            .iter()
            .any(|state| state == state_of(&event.event_type))
        {
            closed
                .entry(item.clone())
                .and_modify(|(at, state)| {
                    if event.timestamp >= *at {
                        *at = event.timestamp;
                        *state = state_of(&event.event_type).to_string();
                    }
                })
                .or_insert((event.timestamp, state_of(&event.event_type).to_string()));
        }
        latest
            .entry(item.clone())
            .and_modify(|current| {
                if event.timestamp >= current.timestamp {
                    *current = event;
                }
            })
            .or_insert(event);
        items.entry(item).or_insert(Value::Null);
    }

    let rows: Vec<Value> = items
        .keys()
        .map(|item| {
            let started_at = opened.get(item).copied();
            let finished = closed.get(item);
            let elapsed_ms = started_at
                .and_then(|start| finished.map(|(end, _)| (*end - start).num_milliseconds()));
            json!({
                "item": item,
                "started_at": started_at.map(|at| at.to_rfc3339()),
                "finished_at": finished.map(|(at, _)| at.to_rfc3339()),
                "final_state": finished.map(|(_, state)| state.clone()),
                "open": finished.is_none(),
                "elapsed_ms": elapsed_ms,
                "fields": latest
                    .get(item)
                    .map_or(Value::Null, |event| project_fields(&event.payload, &fields)),
            })
        })
        .collect();

    Ok(fold_result(
        policy,
        rows,
        limit,
        scanned,
        exhausted,
        fresh_through,
    ))
}

/// Render one event, applying payload exposure mode then field projection.
fn render_event(
    event: &allsource_core::embedded::EventView,
    mode: PayloadMode,
    fields: &[String],
) -> Value {
    let payload = event_payload(&event.payload, mode);
    let payload = if fields.is_empty() {
        payload
    } else {
        project_fields(&payload, fields)
    };
    json!({
        "id": event.id.to_string(),
        "entity_id": event.entity_id,
        "event_type": event.event_type,
        "tenant_id": event.tenant_id,
        "timestamp": event.timestamp.to_rfc3339(),
        "version": event.version,
        "payload": payload,
        "metadata": event.metadata.as_ref().map(redact),
    })
}

/// Filter by payload text, which the store cannot do, by scanning bounded pages.
///
/// The scan starts at the first event every time: an offset cursor would describe
/// the STORE's ordering, not the filtered result, so the caller would silently skip
/// matches. `max_scan` bounds the read, and `completeness` reports what was covered.
async fn scan_query_events(
    core: &EmbeddedCore,
    policy: &DiagnosticPolicy,
    args: &Value,
    limit: usize,
    mode: PayloadMode,
    fields: &[String],
    needles: &[String],
) -> Result<Value> {
    if args.get("cursor").is_some() {
        anyhow::bail!(
            "invalid argument: payload_contains cannot be combined with cursor; raise max_scan instead"
        );
    }
    let max_scan = limit_arg(args, "max_scan", DEFAULT_MAX_SCAN, MAX_SCAN)?;

    let mut matched: Vec<Value> = Vec::new();
    let mut scanned = 0usize;
    let mut fresh_through: Option<DateTime<Utc>> = None;
    let mut exhausted = false;

    while scanned < max_scan && matched.len() < limit {
        let page_size = SCAN_PAGE.min(max_scan - scanned);
        let (query, _) = scoped_query(policy, "query_events", args, page_size)?;
        let page = core.query_page(query.offset(scanned)).await?;
        if page.events.is_empty() {
            exhausted = true;
            break;
        }
        for event in &page.events {
            scanned += 1;
            fresh_through = fresh_through.max(Some(event.timestamp));
            if payload_contains_all(&event.payload, needles) {
                matched.push(render_event(event, mode, fields));
                if matched.len() == limit {
                    break;
                }
            }
        }
        if page.next_offset.is_none() {
            exhausted = true;
            break;
        }
    }

    let complete = exhausted && matched.len() < limit;
    Ok(json!({
        "context": policy.context(fresh_through.as_ref().map(DateTime::to_rfc3339).as_deref()),
        "items": Value::Array(matched.clone()),
        "page": {
            "requestedLimit": limit,
            "returned": matched.len(),
            "totalCount": matched.len(),
            "nextCursor": Value::Null,
        },
        "completeness": {
            "complete": complete,
            "reason": if complete { Value::Null } else if matched.len() == limit { json!("limit_reached") } else { json!("max_scan_reached") },
            "scanned": scanned,
            "matched": matched.len(),
            "omitted": 0,
            "unparsedTimestamps": 0,
            "sourcesRequested": 1,
            "sourcesRead": 1,
            "sourcesSkipped": [],
        }
    }))
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
        EvidencePageOptions, evidence_page, exec_fold_entity_lifecycle, exec_fold_steps,
        exec_list_stores, exec_query_events, exec_reconstruct_state, exec_watch_events,
        payload_contains_all, payload_mode, project_fields, query_signature, redact,
        selected_store, tool_definitions,
    };
    use crate::{
        diagnostics::{AccessProfile, DiagnosticPolicy},
        stores::StoreRegistry,
    };

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
            query_signature(
                &tenant_a,
                "query_events",
                50,
                &json!({ "entity_id": "same-id" })
            )
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

    async fn two_store_registry() -> StoreRegistry {
        let mut cores = Vec::new();
        for _ in 0..2 {
            cores.push(
                EmbeddedCore::open(Config::builder().build().expect("valid config"))
                    .await
                    .expect("in-memory core"),
            );
        }
        let mut cores = cores.into_iter();
        StoreRegistry::from_cores(vec![
            ("default", cores.next().expect("default core")),
            ("workspace", cores.next().expect("workspace core")),
        ])
    }

    #[tokio::test]
    async fn a_call_selects_a_store_by_name_and_an_unknown_name_is_not_found() {
        let stores = two_store_registry().await;
        let policy =
            DiagnosticPolicy::new(AccessProfile::Local, None, "local").expect("local policy");

        assert!(selected_store(&stores, &policy, &json!({})).is_ok());
        assert!(selected_store(&stores, &policy, &json!({ "store": "workspace" })).is_ok());

        let error = selected_store(&stores, &policy, &json!({ "store": "nope" }))
            .map(|_| ())
            .expect_err("an unknown store is not readable");
        assert!(error.to_string().starts_with("not found:"));
    }

    #[tokio::test]
    async fn a_hosted_tenant_cannot_leave_its_bound_store() {
        let stores = two_store_registry().await;
        let policy = DiagnosticPolicy::new(
            AccessProfile::HostedTenant,
            Some("tenant-a".to_string()),
            "prod",
        )
        .expect("tenant policy");

        let error = selected_store(&stores, &policy, &json!({ "store": "workspace" }))
            .map(|_| ())
            .expect_err("a hosted tenant is bound to one store");
        assert!(error.to_string().starts_with("access denied:"));

        assert!(
            selected_store(&stores, &policy, &json!({})).is_ok(),
            "its own store stays readable"
        );
        let error = exec_list_stores(&stores, &policy).expect_err("listing names other stores");
        assert!(error.to_string().starts_with("access denied:"));
    }

    async fn core_with(events: &[(&str, &str, Value)]) -> EmbeddedCore {
        let core = EmbeddedCore::open(
            Config::builder()
                .single_tenant(true)
                .build()
                .expect("valid config"),
        )
        .await
        .expect("in-memory core");
        for (entity_id, event_type, payload) in events {
            core.ingest(allsource_core::embedded::IngestEvent {
                entity_id,
                event_type,
                payload: payload.clone(),
                metadata: None,
                tenant_id: None,
            })
            .await
            .expect("ingest");
        }
        core
    }

    #[tokio::test]
    async fn a_first_watch_reports_where_to_start_without_replaying_history() {
        let core = core_with(&[
            ("run-1", "workflow_run.started", json!({})),
            ("run-2", "workflow_run.started", json!({})),
        ])
        .await;
        let policy =
            DiagnosticPolicy::new(AccessProfile::Local, None, "local").expect("local policy");

        let first = exec_watch_events(&core, &policy, &json!({ "event_type": "workflow_run" }))
            .await
            .expect("watch");

        assert_eq!(
            first["items"].as_array().expect("items").len(),
            0,
            "a first watch must not replay the store"
        );
        assert!(
            first["checkpoint"]
                .as_str()
                .expect("checkpoint")
                .starts_with("v1|")
        );
    }

    #[tokio::test]
    async fn a_watch_returns_only_events_after_its_checkpoint() {
        let core = core_with(&[("run-1", "workflow_run.started", json!({}))]).await;
        let policy =
            DiagnosticPolicy::new(AccessProfile::Local, None, "local").expect("local policy");

        let first = exec_watch_events(&core, &policy, &json!({ "event_type": "workflow_run" }))
            .await
            .expect("watch");
        let checkpoint = first["checkpoint"]
            .as_str()
            .expect("checkpoint")
            .to_string();

        let idle = exec_watch_events(
            &core,
            &policy,
            &json!({ "event_type": "workflow_run", "checkpoint": checkpoint.clone() }),
        )
        .await
        .expect("watch");
        assert_eq!(
            idle["items"].as_array().expect("items").len(),
            0,
            "the event at the checkpoint was already delivered"
        );

        core.ingest(allsource_core::embedded::IngestEvent {
            entity_id: "run-1",
            event_type: "workflow_run.completed",
            payload: json!({}),
            metadata: None,
            tenant_id: None,
        })
        .await
        .expect("ingest");

        let after = exec_watch_events(
            &core,
            &policy,
            &json!({ "event_type": "workflow_run", "checkpoint": checkpoint }),
        )
        .await
        .expect("watch");
        let items = after["items"].as_array().expect("items");
        assert_eq!(items.len(), 1, "only the new event comes back");
        assert_eq!(items[0]["event_type"], "workflow_run.completed");
    }

    #[tokio::test]
    async fn a_watch_refuses_a_checkpoint_it_did_not_issue() {
        let core = core_with(&[]).await;
        let policy =
            DiagnosticPolicy::new(AccessProfile::Local, None, "local").expect("local policy");

        let error = exec_watch_events(&core, &policy, &json!({ "checkpoint": "yesterday" }))
            .await
            .expect_err("a checkpoint must be one this server issued");
        assert!(error.to_string().starts_with("invalid argument:"));
    }

    /// `since` is inclusive, so a checkpoint carrying only a timestamp would
    /// re-deliver every event at that timestamp on each call. The delivered-count
    /// is what makes it resumable, and it is only sound because the store orders
    /// by (timestamp, version, scan position) — a total order added so "the
    /// latest event" is unambiguous (all-source issue #177).
    ///
    /// Event timestamps come from the HLC's `physical_ms`, so two events CAN
    /// share one. `ingest` cannot be made to produce that here — each call
    /// crosses a millisecond — so this drives the arithmetic directly from a
    /// hand-built checkpoint instead of waiting on a collision that may not come.
    #[tokio::test]
    async fn a_checkpoint_skips_exactly_the_events_already_delivered_at_its_instant() {
        let core = core_with(&[
            ("run-0", "workflow_run.started", json!({ "index": 0 })),
            ("run-0", "workflow_run.progressed", json!({ "index": 1 })),
            ("run-0", "workflow_run.completed", json!({ "index": 2 })),
        ])
        .await;
        let policy =
            DiagnosticPolicy::new(AccessProfile::Local, None, "local").expect("local policy");

        let all = exec_query_events(
            &core,
            &policy,
            &json!({ "event_type": "workflow_run", "payload_mode": "full" }),
        )
        .await
        .expect("query");
        let events = all["items"].as_array().expect("items");
        assert_eq!(events.len(), 3, "three events to resume across");
        let second = events[1]["timestamp"].as_str().expect("timestamp");

        let indexes_of = |page: &Value| -> Vec<u64> {
            page["items"]
                .as_array()
                .expect("items")
                .iter()
                .map(|item| item["payload"]["index"].as_u64().expect("index"))
                .collect()
        };

        let none_delivered = exec_watch_events(
            &core,
            &policy,
            &json!({
                "event_type": "workflow_run",
                "checkpoint": format!("v1|{second}|0"),
                "payload_mode": "full",
            }),
        )
        .await
        .expect("watch");
        assert_eq!(
            indexes_of(&none_delivered),
            vec![1, 2],
            "delivered=0 keeps the event at the checkpoint's own instant, because since is inclusive"
        );

        let one_delivered = exec_watch_events(
            &core,
            &policy,
            &json!({
                "event_type": "workflow_run",
                "checkpoint": format!("v1|{second}|1"),
                "payload_mode": "full",
            }),
        )
        .await
        .expect("watch");
        assert_eq!(
            indexes_of(&one_delivered),
            vec![2],
            "delivered=1 drops exactly the one already handed back, and nothing after it"
        );
    }

    #[tokio::test]
    async fn entity_lifecycle_folds_to_the_latest_state_and_filters_on_it() {
        let core = core_with(&[
            ("run-1", "workflow_run.started", json!({ "name": "first" })),
            ("run-2", "workflow_run.started", json!({ "name": "second" })),
            (
                "run-1",
                "workflow_run.completed",
                json!({ "name": "first" }),
            ),
        ])
        .await;
        let policy =
            DiagnosticPolicy::new(AccessProfile::Local, None, "local").expect("local policy");

        let all = exec_fold_entity_lifecycle(
            &core,
            &policy,
            &json!({ "event_type": "workflow_run", "fields": ["name"] }),
        )
        .await
        .expect("fold");
        let items = all["items"].as_array().expect("items");
        assert_eq!(items.len(), 2, "one row per entity, not per event");
        let run_1 = items
            .iter()
            .find(|i| i["entity_id"] == "run-1")
            .expect("run-1");
        assert_eq!(run_1["state"], "completed", "the latest event wins");
        assert_eq!(run_1["events"], 2);
        assert_eq!(run_1["fields"]["name"], "first");
        assert_eq!(
            items
                .iter()
                .find(|i| i["entity_id"] == "run-2")
                .expect("run-2")["state"],
            "started"
        );

        let completed = exec_fold_entity_lifecycle(
            &core,
            &policy,
            &json!({ "event_type": "workflow_run", "state": "completed" }),
        )
        .await
        .expect("fold");
        let items = completed["items"].as_array().expect("items");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["entity_id"], "run-1");
    }

    #[tokio::test]
    async fn steps_pair_by_payload_key_and_report_open_items() {
        let core = core_with(&[
            (
                "step-a",
                "step_run.started",
                json!({ "step_run_id": "a", "run_id": "run-1" }),
            ),
            (
                "step-a",
                "step_run.completed",
                json!({ "step_run_id": "a", "run_id": "run-1" }),
            ),
            (
                "step-b",
                "step_run.started",
                json!({ "step_run_id": "b", "run_id": "run-1" }),
            ),
            (
                "step-c",
                "step_run.started",
                json!({ "step_run_id": "c", "run_id": "run-2" }),
            ),
        ])
        .await;
        let policy =
            DiagnosticPolicy::new(AccessProfile::Local, None, "local").expect("local policy");

        let folded = exec_fold_steps(
            &core,
            &policy,
            &json!({
                "event_type": "step_run",
                "item_key": "step_run_id",
                "group_key": "run_id",
                "group_value": "run-1",
            }),
        )
        .await
        .expect("fold");

        let items = folded["items"].as_array().expect("items");
        assert_eq!(
            items.len(),
            2,
            "run-2's step is filtered out by group_value"
        );
        let a = items.iter().find(|i| i["item"] == "a").expect("item a");
        assert_eq!(a["open"], false);
        assert_eq!(a["final_state"], "completed");
        assert!(a["elapsed_ms"].is_number());
        let b = items.iter().find(|i| i["item"] == "b").expect("item b");
        assert_eq!(b["open"], true, "a step with no terminal event stays open");
        assert_eq!(b["final_state"], Value::Null);
    }

    #[tokio::test]
    async fn a_fold_requires_the_arguments_that_define_it() {
        let core = core_with(&[]).await;
        let policy =
            DiagnosticPolicy::new(AccessProfile::Local, None, "local").expect("local policy");

        let error = exec_fold_steps(&core, &policy, &json!({ "event_type": "step_run" }))
            .await
            .expect_err("item_key names the thing being folded");
        assert!(error.to_string().starts_with("invalid argument:"));

        let error = exec_fold_steps(
            &core,
            &policy,
            &json!({ "event_type": "step_run", "item_key": "id", "group_key": "run_id" }),
        )
        .await
        .expect_err("a group key without a value filters nothing");
        assert!(error.to_string().starts_with("invalid argument:"));
    }

    #[test]
    fn field_projection_renders_missing_paths_as_null() {
        let payload = json!({ "run": { "id": "run-1" }, "step": 3 });

        let projected = project_fields(
            &payload,
            &[
                "run.id".to_string(),
                "step".to_string(),
                "absent.x".to_string(),
            ],
        );

        assert_eq!(projected["run.id"], "run-1");
        assert_eq!(projected["step"], 3);
        assert_eq!(projected["absent.x"], Value::Null);
    }

    #[test]
    fn payload_text_filter_requires_every_needle() {
        let payload = json!({ "reason": "queue paused", "org": "acme" });

        assert!(payload_contains_all(
            &payload,
            &["paused".to_string(), "acme".to_string()]
        ));
        assert!(!payload_contains_all(
            &payload,
            &["paused".to_string(), "missing".to_string()]
        ));
    }

    #[tokio::test]
    async fn payload_contains_excludes_non_matching_events_and_reports_the_scan() {
        let core = EmbeddedCore::open(
            Config::builder()
                .single_tenant(true)
                .build()
                .expect("valid config"),
        )
        .await
        .expect("in-memory core");
        for (entity, reason) in [("run-1", "queue paused"), ("run-2", "queue drained")] {
            core.ingest(allsource_core::embedded::IngestEvent {
                entity_id: entity,
                event_type: "queue.state_changed",
                payload: json!({ "reason": reason }),
                metadata: None,
                tenant_id: None,
            })
            .await
            .expect("ingest");
        }
        let policy =
            DiagnosticPolicy::new(AccessProfile::Local, None, "local").expect("local policy");

        let result = exec_query_events(
            &core,
            &policy,
            &json!({ "payload_contains": ["paused"], "limit": 10 }),
        )
        .await
        .expect("filtered query");

        let items = result["items"].as_array().expect("items array");
        assert_eq!(items.len(), 1, "only the matching event is returned");
        assert_eq!(items[0]["entity_id"], "run-1");
        assert_eq!(result["completeness"]["scanned"], 2);
        assert_eq!(result["completeness"]["matched"], 1);
        assert_eq!(result["completeness"]["complete"], true);
    }

    #[tokio::test]
    async fn payload_contains_refuses_a_cursor_rather_than_skipping_matches() {
        let core = EmbeddedCore::open(Config::builder().build().expect("valid config"))
            .await
            .expect("in-memory core");
        let policy =
            DiagnosticPolicy::new(AccessProfile::Local, None, "local").expect("local policy");

        let error = exec_query_events(
            &core,
            &policy,
            &json!({ "payload_contains": ["x"], "cursor": "v1:0:abc" }),
        )
        .await
        .expect_err("a cursor describes store order, not filtered order");

        assert!(error.to_string().starts_with("invalid argument:"));
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
