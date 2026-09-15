use crate::{
    domain::entities::Event,
    error::{AllSourceError, Result},
};
use arrow::{
    array::{
        Array, ArrayRef, StringBuilder, TimestampMicrosecondArray, TimestampMicrosecondBuilder,
        UInt64Builder,
    },
    datatypes::{DataType, Field, Schema, TimeUnit},
    record_batch::RecordBatch,
};
use parquet::{arrow::ArrowWriter, file::properties::WriterProperties};
use std::{
    collections::HashMap,
    fs::{self, File},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

/// Default batch size for Parquet writes (10,000 events as per US-023)
pub const DEFAULT_BATCH_SIZE: usize = 10_000;

/// Default flush timeout in milliseconds
pub const DEFAULT_FLUSH_TIMEOUT_MS: u64 = 5_000;

/// Configuration for ParquetStorage batch processing
#[derive(Debug, Clone)]
pub struct ParquetStorageConfig {
    /// Batch size before automatic flush (default: 10,000)
    pub batch_size: usize,
    /// Timeout before flushing partial batch (default: 5 seconds)
    pub flush_timeout: Duration,
    /// Compression codec for Parquet files
    pub compression: parquet::basic::Compression,
}

impl Default for ParquetStorageConfig {
    fn default() -> Self {
        Self {
            batch_size: DEFAULT_BATCH_SIZE,
            flush_timeout: Duration::from_millis(DEFAULT_FLUSH_TIMEOUT_MS),
            compression: parquet::basic::Compression::SNAPPY,
        }
    }
}

impl ParquetStorageConfig {
    /// High-throughput configuration optimized for large batch writes
    pub fn high_throughput() -> Self {
        Self {
            batch_size: 50_000,
            flush_timeout: Duration::from_secs(10),
            compression: parquet::basic::Compression::SNAPPY,
        }
    }

    /// Low-latency configuration for smaller, more frequent writes
    pub fn low_latency() -> Self {
        Self {
            batch_size: 1_000,
            flush_timeout: Duration::from_secs(1),
            compression: parquet::basic::Compression::SNAPPY,
        }
    }
}

/// Statistics for batch write operations
#[derive(Debug, Clone, Default)]
pub struct BatchWriteStats {
    /// Total batches written
    pub batches_written: u64,
    /// Total events written
    pub events_written: u64,
    /// Total bytes written
    pub bytes_written: u64,
    /// Average batch size
    pub avg_batch_size: f64,
    /// Events per second (throughput)
    pub events_per_sec: f64,
    /// Total write time in nanoseconds
    pub total_write_time_ns: u64,
    /// Number of timeout-triggered flushes
    pub timeout_flushes: u64,
    /// Number of size-triggered flushes
    pub size_flushes: u64,
}

/// Result of a batch write operation
#[derive(Debug, Clone)]
pub struct BatchWriteResult {
    /// Number of events written
    pub events_written: usize,
    /// Number of batches flushed to disk
    pub batches_flushed: usize,
    /// Total duration of the write operation
    pub duration: Duration,
    /// Write throughput in events per second
    pub events_per_sec: f64,
}

/// Parquet-based persistent storage for events with batch processing
///
/// Features:
/// - Configurable batch size (default: 10,000 events per US-023)
/// - Timeout-based flushing for partial batches
/// - Thread-safe batch accumulation
/// - SNAPPY compression for efficient storage
/// - Automatic flush on shutdown via Drop
pub struct ParquetStorage {
    /// Base directory for storing parquet files
    storage_dir: PathBuf,

    /// Buffered events keyed by tenant_id. Each tenant accumulates its own
    /// batch and flushes independently into its partition under
    /// `storage_dir/<tenant_id>/<yyyy-mm>/`. Single outer mutex protects the
    /// whole map: lookup is O(1), tenant cardinality is low (single digits
    /// today; bounded by Step 3's cache budget later), so contention is
    /// fine. We keep the mutex held only for the push, not for disk I/O —
    /// flush takes ownership of a tenant's batch via remove() and writes
    /// after the lock is released.
    current_batches: Mutex<HashMap<String, Vec<Event>>>,

    /// Configuration
    config: ParquetStorageConfig,

    /// Schema for Arrow/Parquet
    schema: Arc<Schema>,

    /// Last flush timestamp for timeout tracking
    last_flush_time: Mutex<Instant>,

    /// Statistics tracking
    batches_written: AtomicU64,
    events_written: AtomicU64,
    bytes_written: AtomicU64,
    total_write_time_ns: AtomicU64,
    timeout_flushes: AtomicU64,
    size_flushes: AtomicU64,
}

impl ParquetStorage {
    /// Create a new ParquetStorage with default configuration (10,000 event batches)
    pub fn new(storage_dir: impl AsRef<Path>) -> Result<Self> {
        Self::with_config(storage_dir, ParquetStorageConfig::default())
    }

    /// Create a new ParquetStorage with custom configuration
    pub fn with_config(
        storage_dir: impl AsRef<Path>,
        config: ParquetStorageConfig,
    ) -> Result<Self> {
        let storage_dir = storage_dir.as_ref().to_path_buf();

        // Create storage directory if it doesn't exist
        fs::create_dir_all(&storage_dir).map_err(|e| {
            AllSourceError::StorageError(format!("Failed to create storage directory: {e}"))
        })?;

        // Define Arrow schema for events
        let schema = Arc::new(Schema::new(vec![
            Field::new("event_id", DataType::Utf8, false),
            Field::new("event_type", DataType::Utf8, false),
            Field::new("entity_id", DataType::Utf8, false),
            Field::new("payload", DataType::Utf8, false),
            Field::new(
                "timestamp",
                DataType::Timestamp(TimeUnit::Microsecond, None),
                false,
            ),
            Field::new("metadata", DataType::Utf8, true),
            Field::new("version", DataType::UInt64, false),
        ]));

        let storage = Self {
            storage_dir,
            current_batches: Mutex::new(HashMap::new()),
            config,
            schema,
            last_flush_time: Mutex::new(Instant::now()),
            batches_written: AtomicU64::new(0),
            events_written: AtomicU64::new(0),
            bytes_written: AtomicU64::new(0),
            total_write_time_ns: AtomicU64::new(0),
            timeout_flushes: AtomicU64::new(0),
            size_flushes: AtomicU64::new(0),
        };

        // Boot-time crash recovery: heal *.parquet.tmp files from
        // crashed atomic writes (snapshot/compaction or, post-#166-fix,
        // checkpoint flushes) and quarantine any 0-byte *.parquet files
        // left over from pre-fix checkpoint crashes (issue #166).
        match storage.cleanup_partial_writes() {
            Ok(0) => {}
            Ok(n) => tracing::warn!(
                "cleanup_partial_writes acted on {n} crash-detritus file(s) on boot — \
                 see preceding logs for per-file detail"
            ),
            Err(e) => tracing::error!("cleanup_partial_writes failed on boot: {e}"),
        }

        Ok(storage)
    }

    /// Create storage with legacy batch size (1000) for backward compatibility
    #[deprecated(note = "Use new() or with_config() instead - default batch size is now 10,000")]
    pub fn with_legacy_batch_size(storage_dir: impl AsRef<Path>) -> Result<Self> {
        Self::with_config(
            storage_dir,
            ParquetStorageConfig {
                batch_size: 1000,
                ..Default::default()
            },
        )
    }

    /// Add an event to the current batch
    ///
    /// Events are routed to a per-tenant batch keyed by `event.tenant_id_str()`.
    /// A tenant's batch is buffered until any of:
    /// - That tenant's batch hits the configured `batch_size` (default 10,000)
    ///   — flushes only that tenant, not the whole world
    /// - The flush timeout elapses — flushes every tenant with pending data
    /// - `flush()` is called explicitly
    /// - The process shuts down
    #[cfg_attr(feature = "hotpath", hotpath::measure)]
    pub fn append_event(&self, event: Event) -> Result<()> {
        let tenant = event.tenant_id_str().to_string();
        let should_flush_tenant = {
            let mut batches = self.current_batches.lock().unwrap();
            let entry = batches.entry(tenant.clone()).or_default();
            entry.push(event);
            entry.len() >= self.config.batch_size
        };

        if should_flush_tenant {
            self.size_flushes.fetch_add(1, Ordering::Relaxed);
            self.flush_tenant(&tenant)?;
        }

        Ok(())
    }

    /// Add multiple events to the batch (optimized batch insertion)
    ///
    /// Preferred entry point for high-throughput ingestion. Events are
    /// grouped by tenant under a single mutex acquisition and any tenant
    /// that crosses `batch_size` is flushed on the spot.
    #[cfg_attr(feature = "hotpath", hotpath::measure)]
    pub fn batch_write(&self, events: Vec<Event>) -> Result<BatchWriteResult> {
        let start = Instant::now();
        let event_count = events.len();

        // Pre-group by tenant to keep the lock window short — one acquire,
        // one extend per tenant, decide which tenants are over threshold.
        let mut grouped: HashMap<String, Vec<Event>> = HashMap::new();
        for event in events {
            grouped
                .entry(event.tenant_id_str().to_string())
                .or_default()
                .push(event);
        }

        let mut tenants_to_flush: Vec<String> = Vec::new();
        {
            let mut batches = self.current_batches.lock().unwrap();
            for (tenant, mut new_events) in grouped {
                let entry = batches.entry(tenant.clone()).or_default();
                entry.append(&mut new_events);
                if entry.len() >= self.config.batch_size {
                    tenants_to_flush.push(tenant);
                }
            }
        }

        let mut batches_flushed = 0;
        for tenant in tenants_to_flush {
            self.size_flushes.fetch_add(1, Ordering::Relaxed);
            self.flush_tenant(&tenant)?;
            batches_flushed += 1;
        }

        let duration = start.elapsed();

        Ok(BatchWriteResult {
            events_written: event_count,
            batches_flushed,
            duration,
            events_per_sec: event_count as f64 / duration.as_secs_f64(),
        })
    }

    /// Check if a timeout-based flush is needed and perform it
    ///
    /// Call this periodically (e.g., from a background task) to ensure
    /// partial batches are flushed within the configured timeout. When
    /// triggered, every tenant with pending events flushes — the timer is
    /// global, not per-tenant, so a slow-trickle tenant doesn't get
    /// stranded waiting for its own batch to fill.
    #[cfg_attr(feature = "hotpath", hotpath::measure)]
    pub fn check_timeout_flush(&self) -> Result<bool> {
        let should_flush = {
            let last_flush = self.last_flush_time.lock().unwrap();
            let batches = self.current_batches.lock().unwrap();
            let any_pending = batches.values().any(|v| !v.is_empty());
            any_pending && last_flush.elapsed() >= self.config.flush_timeout
        };

        if should_flush {
            self.timeout_flushes.fetch_add(1, Ordering::Relaxed);
            self.flush()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Flush every tenant's pending batch to its partition.
    ///
    /// Thread-safe: callable from any thread. A snapshot of which tenants
    /// have pending data is taken under a short lock; each tenant is then
    /// flushed individually with its own lock cycle, so disk I/O for one
    /// tenant doesn't block writes against another.
    #[cfg_attr(feature = "hotpath", hotpath::measure)]
    pub fn flush(&self) -> Result<()> {
        let tenants: Vec<String> = {
            let batches = self.current_batches.lock().unwrap();
            batches
                .iter()
                .filter(|(_, v)| !v.is_empty())
                .map(|(k, _)| k.clone())
                .collect()
        };
        if tenants.is_empty() {
            return Ok(());
        }
        for tenant in tenants {
            self.flush_tenant(&tenant)?;
        }
        Ok(())
    }

    /// Flush a single tenant's pending events into its partition file.
    ///
    /// File path: `storage_dir/<sanitized_tenant_id>/<yyyy-mm>/events-<ts>-<uuid>.parquet`.
    /// `<yyyy-mm>` is taken from the wall-clock at flush time (matching the
    /// pre-tenant filename's timestamp semantics) rather than from
    /// individual event timestamps — keeps each flush to a single output
    /// file even when buffered events span months.
    ///
    /// The file is written atomically via `write_record_batch_atomic`:
    /// crash mid-write leaves a `.parquet.tmp` file (cleaned on next boot)
    /// rather than a 0-byte `.parquet` file with the final name. Issue
    /// #166 — pre-fix, a SIGKILL inside this function bricked subsequent
    /// loads because `load_all_events` aborted on the first unreadable
    /// file.
    fn flush_tenant(&self, tenant_id: &str) -> Result<()> {
        let events_to_write = {
            let mut batches = self.current_batches.lock().unwrap();
            match batches.get_mut(tenant_id) {
                Some(v) if !v.is_empty() => std::mem::take(v),
                _ => return Ok(()),
            }
        };

        let batch_count = events_to_write.len();
        let start = Instant::now();

        let written = self.write_tenant_events(tenant_id, &events_to_write);
        let (file_path, file_metadata) = match written {
            Ok(written) => written,
            Err(e) => {
                // The WAL may be retired by the next successful checkpoint, so
                // dropping these here would make the failure permanent (#287).
                let mut batches = self.current_batches.lock().unwrap();
                let pending = batches.entry(tenant_id.to_string()).or_default();
                let arrived_during_write = std::mem::replace(pending, events_to_write);
                pending.extend(arrived_during_write);
                return Err(e);
            }
        };

        let duration = start.elapsed();

        self.batches_written.fetch_add(1, Ordering::Relaxed);
        self.events_written
            .fetch_add(batch_count as u64, Ordering::Relaxed);
        if let Some(size) = file_metadata
            .row_groups()
            .first()
            .map(parquet::file::metadata::RowGroupMetaData::total_byte_size)
        {
            self.bytes_written.fetch_add(size as u64, Ordering::Relaxed);
        }
        self.total_write_time_ns
            .fetch_add(duration.as_nanos() as u64, Ordering::Relaxed);

        {
            let mut last_flush = self.last_flush_time.lock().unwrap();
            *last_flush = Instant::now();
        }

        tracing::info!(
            "Wrote {} events for tenant={} to {} in {:?}",
            batch_count,
            tenant_id,
            file_path.display(),
            duration
        );

        Ok(())
    }

    fn write_tenant_events(
        &self,
        tenant_id: &str,
        events: &[Event],
    ) -> Result<(PathBuf, parquet::file::metadata::ParquetMetaData)> {
        let record_batch = self.events_to_record_batch(events)?;

        let now = chrono::Utc::now();
        let partition_dir = partition_path_for_tenant(&self.storage_dir, tenant_id, now)?;
        fs::create_dir_all(&partition_dir).map_err(|e| {
            AllSourceError::StorageError(format!(
                "Failed to create tenant partition {}: {e}",
                partition_dir.display()
            ))
        })?;
        let file_stem = format!(
            "events-{}-{}",
            now.format("%Y%m%d-%H%M%S%3f"),
            uuid::Uuid::new_v4().as_simple()
        );

        tracing::info!(
            "Flushing {} events for tenant={} to {}/{}.parquet",
            events.len(),
            tenant_id,
            partition_dir.display(),
            file_stem
        );

        self.write_record_batch_atomic(&partition_dir, &file_stem, &record_batch)
    }

    /// Write a single record batch atomically to
    /// `<partition_dir>/<file_stem>.parquet`, returning the final path
    /// and the parquet writer's metadata (used for byte-size metrics by
    /// the checkpoint flush path).
    ///
    /// Crash-safety contract — same four steps as `write_atomic_parquet`:
    /// 1. Write to `<final_path>.tmp` first.
    /// 2. fsync the .tmp file so the data is durably on disk.
    /// 3. Rename `.tmp` → final name (atomic POSIX rename).
    /// 4. fsync the parent directory so the rename survives crash.
    ///
    /// Caller is responsible for choosing `partition_dir` (and creating
    /// it). On any failure mid-way, the `.tmp` file gets cleaned up by
    /// `cleanup_partial_writes` on next boot. The final file appears
    /// atomically — readers either see the old state (no file) or the
    /// complete new file, never a half-written one.
    fn write_record_batch_atomic(
        &self,
        partition_dir: &Path,
        file_stem: &str,
        record_batch: &RecordBatch,
    ) -> Result<(PathBuf, parquet::file::metadata::ParquetMetaData)> {
        let final_path = partition_dir.join(format!("{file_stem}.parquet"));
        let tmp_path = partition_dir.join(format!("{file_stem}.parquet.tmp"));

        // 1. Write to .tmp.
        let metadata = {
            let file = File::create(&tmp_path).map_err(|e| {
                AllSourceError::StorageError(format!(
                    "Failed to create parquet tmp file {}: {e}",
                    tmp_path.display()
                ))
            })?;

            let props = WriterProperties::builder()
                .set_compression(self.config.compression)
                .build();

            let mut writer = ArrowWriter::try_new(file, self.schema.clone(), Some(props))?;
            writer.write(record_batch)?;
            // close() consumes writer and returns the file metadata; the
            // underlying File is flushed and closed here.
            writer.close()?
        };

        // 2. fsync the .tmp file.
        let tmp_file = File::open(&tmp_path).map_err(|e| {
            AllSourceError::StorageError(format!(
                "Failed to reopen parquet tmp for fsync {}: {e}",
                tmp_path.display()
            ))
        })?;
        tmp_file.sync_all().map_err(|e| {
            AllSourceError::StorageError(format!("fsync on parquet tmp failed: {e}"))
        })?;
        drop(tmp_file);

        // 3. Atomic rename.
        fs::rename(&tmp_path, &final_path).map_err(|e| {
            AllSourceError::StorageError(format!(
                "Failed to rename {} → {}: {e}",
                tmp_path.display(),
                final_path.display()
            ))
        })?;

        // 4. fsync the parent directory so the rename survives crash.
        // Linux fsync-on-dir is the canonical way to make a rename
        // durable; macOS no-ops the dir fsync but doesn't error.
        if let Ok(dir) = File::open(partition_dir) {
            let _ = dir.sync_all();
        }

        Ok((final_path, metadata))
    }

    /// Atomically write `events` to a Parquet file under the tenant's
    /// partition. Step 4 of the sustainable data strategy uses this to
    /// emit per-tenant snapshot/compaction files (`snapshot.<tenant>.<from>-<to>`)
    /// without risking partial files on crash.
    ///
    /// Crash-safety contract:
    /// 1. Write to `<final_path>.tmp` first.
    /// 2. fsync the .tmp file so the data is durably on disk.
    /// 3. Rename `.tmp` → final name (atomic POSIX rename).
    /// 4. fsync the parent directory so the rename is durable.
    ///
    /// On any failure mid-way, the .tmp file gets cleaned up by
    /// `cleanup_partial_writes` on next boot. The final file appears
    /// atomically — readers either see the old state (no file) or
    /// the complete new file, never a half-written one.
    ///
    /// `file_stem` is the filename without extension or partition path
    /// — caller-controlled so the snapshot naming convention
    /// (`snapshot.<tenant>.<from>-<to>`) lives in the compaction layer,
    /// not here. The `.parquet` extension is appended automatically.
    ///
    /// Returns the final (post-rename) path so the caller can
    /// confirm the file landed where expected.
    pub fn write_atomic_parquet(
        &self,
        tenant_id: &str,
        file_stem: &str,
        events: &[Event],
    ) -> Result<PathBuf> {
        if events.is_empty() {
            return Err(AllSourceError::StorageError(
                "write_atomic_parquet called with empty event slice".to_string(),
            ));
        }
        // Anchor partition by the earliest event's month — keeps the
        // file in the same yyyy-mm bucket as the data it represents.
        // Callers that span multiple months can use the earliest
        // event's month and trust the recursive walker to find it.
        let anchor_ts = events
            .iter()
            .map(|e| e.timestamp)
            .min()
            .unwrap_or_else(chrono::Utc::now);
        let partition_dir = partition_path_for_tenant(&self.storage_dir, tenant_id, anchor_ts)?;
        fs::create_dir_all(&partition_dir).map_err(|e| {
            AllSourceError::StorageError(format!(
                "Failed to create tenant partition {}: {e}",
                partition_dir.display()
            ))
        })?;

        let record_batch = self.events_to_record_batch(events)?;
        let (final_path, _meta) =
            self.write_record_batch_atomic(&partition_dir, file_stem, &record_batch)?;

        tracing::info!(
            tenant_id = tenant_id,
            file = %final_path.display(),
            event_count = events.len(),
            "wrote atomic snapshot file"
        );

        Ok(final_path)
    }

    /// Sweep the storage tree for crash detritus and heal it on boot.
    /// Called on boot from `ParquetStorage::new` so every cold start
    /// starts clean. Two kinds of leftovers are handled:
    ///
    /// 1. **`*.parquet.tmp` files** — a snapshot or flush write that
    ///    crashed between fsync and rename. Safe to delete: the data is
    ///    either still in the WAL (for checkpoint flushes) or in the
    ///    constituent raw files (for snapshot/compaction, which only
    ///    deletes the inputs AFTER the rename succeeds). Either way, the
    ///    failed write will be retried.
    ///
    /// 2. **0-byte `*.parquet` files** — issue #166. Pre-fix releases
    ///    (≤ 0.20.0 for the checkpoint path) wrote directly to the
    ///    final filename, so SIGKILL between `File::create` and the
    ///    first flush left a 0-byte `events-*.parquet` that bricked
    ///    every subsequent load. Rather than delete, we rename to
    ///    `<original>.parquet.corrupt-<unix-ts>` so an operator can
    ///    inspect/forensicate before destroying the evidence — most
    ///    will just `rm` after seeing the size.
    ///
    /// Returns the total number of files acted on, for boot-log
    /// observability.
    pub fn cleanup_partial_writes(&self) -> Result<usize> {
        let mut acted = 0usize;
        let mut stack: Vec<PathBuf> = vec![self.storage_dir.clone()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let Ok(ft) = entry.file_type() else { continue };
                if ft.is_dir() {
                    stack.push(path);
                    continue;
                }
                if !ft.is_file() {
                    continue;
                }
                let path_str = path.to_string_lossy();
                if path_str.ends_with(".parquet.tmp") {
                    match fs::remove_file(&path) {
                        Ok(()) => {
                            tracing::warn!(
                                file = %path.display(),
                                "cleaned up orphan snapshot tmp file (crash recovery)"
                            );
                            acted += 1;
                        }
                        Err(e) => {
                            tracing::error!(
                                file = %path.display(),
                                "failed to remove orphan snapshot tmp file: {e}"
                            );
                        }
                    }
                } else if path_str.ends_with(".parquet")
                    && fs::metadata(&path).is_ok_and(|m| m.len() == 0)
                {
                    // Issue #166: pre-fix checkpoint write left a 0-byte
                    // .parquet file with the final name on crash. Quarantine
                    // (rename) rather than delete — operator can inspect.
                    let ts = chrono::Utc::now().timestamp();
                    let quarantine_path = path.with_extension(format!("parquet.corrupt-{ts}"));
                    match fs::rename(&path, &quarantine_path) {
                        Ok(()) => {
                            tracing::error!(
                                from = %path.display(),
                                to = %quarantine_path.display(),
                                "quarantined 0-byte parquet file (issue #166 pre-fix crash). \
                                 Operator: inspect and rm if not needed."
                            );
                            acted += 1;
                        }
                        Err(e) => {
                            tracing::error!(
                                file = %path.display(),
                                "failed to quarantine 0-byte parquet file: {e}"
                            );
                        }
                    }
                }
            }
        }
        Ok(acted)
    }

    /// Force flush any remaining events (for shutdown handling).
    ///
    /// Sums pending counts across every tenant's batch so the caller can
    /// log "we flushed N events on shutdown" without caring about
    /// per-tenant breakdown.
    #[cfg_attr(feature = "hotpath", hotpath::measure)]
    pub fn flush_on_shutdown(&self) -> Result<usize> {
        let total_pending: usize = {
            let batches = self.current_batches.lock().unwrap();
            batches.values().map(Vec::len).sum()
        };

        if total_pending > 0 {
            tracing::info!(
                "Shutdown: flushing {} pending events across all tenants",
                total_pending
            );
            self.flush()?;
        }

        Ok(total_pending)
    }

    /// Get batch write statistics
    pub fn batch_stats(&self) -> BatchWriteStats {
        let batches = self.batches_written.load(Ordering::Relaxed);
        let events = self.events_written.load(Ordering::Relaxed);
        let bytes = self.bytes_written.load(Ordering::Relaxed);
        let time_ns = self.total_write_time_ns.load(Ordering::Relaxed);

        let time_secs = time_ns as f64 / 1_000_000_000.0;

        BatchWriteStats {
            batches_written: batches,
            events_written: events,
            bytes_written: bytes,
            avg_batch_size: if batches > 0 {
                events as f64 / batches as f64
            } else {
                0.0
            },
            events_per_sec: if time_secs > 0.0 {
                events as f64 / time_secs
            } else {
                0.0
            },
            total_write_time_ns: time_ns,
            timeout_flushes: self.timeout_flushes.load(Ordering::Relaxed),
            size_flushes: self.size_flushes.load(Ordering::Relaxed),
        }
    }

    /// Total pending events across all tenant batches.
    pub fn pending_count(&self) -> usize {
        self.current_batches
            .lock()
            .unwrap()
            .values()
            .map(Vec::len)
            .sum()
    }

    /// Get configured batch size
    pub fn batch_size(&self) -> usize {
        self.config.batch_size
    }

    /// Get configured flush timeout
    pub fn flush_timeout(&self) -> Duration {
        self.config.flush_timeout
    }

    /// Convert events to Arrow RecordBatch
    #[cfg_attr(feature = "hotpath", hotpath::measure)]
    fn events_to_record_batch(&self, events: &[Event]) -> Result<RecordBatch> {
        let mut event_id_builder = StringBuilder::new();
        let mut event_type_builder = StringBuilder::new();
        let mut entity_id_builder = StringBuilder::new();
        let mut payload_builder = StringBuilder::new();
        let mut timestamp_builder = TimestampMicrosecondBuilder::new();
        let mut metadata_builder = StringBuilder::new();
        let mut version_builder = UInt64Builder::new();

        for event in events {
            event_id_builder.append_value(event.id.to_string());
            event_type_builder.append_value(event.event_type_str());
            entity_id_builder.append_value(event.entity_id_str());
            payload_builder.append_value(serde_json::to_string(&event.payload)?);

            // Convert timestamp to microseconds
            let timestamp_micros = event.timestamp.timestamp_micros();
            timestamp_builder.append_value(timestamp_micros);

            if let Some(ref metadata) = event.metadata {
                metadata_builder.append_value(serde_json::to_string(metadata)?);
            } else {
                metadata_builder.append_null();
            }

            version_builder.append_value(event.version as u64);
        }

        let arrays: Vec<ArrayRef> = vec![
            Arc::new(event_id_builder.finish()),
            Arc::new(event_type_builder.finish()),
            Arc::new(entity_id_builder.finish()),
            Arc::new(payload_builder.finish()),
            Arc::new(timestamp_builder.finish()),
            Arc::new(metadata_builder.finish()),
            Arc::new(version_builder.finish()),
        ];

        let record_batch = RecordBatch::try_new(self.schema.clone(), arrays)?;

        Ok(record_batch)
    }

    /// Load events from all Parquet files under the storage directory.
    ///
    /// Walks the tree recursively so both layouts work: legacy flat
    /// (`storage_dir/events-*.parquet`) and the tenant-partitioned tree
    /// introduced by the data-strategy work (`storage_dir/<tenant>/<yyyy-mm>/
    /// events-*.parquet`). The two coexist on disk during migration.
    #[cfg_attr(feature = "hotpath", hotpath::measure)]
    pub fn load_all_events(&self) -> Result<Vec<Event>> {
        let parquet_files = find_parquet_files_recursive(&self.storage_dir)?;

        let mut all_events = Vec::with_capacity(parquet_files.len() * self.config.batch_size);
        let mut skipped = 0usize;
        for file_path in parquet_files {
            tracing::info!("Loading events from {}", file_path.display());
            let tenant_id = tenant_id_from_path(&self.storage_dir, &file_path);
            // Issue #166: a single corrupt/0-byte parquet file (crash mid-write
            // on pre-fix releases) would propagate up and abort the whole load,
            // bricking access to every healthy file alongside it. Log and skip
            // instead so one bad file can't take the rest down.
            match self.load_events_from_file(&file_path, &tenant_id) {
                Ok(file_events) => all_events.extend(file_events),
                Err(e) => {
                    tracing::error!(
                        file = %file_path.display(),
                        error = %e,
                        "Skipping unreadable parquet file — other files will still load. \
                         Likely a 0-byte or truncated file from an unclean shutdown; \
                         inspect and remove manually after confirming."
                    );
                    skipped += 1;
                }
            }
        }

        if skipped > 0 {
            tracing::warn!(
                "Loaded {} events from storage; skipped {} unreadable file(s)",
                all_events.len(),
                skipped
            );
        } else {
            tracing::info!("Loaded {} total events from storage", all_events.len());
        }

        Ok(all_events)
    }

    /// Load events from a single Parquet file. `tenant_id` is the value to
    /// stamp onto each loaded event — derived from the file's location in
    /// the tree by `load_all_events`. The Parquet schema doesn't include
    /// tenant_id today (path is the source of truth), so this is how
    /// per-tenant identity survives the round trip.
    #[cfg_attr(feature = "hotpath", hotpath::measure)]
    fn load_events_from_file(&self, file_path: &Path, tenant_id: &str) -> Result<Vec<Event>> {
        use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;

        let file = File::open(file_path).map_err(|e| {
            AllSourceError::StorageError(format!("Failed to open parquet file: {e}"))
        })?;

        let builder = ParquetRecordBatchReaderBuilder::try_new(file)?;
        let mut reader = builder.build()?;

        let mut events = Vec::new();

        while let Some(Ok(batch)) = reader.next() {
            let batch_events = self.record_batch_to_events(&batch, tenant_id)?;
            events.extend(batch_events);
        }

        Ok(events)
    }

    /// Public wrapper around the internal single-file loader. Used
    /// by the per-tenant compaction (Step 4) which needs to read a
    /// specific candidate set rather than the whole tenant subtree.
    /// Caller passes the tenant_id explicitly because the schema
    /// doesn't carry it.
    pub fn load_events_from_file_path(
        &self,
        file_path: &Path,
        tenant_id: &str,
    ) -> Result<Vec<Event>> {
        self.load_events_from_file(file_path, tenant_id)
    }

    /// Convert Arrow RecordBatch back to events. `tenant_id` is stamped onto
    /// each reconstructed event — the schema doesn't carry it today, so the
    /// caller passes the value derived from the file path.
    #[cfg_attr(feature = "hotpath", hotpath::measure)]
    fn record_batch_to_events(&self, batch: &RecordBatch, tenant_id: &str) -> Result<Vec<Event>> {
        let event_ids = batch
            .column(0)
            .as_any()
            .downcast_ref::<arrow::array::StringArray>()
            .ok_or_else(|| AllSourceError::StorageError("Invalid event_id column".to_string()))?;

        let event_types = batch
            .column(1)
            .as_any()
            .downcast_ref::<arrow::array::StringArray>()
            .ok_or_else(|| AllSourceError::StorageError("Invalid event_type column".to_string()))?;

        let entity_ids = batch
            .column(2)
            .as_any()
            .downcast_ref::<arrow::array::StringArray>()
            .ok_or_else(|| AllSourceError::StorageError("Invalid entity_id column".to_string()))?;

        let payloads = batch
            .column(3)
            .as_any()
            .downcast_ref::<arrow::array::StringArray>()
            .ok_or_else(|| AllSourceError::StorageError("Invalid payload column".to_string()))?;

        let timestamps = batch
            .column(4)
            .as_any()
            .downcast_ref::<TimestampMicrosecondArray>()
            .ok_or_else(|| AllSourceError::StorageError("Invalid timestamp column".to_string()))?;

        let metadatas = batch
            .column(5)
            .as_any()
            .downcast_ref::<arrow::array::StringArray>()
            .ok_or_else(|| AllSourceError::StorageError("Invalid metadata column".to_string()))?;

        let versions = batch
            .column(6)
            .as_any()
            .downcast_ref::<arrow::array::UInt64Array>()
            .ok_or_else(|| AllSourceError::StorageError("Invalid version column".to_string()))?;

        let mut events = Vec::new();

        for i in 0..batch.num_rows() {
            let id = uuid::Uuid::parse_str(event_ids.value(i))
                .map_err(|e| AllSourceError::StorageError(format!("Invalid UUID: {e}")))?;

            let timestamp = chrono::DateTime::from_timestamp_micros(timestamps.value(i))
                .ok_or_else(|| AllSourceError::StorageError("Invalid timestamp".to_string()))?;

            let metadata = if metadatas.is_null(i) {
                None
            } else {
                Some(serde_json::from_str(metadatas.value(i))?)
            };

            let event = Event::reconstruct_from_strings(
                id,
                event_types.value(i).to_string(),
                entity_ids.value(i).to_string(),
                tenant_id.to_string(),
                serde_json::from_str(payloads.value(i))?,
                timestamp,
                metadata,
                versions.value(i) as i64,
            );

            events.push(event);
        }

        Ok(events)
    }

    /// List all Parquet file paths under the storage directory, sorted by
    /// the relative path so files in the same partition stay grouped.
    ///
    /// Used by the replication catch-up protocol to stream snapshot files
    /// to followers that are too far behind for WAL-only catch-up.
    pub fn list_parquet_files(&self) -> Result<Vec<PathBuf>> {
        find_parquet_files_recursive(&self.storage_dir)
    }

    /// List Parquet files belonging to a single tenant — i.e. only files
    /// under `<storage_dir>/<tenant>/...`. Legacy flat-layout files at the
    /// root are intentionally excluded; the migration tool moves them under
    /// `default/` so once it has run a `tenant=default` query sees them.
    ///
    /// Returns an empty vec when the tenant subtree doesn't exist (no data
    /// for that tenant yet). Returns an error only if `tenant_id` fails the
    /// path-safety whitelist.
    ///
    /// This is the building block for tenant-scoped reads: the caller knows
    /// which files might contain the tenant's data without opening any of
    /// the others.
    pub fn list_parquet_files_for_tenant(&self, tenant_id: &str) -> Result<Vec<PathBuf>> {
        let safe = sanitize_tenant_id_for_path(tenant_id)?;
        let tenant_root = self.storage_dir.join(safe);
        if !tenant_root.is_dir() {
            return Ok(Vec::new());
        }
        find_parquet_files_recursive(&tenant_root)
    }

    /// Load only the events belonging to `tenant_id`, walking just that
    /// tenant's subtree on disk. The full-storage loader
    /// (`load_all_events`) opens every Parquet file regardless of tenant;
    /// this one only opens files under `<storage_dir>/<tenant>/`.
    ///
    /// Returns an empty vec when the tenant has no on-disk data. Returns
    /// an error if the tenant_id fails the path-safety whitelist or any
    /// individual file fails to load.
    ///
    /// This is the read-side complement to per-tenant flushing. It's the
    /// foundation Step 2 (lazy per-tenant load on demand) needs: a way to
    /// hydrate one tenant without paying the cost of loading every other
    /// tenant's data into memory.
    ///
    /// Tenant identity for loaded events comes from the file path, the
    /// same as `load_all_events` — `record_batch_to_events` stamps the
    /// passed `tenant_id` onto every reconstructed event.
    #[cfg_attr(feature = "hotpath", hotpath::measure)]
    pub fn load_events_for_tenant(&self, tenant_id: &str) -> Result<Vec<Event>> {
        let parquet_files = self.list_parquet_files_for_tenant(tenant_id)?;
        tracing::info!(
            tenant_id = tenant_id,
            file_count = parquet_files.len(),
            "load_events_for_tenant: walking tenant subtree only"
        );

        let mut events = Vec::with_capacity(parquet_files.len() * self.config.batch_size);
        let mut skipped = 0usize;
        for file_path in parquet_files {
            tracing::debug!(
                tenant_id = tenant_id,
                file = %file_path.display(),
                "load_events_for_tenant: opening file"
            );
            // Issue #166: same defensive log-and-skip as load_all_events.
            // The lazy-load path can hit a corrupt file just as easily as
            // boot can — failing the whole tenant load on one bad file
            // would brick every query for that tenant.
            match self.load_events_from_file(&file_path, tenant_id) {
                Ok(file_events) => events.extend(file_events),
                Err(e) => {
                    tracing::error!(
                        tenant_id = tenant_id,
                        file = %file_path.display(),
                        error = %e,
                        "Skipping unreadable parquet file in tenant subtree"
                    );
                    skipped += 1;
                }
            }
        }

        tracing::info!(
            tenant_id = tenant_id,
            event_count = events.len(),
            skipped_files = skipped,
            "load_events_for_tenant: complete"
        );
        Ok(events)
    }

    /// Get the storage directory path.
    pub fn storage_dir(&self) -> &Path {
        &self.storage_dir
    }

    /// One-shot migration of flat-layout files into the tenant-partitioned
    /// tree. Run with Core stopped (no concurrent writes).
    ///
    /// Walks `storage_dir`'s top level (non-recursive) for the legacy
    /// `events-*.parquet` files. For each one it loads the events,
    /// regroups them by (tenant_id, yyyy-mm) — events from a flat file
    /// take the path-derived "default" tenant, since pre-partitioning
    /// data carried no tenant in its on-disk form — writes a fresh
    /// Parquet under the corresponding partition directory, and deletes
    /// the original flat file once the new file is closed.
    ///
    /// `dry_run = true` reports what would happen without touching disk.
    /// Run dry first; production data deserves the rehearsal.
    ///
    /// Crash safety: this writes the new partition file before deleting
    /// the flat file, so a crash between the two leaves both on disk.
    /// The recursive loader (`load_all_events`) will then return both,
    /// duplicating those events on next boot. Mitigation: stop Core
    /// before running, and re-run the migration after any crash so the
    /// flat file gets deleted. A future commit can add atomic rename +
    /// fsync semantics; for the one-time migration the stop-Core
    /// constraint is enough.
    pub fn migrate_flat_layout(&self, dry_run: bool) -> Result<MigrationReport> {
        let flat_files = list_flat_layout_files(&self.storage_dir)?;
        let mut report = MigrationReport {
            dry_run,
            ..Default::default()
        };

        for flat_file in flat_files {
            // Pre-partition events used path-derived tenant. For flat-layout
            // files that's always "default" (`tenant_id_from_path` falls back
            // to "default" for single-component paths).
            let events = self.load_events_from_file(&flat_file, "default")?;
            report.flat_files_seen += 1;

            if events.is_empty() {
                // Stale empty file (zero rows). Just remove it.
                if !dry_run {
                    fs::remove_file(&flat_file).map_err(|e| {
                        AllSourceError::StorageError(format!(
                            "Failed to remove empty flat file {}: {e}",
                            flat_file.display()
                        ))
                    })?;
                }
                report.flat_files_removed += 1;
                continue;
            }

            // Group by (tenant, yyyy-mm-from-event-timestamp). The
            // partition month tracks the event's wall-clock time so that
            // post-migration the layout reflects when data happened, not
            // when migration ran. Step 4 (per-tenant snapshots) and Step
            // 5 (retention) will key on that.
            let mut groups: HashMap<(String, String), Vec<Event>> = HashMap::new();
            for event in events {
                let key = (
                    event.tenant_id_str().to_string(),
                    event.timestamp().format("%Y-%m").to_string(),
                );
                groups.entry(key).or_default().push(event);
            }

            for ((tenant, yyyy_mm), group_events) in groups {
                let count = group_events.len();
                if !dry_run {
                    let safe_tenant = sanitize_tenant_id_for_path(&tenant)?;
                    let target_dir = self.storage_dir.join(safe_tenant).join(&yyyy_mm);
                    fs::create_dir_all(&target_dir).map_err(|e| {
                        AllSourceError::StorageError(format!(
                            "Failed to create partition {}: {e}",
                            target_dir.display()
                        ))
                    })?;
                    let filename = format!(
                        "events-{}-{}.parquet",
                        chrono::Utc::now().format("%Y%m%d-%H%M%S%3f"),
                        uuid::Uuid::new_v4().as_simple()
                    );
                    let target_path = target_dir.join(&filename);
                    let record_batch = self.events_to_record_batch(&group_events)?;
                    let file = File::create(&target_path).map_err(|e| {
                        AllSourceError::StorageError(format!(
                            "Failed to create migration target {}: {e}",
                            target_path.display()
                        ))
                    })?;
                    let props = WriterProperties::builder()
                        .set_compression(self.config.compression)
                        .build();
                    let mut writer = ArrowWriter::try_new(file, self.schema.clone(), Some(props))?;
                    writer.write(&record_batch)?;
                    writer.close()?;
                    report.partitions_written += 1;
                }
                report.events_migrated += count;
            }

            if !dry_run {
                fs::remove_file(&flat_file).map_err(|e| {
                    AllSourceError::StorageError(format!(
                        "Failed to remove flat file {} after migration: {e}",
                        flat_file.display()
                    ))
                })?;
                report.flat_files_removed += 1;
            }
        }

        Ok(report)
    }

    /// Get storage statistics
    pub fn stats(&self) -> Result<StorageStats> {
        let parquet_files = find_parquet_files_recursive(&self.storage_dir)?;
        let mut total_size_bytes = 0u64;
        for path in &parquet_files {
            if let Ok(metadata) = fs::metadata(path) {
                total_size_bytes += metadata.len();
            }
        }

        let current_batch_size: usize = self
            .current_batches
            .lock()
            .unwrap()
            .values()
            .map(Vec::len)
            .sum();

        Ok(StorageStats {
            total_files: parquet_files.len(),
            total_size_bytes,
            storage_dir: self.storage_dir.clone(),
            current_batch_size,
        })
    }
}

/// Validate a tenant ID for use as a filesystem path component.
///
/// Whitelist: ASCII letters, digits, `-`, `_`, `.` — covers UUIDs, the
/// hyphen-and-lowercase tenant strings the onboarding flow produces, and
/// the `system` tenant the heartbeat emitter uses. Rejects empty input,
/// any path separator (`/`, `\`), and any "..". The whitelist is the
/// primary defence against path traversal; the explicit ".." check is
/// belt-and-braces in case the whitelist ever loosens.
///
/// Length capped at 128 bytes — comfortably above the 36-byte UUID and
/// the longest onboarding tenant the system has produced, well below
/// every common filesystem's NAME_MAX (typically 255).
fn sanitize_tenant_id_for_path(tenant_id: &str) -> Result<&str> {
    if tenant_id.is_empty() {
        return Err(AllSourceError::StorageError(
            "tenant_id is empty (cannot derive partition path)".to_string(),
        ));
    }
    if tenant_id.len() > 128 {
        return Err(AllSourceError::StorageError(format!(
            "tenant_id is too long for partition path: {} bytes (max 128)",
            tenant_id.len()
        )));
    }
    if tenant_id == "." || tenant_id == ".." {
        return Err(AllSourceError::StorageError(format!(
            "tenant_id {tenant_id:?} is reserved"
        )));
    }
    for c in tenant_id.chars() {
        let ok = c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.';
        if !ok {
            return Err(AllSourceError::StorageError(format!(
                "tenant_id {tenant_id:?} contains disallowed character {c:?} for partition path"
            )));
        }
    }
    Ok(tenant_id)
}

/// Resolve the directory a flush should write into for `(tenant, when)`.
///
/// Returns `<root>/<tenant>/<yyyy-mm>/`. Caller is responsible for
/// `create_dir_all`-ing the result before opening files in it.
fn partition_path_for_tenant(
    root: &Path,
    tenant_id: &str,
    when: chrono::DateTime<chrono::Utc>,
) -> Result<PathBuf> {
    let safe = sanitize_tenant_id_for_path(tenant_id)?;
    Ok(root.join(safe).join(when.format("%Y-%m").to_string()))
}

/// Reverse of `partition_path_for_tenant` — given a parquet file's full
/// path and the storage root, return the tenant_id stored in the path.
///
/// Tenant-partitioned shape: `<root>/<tenant>/<yyyy-mm>/events-*.parquet`
/// → first component after root is the tenant.
///
/// Legacy flat shape: `<root>/events-*.parquet` → no tenant in path, fall
/// back to `"default"` so events written before the partitioning change
/// keep loading with their original (and only ever) tenant identity.
fn tenant_id_from_path(root: &Path, file_path: &Path) -> String {
    let Ok(rel) = file_path.strip_prefix(root) else {
        return "default".to_string();
    };
    let mut comps = rel.components();
    let first = comps.next();
    let next = comps.next();
    match (first, next) {
        // Two or more components: <tenant>/<rest>... → tenant
        (Some(std::path::Component::Normal(tenant)), Some(_)) => {
            tenant.to_string_lossy().into_owned()
        }
        // Single component (the parquet file itself): legacy flat layout.
        _ => "default".to_string(),
    }
}

/// List Parquet files at the top level of `root` only — i.e. the legacy
/// flat-layout files. Used by the one-shot migration tool to find data
/// that needs moving into the tenant-partitioned tree. The opposite of
/// `find_parquet_files_recursive`: stops at the first directory level so
/// already-partitioned data isn't included.
fn list_flat_layout_files(root: &Path) -> Result<Vec<PathBuf>> {
    let entries = fs::read_dir(root).map_err(|e| {
        AllSourceError::StorageError(format!("Failed to read storage directory: {e}"))
    })?;
    let mut out: Vec<PathBuf> = entries
        .filter_map(std::result::Result::ok)
        .filter_map(|entry| {
            let ft = entry.file_type().ok()?;
            if !ft.is_file() {
                return None;
            }
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("parquet") {
                Some(path)
            } else {
                None
            }
        })
        .collect();
    out.sort();
    Ok(out)
}

/// Recursively collect all `*.parquet` files under `root`, sorted by path so
/// callers see a deterministic, tenant-grouped order.
///
/// Existence rationale: the storage layout is moving from a flat
/// `storage_dir/events-*.parquet` pile to a tenant-partitioned tree of the
/// shape `storage_dir/<tenant>/<yyyy-mm>/events-*.parquet`. During the
/// migration both shapes coexist, so every code path that asks "what
/// parquet files do we have?" needs to walk subdirectories. Symlinks are
/// not followed — the storage tree is mounted from a single volume and
/// chasing symlinks invites cycles.
fn find_parquet_files_recursive(root: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            // Root must exist (we created it in `new`); subdirectories may
            // race a delete from compaction. Skip vanished subdirs rather
            // than failing the whole load.
            Err(e) if dir == root => {
                return Err(AllSourceError::StorageError(format!(
                    "Failed to read storage directory: {e}"
                )));
            }
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            // Use file_type() rather than metadata() so symlinks don't get
            // followed by accident (metadata() resolves symlinks, file_type()
            // doesn't).
            let Ok(ft) = entry.file_type() else {
                continue;
            };
            if ft.is_dir() {
                stack.push(path);
            } else if ft.is_file()
                && path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ext == "parquet")
            {
                out.push(path);
            }
        }
    }

    out.sort();
    Ok(out)
}

impl Drop for ParquetStorage {
    fn drop(&mut self) {
        // Ensure any remaining events are flushed on shutdown
        if let Err(e) = self.flush_on_shutdown() {
            tracing::error!("Failed to flush events on drop: {}", e);
        }
    }
}

/// Outcome of a `migrate_flat_layout` run.
#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct MigrationReport {
    /// Whether the run was a rehearsal (no disk changes).
    pub dry_run: bool,
    /// Number of legacy flat-layout files discovered.
    pub flat_files_seen: usize,
    /// Number of legacy flat files deleted (always 0 when `dry_run`).
    pub flat_files_removed: usize,
    /// Number of new partition files written under the tenant tree.
    pub partitions_written: usize,
    /// Total events copied into the new tree (counted in dry-run too).
    pub events_migrated: usize,
}

#[derive(Debug, serde::Serialize)]
pub struct StorageStats {
    pub total_files: usize,
    pub total_size_bytes: u64,
    pub storage_dir: PathBuf,
    pub current_batch_size: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn create_test_event(entity_id: &str) -> Event {
        Event::reconstruct_from_strings(
            uuid::Uuid::new_v4(),
            "test.event".to_string(),
            entity_id.to_string(),
            "default".to_string(),
            json!({
                "test": "data",
                "value": 42
            }),
            chrono::Utc::now(),
            None,
            1,
        )
    }

    #[test]
    fn test_parquet_storage_write_read() {
        let temp_dir = TempDir::new().unwrap();
        let storage = ParquetStorage::new(temp_dir.path()).unwrap();

        // Add events
        for i in 0..10 {
            let event = create_test_event(&format!("entity-{i}"));
            storage.append_event(event).unwrap();
        }

        // Flush to disk
        storage.flush().unwrap();

        // Load back
        let loaded_events = storage.load_all_events().unwrap();
        assert_eq!(loaded_events.len(), 10);
    }

    #[test]
    fn test_storage_stats() {
        let temp_dir = TempDir::new().unwrap();
        let storage = ParquetStorage::new(temp_dir.path()).unwrap();

        // Add and flush events
        for i in 0..5 {
            storage
                .append_event(create_test_event(&format!("entity-{i}")))
                .unwrap();
        }
        storage.flush().unwrap();

        let stats = storage.stats().unwrap();
        assert_eq!(stats.total_files, 1);
        assert!(stats.total_size_bytes > 0);
    }

    #[test]
    fn test_default_batch_size() {
        let temp_dir = TempDir::new().unwrap();
        let storage = ParquetStorage::new(temp_dir.path()).unwrap();

        // Default batch size should be 10,000 as per US-023
        assert_eq!(storage.batch_size(), DEFAULT_BATCH_SIZE);
        assert_eq!(storage.batch_size(), 10_000);
    }

    #[test]
    fn test_custom_config() {
        let temp_dir = TempDir::new().unwrap();
        let config = ParquetStorageConfig {
            batch_size: 5_000,
            flush_timeout: Duration::from_secs(2),
            compression: parquet::basic::Compression::SNAPPY,
        };
        let storage = ParquetStorage::with_config(temp_dir.path(), config).unwrap();

        assert_eq!(storage.batch_size(), 5_000);
        assert_eq!(storage.flush_timeout(), Duration::from_secs(2));
    }

    #[test]
    fn test_batch_write() {
        let temp_dir = TempDir::new().unwrap();
        let config = ParquetStorageConfig {
            batch_size: 100, // Small batch for testing
            ..Default::default()
        };
        let storage = ParquetStorage::with_config(temp_dir.path(), config).unwrap();

        // 250 events for a single tenant. With per-tenant flush, when the
        // tenant's pending batch crosses batch_size we drain the whole
        // tenant in one flush — not chunk at exactly batch_size like the
        // old global-batch path did. So 250 events triggers exactly one
        // size-flush (the appender pushes all 250 onto the tenant's batch
        // under one lock, sees length >= 100, schedules a flush which
        // drains everything). 0 left pending.
        let events: Vec<Event> = (0..250)
            .map(|i| create_test_event(&format!("entity-{i}")))
            .collect();

        let result = storage.batch_write(events).unwrap();
        assert_eq!(result.events_written, 250);
        assert_eq!(result.batches_flushed, 1);
        assert_eq!(storage.pending_count(), 0);

        // Manual flush is a no-op since nothing's pending.
        storage.flush().unwrap();

        // All 250 events round-trip through the tenant-partitioned tree.
        let loaded = storage.load_all_events().unwrap();
        assert_eq!(loaded.len(), 250);
    }

    #[test]
    fn test_auto_flush_on_batch_size() {
        let temp_dir = TempDir::new().unwrap();
        let config = ParquetStorageConfig {
            batch_size: 10, // Very small for testing
            ..Default::default()
        };
        let storage = ParquetStorage::with_config(temp_dir.path(), config).unwrap();

        // Add 15 events - should auto-flush at 10
        for i in 0..15 {
            storage
                .append_event(create_test_event(&format!("entity-{i}")))
                .unwrap();
        }

        // Should have 5 pending, 10 written
        assert_eq!(storage.pending_count(), 5);

        let stats = storage.batch_stats();
        assert_eq!(stats.events_written, 10);
        assert_eq!(stats.batches_written, 1);
        assert_eq!(stats.size_flushes, 1);
    }

    #[test]
    fn test_flush_on_shutdown() {
        let temp_dir = TempDir::new().unwrap();
        let storage = ParquetStorage::new(temp_dir.path()).unwrap();

        // Add some events without reaching batch size
        for i in 0..5 {
            storage
                .append_event(create_test_event(&format!("entity-{i}")))
                .unwrap();
        }

        assert_eq!(storage.pending_count(), 5);

        // Manually trigger shutdown flush
        let flushed = storage.flush_on_shutdown().unwrap();
        assert_eq!(flushed, 5);
        assert_eq!(storage.pending_count(), 0);

        // Verify events are persisted
        let loaded = storage.load_all_events().unwrap();
        assert_eq!(loaded.len(), 5);
    }

    #[test]
    fn test_thread_safe_writes() {
        let temp_dir = TempDir::new().unwrap();
        let config = ParquetStorageConfig {
            batch_size: 100,
            ..Default::default()
        };
        let storage = Arc::new(ParquetStorage::with_config(temp_dir.path(), config).unwrap());

        let events_per_thread = 50;
        let thread_count = 4;

        std::thread::scope(|s| {
            for t in 0..thread_count {
                let storage_ref = Arc::clone(&storage);
                s.spawn(move || {
                    for i in 0..events_per_thread {
                        let event = create_test_event(&format!("thread-{t}-entity-{i}"));
                        storage_ref.append_event(event).unwrap();
                    }
                });
            }
        });

        // Flush remaining
        storage.flush().unwrap();

        // All events should be written
        let loaded = storage.load_all_events().unwrap();
        assert_eq!(loaded.len(), events_per_thread * thread_count);
    }

    #[test]
    fn test_batch_stats() {
        let temp_dir = TempDir::new().unwrap();
        let config = ParquetStorageConfig {
            batch_size: 50,
            ..Default::default()
        };
        let storage = ParquetStorage::with_config(temp_dir.path(), config).unwrap();

        // 100 events, single tenant, batch_size=50. Per-tenant flush
        // drains the whole tenant on the first size trigger, so this
        // produces exactly one size-flush and one batches_written event
        // (vs. the pre-tenant world's two).
        let events: Vec<Event> = (0..100)
            .map(|i| create_test_event(&format!("entity-{i}")))
            .collect();

        storage.batch_write(events).unwrap();

        let stats = storage.batch_stats();
        assert_eq!(stats.batches_written, 1);
        assert_eq!(stats.events_written, 100);
        assert!(stats.avg_batch_size > 0.0);
        assert!(stats.events_per_sec > 0.0);
        assert_eq!(stats.size_flushes, 1);
    }

    #[test]
    fn test_config_presets() {
        let high_throughput = ParquetStorageConfig::high_throughput();
        assert_eq!(high_throughput.batch_size, 50_000);
        assert_eq!(high_throughput.flush_timeout, Duration::from_secs(10));

        let low_latency = ParquetStorageConfig::low_latency();
        assert_eq!(low_latency.batch_size, 1_000);
        assert_eq!(low_latency.flush_timeout, Duration::from_secs(1));

        let default = ParquetStorageConfig::default();
        assert_eq!(default.batch_size, DEFAULT_BATCH_SIZE);
        assert_eq!(default.batch_size, 10_000);
    }

    /// Benchmark: Compare single-event writes vs batch writes
    /// Run with: cargo test --release -- --ignored test_batch_write_throughput
    #[test]
    #[ignore]
    fn test_batch_write_throughput() {
        let temp_dir = TempDir::new().unwrap();
        let storage = ParquetStorage::new(temp_dir.path()).unwrap();

        let event_count = 50_000;

        // Benchmark batch write
        let events: Vec<Event> = (0..event_count)
            .map(|i| create_test_event(&format!("entity-{i}")))
            .collect();

        let start = std::time::Instant::now();
        let result = storage.batch_write(events).unwrap();
        storage.flush().unwrap(); // Flush any remaining
        let batch_duration = start.elapsed();

        let batch_stats = storage.batch_stats();

        println!("\n=== Parquet Batch Write Performance (BATCH_SIZE=10,000) ===");
        println!("Events: {event_count}");
        println!("Duration: {batch_duration:?}");
        println!("Events/sec: {:.0}", result.events_per_sec);
        println!("Batches written: {}", batch_stats.batches_written);
        println!("Avg batch size: {:.0}", batch_stats.avg_batch_size);
        println!("Bytes written: {} KB", batch_stats.bytes_written / 1024);

        // Target: Batch writes should achieve at least 100K events/sec in release mode
        // This represents 40%+ improvement over single-event writes
        assert!(
            result.events_per_sec > 10_000.0,
            "Batch write throughput too low: {:.0} events/sec (expected >10K in debug, >100K in release)",
            result.events_per_sec
        );
    }

    /// Benchmark: Single-event write baseline (for comparison)
    #[test]
    #[ignore]
    fn test_single_event_write_baseline() {
        let temp_dir = TempDir::new().unwrap();
        let config = ParquetStorageConfig {
            batch_size: 1, // Force flush after each event
            ..Default::default()
        };
        let storage = ParquetStorage::with_config(temp_dir.path(), config).unwrap();

        let event_count = 1_000; // Fewer events since this is slow

        let start = std::time::Instant::now();
        for i in 0..event_count {
            let event = create_test_event(&format!("entity-{i}"));
            storage.append_event(event).unwrap();
        }
        let duration = start.elapsed();

        let events_per_sec = f64::from(event_count) / duration.as_secs_f64();

        println!("\n=== Single-Event Write Baseline ===");
        println!("Events: {event_count}");
        println!("Duration: {duration:?}");
        println!("Events/sec: {events_per_sec:.0}");

        // This should be significantly slower than batch writes
        // Used as a baseline to demonstrate 40%+ improvement
    }

    // -----------------------------------------------------------------
    // Tests for the recursive parquet walker (Step 1, commit #1: read-side
    // bidirectional layout support — see SUSTAINABLE_DATA_STRATEGY.md).
    // -----------------------------------------------------------------

    /// Helper: write a tiny placeholder parquet file at an arbitrary path so
    /// the walker has something concrete to find. We only care that the
    /// walker discovers the path, not that the file is loadable here — the
    /// load path is exercised by the existing read tests.
    fn touch_parquet(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"").unwrap();
    }

    #[test]
    fn test_walker_finds_files_in_flat_layout() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();
        touch_parquet(&root.join("events-20260101-120000000-aaaa.parquet"));
        touch_parquet(&root.join("events-20260101-130000000-bbbb.parquet"));

        let mut found = find_parquet_files_recursive(root).unwrap();
        found.sort();
        assert_eq!(found.len(), 2);
        assert!(
            found[0]
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("events-"),
            "expected events-* file, got {found:?}"
        );
    }

    #[test]
    fn test_walker_finds_files_in_tenant_partitioned_tree() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();
        // Tenant-partitioned shape: storage_dir/<tenant>/<yyyy-mm>/events-*.parquet
        touch_parquet(&root.join("tenant-a/2026-01/events-20260101-120000000-aaaa.parquet"));
        touch_parquet(&root.join("tenant-a/2026-02/events-20260201-120000000-bbbb.parquet"));
        touch_parquet(&root.join("tenant-b/2026-01/events-20260103-120000000-cccc.parquet"));

        let found = find_parquet_files_recursive(root).unwrap();
        assert_eq!(found.len(), 3);
        // Sort places tenant-a files before tenant-b — that's the
        // tenant-grouping the docs claim.
        assert!(found[0].to_str().unwrap().contains("tenant-a"));
        assert!(found[1].to_str().unwrap().contains("tenant-a"));
        assert!(found[2].to_str().unwrap().contains("tenant-b"));
    }

    #[test]
    fn test_walker_handles_mixed_legacy_and_partitioned_layouts() {
        // The migration window: some tenants have been moved into the tree,
        // some flat files still sit at the root. The walker must surface
        // both so load_all_events sees every event regardless of where it
        // currently lives.
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();
        touch_parquet(&root.join("events-legacy-aaaa.parquet"));
        touch_parquet(&root.join("tenant-a/2026-01/events-new-bbbb.parquet"));

        let found = find_parquet_files_recursive(root).unwrap();
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn test_walker_ignores_non_parquet_files() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();
        std::fs::write(root.join("README.md"), b"hello").unwrap();
        std::fs::write(root.join("events.json"), b"[]").unwrap();
        touch_parquet(&root.join("events-20260101-120000000-aaaa.parquet"));
        // Files that just happen to have "parquet" in the name but no
        // .parquet extension stay out — extension-only filter, no name match.
        std::fs::write(root.join("not-a-parquet-file.bin"), b"").unwrap();

        let found = find_parquet_files_recursive(root).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].extension().and_then(|s| s.to_str()),
            Some("parquet")
        );
    }

    /// Build an event whose tenant_id and entity_id we control, so tests
    /// can verify per-tenant routing without depending on the helper that
    /// hardcodes "default".
    fn event_with_tenant(tenant: &str, entity_id: &str) -> Event {
        Event::reconstruct_from_strings(
            uuid::Uuid::new_v4(),
            "test.event".to_string(),
            entity_id.to_string(),
            tenant.to_string(),
            json!({"k": "v"}),
            chrono::Utc::now(),
            None,
            1,
        )
    }

    #[test]
    fn test_flush_writes_into_per_tenant_partition() {
        // End-to-end check that the new write path produces
        // <root>/<tenant>/<yyyy-mm>/events-*.parquet — no flat file at the
        // root, no cross-tenant mixing.
        let temp_dir = TempDir::new().unwrap();
        let storage = ParquetStorage::new(temp_dir.path()).unwrap();

        for i in 0..3 {
            storage
                .append_event(event_with_tenant("default", &format!("entity-{i}")))
                .unwrap();
        }
        storage.flush().unwrap();

        let parquet_files = find_parquet_files_recursive(temp_dir.path()).unwrap();
        assert_eq!(parquet_files.len(), 1);

        let rel = parquet_files[0]
            .strip_prefix(temp_dir.path())
            .unwrap()
            .to_string_lossy()
            .into_owned();
        // Path shape: default/<yyyy-mm>/events-*.parquet
        let parts: Vec<&str> = rel.split(std::path::MAIN_SEPARATOR).collect();
        assert_eq!(parts.len(), 3, "expected tenant/yyyy-mm/file, got {rel}");
        assert_eq!(parts[0], "default");
        // yyyy-mm is two digits dash four — loose check, exact month
        // varies with wall-clock at test runtime.
        assert!(
            parts[1].len() == 7 && parts[1].as_bytes()[4] == b'-',
            "expected yyyy-mm, got {}",
            parts[1]
        );
        assert!(parts[2].starts_with("events-") && parts[2].ends_with(".parquet"));

        let loaded = storage.load_all_events().unwrap();
        assert_eq!(loaded.len(), 3);
    }

    #[test]
    fn test_multiple_tenants_get_isolated_subtrees() {
        // Per-tenant flush must not mix tenants into the same Parquet file
        // and must put each tenant under its own subdirectory.
        let temp_dir = TempDir::new().unwrap();
        let storage = ParquetStorage::new(temp_dir.path()).unwrap();

        for i in 0..2 {
            storage
                .append_event(event_with_tenant("alice", &format!("a-{i}")))
                .unwrap();
        }
        for i in 0..3 {
            storage
                .append_event(event_with_tenant("bob", &format!("b-{i}")))
                .unwrap();
        }
        storage.flush().unwrap();

        let alice_subtree = temp_dir.path().join("alice");
        let bob_subtree = temp_dir.path().join("bob");
        assert!(alice_subtree.is_dir(), "alice should have its own subtree");
        assert!(bob_subtree.is_dir(), "bob should have its own subtree");

        let alice_files = find_parquet_files_recursive(&alice_subtree).unwrap();
        let bob_files = find_parquet_files_recursive(&bob_subtree).unwrap();
        assert_eq!(alice_files.len(), 1);
        assert_eq!(bob_files.len(), 1);

        // Loaded events keep their tenant_id — round-trip preserves which
        // tenant each event belonged to.
        let loaded = storage.load_all_events().unwrap();
        let (alice_count, bob_count) =
            loaded
                .iter()
                .fold((0, 0), |(a, b), e| match e.tenant_id_str() {
                    "alice" => (a + 1, b),
                    "bob" => (a, b + 1),
                    _ => (a, b),
                });
        assert_eq!(alice_count, 2);
        assert_eq!(bob_count, 3);
    }

    #[test]
    fn test_size_flush_only_drains_full_tenant() {
        // When one tenant exactly hits batch_size, only that tenant
        // flushes; the other tenant keeps its events buffered. Prevents
        // one noisy tenant from causing fragmented writes for everyone.
        let temp_dir = TempDir::new().unwrap();
        let config = ParquetStorageConfig {
            batch_size: 5,
            ..Default::default()
        };
        let storage = ParquetStorage::with_config(temp_dir.path(), config).unwrap();

        // Alice: 5 events → on the 5th, len == batch_size triggers flush
        // which drains all 5. Alice ends empty.
        for i in 0..5 {
            storage
                .append_event(event_with_tenant("alice", &format!("a-{i}")))
                .unwrap();
        }
        // Bob: 2 events → still under threshold, stays pending.
        for i in 0..2 {
            storage
                .append_event(event_with_tenant("bob", &format!("b-{i}")))
                .unwrap();
        }

        assert_eq!(
            storage.pending_count(),
            2,
            "only bob's 2 events should be pending"
        );

        let parquet_files = find_parquet_files_recursive(temp_dir.path()).unwrap();
        assert_eq!(parquet_files.len(), 1, "only alice should have flushed");
        assert!(
            parquet_files[0]
                .to_string_lossy()
                .contains(&format!("alice{}", std::path::MAIN_SEPARATOR)),
            "expected alice partition, got {}",
            parquet_files[0].display()
        );
    }

    #[test]
    fn test_tenant_id_from_path_recovers_tenant_for_partitioned_files() {
        let root = Path::new("/data/storage");
        let f = Path::new("/data/storage/alice/2026-04/events-20260426-120000000-aaaa.parquet");
        assert_eq!(tenant_id_from_path(root, f), "alice");
    }

    #[test]
    fn test_tenant_id_from_path_falls_back_to_default_for_legacy_flat_layout() {
        let root = Path::new("/data/storage");
        let f = Path::new("/data/storage/events-20260426-120000000-aaaa.parquet");
        // Legacy single-component path. Pre-tenant data was always
        // commingled with tenant=default, so default is the right fallback.
        assert_eq!(tenant_id_from_path(root, f), "default");
    }

    #[test]
    fn test_sanitize_tenant_id_for_path_accepts_safe_inputs() {
        for ok in [
            "default",
            "system",
            "1e6b2d1c-2f64-4441-9cf9-42f2e451aa17",
            "onboard-diagnostic-160-at-example-com",
            "tenant_with_underscore",
            "v1.0",
        ] {
            assert!(
                sanitize_tenant_id_for_path(ok).is_ok(),
                "{ok:?} should be accepted"
            );
        }
    }

    #[test]
    fn test_sanitize_tenant_id_for_path_rejects_unsafe_inputs() {
        for bad in [
            "",         // empty
            "..",       // parent traversal
            ".",        // current dir
            "foo/bar",  // path separator
            "foo\\bar", // windows-style separator
            "foo bar",  // whitespace
            "foo\nbar", // newline
            "foo\0bar", // null byte
            "tenant?",  // shell glob char
            "tenant*",  // shell glob char
        ] {
            assert!(
                sanitize_tenant_id_for_path(bad).is_err(),
                "{bad:?} should be rejected"
            );
        }

        // Length cap.
        let too_long = "a".repeat(129);
        assert!(sanitize_tenant_id_for_path(&too_long).is_err());
    }

    #[test]
    fn test_partition_path_for_tenant_shape() {
        let root = Path::new("/data");
        let when = chrono::DateTime::parse_from_rfc3339("2026-04-26T12:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let path = partition_path_for_tenant(root, "alice", when).unwrap();
        assert_eq!(path, Path::new("/data/alice/2026-04"));
    }

    #[test]
    fn test_append_event_rejects_unsafe_tenant_at_flush() {
        // Defence in depth: even if some upstream forgets to validate, the
        // sanitizer in flush_tenant catches it. Since append doesn't write
        // synchronously, the bad tenant is rejected on the first flush
        // attempt. We test that flush surfaces an error rather than
        // silently writing somewhere weird.
        let temp_dir = TempDir::new().unwrap();
        let storage = ParquetStorage::new(temp_dir.path()).unwrap();

        // append accepts whatever tenant_id the event carries — domain
        // construction would normally reject this, but if it slipped
        // through, flush should refuse to derive a path from it.
        storage
            .append_event(event_with_tenant("../escape", "e-0"))
            .unwrap();
        let result = storage.flush();
        assert!(result.is_err(), "flush should reject unsafe tenant_id");
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("disallowed character") || msg.contains("reserved"),
            "expected sanitization error message, got: {msg}"
        );
    }

    // -----------------------------------------------------------------
    // Tenant-pruned read tests (Step 1, commit #4).
    // -----------------------------------------------------------------

    #[test]
    fn test_load_events_for_tenant_only_walks_target_subtree() {
        // Seed three tenants with distinct event counts. Loading one
        // tenant must return only that tenant's events — and the file
        // list helper must report only that tenant's files (the strong
        // form of "didn't open the others").
        let temp_dir = TempDir::new().unwrap();
        let storage = ParquetStorage::new(temp_dir.path()).unwrap();

        for i in 0..2 {
            storage
                .append_event(event_with_tenant("alice", &format!("a-{i}")))
                .unwrap();
        }
        for i in 0..3 {
            storage
                .append_event(event_with_tenant("bob", &format!("b-{i}")))
                .unwrap();
        }
        for i in 0..1 {
            storage
                .append_event(event_with_tenant("carol", &format!("c-{i}")))
                .unwrap();
        }
        storage.flush().unwrap();

        let alice_files = storage.list_parquet_files_for_tenant("alice").unwrap();
        assert_eq!(alice_files.len(), 1);
        assert!(
            alice_files[0]
                .to_string_lossy()
                .contains(&format!("alice{}", std::path::MAIN_SEPARATOR)),
            "expected alice file, got {}",
            alice_files[0].display()
        );
        // The pruned listing must NOT include any bob/carol files — this
        // is the property Step 2 will rely on to avoid loading every
        // tenant's data on a single-tenant query.
        for f in &alice_files {
            let s = f.to_string_lossy();
            assert!(!s.contains("bob"), "alice listing leaked bob file: {s}");
            assert!(!s.contains("carol"), "alice listing leaked carol file: {s}");
        }

        let alice_events = storage.load_events_for_tenant("alice").unwrap();
        assert_eq!(alice_events.len(), 2);
        for e in &alice_events {
            assert_eq!(e.tenant_id_str(), "alice");
        }

        let bob_events = storage.load_events_for_tenant("bob").unwrap();
        assert_eq!(bob_events.len(), 3);
        for e in &bob_events {
            assert_eq!(e.tenant_id_str(), "bob");
        }

        let carol_events = storage.load_events_for_tenant("carol").unwrap();
        assert_eq!(carol_events.len(), 1);
        assert_eq!(carol_events[0].tenant_id_str(), "carol");
    }

    #[test]
    fn test_load_events_for_tenant_returns_empty_when_subtree_missing() {
        // Querying a tenant that has never written must not error — it's
        // a normal "no data" case, not a misconfiguration. Important for
        // first-query latency on a fresh tenant.
        let temp_dir = TempDir::new().unwrap();
        let storage = ParquetStorage::new(temp_dir.path()).unwrap();

        // Seed only alice so the storage_dir isn't empty (rule out the
        // empty-dir trivial case).
        storage
            .append_event(event_with_tenant("alice", "a-0"))
            .unwrap();
        storage.flush().unwrap();

        let files = storage
            .list_parquet_files_for_tenant("nobody-here")
            .unwrap();
        assert!(files.is_empty());

        let events = storage.load_events_for_tenant("nobody-here").unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn test_load_events_for_tenant_rejects_unsafe_tenant_id() {
        // Path traversal must fail at the API boundary, not after disk
        // reads. Same whitelist as the write path.
        let temp_dir = TempDir::new().unwrap();
        let storage = ParquetStorage::new(temp_dir.path()).unwrap();

        for unsafe_tid in ["..", "a/b", "a\\b", "", "a..b/.."] {
            let result = storage.load_events_for_tenant(unsafe_tid);
            assert!(
                result.is_err(),
                "tenant_id {unsafe_tid:?} should have been rejected"
            );
        }
    }

    #[test]
    fn test_load_events_for_tenant_ignores_legacy_flat_layout_files() {
        // Flat-layout files at the storage root predate partitioning. A
        // tenant-scoped load must not pick them up — the migration tool
        // is what relocates them under default/. Until it runs, those
        // files are invisible to per-tenant queries (correct behavior:
        // the system has no way to tell which tenant they belong to
        // beyond "default", and pretending otherwise would mis-attribute
        // them).
        let temp_dir = TempDir::new().unwrap();
        let storage = ParquetStorage::new(temp_dir.path()).unwrap();

        // Seed a flat-layout file (relocates default/<yyyy-mm>/ → root).
        let _flat = seed_flat_layout_file(&storage, 4);

        // Querying default returns nothing — the flat file at the root
        // isn't under default/.
        let default_events = storage.load_events_for_tenant("default").unwrap();
        assert!(
            default_events.is_empty(),
            "tenant-scoped load must not pick up flat-layout files; got {} events",
            default_events.len()
        );

        // Sanity: the full loader still sees them via the recursive walk.
        let all_events = storage.load_all_events().unwrap();
        assert_eq!(all_events.len(), 4);
    }

    // -----------------------------------------------------------------
    // Atomic snapshot write tests (Step 4, commit #1).
    // -----------------------------------------------------------------

    #[test]
    fn test_write_atomic_parquet_emits_file_under_tenant_partition() {
        // Happy path: write 3 events for tenant alice, get back a
        // path under <root>/alice/<yyyy-mm>/. The .tmp file should
        // be gone (rename completed); the final file readable via
        // load_events_for_tenant.
        let temp_dir = TempDir::new().unwrap();
        let storage = ParquetStorage::new(temp_dir.path()).unwrap();

        let events: Vec<Event> = (0..3)
            .map(|i| event_with_tenant("alice", &format!("a-{i}")))
            .collect();

        let final_path = storage
            .write_atomic_parquet("alice", "snapshot.alice.range", &events)
            .unwrap();

        // Path shape: <root>/alice/<yyyy-mm>/snapshot.alice.range.parquet
        let rel = final_path
            .strip_prefix(temp_dir.path())
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let parts: Vec<&str> = rel.split(std::path::MAIN_SEPARATOR).collect();
        assert_eq!(parts.len(), 3, "expected tenant/yyyy-mm/file, got {rel}");
        assert_eq!(parts[0], "alice");
        assert_eq!(parts[2], "snapshot.alice.range.parquet");

        // Final file must exist; .tmp must NOT.
        assert!(final_path.is_file());
        let tmp = final_path.with_extension("parquet.tmp");
        assert!(
            !tmp.exists(),
            "tmp should have been renamed away; still at {}",
            tmp.display()
        );

        // Loadable via the tenant loader.
        let loaded = storage.load_events_for_tenant("alice").unwrap();
        assert_eq!(loaded.len(), 3);
    }

    #[test]
    fn test_write_atomic_parquet_rejects_empty_events() {
        let temp_dir = TempDir::new().unwrap();
        let storage = ParquetStorage::new(temp_dir.path()).unwrap();
        let result = storage.write_atomic_parquet("alice", "snap", &[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_write_atomic_parquet_rejects_unsafe_tenant() {
        let temp_dir = TempDir::new().unwrap();
        let storage = ParquetStorage::new(temp_dir.path()).unwrap();
        let events = [event_with_tenant("alice", "e-0")];
        for unsafe_tid in ["..", "a/b", ""] {
            let result = storage.write_atomic_parquet(unsafe_tid, "snap", &events);
            assert!(
                result.is_err(),
                "unsafe tenant_id {unsafe_tid:?} should have been rejected"
            );
        }
    }

    #[test]
    fn test_cleanup_partial_writes_removes_orphan_tmps() {
        // Simulate a crashed mid-snapshot: pretend we have a leftover
        // events-x.parquet.tmp file in a tenant partition. Cleanup
        // must delete it without touching real .parquet files.
        let temp_dir = TempDir::new().unwrap();
        let storage = ParquetStorage::new(temp_dir.path()).unwrap();

        // Seed a real parquet via a normal flush, so we have one
        // legit file we don't want to delete.
        for i in 0..2 {
            storage
                .append_event(event_with_tenant("alice", &format!("a-{i}")))
                .unwrap();
        }
        storage.flush().unwrap();
        let real_files_before = find_parquet_files_recursive(temp_dir.path()).unwrap();
        assert_eq!(real_files_before.len(), 1);

        // Manufacture an orphan .tmp file in alice's partition.
        let alice_subtree = temp_dir.path().join("alice");
        let orphan_dir = real_files_before[0].parent().unwrap();
        let orphan_path = orphan_dir.join("snapshot.alice.crashed.parquet.tmp");
        std::fs::write(&orphan_path, b"fake partial parquet").unwrap();
        assert!(orphan_path.is_file());

        // And one nested deeper, just to confirm recursion.
        let nested_dir = alice_subtree.join("2099-01");
        std::fs::create_dir_all(&nested_dir).unwrap();
        let nested_orphan = nested_dir.join("events-x.parquet.tmp");
        std::fs::write(&nested_orphan, b"junk").unwrap();

        let removed = storage.cleanup_partial_writes().unwrap();
        assert_eq!(removed, 2, "two orphan tmps should have been cleaned");
        assert!(!orphan_path.exists());
        assert!(!nested_orphan.exists());

        // Real parquet untouched.
        let real_files_after = find_parquet_files_recursive(temp_dir.path()).unwrap();
        assert_eq!(real_files_after, real_files_before);
    }

    #[test]
    fn test_cleanup_partial_writes_quarantines_zero_byte_parquet() {
        // Issue #166 regression: a pre-fix checkpoint crash left a 0-byte
        // events-*.parquet file with the FINAL extension (no .tmp). The
        // cleanup pass must rename it aside rather than delete, so an
        // operator can inspect.
        let temp_dir = TempDir::new().unwrap();
        let storage = ParquetStorage::new(temp_dir.path()).unwrap();

        // Seed a real parquet alongside, to confirm we don't molest healthy data.
        for i in 0..2 {
            storage
                .append_event(event_with_tenant("alice", &format!("a-{i}")))
                .unwrap();
        }
        storage.flush().unwrap();
        let healthy_files_before = find_parquet_files_recursive(temp_dir.path()).unwrap();
        assert_eq!(healthy_files_before.len(), 1);
        let healthy_file = &healthy_files_before[0];
        let healthy_dir = healthy_file.parent().unwrap();

        // Drop a 0-byte parquet next to the healthy one (the issue's failure mode).
        let bricked = healthy_dir.join("events-bricked-deadbeef.parquet");
        std::fs::write(&bricked, b"").unwrap();
        assert_eq!(std::fs::metadata(&bricked).unwrap().len(), 0);

        let acted = storage.cleanup_partial_writes().unwrap();
        assert_eq!(acted, 1, "only the 0-byte file should have been acted on");

        // 0-byte file is gone from its original name, but a quarantined
        // sibling exists.
        assert!(!bricked.exists(), "0-byte file should have been renamed");
        let quarantined: Vec<_> = std::fs::read_dir(healthy_dir)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("events-bricked-deadbeef.parquet.corrupt-"))
            })
            .collect();
        assert_eq!(
            quarantined.len(),
            1,
            "expected one .parquet.corrupt-<ts> sibling"
        );

        // Healthy file untouched.
        assert!(
            healthy_file.exists(),
            "healthy parquet must not be molested"
        );
    }

    #[test]
    fn test_load_all_events_skips_zero_byte_parquet() {
        // Issue #166 defensive layer: a single bad file must not abort
        // the whole load. With t-d8b1's match-and-continue, the healthy
        // events still come back and the bad file is logged-and-skipped.
        let temp_dir = TempDir::new().unwrap();
        let storage = ParquetStorage::new(temp_dir.path()).unwrap();

        // Two healthy events, flushed.
        for i in 0..2 {
            storage
                .append_event(event_with_tenant("alice", &format!("a-{i}")))
                .unwrap();
        }
        storage.flush().unwrap();

        // Now drop a 0-byte parquet alongside (sneaking it in AFTER
        // construction so cleanup_partial_writes doesn't rename it
        // before we get to test the load path).
        let healthy_files = find_parquet_files_recursive(temp_dir.path()).unwrap();
        let bricked = healthy_files[0]
            .parent()
            .unwrap()
            .join("events-bricked-cafef00d.parquet");
        std::fs::write(&bricked, b"").unwrap();

        let loaded = storage.load_all_events().unwrap();
        assert_eq!(
            loaded.len(),
            2,
            "all healthy events should still load despite the 0-byte file"
        );
    }

    #[test]
    fn test_load_events_for_tenant_skips_zero_byte_parquet() {
        // Same defensive layer for the lazy-load path. A bad file in
        // the tenant subtree must not poison subsequent queries.
        let temp_dir = TempDir::new().unwrap();
        let storage = ParquetStorage::new(temp_dir.path()).unwrap();

        for i in 0..3 {
            storage
                .append_event(event_with_tenant("alice", &format!("a-{i}")))
                .unwrap();
        }
        storage.flush().unwrap();

        let healthy = find_parquet_files_recursive(temp_dir.path()).unwrap();
        let bricked = healthy[0]
            .parent()
            .unwrap()
            .join("events-bricked-feedface.parquet");
        std::fs::write(&bricked, b"").unwrap();

        let events = storage.load_events_for_tenant("alice").unwrap();
        assert_eq!(events.len(), 3, "lazy load must skip the 0-byte file");
    }

    #[test]
    fn test_flush_tenant_leaves_no_zero_byte_parquet_after_normal_flush() {
        // Direct verification of t-c1d3: a successful flush_tenant
        // leaves exactly one healthy *.parquet — no .tmp survivors,
        // no 0-byte files.
        let temp_dir = TempDir::new().unwrap();
        let storage = ParquetStorage::new(temp_dir.path()).unwrap();

        for i in 0..5 {
            storage
                .append_event(event_with_tenant("alice", &format!("a-{i}")))
                .unwrap();
        }
        storage.flush().unwrap();

        let mut tmp_count = 0;
        let mut zero_byte_count = 0;
        let mut healthy_count = 0;
        let mut stack = vec![temp_dir.path().to_path_buf()];
        while let Some(d) = stack.pop() {
            for entry in std::fs::read_dir(&d).unwrap().flatten() {
                let p = entry.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                let name = p.file_name().unwrap().to_string_lossy().into_owned();
                if name.ends_with(".parquet.tmp") {
                    tmp_count += 1;
                } else if name.ends_with(".parquet") {
                    if std::fs::metadata(&p).unwrap().len() == 0 {
                        zero_byte_count += 1;
                    } else {
                        healthy_count += 1;
                    }
                }
            }
        }
        assert_eq!(tmp_count, 0, ".parquet.tmp survivors after flush");
        assert_eq!(zero_byte_count, 0, "0-byte .parquet survivors after flush");
        assert_eq!(healthy_count, 1, "expected exactly one healthy parquet");
    }

    #[test]
    fn test_new_calls_cleanup_partial_writes_on_boot() {
        // Drop a stale .tmp into a directory, then construct a
        // fresh ParquetStorage on it. The constructor must clean
        // it up.
        let temp_dir = TempDir::new().unwrap();
        let stale = temp_dir.path().join("orphan.parquet.tmp");
        std::fs::write(&stale, b"crash detritus").unwrap();
        assert!(stale.is_file());

        let _storage = ParquetStorage::new(temp_dir.path()).unwrap();
        assert!(
            !stale.exists(),
            "stale tmp should have been cleaned by ParquetStorage::new"
        );
    }

    // -----------------------------------------------------------------
    // Migration tests (Step 1, commit #3: flat → tenant-tree migration).
    // -----------------------------------------------------------------

    /// Helper: produce a flat-layout Parquet file at the storage root,
    /// matching what pre-#2 deploys wrote. Uses the existing flush path
    /// briefly and then relocates the resulting file from
    /// default/<yyyy-mm>/ back up to the root, simulating the legacy
    /// state.
    fn seed_flat_layout_file(storage: &ParquetStorage, count: usize) -> PathBuf {
        for i in 0..count {
            storage
                .append_event(create_test_event(&format!("entity-{i}")))
                .unwrap();
        }
        storage.flush().unwrap();

        // create_test_event uses tenant="default", so the just-flushed file
        // landed under <root>/default/<yyyy-mm>/. Find the newest file in
        // that subtree to avoid picking up files from other tenants seeded
        // by the test before us.
        let default_subtree = storage.storage_dir().join("default");
        let candidates = find_parquet_files_recursive(&default_subtree).unwrap();
        assert!(
            !candidates.is_empty(),
            "seed expected at least one file under default/"
        );
        let src = candidates.into_iter().max().unwrap();

        let dst = storage.storage_dir().join(src.file_name().unwrap());
        std::fs::rename(&src, &dst).unwrap();
        // Best-effort cleanup of the now-empty intermediate dirs so the
        // migration tool only sees the flat file. (`remove_dir` succeeds
        // only on empty dirs, which is exactly the safety we want here.)
        if let Some(month_dir) = src.parent() {
            let _ = std::fs::remove_dir(month_dir);
            if let Some(tenant_dir) = month_dir.parent() {
                let _ = std::fs::remove_dir(tenant_dir);
            }
        }
        dst
    }

    #[test]
    fn test_migrate_flat_layout_dry_run_touches_nothing() {
        let temp_dir = TempDir::new().unwrap();
        let storage = ParquetStorage::new(temp_dir.path()).unwrap();
        let flat = seed_flat_layout_file(&storage, 7);
        assert!(flat.is_file(), "test setup: flat file should exist");

        let report = storage.migrate_flat_layout(true).unwrap();
        assert!(report.dry_run);
        assert_eq!(report.flat_files_seen, 1);
        assert_eq!(report.events_migrated, 7);
        assert_eq!(report.flat_files_removed, 0);
        assert_eq!(report.partitions_written, 0);
        assert!(
            flat.is_file(),
            "flat file must still be present after dry run"
        );
    }

    #[test]
    fn test_migrate_flat_layout_moves_events_into_default_tree_and_removes_flat() {
        let temp_dir = TempDir::new().unwrap();
        let storage = ParquetStorage::new(temp_dir.path()).unwrap();
        let flat = seed_flat_layout_file(&storage, 5);

        let report = storage.migrate_flat_layout(false).unwrap();
        assert!(!report.dry_run);
        assert_eq!(report.flat_files_seen, 1);
        assert_eq!(report.flat_files_removed, 1);
        assert_eq!(report.events_migrated, 5);
        assert!(report.partitions_written >= 1);
        assert!(
            !flat.exists(),
            "flat file should be deleted after migration"
        );

        let post = find_parquet_files_recursive(temp_dir.path()).unwrap();
        assert!(
            post.iter().all(|p| {
                let rel = p
                    .strip_prefix(temp_dir.path())
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                rel.starts_with(&format!("default{}", std::path::MAIN_SEPARATOR))
            }),
            "all migrated files should be under default/"
        );

        let loaded = storage.load_all_events().unwrap();
        assert_eq!(loaded.len(), 5);
    }

    #[test]
    fn test_migrate_flat_layout_is_idempotent_when_re_run_after_completion() {
        let temp_dir = TempDir::new().unwrap();
        let storage = ParquetStorage::new(temp_dir.path()).unwrap();
        let _flat = seed_flat_layout_file(&storage, 4);

        let first = storage.migrate_flat_layout(false).unwrap();
        assert_eq!(first.events_migrated, 4);

        // Second run sees no flat files at the root, so it's a no-op —
        // events do not duplicate even if an operator runs the tool twice.
        let second = storage.migrate_flat_layout(false).unwrap();
        assert_eq!(second.flat_files_seen, 0);
        assert_eq!(second.events_migrated, 0);
        assert_eq!(second.flat_files_removed, 0);

        let loaded = storage.load_all_events().unwrap();
        assert_eq!(loaded.len(), 4, "rerun must not duplicate or lose events");
    }

    #[test]
    fn test_migrate_flat_layout_ignores_already_partitioned_data() {
        // Mixed state: a tenant tree already exists alongside one flat file.
        // Migration must touch only the flat file.
        let temp_dir = TempDir::new().unwrap();
        let storage = ParquetStorage::new(temp_dir.path()).unwrap();

        for i in 0..3 {
            storage
                .append_event(event_with_tenant("alice", &format!("a-{i}")))
                .unwrap();
        }
        storage.flush().unwrap();

        let _flat = seed_flat_layout_file(&storage, 2);

        let report = storage.migrate_flat_layout(false).unwrap();
        assert_eq!(report.flat_files_seen, 1, "only the flat file is in scope");
        assert_eq!(report.events_migrated, 2);

        let alice_files = find_parquet_files_recursive(&temp_dir.path().join("alice")).unwrap();
        assert_eq!(alice_files.len(), 1, "alice's tree must be untouched");

        let loaded = storage.load_all_events().unwrap();
        assert_eq!(loaded.len(), 5);
        let alice_count = loaded
            .iter()
            .filter(|e| e.tenant_id_str() == "alice")
            .count();
        let default_count = loaded
            .iter()
            .filter(|e| e.tenant_id_str() == "default")
            .count();
        assert_eq!(alice_count, 3);
        assert_eq!(default_count, 2);
    }

    #[test]
    fn test_migrate_flat_layout_with_no_flat_files_is_a_clean_noop() {
        let temp_dir = TempDir::new().unwrap();
        let storage = ParquetStorage::new(temp_dir.path()).unwrap();
        let report = storage.migrate_flat_layout(false).unwrap();
        assert_eq!(report.flat_files_seen, 0);
        assert_eq!(report.events_migrated, 0);
    }
}
