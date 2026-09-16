use crate::{
    error::{AllSourceError, Result},
    infrastructure::persistence::{cold_tier::ArchiveTarget, storage::ParquetStorage},
};
use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, path::PathBuf, sync::Arc, time::Duration};

/// Manages Parquet file compaction for optimal storage and query performance.
///
/// Step 4 of the sustainable data strategy moved compaction from a
/// global-stream pass to a per-tenant pass. Each invocation iterates
/// the tenants discovered under `<storage_dir>/<tenant>/...` and
/// emits a `snapshot.<tenant>.<from>-<to>.parquet` per qualifying
/// chunk. Snapshot files are written atomically (tmp + rename) and
/// their constituent raw files are removed only after the rename
/// succeeds — so a mid-compaction crash leaves data intact.
pub struct CompactionManager {
    /// Directory where Parquet files are stored
    storage_dir: PathBuf,

    /// Configuration
    config: CompactionConfig,

    /// Statistics
    stats: Arc<RwLock<CompactionStats>>,

    /// Last compaction time
    last_compaction: Arc<RwLock<Option<DateTime<Utc>>>>,
}

/// Filename prefix that marks a file as already-compacted output.
/// Excluded from the input set when picking compaction candidates
/// (we don't re-compact a snapshot until the snapshot itself
/// triggers the strategy criteria from a future commit).
const SNAPSHOT_PREFIX: &str = "snapshot.";

#[derive(Debug, Clone)]
pub struct CompactionConfig {
    /// Minimum number of files to trigger compaction
    pub min_files_to_compact: usize,

    /// Target size for compacted files (in bytes)
    pub target_file_size: usize,

    /// Maximum size for a single compacted file (in bytes)
    pub max_file_size: usize,

    /// Minimum file size to consider for compaction (small files)
    pub small_file_threshold: usize,

    /// Time interval between automatic compactions (in seconds)
    pub compaction_interval_seconds: u64,

    /// Enable automatic background compaction
    pub auto_compact: bool,

    /// Compaction strategy
    pub strategy: CompactionStrategy,

    /// Per-tenant retention TTLs (Step 5 of the sustainable data
    /// strategy). Applied during the same compaction pass — events
    /// older than `now - ttl` for that tenant are dropped from the
    /// snapshot output and the originals are removed. Default
    /// honors the bead: tenant `system` keeps 30 days; everyone
    /// else keeps forever.
    pub retention: RetentionConfig,

    /// Optional cold-tier archive. When set, events that would be
    /// dropped by retention are archived to this target BEFORE the
    /// originals are deleted. A failed archive aborts the
    /// compaction pass — originals stay on disk and the next run
    /// retries. Default `None` preserves the pre-cold-tier behavior:
    /// retention deletes outright. See
    /// `infrastructure::persistence::cold_tier`.
    pub archive: Option<Arc<dyn ArchiveTarget>>,
}

/// Per-tenant retention configuration. Look up a TTL via
/// `ttl_for(tenant_id)`; `None` means "keep forever" for that
/// tenant.
///
/// The default rule (from the bead): the CP heartbeat tenant
/// (`system`) defaults to 30 days. The CP emits ~69k heartbeat
/// events/day; without retention this grows unbounded for data
/// that has no audit value past the dashboard window. Other
/// tenants default to no TTL — user data stays put unless the
/// owner opts in.
///
/// Per-tenant overrides win over `default_ttl`; "no entry" falls
/// back to `default_ttl`.
#[derive(Debug, Clone)]
pub struct RetentionConfig {
    /// Default TTL when no per-tenant override exists. `None` = keep forever.
    pub default_ttl: Option<Duration>,
    /// Per-tenant overrides. `Some(None)` would mean "explicitly no
    /// TTL"; the API uses `Option<Duration>` directly so an entry
    /// can record an explicit "keep forever" decision distinct
    /// from "no entry".
    pub per_tenant_ttl: HashMap<String, Option<Duration>>,
}

impl Default for RetentionConfig {
    fn default() -> Self {
        let mut per_tenant_ttl = HashMap::new();
        per_tenant_ttl.insert("system".to_string(), Some(Duration::from_hours(30 * 24)));
        Self {
            default_ttl: None,
            per_tenant_ttl,
        }
    }
}

impl RetentionConfig {
    /// Effective TTL for `tenant_id`. Returns `None` if the tenant
    /// has no TTL (keep forever).
    ///
    /// Lookup order:
    /// 1. Per-tenant entry → that value (whether Some or explicit None).
    /// 2. No entry → fall back to `default_ttl`.
    pub fn ttl_for(&self, tenant_id: &str) -> Option<Duration> {
        match self.per_tenant_ttl.get(tenant_id) {
            Some(v) => *v,
            None => self.default_ttl,
        }
    }

    /// Override the TTL for a specific tenant. Use `None` to mean
    /// "keep forever for this tenant".
    pub fn set(&mut self, tenant_id: &str, ttl: Option<Duration>) {
        self.per_tenant_ttl.insert(tenant_id.to_string(), ttl);
    }
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            min_files_to_compact: 3,
            target_file_size: 128 * 1024 * 1024,    // 128 MB
            max_file_size: 256 * 1024 * 1024,       // 256 MB
            small_file_threshold: 10 * 1024 * 1024, // 10 MB
            compaction_interval_seconds: 3600,      // 1 hour
            auto_compact: true,
            strategy: CompactionStrategy::SizeBased,
            retention: RetentionConfig::default(),
            archive: None,
        }
    }
}

impl CompactionConfig {
    /// Build a config from the relevant env vars:
    /// - `ALLSOURCE_SNAPSHOT_INTERVAL_SECONDS`: per-pass cadence
    ///   (default 3600).
    /// - `ALLSOURCE_RETENTION_SYSTEM_DAYS`: TTL for the `system`
    ///   tenant in days (default 30).
    ///
    /// Unparseable values log a warning and fall back to defaults
    /// — boot doesn't fail.
    pub fn from_env() -> Self {
        let config = Self::from_env_vars(
            std::env::var("ALLSOURCE_SNAPSHOT_INTERVAL_SECONDS").ok(),
            std::env::var("ALLSOURCE_RETENTION_SYSTEM_DAYS").ok(),
        );
        config.with_cold_storage_url(std::env::var("ALLSOURCE_COLD_STORAGE_URL").ok())
    }

    /// Attach a cold-tier archive from `ALLSOURCE_COLD_STORAGE_URL`
    /// (`s3://bucket/prefix`). Unset or empty leaves `archive` as `None`,
    /// which is the default: retention deletes without archiving.
    ///
    /// A URL that is set but unusable is a hard error, not a warning. Every
    /// other env var here falls back to a default because a wrong interval
    /// costs a slow pass; this one decides whether events are copied somewhere
    /// before compaction deletes the originals, so degrading to "no archive"
    /// would turn an operator's typo into silent data loss.
    #[allow(unused_variables, unused_mut)]
    pub fn with_cold_storage_url(mut self, url: Option<String>) -> Self {
        let Some(url) = url.filter(|u| !u.trim().is_empty()) else {
            return self;
        };

        #[cfg(feature = "cold-tier-s3")]
        {
            match super::cold_tier_s3::S3Archive::from_url(url.trim()) {
                Ok(archive) => {
                    tracing::info!(
                        target = %super::cold_tier::ArchiveTarget::description(&archive),
                        "cold-tier archive enabled"
                    );
                    self.archive = Some(std::sync::Arc::new(archive));
                }
                Err(e) => panic!("ALLSOURCE_COLD_STORAGE_URL={url:?} is set but unusable: {e}"),
            }
        }

        #[cfg(not(feature = "cold-tier-s3"))]
        panic!(
            "ALLSOURCE_COLD_STORAGE_URL={url:?} is set but this binary was built without the \
             `cold-tier-s3` feature, so nothing would be archived before retention deletes it. \
             Rebuild with --features cold-tier-s3, or unset the variable."
        );

        #[cfg(feature = "cold-tier-s3")]
        self
    }

    /// Testable variant of `from_env`. Production calls `from_env`;
    /// tests pass explicit values.
    pub fn from_env_vars(
        interval_var: Option<String>,
        system_retention_days_var: Option<String>,
    ) -> Self {
        let mut config = Self::default();
        if let Some(s) = interval_var.filter(|s| !s.is_empty()) {
            match s.parse::<u64>() {
                Ok(v) => config.compaction_interval_seconds = v,
                Err(e) => {
                    tracing::warn!(
                        "ALLSOURCE_SNAPSHOT_INTERVAL_SECONDS={s:?} could not be parsed as \
                         u64: {e}; defaulting to {}s",
                        config.compaction_interval_seconds
                    );
                }
            }
        }
        if let Some(s) = system_retention_days_var.filter(|s| !s.is_empty()) {
            match s.parse::<u64>() {
                Ok(days) => {
                    config
                        .retention
                        .set("system", Some(Duration::from_secs(days * 24 * 3600)));
                }
                Err(e) => {
                    tracing::warn!(
                        "ALLSOURCE_RETENTION_SYSTEM_DAYS={s:?} could not be parsed as u64: \
                         {e}; defaulting to 30 days for tenant=system"
                    );
                }
            }
        }
        config
    }

    /// Backwards-compatible single-arg variant for existing
    /// callers that only set the snapshot interval.
    pub fn from_env_var(interval_var: Option<String>) -> Self {
        Self::from_env_vars(interval_var, None)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CompactionStrategy {
    /// Compact based on file size (default)
    SizeBased,
    /// Compact based on file age
    TimeBased,
    /// Compact all files into one
    FullCompaction,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CompactionStats {
    pub total_compactions: u64,
    pub total_files_compacted: u64,
    pub total_bytes_before: u64,
    pub total_bytes_after: u64,
    pub total_events_compacted: u64,
    pub last_compaction_duration_ms: u64,
    pub space_saved_bytes: u64,
}

/// Information about a Parquet file candidate for compaction
#[derive(Debug, Clone)]
struct FileInfo {
    path: PathBuf,
    size: u64,
    created: DateTime<Utc>,
}

impl CompactionManager {
    /// Create a new compaction manager
    pub fn new(storage_dir: impl Into<PathBuf>, config: CompactionConfig) -> Self {
        let storage_dir = storage_dir.into();

        tracing::info!(
            "✅ Compaction manager initialized at: {}",
            storage_dir.display()
        );

        Self {
            storage_dir,
            config,
            stats: Arc::new(RwLock::new(CompactionStats::default())),
            last_compaction: Arc::new(RwLock::new(None)),
        }
    }

    /// List all Parquet files in the storage directory
    fn list_parquet_files(&self) -> Result<Vec<FileInfo>> {
        let entries = fs::read_dir(&self.storage_dir).map_err(|e| {
            AllSourceError::StorageError(format!("Failed to read storage directory: {e}"))
        })?;

        let mut files = Vec::new();

        for entry in entries {
            let entry = entry.map_err(|e| {
                AllSourceError::StorageError(format!("Failed to read directory entry: {e}"))
            })?;

            let path = entry.path();
            if let Some(ext) = path.extension()
                && ext == "parquet"
            {
                let metadata = entry.metadata().map_err(|e| {
                    AllSourceError::StorageError(format!("Failed to read file metadata: {e}"))
                })?;

                let size = metadata.len();
                let created = metadata
                    .created()
                    .ok()
                    .and_then(|t| {
                        t.duration_since(std::time::UNIX_EPOCH).ok().map(|d| {
                            DateTime::from_timestamp(d.as_secs() as i64, 0).unwrap_or_else(Utc::now)
                        })
                    })
                    .unwrap_or_else(Utc::now);

                files.push(FileInfo {
                    path,
                    size,
                    created,
                });
            }
        }

        // Sort by creation time (oldest first)
        files.sort_by_key(|f| f.created);

        Ok(files)
    }

    /// Identify files that should be compacted based on strategy
    fn select_files_for_compaction(&self, files: &[FileInfo]) -> Vec<FileInfo> {
        match self.config.strategy {
            CompactionStrategy::SizeBased => self.select_small_files(files),
            CompactionStrategy::TimeBased => self.select_old_files(files),
            CompactionStrategy::FullCompaction => files.to_vec(),
        }
    }

    /// Select small files for compaction
    fn select_small_files(&self, files: &[FileInfo]) -> Vec<FileInfo> {
        let small_files: Vec<FileInfo> = files
            .iter()
            .filter(|f| f.size < self.config.small_file_threshold as u64)
            .cloned()
            .collect();

        // Only compact if we have enough small files
        if small_files.len() >= self.config.min_files_to_compact {
            small_files
        } else {
            Vec::new()
        }
    }

    /// Select old files for time-based compaction
    fn select_old_files(&self, files: &[FileInfo]) -> Vec<FileInfo> {
        let now = Utc::now();
        let age_threshold = chrono::Duration::hours(24); // Files older than 24 hours

        let old_files: Vec<FileInfo> = files
            .iter()
            .filter(|f| now - f.created > age_threshold)
            .cloned()
            .collect();

        if old_files.len() >= self.config.min_files_to_compact {
            old_files
        } else {
            Vec::new()
        }
    }

    /// Check if compaction should run
    #[cfg_attr(feature = "hotpath", hotpath::measure)]
    pub fn should_compact(&self) -> bool {
        if !self.config.auto_compact {
            return false;
        }

        let last = self.last_compaction.read();
        match *last {
            None => true, // Never compacted
            Some(last_time) => {
                let elapsed = (Utc::now() - last_time).num_seconds();
                elapsed >= self.config.compaction_interval_seconds as i64
            }
        }
    }

    /// Perform compaction across every discovered tenant.
    ///
    /// Iterates the tenants under `<storage_dir>/<tenant>/...`, calls
    /// `compact_tenant` for each, and aggregates the results. Step 4
    /// of the sustainable data strategy: per-tenant compaction
    /// instead of global, keyed off the per-tenant directory tree
    /// Step 1 introduced.
    ///
    /// Errors compacting one tenant are logged but don't abort the
    /// pass — other tenants still get compacted. The aggregate
    /// result reflects what actually completed.
    #[cfg_attr(feature = "hotpath", hotpath::measure)]
    pub fn compact(&self) -> Result<CompactionResult> {
        let start_time = std::time::Instant::now();
        tracing::info!("🔄 Starting per-tenant compaction sweep...");

        let tenants = self.discover_tenants()?;
        if tenants.is_empty() {
            tracing::debug!("No tenants found under {}", self.storage_dir.display());
            return Ok(CompactionResult::default());
        }

        let mut aggregate = CompactionResult::default();
        for tenant in &tenants {
            match self.compact_tenant(tenant) {
                Ok(r) => {
                    aggregate.files_compacted += r.files_compacted;
                    aggregate.bytes_before += r.bytes_before;
                    aggregate.bytes_after += r.bytes_after;
                    aggregate.events_compacted += r.events_compacted;
                }
                Err(e) => {
                    tracing::error!(
                        tenant_id = %tenant,
                        "compact_tenant failed: {e}"
                    );
                }
            }
        }
        aggregate.duration_ms = start_time.elapsed().as_millis() as u64;

        if aggregate.files_compacted > 0 {
            let mut stats = self.stats.write();
            stats.total_compactions += 1;
            stats.total_files_compacted += aggregate.files_compacted as u64;
            stats.total_bytes_before += aggregate.bytes_before;
            stats.total_bytes_after += aggregate.bytes_after;
            stats.total_events_compacted += aggregate.events_compacted as u64;
            stats.last_compaction_duration_ms = aggregate.duration_ms;
            stats.space_saved_bytes += aggregate.bytes_before.saturating_sub(aggregate.bytes_after);
        }
        *self.last_compaction.write() = Some(Utc::now());

        tracing::info!(
            "✅ Compaction sweep complete: {} files → 1 snapshot per tenant, \
             {:.2} MB → {:.2} MB, {} events, {} tenants in {}ms",
            aggregate.files_compacted,
            aggregate.bytes_before as f64 / (1024.0 * 1024.0),
            aggregate.bytes_after as f64 / (1024.0 * 1024.0),
            aggregate.events_compacted,
            tenants.len(),
            aggregate.duration_ms
        );

        Ok(aggregate)
    }

    /// Compact one tenant's raw event files into a single snapshot
    /// file under that tenant's partition.
    ///
    /// Per-tenant pipeline:
    /// 1. List `<storage>/<tenant>/...*.parquet` excluding existing
    ///    `snapshot.*` files.
    /// 2. Apply the configured strategy (size / time / full) to
    ///    pick candidate raw files.
    /// 3. If enough candidates: read events, sort by timestamp,
    ///    atomically write `snapshot.<tenant>.<from>-<to>.parquet`
    ///    via `ParquetStorage::write_atomic_parquet`.
    /// 4. After the snapshot rename succeeds, delete the
    ///    constituent raw files. Crash between snapshot and delete
    ///    leaves both on disk; the dedupe in `append_loaded_event`
    ///    keeps memory consistent on next load. A future commit can
    ///    record the constituent file list in the snapshot's
    ///    metadata and let a cleanup pass finish the deletion.
    pub fn compact_tenant(&self, tenant_id: &str) -> Result<CompactionResult> {
        let start_time = std::time::Instant::now();

        // 1. List raw files for this tenant.
        let storage = ParquetStorage::new(&self.storage_dir)?;
        let all_files = storage.list_parquet_files_for_tenant(tenant_id)?;
        let raw_files: Vec<FileInfo> = all_files
            .into_iter()
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_none_or(|n| !n.starts_with(SNAPSHOT_PREFIX))
            })
            .filter_map(|p| {
                let metadata = fs::metadata(&p).ok()?;
                let size = metadata.len();
                let created = metadata
                    .created()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .and_then(|d| DateTime::from_timestamp(d.as_secs() as i64, 0))
                    .unwrap_or_else(Utc::now);
                Some(FileInfo {
                    path: p,
                    size,
                    created,
                })
            })
            .collect();

        // 2. Strategy filter.
        let candidates = self.select_files_for_compaction(&raw_files);
        if candidates.is_empty() {
            tracing::debug!(
                tenant_id = tenant_id,
                strategy = ?self.config.strategy,
                "no files meet compaction criteria"
            );
            return Ok(CompactionResult::default());
        }

        let bytes_before: u64 = candidates.iter().map(|f| f.size).sum();
        tracing::info!(
            tenant_id = tenant_id,
            files = candidates.len(),
            mib = bytes_before as f64 / (1024.0 * 1024.0),
            "compacting tenant"
        );

        // 3. Read events from candidates. We deliberately read each
        // file individually (not via load_events_for_tenant) so we
        // only pick up the candidate set — concurrent writes that
        // produced new files since step 1 are skipped, deferred to
        // the next interval (AC #6).
        let mut events = Vec::new();
        for fi in &candidates {
            // Recover tenant from path so loaded events keep
            // their identity (Step 1's path-as-tenant-source).
            let event_tenant = match fi.path.strip_prefix(&self.storage_dir).ok() {
                Some(rel) => rel
                    .components()
                    .next()
                    .and_then(|c| match c {
                        std::path::Component::Normal(t) => Some(t.to_string_lossy().into_owned()),
                        _ => None,
                    })
                    .unwrap_or_else(|| "default".to_string()),
                None => "default".to_string(),
            };
            match storage.load_events_from_file_path(&fi.path, &event_tenant) {
                Ok(mut e) => events.append(&mut e),
                Err(e) => {
                    tracing::error!(
                        file = %fi.path.display(),
                        "failed to read parquet file for compaction: {e}"
                    );
                }
            }
        }

        if events.is_empty() {
            tracing::warn!(
                tenant_id = tenant_id,
                "candidate files had no readable events; skipping snapshot"
            );
            return Ok(CompactionResult::default());
        }

        // Apply retention (Step 5). Drop events older than the
        // tenant's TTL before they get rewritten into the
        // snapshot. The originals get deleted at the end of the
        // happy path either way, so dropped events go away with
        // the same crash-safe guarantee as the rest of the
        // compaction pipeline (snapshot rename completes BEFORE
        // any original is deleted; AC #6).
        //
        // Cold-tier (sustainability): when an `archive` target is
        // configured, dropped events are written to it BEFORE this
        // function deletes any original file. A failed archive
        // returns `Err`, originals stay on disk, and the next
        // compaction pass retries — same crash-safety contract as
        // the snapshot path. Without an archive, retention behaves
        // exactly as before (delete outright).
        let dropped_by_retention = if let Some(ttl) = self.config.retention.ttl_for(tenant_id) {
            let cutoff = Utc::now()
                - chrono::Duration::from_std(ttl).unwrap_or_else(|_| chrono::Duration::zero());
            let before = events.len();

            // Partition: drained = events past TTL, kept = events to keep.
            // Using drain_filter would be simpler but it's unstable;
            // partition + reassign keeps events: Vec<Event> with the
            // kept slice in original order.
            let (drained, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut events)
                .into_iter()
                .partition(|e| e.timestamp < cutoff);
            events = kept;
            let dropped = before - events.len();

            if dropped > 0 {
                tracing::info!(
                    retention_tenant = tenant_id,
                    dropped = dropped,
                    kept = events.len(),
                    cutoff = %cutoff.to_rfc3339(),
                    ttl_secs = ttl.as_secs(),
                    "retention: dropped events older than TTL"
                );

                if let Some(archive) = self.config.archive.as_ref() {
                    let from = drained
                        .iter()
                        .map(|e| e.timestamp)
                        .min()
                        .expect("dropped > 0 guarantees non-empty drained");
                    let to = drained
                        .iter()
                        .map(|e| e.timestamp)
                        .max()
                        .expect("dropped > 0 guarantees non-empty drained");
                    archive.archive(tenant_id, from, to, &drained)?;
                    tracing::info!(
                        retention_tenant = tenant_id,
                        archived_to = %archive.description(),
                        archived = drained.len(),
                        "retention: dropped events archived to cold tier"
                    );
                }
            }
            dropped
        } else {
            0
        };

        // Edge case: every event aged out. Skip the snapshot
        // write and just delete originals — the data is gone by
        // design. Delete-without-snapshot is safe here because
        // every event we'd have written to the snapshot was
        // already past its TTL, and the input files live under
        // the tenant's partition with nothing else relying on
        // them.
        if events.is_empty() {
            tracing::info!(
                tenant_id = tenant_id,
                files_dropped = candidates.len(),
                events_dropped = dropped_by_retention,
                "retention: every event aged out — deleting originals without snapshot"
            );
            for fi in &candidates {
                if let Err(e) = fs::remove_file(&fi.path) {
                    tracing::error!(
                        file = %fi.path.display(),
                        "failed to remove fully-aged raw file: {e}"
                    );
                }
            }
            return Ok(CompactionResult {
                files_compacted: candidates.len(),
                bytes_before,
                bytes_after: 0,
                events_compacted: 0,
                duration_ms: start_time.elapsed().as_millis() as u64,
            });
        }

        events.sort_by_key(|e| e.timestamp);
        let from = events.first().expect("non-empty checked above").timestamp;
        let to = events.last().expect("non-empty checked above").timestamp;

        // Filename: snapshot.<tenant>.<from>-<to> with filesystem-safe
        // ISO-basic timestamps (no colons).
        let file_stem = format!(
            "snapshot.{tenant_id}.{}-{}",
            format_iso_basic(from),
            format_iso_basic(to)
        );
        let snapshot_path = storage.write_atomic_parquet(tenant_id, &file_stem, &events)?;
        let bytes_after = fs::metadata(&snapshot_path).map_or(0, |m| m.len());

        // 4. Delete originals AFTER snapshot is durably renamed.
        // AC #6: a snapshot-write failure short-circuits via the
        // `?` above, so originals stay on disk and events remain
        // queryable until the next successful pass.
        for fi in &candidates {
            if let Err(e) = fs::remove_file(&fi.path) {
                tracing::error!(
                    file = %fi.path.display(),
                    "failed to remove pre-snapshot raw file: {e}"
                );
            }
        }

        let duration_ms = start_time.elapsed().as_millis() as u64;
        tracing::info!(
            tenant_id = tenant_id,
            files_compacted = candidates.len(),
            events = events.len(),
            dropped_by_retention = dropped_by_retention,
            mib_before = bytes_before as f64 / (1024.0 * 1024.0),
            mib_after = bytes_after as f64 / (1024.0 * 1024.0),
            duration_ms = duration_ms,
            "tenant compaction complete"
        );

        Ok(CompactionResult {
            files_compacted: candidates.len(),
            bytes_before,
            bytes_after,
            events_compacted: events.len(),
            duration_ms,
        })
    }

    /// Discover tenant ids by scanning `<storage_dir>/<X>/` for
    /// directories. The migration tool's flat-layout files at the
    /// root are not picked up — Step 1 #3's migration moves them
    /// under `default/`.
    fn discover_tenants(&self) -> Result<Vec<String>> {
        let Ok(entries) = fs::read_dir(&self.storage_dir) else {
            return Ok(Vec::new());
        };
        let mut tenants: Vec<String> = entries
            .filter_map(std::result::Result::ok)
            .filter_map(|entry| {
                let ft = entry.file_type().ok()?;
                if !ft.is_dir() {
                    return None;
                }
                let name = entry.file_name().to_string_lossy().into_owned();
                // Skip the system metadata subtree (Core's own
                // event-sourced repos) and any hidden folders.
                if name.starts_with('.') || name == "__system" {
                    return None;
                }
                Some(name)
            })
            .collect();
        tenants.sort();
        Ok(tenants)
    }

    /// Get compaction statistics
    pub fn stats(&self) -> CompactionStats {
        (*self.stats.read()).clone()
    }

    /// Get configuration
    pub fn config(&self) -> &CompactionConfig {
        &self.config
    }

    /// Trigger manual compaction
    #[cfg_attr(feature = "hotpath", hotpath::measure)]
    pub fn compact_now(&self) -> Result<CompactionResult> {
        tracing::info!("Manual compaction triggered");
        self.compact()
    }
}

/// Result of a compaction operation
#[derive(Debug, Clone, Default, Serialize)]
pub struct CompactionResult {
    pub files_compacted: usize,
    pub bytes_before: u64,
    pub bytes_after: u64,
    pub events_compacted: usize,
    pub duration_ms: u64,
}

/// Format a UTC timestamp as a filename-safe ISO-8601 basic-form
/// string: `2026-04-27T134567Z` — no colons, no fractional second.
/// Used for the `<from>-<to>` portion of snapshot filenames so
/// they're portable across filesystems. `pub(super)` so the
/// cold-tier archive can reuse the same naming convention.
pub(super) fn format_iso_basic(t: DateTime<Utc>) -> String {
    t.format("%Y-%m-%dT%H%M%SZ").to_string()
}

/// Background compaction task
pub struct CompactionTask {
    manager: Arc<CompactionManager>,
    interval: Duration,
}

impl CompactionTask {
    /// Create a new background compaction task
    pub fn new(manager: Arc<CompactionManager>, interval_seconds: u64) -> Self {
        Self {
            manager,
            interval: Duration::from_secs(interval_seconds),
        }
    }

    /// Run the compaction task in a loop
    #[cfg_attr(feature = "hotpath", hotpath::measure)]
    pub async fn run(self) {
        let mut interval = tokio::time::interval(self.interval);

        loop {
            interval.tick().await;

            if self.manager.should_compact() {
                tracing::debug!("Auto-compaction check triggered");

                match self.manager.compact() {
                    Ok(result) => {
                        if result.files_compacted > 0 {
                            tracing::info!(
                                "Auto-compaction succeeded: {} files, {:.2} MB saved",
                                result.files_compacted,
                                (result.bytes_before - result.bytes_after) as f64
                                    / (1024.0 * 1024.0)
                            );
                        }
                    }
                    Err(e) => {
                        tracing::error!("Auto-compaction failed: {}", e);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_compaction_manager_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config = CompactionConfig::default();
        let manager = CompactionManager::new(temp_dir.path(), config);

        assert_eq!(manager.stats().total_compactions, 0);
    }

    #[test]
    fn test_should_compact() {
        let temp_dir = TempDir::new().unwrap();
        let config = CompactionConfig {
            auto_compact: true,
            compaction_interval_seconds: 1,
            ..Default::default()
        };
        let manager = CompactionManager::new(temp_dir.path(), config);

        // Should compact on first check (never compacted)
        assert!(manager.should_compact());
    }

    #[test]
    fn test_file_selection_size_based() {
        let temp_dir = TempDir::new().unwrap();
        let config = CompactionConfig {
            small_file_threshold: 1024 * 1024, // 1 MB
            min_files_to_compact: 2,
            strategy: CompactionStrategy::SizeBased,
            ..Default::default()
        };
        let manager = CompactionManager::new(temp_dir.path(), config);

        let files = vec![
            FileInfo {
                path: PathBuf::from("small1.parquet"),
                size: 500_000, // 500 KB
                created: Utc::now(),
            },
            FileInfo {
                path: PathBuf::from("small2.parquet"),
                size: 600_000, // 600 KB
                created: Utc::now(),
            },
            FileInfo {
                path: PathBuf::from("large.parquet"),
                size: 10_000_000, // 10 MB
                created: Utc::now(),
            },
        ];

        let selected = manager.select_files_for_compaction(&files);
        assert_eq!(selected.len(), 2); // Only the 2 small files
    }

    #[test]
    fn test_default_compaction_config() {
        let config = CompactionConfig::default();
        assert_eq!(config.min_files_to_compact, 3);
        assert_eq!(config.target_file_size, 128 * 1024 * 1024);
        assert_eq!(config.max_file_size, 256 * 1024 * 1024);
        assert_eq!(config.small_file_threshold, 10 * 1024 * 1024);
        assert_eq!(config.compaction_interval_seconds, 3600);
        assert!(config.auto_compact);
        assert_eq!(config.strategy, CompactionStrategy::SizeBased);
    }

    #[test]
    fn test_should_compact_disabled() {
        let temp_dir = TempDir::new().unwrap();
        let config = CompactionConfig {
            auto_compact: false,
            ..Default::default()
        };
        let manager = CompactionManager::new(temp_dir.path(), config);

        assert!(!manager.should_compact());
    }

    #[test]
    fn test_compact_empty_directory() {
        let temp_dir = TempDir::new().unwrap();
        let config = CompactionConfig::default();
        let manager = CompactionManager::new(temp_dir.path(), config);

        let result = manager.compact().unwrap();
        assert_eq!(result.files_compacted, 0);
        assert_eq!(result.bytes_before, 0);
        assert_eq!(result.bytes_after, 0);
        assert_eq!(result.events_compacted, 0);
    }

    #[test]
    fn test_compact_now() {
        let temp_dir = TempDir::new().unwrap();
        let config = CompactionConfig::default();
        let manager = CompactionManager::new(temp_dir.path(), config);

        let result = manager.compact_now().unwrap();
        assert_eq!(result.files_compacted, 0);
    }

    #[test]
    fn test_get_config() {
        let temp_dir = TempDir::new().unwrap();
        let config = CompactionConfig {
            min_files_to_compact: 5,
            ..Default::default()
        };
        let manager = CompactionManager::new(temp_dir.path(), config);

        assert_eq!(manager.config().min_files_to_compact, 5);
    }

    #[test]
    fn test_get_stats() {
        let temp_dir = TempDir::new().unwrap();
        let config = CompactionConfig::default();
        let manager = CompactionManager::new(temp_dir.path(), config);

        let stats = manager.stats();
        assert_eq!(stats.total_compactions, 0);
        assert_eq!(stats.total_files_compacted, 0);
        assert_eq!(stats.total_bytes_before, 0);
        assert_eq!(stats.total_bytes_after, 0);
        assert_eq!(stats.total_events_compacted, 0);
        assert_eq!(stats.last_compaction_duration_ms, 0);
        assert_eq!(stats.space_saved_bytes, 0);
    }

    #[test]
    fn test_file_selection_not_enough_small_files() {
        let temp_dir = TempDir::new().unwrap();
        let config = CompactionConfig {
            small_file_threshold: 1024 * 1024,
            min_files_to_compact: 3, // Need 3 files
            strategy: CompactionStrategy::SizeBased,
            ..Default::default()
        };
        let manager = CompactionManager::new(temp_dir.path(), config);

        let files = vec![
            FileInfo {
                path: PathBuf::from("small1.parquet"),
                size: 500_000,
                created: Utc::now(),
            },
            FileInfo {
                path: PathBuf::from("small2.parquet"),
                size: 600_000,
                created: Utc::now(),
            },
        ];

        let selected = manager.select_files_for_compaction(&files);
        assert_eq!(selected.len(), 0); // Not enough small files
    }

    #[test]
    fn test_file_selection_time_based() {
        let temp_dir = TempDir::new().unwrap();
        let config = CompactionConfig {
            min_files_to_compact: 2,
            strategy: CompactionStrategy::TimeBased,
            ..Default::default()
        };
        let manager = CompactionManager::new(temp_dir.path(), config);

        let old_time = Utc::now() - chrono::Duration::hours(48);
        let files = vec![
            FileInfo {
                path: PathBuf::from("old1.parquet"),
                size: 1_000_000,
                created: old_time,
            },
            FileInfo {
                path: PathBuf::from("old2.parquet"),
                size: 2_000_000,
                created: old_time,
            },
            FileInfo {
                path: PathBuf::from("new.parquet"),
                size: 500_000,
                created: Utc::now(),
            },
        ];

        let selected = manager.select_files_for_compaction(&files);
        assert_eq!(selected.len(), 2); // Only the 2 old files
    }

    #[test]
    fn test_file_selection_time_based_not_enough() {
        let temp_dir = TempDir::new().unwrap();
        let config = CompactionConfig {
            min_files_to_compact: 3,
            strategy: CompactionStrategy::TimeBased,
            ..Default::default()
        };
        let manager = CompactionManager::new(temp_dir.path(), config);

        let old_time = Utc::now() - chrono::Duration::hours(48);
        let files = vec![
            FileInfo {
                path: PathBuf::from("old1.parquet"),
                size: 1_000_000,
                created: old_time,
            },
            FileInfo {
                path: PathBuf::from("new.parquet"),
                size: 500_000,
                created: Utc::now(),
            },
        ];

        let selected = manager.select_files_for_compaction(&files);
        assert_eq!(selected.len(), 0); // Not enough old files
    }

    #[test]
    fn test_file_selection_full_compaction() {
        let temp_dir = TempDir::new().unwrap();
        let config = CompactionConfig {
            strategy: CompactionStrategy::FullCompaction,
            ..Default::default()
        };
        let manager = CompactionManager::new(temp_dir.path(), config);

        let files = vec![
            FileInfo {
                path: PathBuf::from("file1.parquet"),
                size: 1_000_000,
                created: Utc::now(),
            },
            FileInfo {
                path: PathBuf::from("file2.parquet"),
                size: 2_000_000,
                created: Utc::now(),
            },
        ];

        let selected = manager.select_files_for_compaction(&files);
        assert_eq!(selected.len(), 2); // All files selected
    }

    #[test]
    fn test_compaction_strategy_serde() {
        let strategies = vec![
            CompactionStrategy::SizeBased,
            CompactionStrategy::TimeBased,
            CompactionStrategy::FullCompaction,
        ];

        for strategy in strategies {
            let json = serde_json::to_string(&strategy).unwrap();
            let parsed: CompactionStrategy = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, strategy);
        }
    }

    #[test]
    fn test_compaction_stats_default() {
        let stats = CompactionStats::default();
        assert_eq!(stats.total_compactions, 0);
        assert_eq!(stats.total_files_compacted, 0);
    }

    #[test]
    fn test_compaction_stats_serde() {
        let stats = CompactionStats {
            total_compactions: 5,
            total_files_compacted: 20,
            total_bytes_before: 1000000,
            total_bytes_after: 500000,
            total_events_compacted: 10000,
            last_compaction_duration_ms: 500,
            space_saved_bytes: 500000,
        };

        let json = serde_json::to_string(&stats).unwrap();
        assert!(json.contains("\"total_compactions\":5"));
        assert!(json.contains("\"space_saved_bytes\":500000"));
    }

    #[test]
    fn test_compaction_result_serde() {
        let result = CompactionResult {
            files_compacted: 3,
            bytes_before: 1000000,
            bytes_after: 500000,
            events_compacted: 5000,
            duration_ms: 250,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"files_compacted\":3"));
        assert!(json.contains("\"bytes_before\":1000000"));
    }

    #[test]
    fn test_compaction_task_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config = CompactionConfig::default();
        let manager = Arc::new(CompactionManager::new(temp_dir.path(), config));

        let _task = CompactionTask::new(manager.clone(), 60);
        // Task created successfully
    }

    #[test]
    fn test_list_parquet_files_empty() {
        let temp_dir = TempDir::new().unwrap();
        let config = CompactionConfig::default();
        let manager = CompactionManager::new(temp_dir.path(), config);

        let files = manager.list_parquet_files().unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn test_list_parquet_files_with_non_parquet() {
        let temp_dir = TempDir::new().unwrap();
        let config = CompactionConfig::default();
        let manager = CompactionManager::new(temp_dir.path(), config);

        // Create non-parquet files
        std::fs::write(temp_dir.path().join("test.txt"), "test").unwrap();
        std::fs::write(temp_dir.path().join("data.json"), "{}").unwrap();

        let files = manager.list_parquet_files().unwrap();
        assert!(files.is_empty()); // No parquet files
    }

    // -----------------------------------------------------------------
    // Per-tenant compaction tests (Step 4, commit #2).
    // -----------------------------------------------------------------

    fn ingest_and_flush_per_call(storage_dir: &std::path::Path, tenant: &str, count: usize) {
        // Each ingested batch produces one parquet file when its
        // ParquetStorage is dropped, giving us multiple raw files
        // per tenant for the strategy filter to pick up.
        for i in 0..count {
            let storage = ParquetStorage::with_config(
                storage_dir,
                crate::infrastructure::persistence::ParquetStorageConfig {
                    batch_size: 1,
                    ..Default::default()
                },
            )
            .unwrap();
            let event = crate::domain::entities::Event::from_strings(
                "test.event".to_string(),
                format!("{tenant}-{i}"),
                tenant.to_string(),
                serde_json::json!({"i": i}),
                None,
            )
            .unwrap();
            storage.append_event(event).unwrap();
            storage.flush().unwrap();
        }
    }

    #[test]
    fn test_compact_tenant_emits_one_snapshot_and_removes_originals() {
        let temp_dir = TempDir::new().unwrap();

        // Seed 4 raw files for alice.
        ingest_and_flush_per_call(temp_dir.path(), "alice", 4);

        let config = CompactionConfig {
            min_files_to_compact: 2,
            small_file_threshold: 100 * 1024 * 1024,
            strategy: CompactionStrategy::SizeBased,
            ..Default::default()
        };
        let manager = CompactionManager::new(temp_dir.path(), config);

        let result = manager.compact_tenant("alice").unwrap();
        assert_eq!(result.files_compacted, 4);
        assert_eq!(result.events_compacted, 4);

        // After: zero raw files, exactly one snapshot.* file under
        // alice's subtree.
        let storage = ParquetStorage::new(temp_dir.path()).unwrap();
        let alice_files = storage.list_parquet_files_for_tenant("alice").unwrap();
        assert_eq!(
            alice_files.len(),
            1,
            "expected exactly one snapshot file for alice"
        );

        let name = alice_files[0]
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap()
            .to_string();
        assert!(
            name.starts_with("snapshot.alice."),
            "expected snapshot prefix, got {name}"
        );
        assert!(name.ends_with(".parquet"));

        // No tmp files left behind.
        let tmps: Vec<_> = std::fs::read_dir(alice_files[0].parent().unwrap())
            .unwrap()
            .filter_map(std::result::Result::ok)
            .filter(|e| e.path().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(tmps.is_empty());

        // Loaded events round-trip correctly.
        let loaded = storage.load_events_for_tenant("alice").unwrap();
        assert_eq!(loaded.len(), 4);
        for e in &loaded {
            assert_eq!(e.tenant_id_str(), "alice");
        }
    }

    #[test]
    fn test_compact_tenant_skips_existing_snapshot_files() {
        // Seed alice with 4 raw files, run compaction → 1 snapshot.
        // Run compaction AGAIN: the snapshot is excluded from the
        // candidate set, no qualifying raw files remain, the second
        // pass is a no-op.
        let temp_dir = TempDir::new().unwrap();
        ingest_and_flush_per_call(temp_dir.path(), "alice", 4);

        let config = CompactionConfig {
            min_files_to_compact: 2,
            small_file_threshold: 100 * 1024 * 1024,
            ..Default::default()
        };
        let manager = CompactionManager::new(temp_dir.path(), config);

        let r1 = manager.compact_tenant("alice").unwrap();
        assert_eq!(r1.files_compacted, 4);

        let r2 = manager.compact_tenant("alice").unwrap();
        assert_eq!(r2.files_compacted, 0, "snapshot must not be re-compacted");

        // Still exactly one file on disk for alice.
        let storage = ParquetStorage::new(temp_dir.path()).unwrap();
        let alice_files = storage.list_parquet_files_for_tenant("alice").unwrap();
        assert_eq!(alice_files.len(), 1);
    }

    #[test]
    fn test_compact_tenant_below_threshold_is_a_noop() {
        // 1 file < min_files_to_compact (default 3) → nothing happens.
        let temp_dir = TempDir::new().unwrap();
        ingest_and_flush_per_call(temp_dir.path(), "alice", 1);

        let manager = CompactionManager::new(temp_dir.path(), CompactionConfig::default());
        let result = manager.compact_tenant("alice").unwrap();
        assert_eq!(result.files_compacted, 0);

        // Raw file untouched.
        let storage = ParquetStorage::new(temp_dir.path()).unwrap();
        let alice_files = storage.list_parquet_files_for_tenant("alice").unwrap();
        assert_eq!(alice_files.len(), 1);
        let name = alice_files[0]
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        assert!(
            !name.starts_with("snapshot."),
            "raw file must not be renamed"
        );
    }

    #[test]
    fn test_compact_iterates_every_tenant() {
        // Two tenants each with enough raw files. compact() must
        // produce one snapshot per tenant.
        let temp_dir = TempDir::new().unwrap();
        ingest_and_flush_per_call(temp_dir.path(), "alice", 3);
        ingest_and_flush_per_call(temp_dir.path(), "bob", 3);

        let config = CompactionConfig {
            min_files_to_compact: 2,
            small_file_threshold: 100 * 1024 * 1024,
            strategy: CompactionStrategy::SizeBased,
            ..Default::default()
        };
        let manager = CompactionManager::new(temp_dir.path(), config);

        let result = manager.compact().unwrap();
        assert_eq!(result.files_compacted, 6);
        assert_eq!(result.events_compacted, 6);

        let storage = ParquetStorage::new(temp_dir.path()).unwrap();
        for tenant in ["alice", "bob"] {
            let files = storage.list_parquet_files_for_tenant(tenant).unwrap();
            assert_eq!(files.len(), 1, "{tenant} should have one snapshot");
            let name = files[0].file_name().unwrap().to_string_lossy().into_owned();
            assert!(name.starts_with(&format!("snapshot.{tenant}.")));
        }
    }

    #[test]
    fn test_retention_drops_events_older_than_ttl() {
        // Bead's integration test: ingest 100 events spanning 60
        // days, run compaction with 30-day TTL, only the last
        // 30 days remain queryable.
        let temp_dir = TempDir::new().unwrap();

        // Seed alice with 100 events, timestamps spread across
        // 60 days. We can't backdate via ingest (timestamp = now()
        // in domain), so write parquet directly via flush of
        // back-dated events.
        let storage = ParquetStorage::new(temp_dir.path()).unwrap();
        let now = Utc::now();
        for i in 0..100 {
            // Day 0 = 60 days ago; day 99 ≈ now. Spreads evenly.
            let day_offset = 60 - (i * 60 / 99);
            let ts = now - chrono::Duration::days(i64::from(day_offset));
            let event = crate::domain::entities::Event::reconstruct_from_strings(
                uuid::Uuid::new_v4(),
                "test.event".to_string(),
                format!("e-{i}"),
                "alice".to_string(),
                serde_json::json!({"i": i}),
                ts,
                None,
                1,
            );
            storage.append_event(event).unwrap();
            // Every 10 events get a fresh storage to produce a
            // separate file (so compaction has multiple files
            // to merge).
            if i % 10 == 9 {
                storage.flush().unwrap();
            }
        }
        storage.flush().unwrap();

        // 30-day TTL for alice via per-tenant override.
        let mut retention = RetentionConfig::default();
        retention.set("alice", Some(Duration::from_hours(30 * 24)));
        let config = CompactionConfig {
            min_files_to_compact: 2,
            small_file_threshold: 100 * 1024 * 1024,
            strategy: CompactionStrategy::SizeBased,
            retention,
            ..Default::default()
        };
        let manager = CompactionManager::new(temp_dir.path(), config);

        let result = manager.compact_tenant("alice").unwrap();
        assert!(result.events_compacted > 0);
        assert!(
            result.events_compacted < 100,
            "retention should have dropped some events; kept {} of 100",
            result.events_compacted
        );

        // Re-load to confirm the dropped events are gone.
        let storage2 = ParquetStorage::new(temp_dir.path()).unwrap();
        let loaded = storage2.load_events_for_tenant("alice").unwrap();
        assert_eq!(loaded.len(), result.events_compacted);

        // Every loaded event must be within the 30-day window
        // (with a generous fudge for test-clock drift).
        let cutoff = Utc::now() - chrono::Duration::days(30);
        for e in &loaded {
            assert!(
                e.timestamp >= cutoff - chrono::Duration::seconds(60),
                "event with ts {} survived retention but is older than cutoff {}",
                e.timestamp.to_rfc3339(),
                cutoff.to_rfc3339()
            );
        }
    }

    #[test]
    fn test_retention_keeps_forever_by_default_for_non_system_tenants() {
        // alice has no override → falls through to default_ttl
        // which is None. All events kept regardless of age.
        let temp_dir = TempDir::new().unwrap();
        let storage = ParquetStorage::new(temp_dir.path()).unwrap();
        let now = Utc::now();
        for i in 0..6 {
            let ts = now - chrono::Duration::days(i * 365);
            let event = crate::domain::entities::Event::reconstruct_from_strings(
                uuid::Uuid::new_v4(),
                "test.event".to_string(),
                format!("e-{i}"),
                "alice".to_string(),
                serde_json::json!({"i": i}),
                ts,
                None,
                1,
            );
            storage.append_event(event).unwrap();
            if i % 2 == 1 {
                storage.flush().unwrap();
            }
        }
        storage.flush().unwrap();

        // Default RetentionConfig: alice has no entry, default_ttl = None.
        let config = CompactionConfig {
            min_files_to_compact: 2,
            small_file_threshold: 100 * 1024 * 1024,
            strategy: CompactionStrategy::SizeBased,
            ..Default::default()
        };
        let manager = CompactionManager::new(temp_dir.path(), config);
        let result = manager.compact_tenant("alice").unwrap();
        assert_eq!(result.events_compacted, 6, "no events should be dropped");
    }

    #[test]
    fn test_retention_system_tenant_default_is_30_days() {
        // The system tenant's 30-day TTL is the bead's headline
        // requirement. Default config without any overrides
        // should already enforce it.
        let cfg = RetentionConfig::default();
        let ttl = cfg.ttl_for("system").unwrap();
        assert_eq!(ttl.as_secs(), 30 * 24 * 3600);
        // No override for arbitrary tenants → keep forever.
        assert!(cfg.ttl_for("acme").is_none());
    }

    #[test]
    fn test_retention_drops_all_events_deletes_originals_without_snapshot() {
        // Edge case: every event is past the TTL. We delete the
        // raw files and emit no snapshot — there's nothing to
        // write. Tenant ends with zero files on disk.
        let temp_dir = TempDir::new().unwrap();
        let storage = ParquetStorage::new(temp_dir.path()).unwrap();
        let very_old = Utc::now() - chrono::Duration::days(90);
        for i in 0..6 {
            let event = crate::domain::entities::Event::reconstruct_from_strings(
                uuid::Uuid::new_v4(),
                "test.event".to_string(),
                format!("e-{i}"),
                "alice".to_string(),
                serde_json::json!({"i": i}),
                very_old,
                None,
                1,
            );
            storage.append_event(event).unwrap();
            if i % 2 == 1 {
                storage.flush().unwrap();
            }
        }
        storage.flush().unwrap();

        let mut retention = RetentionConfig::default();
        retention.set("alice", Some(Duration::from_hours(7 * 24)));
        let config = CompactionConfig {
            min_files_to_compact: 2,
            small_file_threshold: 100 * 1024 * 1024,
            strategy: CompactionStrategy::SizeBased,
            retention,
            ..Default::default()
        };
        let manager = CompactionManager::new(temp_dir.path(), config);
        let result = manager.compact_tenant("alice").unwrap();
        assert_eq!(result.events_compacted, 0);
        assert!(result.files_compacted >= 2); // originals were deleted

        // Tenant subtree has zero parquet files now.
        let storage2 = ParquetStorage::new(temp_dir.path()).unwrap();
        let alice_files = storage2.list_parquet_files_for_tenant("alice").unwrap();
        assert!(alice_files.is_empty(), "all originals should be deleted");
    }

    #[test]
    fn test_compaction_with_simulated_crash_leaves_data_recoverable() {
        // AC #7-#8: simulate "crash mid-snapshot" by manually
        // dropping a tmp file in the partition, then asserting
        // a fresh ParquetStorage::new boot cleans it up and the
        // raw files (which would still be present in a real
        // mid-rename crash) remain queryable.
        let temp_dir = TempDir::new().unwrap();

        // Seed alice with raw files.
        ingest_and_flush_per_call(temp_dir.path(), "alice", 3);

        // Locate alice's partition dir.
        let storage = ParquetStorage::new(temp_dir.path()).unwrap();
        let alice_files = storage.list_parquet_files_for_tenant("alice").unwrap();
        let partition = alice_files[0].parent().unwrap().to_path_buf();

        // Simulate the crash state: write a partial .tmp file as
        // if write_atomic_parquet had crashed mid-rename.
        let crashed_tmp = partition.join("snapshot.alice.range.parquet.tmp");
        std::fs::write(&crashed_tmp, b"partial parquet bytes").unwrap();
        assert!(crashed_tmp.is_file());

        // Reboot — ParquetStorage::new triggers cleanup_partial_writes.
        let storage2 = ParquetStorage::new(temp_dir.path()).unwrap();
        assert!(
            !crashed_tmp.exists(),
            "stale tmp file should have been cleaned by ParquetStorage::new"
        );

        // Raw files survived; events queryable.
        let events = storage2.load_events_for_tenant("alice").unwrap();
        assert_eq!(events.len(), 3);
    }

    #[test]
    fn test_cold_tier_archives_dropped_events_before_deletion() {
        // Cold-tier integration: when retention drops events AND an
        // archive target is configured, the dropped events end up
        // in the archive root before originals are removed. This is
        // the load-bearing property — without archive-before-delete
        // the cold tier would silently lose data on retention runs.
        use crate::infrastructure::persistence::cold_tier::LocalFsArchive;

        let live_dir = TempDir::new().unwrap();
        let archive_dir = TempDir::new().unwrap();

        // Seed alice with 50 events spread across 60 days.
        let storage = ParquetStorage::new(live_dir.path()).unwrap();
        let now = Utc::now();
        for i in 0..50 {
            let day_offset = 60 - (i * 60 / 49);
            let ts = now - chrono::Duration::days(i64::from(day_offset));
            let event = crate::domain::entities::Event::reconstruct_from_strings(
                uuid::Uuid::new_v4(),
                "test.event".to_string(),
                format!("e-{i}"),
                "alice".to_string(),
                serde_json::json!({"i": i}),
                ts,
                None,
                1,
            );
            storage.append_event(event).unwrap();
            if i % 5 == 4 {
                storage.flush().unwrap();
            }
        }
        storage.flush().unwrap();

        // 30-day TTL + cold-tier archive.
        let mut retention = RetentionConfig::default();
        retention.set("alice", Some(Duration::from_hours(30 * 24)));
        let archive: Arc<dyn ArchiveTarget> =
            Arc::new(LocalFsArchive::new(archive_dir.path()).unwrap());
        let config = CompactionConfig {
            min_files_to_compact: 2,
            small_file_threshold: 100 * 1024 * 1024,
            strategy: CompactionStrategy::SizeBased,
            retention,
            archive: Some(archive),
            ..Default::default()
        };
        let manager = CompactionManager::new(live_dir.path(), config);

        let result = manager.compact_tenant("alice").unwrap();
        assert!(result.events_compacted > 0, "some events kept");
        assert!(
            result.events_compacted < 50,
            "some events dropped to retention; kept {} of 50",
            result.events_compacted
        );

        // Live storage: only events within the 30-day window.
        let live_after = ParquetStorage::new(live_dir.path())
            .unwrap()
            .load_events_for_tenant("alice")
            .unwrap();
        assert_eq!(live_after.len(), result.events_compacted);

        // Archive: contains the dropped events. Walk the archive root
        // and load any archive.alice.* file we find.
        let mut archive_files = vec![];
        let mut stack = vec![archive_dir.path().to_path_buf()];
        while let Some(d) = stack.pop() {
            for entry in std::fs::read_dir(&d).unwrap().flatten() {
                let p = entry.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with("archive.alice."))
                {
                    archive_files.push(p);
                }
            }
        }
        assert!(
            !archive_files.is_empty(),
            "archive directory must contain at least one archive.alice.* file"
        );

        // Sum events across archive files; total live + archived
        // must equal the original 50 (no events lost in the pipeline).
        let archive_storage = ParquetStorage::new(archive_dir.path()).unwrap();
        let archived = archive_storage.load_events_for_tenant("alice").unwrap();
        assert_eq!(
            live_after.len() + archived.len(),
            50,
            "live + archived must equal original event count (live={}, archived={})",
            live_after.len(),
            archived.len()
        );
    }

    #[test]
    fn test_cold_tier_failure_keeps_originals_on_disk() {
        // Crash-safety contract: a failing archive must NOT delete
        // originals. We use a custom ArchiveTarget that always
        // returns Err to simulate an outage.
        let live_dir = TempDir::new().unwrap();
        let storage = ParquetStorage::new(live_dir.path()).unwrap();
        let now = Utc::now();
        for i in 0..20 {
            let ts = now - chrono::Duration::days(60 - i);
            let event = crate::domain::entities::Event::reconstruct_from_strings(
                uuid::Uuid::new_v4(),
                "test.event".to_string(),
                format!("e-{i}"),
                "alice".to_string(),
                serde_json::json!({"i": i}),
                ts,
                None,
                1,
            );
            storage.append_event(event).unwrap();
            if i % 5 == 4 {
                storage.flush().unwrap();
            }
        }
        storage.flush().unwrap();

        // Count files before. The compaction pipeline should leave
        // them all on disk after the failed archive.
        let count_files = |dir: &std::path::Path| -> usize {
            let mut n = 0;
            let mut stack = vec![dir.to_path_buf()];
            while let Some(d) = stack.pop() {
                for entry in std::fs::read_dir(&d).unwrap().flatten() {
                    let p = entry.path();
                    if p.is_dir() {
                        stack.push(p);
                    } else if p.extension().is_some_and(|e| e == "parquet") {
                        n += 1;
                    }
                }
            }
            n
        };
        let before = count_files(live_dir.path());
        assert!(before > 0);

        #[derive(Debug)]
        struct FailingArchive;
        impl ArchiveTarget for FailingArchive {
            fn archive(
                &self,
                _: &str,
                _: DateTime<Utc>,
                _: DateTime<Utc>,
                _: &[crate::domain::entities::Event],
            ) -> Result<()> {
                Err(AllSourceError::StorageError(
                    "simulated archive outage".to_string(),
                ))
            }
        }

        let mut retention = RetentionConfig::default();
        retention.set("alice", Some(Duration::from_hours(30 * 24)));
        let config = CompactionConfig {
            min_files_to_compact: 2,
            small_file_threshold: 100 * 1024 * 1024,
            strategy: CompactionStrategy::SizeBased,
            retention,
            archive: Some(Arc::new(FailingArchive) as Arc<dyn ArchiveTarget>),
            ..Default::default()
        };
        let manager = CompactionManager::new(live_dir.path(), config);

        let result = manager.compact_tenant("alice");
        assert!(result.is_err(), "compaction must fail when archive fails");

        // Originals still on disk — every event still queryable.
        let after = count_files(live_dir.path());
        assert_eq!(
            before, after,
            "no files should be removed after archive failure"
        );

        let storage2 = ParquetStorage::new(live_dir.path()).unwrap();
        let loaded = storage2.load_events_for_tenant("alice").unwrap();
        assert_eq!(
            loaded.len(),
            20,
            "all 20 events still present after failed archive"
        );
    }

    #[test]
    fn test_cold_tier_not_invoked_when_no_events_dropped() {
        // If retention drops zero events (e.g. tenant has no TTL),
        // the archive target must NOT be called. We assert by using
        // an archive that panics on call.
        let live_dir = TempDir::new().unwrap();
        let storage = ParquetStorage::new(live_dir.path()).unwrap();
        let now = Utc::now();
        for i in 0..10 {
            let ts = now - chrono::Duration::hours(i);
            let event = crate::domain::entities::Event::reconstruct_from_strings(
                uuid::Uuid::new_v4(),
                "test.event".to_string(),
                format!("e-{i}"),
                "alice".to_string(),
                serde_json::json!({"i": i}),
                ts,
                None,
                1,
            );
            storage.append_event(event).unwrap();
            if i % 3 == 2 {
                storage.flush().unwrap();
            }
        }
        storage.flush().unwrap();

        #[derive(Debug)]
        struct PanickingArchive;
        impl ArchiveTarget for PanickingArchive {
            fn archive(
                &self,
                _: &str,
                _: DateTime<Utc>,
                _: DateTime<Utc>,
                _: &[crate::domain::entities::Event],
            ) -> Result<()> {
                panic!("archive must not be called when no events are dropped");
            }
        }

        // Default retention → alice has no TTL → no events dropped.
        let config = CompactionConfig {
            min_files_to_compact: 2,
            small_file_threshold: 100 * 1024 * 1024,
            strategy: CompactionStrategy::SizeBased,
            archive: Some(Arc::new(PanickingArchive) as Arc<dyn ArchiveTarget>),
            ..Default::default()
        };
        let manager = CompactionManager::new(live_dir.path(), config);
        let result = manager.compact_tenant("alice").unwrap();
        assert_eq!(result.events_compacted, 10);
    }

    #[test]
    fn test_discover_tenants_skips_system_and_hidden() {
        // Ensure the tenant scan doesn't pick up __system or hidden
        // directories — they're internal, not real tenants.
        let temp_dir = TempDir::new().unwrap();
        std::fs::create_dir_all(temp_dir.path().join("alice")).unwrap();
        std::fs::create_dir_all(temp_dir.path().join("bob")).unwrap();
        std::fs::create_dir_all(temp_dir.path().join("__system")).unwrap();
        std::fs::create_dir_all(temp_dir.path().join(".hidden")).unwrap();

        let manager = CompactionManager::new(temp_dir.path(), CompactionConfig::default());
        let tenants = manager.discover_tenants().unwrap();
        assert_eq!(tenants, vec!["alice".to_string(), "bob".to_string()]);
    }
}
