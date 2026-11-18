//! Cost tools: `stream_sizes`, `read_costs` and `cap_check`.
//!
//! An embedded store is sized by scanning it; a gateway store asks Core's
//! `/api/v1/streams?with_bytes=true` and `/api/v1/stats`. Read costs exist only
//! in a serving Core, so `read_costs` needs a gateway store.

use std::collections::BTreeMap;

use allsource_core::{
    domain::stream_class::{is_class, stream_class},
    embedded::EmbeddedCore,
    store::{StreamSort, sort_streams},
};
use anyhow::Result;
use serde_json::{Value, json};

use super::{limit_arg, requested_store};
use crate::{
    diagnostics::DiagnosticPolicy,
    remote::{CostReader, gateway_reply, needs_remote},
    stores::{StoreRef, StoreRegistry},
};

pub const DEFAULT_STREAM_LIMIT: usize = 50;
pub const MAX_STREAM_LIMIT: usize = 500;
/// Streams asked of the gateway so class totals cover the tenant; a tenant
/// with more streams gets classes marked incomplete.
const HOSTED_STREAM_SCAN: usize = 10_000;
pub const DEFAULT_READ_COST_LIMIT: usize = 50;
/// Core's own cap on `/api/v1/read-costs`.
pub const MAX_READ_COST_LIMIT: usize = 256;
pub const READ_COST_ORDERS: [&str; 5] = ["bytes", "pages", "matched", "whole", "truncated"];
pub const DEFAULT_THRESHOLD_PCT: u64 = 80;
pub const MAX_CAPS: usize = 256;

/// One stream as either source reports it.
#[derive(Debug, Clone)]
struct StreamRow {
    stream_id: String,
    stream_class: String,
    events: u64,
    bytes: Option<u64>,
    events_last_7d: Option<u64>,
    last_event_at: Option<String>,
}

impl StreamRow {
    fn new(
        stream_id: String,
        events: u64,
        bytes: Option<u64>,
        events_last_7d: Option<u64>,
        last_event_at: Option<String>,
    ) -> Self {
        Self {
            stream_class: stream_class(Some(&stream_id), None, None),
            stream_id,
            events,
            bytes,
            events_last_7d,
            last_event_at,
        }
    }

    fn json(&self) -> Value {
        json!({
            "stream_id": self.stream_id,
            "stream_class": self.stream_class,
            "events": self.events,
            "bytes": self.bytes,
            "events_last_7d": self.events_last_7d,
            "last_event_at": self.last_event_at,
        })
    }
}

#[derive(Debug, Default, Clone)]
struct ClassTotals {
    streams: u64,
    events: u64,
    bytes: u64,
    events_last_7d: u64,
    max_stream_events: u64,
    max_stream_bytes: u64,
}

/// Every stream size the source gave, and why it is partial when it is.
struct Sizes {
    streams: Vec<StreamRow>,
    incomplete: Option<&'static str>,
    tenant: Value,
    store: Value,
}

/// Core saw events removed during its scan and its retry.
const SCAN_INTERRUPTED: &str = "scan_interrupted";
/// The tenant has more streams than the gateway was asked for.
const STREAM_LIMIT_REACHED: &str = "stream_limit_reached";

pub async fn exec_cost_tool(
    stores: &StoreRegistry,
    policy: &DiagnosticPolicy,
    name: &str,
    args: &Value,
) -> Result<Value> {
    let store = stores.resolve(requested_store(policy, args)?)?;
    match name {
        "stream_sizes" => exec_stream_sizes(&store, policy, args).await,
        "cap_check" => exec_cap_check(&store, policy, args).await,
        "read_costs" => exec_read_costs(&store, policy, args).await,
        _ => anyhow::bail!("invalid argument: unknown tool '{name}'"),
    }
}

fn sort_arg(args: &Value) -> Result<StreamSort> {
    match args.get("sort").and_then(Value::as_str).unwrap_or("events") {
        "events" => Ok(StreamSort::Events),
        "bytes" => Ok(StreamSort::Bytes),
        value => anyhow::bail!("invalid argument: sort must be events or bytes; got '{value}'"),
    }
}

fn sort_name(sort: StreamSort) -> &'static str {
    match sort {
        StreamSort::Events => "events",
        StreamSort::Bytes => "bytes",
    }
}

async fn read_sizes(
    store: &StoreRef<'_>,
    policy: &DiagnosticPolicy,
    sort: StreamSort,
) -> Result<Sizes> {
    match store {
        StoreRef::Embedded(store) => {
            let core = store.fresh_core().await;
            Ok(embedded_sizes(core, policy.tenant_id()))
        }
        StoreRef::Remote(remote) => {
            let tenant = policy.tenant_id();
            let streams = gateway_reply(
                remote
                    .streams_with_bytes(tenant, sort_name(sort), HOSTED_STREAM_SCAN)
                    .await?,
                &[],
            )?;
            let stats = gateway_reply(remote.tenant_stats(tenant).await?, &[])?;
            let mut sizes = hosted_sizes(&streams.body, &stats.body)?;
            sizes.store = json!({ "kind": "remote", "url": remote.display_url() });
            Ok(sizes)
        }
    }
}

fn embedded_sizes(core: &EmbeddedCore, tenant: Option<&str>) -> Sizes {
    let sized = core.inner().stream_sizes(tenant);
    let complete = sized.complete;
    let mut infos = sized.streams;
    sort_streams(&mut infos, StreamSort::Events);
    let streams: Vec<StreamRow> = infos
        .into_iter()
        .map(|info| {
            StreamRow::new(
                info.stream_id,
                info.event_count as u64,
                info.estimated_bytes,
                info.events_last_7d.map(|count| count as u64),
                info.last_event_at.map(|at| at.to_rfc3339()),
            )
        })
        .collect();
    let total_events: u64 = streams.iter().map(|row| row.events).sum();
    Sizes {
        tenant: json!({
            "totalEvents": total_events,
            "totalStreams": streams.len(),
        }),
        streams,
        incomplete: (!complete).then_some(SCAN_INTERRUPTED),
        store: json!({ "kind": "embedded" }),
    }
}

fn hosted_sizes(streams: &Value, stats: &Value) -> Result<Sizes> {
    let listed = streams["streams"].as_array().ok_or_else(|| {
        anyhow::anyhow!("gateway answered /api/v1/streams without a streams array")
    })?;
    let rows: Vec<StreamRow> = listed
        .iter()
        .filter_map(|stream| {
            Some(StreamRow::new(
                stream["stream_id"].as_str()?.to_string(),
                stream["event_count"].as_u64().unwrap_or(0),
                stream["estimated_bytes"].as_u64(),
                stream["events_last_7d"].as_u64(),
                stream["last_event_at"].as_str().map(str::to_string),
            ))
        })
        .collect();
    let total = streams["total"].as_u64().unwrap_or(rows.len() as u64);
    let incomplete = if streams["complete"].as_bool() == Some(false) {
        Some(SCAN_INTERRUPTED)
    } else if total > rows.len() as u64 {
        Some(STREAM_LIMIT_REACHED)
    } else {
        None
    };
    Ok(Sizes {
        incomplete,
        tenant: json!({
            "totalEvents": stats.get("total_events"),
            "totalStreams": total,
            "oldestEvent": stats.get("oldest_event"),
            "newestEvent": stats.get("newest_event"),
        }),
        streams: rows,
        store: Value::Null,
    })
}

fn class_totals(streams: &[StreamRow]) -> BTreeMap<String, ClassTotals> {
    let mut classes: BTreeMap<String, ClassTotals> = BTreeMap::new();
    for row in streams {
        let totals = classes.entry(row.stream_class.clone()).or_default();
        totals.streams += 1;
        totals.events += row.events;
        totals.bytes += row.bytes.unwrap_or(0);
        totals.events_last_7d += row.events_last_7d.unwrap_or(0);
        totals.max_stream_events = totals.max_stream_events.max(row.events);
        totals.max_stream_bytes = totals.max_stream_bytes.max(row.bytes.unwrap_or(0));
    }
    classes
}

fn stream_key(sort: StreamSort, events: u64, bytes: u64) -> u64 {
    match sort {
        StreamSort::Events => events,
        StreamSort::Bytes => bytes,
    }
}

fn completeness(incomplete: Option<&str>, scanned: usize) -> Value {
    json!({
        "complete": incomplete.is_none(),
        "reason": incomplete,
        "scanned": scanned,
        "matched": scanned,
        "omitted": 0,
        "unparsedTimestamps": 0,
        "sourcesRequested": 1,
        "sourcesRead": 1,
        "sourcesSkipped": [],
    })
}

async fn exec_stream_sizes(
    store: &StoreRef<'_>,
    policy: &DiagnosticPolicy,
    args: &Value,
) -> Result<Value> {
    let sort = sort_arg(args)?;
    let limit = limit_arg(args, "limit", DEFAULT_STREAM_LIMIT, MAX_STREAM_LIMIT)?;
    let sizes = read_sizes(store, policy, sort).await?;

    let mut streams = sizes.streams.clone();
    streams.sort_by(|a, b| {
        stream_key(sort, b.events, b.bytes.unwrap_or(0))
            .cmp(&stream_key(sort, a.events, a.bytes.unwrap_or(0)))
            .then_with(|| a.stream_id.cmp(&b.stream_id))
    });
    let mut classes: Vec<(String, ClassTotals)> =
        class_totals(&sizes.streams).into_iter().collect();
    classes.sort_by(|(a_name, a), (b_name, b)| {
        stream_key(sort, b.events, b.bytes)
            .cmp(&stream_key(sort, a.events, a.bytes))
            .then_with(|| a_name.cmp(b_name))
    });
    let class_rows: Vec<Value> = classes
        .iter()
        .take(limit)
        .map(|(name, totals)| {
            json!({
                "stream_class": name,
                "streams": totals.streams,
                "events": totals.events,
                "bytes": totals.bytes,
                "events_last_7d": totals.events_last_7d,
                "max_stream_events": totals.max_stream_events,
                "max_stream_bytes": totals.max_stream_bytes,
            })
        })
        .collect();
    let stream_rows: Vec<Value> = streams.iter().take(limit).map(StreamRow::json).collect();

    let mut context = policy.context(None);
    context["store"] = sizes.store;
    Ok(json!({
        "context": context,
        "sort": sort_name(sort),
        "streams": stream_rows,
        "classes": class_rows,
        "tenant": sizes.tenant,
        "page": {
            "requestedLimit": limit,
            "returned": stream_rows.len(),
            "totalCount": sizes.streams.len(),
            "nextCursor": Value::Null,
        },
        "completeness": completeness(sizes.incomplete, sizes.streams.len()),
    }))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Measure {
    LargestStream,
    ClassTotal,
}

impl Measure {
    fn name(self) -> &'static str {
        match self {
            Self::LargestStream => "largest_stream",
            Self::ClassTotal => "class_total",
        }
    }
}

struct Cap {
    stream_class: String,
    events: u64,
    measure: Measure,
}

fn caps_arg(args: &Value) -> Result<Vec<Cap>> {
    let caps = args
        .get("caps")
        .and_then(Value::as_array)
        .filter(|caps| !caps.is_empty())
        .ok_or_else(|| anyhow::anyhow!("invalid argument: caps must be a non-empty array"))?;
    if caps.len() > MAX_CAPS {
        anyhow::bail!("invalid argument: at most {MAX_CAPS} caps");
    }
    caps.iter()
        .map(|cap| {
            let class = cap["stream_class"].as_str().unwrap_or_default();
            if !is_class(class) {
                anyhow::bail!(
                    "invalid argument: stream_class must match ^[a-z][a-z0-9_.]{{0,63}}$; got '{class}'"
                );
            }
            let cap_events = cap["cap_events"]
                .as_u64()
                .filter(|events| *events > 0)
                .ok_or_else(|| {
                    anyhow::anyhow!("invalid argument: cap_events must be a positive integer for '{class}'")
                })?;
            let measure = match cap.get("measure").and_then(Value::as_str) {
                None | Some("largest_stream") => Measure::LargestStream,
                Some("class_total") => Measure::ClassTotal,
                Some(other) => anyhow::bail!(
                    "invalid argument: measure must be largest_stream or class_total; got '{other}'"
                ),
            };
            Ok(Cap {
                stream_class: class.to_string(),
                events: cap_events,
                measure,
            })
        })
        .collect()
}

fn threshold_arg(args: &Value) -> Result<u64> {
    match args.get("threshold_pct") {
        None | Some(Value::Null) => Ok(DEFAULT_THRESHOLD_PCT),
        Some(value) => value
            .as_u64()
            .filter(|pct| (1..=100).contains(pct))
            .ok_or_else(|| anyhow::anyhow!("invalid argument: threshold_pct must be 1..100")),
    }
}

async fn exec_cap_check(
    store: &StoreRef<'_>,
    policy: &DiagnosticPolicy,
    args: &Value,
) -> Result<Value> {
    let caps = caps_arg(args)?;
    let threshold = threshold_arg(args)?;
    let sizes = read_sizes(store, policy, StreamSort::Events).await?;
    let classes = class_totals(&sizes.streams);

    let mut over = Vec::new();
    let mut under = Vec::new();
    let mut unmeasured = Vec::new();
    for cap in &caps {
        let Some(totals) = classes.get(&cap.stream_class) else {
            unmeasured.push(json!(cap.stream_class));
            continue;
        };
        let measured = match cap.measure {
            Measure::LargestStream => totals.max_stream_events,
            Measure::ClassTotal => totals.events,
        };
        let row = json!({
            "stream_class": cap.stream_class,
            "measure": cap.measure.name(),
            "cap_events": cap.events,
            "measured_events": measured,
            "pct": measured.saturating_mul(100) / cap.events,
        });
        if measured.saturating_mul(100) >= cap.events.saturating_mul(threshold) {
            over.push(row);
        } else {
            under.push(row);
        }
    }

    let mut context = policy.context(None);
    context["store"] = sizes.store;
    Ok(json!({
        "context": context,
        "thresholdPct": threshold,
        "over": over,
        "under": under,
        "unmeasured": unmeasured,
        "completeness": completeness(sizes.incomplete, sizes.streams.len()),
    }))
}

fn sum_field(rows: &[Value], field: &str) -> u64 {
    rows.iter().filter_map(|row| row[field].as_u64()).sum()
}

async fn exec_read_costs(
    store: &StoreRef<'_>,
    policy: &DiagnosticPolicy,
    args: &Value,
) -> Result<Value> {
    let StoreRef::Remote(remote) = store else {
        return Err(needs_remote("read_costs"));
    };
    let limit = limit_arg(args, "limit", DEFAULT_READ_COST_LIMIT, MAX_READ_COST_LIMIT)?;
    let order = args.get("order").and_then(Value::as_str).unwrap_or("bytes");
    if !READ_COST_ORDERS.contains(&order) {
        anyhow::bail!(
            "invalid argument: order must be one of {}; got '{order}'",
            READ_COST_ORDERS.join(", ")
        );
    }

    let reply = gateway_reply(
        remote.read_costs(policy.tenant_id(), order, limit).await?,
        &[],
    )?;
    let callers = reply.body["callers"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let classes = reply.body["classes"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let max_pages = classes
        .iter()
        .filter_map(|row| row["max_pages_per_walk"].as_u64())
        .max()
        .unwrap_or(0);
    let all_classes = classes.len() < limit;

    let mut context = policy.context(None);
    context["store"] = json!({ "kind": "remote", "url": remote.display_url() });
    Ok(json!({
        "context": context,
        "order": order,
        "truncated": {
            "single": sum_field(&classes, "single_truncated"),
            "walk_abandoned": sum_field(&classes, "walk_abandoned"),
            "walk_evicted": sum_field(&classes, "walk_evicted"),
        },
        "maxPagesPerWalk": max_pages,
        "totalsCover": if all_classes { "all_class_rows" } else { "returned_class_rows" },
        "callers": callers,
        "classes": classes,
        "page": {
            "requestedLimit": limit,
            "returned": callers.len().max(classes.len()),
            "totalCount": Value::Null,
            "nextCursor": Value::Null,
        },
        "completeness": {
            "complete": callers.len() < limit && classes.len() < limit,
            "reason": if callers.len() < limit && classes.len() < limit { Value::Null } else { json!("limit_reached") },
            "scanned": callers.len() + classes.len(),
            "matched": callers.len() + classes.len(),
            "omitted": 0,
            "unparsedTimestamps": 0,
            "sourcesRequested": 1,
            "sourcesRead": 1,
            "sourcesSkipped": [],
        },
    }))
}

#[cfg(test)]
#[path = "stream_sizes_tests.rs"]
mod tests;
