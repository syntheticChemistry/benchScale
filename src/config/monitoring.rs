// SPDX-License-Identifier: AGPL-3.0-or-later
//! Monitoring Configuration
//!
//! **Phase 2: Configuration Externalization**
//!
//! Configuration for VM health monitoring, senescence detection,
//! and DHCP IP re-discovery (Evolution #20, #21, #22).
//!
//! # Philosophy
//! - **Runtime Discovery**: IP tracking via MAC addresses
//! - **Self-Healing**: Auto-recovery from failures
//! - **Configurable**: Adapt to workload characteristics

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Monitoring configuration for VM health checks and senescence detection
///
/// # Examples
///
/// ```rust
/// use benchscale::config::MonitoringConfig;
///
/// // Quick VMs (default)
/// let config = MonitoringConfig::default();
/// assert_eq!(config.max_failures, 10); // 100s tolerance
///
/// // Long-running builds
/// let config = MonitoringConfig {
///     max_failures: 180, // 30 min tolerance
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MonitoringConfig {
    /// Health check interval (seconds)
    ///
    /// **Default**: 10s
    ///
    /// How often to check VM health (ping + SSH + cloud-init status).
    #[serde(default = "default_check_interval")]
    pub check_interval_secs: u64,

    /// Max consecutive failures before declaring VM failed
    ///
    /// **Default**: 10 (100s tolerance with 10s checks)
    ///
    /// **Evolution #21**: This was hardcoded to 10. Now configurable:
    /// - Quick VMs: 10 → 100s tolerance
    /// - Desktop builds: 60 → 10min tolerance
    /// - Cloud-init with packages: 180 → 30min tolerance
    #[serde(default = "default_max_failures")]
    pub max_failures: u32,

    /// Stall detection threshold (seconds)
    ///
    /// **Default**: 120s (2 minutes)
    ///
    /// If no progress for this duration, consider VM stalled.
    #[serde(default = "default_stall_threshold")]
    pub stall_threshold_secs: u64,

    /// IP re-discovery interval (health checks)
    ///
    /// **Default**: 10 checks (100s with 10s interval)
    ///
    /// **Evolution #22**: How often to check for DHCP lease changes.
    /// Prevents false negatives when IP addresses change during long builds.
    #[serde(default = "default_ip_rediscovery_interval")]
    pub ip_rediscovery_interval: u32,

    /// Enable automatic IP re-discovery
    ///
    /// **Default**: true
    ///
    /// **Evolution #22**: Disable if you're using static IPs or want to
    /// reduce DHCP query overhead.
    #[serde(default = "default_enable_ip_rediscovery")]
    pub enable_ip_rediscovery: bool,
}

// Default value functions
fn default_check_interval() -> u64 {
    10
}
fn default_max_failures() -> u32 {
    10
} // 100s tolerance
fn default_stall_threshold() -> u64 {
    120
} // 2 min
fn default_ip_rediscovery_interval() -> u32 {
    10
} // Every 10 checks = 100s
fn default_enable_ip_rediscovery() -> bool {
    true
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            check_interval_secs: default_check_interval(),
            max_failures: default_max_failures(),
            stall_threshold_secs: default_stall_threshold(),
            ip_rediscovery_interval: default_ip_rediscovery_interval(),
            enable_ip_rediscovery: default_enable_ip_rediscovery(),
        }
    }
}

impl MonitoringConfig {
    /// Convert check interval to Duration
    pub fn check_interval(&self) -> Duration {
        Duration::from_secs(self.check_interval_secs)
    }

    /// Convert stall threshold to Duration
    pub fn stall_threshold(&self) -> Duration {
        Duration::from_secs(self.stall_threshold_secs)
    }

    /// Calculate total failure tolerance duration
    ///
    /// Returns the maximum time a VM can be unhealthy before being
    /// declared failed (max_failures × check_interval).
    pub fn failure_tolerance(&self) -> Duration {
        Duration::from_secs(self.check_interval_secs * u64::from(self.max_failures))
    }

    /// Calculate IP re-discovery interval duration
    ///
    /// Returns how often IP re-discovery happens in real time.
    pub fn ip_rediscovery_duration(&self) -> Duration {
        Duration::from_secs(self.check_interval_secs * u64::from(self.ip_rediscovery_interval))
    }

    /// Create config optimized for quick VMs
    ///
    /// - 10s checks
    /// - 10 max failures (100s tolerance)
    /// - 2min stall threshold
    pub fn for_quick_vms() -> Self {
        Self {
            max_failures: 10,
            ..Default::default()
        }
    }

    /// Create config optimized for desktop builds
    ///
    /// - 10s checks
    /// - 60 max failures (10min tolerance)
    /// - 3min stall threshold
    pub fn for_desktop_builds() -> Self {
        Self {
            max_failures: 60,
            stall_threshold_secs: 180,
            ..Default::default()
        }
    }

    /// Create config optimized for cloud-init with packages
    ///
    /// **Evolution #21**: Handles long package downloads.
    ///
    /// - 10s checks
    /// - 180 max failures (30min tolerance)
    /// - 5min stall threshold
    pub fn for_cloud_init_packages() -> Self {
        Self {
            max_failures: 180,
            stall_threshold_secs: 300,
            ..Default::default()
        }
    }

    /// Validate configuration values
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.check_interval_secs == 0 {
            anyhow::bail!("check_interval_secs must be > 0");
        }
        if self.max_failures == 0 {
            anyhow::bail!("max_failures must be > 0");
        }
        if self.stall_threshold_secs == 0 {
            anyhow::bail!("stall_threshold_secs must be > 0");
        }
        if self.ip_rediscovery_interval == 0 {
            anyhow::bail!("ip_rediscovery_interval must be > 0");
        }

        // Sanity checks
        if self.check_interval_secs > 300 {
            anyhow::bail!("check_interval_secs > 5min is unreasonably long");
        }
        if self.max_failures > 3600 {
            anyhow::bail!("max_failures > 3600 is unreasonably large");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_is_valid() {
        let config = MonitoringConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_default_values() {
        let config = MonitoringConfig::default();
        assert_eq!(config.check_interval_secs, 10);
        assert_eq!(config.max_failures, 10);
        assert_eq!(config.stall_threshold_secs, 120);
        assert_eq!(config.ip_rediscovery_interval, 10);
        assert!(config.enable_ip_rediscovery);
    }

    #[test]
    fn test_duration_conversions() {
        let config = MonitoringConfig::default();
        assert_eq!(config.check_interval(), Duration::from_secs(10));
        assert_eq!(config.stall_threshold(), Duration::from_secs(120));
    }

    #[test]
    fn test_failure_tolerance_calculation() {
        let config = MonitoringConfig::default();
        // 10 failures × 10s = 100s
        assert_eq!(config.failure_tolerance(), Duration::from_secs(100));

        let config = MonitoringConfig {
            max_failures: 180,
            ..Default::default()
        };
        // 180 failures × 10s = 1800s = 30min
        assert_eq!(config.failure_tolerance(), Duration::from_secs(1800));
    }

    #[test]
    fn test_ip_rediscovery_duration_calculation() {
        let config = MonitoringConfig::default();
        // 10 checks × 10s = 100s
        assert_eq!(config.ip_rediscovery_duration(), Duration::from_secs(100));
    }

    #[test]
    fn test_for_quick_vms() {
        let config = MonitoringConfig::for_quick_vms();
        assert_eq!(config.max_failures, 10);
        assert_eq!(config.failure_tolerance(), Duration::from_secs(100));
    }

    #[test]
    fn test_for_desktop_builds() {
        let config = MonitoringConfig::for_desktop_builds();
        assert_eq!(config.max_failures, 60);
        assert_eq!(config.failure_tolerance(), Duration::from_secs(600)); // 10 min
        assert_eq!(config.stall_threshold_secs, 180); // 3 min
    }

    #[test]
    fn test_for_cloud_init_packages() {
        let config = MonitoringConfig::for_cloud_init_packages();
        assert_eq!(config.max_failures, 180);
        assert_eq!(config.failure_tolerance(), Duration::from_secs(1800)); // 30 min
        assert_eq!(config.stall_threshold_secs, 300); // 5 min
    }

    #[test]
    fn test_validation_rejects_zero_check_interval() {
        let config = MonitoringConfig {
            check_interval_secs: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validation_rejects_zero_max_failures() {
        let config = MonitoringConfig {
            max_failures: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validation_rejects_excessive_check_interval() {
        let config = MonitoringConfig {
            check_interval_secs: 600, // 10 min
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_serde_yaml_serialization() {
        let config = MonitoringConfig::default();
        let yaml = serde_yaml::to_string(&config).unwrap();
        assert!(yaml.contains("check_interval_secs"));
        assert!(yaml.contains("max_failures"));
    }

    #[test]
    fn test_serde_yaml_deserialization() {
        let yaml = r#"
check_interval_secs: 20
max_failures: 180
enable_ip_rediscovery: false
"#;
        let config: MonitoringConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.check_interval_secs, 20);
        assert_eq!(config.max_failures, 180);
        assert!(!config.enable_ip_rediscovery);
        // Others should use defaults
        assert_eq!(config.stall_threshold_secs, 120);
    }

    #[test]
    fn test_evolution_21_cloud_init_tolerance() {
        // Evolution #21: 30-minute tolerance for cloud-init
        let config = MonitoringConfig::for_cloud_init_packages();
        assert_eq!(config.failure_tolerance().as_secs(), 1800); // 30 min
    }

    #[test]
    fn test_evolution_22_ip_rediscovery() {
        // Evolution #22: IP re-discovery every 100s by default
        let config = MonitoringConfig::default();
        assert!(config.enable_ip_rediscovery);
        assert_eq!(config.ip_rediscovery_duration().as_secs(), 100);
    }
}
