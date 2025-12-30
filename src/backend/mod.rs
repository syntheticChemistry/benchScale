//! Backend abstraction for container runtimes

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::Result;

pub mod docker;
pub use docker::DockerBackend;

#[cfg(feature = "libvirt")]
pub mod libvirt;
#[cfg(feature = "libvirt")]
pub use libvirt::LibvirtBackend;

#[cfg(feature = "libvirt")]
pub mod ssh;

#[cfg(feature = "libvirt")]
pub mod vm_utils;

#[cfg(feature = "libvirt")]
pub mod serial_console;

#[cfg(feature = "libvirt")]
pub mod health;

#[cfg(feature = "libvirt")]
pub use health::{HealthCheck, HealthMonitor, HealthStatus};

#[cfg(feature = "libvirt")]
pub mod ip_pool;

#[cfg(feature = "libvirt")]
pub use ip_pool::IpPool;

#[cfg(feature = "libvirt")]
pub mod timeout_utils;

#[cfg(feature = "libvirt")]
pub use timeout_utils::{BackoffConfig, retry_with_backoff, wait_for_condition, wait_for_condition_backoff};

#[cfg(all(feature = "libvirt", test))]
mod vnc_display_tests;

/// Information about a running container/node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    /// Node identifier
    pub id: String,
    /// Node name
    pub name: String,
    /// Container/VM ID
    pub container_id: String,
    /// IP address
    pub ip_address: String,
    /// Network name
    pub network: String,
    /// Current status
    pub status: NodeStatus,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Node status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeStatus {
    /// Node is starting
    Starting,
    /// Node is running
    Running,
    /// Node is stopped
    Stopped,
    /// Node failed
    Failed,
}

/// Network information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInfo {
    /// Network name
    pub name: String,
    /// Network ID
    pub id: String,
    /// Subnet CIDR
    pub subnet: String,
    /// Gateway IP
    pub gateway: String,
}

/// Backend trait for container runtime abstraction
#[async_trait]
pub trait Backend: Send + Sync {
    /// Create a new network
    async fn create_network(&self, name: &str, subnet: &str) -> Result<NetworkInfo>;

    /// Delete a network
    async fn delete_network(&self, name: &str) -> Result<()>;

    /// Create a new node (container)
    async fn create_node(
        &self,
        name: &str,
        image: &str,
        network: &str,
        env: HashMap<String, String>,
    ) -> Result<NodeInfo>;

    /// Start a node
    async fn start_node(&self, node_id: &str) -> Result<()>;

    /// Stop a node
    async fn stop_node(&self, node_id: &str) -> Result<()>;

    /// Delete a node
    async fn delete_node(&self, node_id: &str) -> Result<()>;

    /// Get node information
    async fn get_node(&self, node_id: &str) -> Result<NodeInfo>;

    /// List all nodes in a network
    async fn list_nodes(&self, network: &str) -> Result<Vec<NodeInfo>>;

    /// Execute a command in a node
    async fn exec_command(&self, node_id: &str, command: Vec<String>) -> Result<ExecResult>;

    /// Copy file to node
    async fn copy_to_node(&self, node_id: &str, src_path: &str, dest_path: &str) -> Result<()>;

    /// Get logs from a node
    async fn get_logs(&self, node_id: &str) -> Result<String>;

    /// Apply network conditions to a node
    async fn apply_network_conditions(
        &self,
        node_id: &str,
        latency_ms: Option<u32>,
        packet_loss_percent: Option<f32>,
        bandwidth_kbps: Option<u32>,
    ) -> Result<()>;

    /// Check if backend is available
    async fn is_available(&self) -> Result<bool>;
}

/// Result of executing a command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecResult {
    /// Exit code
    pub exit_code: i64,
    /// Standard output
    pub stdout: String,
    /// Standard error
    pub stderr: String,
}

impl ExecResult {
    /// Check if command was successful
    pub fn success(&self) -> bool {
        self.exit_code == 0
    }
}
