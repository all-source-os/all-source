//! The set of stores this server may read.
//!
//! Every store is named and opened at startup, and a tool call selects one by
//! name. A path never arrives in a request: the server would then read whatever
//! directory a caller could describe, and the access profile could not bound it.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use allsource_core::embedded::{Config, EmbeddedCore};
use anyhow::{Context, Result};

/// The name a request uses when it selects no store.
pub const DEFAULT_STORE: &str = "default";

pub struct StoreRegistry {
    stores: BTreeMap<String, Store>,
}

pub struct Store {
    pub path: PathBuf,
    pub core: EmbeddedCore,
}

impl StoreRegistry {
    /// Open every configured store read-only, `default` first.
    ///
    /// A store that cannot be opened fails startup rather than disappearing from
    /// the listing: a silently missing store reads as "this store holds nothing".
    pub async fn open(default_dir: &Path, extra: &[(String, PathBuf)]) -> Result<Self> {
        let mut stores = BTreeMap::new();
        stores.insert(
            DEFAULT_STORE.to_string(),
            Store {
                path: default_dir.to_path_buf(),
                core: open_read_only(default_dir).await?,
            },
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
                Store {
                    path: path.clone(),
                    core: open_read_only(path).await?,
                },
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
                        Store {
                            path: PathBuf::from(format!("<in-memory:{name}>")),
                            core,
                        },
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
