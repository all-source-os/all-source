//! `trace`: follow one id across every readable store.
//!
//! Each store is scanned once, bounded by `max_scan`, and the walk runs over
//! that in-memory window, so raising `depth` never re-reads a store. An event
//! joins the trace when its entity id equals a frontier id or a string value in
//! its payload, as the caller is allowed to see it, equals one exactly. The ids an event carries are
//! read from that same view, so a redacted value is never matched or followed.

use std::collections::{BTreeMap, BTreeSet};

use allsource_core::embedded::{EmbeddedCore, EventView, Query};
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use super::{
    DEFAULT_MAX_SCAN, MAX_LIMIT, MAX_SCAN, PayloadMode, SCAN_PAGE, event_payload, limit_arg,
    payload_mode, redact, render_event, string_list, timestamp_arg,
};
use crate::{
    diagnostics::DiagnosticPolicy,
    stores::{DEFAULT_STORE, StoreRegistry},
};

const DEFAULT_TRACE_LIMIT: usize = 200;
const DEFAULT_DEPTH: u64 = 1;
const MAX_DEPTH: u64 = 3;
/// Ids followed from one hop to the next; more than this and the walk is
/// reported as truncated rather than fanning out across the whole store.
const FRONTIER_CAP: usize = 50;
const MIN_ID_LEN: usize = 3;
const DEFAULT_ID_KEYS: [&str; 4] = ["id", "_id", "Id", "_ids"];

/// The JSON schema of the `trace` tool's input.
pub(super) fn input_schema(payload_modes: &Value) -> Value {
    json!({
        "type": "object",
        "properties": {
            "id": { "type": "string", "minLength": MIN_ID_LEN, "description": "The id to follow: an entity id, run id, or any id a payload carries" },
            "depth": { "type": "integer", "minimum": 0, "maximum": MAX_DEPTH, "default": DEFAULT_DEPTH, "description": "Hops to follow. 0 returns only events that reference id; each further hop follows the ids those events carry." },
            "stores": { "type": "array", "items": { "type": "string" }, "description": "Stores to read; defaults to every configured store. A hosted tenant reads only 'default'." },
            "limit": { "type": "integer", "minimum": 1, "maximum": MAX_LIMIT, "default": DEFAULT_TRACE_LIMIT },
            "max_scan": { "type": "integer", "minimum": 1, "maximum": MAX_SCAN, "default": DEFAULT_MAX_SCAN, "description": "Upper bound on events read from EACH store." },
            "payload_mode": { "type": "string", "enum": payload_modes },
            "fields": { "type": "array", "items": { "type": "string" }, "description": "Project rendered payloads to these dotted paths." },
            "since": { "type": "string", "format": "date-time" },
            "until": { "type": "string", "format": "date-time" },
            "id_keys": { "type": "array", "items": { "type": "string" }, "description": "Payload keys whose string values count as ids to follow. An entry starting with '_' or an uppercase letter matches as a key suffix (run_id, runId); any other entry matches the whole key. Defaults to id, _id, Id, _ids." }
        },
        "required": ["id"]
    })
}

struct Scanned {
    store: String,
    event: EventView,
    /// Every string value in the permitted payload view; empty when the view
    /// shows no values (`none`, `keys`), so a key name never matches an id.
    values: BTreeSet<String>,
    carries: BTreeSet<String>,
}

fn string_values(view: &Value, out: &mut BTreeSet<String>) {
    match view {
        Value::String(text) => {
            out.insert(text.clone());
        }
        Value::Array(items) => items.iter().for_each(|item| string_values(item, out)),
        Value::Object(map) => map.values().for_each(|value| string_values(value, out)),
        _ => {}
    }
}

struct Hit {
    hop: u64,
    matched_id: String,
    via: &'static str,
}

/// Follow one id across stores, breadth first, up to `depth` hops.
#[allow(clippy::too_many_lines)] // One read of the scan, the walk and the envelope.
pub(super) async fn exec_trace(
    stores: &StoreRegistry,
    policy: &DiagnosticPolicy,
    args: &Value,
) -> Result<Value> {
    let root = args
        .get("id")
        .and_then(Value::as_str)
        .map(str::trim)
        .ok_or_else(|| anyhow::anyhow!("invalid argument: id is required"))?;
    if root.chars().count() < MIN_ID_LEN {
        anyhow::bail!("invalid argument: id must be at least {MIN_ID_LEN} characters");
    }
    if args.get("store").is_some() {
        anyhow::bail!("invalid argument: trace selects stores with 'stores', not 'store'");
    }
    let depth = args
        .get("depth")
        .map_or(Some(DEFAULT_DEPTH), Value::as_u64)
        .filter(|depth| *depth <= MAX_DEPTH)
        .ok_or_else(|| {
            anyhow::anyhow!("invalid argument: depth must be between 0 and {MAX_DEPTH}")
        })?;
    let limit = limit_arg(args, "limit", DEFAULT_TRACE_LIMIT, MAX_LIMIT)?;
    let max_scan = limit_arg(args, "max_scan", DEFAULT_MAX_SCAN, MAX_SCAN)?;
    let mode = payload_mode(args, policy)?;
    let fields = string_list(args, "fields")?;
    let since = timestamp_arg(args, "since")?;
    let until = timestamp_arg(args, "until")?;
    if since.zip(until).is_some_and(|(start, end)| start > end) {
        anyhow::bail!("invalid argument: since must not be after until");
    }
    let id_keys = {
        let configured = string_list(args, "id_keys")?;
        if configured.is_empty() {
            DEFAULT_ID_KEYS.iter().map(ToString::to_string).collect()
        } else {
            configured
        }
    };
    let requested = requested_stores(stores, policy, args)?;

    let mut scanned_events: Vec<Scanned> = Vec::new();
    let mut scanned_per_store = serde_json::Map::new();
    let mut store_refresh = serde_json::Map::new();
    let mut every_store_exhausted = true;
    for name in &requested {
        let store = stores.get(Some(name))?;
        let core = store.fresh_core().await;
        let (events, exhausted) = scan_store(core, policy, since, until, max_scan).await?;
        every_store_exhausted &= exhausted;
        scanned_per_store.insert(name.clone(), json!(events.len()));
        store_refresh.insert(name.clone(), store.refresh_context().await);
        for event in events {
            let view = event_payload(&event.payload, mode);
            let mut carries = BTreeSet::new();
            carries.insert(event.entity_id.clone());
            collect_ids(&view, &id_keys, &mut carries);
            if let Some(metadata) = event.metadata.as_ref() {
                collect_ids(&redact(metadata), &id_keys, &mut carries);
            }
            let mut values = BTreeSet::new();
            if matches!(mode, PayloadMode::Redacted | PayloadMode::Full) {
                string_values(&view, &mut values);
            }
            scanned_events.push(Scanned {
                store: name.clone(),
                values,
                event,
                carries,
            });
        }
    }

    let mut hits: BTreeMap<usize, Hit> = BTreeMap::new();
    let mut visited: BTreeMap<String, u64> = BTreeMap::from([(root.to_string(), 0)]);
    let mut edges: Vec<Value> = Vec::new();
    let mut frontier: Vec<String> = vec![root.to_string()];
    let mut frontier_truncated = false;

    for hop in 0..=depth {
        let mut next: BTreeSet<String> = BTreeSet::new();
        for (index, scanned) in scanned_events.iter().enumerate() {
            if hits.contains_key(&index) {
                continue;
            }
            let Some((matched_id, via)) = frontier.iter().find_map(|id| {
                if scanned.event.entity_id == *id {
                    Some((id, "entity_id"))
                } else if scanned.values.contains(id) {
                    Some((id, "payload"))
                } else {
                    None
                }
            }) else {
                continue;
            };
            if hop < depth {
                for carried in &scanned.carries {
                    if carried != matched_id
                        && !visited.contains_key(carried)
                        && next.insert(carried.clone())
                    {
                        edges.push(json!({
                            "from_id": matched_id,
                            "to_id": carried,
                            "via_event": scanned.event.id.to_string(),
                        }));
                    }
                }
            }
            hits.insert(
                index,
                Hit {
                    hop,
                    matched_id: matched_id.clone(),
                    via,
                },
            );
        }
        if hop == depth || next.is_empty() {
            break;
        }
        if next.len() > FRONTIER_CAP {
            frontier_truncated = true;
        }
        frontier = next.into_iter().take(FRONTIER_CAP).collect();
        for id in &frontier {
            visited.insert(id.clone(), hop + 1);
        }
        edges.retain(|edge| {
            edge["to_id"]
                .as_str()
                .is_some_and(|to| visited.contains_key(to))
        });
    }

    let mut ordered: Vec<(usize, &Hit)> = hits.iter().map(|(index, hit)| (*index, hit)).collect();
    ordered.sort_by(|(a, _), (b, _)| {
        let (a, b) = (&scanned_events[*a], &scanned_events[*b]);
        (a.event.timestamp, &a.store, a.event.id).cmp(&(b.event.timestamp, &b.store, b.event.id))
    });
    let matched = ordered.len();
    let fresh_through: Option<DateTime<Utc>> = ordered
        .iter()
        .map(|(index, _)| scanned_events[*index].event.timestamp)
        .max();

    let mut graph_ids: BTreeMap<&str, (u64, Option<&str>, usize)> = visited
        .iter()
        .map(|(id, hop)| (id.as_str(), (*hop, None, 0)))
        .collect();
    for (index, hit) in &ordered {
        if let Some(entry) = graph_ids.get_mut(hit.matched_id.as_str()) {
            entry.1.get_or_insert(scanned_events[*index].store.as_str());
            entry.2 += 1;
        }
    }

    let items: Vec<Value> = ordered
        .iter()
        .take(limit)
        .map(|(index, hit)| {
            let scanned = &scanned_events[*index];
            let mut item = render_event(&scanned.event, mode, &fields);
            item["store"] = json!(scanned.store);
            item["hop"] = json!(hit.hop);
            item["matched_by"] = json!({ "id": hit.matched_id, "via": hit.via });
            item["carries"] = json!(scanned.carries);
            item
        })
        .collect();

    let reason = if !every_store_exhausted {
        Some("max_scan_reached")
    } else if frontier_truncated {
        Some("frontier_truncated")
    } else if matched > limit {
        Some("limit_reached")
    } else {
        None
    };

    let mut context = policy.context(fresh_through.as_ref().map(DateTime::to_rfc3339).as_deref());
    context["stores"] = Value::Object(store_refresh);

    Ok(json!({
        "context": context,
        "items": items,
        "graph": {
            "ids": graph_ids
                .iter()
                .map(|(id, (hop, store, count))| json!({
                    "id": id,
                    "hop": hop,
                    "first_seen_store": store,
                    "event_count": count,
                }))
                .collect::<Vec<_>>(),
            "edges": edges,
        },
        "page": {
            "requestedLimit": limit,
            "returned": items.len(),
            "totalCount": matched,
            "nextCursor": Value::Null,
        },
        "completeness": {
            "complete": reason.is_none(),
            "reason": reason,
            "scanned": scanned_per_store,
            "matched": matched,
            "omitted": matched.saturating_sub(items.len()),
            "unparsedTimestamps": 0,
            "sourcesRequested": requested.len(),
            "sourcesRead": requested.len(),
            "sourcesSkipped": [],
        }
    }))
}

/// The stores a trace reads, refusing any but the bound one for a hosted tenant.
fn requested_stores(
    stores: &StoreRegistry,
    policy: &DiagnosticPolicy,
    args: &Value,
) -> Result<Vec<String>> {
    let named = string_list(args, "stores")?;
    if policy.is_hosted_tenant() {
        if named.iter().any(|name| name != DEFAULT_STORE) {
            anyhow::bail!("access denied: hosted tenant profiles read only their bound store");
        }
        return Ok(vec![DEFAULT_STORE.to_string()]);
    }
    if named.is_empty() {
        return Ok(stores.iter().map(|(name, _)| name.clone()).collect());
    }
    let mut unique = Vec::new();
    for name in named {
        stores.get(Some(&name))?;
        if !unique.contains(&name) {
            unique.push(name);
        }
    }
    Ok(unique)
}

/// Read one store oldest first, up to `max_scan` events. `true` when the
/// window covered every event in range.
async fn scan_store(
    core: &EmbeddedCore,
    policy: &DiagnosticPolicy,
    since: Option<DateTime<Utc>>,
    until: Option<DateTime<Utc>>,
    max_scan: usize,
) -> Result<(Vec<EventView>, bool)> {
    let mut events = Vec::new();
    while events.len() < max_scan {
        let mut query = Query::new()
            .limit(SCAN_PAGE.min(max_scan - events.len()))
            .offset(events.len())
            .descending(false);
        if let Some(tenant_id) = policy.tenant_id() {
            query = query.tenant_id(tenant_id);
        }
        if let Some(since) = since {
            query = query.since(since);
        }
        if let Some(until) = until {
            query = query.until(until);
        }
        let page = core.query_page(query).await?;
        let last_page = page.next_offset.is_none();
        let empty = page.events.is_empty();
        events.extend(page.events);
        if empty || last_page {
            return Ok((events, true));
        }
    }
    Ok((events, false))
}

/// Whether a payload key names an id under the configured `id_keys`.
fn is_id_key(key: &str, id_keys: &[String]) -> bool {
    id_keys.iter().any(|wanted| {
        let suffix =
            wanted.starts_with('_') || wanted.starts_with(|c: char| c.is_ascii_uppercase());
        key == wanted || (suffix && key.ends_with(wanted.as_str()))
    })
}

/// Gather string values under id keys, at any depth of the payload.
fn collect_ids(value: &Value, id_keys: &[String], out: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                if is_id_key(key, id_keys) {
                    match value {
                        Value::String(id) => push_id(id, out),
                        Value::Array(values) => values
                            .iter()
                            .filter_map(Value::as_str)
                            .for_each(|id| push_id(id, out)),
                        _ => {}
                    }
                }
                collect_ids(value, id_keys, out);
            }
        }
        Value::Array(values) => values
            .iter()
            .for_each(|value| collect_ids(value, id_keys, out)),
        _ => {}
    }
}

fn push_id(id: &str, out: &mut BTreeSet<String>) {
    // `redact` writes this marker over hidden values; following it would join
    // every event with a redacted key into one trace.
    if id.chars().count() >= MIN_ID_LEN && id != "[REDACTED]" {
        out.insert(id.to_string());
    }
}

#[cfg(test)]
#[path = "trace_tests.rs"]
mod tests;
