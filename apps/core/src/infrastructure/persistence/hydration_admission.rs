//! Whether a tenant's archive can be made resident.
//!
//! Hydration allocates in proportion to a tenant's history. Until this
//! decision existed the only thing between an oversized tenant and the
//! kernel's OOM killer was a byte budget that is checked *after* the
//! allocation and declines to act on the tenant that caused the overage, off
//! an estimate that read 554 MB while the process held 3.3 GB (gh#321).
//!
//! The decision is a pure function so it can be tested at the sizes that
//! matter — a 1.9 GB archive on a 4 GiB machine — without allocating them.

/// Resident bytes one archive byte becomes once decoded.
///
/// Measured in production: a tenant holding 154 MB of Snappy-compressed
/// Parquet took the process from ~1.1 GB to ~3.3 GB resident, so ~16×. A
/// live `serde_json::Value` costs several times its serialized form, and the
/// event index and projection state are not counted by the per-event
/// estimate at all.
pub(crate) const DEFAULT_RESIDENT_BYTES_PER_ARCHIVE_BYTE: u64 = 16;

/// Resident bytes one event costs however small its payload.
///
/// The byte ratio misses tenants of tiny, highly compressible events, where
/// the per-event struct and index entries dominate. Production, 2026-11-25:
/// 9,761,062 heartbeats in 429 MB of Parquet reached 16.1 GB resident before
/// the OOM kill, so at least ~1,650 bytes each (gh#444).
pub(crate) const DEFAULT_RESIDENT_BYTES_PER_EVENT: u64 = 2048;

/// Bytes left free for the runtime, in-flight requests and archive staging.
pub(crate) const DEFAULT_MEMORY_RESERVE_BYTES: u64 = 1024 * 1024 * 1024;

/// Why a hydration was refused, carrying the two numbers an operator needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Refusal {
    pub(crate) needed_bytes: u64,
    pub(crate) available_bytes: u64,
}

/// What admission reads about a tenant's archive without decoding it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Archive {
    pub(crate) bytes: u64,
    pub(crate) rows: u64,
}

/// How archive size converts to resident bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Costs {
    pub(crate) reserve: u64,
    pub(crate) ratio: u64,
    pub(crate) per_event: u64,
}

impl Costs {
    pub(crate) fn from_env() -> Self {
        Self {
            reserve: reserve_from_env(),
            ratio: ratio_from_env(),
            per_event: per_event_from_env(),
        }
    }
}

/// Bytes the process needs free to hold this tenant: the larger of the byte
/// and per-event estimates, plus the reserve.
pub(crate) fn needed_bytes(archive: Archive, costs: Costs) -> u64 {
    let by_bytes = archive.bytes.saturating_mul(costs.ratio.max(1));
    let by_rows = archive.rows.saturating_mul(costs.per_event);
    by_bytes.max(by_rows).saturating_add(costs.reserve)
}

/// Admit the hydration, or refuse it with the numbers behind the decision.
///
/// `available` of `None` means the platform does not report memory, and
/// admits unconditionally. Fail-open is deliberate: a probe that cannot read
/// the kernel must not be able to refuse every load on the machine, and
/// developer and CI machines are where that probe is absent.
pub(crate) fn admit(
    archive: Archive,
    available: Option<u64>,
    costs: Costs,
) -> std::result::Result<(), Refusal> {
    let Some(available_bytes) = available else {
        return Ok(());
    };
    let needed = needed_bytes(archive, costs);
    if needed <= available_bytes {
        return Ok(());
    }
    Err(Refusal {
        needed_bytes: needed,
        available_bytes,
    })
}

/// `ALLSOURCE_MEMORY_RESERVE_BYTES`, or the default. Zero is rejected: it
/// would admit a load that leaves nothing for the request serving it.
pub(crate) fn reserve_from_env() -> u64 {
    positive_env("ALLSOURCE_MEMORY_RESERVE_BYTES").unwrap_or(DEFAULT_MEMORY_RESERVE_BYTES)
}

/// `ALLSOURCE_RESIDENT_BYTE_RATIO`, or the measured default.
pub(crate) fn ratio_from_env() -> u64 {
    positive_env("ALLSOURCE_RESIDENT_BYTE_RATIO").unwrap_or(DEFAULT_RESIDENT_BYTES_PER_ARCHIVE_BYTE)
}

/// `ALLSOURCE_RESIDENT_BYTES_PER_EVENT`, or the measured default.
pub(crate) fn per_event_from_env() -> u64 {
    positive_env("ALLSOURCE_RESIDENT_BYTES_PER_EVENT").unwrap_or(DEFAULT_RESIDENT_BYTES_PER_EVENT)
}

fn positive_env(name: &str) -> Option<u64> {
    std::env::var(name)
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
        .filter(|value| *value > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1024 * 1024 * 1024;
    const MIB: u64 = 1024 * 1024;

    const COSTS: Costs = Costs {
        reserve: GIB,
        ratio: DEFAULT_RESIDENT_BYTES_PER_ARCHIVE_BYTE,
        per_event: DEFAULT_RESIDENT_BYTES_PER_EVENT,
    };

    fn bytes(bytes: u64) -> Archive {
        Archive { bytes, rows: 0 }
    }

    #[test]
    fn an_unmeasurable_platform_admits_everything() {
        assert_eq!(admit(bytes(u64::MAX), None, COSTS), Ok(()));
    }

    #[test]
    fn the_tenant_that_oom_killed_production_is_refused_on_its_old_machine() {
        // 1,910 MB of archive (the `system` tenant) against the 4 GiB
        // machine that died: 30 GB needed, so this must never be attempted.
        let refusal = admit(bytes(1910 * MIB), Some(3 * GIB), COSTS).unwrap_err();
        assert!(refusal.needed_bytes > 29 * GIB);
        assert_eq!(refusal.available_bytes, 3 * GIB);
    }

    #[test]
    fn the_tenant_from_the_outage_fits_the_scaled_machine() {
        // 154 MB of archive on an 8 GiB machine with ~6.8 GiB free: ~2.4 GB
        // plus reserve, which fits. The fix must not refuse the tenant the
        // publish path needs. Its row count was not recorded; 1 M rows of
        // ordinary payloads keeps the byte estimate the larger one.
        let archive = Archive {
            bytes: 154 * MIB,
            rows: 1_000_000,
        };
        assert_eq!(admit(archive, Some(6 * GIB + 800 * MIB), COSTS), Ok(()));
    }

    #[test]
    fn a_tenant_of_tiny_events_is_refused_on_its_event_count() {
        // gh#444: 9,761,062 heartbeats in 429 MB on the 16 GB machine with
        // ~14 GiB free. The byte estimate (~7.7 GB) admitted it and the load
        // reached 16.1 GB; the per-event estimate must refuse it.
        let archive = Archive {
            bytes: 429 * MIB,
            rows: 9_761_062,
        };
        assert!(needed_bytes(bytes(archive.bytes), COSTS) < 14 * GIB);
        let refusal = admit(archive, Some(14 * GIB), COSTS).unwrap_err();
        assert!(refusal.needed_bytes > 19 * GIB);
    }

    #[test]
    fn an_empty_archive_still_demands_the_reserve() {
        // A tenant with no files must not be admitted onto a machine with
        // nothing left: the request serving it needs room too.
        assert_eq!(needed_bytes(bytes(0), COSTS), GIB);
        assert!(admit(bytes(0), Some(GIB / 2), COSTS).is_err());
    }

    #[test]
    fn a_huge_archive_saturates_instead_of_wrapping() {
        // u64::MAX * 16 must not wrap to a small number and admit the load.
        let refusal = admit(bytes(u64::MAX), Some(GIB), COSTS).unwrap_err();
        assert_eq!(refusal.needed_bytes, u64::MAX);
        let rows = Archive {
            bytes: 0,
            rows: u64::MAX,
        };
        assert_eq!(needed_bytes(rows, COSTS), u64::MAX);
    }

    #[test]
    fn a_zero_ratio_is_treated_as_one() {
        // Otherwise a misconfigured ratio makes every archive free.
        let costs = Costs {
            reserve: 0,
            ratio: 0,
            per_event: 0,
        };
        assert_eq!(needed_bytes(bytes(100), costs), 100);
    }

    #[test]
    fn exactly_enough_is_admitted() {
        let archive = Archive {
            bytes: MIB,
            rows: 10_000,
        };
        assert_eq!(
            admit(archive, Some(needed_bytes(archive, COSTS)), COSTS),
            Ok(())
        );
    }

    #[test]
    fn env_overrides_reject_zero_and_garbage() {
        assert!(positive_env("ALLSOURCE_TEST_ABSENT_VAR_NAME").is_none());
    }
}
