//! The process's own memory headroom, read from the kernel.
//!
//! Hydrating a tenant allocates in proportion to that tenant's history, so
//! something has to say when there is no room left. The resident-byte
//! estimate in `TenantLoader` cannot: it counts a serialized payload plus a
//! fixed overhead per event, while a live `serde_json::Value`, the event
//! index and the projection state all cost more than that. In production it
//! read 554 MB against a 2 GiB budget while the process held 3.3 GB, so the
//! budget never fired and the kernel's OOM killer decided instead (gh#321).
//!
//! These functions report what the kernel reports. A caller that refuses
//! work on a small `available_bytes` is right whatever the estimate says.

/// Bytes the process can still allocate before it is killed, or `None` when
/// this platform exposes no limit worth trusting.
///
/// Prefers the cgroup v2 memory limit the container is actually held to over
/// host-wide figures: on a shared host `MemAvailable` describes the host, not
/// this container's allowance.
#[cfg(target_os = "linux")]
pub fn available_bytes() -> Option<u64> {
    cgroup_v2_available().or_else(host_available)
}

/// Non-Linux builds (developer machines, CI on macOS) get no guard. Returning
/// `None` must mean "do not refuse work", never "no room".
#[cfg(not(target_os = "linux"))]
pub fn available_bytes() -> Option<u64> {
    None
}

/// `memory.max - memory.current` for this container's cgroup v2 scope.
///
/// `memory.max` reads `max` when unlimited, which is not a number and must
/// not be treated as one.
#[cfg(target_os = "linux")]
fn cgroup_v2_available() -> Option<u64> {
    let limit = read_u64("/sys/fs/cgroup/memory.max")?;
    let current = read_u64("/sys/fs/cgroup/memory.current")?;
    Some(limit.saturating_sub(current))
}

/// `MemAvailable` from `/proc/meminfo`, in bytes. The kernel's own estimate
/// of what can be allocated without swapping, which is the figure to use
/// rather than `MemFree` — page cache is reclaimable and counts as available.
#[cfg(target_os = "linux")]
fn host_available() -> Option<u64> {
    let meminfo = std::fs::read_to_string("/proc/meminfo").ok()?;
    meminfo
        .lines()
        .find_map(|line| line.strip_prefix("MemAvailable:"))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|kb| kb.parse::<u64>().ok())
        .map(|kb| kb * 1024)
}

#[cfg(target_os = "linux")]
fn read_u64(path: &str) -> Option<u64> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// True when the process still has `reserve_bytes` to spare, and when the
/// platform does not say.
///
/// Fail-open is deliberate: a reader that cannot measure must not be able to
/// stop every hydration on the machine.
pub fn has_headroom(reserve_bytes: u64) -> bool {
    available_bytes().is_none_or(|available| available >= reserve_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_zero_reserve_always_has_headroom() {
        assert!(has_headroom(0));
    }

    #[test]
    fn an_unmeasurable_platform_does_not_refuse_work() {
        // u64::MAX cannot be available anywhere, so on a platform that
        // reports a figure this is false and on one that reports nothing it
        // is true. Either way the call must not panic, and the None branch
        // must read as "allowed".
        let measured = available_bytes();
        assert_eq!(has_headroom(u64::MAX), measured.is_none());
    }

    #[test]
    fn available_bytes_is_plausible_when_reported() {
        // 128 TiB: above any machine this runs on, so a reported value past
        // it means a unit error (kB read as bytes, or a `max` sentinel
        // parsed as a number) rather than a very large host.
        if let Some(available) = available_bytes() {
            assert!(
                available < 128 * 1024 * 1024 * 1024 * 1024,
                "available_bytes reported {available}, which is not a byte count"
            );
        }
    }
}
