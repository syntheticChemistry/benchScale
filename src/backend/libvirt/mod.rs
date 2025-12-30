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
//! - **Static IP allocation** to eliminate DHCP race conditions
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
//!     .password("ubuntu", "password123")
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
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

#[cfg(feature = "libvirt")]
use virt::connect::Connect;

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
/// let backend = LibvirtBackend::new().await?;
/// // Backend is ready to create VMs
/// # Ok(())
/// # }
/// ```
#[cfg(feature = "libvirt")]
pub struct LibvirtBackend {
    /// Libvirt connection (wrapped in Arc<Mutex> for async safety)
    pub(crate) conn: Arc<Mutex<Connect>>,

    /// Configuration for the backend
    pub(crate) config: crate::config::LibvirtConfig,

    /// Runtime-discovered system capabilities
    pub(crate) capabilities: crate::capabilities::SystemCapabilities,

    /// IP pool for deterministic IP allocation
    pub(crate) ip_pool: crate::backend::IpPool,

    /// Registered templates (name -> path mapping)
    pub(crate) templates: HashMap<String, PathBuf>,
}

#[cfg(feature = "libvirt")]
impl LibvirtBackend {
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

    /// Create a new LibvirtBackend with default configuration
    ///
    /// Connects to libvirt using the default URI (qemu:///system) and
    /// initializes with default capabilities (standard libvirt setup).
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
    /// # Ok(())
    /// # }
    /// ```
    pub fn new() -> Result<Self> {
        let mut backend = Self::with_config(crate::config::LibvirtConfig::default())?;

        // Auto-discover templates if template_dir is configured
        if backend.config.template_dir.is_some() {
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
        let mut backend =
            Self::with_config_and_discovery(crate::config::LibvirtConfig::default()).await?;

        // Auto-discover templates if template_dir is configured
        if backend.config.template_dir.is_some() {
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
    /// use benchscale::{LibvirtBackend, config::LibvirtConfig};
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let config = LibvirtConfig::default();
    /// let backend = LibvirtBackend::with_config(config)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_config(config: crate::config::LibvirtConfig) -> Result<Self> {
        let conn = Connect::open(Some(&config.uri))
            .map_err(|e| crate::Error::Backend(format!("Failed to connect to libvirt: {}", e)))?;

        // Use default capabilities (standard libvirt)
        let capabilities = crate::capabilities::NetworkCapabilities::default_libvirt();
        let full_capabilities = crate::capabilities::SystemCapabilities {
            network: capabilities.clone(),
            storage: crate::capabilities::StorageCapabilities {
                images_dir: std::path::PathBuf::from("/var/lib/libvirt/images"),
                temp_dir: std::env::temp_dir(),
                cloud_init_dir: std::env::temp_dir().join("benchscale-cloud-init"),
            },
            virtualization: crate::capabilities::VirtCapabilities {
                uri: config.uri.clone(),
                default_os_variant: "ubuntu22.04".to_string(),
                ssh_port: 22,
                vnc_base_port: 5900,
            },
        };

        // Initialize IP pool from capabilities
        let ip_pool = crate::backend::IpPool::from_range(
            &full_capabilities.network.ip_pool_start,
            &full_capabilities.network.ip_pool_end,
        )?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            config,
            capabilities: full_capabilities,
            ip_pool,
            templates: HashMap::new(),
        })
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
    /// use benchscale::{LibvirtBackend, config::LibvirtConfig};
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let config = LibvirtConfig::default();
    /// let backend = LibvirtBackend::with_config_and_discovery(config).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn with_config_and_discovery(config: crate::config::LibvirtConfig) -> Result<Self> {
        let conn = Connect::open(Some(&config.uri))
            .map_err(|e| crate::Error::Backend(format!("Failed to connect to libvirt: {}", e)))?;

        // Discover system capabilities at runtime
        info!("Discovering system capabilities for portable configuration...");
        let capabilities = crate::capabilities::SystemCapabilities::discover().await?;

        // Initialize IP pool from discovered network configuration
        let ip_pool = crate::backend::IpPool::from_range(
            &capabilities.network.ip_pool_start,
            &capabilities.network.ip_pool_end,
        )?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            config,
            capabilities,
            ip_pool,
            templates: HashMap::new(),
        })
    }
}

#[cfg(feature = "libvirt")]
impl Default for LibvirtBackend {
    fn default() -> Self {
        Self::new().expect("Failed to create LibvirtBackend")
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
