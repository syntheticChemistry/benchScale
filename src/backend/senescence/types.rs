// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright © 2024-2025 DataScienceBioLab

//! Domain types for VM senescence monitoring.
//!
//! Extracted from the monolithic `senescence.rs` so that health status,
//! cloud-init progress, and metrics can be referenced without pulling in
//! the full monitor implementation.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::{Duration, Instant};

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

/// Cloud-init status values from `cloud-init status --format=json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloudInitStatus {
    /// Cloud-init is still running modules.
    Running,
    /// All modules finished successfully.
    Done,
    /// Cloud-init reported an error state.
    Error,
    /// Cloud-init was disabled on this instance.
    Disabled,
    /// Unrecognized status string from the guest.
    Unknown(String),
}

impl CloudInitStatus {
    pub(crate) fn from_status_str(s: &str) -> Self {
        match s {
            "running" => Self::Running,
            "done" => Self::Done,
            "error" => Self::Error,
            "disabled" => Self::Disabled,
            other => Self::Unknown(other.to_string()),
        }
    }
}

impl<'de> Deserialize<'de> for CloudInitStatus {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(Self::from_status_str(&s))
    }
}

impl fmt::Display for CloudInitStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Running => write!(f, "running"),
            Self::Done => write!(f, "done"),
            Self::Error => write!(f, "error"),
            Self::Disabled => write!(f, "disabled"),
            Self::Unknown(s) => write!(f, "{s}"),
        }
    }
}

/// Cloud-init progress information
#[derive(Debug, Clone)]
pub struct CloudInitProgress {
    /// Current status (running, done, error)
    pub status: CloudInitStatus,
    /// Detailed stage information
    pub detail: Option<String>,
    /// Any errors encountered
    pub errors: Vec<String>,
    /// Last successful check timestamp (set each poll; reserved for diagnostics)
    pub(crate) _last_check: Instant,
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
