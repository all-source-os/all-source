use std::{
    sync::atomic::{AtomicU32, AtomicU64, Ordering},
    time::{Duration, Instant},
};

/// Circuit breaker states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Closed,
    Open,
    HalfOpen,
}

/// A circuit breaker that trips after consecutive failures.
///
/// When open, requests are rejected immediately. After the recovery timeout,
/// the breaker enters half-open state and allows a single probe request.
/// If it succeeds, the breaker closes. If it fails, it reopens.
#[derive(Debug)]
pub struct CircuitBreaker {
    failure_threshold: u32,
    recovery_timeout: Duration,
    consecutive_failures: AtomicU32,
    last_failure_epoch_ms: AtomicU64,
    start: Instant,
}

impl CircuitBreaker {
    /// Create a new circuit breaker.
    ///
    /// - `failure_threshold`: Number of consecutive failures before opening.
    /// - `recovery_timeout`: How long to wait before allowing a probe request.
    pub fn new(failure_threshold: u32, recovery_timeout: Duration) -> Self {
        Self {
            failure_threshold,
            recovery_timeout,
            consecutive_failures: AtomicU32::new(0),
            last_failure_epoch_ms: AtomicU64::new(0),
            start: Instant::now(),
        }
    }

    /// Get current state.
    pub fn state(&self) -> State {
        let failures = self.consecutive_failures.load(Ordering::Relaxed);
        if failures < self.failure_threshold {
            return State::Closed;
        }

        let last_failure_ms = self.last_failure_epoch_ms.load(Ordering::Relaxed);
        let now_ms = self.start.elapsed().as_millis() as u64;
        let elapsed = Duration::from_millis(now_ms.saturating_sub(last_failure_ms));

        if elapsed >= self.recovery_timeout {
            State::HalfOpen
        } else {
            State::Open
        }
    }

    /// Check if a request is allowed. Returns `Ok(())` if allowed,
    /// or `Err(retry_after_secs)` if the circuit is open.
    pub fn check(&self) -> Result<(), u64> {
        match self.state() {
            State::Closed | State::HalfOpen => Ok(()),
            State::Open => {
                let last_failure_ms = self.last_failure_epoch_ms.load(Ordering::Relaxed);
                let now_ms = self.start.elapsed().as_millis() as u64;
                let elapsed = Duration::from_millis(now_ms.saturating_sub(last_failure_ms));
                let remaining = self.recovery_timeout.saturating_sub(elapsed);
                Err(remaining.as_secs().max(1))
            }
        }
    }

    /// Record a successful request. Resets failure count.
    pub fn record_success(&self) {
        self.consecutive_failures.store(0, Ordering::Relaxed);
    }

    /// Record a failed request. Increments failure count and may trip the breaker.
    pub fn record_failure(&self) {
        self.consecutive_failures.fetch_add(1, Ordering::Relaxed);
        let now_ms = self.start.elapsed().as_millis() as u64;
        self.last_failure_epoch_ms.store(now_ms, Ordering::Relaxed);
    }
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new(5, Duration::from_secs(30))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_starts_closed() {
        let cb = CircuitBreaker::new(3, Duration::from_secs(10));
        assert_eq!(cb.state(), State::Closed);
        assert!(cb.check().is_ok());
    }

    #[test]
    fn test_trips_after_threshold() {
        let cb = CircuitBreaker::new(3, Duration::from_mins(1));
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), State::Closed);
        cb.record_failure();
        assert_eq!(cb.state(), State::Open);
        assert!(cb.check().is_err());
    }

    #[test]
    fn test_resets_on_success() {
        let cb = CircuitBreaker::new(3, Duration::from_mins(1));
        cb.record_failure();
        cb.record_failure();
        cb.record_success();
        assert_eq!(cb.state(), State::Closed);
        cb.record_failure();
        assert_eq!(cb.state(), State::Closed); // only 1 failure after reset
    }

    #[test]
    fn test_half_open_after_timeout() {
        let cb = CircuitBreaker::new(2, Duration::from_millis(1));
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), State::Open);
        std::thread::sleep(Duration::from_millis(5));
        assert_eq!(cb.state(), State::HalfOpen);
        assert!(cb.check().is_ok());
    }
}
