//! VM readiness validation for LibvirtBackend
//!
//! This module provides functions to wait for VMs to become fully operational,
//! including cloud-init completion and SSH availability. These are critical
//! for ensuring VMs are ready before attempting to use them.

use crate::backend::NodeInfo;
use crate::Result;
use std::time::Duration;
use tracing::{info, warn};

use super::ssh::SshClient;
use super::LibvirtBackend;

impl LibvirtBackend {
    /// Wait for a VM to acquire an IP address
    ///
    /// This is an internal method used by create_desktop_vm() and similar
    /// functions to ensure the VM has obtained an IP before proceeding.
    pub(super) async fn wait_for_ip(&self, name: &str, timeout: Duration) -> Result<String> {
        let start = std::time::Instant::now();

        loop {
            if start.elapsed() > timeout {
                return Err(crate::Error::Backend(format!(
                    "Timeout waiting for IP for VM {}",
                    name
                )));
            }

            if let Ok(ip) = self.get_vm_ip_by_name(name).await {
                if !ip.is_empty() && ip != "0.0.0.0" {
                    return Ok(ip);
                }
            }

            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }

    /// Wait for cloud-init to complete on a VM
    ///
    /// Polls the VM until cloud-init finishes or timeout is reached.
    /// This should be called after `create_desktop_vm()` or `create_from_template()`
    /// and before attempting SSH connections.
    ///
    /// # Arguments
    /// * `node_id` - The VM name/ID
    /// * `known_ip` - Optional known IP (for static IP VMs, avoids libvirt query)
    /// * `username` - Username to use for SSH connection
    /// * `password` - Password for SSH authentication
    /// * `timeout` - Maximum time to wait (recommended: 10 minutes for desktop VMs)
    ///
    /// # Returns
    /// `Ok(())` when cloud-init completes successfully
    ///
    /// # Errors
    /// Returns error if:
    /// - Timeout is reached
    /// - Cloud-init fails
    /// - VM is not accessible
    ///
    /// # Example
    /// ```no_run
    /// # use benchscale::LibvirtBackend;
    /// # use std::time::Duration;
    /// # async fn example() -> anyhow::Result<()> {
    /// let backend = LibvirtBackend::new()?;
    /// let node = backend.create_desktop_vm(...).await?;
    /// backend.wait_for_cloud_init(&node.id, Some(&node.ip_address), "ubuntu", "password", Duration::from_secs(600)).await?;
    /// // Now safe to use SSH
    /// # Ok(())
    /// # }
    /// ```
    pub async fn wait_for_cloud_init(
        &self,
        node_id: &str,
        known_ip: Option<&str>,
        username: &str,
        password: &str,
        timeout: Duration,
    ) -> Result<()> {
        use std::time::Instant;

        info!("Waiting for cloud-init to complete on VM: {}", node_id);
        let start = Instant::now();

        // Use known IP if provided (for static IP VMs), otherwise query libvirt (for DHCP VMs)
        let ip = if let Some(ip) = known_ip {
            info!("Using known static IP: {}", ip);
            ip.to_string()
        } else {
            info!("Querying libvirt for VM IP...");
            self.get_vm_ip_by_name(node_id).await?
        };

        let mut backoff = Duration::from_secs(5);
        let mut last_error = String::new();

        while start.elapsed() < timeout {
            // Try to connect and check cloud-init status
            match SshClient::connect(&ip, 22, username, password).await {
                Ok(mut ssh) => {
                    // Cloud-init status command
                    match ssh.execute(&["cloud-init status --wait".to_string()]).await {
                        Ok((exit_code, stdout, _stderr)) => {
                            if exit_code == 0 && stdout.contains("status: done") {
                                info!("Cloud-init completed successfully on {}", node_id);
                                let _ = ssh.disconnect().await;
                                return Ok(());
                            } else if exit_code == 0 && stdout.contains("status: running") {
                                info!("Cloud-init still running on {}, waiting...", node_id);
                                last_error = "Cloud-init still running".to_string();
                            } else if stdout.contains("status: error")
                                || stdout.contains("status: degraded")
                            {
                                warn!(
                                    "Cloud-init reported error/degraded status on {}: {}",
                                    node_id, stdout
                                );
                                // Continue waiting - some errors are non-fatal
                                last_error = format!("Cloud-init error: {}", stdout);
                            }
                        }
                        Err(e) => {
                            // Command failed, but SSH worked - cloud-init might not be installed
                            // or command is not available yet
                            last_error = format!("Cloud-init status check failed: {}", e);
                        }
                    }
                    let _ = ssh.disconnect().await;
                }
                Err(e) => {
                    // SSH not ready yet - normal during early boot
                    last_error = format!("SSH not ready: {}", e);
                }
            }

            // Exponential backoff up to 30 seconds
            tokio::time::sleep(backoff).await;
            backoff = (backoff * 2).min(Duration::from_secs(30));
        }

        Err(crate::Error::Backend(format!(
            "Timeout waiting for cloud-init on {} after {}s. Last error: {}",
            node_id,
            timeout.as_secs(),
            last_error
        )))
    }

    /// Wait for SSH to become available on a VM
    ///
    /// Polls until SSH connection succeeds with exponential backoff.
    /// This is useful for VMs that don't use cloud-init or for additional validation.
    ///
    /// # Arguments
    /// * `ip` - VM IP address
    /// * `username` - SSH username
    /// * `password` - SSH password
    /// * `timeout` - Maximum time to wait
    ///
    /// # Example
    /// ```no_run
    /// # use benchscale::LibvirtBackend;
    /// # use std::time::Duration;
    /// # async fn example() -> anyhow::Result<()> {
    /// let backend = LibvirtBackend::new()?;
    /// let node = backend.create_desktop_vm(...).await?;
    /// backend.wait_for_ssh(&node.ip_address, "ubuntu", "password", Duration::from_secs(300)).await?;
    /// // SSH is ready
    /// # Ok(())
    /// # }
    /// ```
    pub async fn wait_for_ssh(
        &self,
        ip: &str,
        username: &str,
        password: &str,
        timeout: Duration,
    ) -> Result<()> {
        use std::time::Instant;

        info!("Waiting for SSH to become available on {}", ip);
        let start = Instant::now();
        let mut backoff = Duration::from_secs(2);
        let mut last_error = String::new();

        while start.elapsed() < timeout {
            match SshClient::connect(ip, 22, username, password).await {
                Ok(mut ssh) => {
                    // Test with simple command
                    match ssh.execute(&["echo 'SSH ready'".to_string()]).await {
                        Ok((exit_code, stdout, _)) => {
                            if exit_code == 0 && stdout.contains("SSH ready") {
                                info!("SSH is ready on {}", ip);
                                let _ = ssh.disconnect().await;
                                return Ok(());
                            }
                        }
                        Err(e) => {
                            last_error = format!("Command execution failed: {}", e);
                        }
                    }
                    let _ = ssh.disconnect().await;
                }
                Err(e) => {
                    last_error = format!("Connection failed: {}", e);
                }
            }

            // Exponential backoff up to 30 seconds
            tokio::time::sleep(backoff).await;
            backoff = (backoff * 2).min(Duration::from_secs(30));
        }

        Err(crate::Error::Backend(format!(
            "Timeout waiting for SSH on {} after {}s. Last error: {}",
            ip,
            timeout.as_secs(),
            last_error
        )))
    }

    /// Create a desktop VM and wait for it to be ready
    ///
    /// Convenience method that combines `create_desktop_vm()` with validation.
    /// **This is the recommended API** as it ensures the VM is fully
    /// provisioned before returning.
    ///
    /// # Arguments
    /// * `name` - Unique name for the VM
    /// * `base_image` - Path to cloud image
    /// * `cloud_init` - Cloud-init configuration
    /// * `memory_mb` - RAM in megabytes
    /// * `vcpus` - Number of virtual CPUs
    /// * `disk_size_gb` - Disk size in GB
    /// * `username` - Username for SSH validation
    /// * `password` - Password for SSH validation
    /// * `timeout` - Maximum time to wait for cloud-init (recommended: 600s for desktop)
    ///
    /// # Example
    /// ```no_run
    /// use benchscale::{LibvirtBackend, CloudInit};
    /// use std::path::Path;
    /// use std::time::Duration;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let backend = LibvirtBackend::new()?;
    ///
    /// let cloud_init = CloudInit::builder()
    ///     .add_user("ubuntu", "ssh-rsa AAAAB3...")
    ///     .password("ubuntu", "password123")
    ///     .package("ubuntu-desktop-minimal")
    ///     .build();
    ///
    /// let node = backend.create_desktop_vm_ready(
    ///     "my-vm",
    ///     Path::new("/path/to/ubuntu-22.04.img"),
    ///     &cloud_init,
    ///     3072, 2, 25,
    ///     "ubuntu",
    ///     "password123",
    ///     Duration::from_secs(600), // Wait up to 10 min
    /// ).await?;
    ///
    /// // SSH is guaranteed to work now
    /// // Cloud-init has completed
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create_desktop_vm_ready(
        &self,
        name: &str,
        base_image: &std::path::Path,
        cloud_init: &crate::CloudInit,
        memory_mb: u32,
        vcpus: u32,
        disk_size_gb: u32,
        username: &str,
        password: &str,
        timeout: Duration,
    ) -> Result<NodeInfo> {
        // Create the VM
        let node = self
            .create_desktop_vm(name, base_image, cloud_init, memory_mb, vcpus, disk_size_gb)
            .await?;

        // Wait for cloud-init to complete with known static IP
        info!(
            "Waiting for cloud-init to complete (timeout: {}s)...",
            timeout.as_secs()
        );
        self.wait_for_cloud_init(
            &node.id,
            Some(&node.ip_address), // Pass the known static IP
            username,
            password,
            timeout,
        )
        .await?;

        info!("VM {} is fully ready!", name);
        Ok(node)
    }

    /// Create a VM from template and wait for it to be ready
    ///
    /// Convenience method that combines `create_from_template()` with validation.
    /// Useful when templates include cloud-init customization.
    ///
    /// # Example
    /// ```no_run
    /// use benchscale::LibvirtBackend;
    /// use std::path::Path;
    /// use std::time::Duration;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let backend = LibvirtBackend::new()?;
    ///
    /// let node = backend.create_from_template_ready(
    ///     "my-vm",
    ///     Path::new("../agentReagents/images/templates/ubuntu-template.qcow2"),
    ///     None,
    ///     3072, 2,
    ///     false,
    ///     "ubuntu",
    ///     "password",
    ///     Duration::from_secs(120), // Templates are faster
    /// ).await?;
    ///
    /// // SSH is ready
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create_from_template_ready(
        &self,
        name: &str,
        template_path: &std::path::Path,
        cloud_init: Option<&crate::CloudInit>,
        memory_mb: u32,
        vcpus: u32,
        save_intermediate: bool,
        username: &str,
        password: &str,
        timeout: Duration,
    ) -> Result<NodeInfo> {
        // Create from template
        let node = self
            .create_from_template(
                name,
                template_path,
                cloud_init,
                memory_mb,
                vcpus,
                save_intermediate,
            )
            .await?;

        // If cloud-init was provided, wait for it
        if cloud_init.is_some() {
            info!(
                "Waiting for cloud-init to complete (timeout: {}s)...",
                timeout.as_secs()
            );
            self.wait_for_cloud_init(
                &node.id,
                Some(&node.ip_address),
                username,
                password,
                timeout,
            )
            .await?;
        } else {
            // Just wait for SSH if no cloud-init
            info!(
                "Waiting for SSH to be ready (timeout: {}s)...",
                timeout.as_secs()
            );
            self.wait_for_ssh(&node.ip_address, username, password, timeout)
                .await?;
        }

        info!("VM {} is fully ready!", name);
        Ok(node)
    }
}
