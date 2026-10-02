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

/// The name a request uses when it selects no store.
pub const DEFAULT_STORE: &str = "default";

pub struct StoreRegistry {
    stores: BTreeMap<String, Store>,
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
    /// A store that cannot be opened fails startup rather than disappearing from
    /// the listing: a silently missing store reads as "this store holds nothing".
    pub async fn open(
        default_dir: &Path,
        extra: &[(String, PathBuf)],
        refresh_every: Duration,
    ) -> Result<Self> {
        let mut stores = BTreeMap::new();
        stores.insert(
            DEFAULT_STORE.to_string(),
            Store::new(
                default_dir.to_path_buf(),
                open_read_only(default_dir).await?,
                refresh_every,
            ),
        );

        for (name, path) in extra {
            if name == DEFAULT_STORE {
                anyhow::bail!("store name '{DEFAULT_STORE}' is reserved for --data-dir");
            }
            if stores.contains_key(name) {
                anyhow::bail!("store '{name}' is configured twice");
            }
            stores.insert(
                name.clone(),
                Store::new(path.clone(), open_read_only(path).await?, refresh_every),
            );
        }

        Ok(Self { stores })
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
        }
    }

    /// Resolve the store a request selected, or the default when it selected none.
    pub fn get(&self, name: Option<&str>) -> Result<&Store> {
        let name = name.unwrap_or(DEFAULT_STORE);
        self.stores
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("not found: no store named '{name}' is configured"))
    }

    /// Every configured store, for `list_stores`.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Store)> {
        self.stores.iter()
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
