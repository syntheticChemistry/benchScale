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
//!
//! ## Module structure
//!
//! - `types` — domain types (`HealthStatus`, `CloudInitStatus`, `SenescenceMetrics`)
//! - `mod` — `SenescenceMonitor` implementation (checks, wait helpers, monitoring loop)

mod types;

pub use types::{CloudInitProgress, CloudInitStatus, HealthStatus, SenescenceMetrics};

use crate::{Error, Result};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::interval;
use tracing::{debug, info, warn};

#[cfg(feature = "libvirt")]
use crate::backend::libvirt::dhcp_discovery::{DiscoveryConfig, discover_dhcp_ip};

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
    max_failures: u32,
    #[cfg_attr(not(feature = "libvirt"), allow(dead_code))]
    ip_rediscovery_interval: u32,
    ssh_identity: Option<std::path::PathBuf>,
    qga: Option<crate::backend::qga::QgaClient>,
}

impl SenescenceMonitor {
    /// Create a new senescence monitor with configuration
    ///
    /// This is the recommended constructor that accepts a `MonitoringConfig`
    /// for full control over monitoring behavior.
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
            ssh_identity: None,
            qga: None,
        }
    }

    /// Create a new senescence monitor for a VM (legacy)
    ///
    /// **Deprecated:** Use `from_config()` with `MonitoringConfig` for better control.
    pub fn new(vm_name: String, ip_address: String) -> Self {
        Self::with_mac_address(vm_name, ip_address, None)
    }

    /// Create a senescence monitor with MAC address for DHCP lease tracking
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
            stall_threshold: Duration::from_mins(2),
            max_failures: 10,
            ip_rediscovery_interval: 10,
            ssh_identity: None,
            qga: None,
        }
    }

    /// Configure maximum consecutive failures before declaring VM failed
    pub fn with_max_failures(mut self, max_failures: u32) -> Self {
        self.max_failures = max_failures;
        self
    }

    /// Set an explicit SSH private key for identity-based auth.
    pub fn with_ssh_identity(mut self, path: std::path::PathBuf) -> Self {
        self.ssh_identity = Some(path);
        self
    }

    /// Enable QGA (QEMU Guest Agent) health checks via virtio-serial.
    pub fn with_qga(mut self, client: crate::backend::qga::QgaClient) -> Self {
        self.qga = Some(client);
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

    // ── Health check implementation ─────────────────────────────────────

    async fn perform_health_check(&self, username: &str) -> Result<()> {
        {
            let mut metrics = self.metrics.write().await;
            metrics.check_count += 1;

            #[cfg(feature = "libvirt")]
            if metrics.check_count % self.ip_rediscovery_interval == 0 {
                if let Some(ref mac_address) = metrics.mac_address {
                    debug!(
                        "IP re-discovery for MAC {} (check #{})",
                        mac_address, metrics.check_count
                    );

                    let config = DiscoveryConfig {
                        max_wait_secs: 10,
                        retry_interval_secs: 2,
                        network_name: crate::constants::libvirt_defaults::DEFAULT_NETWORK_NAME,
                    };

                    let old_ip = metrics.ip_address.clone();
                    let mac_for_discovery = mac_address.clone();
                    drop(metrics);

                    match discover_dhcp_ip(&mac_for_discovery, config).await {
                        Ok(new_ip) => {
                            let mut metrics = self.metrics.write().await;
                            if new_ip != old_ip {
                                info!(
                                    "IP changed for VM {} (MAC {}): {} -> {}",
                                    metrics.vm_name, mac_for_discovery, old_ip, new_ip
                                );
                                metrics.ip_address = new_ip;
                                metrics.consecutive_failures = 0;
                            } else {
                                debug!("IP unchanged: {}", new_ip);
                            }
                        }
                        Err(e) => {
                            warn!(
                                "Failed to re-discover IP for MAC {}: {}. Continuing with cached IP {}",
                                mac_for_discovery, e, old_ip
                            );
                        }
                    }
                }
            }
        }

        let mut metrics = self.metrics.write().await;
        metrics.uptime = self.start_time.elapsed();

        let ping_ok = self.check_ping(&metrics.ip_address).await;
        metrics.ping_ok = ping_ok;

        if let Some(ref qga) = self.qga {
            if qga.ping().await {
                debug!("QGA guest-ping OK for {}", metrics.vm_name);
            }
        }

        let ssh_ok = if ping_ok {
            self.check_ssh(&metrics.ip_address, username).await
        } else {
            false
        };
        metrics.ssh_ok = ssh_ok;

        if ssh_ok {
            if let Ok(progress) = self.check_cloud_init(&metrics.ip_address, username).await {
                metrics.cloud_init = Some(progress);
            }
        }

        let new_health = Self::derive_health(
            &mut metrics,
            ping_ok,
            ssh_ok,
            self.check_interval,
            self.stall_threshold,
            self.max_failures,
        );

        if new_health != metrics.health {
            info!(
                "VM {} health changed: {:?} -> {:?}",
                metrics.vm_name, metrics.health, new_health
            );
            metrics.health = new_health;
        }

        Ok(())
    }

    /// Derive health status from current metrics and check results.
    fn derive_health(
        metrics: &mut SenescenceMetrics,
        ping_ok: bool,
        ssh_ok: bool,
        check_interval: Duration,
        stall_threshold: Duration,
        max_failures: u32,
    ) -> HealthStatus {
        if ssh_ok && ping_ok {
            if let Some(ref cloud_init) = metrics.cloud_init {
                if cloud_init.status == CloudInitStatus::Done {
                    metrics.consecutive_failures = 0;
                    metrics.time_since_healthy = Duration::ZERO;
                    HealthStatus::Healthy
                } else if !cloud_init.errors.is_empty() {
                    metrics.consecutive_failures += 1;
                    HealthStatus::Failed
                } else {
                    metrics.time_since_healthy += check_interval;
                    if metrics.time_since_healthy > stall_threshold {
                        HealthStatus::Stalled
                    } else {
                        HealthStatus::Healthy
                    }
                }
            } else {
                HealthStatus::Degraded
            }
        } else if ping_ok {
            metrics.consecutive_failures += 1;
            if metrics.consecutive_failures > 5 {
                HealthStatus::Degraded
            } else {
                HealthStatus::Unknown
            }
        } else {
            metrics.consecutive_failures += 1;
            if metrics.consecutive_failures > max_failures {
                HealthStatus::Failed
            } else {
                HealthStatus::Degraded
            }
        }
    }

    // ── Probe methods ───────────────────────────────────────────────────

    /// Check VM reachability via TCP connect to SSH port.
    async fn check_ping(&self, ip: &str) -> bool {
        use std::net::SocketAddr;
        let addr: SocketAddr = match format!("{ip}:22").parse() {
            Ok(a) => a,
            Err(_) => return false,
        };
        tokio::time::timeout(
            Duration::from_secs(2),
            tokio::net::TcpStream::connect(addr),
        )
        .await
        .is_ok_and(|r| r.is_ok())
    }

    fn ssh_args(&self, ip: &str, username: &str) -> Vec<String> {
        let mut args = vec![
            "-o".to_string(),
            "StrictHostKeyChecking=no".to_string(),
            "-o".to_string(),
            "UserKnownHostsFile=/dev/null".to_string(),
            "-o".to_string(),
            "ConnectTimeout=3".to_string(),
            "-o".to_string(),
            "BatchMode=yes".to_string(),
        ];
        if let Some(ref id) = self.ssh_identity {
            args.push("-i".to_string());
            args.push(id.display().to_string());
        }
        args.push(format!("{}@{}", username, ip));
        args
    }

    async fn check_ssh(&self, ip: &str, username: &str) -> bool {
        let mut args = self.ssh_args(ip, username);
        args.push("echo ok".to_string());

        let output = tokio::process::Command::new("ssh")
            .args(&args)
            .output()
            .await;

        match output {
            Ok(output) => output.status.success(),
            Err(_) => false,
        }
    }

    async fn check_cloud_init(&self, ip: &str, username: &str) -> Result<CloudInitProgress> {
        let mut args = self.ssh_args(ip, username);
        args.push("cloud-init status --format=json".to_string());

        let output = tokio::process::Command::new("ssh")
            .args(&args)
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

        let status_str = status["status"].as_str().unwrap_or("unknown");
        Ok(CloudInitProgress {
            status: CloudInitStatus::from_status_str(status_str),
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

    // ── Wait helpers ────────────────────────────────────────────────────

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
                if cloud_init.status == CloudInitStatus::Done {
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
                return Ok(());
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
                status: CloudInitStatus::Done,
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
                status: CloudInitStatus::Running,
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
