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

use crate::backend::{Backend, ExecResult, NetworkInfo, NodeInfo, NodeStatus};
use crate::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{info, warn};

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
///
/// ## IP Address Management
///
/// Uses a deterministic IP pool to eliminate DHCP race conditions when creating
/// multiple VMs rapidly. Each VM gets a pre-allocated unique IP address configured
/// via cloud-init, ensuring no conflicts regardless of creation speed.
#[cfg(feature = "libvirt")]
pub struct LibvirtBackend {
    conn: Arc<Mutex<Connect>>,
    config: crate::config::LibvirtConfig,
    ip_pool: crate::backend::IpPool,
    templates: HashMap<String, std::path::PathBuf>,
}

#[cfg(feature = "libvirt")]
impl LibvirtBackend {
    /// Create a new LibvirtBackend with default configuration
    pub fn new() -> Result<Self> {
        let mut backend = Self::with_config(crate::config::LibvirtConfig::default())?;
        
        // Auto-discover templates if template_dir is configured
        if backend.config.template_dir.is_some() {
            if let Err(e) = backend.discover_templates() {
                warn!("Failed to auto-discover templates: {}", e);
            }
        }
        
        Ok(backend)
    }

    /// Create a new LibvirtBackend with custom configuration
    pub fn with_config(config: crate::config::LibvirtConfig) -> Result<Self> {
        let conn = Connect::open(Some(&config.uri))
            .map_err(|e| crate::Error::Backend(format!("Failed to connect to libvirt: {}", e)))?;

        // Initialize IP pool for deterministic IP assignment
        let ip_pool = crate::backend::IpPool::default_libvirt();

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            config,
            ip_pool,
            templates: HashMap::new(),
        })
    }

    // ========================================================================
    // Template Management API
    // ========================================================================

    /// Register a template with a friendly name
    ///
    /// Templates allow you to create VMs from pre-configured base images
    /// (e.g., from agentReagents) using friendly names instead of full paths.
    ///
    /// # Arguments
    /// * `name` - Friendly name for the template (e.g., "rustdesk-ubuntu-22.04")
    /// * `path` - Full path to the template qcow2 file
    ///
    /// # Example
    /// ```no_run
    /// # use benchscale::LibvirtBackend;
    /// # use std::path::PathBuf;
    /// # fn example() -> anyhow::Result<()> {
    /// let mut backend = LibvirtBackend::new()?;
    /// backend.register_template(
    ///     "my-template",
    ///     PathBuf::from("/path/to/template.qcow2")
    /// )?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn register_template(&mut self, name: impl Into<String>, path: std::path::PathBuf) -> Result<()> {
        let name = name.into();
        
        if !path.exists() {
            return Err(crate::Error::Backend(format!(
                "Template path does not exist: {:?}",
                path
            )));
        }
        
        if path.extension().and_then(|s| s.to_str()) != Some("qcow2") {
            warn!("Template {:?} does not have .qcow2 extension", path);
        }
        
        info!("Registered template '{}' -> {:?}", name, path);
        self.templates.insert(name, path);
        Ok(())
    }

    /// Discover templates from the configured template directory
    ///
    /// Scans the template directory (from config or BENCHSCALE_TEMPLATE_DIR)
    /// and registers all .qcow2 files as templates.
    ///
    /// # Returns
    /// Number of templates discovered
    ///
    /// # Example
    /// ```no_run
    /// # use benchscale::LibvirtBackend;
    /// # fn example() -> anyhow::Result<()> {
    /// let mut backend = LibvirtBackend::new()?;
    /// let count = backend.discover_templates()?;
    /// println!("Discovered {} templates", count);
    /// # Ok(())
    /// # }
    /// ```
    pub fn discover_templates(&mut self) -> Result<usize> {
        let template_dir = self.config.template_dir
            .as_ref()
            .ok_or_else(|| crate::Error::Backend(
                "No template directory configured. Set BENCHSCALE_TEMPLATE_DIR or ensure agentReagents is in a standard location.".to_string()
            ))?;

        if !template_dir.exists() {
            return Err(crate::Error::Backend(format!(
                "Template directory does not exist: {:?}",
                template_dir
            )));
        }

        let mut count = 0;
        for entry in std::fs::read_dir(template_dir)? {
            let entry = entry?;
            let path = entry.path();

            // Only register .qcow2 files
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("qcow2") {
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    self.templates.insert(name.to_string(), path);
                    count += 1;
                }
            }
        }

        info!("Discovered {} templates from {:?}", count, template_dir);
        Ok(count)
    }

    /// List all registered template names
    ///
    /// Returns a sorted list of template names that can be used with
    /// `create_from_registered_template()`.
    ///
    /// # Example
    /// ```no_run
    /// # use benchscale::LibvirtBackend;
    /// # fn example() -> anyhow::Result<()> {
    /// let backend = LibvirtBackend::new()?;
    /// for template in backend.list_templates() {
    ///     println!("  - {}", template);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn list_templates(&self) -> Vec<String> {
        let mut names: Vec<_> = self.templates.keys().cloned().collect();
        names.sort();
        names
    }

    /// Get the path for a registered template
    ///
    /// # Arguments
    /// * `name` - Template name (as registered or discovered)
    ///
    /// # Returns
    /// Path to the template file
    ///
    /// # Errors
    /// Returns error if template is not registered
    pub fn get_template_path(&self, name: &str) -> Result<&std::path::PathBuf> {
        self.templates.get(name)
            .ok_or_else(|| crate::Error::Backend(format!(
                "Template '{}' not registered. Available templates: {:?}",
                name, self.list_templates()
            )))
    }

    /// Create VM from a registered template by name
    ///
    /// This is a convenience method that looks up the template path and calls
    /// `create_from_template()`. Templates can be registered manually via
    /// `register_template()` or auto-discovered via `discover_templates()`.
    ///
    /// # Arguments
    /// * `vm_name` - Name for the new VM
    /// * `template_name` - Name of the registered template
    /// * `cloud_init` - Optional cloud-init configuration
    /// * `memory_mb` - RAM in megabytes
    /// * `vcpus` - Number of virtual CPUs
    ///
    /// # Example
    /// ```no_run
    /// # use benchscale::{LibvirtBackend, CloudInit};
    /// # async fn example() -> anyhow::Result<()> {
    /// let backend = LibvirtBackend::new()?;
    ///
    /// // Templates auto-discovered from agentReagents
    /// let vm = backend.create_from_registered_template(
    ///     "my-vm",
    ///     "rustdesk-ubuntu-22.04-template",
    ///     None,
    ///     2048,
    ///     2
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create_from_registered_template(
        &self,
        vm_name: &str,
        template_name: &str,
        cloud_init: Option<&crate::CloudInit>,
        memory_mb: u32,
        vcpus: u32,
    ) -> Result<NodeInfo> {
        let template_path = self.get_template_path(template_name)?;
        
        info!("Creating VM '{}' from template '{}'", vm_name, template_name);
        
        self.create_from_template(
            vm_name,
            template_path,
            cloud_init,
            memory_mb,
            vcpus,
            false  // save_intermediate
        ).await
    }

    // ========================================================================
    // VM Creation Methods
    // ========================================================================

    /// Create a desktop VM with cloud-init support
    ///
    /// This creates a full desktop environment VM suitable for GUI applications
    /// like RustDesk. The VM will have a desktop environment installed via cloud-init.
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
    /// `NodeInfo` with the VM's details including IP address once available
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
    ///     .add_user("iontest", "ssh-rsa AAAAB3...")
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
        use std::process::Command;

        info!("Creating desktop VM: {}", name);

        // 1. Allocate static IP from pool (eliminates DHCP race conditions)
        let static_ip = self.ip_pool.allocate().await?;
        info!("  Allocated static IP {} for VM {}", static_ip, name);

        // 2. Create disk from base image
        let disk_path = format!("/var/lib/libvirt/images/{}.qcow2", name);

        info!("  Copying base image to {}", disk_path);
        let output = Command::new("sudo")
            .args(["cp", base_image.to_str().unwrap(), &disk_path])
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
                &disk_path,
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
        
        // Add static IP network configuration for default libvirt network (192.168.122.0/24)
        cloud_init_with_ip.network_config = Some(crate::cloud_init::NetworkConfig::new(
            "enp1s0",                    // Common virtio interface name
            format!("{}/24", static_ip), // IP with /24 netmask
            "192.168.122.1"              // Default libvirt gateway
        ));
        
        let user_data = cloud_init_with_ip
            .to_user_data()
            .map_err(|e| {
                // Release IP on failure
                let ip = static_ip;
                tokio::spawn(async move {
                    // Best-effort release (ignore errors since this is cleanup)
                    let _ = self.ip_pool.release(ip).await;
                });
                crate::Error::Backend(format!("Failed to generate cloud-init: {}", e))
            })?;

        let user_data_path = format!("/tmp/user-data-{}", name);
        std::fs::write(&user_data_path, user_data)
            .map_err(|e| {
                // Release IP on failure
                let ip = static_ip;
                let pool = self.ip_pool.clone();
                tokio::spawn(async move {
                    let _ = pool.release(ip).await;
                });
                crate::Error::Backend(format!("Failed to write user-data: {}", e))
            })?;

        // Create meta-data
        let meta_data = format!("instance-id: {}\nlocal-hostname: {}\n", name, name);
        let meta_data_path = format!("/tmp/meta-data-{}", name);
        std::fs::write(&meta_data_path, meta_data)
            .map_err(|e| crate::Error::Backend(format!("Failed to write meta-data: {}", e)))?;

        // Create ISO
        let iso_path = format!("/var/lib/libvirt/images/{}-cidata.iso", name);
        info!("  Creating cloud-init ISO");
        let output = Command::new("sudo")
            .args([
                "genisoimage",
                "-output",
                &iso_path,
                "-volid",
                "cidata",
                "-joliet",
                "-rock",
                &user_data_path,
                &meta_data_path,
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
                &format!("path={},format=qcow2", disk_path),
                "--disk",
                &format!("path={},device=cdrom", iso_path),
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
            // Release IP on failure
            self.ip_pool.release(static_ip).await?;
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
        use std::process::Command;

        info!("Creating VM from template: {}", name);
        info!("  Template: {}", template_path.display());

        // 1. Create disk from template using CoW (copy-on-write)
        let disk_path = format!("/var/lib/libvirt/images/{}.qcow2", name);

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
                &disk_path,
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

            // Write to temp files
            let user_data_path = format!("/tmp/user-data-{}", name);
            let meta_data_path = format!("/tmp/meta-data-{}", name);

            std::fs::write(&user_data_path, user_data)
                .map_err(|e| crate::Error::Backend(format!("Failed to write user-data: {}", e)))?;
            std::fs::write(&meta_data_path, meta_data)
                .map_err(|e| crate::Error::Backend(format!("Failed to write meta-data: {}", e)))?;

            // Use virt-install's built-in cloud-init support (more reliable than ISO)
            cloud_init_args = vec![
                "--cloud-init".to_string(),
                format!("user-data={},meta-data={}", user_data_path, meta_data_path),
            ];
        }

        // 3. Define and start VM
        info!("  Defining VM in libvirt");

        // Prepare arguments (must live long enough)
        let memory_str = memory_mb.to_string();
        let vcpus_str = vcpus.to_string();
        let disk_arg = format!("path={},format=qcow2", disk_path);

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
                .args(["cp", &disk_path, intermediate_path.to_str().unwrap()])
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

    /// Wait for VM to get an IP address
    async fn wait_for_ip(&self, name: &str, timeout: Duration) -> Result<String> {
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
    /// backend.wait_for_cloud_init(&node.id, "iontest", "iontest123", Duration::from_secs(600)).await?;
    /// // Now safe to use SSH
    /// # Ok(())
    /// # }
    /// ```
    pub async fn wait_for_cloud_init(
        &self,
        node_id: &str,
        username: &str,
        password: &str,
        timeout: Duration,
    ) -> Result<()> {
        use std::time::Instant;

        info!("Waiting for cloud-init to complete on VM: {}", node_id);
        let start = Instant::now();
        let ip = self.get_vm_ip_by_name(node_id).await?;

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
    /// backend.wait_for_ssh(&node.ip_address, "iontest", "iontest123", Duration::from_secs(300)).await?;
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
    /// **This should become the recommended API** as it ensures the VM is fully
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
    ///     .add_user("iontest", "ssh-rsa AAAAB3...")
    ///     .password("iontest", "iontest123")
    ///     .package("ubuntu-desktop-minimal")
    ///     .build();
    ///
    /// let node = backend.create_desktop_vm_ready(
    ///     "my-vm",
    ///     Path::new("/path/to/ubuntu-22.04.img"),
    ///     &cloud_init,
    ///     3072, 2, 25,
    ///     "iontest",
    ///     "iontest123",
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

        // Wait for cloud-init to complete
        info!(
            "Waiting for cloud-init to complete (timeout: {}s)...",
            timeout.as_secs()
        );
        self.wait_for_cloud_init(&node.id, username, password, timeout)
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
    ///     Path::new("../agentReagents/images/templates/popos-cosmic-rustdesk-template.qcow2"),
    ///     None,
    ///     3072, 2,
    ///     false,
    ///     "iontest",
    ///     "iontest123",
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
            self.wait_for_cloud_init(&node.id, username, password, timeout)
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

    /// Get VM IP address by domain name
    async fn get_vm_ip_by_name(&self, name: &str) -> Result<String> {
        // Use virsh command to get IP (simpler than libvirt API for this)
        let output = tokio::process::Command::new("virsh")
            .args(["domifaddr", name, "--source", "lease"])
            .output()
            .await
            .map_err(|e| crate::Error::Backend(format!("Failed to run virsh: {}", e)))?;

        if !output.status.success() {
            return Err(crate::Error::Backend(format!(
                "virsh domifaddr failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
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

        Err(crate::Error::Backend(
            "No IP address found for VM".to_string(),
        ))
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
        use super::vm_utils::{generate_domain_xml, parse_memory, DiskManager};

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
        use super::vm_utils::DiskManager;
        use std::net::Ipv4Addr;
        use std::str::FromStr;

        info!("Deleting VM: {}", node_id);

        // Get node info first to clean up disk overlay and release IP
        let node_info = self.get_node(node_id).await.ok();
        let node_name = node_info.as_ref().map(|n| n.name.as_str());
        
        // Release IP back to pool if it's a valid IPv4 address
        if let Some(ref info) = node_info {
            if let Ok(ip) = Ipv4Addr::from_str(&info.ip_address) {
                if let Err(e) = self.ip_pool.release(ip).await {
                    warn!("Failed to release IP {} for VM {}: {}", ip, node_id, e);
                } else {
                    info!("Released IP {} back to pool", ip);
                }
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

#[cfg(feature = "libvirt")]
impl Default for LibvirtBackend {
    fn default() -> Self {
        Self::new().expect("Failed to create LibvirtBackend")
    }
}

#[cfg(all(test, feature = "libvirt"))]
#[path = "libvirt_validation_tests.rs"]
mod validation_tests;

// Stub implementation when libvirt feature is not enabled
#[cfg(not(feature = "libvirt"))]
pub struct LibvirtBackend;

#[cfg(not(feature = "libvirt"))]
impl LibvirtBackend {
    pub fn new() -> Result<Self> {
        Err(crate::Error::Backend(
            "LibvirtBackend requires 'libvirt' feature to be enabled".to_string(),
        ))
    }
}
