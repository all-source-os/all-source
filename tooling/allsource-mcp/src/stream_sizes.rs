//! Cost tools: `stream_sizes`, `read_costs` and `cap_check`.
//!
//! An embedded store is sized by scanning it; a gateway store asks Core's
//! `/api/v1/streams?with_bytes=true` and `/api/v1/stats`. Read costs exist only
//! in a serving Core, so `read_costs` needs a gateway store.

use std::collections::BTreeMap;

use allsource_core::{
    domain::stream_class::{is_class, stream_class_for_tenant},
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
        tenant: Option<&str>,
        stream_id: String,
        events: u64,
        bytes: Option<u64>,
        events_last_7d: Option<u64>,
        last_event_at: Option<String>,
    ) -> Self {
        Self {
            stream_class: stream_class_for_tenant(tenant, Some(&stream_id), None, None),
            stream_id,
            events,
            bytes,
            events_last_7d,
            last_event_at,
        }
    }

    /// Carries both fields only `with_bytes` produces.
    fn sized(&self) -> bool {
        self.bytes.is_some() && self.events_last_7d.is_some()
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
    /// The tenant's stream count, which exceeds `streams` when the gateway
    /// listing was cut.
    total: u64,
    incomplete: Option<&'static str>,
    /// Every stream carries its bytes and seven-day count, so they may be
    /// summed and ranked.
    bytes_known: bool,
    tenant: Value,
    store: Value,
}

/// Core saw events removed during its scan and its retry, or the tenant left
/// its cache during the scan.
const SCAN_INTERRUPTED: &str = "scan_interrupted";
/// The tenant has more streams than the gateway was asked for.
const STREAM_LIMIT_REACHED: &str = "stream_limit_reached";
/// Core sent no `complete`, or a stream without `estimated_bytes`: a Core
/// older than 0.30 ignores `with_bytes`.
const BYTES_UNAVAILABLE: &str = "bytes_unavailable";
/// Core listed nothing because the request named no tenant.
const NO_TENANT: &str = "no_tenant";
/// Read-cost recording is off in Core.
const RECORDING_OFF: &str = "recording_off";
/// Core holds no rows for the tenant: no reads since `since`, or pushed out
/// of its 128-tenant table.
const TENANT_NOT_TRACKED: &str = "tenant_not_tracked";
/// Core sent neither `enabled` nor `tenant_tracked`: it predates them.
const RECORDING_STATE_UNKNOWN: &str = "recording_state_unknown";
/// The embedded store's last refresh failed, so it serves what it held before.
const REFRESH_FAILED: &str = "refresh_failed";
/// A caller row inherited a replaced row's counts or a body of unknown length.
const ROWS_APPROXIMATE: &str = "rows_approximate";
/// Core's class table was full, so later classes count under `other`.
const CLASSES_OVERFLOWED: &str = "classes_overflowed";
/// The tenant's rows start after `since`: Core had pushed a tenant out of its
/// table by the time this one entered, so earlier reads may be missing.
const TENANT_REENTERED: &str = "tenant_reentered";
const LIMIT_REACHED: &str = "limit_reached";

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
            let mut sizes = embedded_sizes(core, policy.tenant_id()).await?;
            sizes.store = store.refresh_context().await;
            sizes.store["kind"] = json!("embedded");
            if sizes.store["lastRefreshError"] != json!(false) {
                sizes.incomplete = Some(REFRESH_FAILED);
            }
            Ok(sizes)
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
            let mut sizes = hosted_sizes(tenant, &streams.body, &stats.body)?;
            sizes.store = json!({ "kind": "remote", "url": remote.display_url() });
            Ok(sizes)
        }
    }
}

/// Scans on a blocking thread: the scan walks every event of the store.
async fn embedded_sizes(core: &EmbeddedCore, tenant: Option<&str>) -> Result<Sizes> {
    let store = core.inner();
    let owned_tenant = tenant.map(str::to_owned);
    let sized = tokio::task::spawn_blocking(move || store.stream_sizes(owned_tenant.as_deref()))
        .await
        .map_err(|err| anyhow::anyhow!("stream size scan failed: {err}"))?
        .map_err(|err| anyhow::anyhow!("store unavailable: {err}"))?;
    let complete = sized.complete;
    let mut infos = sized.streams;
    sort_streams(&mut infos, StreamSort::Events);
    let streams: Vec<StreamRow> = infos
        .into_iter()
        .map(|info| {
            StreamRow::new(
                tenant,
                info.stream_id,
                info.event_count as u64,
                info.estimated_bytes,
                info.events_last_7d.map(|count| count as u64),
                info.last_event_at.map(|at| at.to_rfc3339()),
            )
        })
        .collect();
    let total_events: u64 = streams.iter().map(|row| row.events).sum();
    let bytes_known = streams.iter().all(StreamRow::sized);
    Ok(Sizes {
        tenant: json!({
            "totalEvents": total_events,
            "totalStreams": streams.len(),
        }),
        total: streams.len() as u64,
        streams,
        incomplete: (!complete).then_some(SCAN_INTERRUPTED),
        bytes_known,
        store: json!({ "kind": "embedded" }),
    })
}

fn hosted_sizes(tenant: Option<&str>, streams: &Value, stats: &Value) -> Result<Sizes> {
    let listed = streams["streams"].as_array().ok_or_else(|| {
        anyhow::anyhow!("gateway answered /api/v1/streams without a streams array")
    })?;
    let rows = listed
        .iter()
        .map(|stream| {
            let stream_id = stream["stream_id"].as_str().ok_or_else(|| {
                anyhow::anyhow!("gateway answered /api/v1/streams with a row without stream_id")
            })?;
            let events = stream["event_count"].as_u64().ok_or_else(|| {
                anyhow::anyhow!("gateway answered /api/v1/streams with a row without event_count")
            })?;
            Ok(StreamRow::new(
                tenant,
                stream_id.to_string(),
                events,
                stream["estimated_bytes"].as_u64(),
                stream["events_last_7d"].as_u64(),
                stream["last_event_at"].as_str().map(str::to_string),
            ))
        })
        .collect::<Result<Vec<StreamRow>>>()?;
    let total = streams["total"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("gateway answered /api/v1/streams without a total"))?;
    let bytes_known = rows.iter().all(StreamRow::sized);
    let incomplete = match streams["complete"].as_bool() {
        None => Some(BYTES_UNAVAILABLE),
        Some(_) if !bytes_known => Some(BYTES_UNAVAILABLE),
        Some(false) if streams["reason"] == NO_TENANT => Some(NO_TENANT),
        Some(false) => Some(SCAN_INTERRUPTED),
        Some(true) if total > rows.len() as u64 => Some(STREAM_LIMIT_REACHED),
        Some(true) => None,
    };
    Ok(Sizes {
        total,
        incomplete,
        bytes_known,
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

/// The byte and seven-day sums are meaningful only when `Sizes::bytes_known`;
/// `exec_stream_sizes` reports them as null otherwise.
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

/// `scanned` is the streams the source sent; `total` the tenant's streams.
fn completeness(incomplete: Option<&str>, scanned: usize, total: u64) -> Value {
    json!({
        "complete": incomplete.is_none(),
        "reason": incomplete,
        "scanned": scanned,
        "matched": total,
        "omitted": total.saturating_sub(scanned as u64),
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
    if sort == StreamSort::Bytes && !sizes.bytes_known {
        anyhow::bail!(
            "unsupported: stream_sizes cannot rank by bytes: Core sent a stream without estimated_bytes (a Core older than 0.30 ignores with_bytes); sort by events instead"
        );
    }
    let bytes = |value: u64| {
        if sizes.bytes_known {
            json!(value)
        } else {
            Value::Null
        }
    };

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
                "bytes": bytes(totals.bytes),
                "events_last_7d": bytes(totals.events_last_7d),
                "max_stream_events": totals.max_stream_events,
                "max_stream_bytes": bytes(totals.max_stream_bytes),
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
            "totalCount": sizes.total,
            "nextCursor": Value::Null,
        },
        "completeness": completeness(sizes.incomplete, sizes.streams.len(), sizes.total),
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
        "completeness": completeness(sizes.incomplete, sizes.streams.len(), sizes.total),
    }))
}

/// Every class row's `field`; a row without it is an error, never a zero.
fn class_field(rows: &[Value], field: &str) -> Result<Vec<u64>> {
    rows.iter()
        .map(|row| {
            row[field].as_u64().ok_or_else(|| {
                anyhow::anyhow!(
                    "gateway answered /api/v1/read-costs with a class row without {field}"
                )
            })
        })
        .collect()
}

/// Whether a tracked tenant's rows start after `since`; `None` when either
/// timestamp is missing or unreadable.
fn tenant_reentered(body: &Value) -> Option<bool> {
    let at = |field: &str| {
        body[field]
            .as_str()
            .and_then(|text| chrono::DateTime::parse_from_rfc3339(text).ok())
    };
    Some(at("tenant_since")? > at("since")?)
}

fn sum_field(rows: &[Value], field: &str) -> Result<u64> {
    Ok(class_field(rows, field)?.into_iter().sum())
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
    let callers = reply.body["callers"].as_array().cloned().ok_or_else(|| {
        anyhow::anyhow!("gateway answered /api/v1/read-costs without a callers array")
    })?;
    let classes = reply.body["classes"].as_array().cloned().ok_or_else(|| {
        anyhow::anyhow!("gateway answered /api/v1/read-costs without a classes array")
    })?;
    let at_limit = callers.len() >= limit || classes.len() >= limit;
    let approximate = callers
        .iter()
        .map(|row| {
            row["approximate"].as_bool().ok_or_else(|| {
                anyhow::anyhow!(
                    "gateway answered /api/v1/read-costs with a caller row without approximate"
                )
            })
        })
        .collect::<Result<Vec<bool>>>()?
        .contains(&true);
    let overflowed = classes.iter().any(|row| row["stream_class"] == "other");
    let incomplete = match (
        reply.body["enabled"].as_bool(),
        reply.body["tenant_tracked"].as_bool(),
    ) {
        (Some(true), Some(true)) => match tenant_reentered(&reply.body) {
            None => Some(RECORDING_STATE_UNKNOWN),
            Some(_) if approximate => Some(ROWS_APPROXIMATE),
            Some(_) if overflowed => Some(CLASSES_OVERFLOWED),
            Some(true) => Some(TENANT_REENTERED),
            Some(false) if at_limit => Some(LIMIT_REACHED),
            Some(false) => None,
        },
        (Some(false), _) => Some(RECORDING_OFF),
        (Some(true), Some(false)) => Some(TENANT_NOT_TRACKED),
        _ => Some(RECORDING_STATE_UNKNOWN),
    };
    let max_pages = class_field(&classes, "max_pages_per_walk")?
        .into_iter()
        .max()
        .unwrap_or(0);
    let truncated = json!({
        "single": sum_field(&classes, "single_truncated")?,
        "walk_abandoned": sum_field(&classes, "walk_abandoned")?,
        "walk_evicted": sum_field(&classes, "walk_evicted")?,
    });
    let all_classes = classes.len() < limit;

    let mut context = policy.context(None);
    context["store"] = json!({ "kind": "remote", "url": remote.display_url() });
    Ok(json!({
        "context": context,
        "order": order,
        "truncated": truncated,
        "maxPagesPerWalk": max_pages,
        "totalsCover": if all_classes { "all_class_rows" } else { "returned_class_rows" },
        "recording": {
            "enabled": reply.body.get("enabled"),
            "instanceId": reply.body.get("instance_id"),
            "since": reply.body.get("since"),
            "tenantTracked": reply.body.get("tenant_tracked"),
            "tenantSince": reply.body.get("tenant_since"),
        },
        "callers": callers,
        "classes": classes,
        "page": {
            "requestedLimit": limit,
            "returned": callers.len().max(classes.len()),
            "totalCount": Value::Null,
            "nextCursor": Value::Null,
        },
        "completeness": {
            "complete": incomplete.is_none(),
            "reason": incomplete,
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
