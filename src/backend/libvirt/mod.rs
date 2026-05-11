// SPDX-License-Identifier: AGPL-3.0-only
#![cfg_attr(feature = "libvirt", allow(unsafe_code))]

//! LibvirtBackend - KVM/QEMU backend for benchScale
//!
//! This backend allows benchScale to work with libvirt-managed VMs
//! instead of Docker containers, making it suitable for testing systems
//! that require full VMs (like ionChannel, BiomeOS, or any OS-level testing).
//!
//! ## Features
//!
//! - **Full VM creation and management** via libvirt/KVM
//! - **Cloud-init support** for automated VM provisioning  
//! - **DHCP lease discovery** for dynamic, fractal-scalable IP allocation (Evolution #12)
//! - **Template support** for fast VM creation from pre-built images
//! - **Readiness validation** ensures VMs are fully operational before returning
//! - **SSH-based operations** for remote execution and file transfer
//! - **Network isolation** and simulation
//!
//! ## Architecture
//!
//! The LibvirtBackend is organized into functional modules:
//!
//! - **`mod.rs`** (this file) - Core struct, initialization, template discovery
//! - **`vm_lifecycle.rs`** - VM creation operations (desktop VMs, templates)
//! - **`vm_ready.rs`** - Readiness validation (cloud-init, SSH waiting)
//! - **`dhcp_discovery.rs`** - DHCP lease discovery for dynamic IPs (Evolution #12)
//! - **`boot_diagnostics.rs`** - Deep boot failure diagnostics (Evolution #13)
//! - **`health_check.rs`** - Libvirt system health checks (Evolution #20)
//! - **`recovery.rs`** - Automatic recovery from libvirt state corruption (Evolution #20)
//! - **`vm_guard.rs`** - RAII-based VM cleanup to prevent orphaned VMs
//! - **`vm_registry.rs`** - VM tracking and orphan detection
//! - **`backend_impl.rs`** - Backend trait implementation
//! - **`utils.rs`** - Utility functions (IP discovery, template management)
//!
//! This organization follows **functional cohesion** principles, grouping code
//! by purpose rather than arbitrary line counts.
//!
//! ## Usage
//!
//! ### Basic VM Creation
//!
//! ```rust,no_run
//! use benchscale::{LibvirtBackend, CloudInit};
//! use std::path::Path;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let backend = LibvirtBackend::new()?;
//!
//! let cloud_init = CloudInit::builder()
//!     .add_user("ubuntu", "ssh-rsa AAAAB3...")
//!     .package("ubuntu-desktop-minimal")
//!     .build();
//!
//! let vm = backend.create_desktop_vm(
//!     "my-vm",
//!     Path::new("/path/to/ubuntu-22.04.img"),
//!     &cloud_init,
//!     3072,  // 3GB RAM
//!     2,     // 2 vCPUs  
//!     25,    // 25GB disk
//!     None,
//! ).await?;
//!
//! println!("VM created at {}", vm.ip_address);
//! # Ok(())
//! # }
//! ```
//!
//! ### Recommended: Use Ready Variants
//!
//! ```rust,no_run
//! use benchscale::{LibvirtBackend, CloudInit};
//! use std::path::Path;
//! use std::time::Duration;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let backend = LibvirtBackend::new()?;
//!
//! let cloud_init = CloudInit::builder()
//!     .add_user("ubuntu", "ssh-rsa AAAAB3...")
//!     .package("ubuntu-desktop-minimal")
//!     .build();
//!
//! // This waits for cloud-init AND SSH to be ready
//! let vm = backend.create_desktop_vm_ready(
//!     "my-vm",
//!     Path::new("/path/to/ubuntu-22.04.img"),
//!     &cloud_init,
//!     3072, 2, 25,
//!     "ubuntu", "password123",
//!     Duration::from_secs(600), // 10 minute timeout
//!     None,
//! ).await?;
//!
//! // SSH is guaranteed to work now!
//! # Ok(())
//! # }
//! ```
//!
//! ### Using Templates
//!
//! ```rust,no_run
//! use benchscale::LibvirtBackend;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let mut backend = LibvirtBackend::new()?;
//!
//! // Auto-discover templates from agentReagents
//! backend.discover_templates()?;
//!
//! // List available templates
//! for template in backend.list_templates() {
//!     println!("Template: {}", template);
//! }
//!
//! // Create VM from template
//! let vm = backend.create_from_registered_template(
//!     "my-vm",
//!     "ubuntu-22.04-template",
//!     None,   // No cloud-init needed
//!     2048,   // 2GB RAM
//!     2,      // 2 vCPUs
//! ).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## IP Address Management
//!
//! LibvirtBackend uses a **deterministic IP pool** to eliminate DHCP race
//! conditions when creating multiple VMs rapidly. Each VM gets a pre-allocated
//! unique IP address configured via cloud-init, ensuring no conflicts regardless
//! of creation speed.
//!
//! Default pool: `192.168.122.10-250` (241 addresses)
//!
//! ## Configuration
//!
//! Configure via environment variables or `Config`:
//!
//! - `BENCHSCALE_LIBVIRT_URI` - Libvirt connection URI (default: "qemu:///system")
//! - `BENCHSCALE_TEMPLATE_DIR` - Template directory path
//! - `BENCHSCALE_VM_IP_TIMEOUT_SECS` - IP acquisition timeout (default: 60s)
//!
//! ## Smart Refactoring
//!
//! This module was refactored from a single 1557-line file into 5 functionally
//! cohesive modules (~300 lines each). Benefits:
//!
//! - **Easy to navigate** - Know exactly which module contains what
//! - **Easy to test** - Test modules in isolation
//! - **Easy to maintain** - Clear responsibilities
//! - **Easy to extend** - Add features without bloating single file

use crate::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

#[cfg(feature = "libvirt")]
use virt::connect::Connect;
#[cfg(feature = "libvirt")]
use virt::domain::Domain;

// Re-export required modules for internal use
#[cfg(feature = "libvirt")]
use super::{ssh, vm_utils};

// Sub-modules (functional organization)
#[cfg(feature = "libvirt")]
mod backend_impl;
#[cfg(feature = "libvirt")]
mod utils;
#[cfg(feature = "libvirt")]
mod vm_lifecycle;
#[cfg(feature = "libvirt")]
mod vm_ready;
#[cfg(feature = "libvirt")]
mod vm_guard;
#[cfg(feature = "libvirt")]
mod vm_registry;
#[cfg(feature = "libvirt")]
mod dhcp_leases;
#[cfg(feature = "libvirt")]
/// DHCP lease discovery for dynamic IP allocation (Evolution #12)
pub mod dhcp_discovery;
#[cfg(feature = "libvirt")]
/// Deep boot diagnostics for failed VM boots (Evolution #13)
pub mod boot_diagnostics;
#[cfg(feature = "libvirt")]
/// Libvirt health check for system stability (Evolution #20)
pub mod health_check;
#[cfg(feature = "libvirt")]
/// Auto-recovery from libvirt state corruption (Evolution #20)
pub mod recovery;

// Public exports
#[cfg(feature = "libvirt")]
pub use vm_guard::VmGuard;
#[cfg(feature = "libvirt")]
pub use vm_registry::{VmRegistry, VmRegistryEntry, VmStatus};

pub(crate) use super::libvirt_uri;

/// LibvirtBackend for KVM/QEMU VMs
///
/// Provides a complete backend for benchScale that uses libvirt to manage VMs.
/// This enables testing scenarios that require full OS environments rather than containers.
///
/// ## Architecture
///
/// The backend is organized into functionally cohesive modules:
/// - **VM Lifecycle** (`vm_lifecycle.rs`) - VM creation operations
/// - **Readiness Validation** (`vm_ready.rs`) - Ensuring VMs are operational
/// - **Backend Interface** (`backend_impl.rs`) - Backend trait implementation
/// - **Utilities** (`utils.rs`) - Template management, IP discovery
///
/// ## Capability-Based Configuration
///
/// LibvirtBackend discovers system capabilities at runtime rather than using
/// hardcoded values. This makes it portable across different libvirt configurations.
///
/// ## Example
///
/// ```no_run
/// use benchscale::LibvirtBackend;
///
/// # async fn example() -> anyhow::Result<()> {
/// let backend = LibvirtBackend::new()?;
/// // Backend is ready to create VMs
/// # Ok(())
/// # }
/// ```
#[cfg(feature = "libvirt")]
pub struct LibvirtBackend {
    /// Libvirt connection (wrapped in Arc<Mutex> for async safety)
    pub(crate) conn: Arc<Mutex<Connect>>,

    /// Unified configuration
    pub(crate) bench_config: crate::config::BenchScaleConfig,

    /// Runtime-discovered system capabilities
    pub(crate) capabilities: crate::capabilities::SystemCapabilities,

    /// IP pool for deterministic IP allocation
    pub(crate) ip_pool: crate::backend::IpPool,

    /// Registered templates (name -> path mapping)
    pub(crate) templates: HashMap<String, PathBuf>,
}

#[cfg(feature = "libvirt")]
impl LibvirtBackend {
    #[allow(deprecated)]
    fn bench_scale_config_from_libvirt_legacy(
        lc: &crate::config_legacy::LibvirtConfig,
    ) -> crate::config::BenchScaleConfig {
        let mut bench = crate::config::BenchScaleConfig::default();
        bench.timeouts.vm_boot_secs = lc.vm_ip_timeout_secs;
        bench.timeouts.ssh_connection_secs = lc.ssh.timeout_secs;
        bench.network.ssh_port = lc.ssh.port;
        bench.network.ssh_default_user = lc.ssh.default_user.clone();
        bench.storage.intermediate_dir = lc.template_dir.clone();
        bench.storage.overlay_dir = Some(lc.overlay_dir.clone());
        let vm_images = if lc.base_image_path.is_dir() {
            lc.base_image_path.clone()
        } else {
            lc.base_image_path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(crate::constants::paths::default_system_vm_images_dir)
        };
        bench.storage.vm_images_dir = Some(vm_images);
        bench
    }

    fn new_connected(
        conn: Connect,
        bench_config: crate::config::BenchScaleConfig,
        capabilities: crate::capabilities::SystemCapabilities,
    ) -> Result<Self> {
        let ip_pool = crate::backend::IpPool::from_range(
            &capabilities.network.ip_pool_start,
            &capabilities.network.ip_pool_end,
        )?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            bench_config,
            capabilities,
            ip_pool,
            templates: HashMap::new(),
        })
    }

    /// Get a raw libvirt connection for VmGuard or other direct libvirt operations
    ///
    /// This creates a new connection to libvirt (not from the Arc<Mutex> pool)
    /// for use cases that need direct ownership of a Connect instance.
    pub fn raw_connection(&self) -> Result<Connect> {
        Connect::open(Some(&super::libvirt_uri()))
            .map_err(|e| crate::Error::Backend(format!("Failed to connect to libvirt: {}", e)))
    }

    /// Get runtime-discovered system capabilities
    ///
    /// Deep debt solution: Exposes capability discovery to dependent crates.
    /// Code should discover system configuration at runtime instead of hardcoding.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use benchscale::LibvirtBackend;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let backend = LibvirtBackend::new()?;
    /// let images_dir = &backend.capabilities().storage.images_dir;
    /// let gateway = &backend.capabilities().network.gateway;
    /// # Ok(())
    /// # }
    /// ```
    pub fn capabilities(&self) -> &crate::capabilities::SystemCapabilities {
        &self.capabilities
    }

    /// Ensure libvirt infrastructure is healthy (Evolution #20)
    ///
    /// Checks libvirt system health and attempts automatic recovery if needed.
    /// This prevents VM operations from failing due to infrastructure corruption
    /// (orphaned processes, network issues, etc).
    ///
    /// Called automatically by `new()` and before critical VM operations.
    /// Can also be called manually to verify/restore infrastructure health.
    ///
    /// # Returns
    /// - `Ok(())` if system is healthy or recovery succeeded
    /// - `Err(...)` if system is unhealthy and recovery failed
    ///
    /// # Example
    ///
    /// ```no_run
    /// use benchscale::LibvirtBackend;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let backend = LibvirtBackend::new()?;
    /// 
    /// // Before critical operations
    /// backend.ensure_healthy().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn ensure_healthy(&self) -> Result<()> {
        use crate::backend::libvirt::health_check::{HealthState, LibvirtHealthCheck};
        use crate::backend::libvirt::recovery::LibvirtRecovery;

        // Perform health check
        let checker = LibvirtHealthCheck::new();
        let health = checker
            .check()
            .await
            .map_err(|e| crate::Error::Backend(format!("Health check failed: {}", e)))?;

        // GRACEFUL DEGRADATION: Handle each state appropriately
        match &health.overall {
            HealthState::Healthy => {
                debug!("✅ libvirt infrastructure is healthy");
                Ok(())
            }

            HealthState::Degraded(reason) => {
                // WARN but DON'T BLOCK
                // Degraded state means system works but has non-critical issues
                warn!("⚠️  Infrastructure degraded: {}", reason);
                warn!("   System is functional but may need attention:");
                for issue in &health.issues {
                    warn!("   • {}", issue);
                }
                
                // Provide recovery instructions without blocking
                if !health.orphaned_processes.is_empty() {
                    warn!("   💡 To clean up orphaned processes (no sudo needed):");
                    warn!("      The system will automatically clean these up when");
                    warn!("      networks are restarted through libvirt APIs.");
                    warn!("      Or manually restart default network:");
                    warn!("      virsh net-destroy default && virsh net-start default");
                }
                
                warn!("   Proceeding with build (graceful degradation)...");
                Ok(()) // DON'T BLOCK on warnings
            }

            HealthState::Unhealthy(reason) => {
                // CRITICAL: System cannot function, attempt recovery
                warn!("❌ Infrastructure unhealthy: {}", reason);

                let recovery = LibvirtRecovery::new();
                let result = recovery
                    .recover(&health)
                    .await
                    .map_err(|e| crate::Error::Backend(format!("Recovery failed: {}", e)))?;

                if result.success {
                    info!("✅ Infrastructure recovery successful: {}", result.summary());
                    Ok(())
                } else {
                    Err(crate::Error::Backend(format!(
                        "Infrastructure recovery failed: {}. System cannot function.",
                        result.summary()
                    )))
                }
            }
        }
    }

    /// Discover the primary network interface name for a VM at runtime
    ///
    /// This queries the VM's actual interface name instead of assuming `enp1s0`.
    /// Uses cloud-init status to query the VM's interface configuration via SSH.
    ///
    /// This is a **deep debt solution** that eliminates hardcoded interface names,
    /// aligning with the "code only has self knowledge and discovers other systems
    /// in runtime" principle.
    ///
    /// # Arguments
    /// * `vm_name` - Name of the VM to query
    /// * `username` - SSH username for accessing the VM
    /// * `vm_ip` - IP address of the VM for SSH access
    ///
    /// # Returns
    /// The primary network interface name (e.g., "enp1s0", "ens3", "eth0")
    ///
    /// # Fallback
    /// If discovery fails, falls back to the configured default interface.
    ///
    /// # Example
    /// ```no_run
    /// use benchscale::LibvirtBackend;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let backend = LibvirtBackend::new()?;
    /// let interface = backend.discover_vm_interface(
    ///     "my-vm",
    ///     "ubuntu",
    ///     "192.168.122.10"
    /// ).await?;
    /// println!("VM interface: {}", interface);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn discover_vm_interface(
        &self,
        vm_name: &str,
        username: &str,
        vm_ip: &str,
    ) -> Result<String> {
        info!("Discovering network interface for VM '{}'", vm_name);

        let port = self.bench_config.network.ssh_port;
        let ssh_result = match crate::backend::ssh::SshClient::connect_with_key(
            vm_ip, port, username, None,
        )
        .await
        {
            Ok(c) => Ok(c),
            Err(_) => {
                crate::backend::ssh::SshClient::connect(vm_ip, port, username, "").await
            }
        };

        if let Ok(mut client) = ssh_result {
            match client
                .exec_stdout("ip -o -4 route show default | awk '{print $5}'")
                .await
            {
                Ok(interface) => {
                    let interface = interface.trim().to_string();
                    if !interface.is_empty() && interface.len() < 20 {
                        info!(
                            "Discovered interface '{}' for VM '{}'",
                            interface, vm_name
                        );
                        let _ = client.disconnect().await;
                        return Ok(interface);
                    }
                    warn!(
                        "Interface discovery returned invalid result: '{}'",
                        interface
                    );
                }
                Err(e) => {
                    warn!("Interface discovery command failed: {}", e);
                }
            }
            let _ = client.disconnect().await;
        } else {
            warn!(
                "SSH connection for interface discovery failed for VM '{}'",
                vm_name
            );
        }

        let fallback = &self.capabilities.network.default_interface;
        warn!(
            "Could not discover interface for VM '{}', using fallback: {}",
            vm_name, fallback
        );
        Ok(fallback.clone())
    }

    /// Discover actual IP address assigned to VM by libvirt DHCP
    ///
    /// This is the **runtime discovery** approach: instead of assuming the VM
    /// uses the IP we allocated from our pool, we query libvirt for the actual
    /// IP that DHCP assigned. This is modern, idiomatic, and capability-based.
    ///
    /// Uses multiple methods for robust discovery:
    /// 1. Domain interface addresses from the DHCP lease source (primary)
    /// 2. Default network DHCP leases (fallback)
    ///
    /// # Arguments
    /// * `vm_name` - Name of the VM to query
    /// * `timeout` - Maximum time to wait for IP assignment
    ///
    /// # Returns
    /// The actual IPv4 address assigned by libvirt DHCP
    ///
    /// # Example
    /// ```no_run
    /// use benchscale::backend::LibvirtBackend;
    /// use std::time::Duration;
    ///
    /// # async fn example() -> Result<(), benchscale::Error> {
    /// let backend = LibvirtBackend::new()?;
    /// let ip = backend.discover_vm_ip("my-vm", Duration::from_secs(60)).await?;
    /// println!("VM actual IP: {}", ip);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn discover_vm_ip(
        &self,
        vm_name: &str,
        timeout: std::time::Duration,
    ) -> crate::Result<String> {
        use std::time::Instant;
        use tokio::time::sleep;
        use tracing::{debug, info, warn};

        let start = Instant::now();
        let mut attempt = 0;

        info!(
            "🔍 Discovering IP for VM '{}' (timeout: {:?})",
            vm_name, timeout
        );

        loop {
            attempt += 1;
            let elapsed = start.elapsed().as_secs();
            debug!("Attempt {} at {}s...", attempt, elapsed);

            let ip = {
                let conn = self.conn.lock().await;
                let from_dom = || -> Option<String> {
                    let domain = Domain::lookup_by_name(&*conn, vm_name).ok()?;
                    let ifaces = domain
                        .interface_addresses(
                            virt::sys::VIR_DOMAIN_INTERFACE_ADDRESSES_SRC_LEASE,
                            0,
                        )
                        .ok()?;
                    Self::first_ipv4_from_lease_interfaces(&ifaces)
                };
                if let Some(ip) = from_dom() {
                    Some(ip)
                } else {
                    dhcp_discovery::query_dhcp_leases_with_connect(&*conn, "default")
                        .ok()
                        .and_then(|leases| Self::ip_from_leases_matching_vm(&leases, vm_name))
                }
            };

            if let Some(ip) = ip {
                info!(
                    "✅ Discovered IP {} for VM '{}' after {} attempts ({:.1}s)",
                    ip,
                    vm_name,
                    attempt,
                    start.elapsed().as_secs_f32()
                );
                return Ok(ip);
            }

            // Check timeout
            if start.elapsed() > timeout {
                warn!(
                    "❌ Timeout after {} attempts ({:.1}s)",
                    attempt,
                    start.elapsed().as_secs_f32()
                );
                return Err(crate::Error::Backend(format!(
                    "Timeout waiting for VM '{}' IP assignment after {:?} ({} attempts)",
                    vm_name, timeout, attempt
                )));
            }

            // Faster polling: 2s fixed interval (not exponential)
            sleep(std::time::Duration::from_secs(2)).await;
        }
    }

    fn first_ipv4_from_lease_interfaces(interfaces: &[virt::domain::Interface]) -> Option<String> {
        for iface in interfaces {
            for addr in &iface.addrs {
                if addr.typed == virt::sys::VIR_IP_ADDR_TYPE_IPV4 as i64 {
                    return Some(addr.addr.clone());
                }
            }
        }
        None
    }

    fn ip_from_leases_matching_vm(
        leases: &[dhcp_discovery::DhcpLease],
        vm_name: &str,
    ) -> Option<String> {
        for lease in leases {
            if lease.hostname.contains(vm_name) {
                return Some(lease.ip_address.clone());
            }
        }
        None
    }

    /// Create a new LibvirtBackend with default configuration
    ///
    /// Connects to libvirt using the default URI (qemu:///system) and
    /// initializes with default capabilities (standard libvirt setup).
    ///
    /// **Evolution #20:** Includes automatic health check and recovery.
    /// This ensures the libvirt infrastructure is stable before use,
    /// preventing failures from orphaned processes or network corruption.
    ///
    /// For runtime capability discovery, use `new_with_discovery().await`.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use benchscale::LibvirtBackend;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let backend = LibvirtBackend::new()?;
    /// // Infrastructure is guaranteed healthy
    /// # Ok(())
    /// # }
    /// ```
    pub fn new() -> Result<Self> {
        #[allow(deprecated)]
        let default_legacy = crate::config_legacy::LibvirtConfig::default();
        let mut backend = Self::with_config(default_legacy)?;

        // Auto-discover templates when intermediate (template) directory is configured
        if backend.bench_config.storage.intermediate_dir.is_some() {
            match backend.discover_templates() {
                Ok(count) => info!("Auto-discovered {} templates", count),
                Err(e) => info!("Template discovery failed (non-fatal): {}", e),
            }
        }

        Ok(backend)
    }

    /// Create a new LibvirtBackend with runtime capability discovery
    ///
    /// This async version discovers system capabilities at runtime,
    /// making the backend portable across different libvirt configurations.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use benchscale::LibvirtBackend;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let backend = LibvirtBackend::new_with_discovery().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn new_with_discovery() -> Result<Self> {
        #[allow(deprecated)]
        let default_legacy = crate::config_legacy::LibvirtConfig::default();
        let mut backend = Self::with_config_and_discovery(default_legacy).await?;

        if backend.bench_config.storage.intermediate_dir.is_some() {
            match backend.discover_templates() {
                Ok(count) => info!("Auto-discovered {} templates", count),
                Err(e) => info!("Template discovery failed (non-fatal): {}", e),
            }
        }

        Ok(backend)
    }

    /// Create a new LibvirtBackend with custom configuration
    ///
    /// Uses default capabilities (standard libvirt setup).
    /// For runtime discovery, use `with_config_and_discovery().await`.
    ///
    /// # Arguments
    /// * `config` - Custom libvirt configuration
    ///
    /// # Example
    ///
    /// ```no_run
    /// use benchscale::{LibvirtBackend, config_legacy::LibvirtConfig};
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let config = LibvirtConfig::default();
    /// let backend = LibvirtBackend::with_config(config)?;
    /// # Ok(())
    /// # }
    /// ```
    #[allow(deprecated)]
    pub fn with_config(config: crate::config_legacy::LibvirtConfig) -> Result<Self> {
        let bench_config = Self::bench_scale_config_from_libvirt_legacy(&config);

        let conn = Connect::open(Some(&super::libvirt_uri()))
            .map_err(|e| crate::Error::Backend(format!("Failed to connect to libvirt: {}", e)))?;

        let capabilities_network = crate::capabilities::NetworkCapabilities::default_libvirt();
        let full_capabilities = crate::capabilities::SystemCapabilities {
            network: capabilities_network,
            storage: crate::capabilities::StorageCapabilities {
                images_dir: crate::constants::paths::default_system_vm_images_dir(),
                temp_dir: std::env::temp_dir(),
                cloud_init_dir: std::env::temp_dir().join("benchscale-cloud-init"),
            },
            virtualization: crate::capabilities::VirtCapabilities {
                uri: super::libvirt_uri(),
                default_os_variant: "ubuntu22.04".to_string(),
                ssh_port: bench_config.network.ssh_port,
                vnc_base_port: 5900,
            },
        };

        Self::new_connected(conn, bench_config, full_capabilities)
    }

    /// Create a new LibvirtBackend with custom configuration and runtime discovery
    ///
    /// Discovers system capabilities at runtime for portable configuration.
    ///
    /// # Arguments
    /// * `config` - Custom libvirt configuration
    ///
    /// # Example
    ///
    /// ```no_run
    /// use benchscale::{LibvirtBackend, config_legacy::LibvirtConfig};
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let config = LibvirtConfig::default();
    /// let backend = LibvirtBackend::with_config_and_discovery(config).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[allow(deprecated)]
    pub async fn with_config_and_discovery(config: crate::config_legacy::LibvirtConfig) -> Result<Self> {
        let mut bench_config = Self::bench_scale_config_from_libvirt_legacy(&config);

        let conn = Connect::open(Some(&super::libvirt_uri()))
            .map_err(|e| crate::Error::Backend(format!("Failed to connect to libvirt: {}", e)))?;

        info!("Discovering system capabilities for portable configuration...");
        let capabilities = crate::capabilities::SystemCapabilities::discover().await?;
        bench_config.merge_with_capabilities(&capabilities);

        Self::new_connected(conn, bench_config, capabilities)
    }
}

// Test module (only compiled with libvirt feature)
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
