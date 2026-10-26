//! Latest-state reads: entity state without history, the event feed, read
//! models and views.
//!
//! Wire contract: `docs/contracts/latest-state/README.md`. Every call here is
//! available on both [`QueryClient`] and [`CoreClient`].
//!
//! A view is a read model folded by the Query Service from a declarative
//! definition. [`QueryClient::latest`] reads one row, [`QueryClient::latest_many`]
//! walks every row, and [`QueryClient::sync`] keeps a local copy current:
//!
//! ```no_run
//! # async fn run(qs: allsource::QueryClient) -> Result<(), allsource::Error> {
//! use allsource::{SyncChange, SyncOptions};
//! use std::{collections::BTreeMap, time::Duration};
//!
//! let mut rows = BTreeMap::new();
//! let mut sync = qs.sync("ops_status", SyncOptions::new().wait(Duration::from_secs(20)));
//! loop {
//!     let batch = sync.next().await?;
//!     if batch.reset {
//!         rows.clear();
//!     }
//!     for change in batch.changes {
//!         match change {
//!             SyncChange::Upsert { key, state, .. } => {
//!                 rows.insert(key, state);
//!             }
//!             SyncChange::Delete { key, .. } => {
//!                 rows.remove(&key);
//!             }
//!         }
//!     }
//!     // Persist `batch.checkpoint` with `rows` and pass it back through
//!     // `SyncOptions::checkpoint` on restart.
//! }
//! # }
//! ```

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, future::Future, sync::Arc, time::Duration};

use crate::{
    client::{CoreClient, HttpTransport, QueryClient},
    error::Error,
    projection_api::urlencode,
    types::Event,
};

/// Core holds a `changes` request for at most this long.
const LONGEST_WAIT_SECS: u64 = 20;
/// Wait used when a refusal carries no `Retry-After`.
const RETRY_FALLBACK: Duration = Duration::from_secs(1);
/// Core rejects a read-model commit above 500 changes or 240 KiB.
#[cfg_attr(not(feature = "projection-worker"), allow(dead_code))]
const COMMIT_MAX_CHANGES: usize = 500;
#[cfg_attr(not(feature = "projection-worker"), allow(dead_code))]
const COMMIT_MAX_BYTES: usize = 192 * 1024;

// ═══════════════════════════════════════════════════════════════════════════
// Shared types
// ═══════════════════════════════════════════════════════════════════════════

/// A position in a tenant's event feed. `epoch` changes whenever Core
/// renumbers the feed (restart or eviction); positions never carry across.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedCursor {
    pub epoch: u64,
    pub position: u64,
}

/// Outcome of a conditional read sent with `If-None-Match`.
#[derive(Debug, Clone)]
pub enum Conditional<T> {
    /// A 200 with a body.
    Modified(T),
    /// A 304: nothing changed since the tag that was sent.
    NotModified { etag: Option<String> },
}

impl<T> Conditional<T> {
    /// The body, or `None` on a 304.
    pub fn modified(self) -> Option<T> {
        match self {
            Self::Modified(value) => Some(value),
            Self::NotModified { .. } => None,
        }
    }

    /// `true` for a 304.
    pub fn is_not_modified(&self) -> bool {
        matches!(self, Self::NotModified { .. })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Entity state
// ═══════════════════════════════════════════════════════════════════════════

/// Parameters for [`QueryClient::entity_state`].
#[derive(Debug, Clone, Default)]
pub struct EntityStateParams {
    /// Fold only events at or before this RFC 3339 timestamp.
    pub as_of: Option<String>,
    /// Ask Core for the `history` array as well. Off by default.
    pub history: bool,
    /// Sent as the `If-None-Match` header.
    pub if_none_match: Option<String>,
}

impl EntityStateParams {
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn as_of(mut self, timestamp: &str) -> Self {
        self.as_of = Some(timestamp.to_string());
        self
    }

    #[must_use]
    pub fn history(mut self, history: bool) -> Self {
        self.history = history;
        self
    }

    /// Send an `ETag` from an earlier [`EntityState::etag`].
    #[must_use]
    pub fn if_none_match(mut self, etag: &str) -> Self {
        self.if_none_match = Some(etag.to_string());
        self
    }

    fn to_query(&self) -> Vec<(&'static str, String)> {
        let mut query = Vec::new();
        if let Some(as_of) = &self.as_of {
            query.push(("as_of", as_of.clone()));
        }
        if self.history {
            query.push(("history", "true".to_string()));
        }
        query
    }
}

/// Current state of one entity, from `GET /api/v1/entities/{id}/state`.
#[derive(Debug, Clone, Deserialize)]
pub struct EntityState {
    pub entity_id: String,
    #[serde(default)]
    pub current_state: Value,
    #[serde(default)]
    pub event_count: u64,
    #[serde(default)]
    pub last_updated: Option<String>,
    #[serde(default)]
    pub as_of: Option<String>,
    /// Number of this tenant's events folded into the state. `None` from a
    /// Core that predates the field.
    #[serde(default)]
    pub version: Option<u64>,
    /// Present only when [`EntityStateParams::history`] was set.
    #[serde(default)]
    pub history: Option<Vec<Value>>,
    /// The response's `ETag`, for a later conditional read.
    #[serde(skip)]
    pub etag: Option<String>,
}

// ═══════════════════════════════════════════════════════════════════════════
// Event feed
// ═══════════════════════════════════════════════════════════════════════════

/// Parameters for [`QueryClient::feed`].
#[derive(Debug, Clone, Default)]
pub struct FeedParams {
    /// Where to read from. `None` starts at position 0 of the current epoch.
    pub cursor: Option<FeedCursor>,
    /// Event-type prefixes, at most 8 of 128 bytes each.
    pub prefixes: Vec<String>,
    pub entity_id: Option<String>,
    /// Core's default is 100.
    pub limit: Option<u32>,
}

impl FeedParams {
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn cursor(mut self, cursor: FeedCursor) -> Self {
        self.cursor = Some(cursor);
        self
    }

    #[must_use]
    pub fn prefixes(mut self, prefixes: &[&str]) -> Self {
        self.prefixes = prefixes.iter().map(|p| (*p).to_string()).collect();
        self
    }

    #[must_use]
    pub fn entity_id(mut self, entity_id: &str) -> Self {
        self.entity_id = Some(entity_id.to_string());
        self
    }

    #[must_use]
    pub fn limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }

    fn to_query(&self) -> Vec<(&'static str, String)> {
        let mut query = Vec::new();
        if let Some(cursor) = self.cursor {
            query.push(("epoch", cursor.epoch.to_string()));
            query.push(("position", cursor.position.to_string()));
        }
        if let Some(limit) = self.limit {
            query.push(("limit", limit.to_string()));
        }
        if !self.prefixes.is_empty() {
            query.push(("prefixes", self.prefixes.join(",")));
        }
        if let Some(entity_id) = &self.entity_id {
            query.push(("entity_id", entity_id.clone()));
        }
        query
    }
}

/// One page of `GET /api/v1/event-feed`.
#[derive(Debug, Clone, Deserialize)]
pub struct FeedPage {
    #[serde(default)]
    pub tenant_id: String,
    pub cursor: FeedCursor,
    pub head: FeedCursor,
    #[serde(default)]
    pub events: Vec<Event>,
    /// Feed position of each event in `events`, in the same order.
    #[serde(default)]
    pub positions: Vec<u64>,
    #[serde(default)]
    pub caught_up: bool,
}

impl FeedPage {
    /// The cursor to send for the next page: the last element of
    /// `positions`, or the request's own cursor when the page was empty.
    pub fn next_cursor(&self) -> FeedCursor {
        FeedCursor {
            epoch: self.cursor.epoch,
            position: self
                .positions
                .last()
                .copied()
                .unwrap_or(self.cursor.position),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Read models
// ═══════════════════════════════════════════════════════════════════════════

/// Parameters for [`ReadModel::page`].
#[derive(Debug, Clone, Default)]
pub struct ReadModelPageParams {
    pub prefix: Option<String>,
    /// Return rows whose key sorts after this one (the previous page's `next`).
    pub after: Option<String>,
    /// 1..500, Core's default is 100.
    pub limit: Option<u32>,
    /// A `content_revision` from an earlier read. When nothing changed since,
    /// Core answers `unchanged: true` with no rows.
    pub revision: Option<u64>,
}

impl ReadModelPageParams {
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn prefix(mut self, prefix: &str) -> Self {
        self.prefix = Some(prefix.to_string());
        self
    }

    #[must_use]
    pub fn after(mut self, key: &str) -> Self {
        self.after = Some(key.to_string());
        self
    }

    #[must_use]
    pub fn limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }

    #[must_use]
    pub fn revision(mut self, revision: u64) -> Self {
        self.revision = Some(revision);
        self
    }

    fn to_query(&self) -> Vec<(&'static str, String)> {
        let mut query = Vec::new();
        if let Some(prefix) = &self.prefix {
            query.push(("prefix", prefix.clone()));
        }
        if let Some(after) = &self.after {
            query.push(("after", after.clone()));
        }
        if let Some(limit) = self.limit {
            query.push(("limit", limit.to_string()));
        }
        if let Some(revision) = self.revision {
            query.push(("revision", revision.to_string()));
        }
        query
    }
}

/// One page of `GET /api/v1/read-models/{name}`.
#[derive(Debug, Clone, Deserialize)]
pub struct ReadModelPage {
    pub revision: u64,
    #[serde(default)]
    pub content_revision: u64,
    #[serde(default)]
    pub checkpoint: Option<FeedCursor>,
    #[serde(default)]
    pub ready: bool,
    /// `true` when the request's `revision` is still current. `rows` is then
    /// empty and must not be read as deletions.
    #[serde(default)]
    pub unchanged: bool,
    #[serde(default)]
    pub rows: BTreeMap<String, Value>,
    #[serde(default)]
    pub next: Option<String>,
}

/// Body of `PUT /api/v1/read-models/{name}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadModelCommit {
    /// The model revision this commit was computed against. A stale value
    /// fails with [`Error::VersionConflict`].
    pub expected_revision: u64,
    /// The feed position the model now reflects.
    pub checkpoint: FeedCursor,
    pub ready: bool,
    /// Drop every row before applying `changes`; required when the feed
    /// epoch changed.
    pub reset: bool,
    /// `None` deletes the row; it is sent as JSON `null`.
    pub changes: BTreeMap<String, Option<Value>>,
}

impl ReadModelCommit {
    /// An empty, ready commit with no reset.
    pub fn new(expected_revision: u64, checkpoint: FeedCursor) -> Self {
        Self {
            expected_revision,
            checkpoint,
            ready: true,
            reset: false,
            changes: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn upsert(mut self, key: impl Into<String>, state: Value) -> Self {
        self.changes.insert(key.into(), Some(state));
        self
    }

    #[must_use]
    pub fn delete(mut self, key: impl Into<String>) -> Self {
        self.changes.insert(key.into(), None);
        self
    }

    #[must_use]
    pub fn ready(mut self, ready: bool) -> Self {
        self.ready = ready;
        self
    }

    #[must_use]
    pub fn reset(mut self, reset: bool) -> Self {
        self.reset = reset;
        self
    }
}

#[derive(Deserialize)]
struct CommitResponse {
    revision: u64,
}

/// Handle on one named read model, from [`QueryClient::read_model`].
#[derive(Debug, Clone)]
pub struct ReadModel {
    transport: Arc<HttpTransport>,
    name: String,
}

impl ReadModel {
    pub fn name(&self) -> &str {
        &self.name
    }

    fn path(&self) -> String {
        format!("/api/v1/read-models/{}", urlencode(&self.name))
    }

    /// Read one page of rows.
    pub async fn page(&self, params: &ReadModelPageParams) -> Result<ReadModelPage, Error> {
        self.transport
            .get_with_query(&self.path(), &params.to_query())
            .await
    }

    /// Apply a commit and return the model's new revision. A stale
    /// `expected_revision` fails with [`Error::VersionConflict`] and is never
    /// retried. Needs `projections:write` at the gateway; `view_*` and
    /// `views_registry` are reserved for the Query Service.
    pub async fn commit(&self, commit: &ReadModelCommit) -> Result<u64, Error> {
        let resp: CommitResponse = self.transport.put(&self.path(), commit).await?;
        Ok(resp.revision)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Views
// ═══════════════════════════════════════════════════════════════════════════

/// A view's definition and fold progress, from [`QueryClient::get_view`] and
/// [`QueryClient::define_view`].
#[derive(Debug, Clone, Deserialize)]
pub struct ViewInfo {
    pub view: String,
    #[serde(default)]
    pub definition: Value,
    /// Incremented by every new definition.
    #[serde(default)]
    pub generation: u64,
    #[serde(default)]
    pub revision: u64,
    #[serde(default)]
    pub checkpoint: Option<FeedCursor>,
    #[serde(default)]
    pub ready: bool,
}

/// One entry of [`QueryClient::list_views`].
#[derive(Debug, Clone, Deserialize)]
pub struct ViewSummary {
    pub view: String,
    #[serde(default)]
    pub generation: u64,
    #[serde(default)]
    pub revision: u64,
    #[serde(default)]
    pub ready: bool,
}

#[derive(Deserialize)]
struct ViewList {
    #[serde(default)]
    views: Vec<ViewSummary>,
}

/// One view row, from [`QueryClient::latest`].
#[derive(Debug, Clone, Deserialize)]
pub struct ViewRow {
    pub view: String,
    pub key: String,
    #[serde(default)]
    pub state: Value,
    /// The model revision that last wrote this key.
    #[serde(default)]
    pub version: u64,
    #[serde(default)]
    pub revision: u64,
    #[serde(default)]
    pub generation: u64,
    #[serde(default)]
    pub ready: bool,
    /// The response's `ETag`, for a later conditional read.
    #[serde(skip)]
    pub etag: Option<String>,
}

/// Outcome of [`QueryClient::latest`].
#[derive(Debug, Clone)]
pub enum LatestRow {
    Found(ViewRow),
    /// A 304 for the `If-None-Match` tag that was sent.
    NotModified {
        etag: Option<String>,
    },
    /// No row under that key. `ready: false` means the view is still
    /// rebuilding, so the row may yet appear.
    NotFound {
        ready: bool,
    },
}

/// Parameters for [`QueryClient::view_entities`] and [`QueryClient::latest_many`].
#[derive(Debug, Clone, Default)]
pub struct ViewEntitiesParams {
    pub prefix: Option<String>,
    /// Start after this key (a previous page's `next`).
    pub after: Option<String>,
    /// 1..500, Core's default is 100.
    pub limit: Option<u32>,
}

impl ViewEntitiesParams {
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn prefix(mut self, prefix: &str) -> Self {
        self.prefix = Some(prefix.to_string());
        self
    }

    #[must_use]
    pub fn after(mut self, key: &str) -> Self {
        self.after = Some(key.to_string());
        self
    }

    #[must_use]
    pub fn limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }

    fn to_query(&self) -> Vec<(&'static str, String)> {
        let mut query = Vec::new();
        if let Some(prefix) = &self.prefix {
            query.push(("prefix", prefix.clone()));
        }
        if let Some(after) = &self.after {
            query.push(("after", after.clone()));
        }
        if let Some(limit) = self.limit {
            query.push(("limit", limit.to_string()));
        }
        query
    }
}

/// One row of a [`ViewEntitiesPage`].
#[derive(Debug, Clone, Deserialize)]
pub struct ViewEntry {
    pub key: String,
    #[serde(default)]
    pub state: Value,
    #[serde(default)]
    pub version: u64,
}

/// One page of `GET /api/v1/views/{view}/entities`.
#[derive(Debug, Clone, Deserialize)]
pub struct ViewEntitiesPage {
    pub view: String,
    #[serde(default)]
    pub rows: Vec<ViewEntry>,
    /// `after` for the next page; `None` on the last page.
    #[serde(default)]
    pub next: Option<String>,
    /// Watermark to start [`QueryClient::view_changes`] from.
    #[serde(default)]
    pub revision: u64,
    #[serde(default)]
    pub content_revision: u64,
    #[serde(default)]
    pub generation: u64,
    #[serde(default)]
    pub ready: bool,
}

/// Parameters for [`QueryClient::view_changes`].
#[derive(Debug, Clone, Default)]
pub struct ViewChangesParams {
    pub after: u64,
    pub limit: Option<u32>,
    /// Long-poll: when nothing changed after `after`, Core holds the request
    /// up to this long (capped at 20 seconds). Sent in whole seconds.
    pub wait: Option<Duration>,
}

impl ViewChangesParams {
    /// Changes after `revision`.
    pub fn after(revision: u64) -> Self {
        Self {
            after: revision,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }

    #[must_use]
    pub fn wait(mut self, wait: Duration) -> Self {
        self.wait = Some(wait);
        self
    }

    fn to_query(&self) -> Vec<(&'static str, String)> {
        let mut query = vec![("after", self.after.to_string())];
        if let Some(limit) = self.limit {
            query.push(("limit", limit.to_string()));
        }
        if let Some(wait) = self.wait {
            query.push(("wait", wait.as_secs().min(LONGEST_WAIT_SECS).to_string()));
        }
        query
    }
}

/// One entry of [`ViewChanges::changes`].
#[derive(Debug, Clone, Deserialize)]
pub struct ViewChange {
    pub key: String,
    #[serde(default)]
    pub version: u64,
    /// `None` is a deletion.
    #[serde(default)]
    pub state: Option<Value>,
}

/// Response of `GET /api/v1/views/{view}/changes`.
#[derive(Debug, Clone, Deserialize)]
pub struct ViewChanges {
    pub view: String,
    #[serde(default)]
    pub changes: Vec<ViewChange>,
    /// `after` for the next call.
    pub revision: u64,
    #[serde(default)]
    pub caught_up: bool,
    #[serde(default)]
    pub generation: u64,
}

fn view_path(view: &str) -> String {
    format!("/api/v1/views/{}", urlencode(view))
}

/// Typed errors for the view routes' refusal bodies, so callers never parse
/// `Error::Api` for them.
pub(crate) fn view_refusal(status: u16, body: Option<&Value>) -> Option<Error> {
    let body = body?;
    match (status, body.get("error").and_then(Value::as_str)) {
        (409, Some("view_reset")) => Some(Error::ViewReset {
            generation: body.get("generation").and_then(Value::as_u64)?,
        }),
        (400, Some("invalid_view")) => Some(Error::InvalidView {
            detail: body
                .get("detail")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        }),
        _ => None,
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Calls
// ═══════════════════════════════════════════════════════════════════════════

async fn fetch_entity_state(
    transport: &HttpTransport,
    entity_id: &str,
    params: &EntityStateParams,
) -> Result<Conditional<EntityState>, Error> {
    let path = format!("/api/v1/entities/{}/state", urlencode(entity_id));
    let (body, meta) = transport
        .get_with_meta::<EntityState>(&path, &params.to_query(), params.if_none_match.as_deref())
        .await?;
    Ok(match body {
        Some(mut state) => {
            state.etag = meta.etag;
            Conditional::Modified(state)
        }
        None => Conditional::NotModified { etag: meta.etag },
    })
}

async fn fetch_feed(transport: &HttpTransport, params: &FeedParams) -> Result<FeedPage, Error> {
    match transport
        .get_with_query::<FeedPage>("/api/v1/event-feed", &params.to_query())
        .await
    {
        Err(Error::VersionConflict { expected, current }) => {
            Err(Error::FeedEpochChanged { expected, current })
        }
        other => other,
    }
}

async fn fetch_latest(
    transport: &HttpTransport,
    view: &str,
    key: &str,
    if_none_match: Option<&str>,
) -> Result<LatestRow, Error> {
    let path = format!("{}/entities/{}", view_path(view), urlencode(key));
    match transport
        .get_with_meta::<ViewRow>(&path, &[], if_none_match)
        .await
    {
        Ok((Some(mut row), meta)) => {
            row.etag = meta.etag;
            Ok(LatestRow::Found(row))
        }
        Ok((None, meta)) => Ok(LatestRow::NotModified { etag: meta.etag }),
        Err(Error::Api {
            status: 404, body, ..
        }) => Ok(LatestRow::NotFound {
            ready: body
                .as_ref()
                .and_then(|b| b.get("ready"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }),
        Err(e) => Err(e),
    }
}

async fn fetch_view_entities(
    transport: &HttpTransport,
    view: &str,
    params: &ViewEntitiesParams,
) -> Result<ViewEntitiesPage, Error> {
    let path = format!("{}/entities", view_path(view));
    transport.get_with_query(&path, &params.to_query()).await
}

async fn fetch_view_changes(
    transport: &HttpTransport,
    view: &str,
    params: &ViewChangesParams,
) -> Result<ViewChanges, Error> {
    let path = format!("{}/changes", view_path(view));
    transport.get_with_query(&path, &params.to_query()).await
}

/// Repeat `call` after every rate or capacity refusal, waiting as long as the
/// server asked. Every other outcome is returned.
async fn honouring_retry_after<T, F, Fut>(call: F) -> Result<T, Error>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, Error>>,
{
    loop {
        let wait = match call().await {
            Err(Error::RateLimited { retry_after, .. }) => retry_after.unwrap_or(RETRY_FALLBACK),
            Err(Error::Api { status: 503, .. }) => RETRY_FALLBACK,
            Err(Error::CircuitOpen { retry_after_secs }) => {
                Duration::from_secs(retry_after_secs.max(1))
            }
            other => return other,
        };
        tracing::debug!(
            wait_ms = wait.as_millis(),
            "refused; waiting before repeating"
        );
        tokio::time::sleep(wait).await;
    }
}

fn no_progress_error(view: &str, key: &str) -> Error {
    Error::Protocol(format!(
        "view {view:?}: the page after {key:?} answered next = {key:?} again, so paging cannot \
         make progress"
    ))
}

macro_rules! latest_state_methods {
    ($client:ty) => {
        impl $client {
            /// Current state of one entity without its history
            /// (`GET /api/v1/entities/{id}/state`).
            ///
            /// With [`EntityStateParams::if_none_match`] set, an unchanged
            /// entity is [`Conditional::NotModified`].
            pub async fn entity_state(
                &self,
                entity_id: &str,
                params: &EntityStateParams,
            ) -> Result<Conditional<EntityState>, Error> {
                fetch_entity_state(self.http(), entity_id, params).await
            }

            /// One page of the tenant's event feed (`GET /api/v1/event-feed`).
            ///
            /// Resume with [`FeedPage::next_cursor`]. A cursor from an older
            /// epoch fails with [`Error::FeedEpochChanged`]: rebuild from
            /// position 0 of the new epoch and commit with `reset: true`.
            pub async fn feed(&self, params: &FeedParams) -> Result<FeedPage, Error> {
                fetch_feed(self.http(), params).await
            }

            /// Handle on the read model `name`, for [`ReadModel::page`] and
            /// [`ReadModel::commit`].
            pub fn read_model(&self, name: &str) -> ReadModel {
                ReadModel {
                    transport: Arc::clone(self.http()),
                    name: name.to_string(),
                }
            }

            /// Create or replace a view (`PUT /api/v1/views/{view}`). The
            /// definition is sent verbatim; a rejected one fails with
            /// [`Error::InvalidView`]. A new definition increments
            /// `generation` and the Query Service rebuilds the rows.
            pub async fn define_view<D: Serialize>(
                &self,
                view: &str,
                definition: &D,
            ) -> Result<ViewInfo, Error> {
                self.http().put(&view_path(view), definition).await
            }

            /// A view's definition and fold progress.
            pub async fn get_view(&self, view: &str) -> Result<ViewInfo, Error> {
                self.http().get(&view_path(view)).await
            }

            /// Remove a view's definition and rows.
            pub async fn delete_view(&self, view: &str) -> Result<(), Error> {
                self.http().delete(&view_path(view)).await
            }

            /// The tenant's views.
            pub async fn list_views(&self) -> Result<Vec<ViewSummary>, Error> {
                let list: ViewList = self.http().get("/api/v1/views").await?;
                Ok(list.views)
            }

            /// Latest state of one view row. `if_none_match` is an earlier
            /// [`ViewRow::etag`].
            pub async fn latest(
                &self,
                view: &str,
                key: &str,
                if_none_match: Option<&str>,
            ) -> Result<LatestRow, Error> {
                fetch_latest(self.http(), view, key, if_none_match).await
            }

            /// Every row of a view, one page per [`LatestManyPaginator::next_page`].
            pub fn latest_many(
                &self,
                view: &str,
                params: ViewEntitiesParams,
            ) -> LatestManyPaginator {
                LatestManyPaginator {
                    transport: Arc::clone(self.http()),
                    view: view.to_string(),
                    params,
                    revision: None,
                    exhausted: false,
                }
            }

            /// One page of a view's rows (`GET /api/v1/views/{view}/entities`).
            pub async fn view_entities(
                &self,
                view: &str,
                params: &ViewEntitiesParams,
            ) -> Result<ViewEntitiesPage, Error> {
                fetch_view_entities(self.http(), view, params).await
            }

            /// Rows changed after a revision (`GET /api/v1/views/{view}/changes`).
            ///
            /// A revision older than the view's last reset fails with
            /// [`Error::ViewReset`]; re-read the entity pages.
            pub async fn view_changes(
                &self,
                view: &str,
                params: &ViewChangesParams,
            ) -> Result<ViewChanges, Error> {
                fetch_view_changes(self.http(), view, params).await
            }

            /// Keep a local copy of a view current: every entity page, then
            /// the changes after them, re-reading the pages after a reset.
            pub fn sync(&self, view: &str, options: SyncOptions) -> ViewSync {
                ViewSync::new(Arc::clone(self.http()), view, options)
            }
        }
    };
}

latest_state_methods!(QueryClient);
latest_state_methods!(CoreClient);

// ═══════════════════════════════════════════════════════════════════════════
// latest_many
// ═══════════════════════════════════════════════════════════════════════════

/// Walks every page of a view's rows. From [`QueryClient::latest_many`].
pub struct LatestManyPaginator {
    transport: Arc<HttpTransport>,
    view: String,
    params: ViewEntitiesParams,
    revision: Option<u64>,
    exhausted: bool,
}

impl LatestManyPaginator {
    /// The next page, or `None` once the last page has been returned.
    pub async fn next_page(&mut self) -> Result<Option<ViewEntitiesPage>, Error> {
        if self.exhausted {
            return Ok(None);
        }
        let page = fetch_view_entities(&self.transport, &self.view, &self.params).await?;
        self.revision = Some(
            self.revision
                .map_or(page.revision, |r| r.min(page.revision)),
        );
        match &page.next {
            Some(next) if self.params.after.as_deref() == Some(next.as_str()) => {
                self.exhausted = true;
                return Err(no_progress_error(&self.view, next));
            }
            Some(next) => self.params.after = Some(next.clone()),
            None => self.exhausted = true,
        }
        Ok(Some(page))
    }

    /// The lowest page `revision` seen so far: a safe `after` for
    /// [`QueryClient::view_changes`] once every page has been read.
    pub fn revision(&self) -> Option<u64> {
        self.revision
    }

    pub fn is_exhausted(&self) -> bool {
        self.exhausted
    }

    /// Every remaining row.
    pub async fn collect_all(mut self) -> Result<Vec<ViewEntry>, Error> {
        let mut rows = Vec::new();
        while let Some(page) = self.next_page().await? {
            rows.extend(page.rows);
        }
        Ok(rows)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// sync
// ═══════════════════════════════════════════════════════════════════════════

/// Options for [`QueryClient::sync`].
#[derive(Debug, Clone)]
pub struct SyncOptions {
    /// A checkpoint stored from an earlier [`SyncBatch::checkpoint`]. `None`
    /// starts with a full read of the entity pages.
    pub checkpoint: Option<u64>,
    /// Long-poll wait for `changes` once caught up (capped at 20 seconds).
    pub wait: Option<Duration>,
    /// Rows per entity page.
    pub page_size: Option<u32>,
    /// Pause between polls when caught up and `wait` is not set.
    pub poll_interval: Duration,
}

impl Default for SyncOptions {
    fn default() -> Self {
        Self {
            checkpoint: None,
            wait: None,
            page_size: None,
            poll_interval: Duration::from_secs(1),
        }
    }
}

impl SyncOptions {
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn checkpoint(mut self, checkpoint: u64) -> Self {
        self.checkpoint = Some(checkpoint);
        self
    }

    #[must_use]
    pub fn wait(mut self, wait: Duration) -> Self {
        self.wait = Some(wait);
        self
    }

    #[must_use]
    pub fn page_size(mut self, page_size: u32) -> Self {
        self.page_size = Some(page_size);
        self
    }

    #[must_use]
    pub fn poll_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }

    fn long_polls(&self) -> bool {
        self.wait.is_some_and(|wait| !wait.is_zero())
    }
}

/// One change yielded by [`ViewSync::next`].
#[derive(Debug, Clone, PartialEq)]
pub enum SyncChange {
    Upsert {
        key: String,
        state: Value,
        version: u64,
    },
    Delete {
        key: String,
        version: u64,
    },
}

impl SyncChange {
    pub fn key(&self) -> &str {
        match self {
            Self::Upsert { key, .. } | Self::Delete { key, .. } => key,
        }
    }
}

/// A batch of changes from [`ViewSync::next`].
#[derive(Debug, Clone)]
pub struct SyncBatch {
    /// Discard every local row before applying `changes`: a full read of the
    /// entity pages is starting.
    pub reset: bool,
    /// `true` while the batch comes from the entity pages.
    pub snapshot: bool,
    pub changes: Vec<SyncChange>,
    /// Store this with the applied rows and pass it to
    /// [`SyncOptions::checkpoint`] on restart. `None` partway through the
    /// entity pages, where there is nothing safe to resume from.
    pub checkpoint: Option<u64>,
}

#[derive(Debug, Clone)]
enum SyncPhase {
    Snapshot {
        after: Option<String>,
        reset: bool,
        watermark: Option<u64>,
    },
    Changes {
        after: u64,
        long_poll: bool,
    },
}

/// A running view sync, from [`QueryClient::sync`].
///
/// Each [`Self::next`] returns the next non-empty batch. A `429` or `503`
/// waits for `Retry-After` and repeats the same request; a
/// [`Error::ViewReset`] restarts the entity pages with `reset: true`. Any
/// other error is returned, and calling `next` again resumes where it failed.
pub struct ViewSync {
    transport: Arc<HttpTransport>,
    view: String,
    options: SyncOptions,
    phase: SyncPhase,
    checkpoint: Option<u64>,
}

impl ViewSync {
    fn new(transport: Arc<HttpTransport>, view: &str, options: SyncOptions) -> Self {
        let phase = match options.checkpoint {
            Some(after) => SyncPhase::Changes {
                after,
                long_poll: false,
            },
            None => SyncPhase::Snapshot {
                after: None,
                reset: true,
                watermark: None,
            },
        };
        Self {
            transport,
            view: view.to_string(),
            checkpoint: options.checkpoint,
            options,
            phase,
        }
    }

    /// The last checkpoint handed out, or the one the sync started from.
    pub fn checkpoint(&self) -> Option<u64> {
        self.checkpoint
    }

    /// Wait for the next non-empty batch.
    pub async fn next(&mut self) -> Result<SyncBatch, Error> {
        loop {
            match self.phase.clone() {
                SyncPhase::Snapshot {
                    after,
                    reset,
                    watermark,
                } => {
                    let params = ViewEntitiesParams {
                        prefix: None,
                        after: after.clone(),
                        limit: self.options.page_size,
                    };
                    let transport = &self.transport;
                    let view = self.view.as_str();
                    let page =
                        honouring_retry_after(|| fetch_view_entities(transport, view, &params))
                            .await?;
                    // The lowest page revision: a row rewritten while later
                    // pages were read is then replayed by `changes`.
                    let watermark = watermark.map_or(page.revision, |w| w.min(page.revision));
                    let changes = page
                        .rows
                        .into_iter()
                        .map(|row| SyncChange::Upsert {
                            key: row.key,
                            state: row.state,
                            version: row.version,
                        })
                        .collect();
                    if let Some(next) = page.next {
                        if after.as_deref() == Some(next.as_str()) {
                            return Err(no_progress_error(&self.view, &next));
                        }
                        self.phase = SyncPhase::Snapshot {
                            after: Some(next),
                            reset: false,
                            watermark: Some(watermark),
                        };
                        return Ok(SyncBatch {
                            reset,
                            snapshot: true,
                            changes,
                            checkpoint: None,
                        });
                    }
                    self.checkpoint = Some(watermark);
                    self.phase = SyncPhase::Changes {
                        after: watermark,
                        long_poll: false,
                    };
                    return Ok(SyncBatch {
                        reset,
                        snapshot: true,
                        changes,
                        checkpoint: Some(watermark),
                    });
                }
                SyncPhase::Changes { after, long_poll } => {
                    let params = ViewChangesParams {
                        after,
                        limit: None,
                        wait: if long_poll { self.options.wait } else { None },
                    };
                    let transport = &self.transport;
                    let view = self.view.as_str();
                    let result =
                        honouring_retry_after(|| fetch_view_changes(transport, view, &params))
                            .await;
                    let resp = match result {
                        Err(Error::ViewReset { generation }) => {
                            tracing::info!(
                                view = %self.view,
                                generation,
                                after,
                                "view reset past the checkpoint; re-reading its rows"
                            );
                            self.checkpoint = None;
                            self.phase = SyncPhase::Snapshot {
                                after: None,
                                reset: true,
                                watermark: None,
                            };
                            continue;
                        }
                        other => other?,
                    };
                    self.checkpoint = Some(resp.revision);
                    self.phase = SyncPhase::Changes {
                        after: resp.revision,
                        long_poll: resp.caught_up,
                    };
                    if resp.changes.is_empty() {
                        if resp.caught_up && !self.options.long_polls() {
                            tokio::time::sleep(self.options.poll_interval).await;
                        }
                        continue;
                    }
                    let changes = resp
                        .changes
                        .into_iter()
                        .map(|change| match change.state {
                            Some(state) => SyncChange::Upsert {
                                key: change.key,
                                state,
                                version: change.version,
                            },
                            None => SyncChange::Delete {
                                key: change.key,
                                version: change.version,
                            },
                        })
                        .collect();
                    return Ok(SyncBatch {
                        reset: false,
                        snapshot: false,
                        changes,
                        checkpoint: Some(resp.revision),
                    });
                }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Read-model rows for ProjectionWorker
// ═══════════════════════════════════════════════════════════════════════════

/// Split `entries` into slices that each fit one read-model commit.
#[cfg_attr(not(feature = "projection-worker"), allow(dead_code))]
fn commit_chunks(entries: &[(String, Value)]) -> Vec<&[(String, Value)]> {
    let mut chunks = Vec::new();
    let mut start = 0;
    let mut bytes = 0;
    for (i, (key, value)) in entries.iter().enumerate() {
        let size = key.len() + value.to_string().len() + 8;
        if i > start && (i - start >= COMMIT_MAX_CHANGES || bytes + size > COMMIT_MAX_BYTES) {
            chunks.push(&entries[start..i]);
            start = i;
            bytes = 0;
        }
        bytes += size;
    }
    if start < entries.len() {
        chunks.push(&entries[start..]);
    }
    chunks
}

/// Upsert `entries` into the read model `name`.
///
/// The checkpoint is the feed head read before the first commit; the writer
/// tracks its own position elsewhere, so the checkpoint only has to stay
/// inside the current feed generation. A model last committed in an older
/// epoch is reset first. A [`Error::VersionConflict`] re-reads the revision
/// and retries that commit once.
#[cfg_attr(not(feature = "projection-worker"), allow(dead_code))]
pub(crate) async fn upsert_read_model_rows(
    core: &CoreClient,
    name: &str,
    entries: &[(String, Value)],
) -> Result<(), Error> {
    let model = core.read_model(name);
    let head = core.feed(&FeedParams::new().limit(1)).await?.head;
    for chunk in commit_chunks(entries) {
        let mut retried = false;
        loop {
            let current = model.page(&ReadModelPageParams::new().limit(1)).await?;
            let reset = current
                .checkpoint
                .is_some_and(|checkpoint| checkpoint.epoch != head.epoch);
            let commit = chunk.iter().fold(
                ReadModelCommit::new(current.revision, head).reset(reset),
                |commit, (key, state)| commit.upsert(key.clone(), state.clone()),
            );
            match model.commit(&commit).await {
                Ok(_) => break,
                Err(Error::VersionConflict { .. }) if !retried => retried = true,
                Err(e) => return Err(e),
            }
        }
    }
    Ok(())
}

/// One row of the read model `name`, or `None`.
#[cfg_attr(not(feature = "projection-worker"), allow(dead_code))]
pub(crate) async fn read_model_row(
    core: &CoreClient,
    name: &str,
    key: &str,
) -> Result<Option<Value>, Error> {
    // The exact key sorts first among the keys it prefixes.
    let page = core
        .read_model(name)
        .page(&ReadModelPageParams::new().prefix(key).limit(1))
        .await?;
    Ok(page
        .rows
        .into_iter()
        .find_map(|(row_key, state)| (row_key == key).then_some(state)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ClientConfig;
    use serde_json::json;
    use wiremock::{
        matchers::{method, path, query_param, query_param_is_missing},
        Mock, MockServer, Request, ResponseTemplate,
    };

    fn query_client(server: &MockServer) -> QueryClient {
        let mut config = ClientConfig::new(&server.uri(), "test-key");
        config.retry.base_delay = Duration::from_millis(1);
        QueryClient::with_config(config).unwrap()
    }

    fn core_client(server: &MockServer) -> CoreClient {
        let mut config = ClientConfig::new(&server.uri(), "test-key");
        config.retry.base_delay = Duration::from_millis(1);
        CoreClient::with_config(config).unwrap()
    }

    fn pairs(request: &Request) -> Vec<(String, String)> {
        request
            .url
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect()
    }

    fn body_json(request: &Request) -> Value {
        serde_json::from_slice(&request.body).unwrap()
    }

    fn feed_body(epoch: u64, head: u64) -> Value {
        json!({
            "tenant_id": "t1",
            "cursor": {"epoch": epoch, "position": 0},
            "head": {"epoch": epoch, "position": head},
            "events": [],
            "positions": [],
            "caught_up": true
        })
    }

    fn model_body(revision: u64, checkpoint: &Value) -> Value {
        json!({
            "revision": revision,
            "content_revision": revision,
            "checkpoint": checkpoint,
            "ready": true,
            "unchanged": false,
            "rows": {},
            "next": null
        })
    }

    #[tokio::test]
    async fn entity_state_sends_history_and_as_of_only_when_asked() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/entities/user-1/state"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "entity_id": "user-1",
                "current_state": {"name": "Ada"},
                "event_count": 2,
                "last_updated": "2026-10-27T10:00:00Z",
                "as_of": "2026-10-27T00:00:00Z",
                "history": [{"event_type": "user.created"}],
                "version": 2
            })))
            .mount(&server)
            .await;

        let state = query_client(&server)
            .entity_state(
                "user-1",
                &EntityStateParams::new()
                    .history(true)
                    .as_of("2026-10-27T00:00:00Z"),
            )
            .await
            .unwrap()
            .modified()
            .unwrap();
        assert_eq!(state.history.as_ref().map(Vec::len), Some(1));
        assert_eq!(state.version, Some(2));

        let requests = server.received_requests().await.unwrap();
        let sent = pairs(&requests[0]);
        assert!(sent.contains(&("history".into(), "true".into())));
        assert!(sent.contains(&("as_of".into(), "2026-10-27T00:00:00Z".into())));
    }

    #[tokio::test]
    async fn entity_state_without_history_sends_no_query() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/entities/user-1/state"))
            .and(query_param_is_missing("history"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "entity_id": "user-1",
                "current_state": {},
                "event_count": 0
            })))
            .mount(&server)
            .await;
        let state = core_client(&server)
            .entity_state("user-1", &EntityStateParams::new())
            .await
            .unwrap()
            .modified()
            .unwrap();
        assert_eq!(state.version, None, "older Core sends no version");
        assert!(state.history.is_none());
    }

    #[tokio::test]
    async fn feed_without_cursor_sends_neither_epoch_nor_position() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/event-feed"))
            .and(query_param_is_missing("epoch"))
            .and(query_param_is_missing("position"))
            .and(query_param("prefixes", "a.,b."))
            .and(query_param("entity_id", "e1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(feed_body(7, 0)))
            .mount(&server)
            .await;
        let page = query_client(&server)
            .feed(&FeedParams::new().prefixes(&["a.", "b."]).entity_id("e1"))
            .await
            .unwrap();
        assert_eq!(
            page.next_cursor(),
            FeedCursor {
                epoch: 7,
                position: 0
            },
            "an empty page resumes from its own cursor"
        );
    }

    #[test]
    fn read_model_commit_serialises_a_deletion_as_null() {
        let commit = ReadModelCommit::new(
            3,
            FeedCursor {
                epoch: 1,
                position: 2,
            },
        )
        .upsert("a", json!({"x": 1}))
        .delete("b");
        assert_eq!(
            serde_json::to_value(&commit).unwrap(),
            json!({
                "expected_revision": 3,
                "checkpoint": {"epoch": 1, "position": 2},
                "ready": true,
                "reset": false,
                "changes": {"a": {"x": 1}, "b": null}
            })
        );
    }

    #[tokio::test]
    async fn delete_view_accepts_an_empty_body() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/api/v1/views/ops_status"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;
        query_client(&server)
            .delete_view("ops_status")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn delete_view_of_a_missing_view_is_an_api_error() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/api/v1/views/nope"))
            .respond_with(ResponseTemplate::new(404).set_body_json(json!({"error": "not_found"})))
            .mount(&server)
            .await;
        let err = query_client(&server).delete_view("nope").await.unwrap_err();
        assert!(err.is_not_found(), "{err:?}");
    }

    #[tokio::test]
    async fn latest_many_collects_every_page_and_tracks_the_lowest_revision() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/views/v/entities"))
            .and(query_param_is_missing("after"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "view": "v", "rows": [{"key": "a", "state": {}, "version": 1}],
                "next": "a", "revision": 9, "content_revision": 9, "generation": 1, "ready": true
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v1/views/v/entities"))
            .and(query_param("after", "a"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "view": "v", "rows": [{"key": "b", "state": {}, "version": 2}],
                "next": null, "revision": 10, "content_revision": 10, "generation": 1, "ready": true
            })))
            .mount(&server)
            .await;

        let client = query_client(&server);
        let mut pages = client.latest_many("v", ViewEntitiesParams::new());
        assert_eq!(pages.next_page().await.unwrap().unwrap().rows.len(), 1);
        assert_eq!(pages.next_page().await.unwrap().unwrap().rows.len(), 1);
        assert!(pages.next_page().await.unwrap().is_none());
        assert_eq!(pages.revision(), Some(9));

        let rows = client
            .latest_many("v", ViewEntitiesParams::new())
            .collect_all()
            .await
            .unwrap();
        let keys: Vec<_> = rows.iter().map(|r| r.key.as_str()).collect();
        assert_eq!(keys, ["a", "b"]);
    }

    #[tokio::test]
    async fn latest_many_refuses_a_server_that_repeats_its_cursor() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/views/v/entities"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "view": "v", "rows": [{"key": "a", "state": {}, "version": 1}],
                "next": "a", "revision": 9
            })))
            .mount(&server)
            .await;
        let client = query_client(&server);
        let mut pages = client.latest_many("v", ViewEntitiesParams::new());
        pages.next_page().await.unwrap();
        assert!(matches!(pages.next_page().await, Err(Error::Protocol(_))));
        assert!(pages.next_page().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn sync_from_a_checkpoint_skips_the_entity_pages() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/views/v/changes"))
            .and(query_param("after", "5"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "view": "v",
                "changes": [{"key": "a", "version": 6, "state": {"n": 1}}],
                "revision": 6, "caught_up": true, "generation": 1
            })))
            .mount(&server)
            .await;
        let mut sync = query_client(&server).sync("v", SyncOptions::new().checkpoint(5));
        assert_eq!(sync.checkpoint(), Some(5));
        let batch = sync.next().await.unwrap();
        assert!(!batch.reset && !batch.snapshot);
        assert_eq!(
            batch.changes,
            vec![SyncChange::Upsert {
                key: "a".into(),
                state: json!({"n": 1}),
                version: 6
            }]
        );
        assert_eq!(batch.checkpoint, Some(6));
        let requests = server.received_requests().await.unwrap();
        assert!(requests
            .iter()
            .all(|r| !r.url.path().ends_with("/entities")));
    }

    #[tokio::test]
    async fn sync_without_wait_pauses_between_empty_polls() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/views/v/changes"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "view": "v", "changes": [], "revision": 5, "caught_up": true, "generation": 1
            })))
            .mount(&server)
            .await;
        let mut sync = query_client(&server).sync(
            "v",
            SyncOptions::new()
                .checkpoint(5)
                .poll_interval(Duration::from_millis(200)),
        );
        let _ = tokio::time::timeout(Duration::from_millis(500), sync.next()).await;
        let polls = server.received_requests().await.unwrap().len();
        assert!((1..=3).contains(&polls), "{polls} polls in 500ms");
        assert!(server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|r| !pairs(r).iter().any(|(k, _)| k == "wait")));
    }

    #[tokio::test]
    async fn sync_repeats_after_a_503() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/views/v/changes"))
            .respond_with(ResponseTemplate::new(503).insert_header("Retry-After", "1"))
            .up_to_n_times(1)
            .with_priority(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v1/views/v/changes"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "view": "v",
                "changes": [{"key": "a", "version": 6, "state": null}],
                "revision": 6, "caught_up": true, "generation": 1
            })))
            .mount(&server)
            .await;
        let mut sync = query_client(&server).sync("v", SyncOptions::new().checkpoint(5));
        let batch = sync.next().await.unwrap();
        assert_eq!(
            batch.changes,
            vec![SyncChange::Delete {
                key: "a".into(),
                version: 6
            }]
        );
    }

    #[test]
    fn view_changes_wait_is_capped_at_twenty_seconds() {
        let query = ViewChangesParams::after(1)
            .wait(Duration::from_secs(90))
            .to_query();
        assert!(query.contains(&("wait", "20".to_string())));
    }

    #[test]
    fn commit_chunks_respect_the_change_limit() {
        let entries: Vec<(String, Value)> =
            (0..1001).map(|i| (format!("k{i}"), json!(i))).collect();
        let sizes: Vec<usize> = commit_chunks(&entries).iter().map(|c| c.len()).collect();
        assert_eq!(sizes, [500, 500, 1]);
        assert!(commit_chunks(&[]).is_empty());
    }

    #[test]
    fn commit_chunks_respect_the_byte_limit() {
        let big = json!("x".repeat(100 * 1024));
        let entries = vec![
            ("a".to_string(), big.clone()),
            ("b".to_string(), big.clone()),
            ("c".to_string(), big),
        ];
        let sizes: Vec<usize> = commit_chunks(&entries).iter().map(|c| c.len()).collect();
        assert_eq!(sizes, [1, 1, 1]);
    }

    #[tokio::test]
    async fn upsert_rows_commits_at_the_feed_head_and_resets_an_older_epoch() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/event-feed"))
            .respond_with(ResponseTemplate::new(200).set_body_json(feed_body(41, 12)))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v1/read-models/assets"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(model_body(4, &json!({"epoch": 40, "position": 99}))),
            )
            .mount(&server)
            .await;
        Mock::given(method("PUT"))
            .and(path("/api/v1/read-models/assets"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"revision": 5})))
            .mount(&server)
            .await;

        upsert_read_model_rows(
            &core_client(&server),
            "assets",
            &[("BTC".to_string(), json!({"symbol": "BTC"}))],
        )
        .await
        .unwrap();

        let requests = server.received_requests().await.unwrap();
        let put = requests
            .iter()
            .find(|r| r.method.as_str() == "PUT")
            .expect("one commit");
        assert_eq!(
            body_json(put),
            json!({
                "expected_revision": 4,
                "checkpoint": {"epoch": 41, "position": 12},
                "ready": true,
                "reset": true,
                "changes": {"BTC": {"symbol": "BTC"}}
            })
        );
    }

    #[tokio::test]
    async fn upsert_rows_retries_a_version_conflict_once() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/event-feed"))
            .respond_with(ResponseTemplate::new(200).set_body_json(feed_body(41, 12)))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v1/read-models/assets"))
            .respond_with(ResponseTemplate::new(200).set_body_json(model_body(4, &Value::Null)))
            .mount(&server)
            .await;
        Mock::given(method("PUT"))
            .and(path("/api/v1/read-models/assets"))
            .respond_with(ResponseTemplate::new(409).set_body_json(json!({
                "error": "version_conflict", "expected_version": 4, "current_version": 5
            })))
            .mount(&server)
            .await;

        let err = upsert_read_model_rows(
            &core_client(&server),
            "assets",
            &[("BTC".to_string(), json!({}))],
        )
        .await
        .unwrap_err();
        assert!(matches!(err, Error::VersionConflict { .. }), "{err:?}");
        let puts = server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|r| r.method.as_str() == "PUT")
            .count();
        assert_eq!(puts, 2, "one commit plus one retry");
    }

    #[tokio::test]
    async fn read_model_row_matches_the_exact_key_only() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/read-models/assets"))
            .and(query_param("prefix", "BTC"))
            .and(query_param("limit", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "revision": 1, "rows": {"BTC-PERP": {"symbol": "BTC-PERP"}}, "next": null
            })))
            .mount(&server)
            .await;
        let row = read_model_row(&core_client(&server), "assets", "BTC")
            .await
            .unwrap();
        assert!(
            row.is_none(),
            "a longer key under the prefix is not a match"
        );
    }
}
