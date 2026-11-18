//! Read cost of `GET /api/v1/events/query`: per-request facts, walks and
//! aggregates. Design sections 6.3 and 10.4.
//!
//! The query handler attaches [`ReadCostFacts`] to its response. [`record`]
//! sits inside [`super::response_limit::bounded`], so it measures a body over
//! the 16 MiB bound before the bound replaces it. Nothing is persisted.

use crate::application::dto::QueryEventsRequest;
use crate::domain::entities::egress::MAX_RESPONSE_BYTES;
use crate::domain::stream_class::{is_class, stream_class};
use crate::infrastructure::observability::metrics::MetricsRegistry;
use crate::infrastructure::security::middleware::Admin;
use axum::{
    Json,
    body::HttpBody,
    extract::{Extension, Query, Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

const CALLER_HEADER: &str = "x-allsource-caller";
const READ_ID_HEADER: &str = "x-allsource-read-id";
const INVALID: &str = "invalid";
const OTHER: &str = "other";

const IN_FLIGHT_CAP: usize = 4_096;
const WALKS_PER_TENANT: usize = 256;
const WALK_IDLE: Duration = Duration::from_secs(30);
const SWEEP_EVERY: Duration = Duration::from_secs(10);
const CLASSES_PER_TENANT: usize = 128;
const CALLERS_PER_TENANT: usize = 256;
const TENANTS: usize = 128;
const LABEL_CAP: usize = 256;
const THRESHOLD_EVERY: Duration = Duration::from_mins(1);
const THRESHOLD_PAGES: u64 = 20;
const THRESHOLD_MATCHED: usize = 50_000;

/// Which requests a read is: the shape decides what counts as a whole read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Whole,
    Latest,
    CountProbe,
    Partial,
}

impl Shape {
    pub fn of(served_limit: usize, descending: bool, since_set: bool, entity_set: bool) -> Self {
        if served_limit == 0 {
            Self::CountProbe
        } else if descending && served_limit == 1 {
            Self::Latest
        } else if since_set || entity_set {
            Self::Partial
        } else {
            Self::Whole
        }
    }
}

/// What one query page answered, as the handler saw it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueryCounts {
    pub matched: usize,
    pub returned: usize,
    pub served_limit: usize,
    pub offset: usize,
    pub has_more: bool,
    pub since_set: bool,
    pub entity_set: bool,
    pub shape: Shape,
}

/// The read a page belongs to, before any header is considered.
#[derive(Debug, Clone)]
pub struct ReadFacts {
    pub tenant: String,
    pub stream_class: String,
    pub counts: QueryCounts,
}

/// What a query request asked for, fixed before the store answers.
#[derive(Debug, Clone)]
pub struct ReadScope {
    class: String,
    since_set: bool,
    entity_set: bool,
    served_limit: usize,
    offset: usize,
    descending: bool,
}

impl ReadScope {
    pub fn of(req: &QueryEventsRequest, offset: usize, descending: bool, max_page: usize) -> Self {
        Self {
            class: stream_class(
                req.entity_id.as_deref(),
                req.event_type_prefix.as_deref(),
                req.event_type.as_deref(),
            ),
            since_set: req.since.is_some(),
            entity_set: req.entity_id.is_some(),
            served_limit: req.limit.unwrap_or(max_page).min(max_page),
            offset,
            descending,
        }
    }

    /// Facts for a page that matched `matched` events and returned `returned`.
    pub fn facts(
        self,
        tenant: String,
        matched: usize,
        returned: usize,
        has_more: bool,
    ) -> ReadFacts {
        let shape = Shape::of(
            self.served_limit,
            self.descending,
            self.since_set,
            self.entity_set,
        );
        ReadFacts {
            tenant,
            stream_class: self.class,
            counts: QueryCounts {
                matched,
                returned,
                served_limit: self.served_limit,
                offset: self.offset,
                has_more,
                since_set: self.since_set,
                entity_set: self.entity_set,
                shape,
            },
        }
    }
}

/// A header that failed its pattern, kept by name and length only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RejectedHeader {
    pub name: &'static str,
    pub length: usize,
}

/// Attached to a query response as an extension. Removed by [`record`].
#[derive(Debug, Clone)]
pub struct ReadCostFacts {
    pub read: ReadFacts,
    /// `Some("invalid")` when the header is present and fails its pattern.
    pub caller: Option<String>,
    /// `None` when absent or invalid: an invalid read id counts as absent.
    pub read_id: Option<String>,
    pub rejected: Vec<RejectedHeader>,
}

impl ReadCostFacts {
    pub fn from_headers(read: ReadFacts, headers: &HeaderMap) -> Self {
        let mut rejected = Vec::new();
        let caller = headers.get(CALLER_HEADER).map(|value| {
            let text = value.to_str().unwrap_or_default();
            if is_class(text) {
                text.to_owned()
            } else {
                rejected.push(RejectedHeader {
                    name: "caller",
                    length: value.as_bytes().len(),
                });
                INVALID.to_owned()
            }
        });
        let read_id = headers.get(READ_ID_HEADER).and_then(|value| {
            let text = value.to_str().ok().filter(|text| is_read_id(text));
            if text.is_none() {
                rejected.push(RejectedHeader {
                    name: "read_id",
                    length: value.as_bytes().len(),
                });
            }
            text.map(str::to_owned)
        });
        Self {
            read,
            caller,
            read_id,
            rejected,
        }
    }
}

fn is_read_id(text: &str) -> bool {
    text.len() == 16 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// How a page ended, from its status and final body length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Complete,
    HasMore,
    NotModified,
    TooLarge,
}

impl Outcome {
    pub fn of(status: StatusCode, body_bytes: u64, has_more: bool) -> Self {
        if body_bytes > MAX_RESPONSE_BYTES {
            Self::TooLarge
        } else if status == StatusCode::NOT_MODIFIED {
            Self::NotModified
        } else if has_more {
            Self::HasMore
        } else {
            Self::Complete
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::HasMore => "has_more",
            Self::NotModified => "not_modified",
            Self::TooLarge => "too_large",
        }
    }
}

/// Exact counts for one stream class within one tenant.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ClassRow {
    pub logical_reads: u64,
    pub pages: u64,
    pub whole_reads: u64,
    pub single_truncated: u64,
    pub walk_abandoned: u64,
    pub walk_evicted: u64,
    pub matched_sum: u64,
    pub matched_max: u64,
    pub max_pages: u64,
}

/// Counts for one caller and stream class within one tenant. A row created by
/// replacing another inherits that row's counts and is `approximate`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CallerRow {
    pub logical_reads: u64,
    pub pages: u64,
    pub whole_reads: u64,
    pub body_bytes: u64,
    pub single_truncated: u64,
    pub walk_abandoned: u64,
    pub approximate: bool,
}

#[derive(Debug)]
struct Walk {
    class: String,
    caller: Option<String>,
    pages: u64,
    last_seen: Instant,
}

#[derive(Debug, Default)]
struct Tenant {
    last_active: u64,
    classes: HashMap<String, ClassRow>,
    callers: HashMap<(Option<String>, String), CallerRow>,
}

#[derive(Debug, Default)]
struct Tables {
    tick: u64,
    tenants: HashMap<String, Tenant>,
    walks: HashMap<(String, String), Walk>,
    labels: HashSet<String>,
    label_overflow_logged: bool,
    last_sweep: Option<Instant>,
    threshold_logged: HashMap<(String, String, String), Instant>,
    header_logged: HashMap<&'static str, Instant>,
}

/// Process-wide read-cost state, shared by the query handler's layer and the
/// admin switch.
pub struct ReadCost {
    enabled: AtomicBool,
    metrics: Arc<MetricsRegistry>,
    tables: Mutex<Tables>,
}

impl ReadCost {
    pub fn new(metrics: Arc<MetricsRegistry>, enabled: bool) -> Arc<Self> {
        Arc::new(Self {
            enabled: AtomicBool::new(enabled),
            metrics,
            tables: Mutex::new(Tables::default()),
        })
    }

    /// Enabled unless `ALLSOURCE_READ_COST=off` at boot.
    pub fn from_env(metrics: Arc<MetricsRegistry>) -> Arc<Self> {
        let disabled = std::env::var("ALLSOURCE_READ_COST")
            .is_ok_and(|value| value.eq_ignore_ascii_case("off"));
        Self::new(metrics, !disabled)
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    /// Caller and class rows for one tenant, ordered by `order` and cut to `limit`.
    pub fn read_costs(&self, tenant: &str, order: RowOrder, limit: usize) -> ReadCostsResponse {
        let tables = self.lock();
        let Some(state) = tables.tenants.get(tenant) else {
            return ReadCostsResponse::default();
        };
        let mut callers: Vec<CallerCost> = state
            .callers
            .iter()
            .map(|((caller, class), counts)| CallerCost {
                caller: caller.clone().unwrap_or_else(|| UNATTRIBUTED.to_owned()),
                stream_class: class.clone(),
                counts: *counts,
            })
            .collect();
        callers.sort_by(|a, b| {
            caller_key(order, &b.counts)
                .cmp(&caller_key(order, &a.counts))
                .then_with(|| (&a.caller, &a.stream_class).cmp(&(&b.caller, &b.stream_class)))
        });
        callers.truncate(limit);
        let mut classes: Vec<ClassCost> = state
            .classes
            .iter()
            .map(|(class, counts)| ClassCost {
                stream_class: class.clone(),
                counts: *counts,
            })
            .collect();
        classes.sort_by(|a, b| {
            class_key(order, &b.counts)
                .cmp(&class_key(order, &a.counts))
                .then_with(|| a.stream_class.cmp(&b.stream_class))
        });
        classes.truncate(limit);
        ReadCostsResponse { callers, classes }
    }

    fn lock(&self) -> MutexGuard<'_, Tables> {
        self.tables.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Folds one page into the walks, aggregates, series and logs.
    pub fn observe(&self, now: Instant, facts: &ReadCostFacts, outcome: Outcome, body_bytes: u64) {
        let mut tables = self.lock();
        self.sweep_if_due(&mut tables, now);
        log_rejected(&mut tables, now, &facts.rejected);

        let read = &facts.read;
        let counts = read.counts;
        let tenant = read.tenant.as_str();
        let class = read.stream_class.as_str();
        let caller = facts.caller.as_deref();
        let who = Who {
            tenant,
            caller,
            class,
        };
        let whole = counts.shape == Shape::Whole;
        let single = is_single_truncated(facts);

        let mut logical = true;
        let mut pages = 1;
        if let Some(read_id) = facts.read_id.as_deref() {
            let key = (tenant.to_owned(), read_id.to_owned());
            let idle = tables
                .walks
                .get(&key)
                .is_some_and(|walk| now.saturating_duration_since(walk.last_seen) >= WALK_IDLE);
            if idle && let Some(walk) = tables.walks.remove(&key) {
                self.abandon(&mut tables, now, tenant, &walk);
            }
            if let Some(mut walk) = tables.walks.remove(&key) {
                logical = false;
                pages = walk.pages + 1;
                if counts.has_more {
                    walk.pages = pages;
                    walk.last_seen = now;
                    tables.walks.insert(key, walk);
                }
            } else if counts.has_more {
                self.make_room(&mut tables, tenant);
                tables.walks.insert(
                    key,
                    Walk {
                        class: class.to_owned(),
                        caller: caller.map(str::to_owned),
                        pages: 1,
                        last_seen: now,
                    },
                );
            }
        }

        let label = self.admit_label(&mut tables, class);
        let state = tenant_mut(&mut tables, tenant);
        let row = class_row(state, class);
        row.pages += 1;
        row.logical_reads += u64::from(logical);
        row.whole_reads += u64::from(whole);
        row.single_truncated += u64::from(single);
        row.matched_sum += wide(counts.matched);
        row.matched_max = row.matched_max.max(wide(counts.matched));
        row.max_pages = row.max_pages.max(pages);
        let row = caller_row(state, caller.map(str::to_owned), class);
        row.pages += 1;
        row.logical_reads += u64::from(logical);
        row.whole_reads += u64::from(whole);
        row.single_truncated += u64::from(single);
        row.body_bytes += body_bytes;

        self.metrics
            .read_pages_total
            .with_label_values(&[label.as_str(), outcome.as_str()])
            .inc();
        self.metrics
            .read_events_matched_total
            .with_label_values(&[label.as_str()])
            .inc_by(wide(counts.matched));
        if whole {
            self.metrics
                .read_whole_reads_total
                .with_label_values(&[label.as_str()])
                .inc();
        }
        if single {
            self.truncated(&label, "single");
        }

        if pages > THRESHOLD_PAGES {
            threshold(&mut tables, now, who, "walk_pages", pages, counts.matched);
        }
        if counts.matched > THRESHOLD_MATCHED {
            threshold(
                &mut tables,
                now,
                who,
                "matched_pages",
                pages,
                counts.matched,
            );
        }
        if single && caller.is_some() {
            threshold(
                &mut tables,
                now,
                who,
                "single_truncated",
                pages,
                counts.matched,
            );
        }
    }

    /// Ends every walk idle for [`WALK_IDLE`] as abandoned.
    pub fn sweep(&self, now: Instant) {
        let mut tables = self.lock();
        self.sweep_locked(&mut tables, now);
    }

    fn sweep_if_due(&self, tables: &mut Tables, now: Instant) {
        if tables
            .last_sweep
            .is_some_and(|last| now.saturating_duration_since(last) < SWEEP_EVERY)
        {
            return;
        }
        self.sweep_locked(tables, now);
    }

    fn sweep_locked(&self, tables: &mut Tables, now: Instant) {
        tables.last_sweep = Some(now);
        let idle: Vec<(String, String)> = tables
            .walks
            .iter()
            .filter(|(_, walk)| now.saturating_duration_since(walk.last_seen) >= WALK_IDLE)
            .map(|(key, _)| key.clone())
            .collect();
        for key in idle {
            if let Some(walk) = tables.walks.remove(&key) {
                self.abandon(tables, now, &key.0, &walk);
            }
        }
    }

    fn abandon(&self, tables: &mut Tables, now: Instant, tenant: &str, walk: &Walk) {
        let label = self.admit_label(tables, &walk.class);
        let state = tenant_mut(tables, tenant);
        class_row(state, &walk.class).walk_abandoned += 1;
        caller_row(state, walk.caller.clone(), &walk.class).walk_abandoned += 1;
        self.truncated(&label, "walk_abandoned");
        let who = Who {
            tenant,
            caller: walk.caller.as_deref(),
            class: &walk.class,
        };
        threshold(tables, now, who, "walk_abandoned", walk.pages, 0);
    }

    /// Evicts `tenant`'s least recently seen walk when it holds
    /// [`WALKS_PER_TENANT`], else the table's least recently seen walk when
    /// the table is full, so one tenant cannot push out another's walks. An
    /// evicted walk is `walk_evicted`, never `walk_abandoned`.
    fn make_room(&self, tables: &mut Tables, tenant: &str) {
        let own = tables.walks.keys().filter(|(t, _)| t == tenant).count();
        let oldest = if own >= WALKS_PER_TENANT {
            tables
                .walks
                .iter()
                .filter(|((t, _), _)| t == tenant)
                .min_by_key(|(_, walk)| walk.last_seen)
                .map(|(key, _)| key.clone())
        } else if tables.walks.len() >= IN_FLIGHT_CAP {
            tables
                .walks
                .iter()
                .min_by_key(|(_, walk)| walk.last_seen)
                .map(|(key, _)| key.clone())
        } else {
            None
        };
        let Some(key) = oldest else { return };
        if let Some(walk) = tables.walks.remove(&key) {
            let label = self.admit_label(tables, &walk.class);
            class_row(tenant_mut(tables, &key.0), &walk.class).walk_evicted += 1;
            self.truncated(&label, "walk_evicted");
        }
    }

    /// The Prometheus label for `class`: the class itself while the process
    /// has room for it, else `other`.
    fn admit_label(&self, tables: &mut Tables, class: &str) -> String {
        if tables.labels.contains(class) {
            return class.to_owned();
        }
        if tables.labels.len() < LABEL_CAP {
            tables.labels.insert(class.to_owned());
            return class.to_owned();
        }
        self.metrics
            .read_label_overflow_total
            .with_label_values(&["stream_class"])
            .inc();
        if !tables.label_overflow_logged {
            tables.label_overflow_logged = true;
            tracing::warn!(event = "read_label_overflow", label = "stream_class");
        }
        OTHER.to_owned()
    }

    fn truncated(&self, label: &str, kind: &str) {
        self.metrics
            .read_truncated_total
            .with_label_values(&[label, kind])
            .inc();
    }
}

/// A walk page's tenant, caller and stream class, for log keys.
#[derive(Clone, Copy)]
struct Who<'a> {
    tenant: &'a str,
    caller: Option<&'a str>,
    class: &'a str,
}

/// Single truncated per design 4: no read id, no since, no entity, offset 0,
/// a served limit of two or more, and a full page with more behind it.
fn is_single_truncated(facts: &ReadCostFacts) -> bool {
    let counts = facts.read.counts;
    facts.read_id.is_none()
        && !counts.since_set
        && !counts.entity_set
        && counts.offset == 0
        && counts.served_limit >= 2
        && counts.returned == counts.served_limit
        && counts.has_more
}

fn tenant_mut<'a>(tables: &'a mut Tables, tenant: &str) -> &'a mut Tenant {
    tables.tick += 1;
    let tick = tables.tick;
    if !tables.tenants.contains_key(tenant)
        && tables.tenants.len() >= TENANTS
        && let Some(coldest) = tables
            .tenants
            .iter()
            .min_by_key(|(_, state)| state.last_active)
            .map(|(name, _)| name.clone())
    {
        tables.tenants.remove(&coldest);
    }
    let state = tables.tenants.entry(tenant.to_owned()).or_default();
    state.last_active = tick;
    state
}

fn class_row<'a>(state: &'a mut Tenant, class: &str) -> &'a mut ClassRow {
    let key = if state.classes.contains_key(class) || state.classes.len() < CLASSES_PER_TENANT {
        class
    } else {
        OTHER
    };
    state.classes.entry(key.to_owned()).or_default()
}

fn caller_row<'a>(state: &'a mut Tenant, caller: Option<String>, class: &str) -> &'a mut CallerRow {
    let key = (caller, class.to_owned());
    if !state.callers.contains_key(&key) && state.callers.len() >= CALLERS_PER_TENANT {
        let coldest = state
            .callers
            .iter()
            .min_by_key(|(_, row)| row.body_bytes)
            .map(|(k, row)| (k.clone(), *row));
        if let Some((victim, row)) = coldest {
            state.callers.remove(&victim);
            state.callers.insert(
                key.clone(),
                CallerRow {
                    approximate: true,
                    ..row
                },
            );
        }
    }
    state.callers.entry(key).or_default()
}

fn threshold(
    tables: &mut Tables,
    now: Instant,
    who: Who<'_>,
    reason: &'static str,
    pages: u64,
    matched: usize,
) {
    let key = (
        fingerprint(who.tenant),
        who.caller.unwrap_or("none").to_owned(),
        who.class.to_owned(),
    );
    if tables
        .threshold_logged
        .get(&key)
        .is_some_and(|last| now.saturating_duration_since(*last) < THRESHOLD_EVERY)
    {
        return;
    }
    if tables.threshold_logged.len() >= 4_096 {
        tables
            .threshold_logged
            .retain(|_, last| now.saturating_duration_since(*last) < THRESHOLD_EVERY);
    }
    tables.threshold_logged.insert(key.clone(), now);
    tracing::warn!(
        event = "read_cost_threshold",
        tenant_fingerprint = %key.0,
        caller = %key.1,
        stream_class = %who.class,
        reason,
        pages,
        matched,
    );
}

fn log_rejected(tables: &mut Tables, now: Instant, rejected: &[RejectedHeader]) {
    for header in rejected {
        if tables
            .header_logged
            .get(header.name)
            .is_some_and(|last| now.saturating_duration_since(*last) < THRESHOLD_EVERY)
        {
            continue;
        }
        tables.header_logged.insert(header.name, now);
        tracing::warn!(
            event = "read_header_invalid",
            header = header.name,
            length = header.length,
        );
    }
}

/// Six bytes of SHA-256 over the tenant: the only form a log line carries.
fn fingerprint(tenant: &str) -> String {
    hex::encode(&Sha256::digest(tenant.as_bytes())[..6])
}

fn wide(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

/// Layer inside [`super::response_limit::bounded`]: reads the facts a query
/// handler attached and records them. Responses without facts pass untouched.
pub async fn record(
    State(read_cost): State<Arc<ReadCost>>,
    request: Request,
    next: Next,
) -> Response {
    let mut response = next.run(request).await;
    let Some(facts) = response.extensions_mut().remove::<ReadCostFacts>() else {
        return response;
    };
    if read_cost.enabled() {
        let body_bytes = response.body().size_hint().exact().unwrap_or(0);
        let outcome = Outcome::of(response.status(), body_bytes, facts.read.counts.has_more);
        read_cost.observe(Instant::now(), &facts, outcome, body_bytes);
    }
    response
}

#[derive(Debug, Deserialize)]
pub struct ReadCostSwitch {
    pub enabled: bool,
}

/// Sort order for `GET /api/v1/read-costs`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RowOrder {
    #[default]
    Bytes,
    Pages,
    Matched,
    Whole,
    Truncated,
}

#[derive(Debug, Deserialize)]
pub struct ReadCostsParams {
    pub tenant_id: Option<String>,
    pub limit: Option<usize>,
    pub order: Option<RowOrder>,
}

#[derive(Debug, Serialize)]
pub struct CallerCost {
    pub caller: String,
    pub stream_class: String,
    #[serde(flatten)]
    pub counts: CallerRow,
}

#[derive(Debug, Serialize)]
pub struct ClassCost {
    pub stream_class: String,
    #[serde(flatten)]
    pub counts: ClassRow,
}

#[derive(Debug, Default, Serialize)]
pub struct ReadCostsResponse {
    pub callers: Vec<CallerCost>,
    pub classes: Vec<ClassCost>,
}

const UNATTRIBUTED: &str = "unattributed";
const READ_COSTS_DEFAULT_LIMIT: usize = 50;
const READ_COSTS_MAX_LIMIT: usize = 256;

/// Caller rows carry no matched count, so `matched` orders them by pages.
fn caller_key(order: RowOrder, counts: &CallerRow) -> u64 {
    match order {
        RowOrder::Bytes => counts.body_bytes,
        RowOrder::Pages | RowOrder::Matched => counts.pages,
        RowOrder::Whole => counts.whole_reads,
        RowOrder::Truncated => counts.single_truncated,
    }
}

/// Class rows carry no byte count, so `bytes` orders them by pages.
fn class_key(order: RowOrder, counts: &ClassRow) -> u64 {
    match order {
        RowOrder::Bytes | RowOrder::Pages => counts.pages,
        RowOrder::Matched => counts.matched_sum,
        RowOrder::Whole => counts.whole_reads,
        RowOrder::Truncated => counts.single_truncated,
    }
}

/// `GET /api/v1/read-costs`: the tenant named by the gateway, never another.
pub async fn serve_read_costs(
    Extension(read_cost): Extension<Arc<ReadCost>>,
    Query(params): Query<ReadCostsParams>,
) -> Response {
    let Some(tenant) = params.tenant_id.filter(|tenant| !tenant.is_empty()) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "tenant_id is required" })),
        )
            .into_response();
    };
    let limit = params
        .limit
        .unwrap_or(READ_COSTS_DEFAULT_LIMIT)
        .min(READ_COSTS_MAX_LIMIT);
    let rows = read_cost.read_costs(&tenant, params.order.unwrap_or_default(), limit);
    Json(rows).into_response()
}

/// `PUT /api/v1/admin/read-cost {"enabled": bool}`. Admin only.
pub async fn set_read_cost(
    _admin: Admin,
    Extension(read_cost): Extension<Arc<ReadCost>>,
    Json(switch): Json<ReadCostSwitch>,
) -> Json<Value> {
    read_cost.set_enabled(switch.enabled);
    Json(json!({ "enabled": switch.enabled }))
}

#[cfg(test)]
#[path = "read_cost_tests.rs"]
mod read_cost_tests;
