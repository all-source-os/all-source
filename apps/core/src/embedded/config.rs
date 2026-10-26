use crate::infrastructure::cluster::crdt::MergeStrategy;
use std::path::{Path, PathBuf};

/// Configuration for [`EmbeddedCore`](super::EmbeddedCore).
///
/// Created via `Config::builder()`. Defaults to in-memory, single-tenant mode.
#[derive(Debug, Clone)]
pub struct EmbeddedConfig {
    data_dir: Option<PathBuf>,
    wal_sync_on_write: bool,
    wal_fsync_interval_ms: Option<u64>,
    parquet_flush_interval_secs: u64,
    single_tenant: bool,
    node_id: Option<u32>,
    merge_strategies: Vec<(String, MergeStrategy)>,
    read_only: bool,
}

impl EmbeddedConfig {
    /// Begin building a new configuration.
    pub fn builder() -> ConfigBuilder {
        ConfigBuilder::default()
    }

    /// Directory for durable storage (WAL + Parquet).
    /// `None` means in-memory only.
    pub fn data_dir(&self) -> Option<&Path> {
        self.data_dir.as_deref()
    }

    /// Whether the WAL syncs to disk on every write.
    pub fn wal_sync_on_write(&self) -> bool {
        self.wal_sync_on_write
    }

    /// Interval-based fsync period in milliseconds.
    /// `None` means no background fsync task.
    pub fn wal_fsync_interval_ms(&self) -> Option<u64> {
        self.wal_fsync_interval_ms
    }

    /// Whether single-tenant mode is enabled.
    pub fn single_tenant(&self) -> bool {
        self.single_tenant
    }

    /// Node ID for HLC-based bidirectional sync.
    /// `None` disables sync capabilities (single-node mode).
    pub fn node_id(&self) -> Option<u32> {
        self.node_id
    }

    /// Per-event-type merge strategies for conflict resolution.
    pub fn merge_strategies(&self) -> &[(String, MergeStrategy)] {
        &self.merge_strategies
    }

    /// Configured Parquet flush interval in seconds.
    pub fn parquet_flush_interval_secs(&self) -> u64 {
        self.parquet_flush_interval_secs
    }

    /// Whether the store is opened read-only (replica mode). A read-only core
    /// replays the WAL + Parquet for reads but never truncates the WAL and
    /// rejects writes. See [`ConfigBuilder::read_only`].
    pub fn read_only(&self) -> bool {
        self.read_only
    }
}

/// Builder for [`EmbeddedConfig`](super::Config).
pub struct ConfigBuilder {
    data_dir: Option<PathBuf>,
    wal_sync_on_write: bool,
    wal_fsync_interval_ms: Option<u64>,
    parquet_flush_interval_secs: u64,
    single_tenant: bool,
    node_id: Option<u32>,
    merge_strategies: Vec<(String, MergeStrategy)>,
    read_only: bool,
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self {
            data_dir: None,
            wal_sync_on_write: true,
            wal_fsync_interval_ms: None,
            parquet_flush_interval_secs: 300,
            single_tenant: true,
            node_id: None,
            merge_strategies: Vec::new(),
            read_only: false,
        }
    }
}

impl ConfigBuilder {
    /// Set the directory where WAL and Parquet files are stored.
    /// When not called, EmbeddedCore runs in-memory (no durability).
    pub fn data_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.data_dir = Some(path.into());
        self
    }

    /// Control whether WAL syncs to disk on every write.
    /// Default: `true`. Set `false` for higher throughput at the cost of
    /// potential data loss on crash within the last sync window.
    pub fn wal_sync_on_write(mut self, sync: bool) -> Self {
        self.wal_sync_on_write = sync;
        self
    }

    /// Set the interval for background coalesced fsync in milliseconds.
    ///
    /// When set, a background task flushes and fsyncs the WAL every `ms`
    /// milliseconds instead of on every write. This gives near-zero write
    /// latency with a bounded data-loss window of at most `ms` milliseconds.
    ///
    /// Automatically disables per-write `sync_on_write` to prevent double-fsync.
    /// Default: `None` (no background fsync task).
    pub fn wal_fsync_interval_ms(mut self, ms: u64) -> Self {
        self.wal_fsync_interval_ms = Some(ms);
        self
    }

    /// How often Parquet files are flushed (in seconds). Default: 300.
    pub fn parquet_flush_interval_secs(mut self, secs: u64) -> Self {
        self.parquet_flush_interval_secs = secs;
        self
    }

    /// Enable or disable single-tenant mode. Default: `true`.
    /// In single-tenant mode, all events automatically use "default" as tenant_id.
    pub fn single_tenant(mut self, enabled: bool) -> Self {
        self.single_tenant = enabled;
        self
    }

    /// Set a node ID for HLC-based bidirectional sync.
    /// Each instance in a sync group must have a unique node ID.
    /// When not set, sync capabilities are disabled (single-node mode).
    pub fn node_id(mut self, id: u32) -> Self {
        self.node_id = Some(id);
        self
    }

    /// Register a per-event-type merge strategy for conflict resolution.
    ///
    /// The `prefix` is matched against event types: `"config."` matches
    /// `"config.updated"`, `"config.deleted"`, etc. The longest matching
    /// prefix wins. Unmatched types default to `AppendOnly`.
    pub fn merge_strategy(mut self, prefix: impl Into<String>, strategy: MergeStrategy) -> Self {
        self.merge_strategies.push((prefix.into(), strategy));
        self
    }

    /// Open the store read-only (replica mode). Default: `false`.
    ///
    /// A read-only core replays the WAL + Parquet into memory at boot so it can
    /// serve reads, but it never truncates the WAL (which would corrupt the
    /// owning writer's log — issue #201) and rejects every write with
    /// `AllSourceError::ReadOnly`. Set this when another process already owns
    /// the data directory.
    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    /// Build the configuration. Returns `Result` for forward compatibility.
    pub fn build(self) -> crate::error::Result<EmbeddedConfig> {
        Ok(EmbeddedConfig {
            data_dir: self.data_dir,
            wal_sync_on_write: self.wal_sync_on_write,
            wal_fsync_interval_ms: self.wal_fsync_interval_ms,
            parquet_flush_interval_secs: self.parquet_flush_interval_secs,
            single_tenant: self.single_tenant,
            node_id: self.node_id,
            merge_strategies: self.merge_strategies,
            read_only: self.read_only,
        })
    }
}
