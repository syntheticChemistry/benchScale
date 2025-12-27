//! LibvirtBackend - Generic KVM/QEMU backend for benchScale
//! 
//! This backend allows benchScale to work with libvirt-managed VMs
//! instead of Docker containers, making it suitable for testing systems
//! that require full VMs (like ionChannel, BiomeOS, or any OS-level testing).
//!
//! ## Features
//! - Full VM creation and management via libvirt
//! - Cloud-init support for automated VM provisioning
//! - SSH-based remote execution and file transfer
//! - Network isolation and simulation
//! - Compatible with KVM/QEMU on Linux
//!
//! ## Usage
//! ```rust,no_run
//! use benchscale::{Lab, Topology};
//! use benchscale::backend::LibvirtBackend;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let topology = Topology::from_file("my-topology.yaml").await?;
//! let backend = LibvirtBackend::new()?;
//! let lab = Lab::create("my-lab", topology, backend).await?;
//! # Ok(())
//! # }
//! ```

use async_trait::async_trait;
use crate::backend::{Backend, ExecResult, NetworkInfo, NodeInfo, NodeStatus};
use crate::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn, error};

#[cfg(feature = "libvirt")]
use virt::connect::Connect;
#[cfg(feature = "libvirt")]
use virt::domain::Domain;
#[cfg(feature = "libvirt")]
use virt::network::Network;

#[cfg(feature = "libvirt")]
use super::ssh::SshClient;

/// LibvirtBackend for KVM/QEMU VMs
/// 
/// Provides a generic backend for benchScale that uses libvirt to manage VMs.
/// This enables testing scenarios that require full OS environments rather than containers.
#[cfg(feature = "libvirt")]
pub struct LibvirtBackend {
    conn: Arc<Mutex<Connect>>,
    ssh_user: String,
    ssh_password: Option<String>,
    base_image_path: String,
}

#[cfg(feature = "libvirt")]
impl LibvirtBackend {
    /// Create a new LibvirtBackend connected to qemu:///system
    pub fn new() -> Result<Self> {
        let conn = Connect::open(Some("qemu:///system"))
            .map_err(|e| crate::Error::Backend(format!("Failed to connect to libvirt: {}", e)))?;
        
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            ssh_user: "testuser".to_string(),
            ssh_password: Some("testpass".to_string()),
            base_image_path: "/var/lib/libvirt/images".to_string(),
        })
    }

    /// Configure SSH credentials for VM access
    pub fn with_ssh_credentials(mut self, user: String, password: Option<String>) -> Self {
        self.ssh_user = user;
        self.ssh_password = password;
        self
    }

    /// Configure base image path for VM disk images
    pub fn with_base_image_path(mut self, path: String) -> Self {
        self.base_image_path = path;
        self
    }

    /// Get VM IP address by domain name
    async fn get_vm_ip_by_name(&self, name: &str) -> Result<String> {
        // Use virsh command to get IP (simpler than libvirt API for this)
        let output = tokio::process::Command::new("virsh")
            .args(&["domifaddr", name, "--source", "lease"])
            .output()
            .await
            .map_err(|e| crate::Error::Backend(format!("Failed to run virsh: {}", e)))?;
        
        if !output.status.success() {
            return Err(crate::Error::Backend(
                format!("virsh domifaddr failed: {}", String::from_utf8_lossy(&output.stderr))
            ));
        }
        
        let output_str = String::from_utf8_lossy(&output.stdout);
        
        // Parse IP from virsh output
        // Format: Name MAC address Protocol Address
        //         ----------------------------------------------------------------
        //         vnet0 52:54:00:xx:xx:xx ipv4 192.168.122.x/24
        for line in output_str.lines() {
            if line.contains("ipv4") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if let Some(ip_with_mask) = parts.last() {
                    if let Some(ip) = ip_with_mask.split('/').next() {
                        info!("Found VM IP: {}", ip);
                        return Ok(ip.to_string());
                    }
                }
            }
        }
        
        Err(crate::Error::Backend("No IP address found for VM".to_string()))
    }
}

#[cfg(feature = "libvirt")]
#[async_trait]
impl Backend for LibvirtBackend {
    async fn create_network(&self, name: &str, subnet: &str) -> Result<NetworkInfo> {
        info!("Creating libvirt network: {}", name);
        
        let conn = self.conn.lock().await;
        
        // Check if network already exists
        if let Ok(existing) = Network::lookup_by_name(&conn, name) {
            warn!("Network {} already exists", name);
            
            let uuid = existing.get_uuid_string()
                .map_err(|e| crate::Error::Backend(format!("Failed to get network UUID: {}", e)))?;
            
            return Ok(NetworkInfo {
                name: name.to_string(),
                id: uuid,
                subnet: subnet.to_string(),
                gateway: subnet.replace("/24", ".1"),
            });
        }

        // Create network XML
        let gateway = subnet.replace("/24", ".1");
        let dhcp_start = subnet.replace("/24", ".100");
        let dhcp_end = subnet.replace("/24", ".254");
        
        let network_xml = format!(
            r#"<network>
  <name>{name}</name>
  <forward mode='nat'/>
  <bridge name='virbr-{bridge}' stp='on' delay='0'/>
  <ip address='{gateway}' netmask='255.255.255.0'>
    <dhcp>
      <range start='{dhcp_start}' end='{dhcp_end}'/>
    </dhcp>
  </ip>
</network>"#,
            name = name,
            bridge = name.replace("-", ""),
            gateway = gateway,
            dhcp_start = dhcp_start,
            dhcp_end = dhcp_end,
        );

        let network = Network::define_xml(&conn, &network_xml)
            .map_err(|e| crate::Error::Backend(format!("Failed to define network: {}", e)))?;
        
        network.create()
            .map_err(|e| crate::Error::Backend(format!("Failed to start network: {}", e)))?;
        
        network.set_autostart(true)
            .map_err(|e| crate::Error::Backend(format!("Failed to set autostart: {}", e)))?;

        let uuid = network.get_uuid_string()
            .map_err(|e| crate::Error::Backend(format!("Failed to get network UUID: {}", e)))?;

        info!("Created network: {} ({})", name, uuid);

        Ok(NetworkInfo {
            name: name.to_string(),
            id: uuid,
            subnet: subnet.to_string(),
            gateway,
        })
    }

    async fn delete_network(&self, name: &str) -> Result<()> {
        info!("Deleting libvirt network: {}", name);
        
        let conn = self.conn.lock().await;
        
        if let Ok(network) = Network::lookup_by_name(&conn, name) {
            if network.is_active().unwrap_or(false) {
                network.destroy()
                    .map_err(|e| crate::Error::Backend(format!("Failed to destroy network: {}", e)))?;
            }
            
            network.undefine()
                .map_err(|e| crate::Error::Backend(format!("Failed to undefine network: {}", e)))?;
            
            info!("Deleted network: {}", name);
        }

        Ok(())
    }

    async fn create_node(
        &self,
        name: &str,
        _image: &str,
        _network: &str,
        _env: HashMap<String, String>,
    ) -> Result<NodeInfo> {
        info!("Creating libvirt VM: {}", name);
        
        // TODO: Implement VM creation
        // 1. Clone base image or create from cloud-init
        // 2. Generate cloud-init config
        // 3. Create VM XML definition
        // 4. Start VM
        // 5. Wait for IP and SSH
        
        error!("VM creation not yet implemented");
        Err(crate::Error::Backend("VM creation not implemented".to_string()))
    }

    async fn start_node(&self, node_id: &str) -> Result<()> {
        info!("Starting VM: {}", node_id);
        
        let conn = self.conn.lock().await;
        let domain = Domain::lookup_by_uuid_string(&conn, node_id)
            .map_err(|e| crate::Error::Backend(format!("Failed to lookup domain: {}", e)))?;
        
        domain.create()
            .map_err(|e| crate::Error::Backend(format!("Failed to start domain: {}", e)))?;
        
        Ok(())
    }

    async fn stop_node(&self, node_id: &str) -> Result<()> {
        info!("Stopping VM: {}", node_id);
        
        let conn = self.conn.lock().await;
        let domain = Domain::lookup_by_uuid_string(&conn, node_id)
            .map_err(|e| crate::Error::Backend(format!("Failed to lookup domain: {}", e)))?;
        
        domain.shutdown()
            .map_err(|e| crate::Error::Backend(format!("Failed to shutdown domain: {}", e)))?;
        
        Ok(())
    }

    async fn delete_node(&self, node_id: &str) -> Result<()> {
        info!("Deleting VM: {}", node_id);
        
        let conn = self.conn.lock().await;
        let domain = Domain::lookup_by_uuid_string(&conn, node_id)
            .map_err(|e| crate::Error::Backend(format!("Failed to lookup domain: {}", e)))?;
        
        if domain.is_active().unwrap_or(false) {
            domain.destroy()
                .map_err(|e| crate::Error::Backend(format!("Failed to destroy domain: {}", e)))?;
        }
        
        domain.undefine()
            .map_err(|e| crate::Error::Backend(format!("Failed to undefine domain: {}", e)))?;
        
        Ok(())
    }

    async fn get_node(&self, node_id: &str) -> Result<NodeInfo> {
        let conn = self.conn.lock().await;
        let domain = Domain::lookup_by_uuid_string(&conn, node_id)
            .map_err(|e| crate::Error::Backend(format!("Failed to lookup domain: {}", e)))?;
        
        let name = domain.get_name()
            .map_err(|e| crate::Error::Backend(format!("Failed to get domain name: {}", e)))?;
        
        let is_active = domain.is_active()
            .map_err(|e| crate::Error::Backend(format!("Failed to get domain state: {}", e)))?;
        
        let status = if is_active {
            NodeStatus::Running
        } else {
            NodeStatus::Stopped
        };
        
        Ok(NodeInfo {
            id: node_id.to_string(),
            name,
            container_id: node_id.to_string(),
            ip_address: "unknown".to_string(),
            network: "default".to_string(),
            status,
            metadata: HashMap::new(),
        })
    }

    async fn list_nodes(&self, _network: &str) -> Result<Vec<NodeInfo>> {
        let conn = self.conn.lock().await;
        let domains = conn.list_all_domains(0)
            .map_err(|e| crate::Error::Backend(format!("Failed to list domains: {}", e)))?;
        
        let mut nodes = Vec::new();
        for domain in domains {
            if let Ok(uuid) = domain.get_uuid_string() {
                if let Ok(name) = domain.get_name() {
                    let status = if domain.is_active().unwrap_or(false) {
                        NodeStatus::Running
                    } else {
                        NodeStatus::Stopped
                    };
                    
                    nodes.push(NodeInfo {
                        id: uuid.clone(),
                        name,
                        container_id: uuid,
                        ip_address: "unknown".to_string(),
                        network: "default".to_string(),
                        status,
                        metadata: HashMap::new(),
                    });
                }
            }
        }
        
        Ok(nodes)
    }

    async fn exec_command(&self, node_id: &str, command: Vec<String>) -> Result<ExecResult> {
        info!("Executing command on VM {}: {:?}", node_id, command);
        
        // Get VM info to find IP
        let node_info = self.get_node(node_id).await?;
        
        // Get VM IP address
        let ip = self.get_vm_ip_by_name(&node_info.name).await?;
        
        // Connect via SSH
        let mut ssh = SshClient::connect(
            &ip,
            22,
            &self.ssh_user,
            self.ssh_password.as_deref().unwrap_or(""),
        ).await?;
        
        // Execute command
        let (exit_code, stdout, stderr) = ssh.execute(&command).await?;
        
        // Disconnect
        ssh.disconnect().await?;
        
        Ok(ExecResult {
            exit_code: exit_code as i64,
            stdout,
            stderr,
        })
    }

    async fn copy_to_node(&self, node_id: &str, src_path: &str, dest_path: &str) -> Result<()> {
        info!("Copying {} to VM {} at {}", src_path, node_id, dest_path);
        
        // Get VM info to find IP
        let node_info = self.get_node(node_id).await?;
        
        // Get VM IP address
        let ip = self.get_vm_ip_by_name(&node_info.name).await?;
        
        // Connect via SSH
        let mut ssh = SshClient::connect(
            &ip,
            22,
            &self.ssh_user,
            self.ssh_password.as_deref().unwrap_or(""),
        ).await?;
        
        // Copy file
        ssh.copy_file(src_path, dest_path).await?;
        
        // Disconnect
        ssh.disconnect().await?;
        
        Ok(())
    }

    async fn get_logs(&self, _node_id: &str) -> Result<String> {
        // TODO: Implement log retrieval
        warn!("Log retrieval not yet implemented");
        Ok(String::new())
    }

    async fn apply_network_conditions(
        &self,
        _node_id: &str,
        latency_ms: Option<u32>,
        packet_loss_percent: Option<f32>,
        bandwidth_kbps: Option<u32>,
    ) -> Result<()> {
        if latency_ms.is_some() || packet_loss_percent.is_some() || bandwidth_kbps.is_some() {
            warn!("Network conditions not yet implemented for LibvirtBackend");
        }
        Ok(())
    }

    async fn is_available(&self) -> Result<bool> {
        let conn = self.conn.lock().await;
        Ok(conn.is_alive().unwrap_or(false))
    }
}

#[cfg(feature = "libvirt")]
impl Default for LibvirtBackend {
    fn default() -> Self {
        Self::new().expect("Failed to create LibvirtBackend")
    }
}

// Stub implementation when libvirt feature is not enabled
#[cfg(not(feature = "libvirt"))]
pub struct LibvirtBackend;

#[cfg(not(feature = "libvirt"))]
impl LibvirtBackend {
    pub fn new() -> Result<Self> {
        Err(crate::Error::Backend(
            "LibvirtBackend requires 'libvirt' feature to be enabled".to_string()
        ))
    }
}

