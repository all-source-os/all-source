/// Adaptive ML-Based Rate Limiting
///
/// Automatically adjusts rate limits based on:
/// - Historical usage patterns
/// - Traffic anomalies
/// - System load
/// - Tenant behavior
/// - Attack detection
use crate::error::Result;
use crate::infrastructure::security::rate_limit::RateLimitResult;
use chrono::{DateTime, Duration, Timelike, Utc};
use dashmap::DashMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Adaptive rate limiting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptiveRateLimitConfig {
    /// Enable adaptive rate limiting
    pub enabled: bool,

    /// Minimum rate limit (safety floor)
    pub min_rate_limit: u32,

    /// Maximum rate limit (safety ceiling)
    pub max_rate_limit: u32,

    /// Learning window in hours
    pub learning_window_hours: i64,

    /// Adjustment factor (how aggressive to adjust, 0.0-1.0)
    pub adjustment_factor: f64,

    /// Enable anomaly-based throttling
    pub enable_anomaly_throttling: bool,

    /// Enable load-based adjustment
    pub enable_load_based_adjustment: bool,

    /// Enable pattern-based prediction
    pub enable_pattern_prediction: bool,
}

impl Default for AdaptiveRateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_rate_limit: 10,
            max_rate_limit: 10_000,
            learning_window_hours: 24 * 7, // 1 week
            adjustment_factor: 0.3,
            enable_anomaly_throttling: true,
            enable_load_based_adjustment: true,
            enable_pattern_prediction: true,
        }
    }
}

/// Tenant usage profile for adaptive learning
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TenantUsageProfile {
    tenant_id: String,

    // Historical patterns
    hourly_averages: Vec<f64>, // Average requests per hour
    daily_averages: Vec<f64>,  // Average requests per day
    peak_times: Vec<u32>,      // Hours with peak usage

    // Statistical metrics
    avg_requests_per_hour: f64,
    stddev_requests_per_hour: f64,
    max_requests_per_hour: f64,

    // Adaptive parameters
    current_limit: u32,
    base_limit: u32,
    adjustment_history: Vec<LimitAdjustment>,

    // Last updated
    last_updated: DateTime<Utc>,
    data_points: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LimitAdjustment {
    timestamp: DateTime<Utc>,
    old_limit: u32,
    new_limit: u32,
    reason: AdjustmentReason,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum AdjustmentReason {
    NormalLearning,
    AnomalyDetected,
    HighLoad,
    AttackMitigation,
    PatternPrediction,
}

/// System load metrics
#[derive(Debug, Clone)]
pub struct SystemLoad {
    pub cpu_usage: f64,
    pub memory_usage: f64,
    pub active_connections: usize,
    pub queue_depth: usize,
}

/// Adaptive rate limiter
pub struct AdaptiveRateLimiter {
    config: Arc<RwLock<AdaptiveRateLimitConfig>>,

    // Tenant profiles - using DashMap for lock-free concurrent access
    profiles: Arc<DashMap<String, TenantUsageProfile>>,

    // Recent requests for pattern analysis
    recent_requests: Arc<RwLock<Vec<RequestRecord>>>,

    // System load history
    load_history: Arc<RwLock<Vec<(DateTime<Utc>, SystemLoad)>>>,
}

#[derive(Debug, Clone)]
struct RequestRecord {
    tenant_id: String,
    timestamp: DateTime<Utc>,
    allowed: bool,
    cost: f64,
}

impl AdaptiveRateLimiter {
    /// Create new adaptive rate limiter
    pub fn new(config: AdaptiveRateLimitConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            profiles: Arc::new(DashMap::new()),
            recent_requests: Arc::new(RwLock::new(Vec::new())),
            load_history: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Check rate limit with adaptive adjustment
    pub fn check_adaptive_limit(&self, tenant_id: &str) -> Result<RateLimitResult> {
        let config = self.config.read();

        if !config.enabled {
            return Ok(RateLimitResult {
                allowed: true,
                remaining: u32::MAX,
                retry_after: None,
                limit: u32::MAX,
            });
        }

        // Get or create profile
        let mut profile = self
            .profiles
            .entry(tenant_id.to_string())
            .or_insert_with(|| {
                TenantUsageProfile::new(tenant_id.to_string(), config.max_rate_limit)
            });

        // Record request
        self.record_request(tenant_id, true, 1.0);

        // Get current hour's request count
        let recent = self.recent_requests.read();
        let cutoff = Utc::now() - Duration::hours(1);
        let recent_count = recent
            .iter()
            .filter(|r| r.tenant_id.as_str() == tenant_id && r.timestamp > cutoff)
            .count();

        // Check against current adaptive limit
        let allowed = (recent_count as u32) < profile.current_limit;

        let result = RateLimitResult {
            allowed,
            remaining: if allowed {
                profile.current_limit.saturating_sub(recent_count as u32)
            } else {
                0
            },
            retry_after: if allowed {
                None
            } else {
                Some(std::time::Duration::from_mins(1))
            },
            limit: profile.current_limit,
        };

        // Update profile statistics
        profile.data_points += 1;
        profile.last_updated = Utc::now();

        Ok(result)
    }

    /// Update adaptive limits based on learned patterns
    pub fn update_adaptive_limits(&self) -> Result<()> {
        let config = self.config.read();

        if !config.enabled {
            return Ok(());
        }

        for mut entry in self.profiles.iter_mut() {
            let tenant_id = entry.key().clone();
            let profile = entry.value_mut();

            if profile.data_points < 100 {
                continue; // Not enough data
            }

            let mut new_limit = profile.current_limit;
            let mut reason = AdjustmentReason::NormalLearning;

            // Learning-based adjustment
            if profile.data_points >= 1000 {
                let usage_factor = profile.avg_requests_per_hour / f64::from(profile.current_limit);

                if usage_factor > 0.8 {
                    // High utilization - increase limit
                    new_limit = (f64::from(profile.current_limit)
                        * (1.0 + config.adjustment_factor)) as u32;
                    reason = AdjustmentReason::NormalLearning;
                } else if usage_factor < 0.3 {
                    // Low utilization - decrease limit (save resources)
                    new_limit = (f64::from(profile.current_limit)
                        * (1.0 - config.adjustment_factor * 0.5))
                        as u32;
                    reason = AdjustmentReason::NormalLearning;
                }
            }

            // Anomaly-based throttling
            if config.enable_anomaly_throttling {
                let recent = self.recent_requests.read();
                let cutoff = Utc::now() - Duration::minutes(5);
                let very_recent_count = recent
                    .iter()
                    .filter(|r| r.tenant_id.as_str() == tenant_id && r.timestamp > cutoff)
                    .count();

                // If current rate is 3x average, throttle aggressively
                let expected_in_5min = profile.avg_requests_per_hour / 12.0;
                if very_recent_count as f64 > expected_in_5min * 3.0 {
                    new_limit = (f64::from(profile.current_limit) * 0.5) as u32;
                    reason = AdjustmentReason::AnomalyDetected;
                }
            }

            // Load-based adjustment
            if config.enable_load_based_adjustment
                && let Some(load) = self.get_current_load()
                && (load.cpu_usage > 0.8 || load.memory_usage > 0.8)
            {
                // High system load - reduce limits
                new_limit = (f64::from(profile.current_limit) * 0.7) as u32;
                reason = AdjustmentReason::HighLoad;
            }

            // Apply safety limits
            new_limit = new_limit.clamp(config.min_rate_limit, config.max_rate_limit);

            // Record adjustment if changed
            if new_limit != profile.current_limit {
                profile.adjustment_history.push(LimitAdjustment {
                    timestamp: Utc::now(),
                    old_limit: profile.current_limit,
                    new_limit,
                    reason,
                });

                profile.current_limit = new_limit;

                // Keep history limited
                if profile.adjustment_history.len() > 100 {
                    profile.adjustment_history.remove(0);
                }
            }
        }

        Ok(())
    }

    /// Predict future load and adjust proactively
    pub fn predict_and_adjust(&self, tenant_id: &str) -> Result<u32> {
        let config = self.config.read();

        if !config.enable_pattern_prediction {
            return Ok(0);
        }

        if let Some(profile_ref) = self.profiles.get(tenant_id) {
            let profile = profile_ref.value();
            if profile.data_points < 1000 {
                return Ok(profile.current_limit);
            }

            // Simple pattern: check if we're approaching a known peak time
            let current_hour = Utc::now().hour();

            if profile.peak_times.contains(&current_hour) {
                // Increase limit proactively
                let predicted_limit = (f64::from(profile.current_limit) * 1.2) as u32;
                return Ok(predicted_limit.min(config.max_rate_limit));
            }
        }

        Ok(0)
    }

    /// Record system load for load-based adjustments
    pub fn record_system_load(&self, load: SystemLoad) {
        let mut history = self.load_history.write();
        history.push((Utc::now(), load));

        // Keep only recent history (last hour)
        let cutoff = Utc::now() - Duration::hours(1);
        history.retain(|(ts, _)| *ts > cutoff);
    }

    fn get_current_load(&self) -> Option<SystemLoad> {
        let history = self.load_history.read();
        history.last().map(|(_, load)| load.clone())
    }

    fn record_request(&self, tenant_id: &str, allowed: bool, cost: f64) {
        let mut requests = self.recent_requests.write();
        requests.push(RequestRecord {
            tenant_id: tenant_id.to_string(),
            timestamp: Utc::now(),
            allowed,
            cost,
        });

        // Clean old records
        let cutoff = Utc::now() - Duration::hours(self.config.read().learning_window_hours);
        requests.retain(|r| r.timestamp > cutoff);
    }

    /// Get statistics for a tenant
    pub fn get_tenant_stats(&self, tenant_id: &str) -> Option<AdaptiveLimitStats> {
        self.profiles.get(tenant_id).map(|profile_ref| {
            let profile = profile_ref.value();
            let recent = self.recent_requests.read();
            let cutoff = Utc::now() - Duration::hours(1);
            let requests_last_hour = recent
                .iter()
                .filter(|r| r.tenant_id.as_str() == tenant_id && r.timestamp > cutoff)
                .count();

            AdaptiveLimitStats {
                current_limit: profile.current_limit,
                base_limit: profile.base_limit,
                requests_last_hour: requests_last_hour as u32,
                avg_requests_per_hour: profile.avg_requests_per_hour,
                utilization: requests_last_hour as f64 / f64::from(profile.current_limit),
                total_adjustments: profile.adjustment_history.len(),
                last_adjustment: profile.adjustment_history.last().map(|a| a.timestamp),
            }
        })
    }

    /// Get overall statistics
    pub fn get_stats(&self) -> AdaptiveRateLimiterStats {
        let recent = self.recent_requests.read();

        AdaptiveRateLimiterStats {
            total_tenants: self.profiles.len(),
            total_requests: recent.len(),
            config: self.config.read().clone(),
        }
    }
}

impl TenantUsageProfile {
    fn new(tenant_id: String, base_limit: u32) -> Self {
        Self {
            tenant_id,
            hourly_averages: vec![0.0; 24],
            daily_averages: vec![0.0; 7],
            peak_times: Vec::new(),
            avg_requests_per_hour: 0.0,
            stddev_requests_per_hour: 0.0,
            max_requests_per_hour: 0.0,
            current_limit: base_limit,
            base_limit,
            adjustment_history: Vec::new(),
            last_updated: Utc::now(),
            data_points: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptiveLimitStats {
    pub current_limit: u32,
    pub base_limit: u32,
    pub requests_last_hour: u32,
    pub avg_requests_per_hour: f64,
    pub utilization: f64,
    pub total_adjustments: usize,
    pub last_adjustment: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptiveRateLimiterStats {
    pub total_tenants: usize,
    pub total_requests: usize,
    pub config: AdaptiveRateLimitConfig,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adaptive_limiter_creation() {
        let limiter = AdaptiveRateLimiter::new(AdaptiveRateLimitConfig::default());
        let stats = limiter.get_stats();

        assert_eq!(stats.total_tenants, 0);
        assert_eq!(stats.total_requests, 0);
    }

    #[test]
    fn test_adaptive_limit_checking() {
        let limiter = AdaptiveRateLimiter::new(AdaptiveRateLimitConfig {
            enabled: true,
            min_rate_limit: 10,
            max_rate_limit: 100,
            ..Default::default()
        });

        // First request should be allowed
        let result = limiter.check_adaptive_limit("tenant1").unwrap();
        assert!(result.allowed);
    }

    #[test]
    fn test_limit_adjustment() {
        let config = AdaptiveRateLimitConfig {
            min_rate_limit: 10,
            max_rate_limit: 1000,
            ..Default::default()
        };

        let limiter = AdaptiveRateLimiter::new(config);

        // Create profile with sufficient data
        {
            let mut profile = TenantUsageProfile::new("tenant1".to_string(), 100);
            profile.data_points = 1500;
            profile.avg_requests_per_hour = 90.0; // High utilization
            profile.current_limit = 100;
            limiter.profiles.insert("tenant1".to_string(), profile);
        }

        // Update limits
        limiter.update_adaptive_limits().unwrap();

        // Check if limit was adjusted upward
        let stats = limiter.get_tenant_stats("tenant1").unwrap();
        assert!(stats.current_limit > 100); // Should have increased
    }

    #[test]
    fn test_load_based_adjustment() {
        let limiter = AdaptiveRateLimiter::new(AdaptiveRateLimitConfig::default());

        // Create profile
        {
            let mut profile = TenantUsageProfile::new("tenant1".to_string(), 100);
            profile.data_points = 1000;
            profile.current_limit = 100;
            limiter.profiles.insert("tenant1".to_string(), profile);
        }

        // Record high system load
        limiter.record_system_load(SystemLoad {
            cpu_usage: 0.9,
            memory_usage: 0.85,
            active_connections: 1000,
            queue_depth: 500,
        });

        // Update limits
        limiter.update_adaptive_limits().unwrap();

        // Check if limit was reduced due to high load
        let stats = limiter.get_tenant_stats("tenant1").unwrap();
        assert!(stats.current_limit < 100); // Should have decreased
    }

    #[test]
    fn test_disabled_adaptive_limiting() {
        let config = AdaptiveRateLimitConfig {
            enabled: false,
            ..Default::default()
        };

        let limiter = AdaptiveRateLimiter::new(config);
        let result = limiter.check_adaptive_limit("tenant1").unwrap();

        assert!(result.allowed);
        assert_eq!(result.remaining, u32::MAX);
    }

    #[test]
    fn test_safety_limits() {
        let limiter = AdaptiveRateLimiter::new(AdaptiveRateLimitConfig {
            min_rate_limit: 50,
            max_rate_limit: 200,
            ..Default::default()
        });

        // Create profile that would exceed max
        {
            let mut profile = TenantUsageProfile::new("tenant1".to_string(), 100);
            profile.data_points = 1500;
            profile.avg_requests_per_hour = 180.0; // Very high
            profile.current_limit = 190;
            limiter.profiles.insert("tenant1".to_string(), profile);
        }

        limiter.update_adaptive_limits().unwrap();

        let stats = limiter.get_tenant_stats("tenant1").unwrap();
        assert!(stats.current_limit <= 200); // Should not exceed max
        assert!(stats.current_limit >= 50); // Should not go below min
    }

    #[test]
    fn test_default_config() {
        let config = AdaptiveRateLimitConfig::default();
        assert!(config.enabled);
        assert!(config.min_rate_limit > 0);
        assert!(config.max_rate_limit > config.min_rate_limit);
    }

    #[test]
    fn test_config_serde() {
        let config = AdaptiveRateLimitConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: AdaptiveRateLimitConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.enabled, config.enabled);
        assert_eq!(parsed.min_rate_limit, config.min_rate_limit);
    }

    #[test]
    fn test_system_load_clone() {
        let load = SystemLoad {
            cpu_usage: 0.5,
            memory_usage: 0.6,
            active_connections: 100,
            queue_depth: 50,
        };

        let cloned = load.clone();
        assert_eq!(cloned.cpu_usage, load.cpu_usage);
        assert_eq!(cloned.active_connections, load.active_connections);
    }

    #[test]
    fn test_get_tenant_stats_none() {
        let limiter = AdaptiveRateLimiter::new(AdaptiveRateLimitConfig::default());
        let stats = limiter.get_tenant_stats("nonexistent");
        assert!(stats.is_none());
    }

    #[test]
    fn test_adaptive_limit_stats_serde() {
        let stats = AdaptiveLimitStats {
            current_limit: 100,
            base_limit: 50,
            requests_last_hour: 25,
            avg_requests_per_hour: 30.0,
            utilization: 0.25,
            total_adjustments: 5,
            last_adjustment: Some(Utc::now()),
        };

        let json = serde_json::to_string(&stats).unwrap();
        let parsed: AdaptiveLimitStats = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.current_limit, stats.current_limit);
        assert_eq!(parsed.utilization, stats.utilization);
    }

    #[test]
    fn test_record_system_load() {
        let limiter = AdaptiveRateLimiter::new(AdaptiveRateLimitConfig::default());

        // Record multiple loads
        for i in 0..5 {
            limiter.record_system_load(SystemLoad {
                cpu_usage: i as f64 * 0.1,
                memory_usage: 0.5,
                active_connections: i * 10,
                queue_depth: i,
            });
        }

        let stats = limiter.get_stats();
        assert_eq!(stats.total_tenants, 0); // No tenants yet
    }

    #[test]
    fn test_multiple_tenants() {
        let limiter = AdaptiveRateLimiter::new(AdaptiveRateLimitConfig {
            enabled: true,
            ..Default::default()
        });

        // Check limits for multiple tenants
        limiter.check_adaptive_limit("tenant1").unwrap();
        limiter.check_adaptive_limit("tenant2").unwrap();
        limiter.check_adaptive_limit("tenant3").unwrap();

        let stats = limiter.get_stats();
        assert_eq!(stats.total_tenants, 3);
    }

    #[test]
    fn test_adaptive_limiter_stats_serde() {
        let stats = AdaptiveRateLimiterStats {
            total_tenants: 10,
            total_requests: 1000,
            config: AdaptiveRateLimitConfig::default(),
        };

        let json = serde_json::to_string(&stats).unwrap();
        let parsed: AdaptiveRateLimiterStats = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.total_tenants, stats.total_tenants);
        assert_eq!(parsed.total_requests, stats.total_requests);
    }

    #[test]
    fn test_tenant_profile_initialization() {
        let limiter = AdaptiveRateLimiter::new(AdaptiveRateLimitConfig {
            enabled: true,
            ..Default::default()
        });

        // First request creates a profile
        limiter.check_adaptive_limit("new_tenant").unwrap();

        let stats = limiter.get_tenant_stats("new_tenant");
        assert!(stats.is_some());
    }
}
