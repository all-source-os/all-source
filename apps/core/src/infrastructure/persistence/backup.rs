use crate::domain::entities::Event;
/// Backup and restore system for AllSource event store
///
/// Features:
/// - Full backups of all event data
/// - Incremental backups from checkpoint
/// - Compressed backup files (gzip)
/// - Metadata tracking
/// - Verification and integrity checks
/// - Support for filesystem and S3-compatible storage
use crate::error::{AllSourceError, Result};
use chrono::{DateTime, Utc};
use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use uuid::Uuid;

/// Backup metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupMetadata {
    pub backup_id: String,
    pub created_at: DateTime<Utc>,
    pub backup_type: BackupType,
    pub event_count: u64,
    pub size_bytes: u64,
    pub checksum: String,
    pub from_sequence: Option<u64>,
    pub to_sequence: u64,
    pub compressed: bool,
}

/// Type of backup
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BackupType {
    Full,
    Incremental { from_backup_id: String },
}

/// Backup configuration
#[derive(Debug, Clone)]
pub struct BackupConfig {
    pub backup_dir: PathBuf,
    pub compression_level: Compression,
    pub verify_after_backup: bool,
}

impl Default for BackupConfig {
    fn default() -> Self {
        Self {
            backup_dir: PathBuf::from("./backups"),
            compression_level: Compression::default(),
            verify_after_backup: true,
        }
    }
}

/// Backup manager
pub struct BackupManager {
    config: BackupConfig,
}

/// Validate a backup identifier before it is joined into a filesystem path.
///
/// Backup IDs come from callers (CLI, embedded API, future HTTP handlers) and
/// are concatenated into filenames under `backup_dir`. Without validation a
/// caller could supply `../../etc/passwd` or similar and escape the backup
/// directory. We accept only ASCII alphanumerics, `_`, `-`, and enforce a
/// maximum length so the resulting path stays within the configured dir.
fn validate_backup_id(id: &str) -> Result<()> {
    if id.is_empty() {
        return Err(AllSourceError::ValidationError(
            "backup id must not be empty".to_string(),
        ));
    }
    if id.len() > 128 {
        return Err(AllSourceError::ValidationError(
            "backup id too long (max 128 chars)".to_string(),
        ));
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(AllSourceError::ValidationError(format!(
            "backup id '{id}' contains invalid characters; only [A-Za-z0-9_-] allowed"
        )));
    }
    Ok(())
}

impl BackupManager {
    pub fn new(config: BackupConfig) -> Result<Self> {
        // Ensure backup directory exists
        fs::create_dir_all(&config.backup_dir).map_err(|e| {
            AllSourceError::StorageError(format!("Failed to create backup dir: {e}"))
        })?;

        Ok(Self { config })
    }

    /// Create a full backup from events
    pub fn create_backup(&self, events: &[Event]) -> Result<BackupMetadata> {
        let backup_id = format!("full_{}", Uuid::new_v4());
        let timestamp = Utc::now();

        tracing::info!("Creating backup: {}", backup_id);

        let event_count = events.len() as u64;

        if event_count == 0 {
            return Err(AllSourceError::ValidationError(
                "No events to backup".to_string(),
            ));
        }

        // Serialize events to JSON
        let json_data = serde_json::to_string(&events)?;

        // Compress backup
        let backup_path = self.get_backup_path(&backup_id);
        let mut encoder = GzEncoder::new(
            File::create(&backup_path).map_err(|e| {
                AllSourceError::StorageError(format!("Failed to create backup file: {e}"))
            })?,
            self.config.compression_level,
        );

        encoder
            .write_all(json_data.as_bytes())
            .map_err(|e| AllSourceError::StorageError(format!("Failed to write backup: {e}")))?;

        encoder.finish().map_err(|e| {
            AllSourceError::StorageError(format!("Failed to finish compression: {e}"))
        })?;

        let size_bytes = fs::metadata(&backup_path)
            .map_err(|e| AllSourceError::StorageError(e.to_string()))?
            .len();

        let checksum = self.calculate_checksum(&backup_path)?;

        let metadata = BackupMetadata {
            backup_id: backup_id.clone(),
            created_at: timestamp,
            backup_type: BackupType::Full,
            event_count,
            size_bytes,
            checksum,
            from_sequence: None,
            to_sequence: event_count,
            compressed: true,
        };

        // Save metadata
        self.save_metadata(&metadata)?;

        // Verify if configured
        if self.config.verify_after_backup {
            self.verify_backup(&metadata)?;
        }

        tracing::info!(
            "Backup complete: {} events, {} bytes compressed",
            event_count,
            size_bytes
        );

        Ok(metadata)
    }

    /// Restore from backup
    pub fn restore_from_backup(&self, backup_id: &str) -> Result<Vec<Event>> {
        validate_backup_id(backup_id)?;
        super::erasure_maintenance::require_no_marker(&self.config.backup_dir)?;
        tracing::info!("Restoring from backup: {}", backup_id);

        let metadata = self.load_metadata(backup_id)?;

        // Verify backup integrity
        self.verify_backup(&metadata)?;

        let backup_path = self.get_backup_path(backup_id);

        // Decompress backup
        let file = File::open(&backup_path)
            .map_err(|e| AllSourceError::StorageError(format!("Failed to open backup: {e}")))?;

        let mut decoder = GzDecoder::new(file);
        let mut json_data = String::new();
        decoder.read_to_string(&mut json_data).map_err(|e| {
            AllSourceError::StorageError(format!("Failed to decompress backup: {e}"))
        })?;

        // Deserialize events
        let events: Vec<Event> = serde_json::from_str(&json_data)?;

        if events.len() != metadata.event_count as usize {
            return Err(AllSourceError::ValidationError(format!(
                "Event count mismatch: expected {}, got {}",
                metadata.event_count,
                events.len()
            )));
        }

        tracing::info!("Restored {} events from backup", events.len());

        Ok(events)
    }

    /// Verify backup integrity
    pub fn verify_backup(&self, metadata: &BackupMetadata) -> Result<()> {
        let backup_path = self.get_backup_path(&metadata.backup_id);

        if !backup_path.exists() {
            return Err(AllSourceError::ValidationError(
                "Backup file not found".to_string(),
            ));
        }

        let checksum = self.calculate_checksum(&backup_path)?;

        if checksum != metadata.checksum {
            return Err(AllSourceError::ValidationError(format!(
                "Checksum mismatch: expected {}, got {}",
                metadata.checksum, checksum
            )));
        }

        Ok(())
    }

    /// List all backups
    pub fn list_backups(&self) -> Result<Vec<BackupMetadata>> {
        let mut backups = Vec::new();

        let entries = fs::read_dir(&self.config.backup_dir)
            .map_err(|e| AllSourceError::StorageError(e.to_string()))?;

        for entry in entries {
            let entry = entry.map_err(|e| AllSourceError::StorageError(e.to_string()))?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("json")
                && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
                && let Some(backup_id) = stem.strip_suffix("_metadata")
                && let Ok(metadata) = self.load_metadata(backup_id)
            {
                backups.push(metadata);
            }
        }

        // Sort by creation time, newest first
        backups.sort_by_key(|x| std::cmp::Reverse(x.created_at));

        Ok(backups)
    }

    /// Delete backup
    pub fn delete_backup(&self, backup_id: &str) -> Result<()> {
        validate_backup_id(backup_id)?;
        tracing::info!("Deleting backup: {}", backup_id);

        let backup_path = self.get_backup_path(backup_id);
        let metadata_path = self.get_metadata_path(backup_id);

        if backup_path.exists() {
            fs::remove_file(&backup_path)
                .map_err(|e| AllSourceError::StorageError(e.to_string()))?;
        }

        if metadata_path.exists() {
            fs::remove_file(&metadata_path)
                .map_err(|e| AllSourceError::StorageError(e.to_string()))?;
        }

        Ok(())
    }

    /// Cleanup old backups (keep last N)
    pub fn cleanup_old_backups(&self, keep_count: usize) -> Result<usize> {
        let mut backups = self.list_backups()?;

        if backups.len() <= keep_count {
            return Ok(0);
        }

        // Sort by date, oldest last
        backups.sort_by_key(|x| std::cmp::Reverse(x.created_at));

        let to_delete = backups.split_off(keep_count);
        let delete_count = to_delete.len();

        for backup in to_delete {
            self.delete_backup(&backup.backup_id)?;
        }

        tracing::info!("Cleaned up {} old backups", delete_count);

        Ok(delete_count)
    }

    // Private helper methods

    fn get_backup_path(&self, backup_id: &str) -> PathBuf {
        self.config
            .backup_dir
            .join(format!("{backup_id}.backup.gz"))
    }

    fn get_metadata_path(&self, backup_id: &str) -> PathBuf {
        self.config
            .backup_dir
            .join(format!("{backup_id}_metadata.json"))
    }

    fn save_metadata(&self, metadata: &BackupMetadata) -> Result<()> {
        let path = self.get_metadata_path(&metadata.backup_id);
        let json = serde_json::to_string_pretty(metadata)?;

        fs::write(&path, json).map_err(|e| AllSourceError::StorageError(e.to_string()))?;

        Ok(())
    }

    fn load_metadata(&self, backup_id: &str) -> Result<BackupMetadata> {
        validate_backup_id(backup_id)?;
        let path = self.get_metadata_path(backup_id);

        if !path.exists() {
            return Err(AllSourceError::ValidationError(
                "Backup metadata not found".to_string(),
            ));
        }

        let json =
            fs::read_to_string(&path).map_err(|e| AllSourceError::StorageError(e.to_string()))?;

        Ok(serde_json::from_str(&json)?)
    }

    fn calculate_checksum(&self, path: &Path) -> Result<String> {
        use sha2::{Digest, Sha256};

        let mut file = File::open(path).map_err(|e| AllSourceError::StorageError(e.to_string()))?;

        let mut hasher = Sha256::new();
        let mut buffer = [0; 8192];

        loop {
            let count = file
                .read(&mut buffer)
                .map_err(|e| AllSourceError::StorageError(e.to_string()))?;

            if count == 0 {
                break;
            }

            hasher.update(&buffer[..count]);
        }

        Ok(format!("{:x}", hasher.finalize()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backup_config_default() {
        let config = BackupConfig::default();
        assert!(config.verify_after_backup);
    }

    #[test]
    fn test_backup_type_serialization() {
        let full = BackupType::Full;
        let json = serde_json::to_string(&full).unwrap();
        let deserialized: BackupType = serde_json::from_str(&json).unwrap();
        assert_eq!(full, deserialized);
    }

    #[test]
    fn test_validate_backup_id_accepts_valid() {
        assert!(validate_backup_id("full_a1b2c3").is_ok());
        assert!(validate_backup_id("incremental-2026-04-13").is_ok());
        assert!(validate_backup_id("A").is_ok());
        assert!(
            validate_backup_id("full_550e8400-e29b-41d4-a716-446655440000").is_ok(),
            "uuid-style ids with dashes should be accepted"
        );
    }

    #[test]
    fn test_validate_backup_id_rejects_traversal() {
        assert!(validate_backup_id("").is_err());
        assert!(validate_backup_id("../etc/passwd").is_err());
        assert!(validate_backup_id("..").is_err());
        assert!(validate_backup_id("foo/bar").is_err());
        assert!(validate_backup_id("foo\\bar").is_err());
        assert!(validate_backup_id("foo.bar").is_err(), "dots are rejected");
        assert!(validate_backup_id("foo\0bar").is_err());
        assert!(validate_backup_id("foo bar").is_err());
    }

    #[test]
    fn test_validate_backup_id_length_limit() {
        let too_long = "a".repeat(129);
        assert!(validate_backup_id(&too_long).is_err());
        let max_ok = "a".repeat(128);
        assert!(validate_backup_id(&max_ok).is_ok());
    }
}
