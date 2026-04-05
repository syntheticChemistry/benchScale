// SPDX-License-Identifier: AGPL-3.0-or-later
//! VM health monitoring and status checking
//!
//! Provides health check capabilities for VMs including boot status,
//! network connectivity, and resource utilization.

use std::time::{Duration, Instant};
use tracing::{debug, warn};

use crate::{Error, Result};

/// VM health status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthStatus {
    /// VM is healthy and operational
    Healthy,
    /// VM is booting
    Booting,
    /// VM is unhealthy
    Unhealthy,
    /// VM status is unknown
    Unknown,
}

/// Health check result
#[derive(Debug, Clone)]
pub struct HealthCheck {
    /// Overall health status
    pub status: HealthStatus,
    /// Boot completion status
    pub boot_complete: bool,
    /// Boot time in milliseconds (if complete)
    pub boot_time_ms: Option<u64>,
    /// Network connectivity status
    pub network_reachable: bool,
    /// Time taken to perform health check
    pub check_duration: Duration,
    /// Any error messages from logs
    pub errors: Vec<String>,
}

impl HealthCheck {
    /// Create a new health check result
    pub fn new(status: HealthStatus) -> Self {
        Self {
            status,
            boot_complete: false,
            boot_time_ms: None,
            network_reachable: false,
            check_duration: Duration::ZERO,
            errors: Vec::new(),
        }
    }

    /// Check if VM is healthy
    pub fn is_healthy(&self) -> bool {
        self.status == HealthStatus::Healthy
    }

    /// Check if VM is ready for use
    pub fn is_ready(&self) -> bool {
        self.boot_complete && self.network_reachable
    }
}

/// VM health monitor
pub struct HealthMonitor {
    check_interval: Duration,
    boot_timeout: Duration,
}

impl HealthMonitor {
    /// Create a new health monitor with default settings
    pub fn new() -> Self {
        Self {
            check_interval: Duration::from_secs(5),
            boot_timeout: Duration::from_secs(300), // 5 minutes
        }
    }

    /// Create a health monitor with custom settings
    pub fn with_timeouts(check_interval: Duration, boot_timeout: Duration) -> Self {
        Self {
            check_interval,
            boot_timeout,
        }
    }

    /// Perform a health check on a VM using serial console log
    pub async fn check_vm_health(&self, serial_log_content: &str, ip_address: &str) -> HealthCheck {
        use super::serial_console;

        let start = Instant::now();

        let boot_complete = serial_console::is_boot_complete(serial_log_content);
        let boot_time_ms = serial_console::parse_boot_time_ms(serial_log_content);
        let errors = serial_console::extract_errors(serial_log_content);

        // Simple network check - if we have an IP, assume reachable
        let network_reachable = !ip_address.is_empty() && ip_address != "0.0.0.0";

        let status = if boot_complete && network_reachable && errors.is_empty() {
            HealthStatus::Healthy
        } else if boot_complete && !errors.is_empty() {
            HealthStatus::Unhealthy
        } else if !boot_complete {
            HealthStatus::Booting
        } else {
            HealthStatus::Unknown
        };

        let check_duration = start.elapsed();

        HealthCheck {
            status,
            boot_complete,
            boot_time_ms,
            network_reachable,
            check_duration,
            errors,
        }
    }

    /// Wait for VM to become healthy
    pub async fn wait_for_healthy(
        &self,
        mut check_fn: impl FnMut() -> HealthCheck,
    ) -> Result<HealthCheck> {
        let start = Instant::now();

        loop {
            if start.elapsed() > self.boot_timeout {
                return Err(Error::Backend(format!(
                    "Timeout waiting for VM to become healthy ({}s)",
                    self.boot_timeout.as_secs()
                )));
            }

            let health = check_fn();

            debug!("Health check: {:?}", health.status);

            if health.is_healthy() {
                return Ok(health);
            }

            if health.status == HealthStatus::Unhealthy {
                warn!("VM is unhealthy: {:?}", health.errors);
                return Err(Error::Backend(format!(
                    "VM is unhealthy: {}",
                    health.errors.join(", ")
                )));
            }

            tokio::time::sleep(self.check_interval).await;
        }
    }

    /// Get check interval
    pub fn check_interval(&self) -> Duration {
        self.check_interval
    }

    /// Get boot timeout
    pub fn boot_timeout(&self) -> Duration {
        self.boot_timeout
    }
}

impl Default for HealthMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEALTHY_LOG: &str = r"[2025-12-27 10:23:45] [Info] BiomeOS Init Starting
[2025-12-27 10:23:49] [Success] BiomeOS Init Complete (178ms)";

    const UNHEALTHY_LOG: &str = r"[2025-12-27 10:23:45] [Info] BiomeOS Init Starting
[2025-12-27 10:23:46] [Error] Failed to mount filesystem
[2025-12-27 10:23:49] [Success] BiomeOS Init Complete (178ms)";

    const BOOTING_LOG: &str = r"[2025-12-27 10:23:45] [Info] BiomeOS Init Starting
[2025-12-27 10:23:46] [Info] Loading kernel modules...";

    #[tokio::test]
    async fn test_healthy_vm() {
        let monitor = HealthMonitor::new();
        let health = monitor.check_vm_health(HEALTHY_LOG, "10.0.0.10").await;

        assert_eq!(health.status, HealthStatus::Healthy);
        assert!(health.boot_complete);
        assert!(health.network_reachable);
        assert!(health.is_healthy());
        assert!(health.is_ready());
        assert_eq!(health.boot_time_ms, Some(178));
    }

    #[tokio::test]
    async fn test_unhealthy_vm() {
        let monitor = HealthMonitor::new();
        let health = monitor.check_vm_health(UNHEALTHY_LOG, "10.0.0.10").await;

        assert_eq!(health.status, HealthStatus::Unhealthy);
        assert!(health.boot_complete);
        assert!(!health.is_healthy());
        assert_eq!(health.errors.len(), 1);
    }

    #[tokio::test]
    async fn test_booting_vm() {
        let monitor = HealthMonitor::new();
        let health = monitor.check_vm_health(BOOTING_LOG, "10.0.0.10").await;

        assert_eq!(health.status, HealthStatus::Booting);
        assert!(!health.boot_complete);
        assert!(!health.is_ready());
    }

    #[tokio::test]
    async fn test_no_network() {
        let monitor = HealthMonitor::new();
        let health = monitor.check_vm_health(HEALTHY_LOG, "").await;

        assert!(!health.network_reachable);
        assert!(!health.is_ready());
    }

    #[test]
    fn test_health_check_creation() {
        let health = HealthCheck::new(HealthStatus::Healthy);
        assert_eq!(health.status, HealthStatus::Healthy);
        assert!(!health.boot_complete); // Defaults to false
    }

    #[test]
    fn test_monitor_timeouts() {
        let monitor = HealthMonitor::with_timeouts(Duration::from_secs(2), Duration::from_secs(60));

        assert_eq!(monitor.check_interval(), Duration::from_secs(2));
        assert_eq!(monitor.boot_timeout(), Duration::from_secs(60));
    }
}
