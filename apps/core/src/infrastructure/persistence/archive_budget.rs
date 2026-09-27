//! Admission and cooperative work limits for integrity-sensitive archive reads.
//! These bound accepted input and work between I/O calls, not kernel I/O latency
//! or exact allocator RSS. HTTP callers also need bounded blocking-worker capacity.

use crate::error::{AllSourceError, Result};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Clone, Debug)]
pub struct ArchiveReadLimits {
    pub timeout: Duration,
    pub max_entries: usize,
    pub max_files: usize,
    pub max_file_bytes: u64,
    pub max_compressed_bytes: u64,
    pub max_uncompressed_bytes: u64,
    pub max_rows: u64,
}

impl Default for ArchiveReadLimits {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(4),
            max_entries: 100_000,
            max_files: 50_000,
            max_file_bytes: 32 * 1024 * 1024,
            max_compressed_bytes: 256 * 1024 * 1024,
            max_uncompressed_bytes: 256 * 1024 * 1024,
            // The existing 566,486-row archive needs headroom. A 600,000-row
            // synthetic load fits the measured 4 GiB / single-worker envelope;
            // 750,000 rows OOM under the same 2.5 GiB background reservation.
            max_rows: 600_000,
        }
    }
}

pub(crate) struct ArchiveReadBudget {
    limits: ArchiveReadLimits,
    started: Instant,
    entries: usize,
    files: usize,
    compressed_bytes: u64,
    uncompressed_bytes: u64,
    rows: u64,
    cancellation: Option<Arc<AtomicBool>>,
}

impl ArchiveReadBudget {
    pub(crate) fn new(limits: ArchiveReadLimits) -> Self {
        Self {
            limits,
            started: Instant::now(),
            entries: 0,
            files: 0,
            compressed_bytes: 0,
            uncompressed_bytes: 0,
            rows: 0,
            cancellation: None,
        }
    }

    pub(crate) fn with_cancellation(mut self, cancellation: Option<Arc<AtomicBool>>) -> Self {
        self.cancellation = cancellation;
        self
    }

    pub(crate) fn remaining(&self) -> Result<Duration> {
        check_cancelled(self.cancellation.as_deref())?;
        self.limits
            .timeout
            .checked_sub(self.started.elapsed())
            .filter(|remaining| !remaining.is_zero())
            .ok_or_else(|| exceeded("elapsed time"))
    }

    pub(crate) fn check(&self) -> Result<()> {
        self.remaining().map(|_| ())
    }

    pub(crate) fn entry(&mut self) -> Result<()> {
        self.check()?;
        self.entries = self
            .entries
            .checked_add(1)
            .ok_or_else(|| exceeded("entries"))?;
        if self.entries > self.limits.max_entries {
            return Err(exceeded("entries"));
        }
        Ok(())
    }

    pub(crate) fn file(&mut self) -> Result<()> {
        self.check()?;
        self.files = self.files.checked_add(1).ok_or_else(|| exceeded("files"))?;
        if self.files > self.limits.max_files {
            return Err(exceeded("files"));
        }
        Ok(())
    }

    pub(crate) fn compressed(&mut self, bytes: u64) -> Result<()> {
        self.check()?;
        if bytes > self.limits.max_file_bytes {
            return Err(exceeded("file bytes"));
        }
        charge(
            &mut self.compressed_bytes,
            bytes,
            self.limits.max_compressed_bytes,
            "compressed bytes",
        )
    }

    pub(crate) fn decoded_metadata(&mut self, rows: i64, bytes: i64) -> Result<()> {
        self.check()?;
        let rows = u64::try_from(rows).map_err(|_| exceeded("invalid row count"))?;
        let bytes = u64::try_from(bytes).map_err(|_| exceeded("invalid decoded size"))?;
        charge(&mut self.rows, rows, self.limits.max_rows, "rows")?;
        charge(
            &mut self.uncompressed_bytes,
            bytes,
            self.limits.max_uncompressed_bytes,
            "uncompressed bytes",
        )
    }
}

pub(crate) fn check_cancelled(cancellation: Option<&AtomicBool>) -> Result<()> {
    if cancellation.is_some_and(|flag| flag.load(Ordering::Acquire)) {
        return Err(AllSourceError::StorageError(
            "Strict archive operation cancelled".into(),
        ));
    }
    Ok(())
}

fn charge(total: &mut u64, amount: u64, limit: u64, dimension: &str) -> Result<()> {
    *total = total
        .checked_add(amount)
        .ok_or_else(|| exceeded(dimension))?;
    if *total > limit {
        return Err(exceeded(dimension));
    }
    Ok(())
}

fn exceeded(dimension: &str) -> AllSourceError {
    AllSourceError::StorageError(format!("Strict archive read budget exceeded: {dimension}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_and_byte_charges_accumulate_and_reject_overflow() {
        let mut budget = ArchiveReadBudget::new(ArchiveReadLimits {
            max_rows: 2,
            max_uncompressed_bytes: 8,
            ..Default::default()
        });
        budget.decoded_metadata(1, 4).unwrap();
        budget.decoded_metadata(1, 4).unwrap();
        assert!(budget.decoded_metadata(1, 0).is_err());
        let mut total = u64::MAX;
        assert!(charge(&mut total, 1, u64::MAX, "test").is_err());
    }

    #[test]
    fn default_row_budget_accepts_existing_dense_history_but_refuses_growth_past_ceiling() {
        let mut budget = ArchiveReadBudget::new(ArchiveReadLimits::default());
        budget.decoded_metadata(566_486, 81_181_257).unwrap();
        budget.decoded_metadata(33_514, 0).unwrap();
        assert!(
            budget
                .decoded_metadata(1, 0)
                .unwrap_err()
                .to_string()
                .contains("budget exceeded: rows")
        );
    }

    #[test]
    fn expired_budget_rejects_work_before_it_starts() {
        let mut budget = ArchiveReadBudget::new(ArchiveReadLimits {
            timeout: Duration::ZERO,
            ..Default::default()
        });
        assert!(budget.remaining().is_err());
        assert!(budget.entry().is_err());
        assert!(budget.compressed(0).is_err());
    }

    #[test]
    fn cancellation_and_invalid_metadata_refuse_more_work() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut budget = ArchiveReadBudget::new(ArchiveReadLimits::default())
            .with_cancellation(Some(Arc::clone(&cancelled)));
        assert!(budget.decoded_metadata(-1, 0).is_err());
        assert!(budget.decoded_metadata(0, -1).is_err());
        budget.entry().unwrap();
        cancelled.store(true, Ordering::Release);
        assert!(budget.entry().is_err());
        assert!(budget.remaining().is_err());
    }
}
