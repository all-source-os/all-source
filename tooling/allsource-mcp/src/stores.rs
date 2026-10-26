//! The set of stores this server may read.
//!
//! Every store is named and opened at startup, and a tool call selects one by
//! name. A path never arrives in a request: the server would then read whatever
//! directory a caller could describe, and the access profile could not bound it.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use allsource_core::embedded::{Config, EmbeddedCore};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use tokio::sync::Mutex;

use crate::remote::RemoteStore;

/// The name a request uses when it selects no store.
pub const DEFAULT_STORE: &str = "default";
/// The name `--remote` takes when `--data-dir` already holds `default`.
pub const REMOTE_STORE: &str = "remote";

pub struct StoreRegistry {
    stores: BTreeMap<String, Store>,
    remotes: BTreeMap<String, RemoteStore>,
}

/// A selected store: an embedded data directory or a gateway.
pub enum StoreRef<'a> {
    Embedded(&'a Store),
    Remote(&'a RemoteStore),
}

pub struct Store {
    pub path: PathBuf,
    pub core: EmbeddedCore,
    refresh_every: Duration,
    refresh: Mutex<RefreshStatus>,
}

#[derive(Default)]
struct RefreshStatus {
    last_attempt: Option<Instant>,
    refreshed_at: Option<DateTime<Utc>>,
    new_events_on_last_refresh: usize,
    last_error: Option<String>,
}

impl Store {
    pub fn new(path: PathBuf, core: EmbeddedCore, refresh_every: Duration) -> Self {
        Self {
            path,
            core,
            refresh_every,
            refresh: Mutex::new(RefreshStatus::default()),
        }
    }

    /// The core, caught up with its writer when the last refresh is older than
    /// `refresh_every`. A failed refresh serves what is already in memory and is
    /// reported by [`Store::refresh_context`].
    pub async fn fresh_core(&self) -> &EmbeddedCore {
        let mut status = self.refresh.lock().await;
        let due = status
            .last_attempt
            .is_none_or(|at| at.elapsed() >= self.refresh_every);
        if due {
            status.last_attempt = Some(Instant::now());
            match self.core.refresh().await {
                Ok(report) => {
                    status.refreshed_at = Some(Utc::now());
                    status.new_events_on_last_refresh = report.new_events;
                    status.last_error = None;
                }
                Err(error) => {
                    tracing::warn!(path = %self.path.display(), error = %error, "store refresh failed");
                    status.last_error = Some(error.to_string());
                }
            }
        }
        &self.core
    }

    /// When this store last caught up with its writer, so a stale `freshThrough`
    /// can be told apart from a quiet writer.
    pub async fn refresh_context(&self) -> Value {
        let status = self.refresh.lock().await;
        json!({
            "refreshedAt": status.refreshed_at.map(|at| at.to_rfc3339()),
            "newEventsOnLastRefresh": status.new_events_on_last_refresh,
            "refreshIntervalMs": u64::try_from(self.refresh_every.as_millis()).unwrap_or(u64::MAX),
            "lastRefreshError": status.last_error.is_some(),
        })
    }
}

impl StoreRegistry {
    /// Open every configured store read-only, `default` first.
    ///
    /// The remote store is `default` when there is no `--data-dir`, otherwise
    /// `remote`. A store that cannot be opened fails startup rather than
    /// disappearing from the listing: a silently missing store reads as "this
    /// store holds nothing".
    pub async fn open(
        default_dir: Option<&Path>,
        extra: &[(String, PathBuf)],
        refresh_every: Duration,
        remote: Option<RemoteStore>,
    ) -> Result<Self> {
        if default_dir.is_none() && remote.is_none() {
            anyhow::bail!("--data-dir or --remote is required");
        }
        let mut stores = BTreeMap::new();
        let mut remotes = BTreeMap::new();
        if let Some(default_dir) = default_dir {
            stores.insert(
                DEFAULT_STORE.to_string(),
                Store::new(
                    default_dir.to_path_buf(),
                    open_read_only(default_dir).await?,
                    refresh_every,
                ),
            );
        }
        if let Some(remote) = remote {
            let name = if stores.is_empty() {
                DEFAULT_STORE
            } else {
                REMOTE_STORE
            };
            remotes.insert(name.to_string(), remote);
        }

        for (name, path) in extra {
            if name == DEFAULT_STORE {
                anyhow::bail!("store name '{DEFAULT_STORE}' is reserved for --data-dir");
            }
            if stores.contains_key(name) || remotes.contains_key(name) {
                anyhow::bail!("store '{name}' is configured twice");
            }
            stores.insert(
                name.clone(),
                Store::new(path.clone(), open_read_only(path).await?, refresh_every),
            );
        }

        Ok(Self { stores, remotes })
    }

    /// Build a registry around already-open cores, for tests.
    #[cfg(test)]
    pub fn from_cores(cores: Vec<(&str, EmbeddedCore)>) -> Self {
        Self {
            stores: cores
                .into_iter()
                .map(|(name, core)| {
                    (
                        name.to_string(),
                        Store::new(
                            PathBuf::from(format!("<in-memory:{name}>")),
                            core,
                            Duration::ZERO,
                        ),
                    )
                })
                .collect(),
            remotes: BTreeMap::new(),
        }
    }

    /// Add a gateway store, for tests.
    #[cfg(test)]
    pub fn with_remote(mut self, name: &str, remote: RemoteStore) -> Self {
        self.remotes.insert(name.to_string(), remote);
        self
    }

    /// Resolve the embedded store a request selected, or the default when it
    /// selected none.
    pub fn get(&self, name: Option<&str>) -> Result<&Store> {
        match self.resolve(name)? {
            StoreRef::Embedded(store) => Ok(store),
            StoreRef::Remote(_) => anyhow::bail!(
                "unsupported: store '{}' is a --remote gateway; this tool reads an embedded data directory",
                name.unwrap_or(DEFAULT_STORE)
            ),
        }
    }

    /// Resolve the store a request selected, of either kind.
    pub fn resolve(&self, name: Option<&str>) -> Result<StoreRef<'_>> {
        let name = name.unwrap_or(DEFAULT_STORE);
        if let Some(store) = self.stores.get(name) {
            return Ok(StoreRef::Embedded(store));
        }
        self.remotes
            .get(name)
            .map(StoreRef::Remote)
            .ok_or_else(|| anyhow::anyhow!("not found: no store named '{name}' is configured"))
    }

    /// Every embedded store, for `list_stores` and `trace`.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Store)> {
        self.stores.iter()
    }

    /// Every gateway store, for `list_stores`.
    pub fn remotes(&self) -> impl Iterator<Item = (&String, &RemoteStore)> {
        self.remotes.iter()
    }
}

/// Open one data directory read-only.
async fn open_read_only(path: &Path) -> Result<EmbeddedCore> {
    EmbeddedCore::open(
        Config::builder()
            .data_dir(path)
            .single_tenant(false)
            .read_only(true)
            .build()?,
    )
    .await
    .with_context(|| format!("opening AllSource store at {}", path.display()))
}
