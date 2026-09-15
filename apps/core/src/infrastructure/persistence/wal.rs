use crate::{
    domain::entities::Event,
    error::{AllSourceError, Result},
};
use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

/// Subdirectory of the WAL dir that holds retired segments containing lines
/// recovery could not read. Nothing lists, replays, or deletes it.
pub const QUARANTINE_DIR: &str = "quarantine";

/// Write-Ahead Log for durability and crash recovery.
///
/// Invariant: a segment is deleted only when every line in it is durable
/// elsewhere. A segment holding a line recovery could not parse or verify is
/// moved to [`QUARANTINE_DIR`] instead, so a format this binary cannot read is
/// never destroyed by it (#287).
pub struct WriteAheadLog {
    /// Directory where WAL files are stored
    wal_dir: PathBuf,

    /// Current active WAL file
    current_file: Arc<RwLock<WALFile>>,

    /// Configuration
    config: WALConfig,

    /// Statistics
    stats: Arc<RwLock<WALStats>>,

    /// Current sequence number
    sequence: Arc<RwLock<u64>>,

    /// Optional broadcast channel for replication — when set, each WAL append
    /// publishes the entry so the WAL shipper can stream it to followers.
    /// Wrapped in Mutex so it can be set at runtime (e.g. during follower → leader promotion).
    replication_tx: parking_lot::Mutex<Option<tokio::sync::broadcast::Sender<WALEntry>>>,

    /// Segments in which `recover()` met a line it could not read.
    unreadable_segments: parking_lot::Mutex<HashSet<PathBuf>>,
}

#[derive(Debug, Clone)]
pub struct WALConfig {
    /// Maximum size of a single WAL file before rotation (in bytes)
    pub max_file_size: usize,

    /// Whether to sync to disk after each write (fsync)
    pub sync_on_write: bool,

    /// Maximum number of WAL files to keep
    pub max_wal_files: usize,

    /// Enable WAL compression
    pub compress: bool,

    /// Interval in milliseconds for background coalesced fsync.
    /// When set, a background task calls `flush() + sync_all()` every N ms,
    /// giving near-zero write latency with bounded data loss window.
    /// When `Some`, `sync_on_write` is forced to `false` to prevent double-fsync.
    pub fsync_interval_ms: Option<u64>,
}

impl Default for WALConfig {
    fn default() -> Self {
        Self {
            max_file_size: 64 * 1024 * 1024, // 64 MB
            sync_on_write: true,
            max_wal_files: 10,
            compress: false,
            fsync_interval_ms: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct WALStats {
    pub total_entries: u64,
    pub total_bytes_written: u64,
    pub current_file_size: usize,
    pub files_rotated: u64,
    pub files_cleaned: u64,
    pub recovery_count: u64,
}

/// WAL entry wrapping an event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WALEntry {
    /// Sequence number for ordering
    pub sequence: u64,

    /// Timestamp when written to WAL
    pub wal_timestamp: DateTime<Utc>,

    /// The event being logged
    pub event: Event,

    /// Checksum for integrity verification
    pub checksum: u32,
}

impl WALEntry {
    pub fn new(sequence: u64, event: Event) -> Self {
        let mut entry = Self {
            sequence,
            wal_timestamp: Utc::now(),
            event,
            checksum: 0,
        };
        entry.checksum = entry.calculate_checksum();
        entry
    }

    fn calculate_checksum(&self) -> u32 {
        // Simple CRC32 checksum
        let data = format!("{}{}{}", self.sequence, self.wal_timestamp, self.event.id);
        crc32fast::hash(data.as_bytes())
    }

    pub fn verify(&self) -> bool {
        self.checksum == self.calculate_checksum()
    }
}

fn ends_with_newline(file: &mut File) -> Result<bool> {
    let io_err = |e: std::io::Error| {
        AllSourceError::StorageError(format!("Failed to read WAL file tail: {e}"))
    };
    file.seek(SeekFrom::End(-1)).map_err(io_err)?;
    let mut last = [0u8; 1];
    file.read_exact(&mut last).map_err(io_err)?;
    Ok(last[0] == b'\n')
}

/// Represents an active WAL file
struct WALFile {
    path: PathBuf,
    writer: BufWriter<File>,
    size: usize,
    created_at: DateTime<Utc>,
}

impl WALFile {
    fn new(path: PathBuf) -> Result<Self> {
        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .open(&path)
            .map_err(|e| AllSourceError::StorageError(format!("Failed to open WAL file: {e}")))?;

        let mut size = file.metadata().map_or(0, |m| m.len() as usize);

        // A crash can leave a torn final line with no newline. Appending
        // straight after it would fuse the next entry onto the torn bytes and
        // make that entry unreadable too.
        if size > 0 && !ends_with_newline(&mut file)? {
            file.write_all(b"\n").map_err(|e| {
                AllSourceError::StorageError(format!("Failed to terminate torn WAL line: {e}"))
            })?;
            size += 1;
        }

        Ok(Self {
            path,
            writer: BufWriter::new(file),
            size,
            created_at: Utc::now(),
        })
    }

    fn write_entry(&mut self, entry: &WALEntry, sync: bool) -> Result<usize> {
        // Serialize entry as JSON line
        let json = serde_json::to_string(entry)?;

        let line = format!("{json}\n");
        let bytes_written = line.len();

        self.writer
            .write_all(line.as_bytes())
            .map_err(|e| AllSourceError::StorageError(format!("Failed to write to WAL: {e}")))?;

        if sync {
            self.writer
                .flush()
                .map_err(|e| AllSourceError::StorageError(format!("Failed to flush WAL: {e}")))?;

            self.writer
                .get_ref()
                .sync_all()
                .map_err(|e| AllSourceError::StorageError(format!("Failed to sync WAL: {e}")))?;
        }

        self.size += bytes_written;
        Ok(bytes_written)
    }

    fn flush(&mut self) -> Result<()> {
        self.writer
            .flush()
            .map_err(|e| AllSourceError::StorageError(format!("Failed to flush WAL: {e}")))?;
        Ok(())
    }
}

impl WriteAheadLog {
    /// Create a new WAL
    pub fn new(wal_dir: impl Into<PathBuf>, config: WALConfig) -> Result<Self> {
        let wal_dir = wal_dir.into();

        // Create WAL directory if it doesn't exist
        fs::create_dir_all(&wal_dir).map_err(|e| {
            AllSourceError::StorageError(format!("Failed to create WAL directory: {e}"))
        })?;

        // Create initial WAL file
        let initial_file_path = Self::generate_wal_filename(&wal_dir, 0);
        let current_file = WALFile::new(initial_file_path)?;

        tracing::info!("✅ WAL initialized at: {}", wal_dir.display());

        Ok(Self {
            wal_dir,
            current_file: Arc::new(RwLock::new(current_file)),
            config,
            stats: Arc::new(RwLock::new(WALStats::default())),
            sequence: Arc::new(RwLock::new(0)),
            replication_tx: parking_lot::Mutex::new(None),
            unreadable_segments: parking_lot::Mutex::new(HashSet::new()),
        })
    }

    /// Generate a WAL filename based on sequence
    fn generate_wal_filename(dir: &Path, sequence: u64) -> PathBuf {
        dir.join(format!("wal-{sequence:016x}.log"))
    }

    /// Write an event to the WAL
    #[cfg_attr(feature = "hotpath", hotpath::measure)]
    pub fn append(&self, event: Event) -> Result<u64> {
        // Get next sequence number
        let mut seq = self.sequence.write();
        *seq += 1;
        let sequence = *seq;
        drop(seq);

        // Create WAL entry
        let entry = WALEntry::new(sequence, event);

        // Write to current file
        let mut current = self.current_file.write();
        let bytes_written = current.write_entry(&entry, self.config.sync_on_write)?;

        // Update statistics
        let mut stats = self.stats.write();
        stats.total_entries += 1;
        stats.total_bytes_written += bytes_written as u64;
        stats.current_file_size = current.size;
        drop(stats);

        // Broadcast to replication channel (if enabled).
        // Errors are ignored: no receivers simply means no followers are connected.
        if let Some(ref tx) = *self.replication_tx.lock() {
            let _ = tx.send(entry);
        }

        // Check if we need to rotate
        let should_rotate = current.size >= self.config.max_file_size;
        drop(current);

        if should_rotate {
            self.rotate()?;
        }

        tracing::trace!("WAL entry written: sequence={}", sequence);

        Ok(sequence)
    }

    /// Rotate to a new WAL file
    #[cfg_attr(feature = "hotpath", hotpath::measure)]
    fn rotate(&self) -> Result<()> {
        let seq = *self.sequence.read();
        let new_file_path = Self::generate_wal_filename(&self.wal_dir, seq);

        tracing::info!("🔄 Rotating WAL to new file: {:?}", new_file_path);

        let new_file = WALFile::new(new_file_path)?;

        let mut current = self.current_file.write();
        current.flush()?;
        *current = new_file;

        let mut stats = self.stats.write();
        stats.files_rotated += 1;
        stats.current_file_size = 0;
        drop(stats);

        self.warn_if_segments_accumulate()?;

        Ok(())
    }

    /// Rotation never deletes: a segment past `max_wal_files` may hold events
    /// no checkpoint has flushed yet. Only a checkpoint retires segments.
    fn warn_if_segments_accumulate(&self) -> Result<()> {
        let count = self.list_wal_files()?.len();
        if count > self.config.max_wal_files {
            tracing::warn!(
                "WAL holds {} segments (max_wal_files = {}); checkpoints are not keeping up, \
                 so the WAL keeps growing rather than dropping unflushed events",
                count,
                self.config.max_wal_files
            );
        }
        Ok(())
    }

    /// Delete a segment, or move it to quarantine if recovery could not read
    /// every line in it.
    fn retire_segment(&self, path: &Path) -> Result<()> {
        if !self.unreadable_segments.lock().remove(path) {
            fs::remove_file(path).map_err(|e| {
                AllSourceError::StorageError(format!("Failed to remove WAL file: {e}"))
            })?;
            tracing::debug!("Removed WAL file: {:?}", path);
            return Ok(());
        }

        let quarantine = self.wal_dir.join(QUARANTINE_DIR);
        fs::create_dir_all(&quarantine).map_err(|e| {
            AllSourceError::StorageError(format!("Failed to create WAL quarantine: {e}"))
        })?;
        let name = path.file_name().map_or_else(
            || "wal-unknown.log".into(),
            |n| n.to_string_lossy().into_owned(),
        );
        let target = quarantine.join(format!(
            "{}-{name}",
            Utc::now().format("%Y%m%dT%H%M%S%.6fZ")
        ));
        fs::rename(path, &target).map_err(|e| {
            AllSourceError::StorageError(format!("Failed to quarantine WAL file: {e}"))
        })?;
        tracing::warn!(
            "WAL segment {:?} held lines this binary could not read; kept at {:?} instead of \
             deleting it",
            path,
            target
        );
        Ok(())
    }

    /// Start a new segment so every entry appended so far sits in a segment
    /// older than the returned one. Pair with [`Self::remove_sealed`].
    ///
    /// The caller must stop appends to durable storage for the duration of
    /// this call; otherwise an entry can land in the sealed segment after the
    /// checkpoint's flush has already run.
    pub fn seal(&self) -> Result<PathBuf> {
        let seq = *self.sequence.read();
        let new_path = Self::generate_wal_filename(&self.wal_dir, seq);

        let mut current = self.current_file.write();
        current.flush()?;
        if current.path != new_path {
            *current = WALFile::new(new_path.clone())?;
            self.stats.write().current_file_size = current.size;
        }
        Ok(new_path)
    }

    /// Retire every segment older than `active`, the path [`Self::seal`]
    /// returned. Segments at or after it are kept.
    pub fn remove_sealed(&self, active: &Path) -> Result<()> {
        let Some(active_name) = active.file_name() else {
            return Ok(());
        };
        for path in self.list_wal_files()? {
            if path.file_name().is_some_and(|name| name < active_name) {
                self.retire_segment(&path)?;
            }
        }
        Ok(())
    }

    /// List all WAL files in the directory
    fn list_wal_files(&self) -> Result<Vec<PathBuf>> {
        let entries = fs::read_dir(&self.wal_dir).map_err(|e| {
            AllSourceError::StorageError(format!("Failed to read WAL directory: {e}"))
        })?;

        let mut wal_files = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| {
                AllSourceError::StorageError(format!("Failed to read directory entry: {e}"))
            })?;

            let path = entry.path();
            if let Some(name) = path.file_name()
                && name.to_string_lossy().starts_with("wal-")
                && name.to_string_lossy().ends_with(".log")
            {
                wal_files.push(path);
            }
        }

        Ok(wal_files)
    }

    /// Recover events from WAL files
    #[cfg_attr(feature = "hotpath", hotpath::measure)]
    pub fn recover(&self) -> Result<Vec<Event>> {
        tracing::info!("🔄 Starting WAL recovery...");

        let mut wal_files = self.list_wal_files()?;
        wal_files.sort();

        let mut recovered_events = Vec::new();
        let mut max_sequence = 0u64;
        let mut corrupted_entries = 0;

        for wal_file_path in &wal_files {
            tracing::debug!("Reading WAL file: {:?}", wal_file_path);

            let file = File::open(wal_file_path).map_err(|e| {
                AllSourceError::StorageError(format!("Failed to open WAL file for recovery: {e}"))
            })?;

            let reader = BufReader::new(file);
            let corrupted_before = corrupted_entries;

            for (line_num, line) in reader.lines().enumerate() {
                let line = match line {
                    Ok(l) => l,
                    Err(e) => {
                        tracing::warn!(
                            "I/O error reading WAL line at {:?}:{}: {}",
                            wal_file_path,
                            line_num + 1,
                            e
                        );
                        corrupted_entries += 1;
                        continue;
                    }
                };

                if line.trim().is_empty() {
                    continue;
                }

                match serde_json::from_str::<WALEntry>(&line) {
                    Ok(entry) => {
                        // Verify checksum
                        if !entry.verify() {
                            tracing::warn!(
                                "Corrupted WAL entry at {:?}:{} (checksum mismatch)",
                                wal_file_path,
                                line_num + 1
                            );
                            corrupted_entries += 1;
                            continue;
                        }

                        max_sequence = max_sequence.max(entry.sequence);
                        recovered_events.push(entry.event);
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to parse WAL entry at {:?}:{}: {}",
                            wal_file_path,
                            line_num + 1,
                            e
                        );
                        corrupted_entries += 1;
                    }
                }
            }

            if corrupted_entries > corrupted_before {
                self.unreadable_segments
                    .lock()
                    .insert(wal_file_path.clone());
            }
        }

        // Update sequence counter
        let mut seq = self.sequence.write();
        *seq = max_sequence;
        drop(seq);

        // Update stats
        let mut stats = self.stats.write();
        stats.recovery_count += 1;
        drop(stats);

        tracing::info!(
            "✅ WAL recovery complete: {} events recovered, {} corrupted entries",
            recovered_events.len(),
            corrupted_entries
        );

        Ok(recovered_events)
    }

    /// Manually flush the current WAL file
    #[cfg_attr(feature = "hotpath", hotpath::measure)]
    pub fn flush(&self) -> Result<()> {
        let mut current = self.current_file.write();
        current.flush()?;
        Ok(())
    }

    /// Flush the BufWriter and fsync the current WAL file to disk.
    ///
    /// Called by the background interval-based fsync task. Acquires the write
    /// lock, flushes buffered data, then issues `sync_all()` to ensure the
    /// OS has persisted the data to durable storage.
    #[cfg_attr(feature = "hotpath", hotpath::measure)]
    pub fn sync(&self) -> Result<()> {
        let mut current = self.current_file.write();
        current
            .writer
            .flush()
            .map_err(|e| AllSourceError::StorageError(format!("Failed to flush WAL: {e}")))?;
        current
            .writer
            .get_ref()
            .sync_all()
            .map_err(|e| AllSourceError::StorageError(format!("Failed to sync WAL: {e}")))?;
        Ok(())
    }

    /// Get the configured fsync interval (if any).
    pub fn fsync_interval_ms(&self) -> Option<u64> {
        self.config.fsync_interval_ms
    }

    /// Truncate WAL after successful checkpoint
    #[cfg_attr(feature = "hotpath", hotpath::measure)]
    pub fn truncate(&self) -> Result<()> {
        tracing::info!("🧹 Truncating WAL after checkpoint");

        // Close current file
        let mut current = self.current_file.write();
        current.flush()?;

        let wal_files = self.list_wal_files()?;
        for file_path in wal_files {
            self.retire_segment(&file_path)?;
        }

        // Create new WAL file
        let new_file_path = Self::generate_wal_filename(&self.wal_dir, 0);
        *current = WALFile::new(new_file_path)?;

        // Reset sequence
        let mut seq = self.sequence.write();
        *seq = 0;

        tracing::info!("✅ WAL truncated successfully");

        Ok(())
    }

    /// Get WAL statistics
    pub fn stats(&self) -> WALStats {
        (*self.stats.read()).clone()
    }

    /// Sum the on-disk size of all WAL segment files and count them.
    ///
    /// Walks the WAL directory and `fs::metadata`s each `wal-*.log` segment.
    /// Returns `(total_bytes, segment_count)`. Unlike `WALStats.total_bytes_written`
    /// (a monotonic write counter that ignores rotation/cleanup), this reflects the
    /// *current* on-disk footprint — what we want to report as storage size and as
    /// `allsource_wal_segments_total`.
    ///
    /// Cheap (one `statx` per segment; each checkpoint retires sealed segments,
    /// so the count stays small), so it's safe to call on the checkpoint cadence. Errors reading the directory
    /// surface as `Err`; a per-file metadata error skips that file rather than
    /// aborting the whole tally.
    pub fn on_disk_stats(&self) -> Result<(u64, usize)> {
        let wal_files = self.list_wal_files()?;
        let mut total_bytes = 0u64;
        for path in &wal_files {
            if let Ok(metadata) = fs::metadata(path) {
                total_bytes += metadata.len();
            }
        }
        Ok((total_bytes, wal_files.len()))
    }

    /// Get current sequence number
    pub fn current_sequence(&self) -> u64 {
        *self.sequence.read()
    }

    /// Get the oldest available WAL sequence number.
    ///
    /// Returns the first sequence found in the oldest WAL file, or `None`
    /// if no WAL entries exist. Used by the replication catch-up protocol
    /// to determine whether a follower can catch up from WAL alone.
    pub fn oldest_sequence(&self) -> Option<u64> {
        let Ok(mut wal_files) = self.list_wal_files() else {
            return None;
        };

        if wal_files.is_empty() {
            return None;
        }

        wal_files.sort();

        // Read the first entry from the oldest WAL file
        for wal_file_path in &wal_files {
            let Ok(file) = File::open(wal_file_path) else {
                continue;
            };
            let reader = BufReader::new(file);
            for line in reader.lines() {
                let Ok(line) = line else {
                    continue;
                };
                if line.trim().is_empty() {
                    continue;
                }
                if let Ok(entry) = serde_json::from_str::<WALEntry>(&line) {
                    return Some(entry.sequence);
                }
            }
        }

        None
    }

    /// Attach a broadcast sender for WAL replication.
    ///
    /// When set, every `append()` call will publish the WAL entry to this
    /// channel so the WAL shipper can stream it to connected followers.
    pub fn set_replication_tx(&self, tx: tokio::sync::broadcast::Sender<WALEntry>) {
        *self.replication_tx.lock() = Some(tx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;
    use uuid::Uuid;

    fn create_test_event() -> Event {
        Event::reconstruct_from_strings(
            Uuid::new_v4(),
            "test.event".to_string(),
            "test-entity".to_string(),
            "default".to_string(),
            json!({"test": "data"}),
            Utc::now(),
            None,
            1,
        )
    }

    #[test]
    fn test_wal_creation() {
        let temp_dir = TempDir::new().unwrap();
        let wal = WriteAheadLog::new(temp_dir.path(), WALConfig::default());
        assert!(wal.is_ok());
    }

    #[test]
    fn test_wal_append() {
        let temp_dir = TempDir::new().unwrap();
        let wal = WriteAheadLog::new(temp_dir.path(), WALConfig::default()).unwrap();

        let event = create_test_event();
        let seq = wal.append(event);
        assert!(seq.is_ok());
        assert_eq!(seq.unwrap(), 1);

        let stats = wal.stats();
        assert_eq!(stats.total_entries, 1);
    }

    #[test]
    fn test_wal_on_disk_stats() {
        let temp_dir = TempDir::new().unwrap();
        let wal = WriteAheadLog::new(temp_dir.path(), WALConfig::default()).unwrap();

        // Empty WAL dir (the active segment is created lazily on first append):
        // zero or one segment, zero bytes.
        let (bytes0, segs0) = wal.on_disk_stats().unwrap();
        assert_eq!(bytes0, 0);
        assert!(segs0 <= 1);

        for _ in 0..5 {
            wal.append(create_test_event()).unwrap();
        }
        wal.flush().unwrap();

        let (bytes, segs) = wal.on_disk_stats().unwrap();
        assert!(segs >= 1, "expected at least one WAL segment, got {segs}");
        assert!(
            bytes > 0,
            "expected non-zero WAL bytes after appends, got {bytes}"
        );
    }

    #[test]
    fn test_wal_recovery() {
        let temp_dir = TempDir::new().unwrap();
        let wal = WriteAheadLog::new(temp_dir.path(), WALConfig::default()).unwrap();

        // Write some events
        for _ in 0..5 {
            wal.append(create_test_event()).unwrap();
        }

        wal.flush().unwrap();

        // Create new WAL instance (simulating restart)
        let wal2 = WriteAheadLog::new(temp_dir.path(), WALConfig::default()).unwrap();
        let recovered = wal2.recover().unwrap();

        assert_eq!(recovered.len(), 5);
    }

    #[test]
    fn test_wal_recovery_with_partial_write() {
        let temp_dir = TempDir::new().unwrap();
        let wal = WriteAheadLog::new(temp_dir.path(), WALConfig::default()).unwrap();

        // Write 3 complete events
        for _ in 0..3 {
            wal.append(create_test_event()).unwrap();
        }
        wal.flush().unwrap();

        // Simulate a partial write by appending malformed data to the WAL file
        let wal_file_path = temp_dir.path().join("wal-0000000000000000.log");
        use std::io::Write as _;
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&wal_file_path)
            .unwrap();
        f.write_all(b"{\"partial\": true, \"seq\"\n").unwrap(); // truncated JSON
        drop(f);

        // Recovery should succeed and return only the 3 valid events
        let wal2 = WriteAheadLog::new(temp_dir.path(), WALConfig::default()).unwrap();
        let recovered = wal2.recover().unwrap();
        assert_eq!(
            recovered.len(),
            3,
            "Should recover only the 3 valid events, not the partial one"
        );
    }

    #[test]
    fn test_wal_rotation() {
        let temp_dir = TempDir::new().unwrap();
        let config = WALConfig {
            max_file_size: 1024, // Small size to trigger rotation
            ..Default::default()
        };

        let wal = WriteAheadLog::new(temp_dir.path(), config).unwrap();

        // Write enough events to trigger rotation
        for _ in 0..50 {
            wal.append(create_test_event()).unwrap();
        }

        let stats = wal.stats();
        assert!(stats.files_rotated > 0);
    }

    #[test]
    fn test_wal_entry_checksum() {
        let event = create_test_event();
        let entry = WALEntry::new(1, event);

        assert!(entry.verify());

        // Modify and verify it fails
        let mut corrupted = entry.clone();
        corrupted.checksum = 0;
        assert!(!corrupted.verify());
    }

    #[test]
    fn test_wal_fsync_interval_config() {
        let config = WALConfig {
            fsync_interval_ms: Some(100),
            ..Default::default()
        };
        assert_eq!(config.fsync_interval_ms, Some(100));
        // Default should have no interval
        assert_eq!(WALConfig::default().fsync_interval_ms, None);
    }

    #[test]
    fn test_wal_sync_method() {
        let temp_dir = TempDir::new().unwrap();
        let config = WALConfig {
            sync_on_write: false, // Disable per-write sync to test explicit sync
            ..Default::default()
        };
        let wal = WriteAheadLog::new(temp_dir.path(), config).unwrap();

        // Write events without per-write fsync
        for _ in 0..5 {
            wal.append(create_test_event()).unwrap();
        }

        // Explicitly sync — should flush + fsync without error
        wal.sync().unwrap();

        // Verify data survives by recovering from a new WAL instance
        let wal2 = WriteAheadLog::new(temp_dir.path(), WALConfig::default()).unwrap();
        let recovered = wal2.recover().unwrap();
        assert_eq!(recovered.len(), 5);
    }

    #[test]
    fn test_wal_truncate() {
        let temp_dir = TempDir::new().unwrap();
        let wal = WriteAheadLog::new(temp_dir.path(), WALConfig::default()).unwrap();

        // Write events
        for _ in 0..5 {
            wal.append(create_test_event()).unwrap();
        }

        // Truncate
        wal.truncate().unwrap();

        // Verify sequence is reset
        assert_eq!(wal.current_sequence(), 0);

        // Verify recovery returns empty
        let recovered = wal.recover().unwrap();
        assert_eq!(recovered.len(), 0);
    }
}
