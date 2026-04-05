// SPDX-License-Identifier: AGPL-3.0-or-later
//! VM lifecycle operations for LibvirtBackend
//!
//! This module contains VM creation functions that orchestrate the complete
//! lifecycle of creating VMs from various sources (cloud images, templates).

use crate::Result;
use crate::backend::{Backend, NodeInfo, NodeStatus};
use std::collections::HashMap;
use std::process::Command;
use std::time::Duration;
use tracing::{info, warn};

use super::LibvirtBackend;
use super::vm_state::{
    desktop_dhcp_node_metadata, qemu_mac_from_vm_name, release_pool_ip_if_needed,
    spawn_release_pool_ip_if_needed,
};

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
    ///     None,
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
        static_ip: Option<String>,
    ) -> Result<NodeInfo> {
        self.create_desktop_vm_with_pci(
            name,
            base_image,
            cloud_init,
            memory_mb,
            vcpus,
            disk_size_gb,
            static_ip,
            &[],
        )
        .await
    }

    /// Create a desktop VM with optional PCI passthrough devices.
    ///
    /// `pci_devices` are passed to `virt-install` as `--hostdev` arguments.
    /// Each device must be bound to `vfio-pci` on the host before calling this.
    pub async fn create_desktop_vm_with_pci(
        &self,
        name: &str,
        base_image: &std::path::Path,
        cloud_init: &crate::CloudInit,
        memory_mb: u32,
        vcpus: u32,
        disk_size_gb: u32,
        static_ip: Option<String>,
        pci_devices: &[crate::config_legacy::PciPassthroughDevice],
    ) -> Result<NodeInfo> {
        info!("Creating desktop VM: {}", name);

        // 0a. EVOLUTION #20: Pre-flight infrastructure health check
        // Ensures libvirt is stable before attempting VM creation
        // This prevents wasted time on corrupted infrastructure
        self.ensure_healthy().await?;

        // 0b. Check if VM exists and clean up (auto VM cleanup)
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

        // 1. Allocate static IP - either from parameter (deep debt) or pool (default)
        let (allocated_ip, from_pool) = if let Some(requested_ip) = static_ip {
            // DEEP DEBT SOLUTION: Use provided static IP from agentReagents manifest
            info!(
                "  Using requested static IP {} for VM {}",
                requested_ip, name
            );
            (requested_ip, false)
        } else {
            // Default: Allocate from pool (convert Ipv4Addr to String)
            let ip = self.ip_pool.allocate().await?;
            info!("  Allocated static IP {} from pool for VM {}", ip, name);
            (ip.to_string(), true)
        };

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

        // Phase 1A: Proper error handling for path conversion
        let base_image_str = base_image.to_str().ok_or_else(|| {
            crate::Error::Backend(format!(
                "Invalid base image path (non-UTF8): {:?}",
                base_image
            ))
        })?;

        let output = Command::new("cp")
            .args([base_image_str, disk_path_str])
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
        let output = Command::new("qemu-img")
            .args(["resize", disk_path_str, &format!("{}G", disk_size_gb)])
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
        // No renderer specified - let netplan choose appropriate default
        // NetworkManager is prevented from managing interfaces via unmanaged-devices config
        cloud_init_with_ip.network_config = Some(crate::cloud_init::NetworkConfig::new(
            &self.capabilities.network.default_interface, // Discovered interface name
            format!(
                "{}/{}",
                allocated_ip, self.capabilities.network.netmask_bits
            ), // IP with discovered netmask
            &self.capabilities.network.gateway,           // Discovered gateway
        ));

        let user_data = match cloud_init_with_ip.to_user_data() {
            Ok(data) => data,
            Err(e) => {
                release_pool_ip_if_needed(from_pool, &allocated_ip, &self.ip_pool).await;
                return Err(crate::Error::Backend(format!(
                    "Failed to generate cloud-init: {}",
                    e
                )));
            }
        };

        // Create cloud-init directory first (BEFORE writing any files)
        std::fs::create_dir_all(&self.capabilities.storage.cloud_init_dir).map_err(|e| {
            crate::Error::Backend(format!("Failed to create cloud-init dir: {}", e))
        })?;

        // Use discovered cloud-init directory
        let user_data_path = self
            .capabilities
            .storage
            .cloud_init_dir
            .join(format!("user-data-{}", name));
        if let Err(e) = std::fs::write(&user_data_path, &user_data) {
            release_pool_ip_if_needed(from_pool, &allocated_ip, &self.ip_pool).await;
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
        let _network_config_path = temp_dir.join("network-config");

        std::fs::write(&meta_data_path, meta_data)
            .map_err(|e| crate::Error::Backend(format!("Failed to write meta-data: {}", e)))?;
        std::fs::copy(&user_data_path, &user_data_final_path)
            .map_err(|e| crate::Error::Backend(format!("Failed to copy user-data: {}", e)))?;

        // EVOLUTION #10: Use DHCP instead of static IP for fractal scaling
        // Static IP configuration doesn't persist after reboot, and prevents
        // location-agnostic deployment. Let cloud-init use DHCP (its default).
        // We'll discover the actual IP from libvirt DHCP leases after boot.
        //
        // This enables true fractal scaling: VMs self-configure on any network.
        //
        // NOTE: network-config file is intentionally NOT written.
        // Cloud-init will default to DHCP, which persists across reboots.
        info!("  Using DHCP for network configuration (fractal scaling mode)");
        if let Some(ref network_cfg) = cloud_init_with_ip.network_config {
            info!(
                "  Note: Pool allocated IP {} will be discovered from DHCP leases",
                network_cfg.address
            );
        }

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

        // Evolution #10: Create cloud-init ISO WITHOUT network-config (DHCP mode)
        // Only include user-data and meta-data. Cloud-init will default to DHCP.
        info!("  Creating cloud-init ISO (DHCP mode - no network-config)");
        let output = Command::new("genisoimage")
            .args([
                "-output",
                iso_path_str,
                "-volid",
                "cidata",
                "-joliet",
                "-rock",
                user_data_final_path_str,
                meta_data_path_str,
                // Evolution #10: network-config intentionally EXCLUDED for DHCP
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

        // Evolution #12: deterministic MAC for DHCP discovery (see `vm_state`).
        let mac_address = qemu_mac_from_vm_name(name);
        info!(
            "  Generated MAC address: {} (for DHCP discovery)",
            mac_address
        );

        let disk_arg = format!("path={},format=qcow2", disk_path_str);
        let cdrom_arg = format!("path={},device=cdrom", iso_path_str);
        let net_arg = format!("network=default,mac={}", mac_address);
        let mem_str = memory_mb.to_string();
        let vcpu_str = vcpus.to_string();

        let mut virt_args: Vec<&str> = vec![
            "--name",
            name,
            "--memory",
            &mem_str,
            "--vcpus",
            &vcpu_str,
            "--disk",
            &disk_arg,
            "--disk",
            &cdrom_arg,
            "--os-variant",
            "ubuntu22.04",
            "--network",
            &net_arg,
            "--graphics",
            "vnc,listen=0.0.0.0",
            "--noautoconsole",
            "--import",
        ];

        let hostdev_bdfs: Vec<String> = pci_devices
            .iter()
            .map(crate::config_legacy::PciPassthroughDevice::to_virt_install_arg)
            .collect();
        for bdf in &hostdev_bdfs {
            virt_args.push("--hostdev");
            virt_args.push(bdf);
            info!("  PCI passthrough: --hostdev {}", bdf);
        }

        let output = Command::new("virt-install")
            .args(&virt_args)
            .output()
            .map_err(|e| {
                spawn_release_pool_ip_if_needed(from_pool, &allocated_ip, &self.ip_pool);
                crate::Error::Backend(format!("Failed to create VM: {}", e))
            })?;

        if !output.status.success() {
            release_pool_ip_if_needed(from_pool, &allocated_ip, &self.ip_pool).await;
            return Err(crate::Error::Backend(format!(
                "Failed to create VM: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        info!("  VM created successfully, discovering DHCP IP...");

        // Evolution #12: Discover actual DHCP-assigned IP
        use crate::backend::libvirt::dhcp_discovery::{DiscoveryConfig, discover_dhcp_ip};
        let dhcp_config = DiscoveryConfig {
            max_wait_secs: 60,
            retry_interval_secs: 2,
            network_name: "default",
        };

        let actual_ip = discover_dhcp_ip(&mac_address, dhcp_config)
            .await
            .map_err(|e| {
                spawn_release_pool_ip_if_needed(from_pool, &allocated_ip, &self.ip_pool);
                crate::Error::Backend(format!("VM created but DHCP IP discovery failed: {}", e))
            })?;

        // Release the pool-allocated IP since VM is using DHCP
        if from_pool {
            release_pool_ip_if_needed(from_pool, &allocated_ip, &self.ip_pool).await;
            info!(
                "  Released pool IP {} (VM using DHCP IP {} instead)",
                allocated_ip, actual_ip
            );
        }

        // 5. Return NodeInfo with discovered DHCP IP
        let node_meta = desktop_dhcp_node_metadata(&mac_address);

        Ok(NodeInfo {
            id: name.to_string(),
            name: name.to_string(),
            container_id: name.to_string(),
            ip_address: actual_ip,
            network: "default".to_string(),
            status: NodeStatus::Running,
            metadata: node_meta,
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
        // Phase 1A: Proper error handling for path conversion
        let template_path_str = template_path.to_str().ok_or_else(|| {
            crate::Error::Backend(format!(
                "Invalid template path (non-UTF8): {:?}",
                template_path
            ))
        })?;

        let output = Command::new("qemu-img")
            .args([
                "create",
                "-f",
                "qcow2",
                "-F",
                "qcow2",
                "-b",
                template_path_str,
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

        let output = Command::new("virt-install")
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

            // Phase 1A: Proper error handling for path conversion
            let intermediate_path_str = intermediate_path.to_str().ok_or_else(|| {
                crate::Error::Backend(format!(
                    "Invalid intermediate path (non-UTF8): {:?}",
                    intermediate_path
                ))
            })?;

            let output = Command::new("cp")
                .args([disk_path_str, intermediate_path_str])
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
