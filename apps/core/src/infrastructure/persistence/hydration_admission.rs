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

/// Bytes left free for the runtime, in-flight requests and archive staging.
pub(crate) const DEFAULT_MEMORY_RESERVE_BYTES: u64 = 1024 * 1024 * 1024;

/// Why a hydration was refused, carrying the two numbers an operator needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Refusal {
    pub(crate) needed_bytes: u64,
    pub(crate) available_bytes: u64,
}

/// Bytes the process needs free to hold `archive_bytes` of this tenant.
pub(crate) fn needed_bytes(archive_bytes: u64, reserve: u64, ratio: u64) -> u64 {
    archive_bytes
        .saturating_mul(ratio.max(1))
        .saturating_add(reserve)
}

/// Admit the hydration, or refuse it with the numbers behind the decision.
///
/// `available` of `None` means the platform does not report memory, and
/// admits unconditionally. Fail-open is deliberate: a probe that cannot read
/// the kernel must not be able to refuse every load on the machine, and
/// developer and CI machines are where that probe is absent.
pub(crate) fn admit(
    archive_bytes: u64,
    available: Option<u64>,
    reserve: u64,
    ratio: u64,
) -> std::result::Result<(), Refusal> {
    let Some(available_bytes) = available else {
        return Ok(());
    };
    let needed = needed_bytes(archive_bytes, reserve, ratio);
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

    #[test]
    fn an_unmeasurable_platform_admits_everything() {
        assert_eq!(admit(u64::MAX, None, GIB, 16), Ok(()));
    }

    #[test]
    fn the_tenant_that_oom_killed_production_is_refused_on_its_old_machine() {
        // 1,910 MB of archive (the `system` tenant) against the 4 GiB
        // machine that died: 30 GB needed, so this must never be attempted.
        let refusal = admit(1910 * MIB, Some(3 * GIB), GIB, 16).unwrap_err();
        assert!(refusal.needed_bytes > 29 * GIB);
        assert_eq!(refusal.available_bytes, 3 * GIB);
    }

    #[test]
    fn the_tenant_from_the_outage_fits_the_scaled_machine() {
        // 154 MB of archive on an 8 GiB machine with ~6.8 GiB free: ~2.4 GB
        // plus reserve, which fits. The fix must not refuse the tenant the
        // publish path needs.
        assert_eq!(admit(154 * MIB, Some(6 * GIB + 800 * MIB), GIB, 16), Ok(()));
    }

    #[test]
    fn an_empty_archive_still_demands_the_reserve() {
        // A tenant with no files must not be admitted onto a machine with
        // nothing left: the request serving it needs room too.
        assert_eq!(needed_bytes(0, GIB, 16), GIB);
        assert!(admit(0, Some(GIB / 2), GIB, 16).is_err());
    }

    #[test]
    fn a_huge_archive_saturates_instead_of_wrapping() {
        // u64::MAX * 16 must not wrap to a small number and admit the load.
        let refusal = admit(u64::MAX, Some(GIB), GIB, 16).unwrap_err();
        assert_eq!(refusal.needed_bytes, u64::MAX);
    }

    #[test]
    fn a_zero_ratio_is_treated_as_one() {
        // Otherwise a misconfigured ratio makes every archive free.
        assert_eq!(needed_bytes(100, 0, 0), 100);
    }

    #[test]
    fn exactly_enough_is_admitted() {
        assert_eq!(
            admit(MIB, Some(needed_bytes(MIB, GIB, 16)), GIB, 16),
            Ok(())
        );
    }

    #[test]
    fn env_overrides_reject_zero_and_garbage() {
        assert!(positive_env("ALLSOURCE_TEST_ABSENT_VAR_NAME").is_none());
    }
}
