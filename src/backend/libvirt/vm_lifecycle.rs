//! VM lifecycle operations for LibvirtBackend
//!
//! This module contains VM creation functions that orchestrate the complete
//! lifecycle of creating VMs from various sources (cloud images, templates).

use crate::backend::{Backend, NodeInfo, NodeStatus};
use crate::Result;
use std::collections::HashMap;
use std::process::Command;
use std::time::Duration;
use tracing::{info, warn};

use super::LibvirtBackend;

impl LibvirtBackend {
    /// Create a VM from a registered template
    ///
    /// Wrapper around create_from_template() that looks up the template by name
    /// from the registry populated during backend initialization.
    ///
    /// # Arguments
    /// * `vm_name` - Name for the new VM
    /// * `template_name` - Name of registered template (e.g., "ubuntu-22.04-desktop")
    /// * `cloud_init` - Optional cloud-init customization
    /// * `memory_mb` - RAM in megabytes
    /// * `vcpus` - Number of virtual CPUs
    pub async fn create_from_registered_template(
        &self,
        vm_name: &str,
        template_name: &str,
        cloud_init: Option<&crate::CloudInit>,
        memory_mb: u32,
        vcpus: u32,
    ) -> Result<NodeInfo> {
        let template_path = self.get_template_path(template_name)?;

        info!(
            "Creating VM '{}' from template '{}'",
            vm_name, template_name
        );

        self.create_from_template(
            vm_name,
            template_path,
            cloud_init,
            memory_mb,
            vcpus,
            false, // save_intermediate
        )
        .await
    }

    /// Create a desktop VM with cloud-init support
    ///
    /// This creates a full desktop environment VM suitable for GUI applications
    /// like RustDesk. The VM will have a desktop environment installed via cloud-init.
    ///
    /// # Features
    /// - **Static IP allocation** from IP pool (eliminates DHCP race conditions)
    /// - **Automatic cleanup** of existing VMs with same name
    /// - **Cloud-init provisioning** with network configuration
    /// - **Disk resizing** to requested size
    ///
    /// # Arguments
    /// * `name` - Unique name for the VM
    /// * `base_image` - Path to cloud image (e.g., Ubuntu 22.04)
    /// * `cloud_init` - Cloud-init configuration for user setup and packages
    /// * `memory_mb` - RAM in megabytes (recommend 3072+ for desktop)
    /// * `vcpus` - Number of virtual CPUs
    /// * `disk_size_gb` - Disk size in GB (recommend 25+ for desktop)
    ///
    /// # Returns
    /// `NodeInfo` with the VM's details including static IP address
    ///
    /// # Example
    /// ```no_run
    /// use benchscale::{LibvirtBackend, CloudInit};
    /// use std::path::Path;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let backend = LibvirtBackend::new()?;
    ///
    /// let cloud_init = CloudInit::builder()
    ///     .add_user("ubuntu", "ssh-rsa AAAAB3...")
    ///     .package("ubuntu-desktop-minimal")
    ///     .package("xrdp")
    ///     .build();
    ///
    /// let node = backend.create_desktop_vm(
    ///     "my-desktop-vm",
    ///     Path::new("/path/to/ubuntu-22.04.img"),
    ///     &cloud_init,
    ///     3072,
    ///     2,
    ///     25,
    /// ).await?;
    ///
    /// println!("VM ready at {}", node.ip_address);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create_desktop_vm(
        &self,
        name: &str,
        base_image: &std::path::Path,
        cloud_init: &crate::CloudInit,
        memory_mb: u32,
        vcpus: u32,
        disk_size_gb: u32,
    ) -> Result<NodeInfo> {
        info!("Creating desktop VM: {}", name);

        // 0. Check if VM exists and clean up (auto VM cleanup)
        if let Ok(_existing) = self.get_node(name).await {
            warn!(
                "VM '{}' already exists, cleaning up before creating new one...",
                name
            );
            // Best-effort cleanup - don't fail if cleanup fails
            if let Err(e) = self.delete_node(name).await {
                warn!(
                    "Cleanup of existing VM '{}' failed: {}. Continuing anyway...",
                    name, e
                );
            }
            // Small delay for libvirt to finish cleanup
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        // 1. Allocate static IP from pool (eliminates DHCP race conditions)
        let static_ip = self.ip_pool.allocate().await?;
        info!("  Allocated static IP {} for VM {}", static_ip, name);

        // 2. Create disk from base image (using discovered storage path)
        let disk_path = self
            .capabilities
            .storage
            .images_dir
            .join(format!("{}.qcow2", name));
        let disk_path_str = disk_path
            .to_str()
            .ok_or_else(|| crate::Error::Backend("Invalid disk path".to_string()))?;

        info!("  Copying base image to {}", disk_path_str);
        let output = Command::new("sudo")
            .args(["cp", base_image.to_str().unwrap(), disk_path_str])
            .output()
            .map_err(|e| crate::Error::Backend(format!("Failed to copy image: {}", e)))?;

        if !output.status.success() {
            return Err(crate::Error::Backend(format!(
                "Failed to copy image: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        // Resize disk
        info!("  Resizing disk to {}GB", disk_size_gb);
        let output = Command::new("sudo")
            .args([
                "qemu-img",
                "resize",
                disk_path_str,
                &format!("{}G", disk_size_gb),
            ])
            .output()
            .map_err(|e| crate::Error::Backend(format!("Failed to resize: {}", e)))?;

        if !output.status.success() {
            return Err(crate::Error::Backend(format!(
                "Failed to resize disk: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        // 3. Generate cloud-init ISO with static IP configuration
        info!("  Generating cloud-init configuration with static IP");

        // Clone cloud-init and add static IP network configuration
        let mut cloud_init_with_ip = cloud_init.clone();

        // Add static IP network configuration using discovered capabilities
        cloud_init_with_ip.network_config = Some(crate::cloud_init::NetworkConfig::new(
            &self.capabilities.network.default_interface, // Discovered interface name
            format!("{}/{}", static_ip, self.capabilities.network.netmask_bits), // IP with discovered netmask
            &self.capabilities.network.gateway, // Discovered gateway
        ));

        let user_data = match cloud_init_with_ip.to_user_data() {
            Ok(data) => data,
            Err(e) => {
                // Release IP on failure
                self.ip_pool.release(static_ip).await;
                return Err(crate::Error::Backend(format!(
                    "Failed to generate cloud-init: {}",
                    e
                )));
            }
        };

        // Use discovered cloud-init directory
        let user_data_path = self
            .capabilities
            .storage
            .cloud_init_dir
            .join(format!("user-data-{}", name));
        if let Err(e) = std::fs::write(&user_data_path, &user_data) {
            // Release IP on failure
            self.ip_pool.release(static_ip).await;
            return Err(crate::Error::Backend(format!(
                "Failed to write user-data: {}",
                e
            )));
        }

        // Create meta-data
        let meta_data = format!("instance-id: {}\nlocal-hostname: {}\n", name, name);

        // Create temp directory for this VM's cloud-init files (using discovered path)
        let temp_dir = self.capabilities.storage.cloud_init_dir.join(name);
        std::fs::create_dir_all(&temp_dir)
            .map_err(|e| crate::Error::Backend(format!("Failed to create temp dir: {}", e)))?;

        // Write with standard cloud-init filenames (not VM-specific names)
        let meta_data_path = temp_dir.join("meta-data");
        let user_data_final_path = temp_dir.join("user-data");

        std::fs::write(&meta_data_path, meta_data)
            .map_err(|e| crate::Error::Backend(format!("Failed to write meta-data: {}", e)))?;
        std::fs::copy(&user_data_path, &user_data_final_path)
            .map_err(|e| crate::Error::Backend(format!("Failed to copy user-data: {}", e)))?;

        let user_data_final_path_str = user_data_final_path
            .to_str()
            .ok_or_else(|| crate::Error::Backend("Invalid user-data path".to_string()))?;
        let meta_data_path_str = meta_data_path
            .to_str()
            .ok_or_else(|| crate::Error::Backend("Invalid meta-data path".to_string()))?;

        // Create ISO with standard cloud-init filenames (using discovered storage path)
        let iso_path = self
            .capabilities
            .storage
            .images_dir
            .join(format!("{}-cidata.iso", name));
        let iso_path_str = iso_path
            .to_str()
            .ok_or_else(|| crate::Error::Backend("Invalid ISO path".to_string()))?;

        info!("  Creating cloud-init ISO");
        let output = Command::new("sudo")
            .args([
                "genisoimage",
                "-output",
                iso_path_str,
                "-volid",
                "cidata",
                "-joliet",
                "-rock",
                user_data_final_path_str,
                meta_data_path_str,
            ])
            .output()
            .map_err(|e| crate::Error::Backend(format!("Failed to create ISO: {}", e)))?;

        if !output.status.success() {
            return Err(crate::Error::Backend(format!(
                "Failed to create cloud-init ISO: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        // 4. Define and start VM
        info!("  Defining VM in libvirt");
        let output = Command::new("sudo")
            .args([
                "virt-install",
                "--name",
                name,
                "--memory",
                &memory_mb.to_string(),
                "--vcpus",
                &vcpus.to_string(),
                "--disk",
                &format!("path={},format=qcow2", disk_path_str),
                "--disk",
                &format!("path={},device=cdrom", iso_path_str),
                "--os-variant",
                "ubuntu22.04",
                "--network",
                "network=default",
                "--graphics",
                "vnc,listen=0.0.0.0",
                "--noautoconsole",
                "--import",
            ])
            .output()
            .map_err(|e| {
                // Release IP on failure
                let ip = static_ip;
                let pool = self.ip_pool.clone();
                tokio::spawn(async move {
                    let _ = pool.release(ip).await;
                });
                crate::Error::Backend(format!("Failed to create VM: {}", e))
            })?;

        if !output.status.success() {
            // Release IP on failure (ignore release errors, prioritize VM creation error)
            let _ = self.ip_pool.release(static_ip).await;
            return Err(crate::Error::Backend(format!(
                "Failed to create VM: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        info!("  VM created with static IP {}", static_ip);

        // 5. Return NodeInfo with pre-assigned IP (no DHCP wait needed!)
        Ok(NodeInfo {
            id: name.to_string(),
            name: name.to_string(),
            container_id: name.to_string(),
            ip_address: static_ip.to_string(),
            network: "default".to_string(),
            status: NodeStatus::Running,
            metadata: HashMap::new(),
        })
    }

    /// Create a VM from a pre-built template image
    ///
    /// Uses a template from agentReagents/ for faster provisioning.
    /// Creates a copy-on-write (CoW) disk to minimize storage usage.
    /// Optionally saves intermediate snapshot for validation.
    ///
    /// # Arguments
    /// * `name` - Unique VM name
    /// * `template_path` - Path to template qcow2 (e.g., from agentReagents/images/templates/)
    /// * `cloud_init` - Optional cloud-init for customization (networking, users, etc.)
    /// * `memory_mb` - RAM in megabytes
    /// * `vcpus` - Number of virtual CPUs
    /// * `save_intermediate` - If true, saves snapshot to agentReagents/images/intermediates/
    ///
    /// # Example
    /// ```rust,no_run
    /// # use benchscale::backend::LibvirtBackend;
    /// # use std::path::PathBuf;
    /// # async fn example() -> anyhow::Result<()> {
    /// let backend = LibvirtBackend::new()?;
    /// let template = PathBuf::from("../agentReagents/images/templates/rustdesk-ubuntu-22.04-template.qcow2");
    ///
    /// let node = backend.create_from_template(
    ///     "my-rustdesk-vm",
    ///     &template,
    ///     None,  // No additional cloud-init needed
    ///     2048,  // 2GB RAM
    ///     2,     // 2 vCPUs
    ///     true,  // Save intermediate
    /// ).await?;
    ///
    /// println!("VM ready at {}", node.ip_address);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create_from_template(
        &self,
        name: &str,
        template_path: &std::path::Path,
        cloud_init: Option<&crate::CloudInit>,
        memory_mb: u32,
        vcpus: u32,
        save_intermediate: bool,
    ) -> Result<NodeInfo> {
        info!("Creating VM from template: {}", name);
        info!("  Template: {}", template_path.display());

        // 1. Create disk from template using CoW (using discovered storage path)
        let disk_path = self
            .capabilities
            .storage
            .images_dir
            .join(format!("{}.qcow2", name));
        let disk_path_str = disk_path
            .to_str()
            .ok_or_else(|| crate::Error::Backend("Invalid disk path".to_string()))?;

        info!("  Creating CoW disk from template");
        let output = Command::new("sudo")
            .args([
                "qemu-img",
                "create",
                "-f",
                "qcow2",
                "-F",
                "qcow2",
                "-b",
                template_path.to_str().unwrap(),
                disk_path_str,
            ])
            .output()
            .map_err(|e| crate::Error::Backend(format!("Failed to create CoW disk: {}", e)))?;

        if !output.status.success() {
            return Err(crate::Error::Backend(format!(
                "Failed to create CoW disk: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        // 2. Handle cloud-init configuration
        let mut cloud_init_args = Vec::new();

        if let Some(cloud_init) = cloud_init {
            info!("  Generating cloud-init for customization");

            let user_data = cloud_init.to_user_data().map_err(|e| {
                crate::Error::Backend(format!("Failed to generate user-data: {}", e))
            })?;

            let meta_data = format!("instance-id: {}\nlocal-hostname: {}\n", name, name);

            // Create temp directory for this VM's cloud-init files (using discovered path)
            let temp_dir = self.capabilities.storage.cloud_init_dir.join(name);
            std::fs::create_dir_all(&temp_dir)
                .map_err(|e| crate::Error::Backend(format!("Failed to create temp dir: {}", e)))?;

            // Write to temp files with standard cloud-init filenames
            let user_data_path = temp_dir.join("user-data");
            let meta_data_path = temp_dir.join("meta-data");

            std::fs::write(&user_data_path, user_data)
                .map_err(|e| crate::Error::Backend(format!("Failed to write user-data: {}", e)))?;
            std::fs::write(&meta_data_path, meta_data)
                .map_err(|e| crate::Error::Backend(format!("Failed to write meta-data: {}", e)))?;

            let user_data_path_str = user_data_path
                .to_str()
                .ok_or_else(|| crate::Error::Backend("Invalid user-data path".to_string()))?;
            let meta_data_path_str = meta_data_path
                .to_str()
                .ok_or_else(|| crate::Error::Backend("Invalid meta-data path".to_string()))?;

            // Use virt-install's built-in cloud-init support (more reliable than ISO)
            cloud_init_args = vec![
                "--cloud-init".to_string(),
                format!(
                    "user-data={},meta-data={}",
                    user_data_path_str, meta_data_path_str
                ),
            ];
        }

        // 3. Define and start VM
        info!("  Defining VM in libvirt");

        // Prepare arguments (must live long enough)
        let memory_str = memory_mb.to_string();
        let vcpus_str = vcpus.to_string();
        let disk_arg = format!("path={},format=qcow2", disk_path_str);

        let mut virt_install_args = vec![
            "virt-install",
            "--name",
            name,
            "--memory",
            &memory_str,
            "--vcpus",
            &vcpus_str,
            "--disk",
            &disk_arg,
            "--os-variant",
            "ubuntu22.04",
            "--network",
            "network=default",
            "--graphics",
            "vnc,listen=0.0.0.0",
            "--noautoconsole",
            "--import",
        ];

        // Add cloud-init args if present (using virt-install's built-in support)
        for arg in &cloud_init_args {
            virt_install_args.push(arg);
        }

        let output = Command::new("sudo")
            .args(&virt_install_args)
            .output()
            .map_err(|e| crate::Error::Backend(format!("Failed to create VM: {}", e)))?;

        if !output.status.success() {
            return Err(crate::Error::Backend(format!(
                "Failed to create VM: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        info!("  VM created, waiting for network");

        // 4. Wait for IP address (using configured timeout)
        let timeout = Duration::from_secs(self.config.vm_ip_timeout_secs);
        let ip_address = self.wait_for_ip(name, timeout).await?;

        info!("  VM got IP: {}", ip_address);

        // 5. Save intermediate if requested
        if save_intermediate {
            info!("  Saving intermediate snapshot");

            // Create intermediates directory if it doesn't exist
            let intermediate_dir = template_path
                .parent()
                .and_then(|p| p.parent())
                .map(|p| p.join("intermediates"))
                .unwrap_or_else(|| std::path::PathBuf::from("/tmp"));

            std::fs::create_dir_all(&intermediate_dir).map_err(|e| {
                crate::Error::Backend(format!("Failed to create intermediates dir: {}", e))
            })?;

            let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
            let intermediate_path =
                intermediate_dir.join(format!("{}-intermediate-{}.qcow2", name, timestamp));

            let output = Command::new("sudo")
                .args(["cp", disk_path_str, intermediate_path.to_str().unwrap()])
                .output()
                .map_err(|e| {
                    crate::Error::Backend(format!("Failed to save intermediate: {}", e))
                })?;

            if output.status.success() {
                info!("  Intermediate saved: {}", intermediate_path.display());
            } else {
                warn!(
                    "  Failed to save intermediate: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }

        // 6. Return NodeInfo
        Ok(NodeInfo {
            id: name.to_string(),
            name: name.to_string(),
            container_id: name.to_string(),
            ip_address,
            network: "default".to_string(),
            status: NodeStatus::Running,
            metadata: HashMap::new(),
        })
    }
}
