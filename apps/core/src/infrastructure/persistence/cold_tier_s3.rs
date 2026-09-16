//! Cold-tier archive to an S3-compatible object store.
//!
//! The [`ArchiveTarget`](super::cold_tier::ArchiveTarget) impl for AWS S3,
//! Cloudflare R2, MinIO, or anything else speaking the S3 protocol. Which one
//! is an operator config call, not a code one: the endpoint is part of the URL.
//!
//! ## Why it encodes through a temp file
//!
//! The archive re-uses `ParquetStorage::write_atomic_parquet` to produce the
//! bytes, then uploads them. Encoding in memory instead would mean a second
//! Parquet writer in the tree, and a cold archive whose schema had drifted from
//! live storage is worth nothing on the day someone needs to read it. One
//! encoder, one format.
//!
//! ## Why it blocks
//!
//! `ArchiveTarget::archive` is sync because the whole compaction pipeline is,
//! and the object-store client is async. The bridge is the same one
//! `Prime::embed_text` uses: `block_in_place` + `block_on` when already on a
//! runtime, a temporary current-thread runtime otherwise. `block_in_place`
//! requires the multi-threaded flavor, which is why the `cold-tier-s3` feature
//! turns on `tokio/rt-multi-thread` rather than assuming it.
//!
//! ## Crash safety and idempotency
//!
//! The object key is a pure function of `(tenant_id, from, to)`, so a retry
//! after a transient failure overwrites the same key with the same bytes rather
//! than accumulating duplicates. A failed upload returns `Err`, which
//! short-circuits `compact_tenant` before any original is deleted.

use super::{cold_tier::ArchiveTarget, storage::ParquetStorage};
use crate::{
    domain::entities::Event,
    error::{AllSourceError, Result},
};
use chrono::{DateTime, Utc};
use object_store::{ObjectStore, ObjectStoreExt, aws::AmazonS3Builder, path::Path as ObjectPath};
use std::{fmt, path::PathBuf, sync::Arc};

/// Parsed form of `ALLSOURCE_COLD_STORAGE_URL`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct S3Location {
    pub bucket: String,
    /// Key prefix within the bucket. Empty when the URL names only a bucket.
    pub prefix: String,
}

/// Parse `s3://bucket/optional/prefix` into its parts.
///
/// Accepts `s3://` and `r2://` — R2 is S3-compatible and operators write the
/// scheme they think in. Anything else is rejected rather than guessed at,
/// because a mistyped scheme that silently became a no-op archive would delete
/// originals with nothing behind them.
pub fn parse_cold_storage_url(url: &str) -> Result<S3Location> {
    let rest = url
        .strip_prefix("s3://")
        .or_else(|| url.strip_prefix("r2://"))
        .ok_or_else(|| {
            AllSourceError::InvalidInput(format!(
                "cold-tier archive: {url:?} must start with s3:// or r2://"
            ))
        })?;

    let mut parts = rest.splitn(2, '/');
    let bucket = parts.next().unwrap_or_default().trim().to_string();
    if bucket.is_empty() {
        return Err(AllSourceError::InvalidInput(format!(
            "cold-tier archive: {url:?} names no bucket"
        )));
    }
    let prefix = parts.next().unwrap_or("").trim_matches('/').to_string();

    Ok(S3Location { bucket, prefix })
}

/// Object key for one archived window. Pure function of its inputs — this is
/// what makes a retry idempotent rather than duplicating.
pub fn archive_object_key(
    prefix: &str,
    tenant_id: &str,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> String {
    let month = from.format("%Y-%m");
    let stem = format!(
        "archive.{tenant_id}.{}-{}.parquet",
        super::compaction::format_iso_basic(from),
        super::compaction::format_iso_basic(to)
    );
    if prefix.is_empty() {
        format!("{tenant_id}/{month}/{stem}")
    } else {
        format!("{prefix}/{tenant_id}/{month}/{stem}")
    }
}

/// S3-compatible cold-tier archive.
pub struct S3Archive {
    store: Arc<dyn ObjectStore>,
    location: S3Location,
    /// Scratch directory for the Parquet encode step. Each encode is deleted
    /// as soon as it is uploaded, so this holds at most one file per in-flight
    /// archive call.
    scratch: Arc<ParquetStorage>,
    scratch_dir: PathBuf,
}

impl S3Archive {
    /// Build from `ALLSOURCE_COLD_STORAGE_URL`.
    ///
    /// Credentials, region and endpoint come from the standard AWS environment
    /// (`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `AWS_REGION`,
    /// `AWS_ENDPOINT_URL`), so pointing at R2 or MinIO is an env change.
    pub fn from_url(url: &str) -> Result<Self> {
        let location = parse_cold_storage_url(url)?;
        let store = AmazonS3Builder::from_env()
            .with_bucket_name(&location.bucket)
            .build()
            .map_err(|e| {
                AllSourceError::StorageError(format!(
                    "cold-tier archive: could not build S3 client for bucket {}: {e}",
                    location.bucket
                ))
            })?;
        Self::with_store(Arc::new(store), location)
    }

    /// Build around an already-configured store. Used by the integration tests
    /// so they can point at a MinIO container without mutating process env.
    pub fn with_store(store: Arc<dyn ObjectStore>, location: S3Location) -> Result<Self> {
        // Deliberately not the `tempfile` crate: it is a dev-dependency here,
        // and pulling it into the shipped build for one directory is a worse
        // trade than naming the directory ourselves.
        let scratch_dir = std::env::temp_dir()
            .join("allsource-cold-tier")
            .join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&scratch_dir).map_err(|e| {
            AllSourceError::StorageError(format!(
                "cold-tier archive: could not create scratch dir {}: {e}",
                scratch_dir.display()
            ))
        })?;
        let scratch = ParquetStorage::new(&scratch_dir).map_err(|e| {
            AllSourceError::StorageError(format!(
                "cold-tier archive: could not open scratch storage: {e}"
            ))
        })?;
        Ok(Self {
            store,
            location,
            scratch: Arc::new(scratch),
            scratch_dir,
        })
    }

    /// Run a future to completion from this sync trait method. Mirrors the
    /// bridge in `prime::vectors::embedder`.
    fn block_on<F: std::future::Future>(fut: F) -> F::Output {
        use tokio::runtime::Handle;
        match Handle::try_current() {
            Ok(handle) => tokio::task::block_in_place(move || handle.block_on(fut)),
            Err(_) => tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("build temporary tokio runtime for cold-tier upload")
                .block_on(fut),
        }
    }
}

impl fmt::Debug for S3Archive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("S3Archive")
            .field("bucket", &self.location.bucket)
            .field("prefix", &self.location.prefix)
            .finish()
    }
}

impl ArchiveTarget for S3Archive {
    fn archive(
        &self,
        tenant_id: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        events: &[Event],
    ) -> Result<()> {
        if events.is_empty() {
            return Ok(());
        }

        let stem = format!(
            "coldtier.{tenant_id}.{}",
            super::compaction::format_iso_basic(from)
        );
        let local = self
            .scratch
            .write_atomic_parquet(tenant_id, &stem, events)?;

        let bytes = std::fs::read(&local).map_err(|e| {
            AllSourceError::StorageError(format!(
                "cold-tier archive: could not read encoded parquet {}: {e}",
                local.display()
            ))
        })?;
        let encoded_len = bytes.len();

        let key = archive_object_key(&self.location.prefix, tenant_id, from, to);
        let path = ObjectPath::from(key.clone());
        let put = Self::block_on(self.store.put(&path, bytes.into()));

        // Remove the scratch copy whether or not the upload worked; the archive
        // is the object store, and leaving encodes behind would slowly fill the
        // volume this tier exists to relieve.
        let _ = std::fs::remove_file(&local);

        put.map_err(|e| {
            AllSourceError::StorageError(format!(
                "cold-tier archive: upload to s3://{}/{key} failed: {e}",
                self.location.bucket
            ))
        })?;

        tracing::info!(
            tenant_id = tenant_id,
            bucket = %self.location.bucket,
            key = %key,
            events = events.len(),
            bytes = encoded_len,
            from = %from.to_rfc3339(),
            to = %to.to_rfc3339(),
            "cold-tier archive: uploaded dropped events"
        );
        Ok(())
    }

    fn description(&self) -> String {
        if self.location.prefix.is_empty() {
            format!("s3:{}", self.location.bucket)
        } else {
            format!("s3:{}/{}", self.location.bucket, self.location.prefix)
        }
    }
}

impl S3Archive {
    /// Scratch directory, for tests asserting it does not accumulate files.
    pub fn scratch_path(&self) -> &std::path::Path {
        &self.scratch_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn ts(day: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 3, day, 12, 0, 0).unwrap()
    }

    #[test]
    fn parses_bucket_and_prefix() {
        assert_eq!(
            parse_cold_storage_url("s3://archive-bucket/allsource/cold").unwrap(),
            S3Location {
                bucket: "archive-bucket".into(),
                prefix: "allsource/cold".into()
            }
        );
    }

    #[test]
    fn parses_a_bare_bucket() {
        assert_eq!(
            parse_cold_storage_url("s3://archive-bucket").unwrap(),
            S3Location {
                bucket: "archive-bucket".into(),
                prefix: String::new()
            }
        );
    }

    #[test]
    fn accepts_the_r2_scheme_operators_actually_type() {
        assert_eq!(
            parse_cold_storage_url("r2://bucket/p").unwrap().bucket,
            "bucket"
        );
    }

    #[test]
    fn a_wrong_scheme_is_rejected_rather_than_guessed() {
        // Silently accepting this would hand compaction an archive that never
        // stores anything, and compaction deletes originals once archive
        // returns Ok.
        for bad in ["gs://bucket/p", "https://bucket/p", "bucket/p", ""] {
            assert!(parse_cold_storage_url(bad).is_err(), "accepted {bad:?}");
        }
    }

    #[test]
    fn a_url_with_no_bucket_is_rejected() {
        assert!(parse_cold_storage_url("s3://").is_err());
        assert!(parse_cold_storage_url("s3:///prefix-only").is_err());
    }

    #[test]
    fn trailing_slashes_do_not_change_the_prefix() {
        assert_eq!(
            parse_cold_storage_url("s3://b/p/").unwrap().prefix,
            "p".to_string()
        );
    }

    #[test]
    fn the_object_key_is_a_pure_function_of_the_window() {
        let a = archive_object_key("cold", "acme", ts(1), ts(2));
        let b = archive_object_key("cold", "acme", ts(1), ts(2));
        assert_eq!(a, b, "a retry must target the same key, or it duplicates");
        assert!(a.starts_with("cold/acme/2026-03/"), "{a}");
    }

    #[test]
    fn different_windows_get_different_keys() {
        assert_ne!(
            archive_object_key("", "acme", ts(1), ts(2)),
            archive_object_key("", "acme", ts(1), ts(3))
        );
    }

    #[test]
    fn different_tenants_never_share_a_key() {
        assert_ne!(
            archive_object_key("cold", "acme", ts(1), ts(2)),
            archive_object_key("cold", "globex", ts(1), ts(2))
        );
    }

    #[test]
    fn an_empty_prefix_does_not_produce_a_leading_slash() {
        let k = archive_object_key("", "acme", ts(1), ts(2));
        assert!(!k.starts_with('/'), "{k}");
        assert!(k.starts_with("acme/"), "{k}");
    }
}
