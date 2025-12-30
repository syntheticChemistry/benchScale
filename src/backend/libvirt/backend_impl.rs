//! Backend trait implementation for LibvirtBackend
//!
//! This module implements the Backend trait for LibvirtBackend, providing
//! the standard benchScale interface for VM management operations.

use crate::backend::{Backend, ExecResult, NetworkInfo, NodeInfo, NodeStatus};
use crate::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::time::Duration;
use tracing::{info, warn};

use virt::domain::Domain;
use virt::network::Network;

use super::ssh::SshClient;
use super::vm_utils;
use super::LibvirtBackend;

#[async_trait]
impl Backend for LibvirtBackend {
    async fn create_network(&self, name: &str, subnet: &str) -> Result<NetworkInfo> {
        info!("Creating libvirt network: {}", name);

        let conn = self.conn.lock().await;

        // Check if network already exists
        if let Ok(existing) = Network::lookup_by_name(&conn, name) {
            warn!("Network {} already exists", name);

            let uuid = existing
                .get_uuid_string()
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
            r"<network>
  <name>{name}</name>
  <forward mode='nat'/>
  <bridge name='virbr-{bridge}' stp='on' delay='0'/>
  <ip address='{gateway}' netmask='255.255.255.0'>
    <dhcp>
      <range start='{dhcp_start}' end='{dhcp_end}'/>
    </dhcp>
  </ip>
</network>",
            name = name,
            bridge = name.replace("-", ""),
            gateway = gateway,
            dhcp_start = dhcp_start,
            dhcp_end = dhcp_end,
        );

        let network = Network::define_xml(&conn, &network_xml)
            .map_err(|e| crate::Error::Backend(format!("Failed to define network: {}", e)))?;

        network
            .create()
            .map_err(|e| crate::Error::Backend(format!("Failed to start network: {}", e)))?;

        network
            .set_autostart(true)
            .map_err(|e| crate::Error::Backend(format!("Failed to set autostart: {}", e)))?;

        let uuid = network
            .get_uuid_string()
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
                network.destroy().map_err(|e| {
                    crate::Error::Backend(format!("Failed to destroy network: {}", e))
                })?;
            }

            network
                .undefine()
                .map_err(|e| crate::Error::Backend(format!("Failed to undefine network: {}", e)))?;

            info!("Deleted network: {}", name);
        }

        Ok(())
    }

    async fn create_node(
        &self,
        name: &str,
        image: &str,
        network: &str,
        env: HashMap<String, String>,
    ) -> Result<NodeInfo> {
        use vm_utils::{generate_domain_xml, parse_memory, DiskManager};

        info!("Creating libvirt VM: {} from image {}", name, image);

        // 1. Create copy-on-write disk overlay
        let disk_mgr = DiskManager::new(&self.config.overlay_dir);
        let overlay_path = disk_mgr
            .create_overlay(std::path::Path::new(image), name)
            .await?;

        // 2. Parse VM configuration from environment
        let memory_mb = env
            .get("MEMORY")
            .and_then(|m| parse_memory(m))
            .unwrap_or(2048); // Default 2GB

        let vcpus = env.get("VCPUS").and_then(|v| v.parse().ok()).unwrap_or(2); // Default 2 vCPUs

        let serial_log = env
            .get("SERIAL_LOG")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                let mut path = std::env::temp_dir();
                path.push(format!("{}-serial.log", name));
                path
            });

        // 3. Generate libvirt domain XML
        let domain_xml =
            generate_domain_xml(name, &overlay_path, memory_mb, vcpus, network, &serial_log);

        // 4. Define and start the VM
        let conn = self.conn.lock().await;
        let domain = Domain::define_xml(&conn, &domain_xml)
            .map_err(|e| crate::Error::Backend(format!("Failed to define domain: {}", e)))?;

        domain
            .create()
            .map_err(|e| crate::Error::Backend(format!("Failed to start domain: {}", e)))?;

        let uuid = domain
            .get_uuid_string()
            .map_err(|e| crate::Error::Backend(format!("Failed to get domain UUID: {}", e)))?;

        drop(conn); // Release lock before async wait

        // 5. Wait for IP address (using configured timeout)
        let timeout = Duration::from_secs(self.config.vm_ip_timeout_secs);
        let ip = self.wait_for_ip(name, timeout).await?;

        info!("VM {} created successfully with IP {}", name, ip);

        Ok(NodeInfo {
            id: uuid.clone(),
            name: name.to_string(),
            container_id: uuid,
            ip_address: ip,
            network: network.to_string(),
            status: NodeStatus::Running,
            metadata: {
                let mut meta = HashMap::new();
                meta.insert("serial_log".to_string(), serial_log.display().to_string());
                meta.insert(
                    "disk_overlay".to_string(),
                    overlay_path.display().to_string(),
                );
                meta
            },
        })
    }

    async fn start_node(&self, node_id: &str) -> Result<()> {
        info!("Starting VM: {}", node_id);

        let conn = self.conn.lock().await;
        let domain = Domain::lookup_by_uuid_string(&conn, node_id)
            .map_err(|e| crate::Error::Backend(format!("Failed to lookup domain: {}", e)))?;

        domain
            .create()
            .map_err(|e| crate::Error::Backend(format!("Failed to start domain: {}", e)))?;

        Ok(())
    }

    async fn stop_node(&self, node_id: &str) -> Result<()> {
        info!("Stopping VM: {}", node_id);

        let conn = self.conn.lock().await;
        let domain = Domain::lookup_by_uuid_string(&conn, node_id)
            .map_err(|e| crate::Error::Backend(format!("Failed to lookup domain: {}", e)))?;

        domain
            .shutdown()
            .map_err(|e| crate::Error::Backend(format!("Failed to shutdown domain: {}", e)))?;

        Ok(())
    }

    async fn delete_node(&self, node_id: &str) -> Result<()> {
        use std::net::Ipv4Addr;
        use std::str::FromStr;
        use vm_utils::DiskManager;

        info!("Deleting VM: {}", node_id);

        // Get node info first to clean up disk overlay and release IP
        let node_info = self.get_node(node_id).await.ok();
        let node_name = node_info.as_ref().map(|n| n.name.as_str());

        // Release IP back to pool if it's a valid IPv4 address
        if let Some(ref info) = node_info {
            if let Ok(ip) = Ipv4Addr::from_str(&info.ip_address) {
                self.ip_pool.release(ip).await;
                info!("Released IP {} back to pool", ip);
            }
        }

        let conn = self.conn.lock().await;
        let domain = Domain::lookup_by_uuid_string(&conn, node_id)
            .map_err(|e| crate::Error::Backend(format!("Failed to lookup domain: {}", e)))?;

        if domain.is_active().unwrap_or(false) {
            domain
                .destroy()
                .map_err(|e| crate::Error::Backend(format!("Failed to destroy domain: {}", e)))?;
        }

        domain
            .undefine()
            .map_err(|e| crate::Error::Backend(format!("Failed to undefine domain: {}", e)))?;

        drop(conn);

        // Clean up disk overlay
        if let Some(name) = node_name {
            let disk_mgr = DiskManager::new(&self.config.overlay_dir);
            if let Err(e) = disk_mgr.delete_overlay(name).await {
                warn!("Failed to delete disk overlay for {}: {}", name, e);
            }
        }

        Ok(())
    }

    async fn get_node(&self, node_id: &str) -> Result<NodeInfo> {
        let conn = self.conn.lock().await;
        let domain = Domain::lookup_by_uuid_string(&conn, node_id)
            .map_err(|e| crate::Error::Backend(format!("Failed to lookup domain: {}", e)))?;

        let name = domain
            .get_name()
            .map_err(|e| crate::Error::Backend(format!("Failed to get domain name: {}", e)))?;

        let is_active = domain
            .is_active()
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
        let domains = conn
            .list_all_domains(0)
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

        // Connect via SSH using configuration
        let mut ssh = SshClient::connect(
            &ip,
            self.config.ssh.port,
            &self.config.ssh.default_user,
            "", // Password auth deprecated - use key-based auth via SSH agent
        )
        .await?;

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

        // Connect via SSH using configuration
        let mut ssh = SshClient::connect(
            &ip,
            self.config.ssh.port,
            &self.config.ssh.default_user,
            "", // Password auth deprecated - use key-based auth via SSH agent
        )
        .await?;

        // Copy file
        ssh.copy_file(src_path, dest_path).await?;

        // Disconnect
        ssh.disconnect().await?;

        Ok(())
    }

    async fn get_logs(&self, node_id: &str) -> Result<String> {
        // Get node info to find serial log path
        let node_info = self.get_node(node_id).await?;

        if let Some(serial_log_path) = node_info.metadata.get("serial_log") {
            // Read serial console log
            match tokio::fs::read_to_string(serial_log_path).await {
                Ok(content) => Ok(content),
                Err(e) => {
                    warn!("Failed to read serial log: {}", e);
                    Ok(String::new())
                }
            }
        } else {
            warn!("No serial log path found for node {}", node_id);
            Ok(String::new())
        }
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
