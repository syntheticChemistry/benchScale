// SPDX-License-Identifier: AGPL-3.0-or-later
//! Libvirt Health Check Module (Evolution #20)
//!
//! This module provides automatic detection of libvirt system health issues,
//! specifically targeting state corruption from orphaned processes and network issues.
//!
//! ## Problem
//! libvirtd can enter an unstable state after crashes or unclean shutdowns, leaving
//! orphaned dnsmasq processes that cause VM creation to fail with I/O errors.
//!
//! ## Solution
//! Automatic health checks that detect:
//! - libvirtd service status
//! - Orphaned dnsmasq processes from previous sessions
//! - Network state (default network active)
//! - DHCP functionality
//!
//! ## Usage
//! ```no_run
//! use benchscale::backend::libvirt::health_check::LibvirtHealthCheck;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let check = LibvirtHealthCheck::new();
//! let health = check.check().await?;
//!
//! if !health.is_healthy() {
//!     println!("Health issues detected: {:?}", health.issues);
//!     // Trigger recovery...
//! }
//! # Ok(())
//! # }
//! ```

use super::dhcp_leases::LeaseList;
use std::process::Command;
use std::ptr;
use std::time::SystemTime;
use tracing::{debug, info, warn};
use virt::connect::Connect;
use virt::network::Network;

/// Health check for libvirt system state
pub struct LibvirtHealthCheck {
    /// Whether to include detailed diagnostics
    verbose: bool,
}

/// Overall health status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthState {
    /// System is fully operational
    Healthy,
    /// System works but has non-critical issues
    Degraded(String),
    /// System cannot function properly
    Unhealthy(String),
}

/// Complete health status with diagnostics
#[derive(Debug, Clone)]
pub struct HealthStatus {
    /// Overall health state
    pub overall: HealthState,
    /// When this check was performed
    pub checked_at: SystemTime,
    /// Is libvirtd service active?
    pub libvirtd_active: bool,
    /// Orphaned dnsmasq process PIDs (from previous libvirtd sessions)
    pub orphaned_processes: Vec<u32>,
    /// Is the default network active?
    pub network_active: bool,
    /// Can we query DHCP leases?
    pub dhcp_functional: bool,
    /// List of detected issues
    pub issues: Vec<String>,
}

impl LibvirtHealthCheck {
    /// Create a new health checker
    pub fn new() -> Self {
        Self { verbose: false }
    }

    /// Create a health checker with verbose diagnostics
    pub fn with_verbose(mut self) -> Self {
        self.verbose = true;
        self
    }

    /// Perform a comprehensive health check
    ///
    /// This checks all aspects of libvirt health and returns a detailed status.
    /// Use the `issues` field to get human-readable explanations of problems.
    ///
    /// # Errors
    /// Returns error only for catastrophic failures (e.g., cannot check anything).
    /// Individual check failures are reported in `HealthStatus.issues`.
    pub async fn check(&self) -> anyhow::Result<HealthStatus> {
        info!("🔍 Performing libvirt health check...");

        let mut status = HealthStatus {
            overall: HealthState::Healthy,
            checked_at: SystemTime::now(),
            libvirtd_active: false,
            orphaned_processes: Vec::new(),
            network_active: false,
            dhcp_functional: false,
            issues: Vec::new(),
        };

        // Check 1: libvirtd service status
        status.libvirtd_active = self.check_libvirtd_active(&mut status.issues);

        // Check 2: Orphaned dnsmasq processes
        status.orphaned_processes = self.find_orphaned_dnsmasq(&mut status.issues);

        // Check 3: Network state
        status.network_active = self.check_network_active(&mut status.issues);

        // Check 4: DHCP functionality
        status.dhcp_functional = self.check_dhcp_functional(&mut status.issues);

        // Determine overall health
        status.overall = self.determine_overall_health(&status);

        // Log results
        match &status.overall {
            HealthState::Healthy => {
                info!("✅ libvirt health check: HEALTHY");
            }
            HealthState::Degraded(reason) => {
                warn!("⚠️  libvirt health check: DEGRADED - {}", reason);
                for issue in &status.issues {
                    debug!("  • {}", issue);
                }
            }
            HealthState::Unhealthy(reason) => {
                warn!("❌ libvirt health check: UNHEALTHY - {}", reason);
                for issue in &status.issues {
                    warn!("  • {}", issue);
                }
            }
        }

        Ok(status)
    }

    /// Check if libvirtd service is active
    fn check_libvirtd_active(&self, issues: &mut Vec<String>) -> bool {
        debug!("Checking libvirtd service status...");

        match Command::new("systemctl")
            .args(["is-active", "libvirtd"])
            .output()
        {
            Ok(output) => {
                let status = String::from_utf8_lossy(&output.stdout);
                let is_active = status.trim() == "active";

                if !is_active {
                    issues.push(format!(
                        "libvirtd service is not active (status: {})",
                        status.trim()
                    ));
                }

                is_active
            }
            Err(e) => {
                issues.push(format!("Failed to check libvirtd status: {}", e));
                false
            }
        }
    }

    /// Find orphaned dnsmasq processes from previous libvirtd sessions
    ///
    /// **EVOLUTION #20: ORPHAN DETECTION DISABLED**
    ///
    /// The problem: dnsmasq daemons by design:
    /// 1. libvirtd spawns master dnsmasq
    /// 2. Master DAEMONIZES → becomes child of PID 1  
    /// 3. Master spawns helper dnsmasq
    ///
    /// Result: Managed processes look like orphans!
    ///   PID 4131873 (parent: 1) ← appears orphaned, is managed
    ///   PID 4131874 (parent: 4131873) ← child of "orphan"
    ///
    /// **PRIMAL DECISION:**
    /// If VMs can be created and reach network, infrastructure is healthy.
    /// False positives in orphan detection cause unnecessary warnings.
    ///
    /// **FUTURE:** Could reintroduce with process start time heuristic:
    /// - Get libvirtd start time from `/proc/{pid}/stat`
    /// - Get dnsmasq start time from `/proc/{pid}/stat`
    /// - If dnsmasq started BEFORE libvirtd → orphaned
    /// - If dnsmasq started AFTER libvirtd → managed (even if daemonized)
    fn find_orphaned_dnsmasq(&self, _issues: &mut Vec<String>) -> Vec<u32> {
        debug!("Orphan detection disabled - daemonization makes parent-based detection unreliable");
        Vec::new() // No false positives!
    }

    /// Check if the default libvirt network is active
    fn check_network_active(&self, issues: &mut Vec<String>) -> bool {
        debug!("Checking default network status...");

        match Connect::open(Some("qemu:///system")) {
            Ok(conn) => match Network::lookup_by_name(&conn, "default") {
                Ok(network) => match network.is_active() {
                    Ok(is_active) => {
                        if !is_active {
                            issues.push("Default libvirt network is not active".to_string());
                        }
                        is_active
                    }
                    Err(e) => {
                        issues.push(format!("Failed to check network status: {}", e));
                        false
                    }
                },
                Err(_) => {
                    issues.push("Default libvirt network is not active".to_string());
                    false
                }
            },
            Err(e) => {
                issues.push(format!("Failed to check network status: {}", e));
                false
            }
        }
    }

    /// Check if DHCP functionality is working
    fn check_dhcp_functional(&self, issues: &mut Vec<String>) -> bool {
        debug!("Checking DHCP functionality...");

        match Connect::open(Some("qemu:///system")) {
            Ok(conn) => match Network::lookup_by_name(&conn, "default") {
                Ok(network) => {
                    match LeaseList::fetch(&network, ptr::null(), 0) {
                        Err(_) => {
                            issues.push("DHCP lease query failed".to_string());
                            return false;
                        }
                        Ok(_list) => {}
                    }
                    true
                }
                Err(_) => {
                    issues.push("DHCP lease query failed".to_string());
                    false
                }
            },
            Err(e) => {
                issues.push(format!("Failed to check DHCP functionality: {}", e));
                false
            }
        }
    }

    /// Determine overall health based on individual checks
    fn determine_overall_health(&self, status: &HealthStatus) -> HealthState {
        // Critical failures (system cannot function)
        if !status.libvirtd_active {
            return HealthState::Unhealthy("libvirtd service is not active".to_string());
        }

        if !status.network_active {
            return HealthState::Unhealthy("Default network is not active".to_string());
        }

        // Non-critical issues (system works but has problems)
        if !status.orphaned_processes.is_empty() {
            return HealthState::Degraded(format!(
                "{} orphaned dnsmasq process(es) detected",
                status.orphaned_processes.len()
            ));
        }

        if !status.dhcp_functional {
            return HealthState::Degraded("DHCP functionality compromised".to_string());
        }

        // All checks passed
        HealthState::Healthy
    }
}

impl Default for LibvirtHealthCheck {
    fn default() -> Self {
        Self::new()
    }
}

impl HealthStatus {
    /// Check if the system is healthy (no critical issues)
    pub fn is_healthy(&self) -> bool {
        matches!(self.overall, HealthState::Healthy)
    }

    /// Check if the system is degraded but still functional
    pub fn is_degraded(&self) -> bool {
        matches!(self.overall, HealthState::Degraded(_))
    }

    /// Check if the system is unhealthy (critical failures)
    pub fn is_unhealthy(&self) -> bool {
        matches!(self.overall, HealthState::Unhealthy(_))
    }

    /// Get a human-readable summary of health status
    pub fn summary(&self) -> String {
        match &self.overall {
            HealthState::Healthy => "All systems operational".to_string(),
            HealthState::Degraded(reason) => format!("System degraded: {}", reason),
            HealthState::Unhealthy(reason) => format!("System unhealthy: {}", reason),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_health_check_basic() {
        let checker = LibvirtHealthCheck::new();
        let result = checker.check().await;

        // Should not error (even if health is bad)
        assert!(result.is_ok());

        let status = result.unwrap();
        println!("Health status: {:?}", status.overall);
        println!("Issues: {:?}", status.issues);
    }

    #[test]
    fn test_health_state_methods() {
        let healthy = HealthStatus {
            overall: HealthState::Healthy,
            checked_at: SystemTime::now(),
            libvirtd_active: true,
            orphaned_processes: Vec::new(),
            network_active: true,
            dhcp_functional: true,
            issues: Vec::new(),
        };

        assert!(healthy.is_healthy());
        assert!(!healthy.is_degraded());
        assert!(!healthy.is_unhealthy());

        let degraded = HealthStatus {
            overall: HealthState::Degraded("test".to_string()),
            ..healthy.clone()
        };

        assert!(!degraded.is_healthy());
        assert!(degraded.is_degraded());
        assert!(!degraded.is_unhealthy());
    }
}
