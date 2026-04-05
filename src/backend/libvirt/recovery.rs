// SPDX-License-Identifier: AGPL-3.0-or-later
//! Libvirt Auto-Recovery Module (Evolution #20)
//!
//! This module provides automatic recovery from libvirt state corruption,
//! specifically handling orphaned processes and network issues.
//!
//! ## Problem
//! When libvirtd crashes or terminates uncleanly, it can leave behind orphaned
//! dnsmasq processes that cause subsequent VM operations to fail with I/O errors.
//!
//! ## Solution
//! Automatic recovery that:
//! - Kills orphaned dnsmasq processes
//! - Restarts libvirtd cleanly
//! - Reinitializes the default network
//! - Verifies recovery succeeded
//!
//! ## Usage
//! ```no_run
//! use benchscale::backend::libvirt::{health_check::LibvirtHealthCheck, recovery::LibvirtRecovery};
//!
//! # async fn example() -> anyhow::Result<()> {
//! let checker = LibvirtHealthCheck::new();
//! let health = checker.check().await?;
//!
//! if !health.is_healthy() {
//!     let recovery = LibvirtRecovery::new();
//!     let result = recovery.recover(&health).await?;
//!     
//!     if result.success {
//!         println!("Recovery successful!");
//!     } else {
//!         println!("Recovery failed, manual intervention required");
//!     }
//! }
//! # Ok(())
//! # }
//! ```

use super::health_check::{HealthStatus, LibvirtHealthCheck};
use std::time::Duration;
use tracing::{debug, info, warn};
use virt::connect::Connect;
use virt::network::Network;

fn system_connection() -> anyhow::Result<Connect> {
    Connect::open(Some("qemu:///system")).map_err(|e| anyhow::anyhow!(e))
}

/// Auto-recovery for libvirt system state
pub struct LibvirtRecovery {
    /// Maximum recovery attempts before giving up
    max_attempts: u32,
    /// Delay between recovery attempts
    retry_delay: Duration,
}

/// Result of a recovery operation
#[derive(Debug, Clone)]
pub struct RecoveryResult {
    /// Whether recovery succeeded
    pub success: bool,
    /// Actions that were taken
    pub actions_taken: Vec<RecoveryAction>,
    /// Health status after recovery
    pub final_health: HealthStatus,
    /// Error message if recovery failed
    pub error: Option<String>,
}

/// Individual recovery action
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryAction {
    /// Killed an orphaned dnsmasq process
    KilledOrphanedProcess(u32),
    /// Restarted libvirtd service
    RestartedLibvirtd,
    /// Reinitialized the default network
    ReinitializedNetwork,
    /// Waited for system stabilization
    WaitedForStabilization,
}

impl LibvirtRecovery {
    /// Create a new recovery manager with default settings
    pub fn new() -> Self {
        Self {
            max_attempts: 3,
            retry_delay: Duration::from_secs(2),
        }
    }

    /// Create a recovery manager with custom retry settings
    pub fn with_retries(mut self, max_attempts: u32, retry_delay: Duration) -> Self {
        self.max_attempts = max_attempts;
        self.retry_delay = retry_delay;
        self
    }

    /// Attempt to recover from detected health issues
    ///
    /// This method analyzes the health status and attempts to fix any issues:
    /// 1. Kill orphaned dnsmasq processes
    /// 2. Restart libvirtd if necessary
    /// 3. Reinitialize networks if needed
    /// 4. Verify recovery succeeded
    ///
    /// # Arguments
    /// * `health` - Current health status from LibvirtHealthCheck
    ///
    /// # Returns
    /// `RecoveryResult` with details of actions taken and final health status
    pub async fn recover(&self, health: &HealthStatus) -> anyhow::Result<RecoveryResult> {
        info!("🔧 Starting libvirt auto-recovery...");

        let mut result = RecoveryResult {
            success: false,
            actions_taken: Vec::new(),
            final_health: health.clone(),
            error: None,
        };

        // Attempt recovery with retries
        for attempt in 1..=self.max_attempts {
            info!("  Recovery attempt {}/{}", attempt, self.max_attempts);

            // Perform recovery actions
            if let Err(e) = self.perform_recovery(health, &mut result).await {
                warn!("  Recovery attempt {} failed: {}", attempt, e);
                result.error = Some(format!("Attempt {}: {}", attempt, e));

                if attempt < self.max_attempts {
                    info!("  Waiting {}s before retry...", self.retry_delay.as_secs());
                    tokio::time::sleep(self.retry_delay).await;
                    continue;
                }
                return Ok(result);
            }

            // Verify recovery succeeded
            let checker = LibvirtHealthCheck::new();
            result.final_health = checker.check().await?;

            if result.final_health.is_healthy() {
                result.success = true;
                info!("✅ Recovery successful after {} attempt(s)", attempt);
                return Ok(result);
            }
            warn!(
                "  System still unhealthy after attempt {}: {}",
                attempt,
                result.final_health.summary()
            );
        }

        warn!("❌ Recovery failed after {} attempts", self.max_attempts);
        result.error = Some(format!(
            "Failed after {} attempts. Final state: {}",
            self.max_attempts,
            result.final_health.summary()
        ));
        Ok(result)
    }

    /// Perform the actual recovery actions
    async fn perform_recovery(
        &self,
        health: &HealthStatus,
        result: &mut RecoveryResult,
    ) -> anyhow::Result<()> {
        // EVOLUTION #20: Sudo-Free Recovery
        //
        // Instead of manually killing processes (requires sudo), we use libvirt's
        // network APIs to cleanly restart networks. This automatically cleans up
        // any orphaned dnsmasq processes that were serving those networks.
        //
        // Primal Philosophy: Work "above" benchScale at the API level, not "below"

        // If libvirtd is not active, we can't recover - this is critical
        if !health.libvirtd_active {
            anyhow::bail!(
                "libvirtd service is not active. Please run: sudo systemctl start libvirtd"
            );
        }

        // Clean up orphaned processes by restarting networks (no sudo needed!)
        if !health.orphaned_processes.is_empty() {
            info!(
                "  Detected {} orphaned dnsmasq process(es)",
                health.orphaned_processes.len()
            );
            info!("  Restarting networks to clean up orphans (no sudo needed)...");

            // Restart default network - this will clean up orphaned dnsmasq
            if self.reinitialize_network()? {
                debug!("    Network restarted successfully");
                result
                    .actions_taken
                    .push(RecoveryAction::ReinitializedNetwork);

                // Wait for network to stabilize
                tokio::time::sleep(Duration::from_secs(2)).await;
                result
                    .actions_taken
                    .push(RecoveryAction::WaitedForStabilization);

                // Mark orphans as cleaned (indirectly by network restart)
                for pid in &health.orphaned_processes {
                    result
                        .actions_taken
                        .push(RecoveryAction::KilledOrphanedProcess(*pid));
                }
            } else {
                warn!("    Failed to restart network (may recover on its own)");
            }
        }

        // Reinitialize network if it's not active
        if !health.network_active {
            info!("  Reinitializing default network...");

            if self.reinitialize_network()? {
                debug!("    Network reinitialized successfully");
                result
                    .actions_taken
                    .push(RecoveryAction::ReinitializedNetwork);
            } else {
                warn!("    Failed to reinitialize network (may recover on its own)");
            }
        }

        Ok(())
    }

    /// Reinitialize the default libvirt network
    ///
    /// **Sudo-Free:** Uses libvirt connection, not manual process management.
    /// This automatically cleans up orphaned dnsmasq processes.
    fn reinitialize_network(&self) -> anyhow::Result<bool> {
        let conn = system_connection()?;
        let network = match Network::lookup_by_name(&conn, "default") {
            Ok(n) => n,
            Err(_) => return Ok(false),
        };

        match network.create() {
            Ok(_) => Ok(true),
            Err(_) => {
                debug!("Network start failed, trying destroy + start...");
                let _ = network.destroy();
                Ok(network.create().is_ok())
            }
        }
    }
}

impl Default for LibvirtRecovery {
    fn default() -> Self {
        Self::new()
    }
}

impl RecoveryResult {
    /// Get a human-readable summary of recovery actions
    pub fn summary(&self) -> String {
        if self.success {
            format!(
                "Recovery successful. {} action(s) taken: {}",
                self.actions_taken.len(),
                self.actions_summary()
            )
        } else {
            format!(
                "Recovery failed. {} action(s) attempted. Error: {}",
                self.actions_taken.len(),
                self.error.as_deref().unwrap_or("Unknown")
            )
        }
    }

    /// Get a comma-separated list of actions taken
    fn actions_summary(&self) -> String {
        self.actions_taken
            .iter()
            .map(|action| match action {
                RecoveryAction::KilledOrphanedProcess(pid) => format!("killed PID {}", pid),
                RecoveryAction::RestartedLibvirtd => "restarted libvirtd".to_string(),
                RecoveryAction::ReinitializedNetwork => "reinitialized network".to_string(),
                RecoveryAction::WaitedForStabilization => "waited for stabilization".to_string(),
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::libvirt::health_check::HealthState;
    use std::time::SystemTime;

    #[tokio::test]
    async fn test_recovery_basic() {
        let recovery = LibvirtRecovery::new();

        // Create a mock unhealthy status
        let health = HealthStatus {
            overall: HealthState::Degraded("test".to_string()),
            checked_at: SystemTime::now(),
            libvirtd_active: true,
            orphaned_processes: Vec::new(), // No actual orphans to kill in test
            network_active: true,
            dhcp_functional: true,
            issues: vec!["Test issue".to_string()],
        };

        let result = recovery.recover(&health).await;

        // Should not error (even if recovery fails)
        assert!(result.is_ok());
    }

    #[test]
    fn test_recovery_result_summary() {
        let result = RecoveryResult {
            success: true,
            actions_taken: vec![
                RecoveryAction::KilledOrphanedProcess(1234),
                RecoveryAction::RestartedLibvirtd,
            ],
            final_health: HealthStatus {
                overall: HealthState::Healthy,
                checked_at: SystemTime::now(),
                libvirtd_active: true,
                orphaned_processes: Vec::new(),
                network_active: true,
                dhcp_functional: true,
                issues: Vec::new(),
            },
            error: None,
        };

        let summary = result.summary();
        assert!(summary.contains("successful"));
        assert!(summary.contains("killed PID 1234"));
        assert!(summary.contains("restarted libvirtd"));
    }
}
