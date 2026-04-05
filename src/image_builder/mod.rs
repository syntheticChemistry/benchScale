// SPDX-License-Identifier: AGPL-3.0-or-later
//! Image Builder for benchScale
//!
//! Provides high-level API for building VM templates with proper monitoring,
//! user interaction, and state management.
//!
//! # Example
//!
//! ```no_run
//! use benchscale::image_builder::{ImageBuilder, BuildStep};
//! use std::path::Path;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let builder = ImageBuilder::new_libvirt("popos-cosmic")?;
//!
//! // Build with user interaction
//! let template = builder
//!     .from_cloud_image(Path::new("ubuntu-24.04.img"))
//!     .with_memory(4096)
//!     .with_vcpus(2)
//!     .add_step(BuildStep::InstallPackages(vec!["cosmic-desktop".to_string()]))
//!     .add_step(BuildStep::UserVerification {
//!         message: "Check VNC - is COSMIC running?".to_string(),
//!         vnc_port: None, // Auto-detect
//!     })
//!     .build()
//!     .await?;
//!
//! println!("Template saved to: {}", template.template_path.display());
//! # Ok(())
//! # }
//! ```

mod pipeline;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

use crate::backend::{Backend, NodeInfo, NodeStatus};
use crate::{CloudInit, Error, Result};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;

#[cfg(feature = "libvirt")]
use crate::backend::LibvirtBackend;

/// Build step for template creation
#[derive(Debug, Clone)]
pub enum BuildStep {
    /// Install packages via apt
    InstallPackages(Vec<String>),

    /// Run arbitrary shell commands
    RunCommands(Vec<String>),

    /// Wait for cloud-init to complete (handles apt locks)
    WaitForCloudInit,

    /// Pause for user verification (GUI check, etc.)
    UserVerification {
        /// Message to display during verification
        message: String,
        /// Optional VNC port for remote access
        vnc_port: Option<u16>,
    },

    /// Save intermediate state
    SaveIntermediate {
        /// Name for the intermediate snapshot
        name: String,
        /// Path where the snapshot will be saved
        path: PathBuf,
    },

    /// Reboot VM
    Reboot,
}

/// Image builder for creating VM templates
///
/// The builder accepts any `Backend` implementation, making it
/// vendor-agnostic and testable without external dependencies.
///
/// # Service Discovery
///
/// **Note**: benchScale does NOT provide custom service discovery.
///
/// For runtime backend selection, use standard solutions:
/// - **mDNS/DNS-SD**: Local network discovery (Avahi, Bonjour)
/// - **Consul**: Distributed service registry  
/// - **Environment variables**: Explicit configuration (recommended)
///
/// # Example with Explicit Backend
///
/// ```no_run
/// use benchscale::image_builder::ImageBuilder;
/// use benchscale::LibvirtBackend;
/// use std::sync::Arc;
///
/// # fn example() -> anyhow::Result<()> {
/// let backend = Arc::new(LibvirtBackend::new()?);
/// let builder = ImageBuilder::new("my-image", backend)?;
/// # Ok(())
/// # }
/// ```
pub struct ImageBuilder {
    pub(crate) name: String,
    pub(crate) base_image: Option<PathBuf>,
    pub(crate) memory_mb: u32,
    pub(crate) vcpus: u32,
    pub(crate) disk_size_gb: u32,
    pub(crate) steps: Vec<BuildStep>,
    pub(crate) cloud_init: Option<CloudInit>,
    pub(crate) backend: Arc<dyn Backend>,
}

/// Build result containing template path and metadata
#[derive(Debug)]
pub struct BuildResult {
    /// Path to the created template image
    pub template_path: PathBuf,
    /// Name of the VM that was built
    pub vm_name: String,
    /// Final size of the template in bytes
    pub final_size_bytes: u64,
}

impl ImageBuilder {
    /// Create a new image builder with a specific backend
    ///
    /// This constructor accepts any `Backend` implementation, making the builder
    /// vendor-agnostic. Use this when you want explicit control over which backend to use.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use benchscale::image_builder::ImageBuilder;
    /// use benchscale::LibvirtBackend;
    /// use std::sync::Arc;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let backend = Arc::new(LibvirtBackend::new()?);
    /// let builder = ImageBuilder::new("ubuntu-desktop", backend)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(name: impl Into<String>, backend: Arc<dyn Backend>) -> Result<Self> {
        Ok(Self {
            name: name.into(),
            base_image: None,
            memory_mb: 4096,
            vcpus: 2,
            disk_size_gb: 35,
            steps: Vec::new(),
            cloud_init: None,
            backend,
        })
    }

    /// Create a new image builder with libvirt backend (convenience method)
    #[cfg(feature = "libvirt")]
    pub fn new_libvirt(name: impl Into<String>) -> Result<Self> {
        let backend = Arc::new(LibvirtBackend::new()?) as Arc<dyn Backend>;
        Self::new(name, backend)
    }

    /// Set base cloud image
    pub fn from_cloud_image(mut self, path: impl Into<PathBuf>) -> Self {
        self.base_image = Some(path.into());
        self
    }

    /// Set memory in MB
    pub fn with_memory(mut self, memory_mb: u32) -> Self {
        self.memory_mb = memory_mb;
        self
    }

    /// Set vCPUs
    pub fn with_vcpus(mut self, vcpus: u32) -> Self {
        self.vcpus = vcpus;
        self
    }

    /// Set disk size in GB
    pub fn with_disk_size(mut self, disk_size_gb: u32) -> Self {
        self.disk_size_gb = disk_size_gb;
        self
    }

    /// Set cloud-init configuration
    pub fn with_cloud_init(mut self, cloud_init: CloudInit) -> Self {
        self.cloud_init = Some(cloud_init);
        self
    }

    /// Add a build step
    pub fn add_step(mut self, step: BuildStep) -> Self {
        self.steps.push(step);
        self
    }

    /// Create ImageBuilder from existing VM
    ///
    /// # Deprecated
    ///
    /// Use `new()` with explicit backend instead.
    #[cfg(feature = "libvirt")]
    pub fn from_existing_vm(vm_name: impl Into<String>) -> Result<Self> {
        let vm_name_str = vm_name.into();
        let backend = Arc::new(LibvirtBackend::new()?) as Arc<dyn Backend>;

        Ok(Self {
            name: vm_name_str,
            base_image: None,
            memory_mb: 4096,
            vcpus: 2,
            disk_size_gb: 35,
            steps: Vec::new(),
            cloud_init: None,
            backend,
        })
    }

    /// Build from existing VM (simplified workflow based on pipeline lessons)
    pub async fn build_from_existing(self, vm_name: &str) -> Result<BuildResult> {
        info!("Building from existing VM: {}", vm_name);

        info!("Step 1/4: Getting actual VM IP...");
        let ip = pipeline::get_actual_vm_ip(vm_name).await?;

        info!("Step 2/4: Detecting SSH user...");
        let user = pipeline::detect_ssh_user(&ip).await?;

        info!("Step 3/4: Waiting for SSH to be ready...");
        pipeline::wait_for_ssh(&ip, &user, 10).await?;

        info!("Step 4/4: Executing {} build steps...", self.steps.len());

        let node = NodeInfo {
            id: vm_name.to_string(),
            name: vm_name.to_string(),
            container_id: vm_name.to_string(),
            ip_address: ip.clone(),
            network: "default".to_string(),
            status: NodeStatus::Running,
            metadata: std::collections::HashMap::new(),
        };

        for (idx, step) in self.steps.iter().enumerate() {
            info!(
                "  Executing step {}/{}: {:?}",
                idx + 1,
                self.steps.len(),
                step
            );
            self.execute_step_with_user(&node, &user, step).await?;
        }

        info!("Saving VM as template...");
        let template_path = self.save_as_template(vm_name).await?;

        let final_size = std::fs::metadata(&template_path)
            .map(|m| m.len())
            .unwrap_or(0);

        info!("Build complete!");

        Ok(BuildResult {
            template_path,
            vm_name: vm_name.to_string(),
            final_size_bytes: final_size,
        })
    }

    /// Build the template
    pub async fn build(mut self) -> Result<BuildResult> {
        let base_image = self
            .base_image
            .take()
            .ok_or_else(|| Error::Backend("No base image specified".to_string()))?;

        if !base_image.exists() {
            return Err(Error::Backend(format!(
                "Base image not found: {}",
                base_image.display()
            )));
        }

        let vm_name = format!(
            "{}-builder-{}",
            self.name,
            chrono::Utc::now().format("%Y%m%d-%H%M%S")
        );

        info!("Starting image build: {}", vm_name);
        info!("  Base image: {}", base_image.display());
        info!("  Memory: {}MB, vCPUs: {}", self.memory_mb, self.vcpus);
        info!("  Build steps: {}", self.steps.len());

        let node = self.create_builder_vm(&vm_name, &base_image).await?;

        info!("Builder VM created: {} at {}", node.name, node.ip_address);

        let vnc_display =
            Self::get_vnc_display(&vm_name).unwrap_or_else(|_| "(unknown)".to_string());
        info!("  VNC: {}", vnc_display);

        for (idx, step) in self.steps.iter().enumerate() {
            info!(
                "Executing step {}/{}: {:?}",
                idx + 1,
                self.steps.len(),
                step
            );
            self.execute_step(&node, step).await?;
        }

        let template_path = self.save_as_template(&vm_name).await?;

        info!("Cleaning up builder VM...");
        self.backend.delete_node(&vm_name).await?;

        let final_size = std::fs::metadata(&template_path)
            .map(|m| m.len())
            .unwrap_or(0);

        Ok(BuildResult {
            template_path,
            vm_name,
            final_size_bytes: final_size,
        })
    }
}
