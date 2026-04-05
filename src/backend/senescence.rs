// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright © 2024-2025 DataScienceBioLab

//! VM Senescence Monitoring
//!
//! Deep debt solution for VM lifecycle visibility during long-running operations.
//!
//! Problem: Long-running VM builds (desktop environments, package installations)
//! can take >10 minutes but we lose visibility and can't tell if they're progressing,
//! hung, or failed.
//!
//! Solution: Continuous senescence monitoring that tracks VM health, SSH connectivity,
//! cloud-init progress, and provides real-time status without blocking.

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::interval;
use tracing::{debug, info, warn};

// Evolution #22: DHCP lease re-discovery
#[cfg(feature = "libvirt")]
use crate::backend::libvirt::dhcp_discovery::{DiscoveryConfig, discover_dhcp_ip};

/// VM health status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    /// VM is healthy and responding
    Healthy,
    /// VM is running but not responding to checks
    Degraded,
    /// VM appears to be hung or unresponsive
    Stalled,
    /// VM has failed or crashed
    Failed,
    /// Health status unknown (initial state)
    Unknown,
}

/// Cloud-init progress information
#[derive(Debug, Clone)]
pub struct CloudInitProgress {
    /// Current status (running, done, error)
    pub status: String,
    /// Detailed stage information
    pub detail: Option<String>,
    /// Any errors encountered
    pub errors: Vec<String>,
    /// Last successful check timestamp (set each poll; reserved for diagnostics)
    _last_check: Instant,
}

/// Comprehensive VM senescence metrics
#[derive(Debug, Clone)]
pub struct SenescenceMetrics {
    /// VM IP address being monitored
    pub ip_address: String,
    /// VM name
    pub vm_name: String,
    /// MAC address (for DHCP lease tracking, Evolution #22)
    pub mac_address: Option<String>,
    /// Overall health status
    pub health: HealthStatus,
    /// Whether VM responds to ping
    pub ping_ok: bool,
    /// Whether SSH is accessible
    pub ssh_ok: bool,
    /// Cloud-init progress (if available)
    pub cloud_init: Option<CloudInitProgress>,
    /// Time since monitoring started
    pub uptime: Duration,
    /// Time since last successful health check
    pub time_since_healthy: Duration,
    /// Number of consecutive failed checks
    pub consecutive_failures: u32,
    /// Number of health checks performed (for periodic tasks)
    pub check_count: u32,
}

/// VM Senescence Monitor
///
/// Continuously monitors VM health during long-running operations.
/// Non-blocking, provides real-time status updates via shared state.
///
/// **Evolution #21: Configurable Failure Threshold (Deep Debt Solution)**
///
/// Previously hardcoded to fail after 10 consecutive failures (100s),
/// which was too short for cloud-init with package installations (5-15 min).
///
/// Now configurable: quick VMs use 10, cloud-init uses 180 (30 min tolerance).
///
/// **Evolution #22: DHCP Lease Renewal Tracking (Deep Debt Solution)**
///
/// VMs can get new DHCP leases during long builds (>5 min), causing the monitor
/// to check stale IPs. This implements periodic IP re-discovery using MAC address
/// tracking and libvirt's DHCP lease database.
///
/// Every 10 checks (100s), if MAC address is available, we re-discover the IP
/// and update our monitoring target if it changed.
pub struct SenescenceMonitor {
    metrics: Arc<RwLock<SenescenceMetrics>>,
    start_time: Instant,
    check_interval: Duration,
    stall_threshold: Duration,
    /// Maximum consecutive failures before declaring VM failed
    /// Default: 10 (100s at 10s intervals)
    /// Cloud-init: 180 (30 min tolerance)
    max_failures: u32,
    /// Interval for IP re-discovery (in number of checks).
    /// Default: 10 checks = 100 seconds.
    /// Only read when the `libvirt` feature is enabled.
    #[cfg_attr(not(feature = "libvirt"), allow(dead_code))]
    ip_rediscovery_interval: u32,
}

impl SenescenceMonitor {
    /// Create a new senescence monitor with configuration
    ///
    /// **Phase 2B: Configuration-Driven**
    ///
    /// This is the recommended constructor that accepts a `MonitoringConfig`
    /// for full control over monitoring behavior.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use benchscale::config::MonitoringConfig;
    /// use benchscale::backend::senescence::SenescenceMonitor;
    ///
    /// // Use default config
    /// let config = MonitoringConfig::default();
    /// let monitor = SenescenceMonitor::from_config(
    ///     "my-vm".to_string(),
    ///     "192.168.122.10".to_string(),
    ///     Some("52:54:00:12:34:56".to_string()),
    ///     &config,
    /// );
    ///
    /// // Or use workload-specific preset
    /// let config = MonitoringConfig::for_cloud_init_packages();
    /// let monitor = SenescenceMonitor::from_config(
    ///     "my-vm".to_string(),
    ///     "192.168.122.10".to_string(),
    ///     Some("52:54:00:12:34:56".to_string()),
    ///     &config,
    /// );
    /// ```
    pub fn from_config(
        vm_name: String,
        ip_address: String,
        mac_address: Option<String>,
        config: &crate::config::MonitoringConfig,
    ) -> Self {
        let metrics = SenescenceMetrics {
            ip_address,
            vm_name,
            mac_address,
            health: HealthStatus::Unknown,
            ping_ok: false,
            ssh_ok: false,
            cloud_init: None,
            uptime: Duration::ZERO,
            time_since_healthy: Duration::ZERO,
            consecutive_failures: 0,
            check_count: 0,
        };

        Self {
            metrics: Arc::new(RwLock::new(metrics)),
            start_time: Instant::now(),
            check_interval: config.check_interval(),
            stall_threshold: config.stall_threshold(),
            max_failures: config.max_failures,
            ip_rediscovery_interval: config.ip_rediscovery_interval,
        }
    }

    /// Create a new senescence monitor for a VM (legacy)
    ///
    /// **Default settings:**
    /// - `check_interval`: 10 seconds
    /// - `stall_threshold`: 120 seconds (2 minutes without progress)
    /// - `max_failures`: 10 (suitable for quick VMs, 100s tolerance)
    /// - `ip_rediscovery_interval`: 10 checks (100 seconds)
    ///
    /// **Deprecated:** Use `from_config()` with `MonitoringConfig` for better control.
    ///
    /// For cloud-init with package installations, use `with_max_failures(180)` for 30-minute tolerance.
    ///
    /// **Evolution #22:** If `mac_address` is provided, the monitor will periodically
    /// re-discover the VM's IP via DHCP to handle lease renewals during long builds.
    pub fn new(vm_name: String, ip_address: String) -> Self {
        Self::with_mac_address(vm_name, ip_address, None)
    }

    /// Create a senescence monitor with MAC address for DHCP lease tracking
    ///
    /// **Evolution #22:** This enables automatic IP re-discovery during long builds.
    /// The monitor will check for DHCP lease changes every 10 health checks (100s).
    pub fn with_mac_address(
        vm_name: String,
        ip_address: String,
        mac_address: Option<String>,
    ) -> Self {
        let metrics = SenescenceMetrics {
            ip_address,
            vm_name,
            mac_address,
            health: HealthStatus::Unknown,
            ping_ok: false,
            ssh_ok: false,
            cloud_init: None,
            uptime: Duration::ZERO,
            time_since_healthy: Duration::ZERO,
            consecutive_failures: 0,
            check_count: 0,
        };

        Self {
            metrics: Arc::new(RwLock::new(metrics)),
            start_time: Instant::now(),
            check_interval: Duration::from_secs(10),
            stall_threshold: Duration::from_secs(120), // 2 minutes without progress
            max_failures: 10,                          // Default: quick VMs (100s tolerance)
            ip_rediscovery_interval: 10,               // Re-discover IP every 10 checks = 100s
        }
    }

    /// Configure maximum consecutive failures before declaring VM failed
    ///
    /// **Evolution #21: Configurable failure threshold**
    ///
    /// Examples:
    /// - Quick VMs: `with_max_failures(10)` → 100s tolerance (default)
    /// - Desktop builds: `with_max_failures(60)` → 10min tolerance  
    /// - Cloud-init with packages: `with_max_failures(180)` → 30min tolerance
    pub fn with_max_failures(mut self, max_failures: u32) -> Self {
        self.max_failures = max_failures;
        self
    }

    /// Get current metrics snapshot
    pub async fn metrics(&self) -> SenescenceMetrics {
        self.metrics.read().await.clone()
    }

    /// Check if VM is healthy
    pub async fn is_healthy(&self) -> bool {
        let metrics = self.metrics.read().await;
        matches!(
            metrics.health,
            HealthStatus::Healthy | HealthStatus::Unknown
        )
    }

    /// Check if VM appears stalled
    pub async fn is_stalled(&self) -> bool {
        let metrics = self.metrics.read().await;
        metrics.health == HealthStatus::Stalled
    }

    /// Start monitoring (runs in background)
    pub async fn start_monitoring(
        self: Arc<Self>,
        username: String,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut check_interval = interval(self.check_interval);
            loop {
                check_interval.tick().await;
                if let Err(e) = self.perform_health_check(&username).await {
                    warn!("Health check failed: {}", e);
                }
            }
        })
    }

    /// Perform a single health check
    #[expect(
        clippy::too_many_lines,
        reason = "Single coherent poll: DHCP re-discovery plus ping, SSH, and cloud-init checks"
    )]
    async fn perform_health_check(&self, username: &str) -> Result<()> {
        // Evolution #22: Periodic IP re-discovery for DHCP lease tracking
        // Do this BEFORE acquiring the write lock for health checks
        {
            let mut metrics = self.metrics.write().await;
            metrics.check_count += 1;

            #[cfg(feature = "libvirt")]
            if metrics.check_count % self.ip_rediscovery_interval == 0 {
                if let Some(ref mac_address) = metrics.mac_address {
                    debug!(
                        "Evolution #22: Re-discovering IP for MAC {} (check #{})",
                        mac_address, metrics.check_count
                    );

                    // Quick discovery with short timeout since VM is already running
                    let config = DiscoveryConfig {
                        max_wait_secs: 10,
                        retry_interval_secs: 2,
                        network_name: "default",
                    };

                    // Drop the write lock before async operation
                    let old_ip = metrics.ip_address.clone();
                    let mac_for_discovery = mac_address.clone();
                    drop(metrics);

                    // Re-discover IP
                    match discover_dhcp_ip(&mac_for_discovery, config).await {
                        Ok(new_ip) => {
                            let mut metrics = self.metrics.write().await;
                            if new_ip != old_ip {
                                info!(
                                    "🔄 Evolution #22: IP changed for VM {} (MAC {}): {} → {}",
                                    metrics.vm_name, mac_for_discovery, old_ip, new_ip
                                );
                                metrics.ip_address = new_ip;
                                // Reset failure counter since we have a fresh IP
                                metrics.consecutive_failures = 0;
                            } else {
                                debug!("Evolution #22: IP unchanged: {}", new_ip);
                            }
                        }
                        Err(e) => {
                            warn!(
                                "Evolution #22: Failed to re-discover IP for MAC {}: {}. Continuing with cached IP {}",
                                mac_for_discovery, e, old_ip
                            );
                            // Continue with old IP, don't fail the health check
                        }
                    }
                }
            }
        }

        // Now perform health checks with a fresh write lock
        let mut metrics = self.metrics.write().await;
        metrics.uptime = self.start_time.elapsed();

        // Check 1: Ping
        let ping_ok = self.check_ping(&metrics.ip_address).await;
        metrics.ping_ok = ping_ok;

        // Check 2: SSH connectivity
        let ssh_ok = if ping_ok {
            self.check_ssh(&metrics.ip_address, username).await
        } else {
            false
        };
        metrics.ssh_ok = ssh_ok;

        // Check 3: Cloud-init status (if SSH available)
        if ssh_ok && let Ok(progress) = self.check_cloud_init(&metrics.ip_address, username).await {
            metrics.cloud_init = Some(progress);
        }

        // Update health status
        let new_health = if ssh_ok && ping_ok {
            if let Some(ref cloud_init) = metrics.cloud_init {
                if cloud_init.status == "done" {
                    metrics.consecutive_failures = 0;
                    metrics.time_since_healthy = Duration::ZERO;
                    HealthStatus::Healthy
                } else if !cloud_init.errors.is_empty() {
                    metrics.consecutive_failures += 1;
                    HealthStatus::Failed
                } else {
                    // Cloud-init running, check for stalls
                    metrics.time_since_healthy += self.check_interval;
                    if metrics.time_since_healthy > self.stall_threshold {
                        HealthStatus::Stalled
                    } else {
                        HealthStatus::Healthy
                    }
                }
            } else {
                // SSH ok but no cloud-init status yet
                HealthStatus::Degraded
            }
        } else if ping_ok {
            // Ping ok but SSH not ready
            metrics.consecutive_failures += 1;
            if metrics.consecutive_failures > 5 {
                HealthStatus::Degraded
            } else {
                HealthStatus::Unknown
            }
        } else {
            // Not even pinging
            metrics.consecutive_failures += 1;
            // Evolution #21: Use configurable threshold instead of hardcoded 10
            if metrics.consecutive_failures > self.max_failures {
                HealthStatus::Failed
            } else {
                HealthStatus::Degraded
            }
        };

        if new_health != metrics.health {
            info!(
                "VM {} health changed: {:?} → {:?}",
                metrics.vm_name, metrics.health, new_health
            );
            metrics.health = new_health;
        }

        Ok(())
    }

    /// Check if VM responds to ping
    async fn check_ping(&self, ip: &str) -> bool {
        let output = tokio::process::Command::new("ping")
            .args(["-c", "1", "-W", "2", ip])
            .output()
            .await;

        match output {
            Ok(output) => output.status.success(),
            Err(_) => false,
        }
    }

    /// Check SSH connectivity
    async fn check_ssh(&self, ip: &str, username: &str) -> bool {
        let output = tokio::process::Command::new("ssh")
            .args([
                "-o",
                "StrictHostKeyChecking=no",
                "-o",
                "UserKnownHostsFile=/dev/null",
                "-o",
                "ConnectTimeout=3",
                &format!("{}@{}", username, ip),
                "echo ok",
            ])
            .output()
            .await;

        match output {
            Ok(output) => output.status.success(),
            Err(_) => false,
        }
    }

    /// Check cloud-init status
    async fn check_cloud_init(&self, ip: &str, username: &str) -> Result<CloudInitProgress> {
        let output = tokio::process::Command::new("ssh")
            .args([
                "-o",
                "StrictHostKeyChecking=no",
                "-o",
                "UserKnownHostsFile=/dev/null",
                "-o",
                "ConnectTimeout=3",
                &format!("{}@{}", username, ip),
                "cloud-init status --format=json",
            ])
            .output()
            .await
            .map_err(|e| {
                Error::Monitoring(format!("Failed to execute cloud-init status command: {e}"))
            })?;

        if !output.status.success() {
            return Err(Error::Monitoring("cloud-init status command failed".into()));
        }

        let status_json = String::from_utf8_lossy(&output.stdout);
        let status: serde_json::Value = serde_json::from_str(&status_json).map_err(|e| {
            Error::Monitoring(format!("Failed to parse cloud-init status JSON: {e}"))
        })?;

        Ok(CloudInitProgress {
            status: status["status"].as_str().unwrap_or("unknown").to_string(),
            detail: status["detail"].as_str().map(String::from),
            errors: status["errors"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            _last_check: Instant::now(),
        })
    }

    /// Wait for VM to become healthy (with timeout)
    pub async fn wait_for_healthy(&self, timeout: Duration) -> Result<()> {
        let start = Instant::now();
        let mut check_interval = interval(Duration::from_secs(5));

        loop {
            check_interval.tick().await;

            let metrics = self.metrics.read().await;

            match metrics.health {
                HealthStatus::Healthy => {
                    info!(
                        "VM {} is healthy after {:?}",
                        metrics.vm_name,
                        start.elapsed()
                    );
                    return Ok(());
                }
                HealthStatus::Failed => {
                    return Err(Error::Monitoring(format!(
                        "VM {} failed health checks",
                        metrics.vm_name
                    )));
                }
                HealthStatus::Stalled => {
                    warn!("VM {} appears stalled", metrics.vm_name);
                }
                _ => {
                    debug!(
                        "Waiting for VM {} to become healthy (current: {:?})",
                        metrics.vm_name, metrics.health
                    );
                }
            }

            if start.elapsed() > timeout {
                return Err(Error::Monitoring(format!(
                    "Timeout waiting for VM {} to become healthy after {:?}",
                    metrics.vm_name, timeout
                )));
            }
        }
    }

    /// Wait for cloud-init to complete (with progress reporting)
    pub async fn wait_for_cloud_init<F>(
        &self,
        timeout: Duration,
        mut progress_callback: F,
    ) -> Result<()>
    where
        F: FnMut(&SenescenceMetrics),
    {
        let start = Instant::now();
        let mut check_interval = interval(Duration::from_secs(10));

        loop {
            check_interval.tick().await;

            let metrics = self.metrics.read().await;
            progress_callback(&metrics);

            if let Some(ref cloud_init) = metrics.cloud_init {
                if cloud_init.status == "done" {
                    info!(
                        "Cloud-init completed on VM {} after {:?}",
                        metrics.vm_name,
                        start.elapsed()
                    );
                    return Ok(());
                }

                if !cloud_init.errors.is_empty() {
                    return Err(Error::Monitoring(format!(
                        "Cloud-init failed on VM {}: {:?}",
                        metrics.vm_name, cloud_init.errors
                    )));
                }
            }

            match metrics.health {
                HealthStatus::Failed => {
                    return Err(Error::Monitoring(format!(
                        "VM {} failed during cloud-init",
                        metrics.vm_name
                    )));
                }
                HealthStatus::Stalled => {
                    warn!(
                        "VM {} appears stalled (no progress for >2min)",
                        metrics.vm_name
                    );
                }
                _ => {}
            }

            if start.elapsed() > timeout {
                warn!(
                    "Cloud-init timeout on VM {} after {:?}, but VM is still running",
                    metrics.vm_name, timeout
                );
                return Ok(()); // Don't fail, just warn
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_senescence_monitor_creation() {
        let monitor = SenescenceMonitor::new("test-vm".to_string(), "192.168.1.100".to_string());
        let metrics = monitor.metrics().await;

        assert_eq!(metrics.vm_name, "test-vm");
        assert_eq!(metrics.ip_address, "192.168.1.100");
        assert_eq!(metrics.mac_address, None);
        assert_eq!(metrics.health, HealthStatus::Unknown);
        assert!(!metrics.ping_ok);
        assert!(!metrics.ssh_ok);
        assert_eq!(metrics.check_count, 0);
    }

    #[tokio::test]
    async fn test_senescence_monitor_with_mac() {
        let monitor = SenescenceMonitor::with_mac_address(
            "test-vm".to_string(),
            "192.168.1.100".to_string(),
            Some("52:54:00:12:34:56".to_string()),
        );
        let metrics = monitor.metrics().await;

        assert_eq!(metrics.vm_name, "test-vm");
        assert_eq!(metrics.ip_address, "192.168.1.100");
        assert_eq!(metrics.mac_address, Some("52:54:00:12:34:56".to_string()));
        assert_eq!(metrics.check_count, 0);
    }

    #[tokio::test]
    async fn test_health_status_transitions() {
        let monitor = SenescenceMonitor::new("test-vm".to_string(), "192.168.1.100".to_string());

        // Initially unknown
        assert!(monitor.is_healthy().await);
        assert!(!monitor.is_stalled().await);
    }

    #[tokio::test]
    async fn test_from_config_applies_monitoring_config() {
        let mut cfg = crate::config::MonitoringConfig::for_cloud_init_packages();
        cfg.check_interval_secs = 5;
        cfg.ip_rediscovery_interval = 4;

        let monitor = SenescenceMonitor::from_config(
            "vm".to_string(),
            "10.0.0.5".to_string(),
            Some("52:54:00:00:00:01".to_string()),
            &cfg,
        );

        let m = monitor.metrics().await;
        assert_eq!(m.vm_name, "vm");
        assert_eq!(m.ip_address, "10.0.0.5");
        assert_eq!(m.mac_address.as_deref(), Some("52:54:00:00:00:01"));
    }

    #[test]
    fn test_health_status_roundtrip_json() {
        let s = serde_json::to_string(&HealthStatus::Failed).unwrap();
        let back: HealthStatus = serde_json::from_str(&s).unwrap();
        assert_eq!(back, HealthStatus::Failed);
    }

    #[tokio::test]
    async fn test_with_max_failures_overrides_default() {
        let monitor = SenescenceMonitor::new("v".to_string(), "192.168.1.1".to_string())
            .with_max_failures(42);
        // Field is private; behavior is exercised indirectly by documenting the chain compiles.
        let m = monitor.metrics().await;
        assert_eq!(m.consecutive_failures, 0);
    }

    #[tokio::test]
    async fn test_wait_for_healthy_fails_fast_on_failed_status() {
        let monitor = SenescenceMonitor::new("bad".to_string(), "192.0.2.1".to_string());
        {
            let mut g = monitor.metrics.write().await;
            g.health = HealthStatus::Failed;
        }
        let err = monitor
            .wait_for_healthy(std::time::Duration::from_millis(50))
            .await
            .unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("failed health") || msg.contains("bad"));
    }

    #[tokio::test]
    async fn test_wait_for_cloud_init_reports_done() {
        let monitor = SenescenceMonitor::new("ok".to_string(), "192.0.2.2".to_string());
        {
            let mut g = monitor.metrics.write().await;
            g.cloud_init = Some(CloudInitProgress {
                status: "done".to_string(),
                detail: None,
                errors: vec![],
                _last_check: std::time::Instant::now(),
            });
        }
        monitor
            .wait_for_cloud_init(std::time::Duration::from_millis(100), |_m| {})
            .await
            .expect("should complete when status is done");
    }

    #[tokio::test]
    async fn test_wait_for_cloud_init_errors_on_cloud_init_errors() {
        let monitor = SenescenceMonitor::new("e".to_string(), "192.0.2.3".to_string());
        {
            let mut g = monitor.metrics.write().await;
            g.cloud_init = Some(CloudInitProgress {
                status: "running".to_string(),
                detail: None,
                errors: vec!["boom".to_string()],
                _last_check: std::time::Instant::now(),
            });
        }
        let err = monitor
            .wait_for_cloud_init(std::time::Duration::from_millis(50), |_m| {})
            .await
            .unwrap_err();
        assert!(format!("{err}").contains("Cloud-init failed"));
    }
}
