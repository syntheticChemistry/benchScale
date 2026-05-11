// SPDX-License-Identifier: AGPL-3.0-only
//! VM lifecycle operations for LibvirtBackend
//!
//! This module contains VM creation functions that orchestrate the complete
//! lifecycle of creating VMs from various sources (cloud images, templates).
//!
//! All VM creation uses pure libvirt XML generation + `Domain::define_xml` +
//! `Domain::create()`. No shell-outs to `virt-install` or `virsh`.

use crate::backend::{Backend, NodeInfo, NodeStatus};
use crate::backend::vm_utils::{generate_desktop_domain_xml, DesktopDomainConfig};
use crate::Result;
use std::collections::HashMap;
use std::process::Command;
use std::time::Duration;
use tracing::{info, warn};

use virt::domain::Domain;
use virt::sys;

use super::LibvirtBackend;

impl LibvirtBackend {
    /// Create a VM from a registered template
    ///
    /// Wrapper around create_from_template() that looks up the template by name
    /// from the registry populated during backend initialization.
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
    /// This creates a full desktop environment VM suitable for GUI applications.
    /// Uses pure libvirt XML generation — no `virt-install` dependency.
    ///
    /// # Features
    /// - **Static IP allocation** from IP pool (eliminates DHCP race conditions)
    /// - **Automatic cleanup** of existing VMs with same name
    /// - **Cloud-init provisioning** with network configuration
    /// - **Disk resizing** to requested size
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
        self.create_desktop_vm_with_pci(name, base_image, cloud_init, memory_mb, vcpus, disk_size_gb, static_ip, &[]).await
    }

    /// Create a desktop VM with optional PCI passthrough devices.
    ///
    /// Uses pure libvirt XML generation + `Domain::define_xml` + `Domain::create()`.
    /// Hot-attach devices are attached via `Domain::attach_device_flags` after boot.
    pub async fn create_desktop_vm_with_pci(
        &self,
        name: &str,
        base_image: &std::path::Path,
        cloud_init: &crate::CloudInit,
        memory_mb: u32,
        vcpus: u32,
        disk_size_gb: u32,
        static_ip: Option<String>,
        pci_devices: &[crate::backend::gpu_lifecycle::VfioPassthrough],
    ) -> Result<NodeInfo> {
        info!("Creating desktop VM: {}", name);

        self.ensure_healthy().await?;

        // Clean up existing VM with the same name
        if let Ok(_existing) = self.get_node(name).await {
            warn!(
                "VM '{}' already exists, cleaning up before creating new one...",
                name
            );
            if let Err(e) = self.delete_node(name).await {
                warn!(
                    "Cleanup of existing VM '{}' failed: {}. Continuing anyway...",
                    name, e
                );
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        // 1. Allocate static IP
        let (allocated_ip, from_pool) = if let Some(requested_ip) = static_ip {
            info!("  Using requested static IP {} for VM {}", requested_ip, name);
            (requested_ip, false)
        } else {
            let ip = self.ip_pool.allocate().await?;
            info!("  Allocated static IP {} from pool for VM {}", ip, name);
            (ip.to_string(), true)
        };

        // 2. Create disk from base image
        let disk_path = self
            .capabilities
            .storage
            .images_dir
            .join(format!("{}.qcow2", name));
        let disk_path_str = disk_path
            .to_str()
            .ok_or_else(|| crate::Error::Backend("Invalid disk path".to_string()))?;

        let base_image_str = base_image
            .to_str()
            .ok_or_else(|| crate::Error::Backend(format!("Invalid base image path (non-UTF8): {:?}", base_image)))?;

        info!("  Copying base image to {}", disk_path_str);
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

        // 3. Generate cloud-init ISO
        let iso_path = self.generate_cloud_init_iso(name, cloud_init, &allocated_ip, from_pool).await?;
        let iso_path_str = iso_path
            .to_str()
            .ok_or_else(|| crate::Error::Backend("Invalid ISO path".to_string()))?;

        // 4. Generate deterministic MAC address for DHCP discovery
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        name.hash(&mut hasher);
        let hash = hasher.finish();
        let mac_address = format!(
            "52:54:00:{:02x}:{:02x}:{:02x}",
            (hash >> 16) & 0xFF,
            (hash >> 8) & 0xFF,
            hash & 0xFF
        );
        info!("  Generated MAC address: {} (for DHCP discovery)", mac_address);

        // 5. Generate domain XML and define via libvirt API
        use crate::backend::gpu_lifecycle::AttachMode;
        let (cold_devs, hot_devs): (Vec<_>, Vec<_>) =
            pci_devices.iter().partition(|d| d.attach_mode == AttachMode::Cold);

        for dev in &cold_devs {
            info!("  PCI passthrough (cold-attach): {}", dev.device.bdf);
        }
        for dev in &hot_devs {
            info!("  PCI passthrough (deferred hot-attach, mode={:?}): {}", dev.attach_mode, dev.device.bdf);
        }

        let domain_config = DesktopDomainConfig {
            name,
            disk_path: &disk_path,
            cdrom_path: Some(iso_path.as_path()),
            memory_mb,
            vcpus,
            network: "default",
            mac_address: Some(&mac_address),
            pci_devices,
            emulator: None,
        };

        let domain_xml = generate_desktop_domain_xml(&domain_config);

        info!("  Defining VM in libvirt (pure XML, no virt-install)");
        let conn = self.conn.lock().await;
        let domain = Domain::define_xml(&conn, &domain_xml)
            .map_err(|e| {
                release_ip_sync(&self.ip_pool, &allocated_ip, from_pool);
                crate::Error::Backend(format!("Failed to define domain: {}", e))
            })?;

        domain.create().map_err(|e| {
            let _ = domain.undefine();
            release_ip_sync(&self.ip_pool, &allocated_ip, from_pool);
            crate::Error::Backend(format!("Failed to start domain: {}", e))
        })?;

        info!("  VM created successfully via libvirt API");

        // 6. Hot-attach devices via Domain::attach_device_flags
        for dev in &hot_devs {
            if let Some(xml) = dev.to_libvirt_xml() {
                info!("  Hot-attaching device {} ({:?}) ...", dev.device.bdf, dev.attach_mode);
                let flags = sys::VIR_DOMAIN_AFFECT_LIVE | sys::VIR_DOMAIN_AFFECT_CONFIG;
                match domain.attach_device_flags(&xml, flags) {
                    Ok(_) => info!("  Device {} attached", dev.device.bdf),
                    Err(e) => warn!("  attach_device for {} failed: {}", dev.device.bdf, e),
                }
            } else {
                warn!("  Skipping device {} (invalid BDF)", dev.device.bdf);
            }
        }

        drop(conn);

        // 7. Discover DHCP IP
        info!("  Discovering DHCP IP...");
        use crate::backend::libvirt::dhcp_discovery::{discover_dhcp_ip, DiscoveryConfig};
        let dhcp_config = DiscoveryConfig {
            max_wait_secs: 60,
            retry_interval_secs: 2,
            network_name: "default",
        };

        let actual_ip = discover_dhcp_ip(&mac_address, dhcp_config)
            .await
            .map_err(|e| {
                release_ip_sync(&self.ip_pool, &allocated_ip, from_pool);
                crate::Error::Backend(format!(
                    "VM created but DHCP IP discovery failed: {}", e
                ))
            })?;

        if from_pool {
            if let Ok(ip_addr) = allocated_ip.parse::<std::net::Ipv4Addr>() {
                self.ip_pool.release(ip_addr).await;
                info!("  Released pool IP {} (VM using DHCP IP {} instead)", allocated_ip, actual_ip);
            }
        }

        // 8. Return NodeInfo
        let mut metadata = HashMap::new();
        metadata.insert("mac_address".to_string(), mac_address.clone());
        metadata.insert("dhcp_mode".to_string(), "true".to_string());
        metadata.insert("iso_path".to_string(), iso_path_str.to_string());

        Ok(NodeInfo {
            id: name.to_string(),
            name: name.to_string(),
            container_id: name.to_string(),
            ip_address: actual_ip,
            network: "default".to_string(),
            status: NodeStatus::Running,
            metadata,
        })
    }

    /// Create a VM from a pre-built template image.
    ///
    /// Uses pure libvirt XML generation — no `virt-install` dependency.
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

        // 1. Create CoW disk from template
        let disk_path = self
            .capabilities
            .storage
            .images_dir
            .join(format!("{}.qcow2", name));
        let disk_path_str = disk_path
            .to_str()
            .ok_or_else(|| crate::Error::Backend("Invalid disk path".to_string()))?;

        let template_path_str = template_path
            .to_str()
            .ok_or_else(|| crate::Error::Backend(format!("Invalid template path (non-UTF8): {:?}", template_path)))?;

        info!("  Creating CoW disk from template");
        let output = Command::new("qemu-img")
            .args(["create", "-f", "qcow2", "-F", "qcow2", "-b", template_path_str, disk_path_str])
            .output()
            .map_err(|e| crate::Error::Backend(format!("Failed to create CoW disk: {}", e)))?;

        if !output.status.success() {
            return Err(crate::Error::Backend(format!(
                "Failed to create CoW disk: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        // 2. Handle cloud-init: create ISO if provided
        let iso_path = if let Some(ci) = cloud_init {
            info!("  Generating cloud-init for customization");
            Some(self.generate_cloud_init_iso(name, ci, "dhcp", false).await?)
        } else {
            None
        };

        // 3. Generate domain XML and define via libvirt API
        let domain_config = DesktopDomainConfig {
            name,
            disk_path: &disk_path,
            cdrom_path: iso_path.as_deref(),
            memory_mb,
            vcpus,
            network: "default",
            mac_address: None,
            pci_devices: &[],
            emulator: None,
        };

        let domain_xml = generate_desktop_domain_xml(&domain_config);

        info!("  Defining VM in libvirt (pure XML, no virt-install)");
        let conn = self.conn.lock().await;
        let domain = Domain::define_xml(&conn, &domain_xml)
            .map_err(|e| crate::Error::Backend(format!("Failed to define domain: {}", e)))?;

        domain
            .create()
            .map_err(|e| crate::Error::Backend(format!("Failed to start domain: {}", e)))?;

        drop(conn);

        info!("  VM created, waiting for network");

        // 4. Wait for IP address
        let timeout = Duration::from_secs(self.config.vm_ip_timeout_secs);
        let ip_address = self.wait_for_ip(name, timeout).await?;

        info!("  VM got IP: {}", ip_address);

        // 5. Save intermediate if requested
        if save_intermediate {
            info!("  Saving intermediate snapshot");
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

            let intermediate_path_str = intermediate_path
                .to_str()
                .ok_or_else(|| crate::Error::Backend(format!("Invalid intermediate path (non-UTF8): {:?}", intermediate_path)))?;

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

    /// Generate a cloud-init NoCloud ISO from a CloudInit config.
    ///
    /// Creates user-data and meta-data files, then produces a cidata ISO
    /// using pure Rust (`hadris-iso`). No external `genisoimage` dependency.
    async fn generate_cloud_init_iso(
        &self,
        name: &str,
        cloud_init: &crate::CloudInit,
        allocated_ip: &str,
        from_pool: bool,
    ) -> Result<std::path::PathBuf> {
        let mut cloud_init_with_ip = cloud_init.clone();

        if allocated_ip != "dhcp" {
            cloud_init_with_ip.network_config = Some(crate::cloud_init::NetworkConfig::new(
                &self.capabilities.network.default_interface,
                format!("{}/{}", allocated_ip, self.capabilities.network.netmask_bits),
                &self.capabilities.network.gateway,
            ));
        }

        let user_data = match cloud_init_with_ip.to_user_data() {
            Ok(data) => data,
            Err(e) => {
                if from_pool {
                    if let Ok(ip_addr) = allocated_ip.parse::<std::net::Ipv4Addr>() {
                        self.ip_pool.release(ip_addr).await;
                    }
                }
                return Err(crate::Error::Backend(format!(
                    "Failed to generate cloud-init: {}", e
                )));
            }
        };

        let meta_data = format!("instance-id: {}\nlocal-hostname: {}\n", name, name);

        let iso_path = self
            .capabilities
            .storage
            .images_dir
            .join(format!("{}-cidata.iso", name));

        info!("  Creating cloud-init ISO via pure Rust (no genisoimage)");
        crate::backend::cidata_iso::generate_nocloud_iso(
            &iso_path,
            user_data.as_bytes(),
            meta_data.as_bytes(),
        )
        .map_err(|e| crate::Error::Backend(format!("Failed to create cloud-init ISO: {}", e)))?;

        Ok(iso_path)
    }
}

/// Release an IP back to the pool (fire-and-forget for error paths).
fn release_ip_sync(pool: &crate::backend::IpPool, ip_str: &str, from_pool: bool) {
    if from_pool {
        if let Ok(ip_addr) = ip_str.parse::<std::net::Ipv4Addr>() {
            let pool = pool.clone();
            tokio::spawn(async move {
                pool.release(ip_addr).await;
            });
        }
    }
}
